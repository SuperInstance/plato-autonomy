# plato-autonomy

Autonomy metrics and reporting for PLATO rooms.

## What It Does

Measures how much each room can handle locally without escalating to higher signal chain layers. The goal: rooms should handle 99% of situations locally, with <1% reaching the cloud.

## Ecosystem

- **[plato-state](https://github.com/SuperInstance/plato-state)** ← Depends on (room state vectors)
- **[plato-dashboard](https://github.com/SuperInstance/plato-dashboard)** → Renders autonomy metrics
- **[plato-nervous](https://github.com/SuperInstance/plato-nervous)** — Signal chain resolution feeds autonomy scoring
- **[plato-coordination](https://github.com/SuperInstance/plato-coordination)** — Coordination decisions affect measurements

See [DEPENDENCIES.md](./DEPENDENCIES.md) for the full dependency map.
