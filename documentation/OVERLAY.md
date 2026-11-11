# Overlay specification

`[overlay].renderer` selects `modern` for a graphical dashboard or `ascii`
for the original text layout. The built-in default is `ascii`; the supplied
configuration selects `modern`. Both use the same data and controls. Existing
configurations must replace `dashboard` with `modern` and `classic` with `ascii`.
The dashboard displays FPS, 10% low and jitter prominently, followed by mode,
cap, hysteresis, target, hardware readings and key hints. It uses the bundled
Fira Sans font (see `src/layer/fonts/OFL.txt`), with antialiased text and
rounded, shaded panels, keycaps and mode badge. The dashboard has a gray
outline; its background opacity is configurable independently of the outline
and text. Very long values are reduced in size or ellipsized within their
cells. At small swapchain sizes, the dashboard is clipped to the available
area. The full dashboard measures 370 × 215 pixels at 1×. It is rendered at
2× when the swapchain width reaches
`[overlay].scale_2x_min_width_px` (default: 3840 px) and the whole dashboard
fits at that scale.
In the full dashboard, the FPS label follows the displayed value and moves as
its width changes.
An unknown renderer name rejects the configuration file. The configured
renderer is selected at application startup.

The ascii overlay presents:

```text
         FPS:   60        | 10% LOW: 55 FPS  | JIT:   0.5 MS    |
-----------------------------------------------------------------
         MODE: AUTO (JIT) | CAP:     60 FPS  | HYS:   5.0 FPS   | TARGET: 55 FPS
-----------------------------------------------------------------
         GPU:    43 °C    | VRM:   57 °C     | Load:   55 %     | PPT:  32.6 W
         CPU:    70 °C    | VRM:   61 °C     | Load:   35 %     | RPM:  1240 RPM
         GDDR6:  56 °C    | GTT:    512 / 12000 MB              | RAM:  4000 MB
-----------------------------------------------------------------
         [F5] CYCLE MODE  | [F6]  -1 FPS     | [F7]  +1 FPS     | [F8] CYCLE OSD
```

The third hardware row is shown only in the full ASCII overlay. GTT used and
total come from the selected AMD GPU's `mem_info_gtt_used` and
`mem_info_gtt_total` sysfs attributes. RAM used is `MemTotal - MemAvailable`
from `/proc/meminfo`, excluding swap and accounting for reclaimable memory.
Memory values are displayed as decimal MB (1,000,000 bytes); RAM and GTT may
overlap on unified-memory systems and must not be added together.

On a uniquely selected BC-250 GPU, GDDR6 is the hottest of all eight memory
chips, read from the `bc250_memory` hwmon device's `VRAM Hotspot` channel.
`[framecap].memory_poll_rate_ms` defaults to 5000 ms; values below 2000
disable this reading, without disabling GTT or RAM. The kernel module must be
loaded separately; Framecap does not load it, install its SMU patch, or request
Polkit authorization. The module accesses the SMU and may apply a temporary
patch at load time; follow the kernel's firmware and SMU debug prerequisites.
Its mailbox access does not coordinate with other SMU tools: avoid concurrent
overclocking, patching or other SMU transactions while polling. On an error,
polling stops until the application is restarted; unavailable, invalid or
stale readings display `--` and are logged. Code saturation is shown as a
lower bound with `>=`. Closing the application waits for an ongoing kernel
read to finish. See the [validation guide](VALIDATION.md).

On release, a short press of F8 cycles full overlay → FPS only → off → full
overlay; holding it for one second switches between `modern` and `ascii`
without waiting for release or changing the display mode. The same applies to
a custom `[key-bindings].hotkey_toggle`. The renderer switch is temporary and
is not written to the configuration file; it also works while the overlay is
hidden. FPS-only mode shows just the FPS measurement (or `COLLECT` while
collecting) with its own background. `[overlay].startup` selects the initial mode: `0` off,
`1` FPS only, `2` full overlay (built-in default). These display modes do not
disable frame limiting or automatic regulation.

The FPS value, including `COLLECT` while collecting, is yellow by default in
both visible modes. `[overlay].fps_color` sets its color as three RGB integers
from 0 to 255, for example `[255, 255, 0]`. Other text keeps its usual colors.
An invalid RGB array rejects the configuration file, as with other invalid TOML
types.

Columns are aligned across lines: F6/CAP/10% LOW/VRM, F7/HYS/JIT/Load,
and F8/TARGET/PPT/RPM. In the full overlay, FPS is shown as `FPS: value`;
while collecting, both FPS and 10% LOW show `COLLECT`. In AUTO, TARGET shows
the selected algorithm's reference: the displayed 10% low in FPS for JIT,
the configured temperature in °C for VRM, or the configured speed in RPM for
FAN. JIT shows
`TARGET: COLLECT FPS` while its reference is unavailable; MANUAL and OFF show
`TARGET: --`. A cap change highlights `CAP`, a change in active
hysteresis highlights `HYS`, an adjusted VRM/FAN target highlights `TARGET`,
and a key press highlights that key for one second. These highlights are independent. HYS is displayed in FPS for JIT,
°C for VRM, and RPM for FAN; the shortcut hints show the corresponding
adjustment step.

`JIT` remains a numeric frametime-variation metric, independent of any graph.
`[overlay].background_opacity` controls the background from 0.0 (transparent)
to 1.0 (opaque); the built-in default is 0.25. For a more opaque dashboard
similar to the mockup, try `background_opacity = 0.8`. The overlay is redrawn
on every presentation. Displayed metrics refresh every `[overlay].refresh_rate`
presentations, built-in default 10. Sensor readings are averaged independently
over `[overlay].gpu_smoothing_window_ms`; an unavailable reading clears that
sensor's history only.
These are overlay values, not the separate rolling values used for AUTO
decisions. Overlay FPS, 10% low and jitter restart collection after a cap
change; AUTO histories do not. In particular, JIT's displayed TARGET is the
overlay's 10% low and can differ from AUTO's reference.

Dashboard shaders are checked in as both GLSL and SPIR-V. After changing a
shader, regenerate its binary with `glslangValidator -V`:

```sh
glslangValidator -V src/layer/shaders/dashboard.vert -o src/layer/shaders/dashboard.vert.spv
glslangValidator -V src/layer/shaders/dashboard.frag -o src/layer/shaders/dashboard.frag.spv
```
