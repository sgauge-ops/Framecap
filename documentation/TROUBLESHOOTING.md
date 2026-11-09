# Troubleshooting

This guide covers a missing Framecap overlay and game crashes. Framecap is an
experimental Vulkan layer; a game may also fail for reasons unrelated to it.
Keep the game, driver, and launch settings the same when comparing runs.

## Overlay does not appear

1. Make sure the game uses Vulkan. Framecap is a Vulkan layer and cannot draw
   an overlay in a process using only another graphics API.
2. Verify that Steam **Properties > General > Launch Options** contains the
   following, adjusted if Framecap was installed elsewhere:

   ```sh
   VK_LAYER_PATH="$HOME/bc250-framecap/layer" VK_INSTANCE_LAYERS=VK_LAYER_FRAMECAP_observability %command%
   ```

   Keep any other required launch options and only one `%command%`. For
   sandboxed Steam installations, also check that the game can read the
   installation directory.
3. Check that the manifest and library exist:

   ```sh
   ls -l "$HOME/bc250-framecap/layer/VkLayer_FRAMECAP_observability.json" \
     "$HOME/bc250-framecap/target/debug/libframecap.so"
   ```

   If the library is missing, run `cd "$HOME/bc250-framecap" && cargo build
   --locked`. The supplied manifest references the debug library, not the
   release build.
4. With a graphical session and `vkcube` installed, test a Vulkan application
   outside the game:

   ```sh
   VK_LAYER_PATH="$HOME/bc250-framecap/layer" \
     VK_INSTANCE_LAYERS=VK_LAYER_FRAMECAP_observability \
     vkcube --c 300
   ```

   Compare with `vkcube --c 300` without Framecap. If the overlay works in
   `vkcube` but not in the game, collect the logs below for the game.
5. Check the effective configuration: `[overlay].startup = 2` shows the full
   overlay at startup, `1` shows FPS only, and `0` hides it. F12 cycles these
   modes by default. Shortcuts require
   X11/XWayland, may be disabled or rebound, and do not work through the
   current backend on native Wayland. Framecap uses one process-wide listener
   for all Vulkan devices in an application. A hidden overlay does not disable
   the limiter.
6. Check the [Framecap log](#framecap-diagnostics-log) for initialization,
   configuration, Vulkan, or overlay errors. An absent log can mean the layer
   was not loaded, but may also mean the configured log is elsewhere or could
   not be opened. Check stderr and the Vulkan loader diagnostics below.

Framecap searches `framecap.toml` in the process working directory, then
`$XDG_CONFIG_HOME` if set, then `$HOME/.config`. The first existing file wins:
an unintended file in the game directory can override the intended settings.
See [installation](INSTALL.md#configure) for the configuration link and
[configuration](CONFIGURATION.md) for lookup and validation details.

## Automatic regulation does not adjust the cap

Check the selected AUTO mode, its sensor availability and effective
configuration in the Framecap log. Older AUTO keys reject the entire file;
follow the [migration instructions](CONFIGURATION.md#automatic-mode-migration)
and restart the application if needed. Framecap logs effective adjustments as
`[AUTO] FPS limit adjusted`.

An initially empty history needs a full collection window followed by a full
period of consistent decisions: with the default 4000 ms window, the first
adjustment can take at least 8 seconds. Neutral or changing decisions,
missing measurements, cap bounds or interruptions can delay it further.
The overlay and AUTO use different windows, so the displayed values alone do
not reproduce an AUTO decision. See the [automatic regulation reference](AUTO-MODE.md).

## Game crashes or fails to start

1. Reproduce once without Framecap by temporarily removing its `VK_LAYER_PATH`
   and `VK_INSTANCE_LAYERS` launch options. Keep other necessary options. Then
   reproduce with Framecap enabled.
2. Note when the failure happens: startup, menu, entering 3D, or later. If
   possible, compare the same scene with the overlay hidden and visible, and
   with the cap OFF. On native Wayland, set `[overlay].startup = 0` or
   `[framecap].target_fps = 0` in the effective configuration and restart
   instead of relying on unavailable global shortcuts.
3. If other overlays, mods, or upscalers are involved, compare one change at a
   time. Do not conclude that Framecap caused a crash solely because it was
   loaded.
4. Collect both the Framecap and Proton logs for the failing run. If the game
   also fails without Framecap, collect a separate Proton log for that baseline.
   The [validation notes](VALIDATION.md) describe known limitations.

## Framecap diagnostics log

Framecap writes diagnostics automatically when the layer loads. Edit the
effective `framecap.toml` and set these keys in the existing `[diagnostics]`
section:

```toml
[diagnostics]
log_path = "/tmp/framecap.log"
append_log = 1
```

Do not add a second `[diagnostics]` section. Restart the game after changing
the configuration. `append_log = 1` preserves earlier entries and helps when
Steam/Proton launches several processes that load Framecap; with `0`, each
process can truncate the file. The supplied configuration already uses
`/tmp/framecap.log`, but defaults to `append_log = 0`. Append mode also
retains old sessions: use timestamps and process IDs to identify the relevant
run.

If `log_path` is omitted, the built-in location is
`$XDG_STATE_HOME/framecap/framecap.log`, or
`$HOME/.local/state/framecap/framecap.log` when `XDG_STATE_HOME` is unset.
The configured path overrides those defaults. If the log cannot be opened,
Framecap reports the problem and writes diagnostics to stderr instead.

After reproducing the issue and closing the game, inspect or attach the
configured log:

```sh
less /tmp/framecap.log
```

The log can contain the process ID, executable, working directory,
configuration, Vulkan initialization and error messages. If the file is absent,
check for a different effective configuration, permissions, or a layer loading
failure.

## Proton and Vulkan loader logs

For a Windows game running through Proton, temporarily use the following Steam
launch options to enable Proton logging while keeping Framecap enabled:

```sh
PROTON_LOG=1 PROTON_LOG_DIR=/tmp VK_LOADER_DEBUG=error,warn,layer VK_LAYER_PATH="$HOME/bc250-framecap/layer" VK_INSTANCE_LAYERS=VK_LAYER_FRAMECAP_observability %command%
```

Launch the game, reproduce the problem, and close it. Proton writes
`/tmp/steam-<APPID>.log`, where `<APPID>` is the Steam game ID; it can be found
in the game's Steam store URL. Without `PROTON_LOG_DIR`, Proton normally writes
the log to the home directory. If no Proton log appears, check that the game
actually uses Proton, that the options were applied, and that the destination
is writable and accessible to Steam.

`VK_LOADER_DEBUG=error,warn,layer` enables Vulkan loader diagnostics on stderr,
which can help identify a missing manifest, library, or loading error. They may
appear in Proton's log or in Steam's output; they are not Framecap's own log.
For a native Vulkan test, capture stderr directly:

```sh
VK_LOADER_DEBUG=error,warn,layer \
  VK_LAYER_PATH="$HOME/bc250-framecap/layer" \
  VK_INSTANCE_LAYERS=VK_LAYER_FRAMECAP_observability \
  vkcube --c 300 2> "$HOME/framecap-vulkan-loader.log"
```

Remove the temporary logging options after diagnosis. Proton logging is not
applicable to native Linux games.

## What to include in a report

Include the game and version, Steam App ID and Proton version where relevant,
Linux distribution, GPU and driver, display backend (X11, XWayland, or native
Wayland), Framecap revision, launch options, effective configuration, steps to
reproduce, and results with and without Framecap. Attach the Framecap log and,
for Proton games, the matching Proton log; include loader stderr when relevant.
Review logs and configuration for personal paths, account information, and
other sensitive data before sharing them.
