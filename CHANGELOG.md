# Changelog

Changes are grouped under Unreleased until a release is made.
Dates reflect the first available Git record or the date of unreleased local
changes, not necessarily when development originally took place.

## Unreleased

| Date | Changes |
| --- | --- |
| 2026-11-11 | Added GDDR6 hotspot, GTT used/total and system RAM used to the full modern overlay. Grouped GDDR6 with PPT and FAN, placed RAM under FAN, and increased the dashboard height to 230 px at 1×. Harmonized hardware label sizes, aligned labels and values on a common baseline, right-aligned values by unit and reduced spacing between labels and values. |
| 2026-11-11 | Added GTT used/total, system RAM used and BC-250 GDDR6 hotspot to the ASCII overlay. `[framecap].memory_poll_rate_ms` defaults to 5000 ms; values below 2000 ms disable the `bc250_memory` hwmon reader. |
| 2026-11-09 | Shifted the HYS label and value one character left in the modern overlay. |
| 2026-11-09 | Changed VRM and FAN F6/F7 controls to adjust the active target by the configured hysteresis, leaving hysteresis unchanged; changes are temporary. JIT continues to adjust hysteresis. |
| 2026-11-09 | Changed global shortcut keys from F9–F12 to F5–F8 to avoid Steam conflicts. Existing explicit key bindings must be updated; invalid or duplicate bindings disable global shortcuts. |
| 2026-11-09 | Moved the initial cap to `[manual-mode].startup_fps`; the legacy `[framecap].target_fps` remains readable with a migration warning and the new key takes precedence. |
| 2026-11-09 | Changed all AUTO algorithms to per-algorithm rolling measurement windows and persistent direction confirmation, retaining history across cap changes. `auto_window_ms` defaults to 4000 ms and also sets the minimum interval between adjustments. |
| 2026-11-09 | Replaced AUTO percentage hysteresis with absolute `hysteresis`: defaults are 5 FPS (JIT), 5 °C (VRM), and 50 RPM (FAN); F10/F11 adjust by 1 FPS, 1 °C, and 10 RPM respectively. |
| 2026-11-09 | Removed `hysteresis_percent`, `auto_increase_interval_ms`, `auto_decrease_interval_ms`, and `measurement_expiry_ms` from AUTO configuration. Existing configuration files must be migrated; unknown keys reject the entire file. |
| 2026-11-09 | Made the overlay renderer switch occur after one second of holding the shortcut, without waiting for key release. |
| 2026-11-09 | Added a long press of the overlay shortcut (F12 by default) to switch temporarily between `modern` and `ascii`, including while hidden; a short press still cycles display modes. |
| 2026-11-09 | Renamed the `[overlay].renderer` values from `dashboard` to `modern` and from `classic` to `ascii`; existing configurations must be updated. |
| 2026-11-09 | Made the modern overlay more compact, reducing its full layout to 370 × 215 pixels at 1× and positioning the FPS label next to the displayed value. |
| 2026-11-09 | Reorganized the ASCII overlay to match the modern layout, with metrics first, hardware readings and shortcuts below, and horizontal separators. |
| 2026-11-09 | Raised the default `[overlay].scale_2x_min_width_px` from 3440 to 3840 pixels. |
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
