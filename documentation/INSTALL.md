# Installing Framecap

Framecap is an explicit Vulkan layer for Linux; there is no separate application
to launch. The instructions below build from source and activate it for one
process or Steam game. The project is experimental. The default installation
directory is `$HOME/FrameCap`; it contains both the configuration and the
built Vulkan layer library.

## Requirements

- 64-bit Linux, a graphical session, a working Vulkan loader, and a Vulkan driver.
- Rust and Cargo; see [rustup](https://rustup.rs/) if needed.
- `git` to clone the sources; `vkcube` and `timeout` for the optional test.
- X11 or XWayland for global F5-F8 shortcuts. Native Wayland does not support
  these shortcuts through the current backend.

For example, on Debian or Ubuntu:

```sh
sudo apt update
sudo apt install git build-essential libvulkan1 mesa-vulkan-drivers vulkan-tools
```

Install the correct Vulkan driver for your GPU; package names vary by
distribution.

## Download and build

```sh
git clone https://github.com/sgauge-ops/bc250-framecap.git "$HOME/FrameCap"
cd "$HOME/FrameCap"
cargo build --locked
```

The development manifest
[`layer/VkLayer_FRAMECAP_observability.json`](../layer/VkLayer_FRAMECAP_observability.json)
references `../target/debug/libframecap.so`. Keep the manifest and the built
library in those relative locations; rebuild after changing source code.
`cargo build --release` alone does not update this development manifest.

## Configure

Edit `$HOME/FrameCap/framecap.toml` for your machine. To make this
configuration available to games launched from other directories, create a
symbolic link in the configuration directory searched by Framecap:

```sh
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}"
mkdir -p "$CONFIG_DIR"
ln -sT "$HOME/FrameCap/framecap.toml" "$CONFIG_DIR/framecap.toml"
```

The link command does not overwrite an existing configuration. If the
destination already exists, review it and decide how to migrate it before
creating the link. Use the same `XDG_CONFIG_HOME` as the Steam process; if it
is unset, Framecap searches `$HOME/.config`.

The example contains machine-specific sensor overrides and values that differ
from the built-in defaults. Review or remove the `[sensors]` overrides before
use; selectors are specific to the machine. A `framecap.toml` in the game's working
directory takes precedence over the link. See [configuration](CONFIGURATION.md).

## Activate for a Steam game

Under **Properties > General > Launch Options**, add:

```sh
VK_LAYER_PATH="$HOME/FrameCap/layer" VK_INSTANCE_LAYERS=VK_LAYER_FRAMECAP_observability %command%
```

Steam replaces `%command%` with the game command. This activates the layer for
that game, not globally. Remove the launch options to disable it.

## Test and troubleshoot

From the installed source directory, with a graphical session and `vkcube`:

```sh
cd "$HOME/FrameCap"
VK_LAYER_PATH="$PWD/layer" VK_INSTANCE_LAYERS=VK_LAYER_FRAMECAP_observability vkcube --c 300
```

On systems with multiple display backends, `vkcube --wsi wayland` or
`vkcube --wsi xcb` can select an available backend. Compare with a run without
`VK_INSTANCE_LAYERS`. The log should indicate that Framecap loaded; see
[diagnostics](CONFIGURATION.md#diagnostics) for paths and troubleshooting.
For Proton, `PROTON_LOG=1 PROTON_LOG_DIR=/tmp` may provide additional context.
Review logs for private paths before sharing them.
For missing overlays, crashes, and instructions to collect Framecap and Proton
logs, see [troubleshooting](TROUBLESHOOTING.md).

For automated checks, run `cargo build --locked && cargo test --locked`. The
`vkcube` integration test may skip when graphical prerequisites are absent.

## Uninstall

Remove the Steam launch options and, if no longer needed, delete
`$HOME/FrameCap` and its configuration link in `$XDG_CONFIG_HOME` or
`$HOME/.config`. Verify that the link points to the installation before
removing it. Remove the diagnostics log separately if desired. Framecap does
not install a global Vulkan manifest.
