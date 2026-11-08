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
defaults.** In particular it sets an initial target of 90 FPS, uses explicit
hwmon paths, and writes diagnostics to `/tmp/framecap.log`. Review it before
using it on a different machine. The [installation procedure](INSTALL.md) keeps
this file in `$HOME/bc250-framecap` and links it from the user configuration
directory. If running from the repository root, that file overrides the user
configuration.

## Settings

| Section | Purpose |
| --- | --- |
| `[framecap]` | Initial cap, metrics window, and GPU polling interval. `target_fps = 0` starts in OFF. |
| `[key-bindings]` | Enable and assign distinct F9-F12 shortcuts. |
| `[sensors]` | Optional hwmon path overrides for temperatures, power, and fan speed. |
| `[overlay]` | Visibility, refresh, smoothing, text scaling, background, and jitter graph. |
| `[auto-mode]` | Algorithm order and GPU/jitter thresholds. |
| `[auto-mode.algorithms.<name>]` | Metric, target, hysteresis, intervals, steps, FPS bounds, and measurement expiry. |
| `[manual-mode]` | Manual FPS step and bounds. |
| `[diagnostics]` | Log path and append behavior. |

The [example file](../framecap.toml) documents individual keys and built-in
defaults. The three automatic algorithms and their complete rules are described
in [automatic regulation](AUTO-MODE.md).

Sensor paths refer to Linux sysfs hwmon files. Indices can change after a
reboot or hardware changes. Remove an override to allow hwmon label discovery;
power channel paths can omit `_input`. Missing or stale readings do not
produce synthetic sensor values. The `fan_rpm` algorithm uses the fan sensor
reading, and stops adjusting when it is unavailable or expired.

Legacy settings moved from `[framecap]` are read for migration without
rewriting the file; conflicting old and new settings are rejected. The old
single-rule auto-mode keys `jitter_high`, `jitter_low`, and `gpu_saturation`
are obsolete and do not control the current algorithms.

## Keyboard shortcuts

The defaults are F9 to cycle manual, configured automatic algorithms, OFF,
then manual; F10/F11 decrease/increase the manual cap or active automatic
hysteresis; F12 toggles the overlay. OFF remembers the previous cap.
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
