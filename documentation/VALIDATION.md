# Validation and known limitations

## Known observations

- Unit and integration tests exist for metrics, telemetry, controls, automatic
  regulation, and an optional `vkcube` comparison. A skipped graphics test is
  not a successful runtime validation.
- A prior local `vkcube` run over 1200 frames reported approximately 8.85 s
  without the layer, with the overlay visible, and with it hidden; the 60 FPS
  limiter run took approximately 20 s. These are limited observations, not a
  general overhead guarantee.
- Manual overlay visibility and F9/F10/F11 controls were exercised in a test
  environment. Broader visual and compatibility testing remains open.
- Wreckfest 2 reached a presentation screen with the overlay but later crashed
  when 3D began. A reported dump pointed at an OptiScaler XeFG/MFG/XeSS DLL;
  a run without OptiScaler worked. Attribution and reproducibility remain
  unresolved.
- GPU load telemetry currently supports AMD. Global shortcuts need
  X11/XWayland; distinguish native Wayland from XWayland in results.

## Repeatable test plan

Test `vkcube`, a Vulkan stress test, and multiple native and Steam/Proton games
across available GPUs, drivers, X11/XCB, XWayland, and native Wayland. For each
combination, check layer loading, presentation, overlay visible/hidden,
limiter OFF/manual/auto, swapchain recreation, and supported shortcuts.
Unavailable telemetry is a valid degraded state, not a fabricated reading.

Compare runs without Framecap, with overlay visible and hidden, and with the
limiter independently enabled. Keep application, scene, duration, resolution,
driver, backend, and settings fixed. Record FPS and presentation frame-time
p50/p95/p99, CPU use where available, errors, and raw data or measurement
method. A CPU presentation interval is not a GPU execution or scanout time.
Use the Khronos validation layer when available.

For polling comparisons, test 10, 50, 100, and 200 ms with identical hardware,
scene, smoothing and expiry settings. Observe idle and repeatable load, sensor
freshness, reaction, regulator decisions, and CPU cost. Do not change defaults
based on a single unrepeatable run.

For each trial record application/version and scene, duration, GPU/driver,
display backend, configuration and modes, comparison baseline, measurements,
errors, and verdict: passed, failed, untested, or not applicable. Remove
private paths and data from any shared logs. Track outstanding work in
[TODO](../TODO.md), not as an implied compatibility guarantee.
