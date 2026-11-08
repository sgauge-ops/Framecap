# Changelog

Changes are grouped under Unreleased until a release is made.
Dates reflect the first available Git record, not necessarily when development
originally took place.

## Unreleased

| Date | Changes |
| --- | --- |
| 2026-11-08 | Made the FPS value yellow by default, with a configurable RGB color in `[overlay].fps_color`. |
| 2026-11-08 | Added full, FPS-only, and off overlay modes, cycled with F12 and selectable at startup. |
| 2026-11-08 | Added the active automatic algorithm's target and unit to the overlay status row. |
| 2026-11-08 | Moved GPU load and frame-jitter thresholds into `[auto-mode.algorithms.gpu_threshold]`, where they are used. |
| 2026-11-08 | Removed the jitter graph and its configuration options; the JIT metric and automatic regulation remain available. |
| 2026-11-08 | Fixed duplicate X11 hotkey listeners when an application creates multiple Vulkan devices by sharing one process-wide listener across registered devices. |
| 2026-11-08 | Added the fan-speed automatic algorithm with a default target of 2000 RPM. |
| 2026-11-08 | Changed automatic mode labels to `JIT`, `VRM`, and `FAN`. |
| 2026-11-08 | Prevented GPU/jitter regulation from decreasing the proposed FPS target below the measured 10% low. |
| On or before 2026-11-08 | Added an explicit Vulkan layer with a per-swapchain overlay, presentation metrics, AMD GPU telemetry, and hardware sensor readings. |
| On or before 2026-11-08 | Added manual FPS limiting with configurable bounds and X11/XWayland shortcuts. |
| On or before 2026-11-08 | Added GPU/jitter and GPU VRM temperature automatic algorithms, with per-algorithm hysteresis, timing, steps, bounds, and measurement expiry. |
| On or before 2026-11-08 | Added TOML configuration, diagnostics, and configurable hardware sensor paths. |
| On or before 2026-11-08 | Made presentation FPS, 10% low, and jitter share a time-based sampling window and restart collection after a cap change. |
