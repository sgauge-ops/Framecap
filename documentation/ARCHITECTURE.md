# Architecture

Framecap is an explicit Vulkan layer written in Rust using `ash`. It intercepts
Vulkan instance/device setup, swapchain operations, queues, and
`vkQueuePresentKHR`, forwarding calls to the next layer or driver. It activates
only when requested by the Vulkan loader.

## Presentation and metrics

Presentation times are recorded with a monotonic clock per swapchain. The
observed frame interval is a CPU-side interval between presentations, not GPU
execution time or a measurement of display scanout. FPS, the 10th percentile
FPS ("10% low"), and frame-time jitter displayed in the overlay share
presentation samples over `[overlay].gpu_smoothing_window_ms`. They show
`COLLECT` after startup or a cap change until a valid interval and the minimum
collection period are available.

GPU load and hardware sensors are sampled and smoothed separately for the
overlay. Automatic regulation keeps separate rolling histories of raw
presentation and telemetry samples per algorithm and swapchain, using each
algorithm's `auto_window_ms`; these histories survive cap changes. Telemetry
samples are deduplicated by timestamp. Displayed metrics can therefore differ
from the metrics used by AUTO, especially after a cap change. The
implemented GPU load backend is AMD sysfs; telemetry has availability,
timestamp, and device identity rather than inventing a value when unsupported.
Hardware readings can be discovered or configured by hwmon device and label
or by path. The renderer
draws the overlay into the application's swapchain with its own Vulkan
resources; errors are logged and should not unnecessarily stop presentation.

## FPS control

The limiter uses a monotonic, absolute deadline per presentation queue, once per
`vkQueuePresentKHR` call even for multiple swapchains. It resets its schedule
on target changes and avoids catch-up bursts after missed deadlines. The cap
can be adjusted manually or by one of the
[automatic algorithms](AUTO-MODE.md), which confirm a persistent direction
over their configured window; OFF disables the limiter while remembering
the last target. VSync or an application cap can result in a lower actual FPS.
The process-wide X11 input worker shares hotkey events with each registered
device's regulator and renderer; it is not part of the frame presentation loop.

## Source layout

The crate root exports the library as both a Rust library and a `cdylib` for
the Vulkan loader. `layer/mod.rs` declares the modules; the files below cover
the complete Rust source, integration-test, and runtime-resource layout.

### Rust library

| Path | Responsibility |
| --- | --- |
| [`src/lib.rs`](../src/lib.rs) | Crate root; exports the `layer` module. |
| [`src/layer/mod.rs`](../src/layer/mod.rs) | Declares the layer's public modules and the private `auto_metrics` module. |
| [`src/layer/loader.rs`](../src/layer/loader.rs) | Vulkan loader entry points, instance/device dispatch chains, swapchain and queue interception, presentation, and per-device runtime wiring. This is the active Vulkan-layer integration path. |
| [`src/layer/lib.rs`](../src/layer/lib.rs) | Shared measurement and hardware-metric data types, snapshots, and the high-level `FramecapLayer` state/helper. |
| [`src/layer/vulkan_interface.rs`](../src/layer/vulkan_interface.rs) | Simple queue-local metrics state and layer initialization/presentation helpers. These helpers are currently standalone and are not called by `loader.rs`. |
| [`src/layer/metrics.rs`](../src/layer/metrics.rs) | Aggregates presentation intervals into FPS, 10% low, jitter, and GPU measurement snapshots. |
| [`src/layer/limiter.rs`](../src/layer/limiter.rs) | Thread-safe FPS target controller and absolute-deadline per-queue frame scheduler. |
| [`src/layer/auto.rs`](../src/layer/auto.rs) | AUTO algorithms, evaluation scheduling, cap changes, target/hysteresis adjustments, and bounds. |
| [`src/layer/auto_metrics.rs`](../src/layer/auto_metrics.rs) | Time-windowed presentation and telemetry histories maintained per AUTO algorithm and swapchain. |
| [`src/layer/config.rs`](../src/layer/config.rs) | Loads configuration and converts validated settings into runtime configuration, including defaults and diagnostics for invalid input. |
| [`src/layer/settings.rs`](../src/layer/settings.rs) | Configuration data structures and defaults for polling, hotkeys, overlay, sensors, manual mode, and AUTO algorithms. |
| [`src/layer/telemetry.rs`](../src/layer/telemetry.rs) | GPU telemetry providers, CPU/GPU/hwmon readers, sensor selection, expiration, and time-window smoothing. |
| [`src/layer/memory_temperature.rs`](../src/layer/memory_temperature.rs) | Reads the BC-250 GDDR6 hotspot from sysfs/hwmon, including saturated readings. |
| [`src/layer/input_x11.rs`](../src/layer/input_x11.rs) | Process-wide X11/XWayland shortcut listener and dispatch to registered devices. |
| [`src/layer/renderer.rs`](../src/layer/renderer.rs) | Vulkan swapchain renderer, ASCII and modern overlay layouts, display modes, and transient input feedback. |
| [`src/layer/dashboard.rs`](../src/layer/dashboard.rs) | Modern dashboard geometry, text glyph rasterization, and rectangle batching; an internal module used by `renderer.rs`. |
| [`src/layer/overlay.rs`](../src/layer/overlay.rs) | Simple text/console rendering of a metrics snapshot, separate from the Vulkan swapchain overlay. |
| [`src/layer/diagnostics.rs`](../src/layer/diagnostics.rs) | Configurable, synchronized diagnostic logging and process/environment context. |

### Vulkan and rendering resources

| Path | Responsibility |
| --- | --- |
| [`layer/VkLayer_BC250_framecap.json`](../layer/VkLayer_BC250_framecap.json) | Vulkan loader manifest declaring the layer and its shared-library path. |
| [`src/layer/shaders/dashboard.vert`](../src/layer/shaders/dashboard.vert), [`src/layer/shaders/dashboard.frag`](../src/layer/shaders/dashboard.frag) | GLSL vertex and fragment shaders for modern dashboard rectangles and rounded shapes. |
| [`src/layer/shaders/dashboard.vert.spv`](../src/layer/shaders/dashboard.vert.spv), [`src/layer/shaders/dashboard.frag.spv`](../src/layer/shaders/dashboard.frag.spv) | Checked-in SPIR-V binaries for the dashboard shaders. |
| [`src/layer/shaders/overlay_background.vert`](../src/layer/shaders/overlay_background.vert), [`src/layer/shaders/overlay_background.frag`](../src/layer/shaders/overlay_background.frag) | GLSL vertex and fragment shaders for the overlay background. |
| [`src/layer/shaders/overlay_background.vert.spv`](../src/layer/shaders/overlay_background.vert.spv), [`src/layer/shaders/overlay_background.frag.spv`](../src/layer/shaders/overlay_background.frag.spv) | Checked-in SPIR-V binaries for the overlay-background shaders. |
| [`src/layer/fonts/FiraSans-Regular.ttf`](../src/layer/fonts/FiraSans-Regular.ttf), [`src/layer/fonts/FiraSans-Bold.ttf`](../src/layer/fonts/FiraSans-Bold.ttf) | Fonts embedded and rasterized by the modern dashboard renderer. |
| [`src/layer/fonts/OFL.txt`](../src/layer/fonts/OFL.txt) | License for the bundled Fira Sans fonts. |

### Integration tests

| Path | Responsibility |
| --- | --- |
| [`tests/metrics_test.rs`](../tests/metrics_test.rs) | Checks metric aggregation, collection state, resets, low-10 FPS, and basic overlay output. |
| [`tests/telemetry_test.rs`](../tests/telemetry_test.rs) | Checks mock telemetry, unsupported GPU status, and timestamp expiry behavior. |
| [`tests/vulkan_layer_test.rs`](../tests/vulkan_layer_test.rs) | Exercises `vkcube` with and without the layer, overlay modes/renderers, diagnostics, and frame-cap timing when graphical prerequisites are available. |

[`Cargo.toml`](../Cargo.toml) defines the library, `cdylib`, and Rust
dependencies. The release packaging workflow is described in
[`../.github/workflows/release.yml`](../.github/workflows/release.yml).

See [validation](VALIDATION.md) for what has and has not been tested; this
description is not a claim of compatibility across all Vulkan drivers.
