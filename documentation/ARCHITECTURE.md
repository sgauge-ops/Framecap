# Architecture

Framecap is an explicit Vulkan layer written in Rust using `ash`. It intercepts
Vulkan instance/device setup, swapchain operations, queues, and
`vkQueuePresentKHR`, forwarding calls to the next layer or driver. It activates
only when requested by the Vulkan loader.

## Presentation and metrics

Presentation times are recorded with a monotonic clock per swapchain. The
observed frame interval is a CPU-side interval between presentations, not GPU
execution time or a measurement of display scanout. FPS, the 10th percentile
FPS ("10% low"), and frame-time jitter share presentation samples over
`[overlay].gpu_smoothing_window_ms`. They show `COLLECT` after startup or a cap
change until a valid interval and the minimum collection period are available.
The optional graph has its own time window.

GPU load and hardware sensors are sampled and smoothed separately. The
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
can be adjusted manually or by one of the independently timed
[automatic algorithms](AUTO-MODE.md); OFF disables the limiter while remembering
the last target. VSync or an application cap can result in a lower actual FPS.
The process-wide X11 input worker shares hotkey events with each registered
device's regulator and renderer; it is not part of the frame presentation loop.

## Source layout

| Path | Responsibility |
| --- | --- |
| [`src/layer/loader.rs`](../src/layer/loader.rs) | Vulkan dispatch and layer integration. |
| [`src/layer/limiter.rs`](../src/layer/limiter.rs) | Cap controller and per-queue scheduler. |
| [`src/layer/auto.rs`](../src/layer/auto.rs) | Automatic regulation and hysteresis adjustment. |
| [`src/layer/renderer.rs`](../src/layer/renderer.rs) | Overlay drawing and presentation metrics. |
| [`src/layer/telemetry.rs`](../src/layer/telemetry.rs) | GPU and hardware sensor collection and smoothing. |
| [`src/layer/input_x11.rs`](../src/layer/input_x11.rs) | One process-wide X11/XWayland listener dispatches global shortcuts to registered Vulkan devices. |
| [`src/layer/config.rs`](../src/layer/config.rs), [`settings.rs`](../src/layer/settings.rs) | Configuration parsing and validation. |
| [`src/layer/diagnostics.rs`](../src/layer/diagnostics.rs) | Diagnostics logging. |
| [`src/layer/shaders/`](../src/layer/shaders/) | Shader sources and checked-in SPIR-V binaries. |

See [validation](VALIDATION.md) for what has and has not been tested; this
description is not a claim of compatibility across all Vulkan drivers.
