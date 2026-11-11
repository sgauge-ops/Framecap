# Installing Framecap

Framecap is an explicit Vulkan layer for Linux; there is no separate application
to launch. The project is experimental. The default installation directory is
`$HOME/bc250-framecap`; it contains the configuration and Vulkan layer files.

## Requirements

- 64-bit Linux, a graphical session, a working Vulkan loader, and a Vulkan driver.
- [bc250 kernel](https://github.com/MastaG/linux-cachyos-bc250) to enable VRM and GDDR6 temp monitoring, with [memory sensors enabled at boot](https://github.com/MastaG/linux-cachyos-bc250#2-enable-the-bc-250-fansensor-driver-at-boot)
- `curl` and `tar` to install a release binary, or Rust and Cargo to build from
  source; see [rustup](https://rustup.rs/) if needed.
- `git` to clone the sources; `vkcube` and `timeout` for the optional test.
- X11 or XWayland for global F5-F8 shortcuts. Native Wayland does not support
  these shortcuts through the current backend.
- The release binary is for 64-bit x86 Linux and requires glibc 2.35 or newer.

For example, on Debian or Ubuntu:

```sh
sudo apt update
sudo apt install curl tar git build-essential libvulkan1 mesa-vulkan-drivers vulkan-tools
```

Install the correct Vulkan driver for your GPU; package names vary by
distribution.

## Install a release binary

Download and extract the Linux x86_64 archive from the
[latest release](https://github.com/sgauge-ops/bc250-framecap/releases/latest):

```sh
mkdir -p "$HOME/bc250-framecap"
curl -fL https://github.com/sgauge-ops/bc250-framecap/releases/latest/download/framecap-linux-x86_64.tar.gz \
  | tar -xz -C "$HOME/bc250-framecap"
```

The archive contains `layer/` and `lib/`. Its Vulkan manifest points to the
release library in `lib/`. Extraction updates those files without replacing a
local `framecap.toml`.

It also includes `framecap.toml.example`; copy that file
to `~/.config/framecap.toml` if you want to configure the layer, then review its
machine-specific sensor selectors. If no configuration exists, Framecap uses
its built-in defaults.

The archive also includes executable `test-vkcube.sh` and `test-furmark.sh` scripts.

## Build from source

```sh
git clone https://github.com/sgauge-ops/bc250-framecap.git "$HOME/bc250-framecap"
cd "$HOME/bc250-framecap"
cargo build --locked
```

The development manifest
[`layer/VkLayer_BC250_framecap.json`](../layer/VkLayer_BC250_framecap.json)
references `../target/debug/libframecap.so`. Keep the manifest and the built
library in those relative locations; rebuild after changing source code.
`cargo build --release` alone does not update this development manifest.
When upgrading an existing installation, update `VK_INSTANCE_LAYERS` to
`VK_LAYER_BC250_framecap`. If the old manifest remains in the layer directory,
remove `VkLayer_FRAMECAP_observability.json` so the obsolete layer name is no
longer discoverable.

## Configure

Edit `$HOME/bc250-framecap/framecap.toml` for your machine. To make this
configuration available to games launched from other directories, create a
symbolic link in the configuration directory searched by Framecap:

```sh
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}"
mkdir -p "$CONFIG_DIR"
ln -sT "$HOME/bc250-framecap/framecap.toml" "$CONFIG_DIR/framecap.toml"
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
VK_LAYER_PATH="$HOME/bc250-framecap/layer" VK_INSTANCE_LAYERS=VK_LAYER_BC250_framecap %command%
```

Steam replaces `%command%` with the game command. This activates the layer for
that game, not globally. Remove the launch options to disable it.

## Test and troubleshoot

From the installation directory, with a graphical session and `vkcube`:

```sh
cd "$HOME/bc250-framecap" ;
VK_LAYER_PATH="$PWD/layer" VK_INSTANCE_LAYERS=VK_LAYER_BC250_framecap vkcube --c 300
```

The packaged test scripts can also be run from this directory:

```sh
./test-vkcube.sh
./test-furmark.sh
```

They require `vkcube` and `furmark`, respectively, to be installed and on
`PATH`. The `vkcube` script requests the Wayland backend and immediate present
mode; both must be available. The scripts use the current working directory to
locate `layer/`, so run them from the installation directory.

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
`$HOME/bc250-framecap` and its configuration link in `$XDG_CONFIG_HOME` or
`$HOME/.config`. Verify that the link points to the installation before
removing it. Remove the diagnostics log separately if desired. Framecap does
not install a global Vulkan manifest.
