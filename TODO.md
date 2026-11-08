# TODO

## Before public release

- [ ] Choose a project license, add its text, and set the Cargo license field.
- [ ] Review the complete Git history and tracked files for secrets, personal
      paths, private information, and third-party material before publication.
      Removing data from the latest tree alone does not remove it from history.
- [ ] Confirm the public repository, visibility, and release process with the
      owner before publishing. Audit dependency licenses.
- [ ] Define a security reporting channel and add a security policy.
- [ ] Design release artifacts containing a matching Vulkan manifest and
      library; the development manifest currently points to `target/debug`.
- [ ] Review sensor path overrides and locally tuned values in `framecap.toml`
      before distributing them as general-purpose defaults.

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

Record tested and untested environments and results using
[the validation plan](documentation/VALIDATION.md).

## Maintenance

- [ ] Document non-obvious synchronization, resource lifetime, and shared
      state invariants in code where needed.
- [ ] Expand automated coverage for concurrency, error handling, and
      multi-swapchain scenarios where it is currently missing.
