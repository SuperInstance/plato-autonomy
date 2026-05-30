# DEPENDENCIES — plato-autonomy

## Signal Chain Layer

**L0-L4 (Metrics)** — Autonomy metrics and reporting.

Measures and reports autonomy metrics for PLATO rooms. Tracks how much each room can handle locally versus escalating to higher layers.

## Ecosystem Dependencies

| Repo | Relationship | Description |
|------|-------------|-------------|
| [plato-state](https://github.com/SuperInstance/plato-state) | **Depends on** | Room state vectors provide the raw data for autonomy calculations |
| [plato-dashboard](https://github.com/SuperInstance/plato-dashboard) | **Depended on by** | Dashboard renders autonomy metrics for fleet monitoring |
| [plato-nervous](https://github.com/SuperInstance/plato-nervous) | **Related** | Signal chain layer resolution data informs autonomy scores |
| [plato-coordination](https://github.com/SuperInstance/plato-coordination) | **Related** | Fleet coordination decisions affect autonomy measurements |

## Data Flow

```
IN:
  - Room state vectors (from plato-state)
  - Signal chain resolution history
  - Escalation counts per layer

OUT:
  - Autonomy score per room (0.0-1.0)
  - Layer utilization breakdown
  - Escalation rate trends
  - Fleet autonomy summary
```

## Dependency Graph Position

```
plato-tiles → plato-rooms → plato-state
                              ↓
                    plato-autonomy ← (this crate)
```
