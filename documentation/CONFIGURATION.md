# Configuration

Framecap reads `framecap.toml` at layer initialization, in this order:

1. The process working directory.
2. `$XDG_CONFIG_HOME/framecap.toml`, if `XDG_CONFIG_HOME` is set.
3. `~/.config/framecap.toml`.

The first existing file wins. Changes require restarting the application.
If no file is found, the checked paths are logged and built-in defaults are
used. Invalid TOML, unknown fields, or read errors are logged and cause a
fallback to built-in settings. Individual invalid values can have their own
validation behavior, documented in the
[example file](../framecap.toml).

**The tracked example is a locally tuned configuration, not the built-in
defaults.** In particular it sets an initial target of 90 FPS, selects specific
hwmon devices and labels, and writes diagnostics to `/tmp/framecap.log`. Review
it before using it on a different machine. The [installation procedure](INSTALL.md) keeps
this file in `$HOME/bc250-framecap` and links it from the user configuration
directory. If running from the repository root, that file overrides the user
configuration.

## Settings

| Section | Purpose |
| --- | --- |
| `[framecap]` | Initial cap, metrics window, and GPU polling interval. `target_fps = 0` starts in OFF. |
| `[key-bindings]` | Enable and assign distinct F9-F12 shortcuts. |
| `[sensors]` | Optional hwmon device/label selectors or path overrides for temperatures, power, and fan speed. |
| `[overlay]` | Visibility, refresh, smoothing, text scaling, FPS color, and background. |
| `[auto-mode]` | Algorithm order. |
| `[auto-mode.algorithms.<name>]` | Metric, target, absolute hysteresis, collection/confirmation window, steps, and FPS bounds; `gpu_threshold` also owns the GPU/jitter thresholds. |
| `[manual-mode]` | Manual FPS step and bounds. |
| `[diagnostics]` | Log path and append behavior. |

The [example file](../framecap.toml) documents individual keys and built-in
defaults. The three automatic algorithms and their complete rules are described
in [automatic regulation](AUTO-MODE.md).

Sensor selectors match the hwmon device's `name` and sensor label rather than
the volatile `hwmonN` index. For example:

```toml
[sensors]
fan_rpm = { device = "nct6686", label = "Pump Fan" }
```

Selectors are supported for `cpu_temp`, `gpu_vrm_temp`, `cpu_vrm_temp`,
`gpu_edge_temp`, `gpu_power`, `cpu_power`, `ppt_power`, and `fan_rpm`.
Names and labels can be inspected with `sensors` or under `/sys/class/hwmon/`.
A selector must match exactly one usable sensor; missing or ambiguous matches
are logged and the reading is unavailable, without falling back to another
sensor. Selected channels use `_input` when available, otherwise `_average`.
The selection is resolved at startup, so restart the application after hardware
or driver changes.

Existing `*_path` overrides remain supported, but hwmon indices may change
after a reboot or hardware changes. A selector and a path for the same sensor
conflict and disable that reading with a warning. Without either override,
Framecap discovers sensors using built-in labels; notably, the default fan
label is `CPU Fan`. An invalid path logs a warning before falling back to that
discovery. Power channel paths can omit `_input`. Missing or stale readings do
not produce synthetic sensor values. The `fan_rpm` algorithm uses the fan
sensor reading and stops adjusting when its AUTO window has no valid data.

Legacy settings moved from `[framecap]` are read for migration without
rewriting the file; conflicting old and new settings are rejected. The old
single-rule auto-mode keys `jitter_high`, `jitter_low`, and `gpu_saturation`
are obsolete and do not control the current algorithms.

## Automatic mode migration

The current AUTO configuration is not compatible with the earlier
per-algorithm timing and percentage hysteresis keys. In every
`[auto-mode.algorithms.<name>]` section:

1. Remove `hysteresis_percent`, `auto_increase_interval_ms`,
   `auto_decrease_interval_ms`, and `measurement_expiry_ms`.
2. Set `hysteresis` in the algorithm's units and `auto_window_ms` in
   milliseconds. Built-in values are 5 FPS for `gpu_threshold`, 5 °C for
   `vrm_temp`, 50 RPM for `fan_rpm`, and 4000 ms for each window.
3. Keep or retune the existing target, FPS steps, bounds, and GPU/jitter
   thresholds as needed. Percentage and absolute hysteresis are not generally
   equivalent; in particular, the FPS cap can change.

The same window governs collection, direction confirmation, and minimum
spacing between effective adjustments. Removing the old keys is essential:
unknown keys reject the **entire** configuration file, not just that
algorithm, and Framecap falls back to built-in settings. Update the actual
file found by the lookup order above, then restart the application.
See [automatic regulation](AUTO-MODE.md) for timing and decision details.

## Keyboard shortcuts

The defaults are F9 to cycle manual, configured automatic algorithms, OFF,
then manual; F10/F11 decrease/increase the manual cap or active automatic
hysteresis (by 1 FPS, 1 °C, or 10 RPM according to the active algorithm);
a short F12 press cycles the overlay through full, FPS only, and off.
Holding F12 for one second switches between `modern` and `ascii` without
waiting for release or changing the display mode. The switch is temporary
and also uses the configured `hotkey_toggle` if reassigned. Limiter OFF
remembers the previous cap.
All shortcuts are configurable and must be distinct. They require X11/XWayland
and may prevent the application from receiving those keys. Native Wayland has no
global shortcut support in the current backend.

## Diagnostics

The built-in log location is `$XDG_STATE_HOME/framecap/framecap.log`, or
`~/.local/state/framecap/framecap.log` when that variable is unset. An
explicit `[diagnostics].log_path`, including the one in the tracked example,
overrides it. If the file cannot be opened, diagnostics go to stderr.

`append_log = 0` truncates the log at startup; `1` appends. Steam/Proton can
load the layer in multiple processes, each of which can truncate the same file.
Use append mode when collecting multi-process traces. Logs include executable
and working-directory paths, selected environment and hardware context,
effective configuration, Vulkan initialization, and errors; review them before
sharing. To diagnose loader discovery, temporarily set
`VK_LOADER_DEBUG=error,warn,layer` for the application.
