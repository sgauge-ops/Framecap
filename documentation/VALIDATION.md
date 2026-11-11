# Validation and known limitations

## Tests performed

These are observations from specific runs, not a compatibility guarantee.
The results below do not establish coverage for all devices or drivers.

| Test | Result and scope |
| --- | --- |
| Rust automated tests | On 2026-11-11, after removal of the standalone probe, `cargo test --locked` reported 145 passing tests and no failures: 137 library tests, seven metrics/telemetry integration tests, and one `vkcube` integration test. These cover metrics, configuration, controls, AUTO regulation, telemetry, rendering helpers, and other logic. The test output reports the overall graphics test as passing, but does not confirm whether its optional immediate-mode timing check ran. |
| Formatting and build | On 2026-11-11, `cargo fmt --check` and `cargo build --locked --release` succeeded locally. |
| Vulkan presentation | A previous local `vkcube` run of 1200 frames took approximately 8.85 s without the layer, with the overlay visible, and with it hidden; with a 60 FPS cap it took approximately 20 s. The environment and measurement detail are insufficient to generalize these numbers. |
| Manual controls | Overlay visibility and F9/F10/F11 controls were exercised in an earlier test environment. These are historical key bindings, not the current defaults (F5-F8); broader visual and shortcut validation remains open. |
| Release package | On 2026-11-11, the workflow's packaging step was run locally. The extracted `test-vkcube.sh` and `test-furmark.sh` had mode `755` and were executed directly against simulated commands to verify their environment and arguments. This did not run the graphical applications or validate GitHub Actions publication. |
| Wreckfest 2 | The overlay appeared on a presentation screen, but the game later crashed when 3D began. A reported dump pointed at an OptiScaler XeFG/MFG/XeSS DLL; a run without OptiScaler worked. Attribution and reproducibility remain unresolved. |

The `vkcube` integration test can return successfully without exercising
graphics when prerequisites are missing; inspect its output and environment
before treating a passing result as runtime validation. Similarly, simulated
commands and offline sensor tests do not validate real hardware.

GPU load telemetry currently supports AMD. Global shortcuts need X11/XWayland;
distinguish native Wayland from XWayland in results. Further tests are needed
for different hardware, drivers, games, display backends, and live BC-250
memory readings.

## Repeatable test plan

Framecap reads the `VRAM Hotspot` sensor exposed by the loaded
`bc250_memory` hwmon driver. It does not invoke Control Center, `pkexec` or
load the module. The driver accesses the SMU and may install a temporary
firmware patch; follow its firmware, debug-access and concurrency warnings.
A saturated reading is shown as `>=120 °C`, not as an exact temperature.
Missing, ambiguous, malformed and out-of-range readings are errors. A
successful offline test is not a hardware validation.

For the full ASCII overlay, test `[framecap].memory_poll_rate_ms` with `0`,
`1999`, `2000`, and `5000`. Verify that only the first two disable GDDR6,
while GTT and RAM remain available; automated `vkcube` tests explicitly disable
GDDR6 to avoid touching the SMU. With the module enabled and no conflicting
SMU transaction running, compare the hotspot with `sensors 'bc250_memory-*'`.
Verify missing and stale readings, saturation, ASCII alignment and clipping,
and unchanged modern/FPS-only modes. A failed read stops further attempts for
that application; inspect the diagnostic log before restarting. A pending
kernel read is allowed to finish when the application closes.

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
scene, overlay smoothing and AUTO window settings. Observe idle and repeatable
load, sensor freshness, reaction, regulator decisions, and CPU cost. Do not
change defaults based on a single unrepeatable run.

For AUTO, test each algorithm with a stable workload, short spikes, sustained
changes, missing telemetry, and recovery. Record the cap and decision times:
verify that short reversals do not trigger a change, persistent requests are
confirmed, and effective changes are separated by at least the configured
window. Check a fresh collection window and a pre-filled history when switching
modes, including history retained after a cap change. Compare displayed
metrics with AUTO's separate window rather than assuming they are identical.
For VRM and FAN, check how the configured decrease step and confirmation delay
affect response to sustained high readings; this regulator is not an emergency
thermal protection mechanism.

For each trial record application/version and scene, duration, GPU/driver,
display backend, configuration and modes, comparison baseline, measurements,
errors, and verdict: passed, failed, untested, or not applicable. Remove
private paths and data from any shared logs. Track outstanding work in
[TODO](../TODO.md), not as an implied compatibility guarantee.
