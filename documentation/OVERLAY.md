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
         MODE: AUTO (JIT) | CAP:     60 FPS  | HYS:   5.0 %     | TARGET: 60 FPS
-----------------------------------------------------------------
         GPU:   43 °C     | VRM:   57 °C     | Load:   55 %     | PPT:  32.6 W
         CPU:   70 °C     | VRM:   61 °C     | Load:   35 %     | RPM:  1240 RPM
-----------------------------------------------------------------
MANUAL:  [F9] CYCLE MODE  | [F10]  -5 FPS    | [F11]  +5 FPS    | [F12] CYCLE OSD
AUTO:    [F9] CYCLE MODE  | [F10]  -1 %      | [F11]  +1 %      | [F12] CYCLE OSD
```

On release, a short press of F12 cycles full overlay → FPS only → off → full
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

Columns are aligned across lines: F10/CAP/10% LOW/VRM, F11/HYS/JIT/Load,
and F12/TARGET/PPT/RPM. In the full overlay, FPS is shown as `FPS: value`;
while collecting, both FPS and 10% LOW show `COLLECT`. In AUTO, TARGET shows
the selected algorithm's reference: the displayed 10% low in FPS for JIT,
the configured temperature in °C for VRM, or the configured speed in RPM for
FAN. JIT shows
`TARGET: COLLECT FPS` while its reference is unavailable; MANUAL and OFF show
`TARGET: --`. A cap change highlights `CAP`, a change in active
hysteresis highlights `HYS`, and a key press highlights that key for one
second. These highlights are independent.

`JIT` remains a numeric frametime-variation metric, independent of any graph.
`[overlay].background_opacity` controls the background from 0.0 (transparent)
to 1.0 (opaque); the built-in default is 0.25. For a more opaque dashboard
similar to the mockup, try `background_opacity = 0.8`. The overlay is redrawn
on every presentation. Displayed metrics refresh every `[overlay].refresh_rate`
presentations, built-in default 10. Sensor readings are averaged independently
over `[overlay].gpu_smoothing_window_ms`; an unavailable reading clears that
sensor's history only.

Dashboard shaders are checked in as both GLSL and SPIR-V. After changing a
shader, regenerate its binary with `glslangValidator -V`:

```sh
glslangValidator -V src/layer/shaders/dashboard.vert -o src/layer/shaders/dashboard.vert.spv
glslangValidator -V src/layer/shaders/dashboard.frag -o src/layer/shaders/dashboard.frag.spv
```
