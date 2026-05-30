use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Processing layer in the PLATO stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Layer {
    /// L0: Deadband — zero latency, fully autonomous.
    Deadband,
    /// L1: Nano model — <100ms, autonomous.
    Nano,
    /// L2: LoRA model — <500ms, autonomous.
    LoRA,
    /// L3: Fleet — <2s, partially autonomous (50% credit).
    Fleet,
    /// L4: Cloud — >2s, not autonomous.
    Cloud,
}

impl Layer {
    /// Autonomy credit for this layer (0.0–1.0).
    pub fn autonomy_credit(&self) -> f64 {
        match self {
            Layer::Deadband | Layer::Nano | Layer::LoRA => 1.0,
            Layer::Fleet => 0.5,
            Layer::Cloud => 0.0,
        }
    }

    /// Expected max latency for this layer in milliseconds.
    pub fn max_latency_ms(&self) -> u64 {
        match self {
            Layer::Deadband => 0,
            Layer::Nano => 100,
            Layer::LoRA => 500,
            Layer::Fleet => 2000,
            Layer::Cloud => u64::MAX,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Layer::Deadband => "L0-Deadband",
            Layer::Nano => "L1-Nano",
            Layer::LoRA => "L2-LoRA",
            Layer::Fleet => "L3-Fleet",
            Layer::Cloud => "L4-Cloud",
        }
    }
}

/// Per-layer statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerStats {
    pub resolved_count: u64,
    pub escalated_count: u64,
    pub total_latency_ms: u64,
    pub total_confidence: f64,
}

impl Default for LayerStats {
    fn default() -> Self {
        Self {
            resolved_count: 0,
            escalated_count: 0,
            total_latency_ms: 0,
            total_confidence: 0.0,
        }
    }
}

impl LayerStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, latency_ms: u64, confidence: f64, escalated: bool) {
        self.resolved_count += 1;
        self.total_latency_ms += latency_ms;
        self.total_confidence += confidence;
        if escalated {
            self.escalated_count += 1;
        }
    }

    pub fn avg_latency_ms(&self) -> f64 {
        if self.resolved_count == 0 {
            0.0
        } else {
            self.total_latency_ms as f64 / self.resolved_count as f64
        }
    }

    pub fn avg_confidence(&self) -> f64 {
        if self.resolved_count == 0 {
            0.0
        } else {
            self.total_confidence / self.resolved_count as f64
        }
    }
}

/// Trend direction for autonomy over time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Trend {
    Improving,
    Stable,
    Declining,
    Critical,
}

/// A single reading record for trend analysis.
#[derive(Debug, Clone)]
struct Reading {
    autonomous: bool,
    timestamp_secs: u64,
}

/// Unique room identifier.
pub type RoomId = String;

/// Per-room autonomy tracker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomAutonomy {
    pub room_id: RoomId,
    pub layers: HashMap<Layer, LayerStats>,
    #[serde(skip)]
    readings: Vec<Reading>,
    total_readings: u64,
    autonomous_readings: u64,
}

impl RoomAutonomy {
    pub fn new(room_id: impl Into<RoomId>) -> Self {
        Self {
            room_id: room_id.into(),
            layers: HashMap::new(),
            readings: Vec::new(),
            total_readings: 0,
            autonomous_readings: 0,
        }
    }

    /// Record a resolution event at a given layer.
    pub fn record_resolution(&mut self, layer: Layer, latency_ms: u64, confidence: f64) {
        let credit = layer.autonomy_credit();
        let escalated = credit < 1.0;
        let autonomous = credit >= 1.0;

        self.layers
            .entry(layer)
            .or_insert_with(LayerStats::new)
            .record(latency_ms, confidence, escalated);

        self.total_readings += 1;
        if autonomous {
            self.autonomous_readings += 1;
        }

        // Keep bounded history for trend analysis (last 200 readings)
        if self.readings.len() >= 200 {
            self.readings.remove(0);
        }
        self.readings.push(Reading {
            autonomous,
            timestamp_secs: 0, // simplified — real impl would use clock
        });
    }

    /// Autonomy percentage: % resolved locally (L0–L2 fully, L3 at 50%).
    pub fn autonomy_percentage(&self) -> f64 {
        if self.total_readings == 0 {
            // No data → undefined; return 100% as "no cloud calls needed yet"
            100.0
        } else {
            // Weighted: L0-L2 count fully, L3 counts 0.5, L4 counts 0
            let mut weighted_autonomous = 0.0_f64;
            let mut total = 0.0_f64;
            for (layer, stats) in &self.layers {
                let credit = layer.autonomy_credit();
                weighted_autonomous += stats.resolved_count as f64 * credit;
                total += stats.resolved_count as f64;
            }
            if total == 0.0 {
                100.0
            } else {
                (weighted_autonomous / total) * 100.0
            }
        }
    }

    /// Determine trend by comparing last 100 vs previous 100 readings.
    pub fn trend(&self) -> Trend {
        let n = self.readings.len();
        if n < 20 {
            return Trend::Stable;
        }

        let half = n / 2;
        let older = &self.readings[..half];
        let newer = &self.readings[half..];

        let older_pct = autonomy_pct_of_slice(older);
        let newer_pct = autonomy_pct_of_slice(newer);
        let delta = newer_pct - older_pct;

        if newer_pct < 50.0 || delta < -10.0 {
            Trend::Critical
        } else if delta > 2.0 {
            Trend::Improving
        } else if delta < -2.0 {
            Trend::Declining
        } else {
            Trend::Stable
        }
    }

    /// Predict how many cloud calls in the next `hours` hours, based on current rate.
    pub fn predicted_cloud_calls(&self, hours: u32) -> u32 {
        if self.total_readings == 0 {
            return 0;
        }

        // Count cloud (L4) readings
        let cloud_count = self
            .layers
            .get(&Layer::Cloud)
            .map(|s| s.resolved_count)
            .unwrap_or(0) as f64;

        // Assume the existing readings represent 1 hour of data for simplicity
        let cloud_rate_per_hour = cloud_count; // readings/hour
        (cloud_rate_per_hour * hours as f64).ceil() as u32
    }
}

fn autonomy_pct_of_slice(readings: &[Reading]) -> f64 {
    if readings.is_empty() {
        return 100.0;
    }
    let autonomous = readings.iter().filter(|r| r.autonomous).count() as f64;
    (autonomous / readings.len() as f64) * 100.0
}

/// Configuration for autonomy thresholds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyConfig {
    /// Target autonomy percentage.
    pub target_autonomy_pct: f64,
    /// Alert if autonomy drops below this.
    pub degradation_threshold_pct: f64,
    /// Hours to look ahead for prediction.
    pub prediction_window_hours: u32,
}

impl Default for AutonomyConfig {
    fn default() -> Self {
        Self {
            target_autonomy_pct: 90.0,
            degradation_threshold_pct: 75.0,
            prediction_window_hours: 24,
        }
    }
}

impl AutonomyConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.target_autonomy_pct < 0.0 || self.target_autonomy_pct > 100.0 {
            return Err("target_autonomy_pct must be 0–100".into());
        }
        if self.degradation_threshold_pct < 0.0 || self.degradation_threshold_pct > 100.0 {
            return Err("degradation_threshold_pct must be 0–100".into());
        }
        if self.degradation_threshold_pct >= self.target_autonomy_pct {
            return Err("degradation_threshold must be below target".into());
        }
        if self.prediction_window_hours == 0 {
            return Err("prediction_window_hours must be > 0".into());
        }
        Ok(())
    }
}

/// A snapshot report of fleet autonomy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyReport {
    pub overall_autonomy_pct: f64,
    pub room_summaries: Vec<RoomSummary>,
    pub degraded_room_ids: Vec<RoomId>,
    pub recommendations: Vec<String>,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomSummary {
    pub room_id: RoomId,
    pub autonomy_pct: f64,
    pub trend: Trend,
    pub predicted_cloud_calls: u32,
}

impl AutonomyReport {
    /// Return room IDs that are below the degradation threshold.
    pub fn degraded_rooms(&self) -> Vec<RoomId> {
        self.degraded_room_ids.clone()
    }

    /// Generate actionable recommendations.
    pub fn recommendations(&self) -> Vec<String> {
        self.recommendations.clone()
    }
}

/// Fleet-wide autonomy aggregator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetAutonomy {
    pub config: AutonomyConfig,
    rooms: HashMap<RoomId, RoomAutonomy>,
}

impl FleetAutonomy {
    pub fn new(config: AutonomyConfig) -> Self {
        Self {
            config,
            rooms: HashMap::new(),
        }
    }

    pub fn add_room(&mut self, room_id: impl Into<RoomId>) {
        let id = room_id.into();
        self.rooms.entry(id.clone()).or_insert_with(|| RoomAutonomy::new(id));
    }

    pub fn update_room(&mut self, room_id: &str, layer: Layer, latency_ms: u64, confidence: f64) {
        if let Some(room) = self.rooms.get_mut(room_id) {
            room.record_resolution(layer, latency_ms, confidence);
        }
    }

    pub fn room(&self, room_id: &str) -> Option<&RoomAutonomy> {
        self.rooms.get(room_id)
    }

    pub fn rooms(&self) -> &HashMap<RoomId, RoomAutonomy> {
        &self.rooms
    }

    /// Generate a full autonomy report.
    pub fn generate_report(&self) -> AutonomyReport {
        let mut total_autonomous = 0.0_f64;
        let mut total_all = 0.0_f64;
        let mut room_summaries = Vec::new();
        let mut degraded = Vec::new();
        let mut recommendations = Vec::new();

        for (id, room) in &self.rooms {
            let pct = room.autonomy_percentage();
            let trend = room.trend();
            let predicted = room.predicted_cloud_calls(self.config.prediction_window_hours);

            // Weighted contribution to fleet total
            let room_total: f64 = room.layers.values().map(|s| s.resolved_count as f64).sum();
            let room_auto: f64 = room
                .layers
                .iter()
                .map(|(l, s)| s.resolved_count as f64 * l.autonomy_credit())
                .sum();
            total_autonomous += room_auto;
            total_all += room_total;

            room_summaries.push(RoomSummary {
                room_id: id.clone(),
                autonomy_pct: pct,
                trend,
                predicted_cloud_calls: predicted,
            });

            if pct < self.config.degradation_threshold_pct {
                degraded.push(id.clone());
            }
        }

        let overall_pct = if total_all == 0.0 {
            100.0
        } else {
            (total_autonomous / total_all) * 100.0
        };

        // Generate recommendations
        if overall_pct < self.config.target_autonomy_pct {
            recommendations.push(format!(
                "Fleet autonomy ({:.1}%) is below target ({:.1}%). Review cloud escalation patterns.",
                overall_pct, self.config.target_autonomy_pct
            ));
        }

        for room in &room_summaries {
            match room.trend {
                Trend::Critical => recommendations.push(format!(
                    "Room {} is in CRITICAL decline ({:.1}% autonomy). Immediate attention needed.",
                    room.room_id, room.autonomy_pct
                )),
                Trend::Declining => recommendations.push(format!(
                    "Room {} is declining ({:.1}% autonomy). Monitor closely.",
                    room.room_id, room.autonomy_pct
                )),
                _ => {}
            }
            if room.predicted_cloud_calls > 50 {
                recommendations.push(format!(
                    "Room {} predicted to make {} cloud calls in {}h. Consider tuning local models.",
                    room.room_id, room.predicted_cloud_calls, self.config.prediction_window_hours
                ));
            }
        }

        if recommendations.is_empty() {
            recommendations.push("Fleet autonomy is healthy. No action needed.".into());
        }

        AutonomyReport {
            overall_autonomy_pct: overall_pct,
            room_summaries,
            degraded_room_ids: degraded,
            recommendations,
            generated_at: Uuid::new_v4().to_string(), // placeholder timestamp
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- LayerStats tests ---

    #[test]
    fn test_layer_stats_default() {
        let stats = LayerStats::default();
        assert_eq!(stats.resolved_count, 0);
        assert_eq!(stats.avg_latency_ms(), 0.0);
        assert_eq!(stats.avg_confidence(), 0.0);
    }

    #[test]
    fn test_layer_stats_record() {
        let mut stats = LayerStats::new();
        stats.record(50, 0.9, false);
        stats.record(150, 0.7, true);
        assert_eq!(stats.resolved_count, 2);
        assert_eq!(stats.escalated_count, 1);
        assert!((stats.avg_latency_ms() - 100.0).abs() < 0.01);
        assert!((stats.avg_confidence() - 0.8).abs() < 0.01);
    }

    // --- Layer autonomy credit ---

    #[test]
    fn test_layer_autonomy_credits() {
        assert_eq!(Layer::Deadband.autonomy_credit(), 1.0);
        assert_eq!(Layer::Nano.autonomy_credit(), 1.0);
        assert_eq!(Layer::LoRA.autonomy_credit(), 1.0);
        assert_eq!(Layer::Fleet.autonomy_credit(), 0.5);
        assert_eq!(Layer::Cloud.autonomy_credit(), 0.0);
    }

    // --- Autonomy percentage ---

    #[test]
    fn test_autonomy_perfect() {
        let mut room = RoomAutonomy::new("room-1");
        for _ in 0..10 {
            room.record_resolution(Layer::Deadband, 0, 1.0);
        }
        assert!((room.autonomy_percentage() - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_autonomy_zero() {
        let mut room = RoomAutonomy::new("room-1");
        for _ in 0..10 {
            room.record_resolution(Layer::Cloud, 3000, 0.3);
        }
        assert!((room.autonomy_percentage() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_autonomy_mixed() {
        let mut room = RoomAutonomy::new("room-1");
        // 8 autonomous (L1) + 2 cloud = 8/10 * 100 = 80%
        for _ in 0..8 {
            room.record_resolution(Layer::Nano, 50, 0.95);
        }
        for _ in 0..2 {
            room.record_resolution(Layer::Cloud, 3000, 0.3);
        }
        assert!((room.autonomy_percentage() - 80.0).abs() < 0.01);
    }

    #[test]
    fn test_autonomy_with_fleet_layer() {
        let mut room = RoomAutonomy::new("room-1");
        // 4 L1 (4 * 1.0 = 4.0) + 4 L3 (4 * 0.5 = 2.0) + 2 L4 (0) = 6.0 / 10 = 60%
        for _ in 0..4 {
            room.record_resolution(Layer::Nano, 50, 0.95);
        }
        for _ in 0..4 {
            room.record_resolution(Layer::Fleet, 1500, 0.7);
        }
        for _ in 0..2 {
            room.record_resolution(Layer::Cloud, 3000, 0.3);
        }
        assert!((room.autonomy_percentage() - 60.0).abs() < 0.01);
    }

    #[test]
    fn test_autonomy_new_room() {
        let room = RoomAutonomy::new("new-room");
        // No data → 100% (no cloud calls needed)
        assert!((room.autonomy_percentage() - 100.0).abs() < 0.01);
    }

    // --- Trend detection ---

    #[test]
    fn test_trend_stable_low_data() {
        let room = RoomAutonomy::new("room-1");
        // < 20 readings → Stable
        assert_eq!(room.trend(), Trend::Stable);
    }

    #[test]
    fn test_trend_improving() {
        let mut room = RoomAutonomy::new("room-1");
        // First 50: all cloud, last 50: all nano
        for _ in 0..50 {
            room.record_resolution(Layer::Cloud, 3000, 0.3);
        }
        for _ in 0..50 {
            room.record_resolution(Layer::Nano, 50, 0.95);
        }
        assert_eq!(room.trend(), Trend::Improving);
    }

    #[test]
    fn test_trend_declining() {
        let mut room = RoomAutonomy::new("room-1");
        // First 50: all nano, last 50: all cloud
        for _ in 0..50 {
            room.record_resolution(Layer::Nano, 50, 0.95);
        }
        for _ in 0..50 {
            room.record_resolution(Layer::Cloud, 3000, 0.3);
        }
        assert_eq!(room.trend(), Trend::Critical); // 0% in newer half → critical
    }

    #[test]
    fn test_trend_stable() {
        let mut room = RoomAutonomy::new("room-1");
        // Both halves: 80% autonomous
        for _ in 0..10 {
            room.record_resolution(Layer::Nano, 50, 0.95);
        }
        for _ in 0..2 {
            room.record_resolution(Layer::Cloud, 3000, 0.3);
        }
        for _ in 0..10 {
            room.record_resolution(Layer::Nano, 50, 0.95);
        }
        for _ in 0..2 {
            room.record_resolution(Layer::Cloud, 3000, 0.3);
        }
        // Both halves ~83% → stable
        assert_eq!(room.trend(), Trend::Stable);
    }

    // --- Prediction ---

    #[test]
    fn test_predicted_cloud_calls_no_data() {
        let room = RoomAutonomy::new("room-1");
        assert_eq!(room.predicted_cloud_calls(24), 0);
    }

    #[test]
    fn test_predicted_cloud_calls_with_data() {
        let mut room = RoomAutonomy::new("room-1");
        // 10 cloud calls → rate ~10/hour → predict 24h → 240
        for _ in 0..10 {
            room.record_resolution(Layer::Cloud, 3000, 0.3);
        }
        assert_eq!(room.predicted_cloud_calls(24), 240);
    }

    // --- Config validation ---

    #[test]
    fn test_config_valid() {
        let config = AutonomyConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_invalid_target() {
        let config = AutonomyConfig {
            target_autonomy_pct: 150.0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_degradation_above_target() {
        let config = AutonomyConfig {
            target_autonomy_pct: 80.0,
            degradation_threshold_pct: 90.0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    // --- Fleet tests ---

    #[test]
    fn test_fleet_add_room() {
        let mut fleet = FleetAutonomy::new(AutonomyConfig::default());
        fleet.add_room("room-1");
        fleet.add_room("room-2");
        assert_eq!(fleet.rooms().len(), 2);
    }

    #[test]
    fn test_fleet_report_healthy() {
        let mut fleet = FleetAutonomy::new(AutonomyConfig::default());
        fleet.add_room("room-1");
        for _ in 0..100 {
            fleet.update_room("room-1", Layer::Nano, 50, 0.95);
        }
        let report = fleet.generate_report();
        assert!(report.degraded_rooms().is_empty());
        assert!((report.overall_autonomy_pct - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_fleet_report_degraded() {
        let config = AutonomyConfig {
            degradation_threshold_pct: 75.0,
            ..Default::default()
        };
        let mut fleet = FleetAutonomy::new(config);
        fleet.add_room("room-1");
        // 70% autonomous → below 75% threshold
        for _ in 0..7 {
            fleet.update_room("room-1", Layer::Nano, 50, 0.95);
        }
        for _ in 0..3 {
            fleet.update_room("room-1", Layer::Cloud, 3000, 0.3);
        }
        let report = fleet.generate_report();
        assert!(report.degraded_rooms().contains(&"room-1".to_string()));
    }

    #[test]
    fn test_fleet_recommendations() {
        let config = AutonomyConfig {
            target_autonomy_pct: 95.0,
            degradation_threshold_pct: 75.0,
            prediction_window_hours: 24,
        };
        let mut fleet = FleetAutonomy::new(config);
        fleet.add_room("room-1");
        for _ in 0..50 {
            fleet.update_room("room-1", Layer::Cloud, 3000, 0.3);
        }
        for _ in 0..50 {
            fleet.update_room("room-1", Layer::Cloud, 3000, 0.3);
        }
        let report = fleet.generate_report();
        assert!(!report.recommendations().is_empty());
        // Should have critical recommendation
        assert!(report.recommendations().iter().any(|r| r.contains("CRITICAL")));
    }

    // --- Serialization ---

    #[test]
    fn test_room_autonomy_serialization() {
        let mut room = RoomAutonomy::new("room-1");
        room.record_resolution(Layer::Nano, 50, 0.95);
        room.record_resolution(Layer::Cloud, 3000, 0.3);
        let json = serde_json::to_string(&room).unwrap();
        let back: RoomAutonomy = serde_json::from_str(&json).unwrap();
        assert_eq!(back.room_id, "room-1");
        assert_eq!(back.layers.len(), 2);
    }

    #[test]
    fn test_fleet_autonomy_serialization() {
        let mut fleet = FleetAutonomy::new(AutonomyConfig::default());
        fleet.add_room("room-1");
        fleet.update_room("room-1", Layer::Nano, 50, 0.95);
        let json = serde_json::to_string(&fleet).unwrap();
        let back: FleetAutonomy = serde_json::from_str(&json).unwrap();
        assert_eq!(back.rooms().len(), 1);
    }

    #[test]
    fn test_report_serialization() {
        let fleet = FleetAutonomy::new(AutonomyConfig::default());
        let report = fleet.generate_report();
        let json = serde_json::to_string(&report).unwrap();
        let back: AutonomyReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.room_summaries.len(), report.room_summaries.len());
    }

    #[test]
    fn test_empty_fleet_report() {
        let fleet = FleetAutonomy::new(AutonomyConfig::default());
        let report = fleet.generate_report();
        assert!((report.overall_autonomy_pct - 100.0).abs() < 0.01);
        assert!(report.degraded_rooms().is_empty());
    }
}
