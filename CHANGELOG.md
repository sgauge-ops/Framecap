# Changelog

Changes are grouped under Unreleased until a release is made.
Dates reflect the first available Git record, not necessarily when development
originally took place.

## Unreleased

| Date | Changes |
| --- | --- |
| 2026-11-08 | Added the fan-speed automatic algorithm with a default target of 2000 RPM. |
| 2026-11-08 | Changed automatic mode labels to `JIT`, `VRM`, and `FAN`. |
| 2026-11-08 | Prevented GPU/jitter regulation from decreasing the proposed FPS target below the measured 10% low. |
| On or before 2026-11-08 | Added an explicit Vulkan layer with a per-swapchain overlay, presentation metrics, optional jitter graph, AMD GPU telemetry, and hardware sensor readings. |
| On or before 2026-11-08 | Added manual FPS limiting with configurable bounds and X11/XWayland shortcuts. |
| On or before 2026-11-08 | Added GPU/jitter and GPU VRM temperature automatic algorithms, with per-algorithm hysteresis, timing, steps, bounds, and measurement expiry. |
| On or before 2026-11-08 | Added TOML configuration, diagnostics, and configurable hardware sensor paths. |
| On or before 2026-11-08 | Made presentation FPS, 10% low, and jitter share a time-based sampling window and restart collection after a cap change. |
