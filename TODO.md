# TODO

## Compatibility and measurements

- [ ] Test more Vulkan games, GPUs, drivers, resolutions, and presentation
      backends, including Steam/Proton, X11, XWayland, and native Wayland.
- [X] Investigate the observed Wreckfest 2 crash, including comparisons with
      and without Framecap and OptiScaler; do not assume Framecap is the cause.
- [X] Compare 10, 50, 100, and 200 ms GPU polling under reproducible load.
- [X] Measure CPU overhead and presentation latency on complex scenes, with
      and without the layer, with the overlay hidden and visible, and with the
      limiter evaluated separately.
- [ ] Validate Vulkan resource lifetime and synchronization with the Khronos
      validation layer; test long-running and swapchain-recreation scenarios.
- [X] Validate the overlay and automatic regulation visually across games,
      resolutions, backgrounds, and varying loads.
- [ ] Investigate an optional native Wayland global-shortcut backend.

## Fine-tuning

- [ ] Fine-tune `auto_window_ms`, `hysteresis` and adjustment steps, for the [auto-mode.algorithms.gpu_threshold] algorithm, 
- [X] Fine-tune `auto_window_ms` and `hysteresis` for the [auto-mode.algorithms.vrm_temp] algorithm
- [X] Fine-tune `auto_window_ms` and `hysteresis` for the [auto-mode.algorithms.fan_rpm] algorithm

## Maintenance

- [ ] Launch a complete code-review, for clean-up, improvements and documentation
- [ ] Document non-obvious synchronization, resource lifetime, and shared
      state invariants in code where needed.
- [ ] Expand automated coverage for concurrency, error handling, and
      multi-swapchain scenarios where it is currently missing.

## GUI

- [ ] Implement a simple configuration GUI
