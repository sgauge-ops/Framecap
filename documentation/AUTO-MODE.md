# Automatic regulation

This is the reference for automatic FPS cap regulation. When changing it,
update the implementation, tests, configuration example, and other affected
documentation.

## Configuration and modes

Algorithms are configured under `[auto-mode.algorithms.<name>]` and cycled in
`[auto-mode].algorithm_order`. Each has a metric, target, hysteresis percentage,
independent increase/decrease intervals and FPS steps, FPS bounds, and
measurement expiry. Supported metric/target pairs are:

- `gpu_threshold`: `fps` / `low_10_fps`, gated by GPU load and jitter.
- `vrm_temp`: `gpu_vrm_temp_c` / a positive numerical temperature in °C.
- `fan_rpm`: `fan_rpm` / a positive numerical fan speed in RPM.

Invalid combinations revert to the built-in pair for that algorithm. Common
`gpu_increase_limit`, `gpu_decrease_limit`, and `frame_jitter_limit` apply only
to `gpu_threshold`. The shipped example may differ from the built-in defaults.

A positive `[framecap].target_fps` starts in manual mode; zero starts OFF. F9
cycles manual through the configured algorithms and OFF, then restores the
last cap in manual mode. By default the order is JIT, VRM, FAN. The overlay
shows the name `AUTO (JIT)`, `AUTO (VRM)`, or `AUTO (FAN)` independently of
order. In automatic mode F10/F11 decrease/increase active hysteresis by one
percentage point, within 0–100%; in manual mode they change the cap by
`[manual-mode].step_fps`. They do not change a parameter in OFF. Global
shortcuts require X11/XWayland.

## Measurements and timing

Presentation metrics update on each presentation. Automatic evaluation occurs
at most once per `[framecap].gpu_poll_rate_ms` per Vulkan device and is
immediate on entry to AUTO; multiple swapchains in one present call do not
cause multiple evaluations.

Average FPS, 10% low (the 10th percentile of presentation FPS), and jitter use
the same presentation samples in the `[overlay].gpu_smoothing_window_ms`
window. They reset together on a cap change and remain unavailable until a
valid interval and the minimum collection period have elapsed. GPU load and
hardware sensors have separate smoothing windows.

An algorithm makes no adjustment when the cap is disabled or its required
measurements are missing, invalid, expired, or timestamped in the future.
The FPS algorithm also needs available presentation history. Each algorithm
has its own expiry; global GPU telemetry expiry uses the largest configured
expiry. Hardware sensor algorithms use the hardware sample timestamp and do
not require the FPS history.

## Adjustment rules

The proposed cap is rounded down to an integer and subject to each algorithm's
`min_fps` / `max_fps`. Each direction has an independent interval counted from
the last effective change in that direction; attempts without a change do not
start or reset a timer.

### `gpu_threshold` (`JIT`)

Let `margin = current_cap * hysteresis_percent / 100`:

1. Increase by `auto_increase_step_fps` when smoothed GPU load is strictly
   below `gpu_increase_limit` and `current_cap - low_10_fps <= margin`. The
   difference is not absolute.
2. Otherwise, decrease by `auto_decrease_step_fps` when
   `low_10_fps < observed_fps - margin` and either GPU load is at least
   `gpu_decrease_limit` or jitter exceeds `frame_jitter_limit`. Skip the
   decrease if the rounded proposed cap is below `low_10_fps`.
3. Otherwise, leave the cap unchanged.

GPU load must be `Available`, finite, in 0–100%, and no older than the
algorithm's expiry. The independent GPU thresholds default to 98% and 90%;
the jitter threshold defaults to 2 ms and accepts 0–500 ms.

### `vrm_temp` (`VRM`)

For target `T` in °C and hysteresis `H` percent, the inclusive neutral band is
`[T - T*H/100, T + T*H/100]`. Below the band increase the cap; above it
decrease the cap; inside it do nothing. At the built-in target of 100 °C and
5% hysteresis the band is 95–105 °C. Missing or expired GPU VRM readings
prevent adjustment.

### `fan_rpm` (`FAN`)

For target `R` in RPM and hysteresis `H` percent, the inclusive neutral band is
`[R - R*H/100, R + R*H/100]`. Below the band increase the cap; above it
decrease the cap; inside it do nothing. At the built-in target of 2000 RPM and
5% hysteresis the band is 1900–2100 RPM. Missing, negative, non-finite, or
expired fan readings prevent adjustment; zero RPM is a valid reading.

## Built-in defaults

| Parameter | `gpu_threshold` | `vrm_temp` | `fan_rpm` |
| --- | ---: | ---: | ---: |
| `metric` / `target` | `fps` / `low_10_fps` | `gpu_vrm_temp_c` / `100.0` | `fan_rpm` / `2000.0` |
| `hysteresis_percent` | 3% | 5% | 5% |
| `auto_increase_interval_ms` | 5000 | 10000 | 10000 |
| `auto_decrease_interval_ms` | 5000 | 5000 | 5000 |
| Increase / decrease step | 5 / 5 FPS | 5 / 5 FPS | 5 / 5 FPS |
| `min_fps` / `max_fps` | 1 / 240 FPS | 1 / 240 FPS | 1 / 240 FPS |
| `measurement_expiry_ms` | 1000 | 1000 | 1000 |

Intervals and numerical targets must be positive; steps must be finite and
positive; FPS bounds must be ordered within 1–240; hysteresis must be finite
and within 0–100%. Invalid fields fall back to their validated defaults.

Tests should cover all rules, inclusive neutral bands, unavailable and stale
measurements, GPU and jitter gates, F9 and hysteresis controls, direction
timers, bounds, TOML configuration, and overlay labels.
