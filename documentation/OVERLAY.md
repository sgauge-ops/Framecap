# Overlay specification

The overlay presents:

```text
MANUAL: | [F9] CYCLE MODE | [F10] -5 FPS | [F11] +5 FPS | [F12] OSD |
AUTO:   | [F9] CYCLE MODE | [F10] -1 %   | [F11] +1 %   | [F12] OSD |
        | MODE: MANUAL/AUTO (JIT/VRM/FAN)/OFF | CAP: xx FPS | HYS: xx % |
        | xx FPS | 10% LOW: xx FPS | JIT: xx MS |
        | GPU: xx °C | VRM: xx °C | Load: xx % | PPT: xx W |
        | CPU: xx °C | VRM: xx °C | Load: xx % | RPM: xx RPM |
        JITTER 0-50ms  (only when the jitter graph is enabled)
```

The jitter graph is disabled by default (`[overlay].jitter_graph_startup = 0`).
When disabled, neither the graph nor its history is rendered or calculated.
Its vertical scale is fixed at 0–50 ms, with higher values clipped.

Columns are aligned across lines: F10/CAP/10% LOW/VRM, F11/HYS/JIT/Load,
and F12/PPT/RPM. A cap change highlights `CAP`, a change in active
hysteresis highlights `HYS`, and a key press highlights that key for one
second. These highlights are independent.

`[overlay].background_opacity` controls the background from 0.0 (transparent)
to 1.0 (opaque); the built-in default is 0.25. The overlay is redrawn on every
presentation. Displayed metrics refresh every `[overlay].refresh_rate`
presentations, built-in default 10. Sensor readings are averaged independently
over `[overlay].gpu_smoothing_window_ms`; an unavailable reading clears that
sensor's history only.
