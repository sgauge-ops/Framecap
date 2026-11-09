# Automatic regulation

This is the reference for automatic FPS cap regulation. When changing it,
update the implementation, tests, configuration example, and other affected
documentation.

## Configuration and modes

Algorithms are configured under `[auto-mode.algorithms.<name>]` and cycled in
`[auto-mode].algorithm_order`. Each has a metric, target, absolute hysteresis,
one collection and confirmation window, independent FPS steps, and FPS bounds.
Supported metric/target pairs are:

- `gpu_threshold`: `fps` / `low_10_fps`, gated by GPU load and jitter.
- `vrm_temp`: `gpu_vrm_temp_c` / a positive numerical temperature in °C.
- `fan_rpm`: `fan_rpm` / a positive numerical fan speed in RPM.

Invalid combinations revert to the built-in pair for that algorithm.
`gpu_increase_limit`, `gpu_decrease_limit`, and `frame_jitter_limit` are
configured in `[auto-mode.algorithms.gpu_threshold]`; they only affect the
`gpu_threshold` algorithm. The shipped example may differ from the built-in
defaults.

A positive `[framecap].target_fps` starts in manual mode; zero starts OFF. F9
cycles manual through the configured algorithms and OFF, then restores the
last cap in manual mode. By default the order is JIT, VRM, FAN. The overlay
shows the name `AUTO (JIT)`, `AUTO (VRM)`, or `AUTO (FAN)` independently of
order. In automatic mode F10/F11 decrease/increase active hysteresis by
1 FPS, 1 °C or 10 RPM, respectively; in manual mode they change the cap by
`[manual-mode].step_fps`. They do not change a parameter in OFF. Global
shortcuts require X11/XWayland.

## Measurements and timing

Presentation metrics update on each presentation. Automatic evaluation occurs
at most once per `[framecap].gpu_poll_rate_ms` per Vulkan device and is
immediate on entry to AUTO; multiple swapchains in one present call do not
cause multiple evaluations.

Overlay presentation metrics use `[overlay].gpu_smoothing_window_ms` and reset
on a cap change. AUTO instead collects raw presentation and telemetry samples
in a separate rolling `auto_window_ms` window for each algorithm and swapchain.
The FPS average, tenth percentile, jitter, GPU load and sensor averages use
that algorithm's window. AUTO history is not reset when the cap changes.
Histories are populated on presentations even when their algorithm is not
active, so an already populated window can be used after switching modes.
Each required measurement must have been collected for a full window before
the algorithm can make a decision. A window with no valid recent sample, a
future timestamp, an unavailable or invalid measurement cannot drive an
adjustment. Samples are removed as they leave the window; there is no separate
AUTO measurement expiry setting. Overlay telemetry expiry remains independent.
GPU and sensor averages are arithmetic averages of samples, rather than
time-weighted averages. Repeated telemetry timestamps are counted only once.

## Adjustment rules

The proposed cap is rounded down to an integer and subject to each algorithm's
`min_fps` / `max_fps`. The same direction must be requested continuously at
scheduled evaluations for one full `auto_window_ms` before an adjustment.
Neutral, invalid or opposite decisions interrupt confirmation. After an
effective adjustment, a new confirmation starts; any two effective adjustments
are separated by at least one window. Switching modes, a change to the cap or
hysteresis, a change of evaluated swapchain, and an evaluation gap longer than
twice the polling interval or two seconds (whichever is greater) also reset
confirmation. Persistence is checked at evaluation times; changes between
evaluations cannot be observed.
An initial full collection window followed by a full confirmation means the
first adjustment with initially empty histories can take at least two windows
(8 seconds with the built-in default), plus scheduling and presentation
delays. A pre-filled history can avoid the initial collection wait.

### `gpu_threshold` (`JIT`)

Let `margin = hysteresis` in FPS:

1. Increase by `auto_increase_step_fps` when smoothed GPU load is strictly
   below `gpu_increase_limit` and `current_cap - low_10_fps <= margin`. The
   difference is not absolute.
2. Otherwise, decrease by `auto_decrease_step_fps` when
   `low_10_fps < observed_fps - margin` and either GPU load is at least
   `gpu_decrease_limit` or jitter exceeds `frame_jitter_limit`. Skip the
   decrease if the rounded proposed cap is below `low_10_fps`.
3. Otherwise, leave the cap unchanged.

GPU load must be `Available`, finite, and in 0–100%.
Because the increase rule is checked first, increasing the FPS margin makes
increases eligible in more cases as well as making decreases harder to trigger.
`gpu_increase_limit` and `gpu_decrease_limit` default to
98% and 90% and accept 0–100%; `frame_jitter_limit` defaults to 2 ms and
accepts 0–500 ms.

### `vrm_temp` (`VRM`)

For target `T` and hysteresis `H` in °C, the inclusive neutral band is
`[T - H, T + H]`. Below the band increase the cap; above it
decrease the cap; inside it do nothing. At the built-in target of 100 °C and
5 °C hysteresis the band is 95–105 °C. Missing GPU VRM readings
prevent adjustment.
The confirmation delay also applies to high temperatures; this mode is not
an emergency thermal protection mechanism.

### `fan_rpm` (`FAN`)

For target `R` and hysteresis `H` in RPM, the inclusive neutral band is
`[R - H, R + H]`. Below the band increase the cap; above it
decrease the cap; inside it do nothing. At the built-in target of 2000 RPM and
50 RPM hysteresis the band is 1950–2050 RPM. Missing, negative or non-finite
fan readings prevent adjustment; zero RPM is a valid reading.

## Built-in defaults

| Parameter | `gpu_threshold` | `vrm_temp` | `fan_rpm` |
| --- | ---: | ---: | ---: |
| `metric` / `target` | `fps` / `low_10_fps` | `gpu_vrm_temp_c` / `100.0` | `fan_rpm` / `2000.0` |
| `hysteresis` | 5 FPS | 5 °C | 50 RPM |
| `auto_window_ms` | 4000 | 4000 | 4000 |
| Increase / decrease step | 5 / 5 FPS | 5 / 5 FPS | 5 / 5 FPS |
| `min_fps` / `max_fps` | 1 / 240 FPS | 1 / 240 FPS | 1 / 240 FPS |

Windows and numerical targets must be positive; steps must be finite and
positive; FPS bounds must be ordered within 1–240; hysteresis must be finite
and non-negative. Invalid fields fall back to their validated defaults.
The old `hysteresis_percent`, `auto_increase_interval_ms`,
`auto_decrease_interval_ms`, and `measurement_expiry_ms` settings are no
longer supported.
Because unknown keys reject the entire configuration file, update all copies
of `framecap.toml` used by the application before testing.

Tests should cover all rules, inclusive neutral bands, unavailable and stale
measurements, GPU and jitter gates, F9 and hysteresis controls, persistent
direction confirmation, bounds, TOML configuration, and overlay labels.
