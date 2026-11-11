# Introduction

FrameCap is a Linux Vulkan layer that provides an overlay with manual or experimental automatic FPS regulation.

FrameCap has been developped and validated with a bc250 mining board running CachyOS with [bc250 kernel](https://github.com/MastaG/linux-cachyos-bc250) and [cyan-skillfish governor](https://github.com/filippor/cyan-skillfish-governor).
Any other configuration is unknown at the moment.

FrameCap offers the following modes, that can be cycled-through via [F5] (by default):
| mode | description | default (*) |
| --- | --- | --- |
| OFF | Overlay only, no frame capping | -- |
| MANUAL | User defined frame capping, use [F6] and [F7] respectively decrease and increase frame cap | 90 fps target |
| AUTO (JIT) | Adjusts the cap using 10%-low FPS, GPU load and jitter; [F6] and [F7] adjust the hysteresis in FPS | 5 fps hysteresis |
| AUTO (VRM) | Adjusts the cap around a VRM temperature target; [F6] and [F7] adjust the target by the hysteresis in °C | 5°C hysteresis |
| AUTO (FAN) | Adjusts the cap around a fan-speed target; [F6] and [F7] adjust the target by the hysteresis in RPM | 50 rpm hysteresis |

(*) Default settings can be configured in `framecap.toml`.

See [automatic regulation](documentation/AUTO-MODE.md) for the rules and limitations. The algorithms are still experimental and being fine-tuned. Your feedback is welcome.

A short [F8] press cycles the overlay through full, FPS only, and off.

Holding [F8] for one second switches between the `modern` and `ascii` renderers. Background logic remains active when the overlay is hidden.

# Short installation guide

## Install the Linux x86_64 binary

Download and extract the archive attached to the
[latest release](https://github.com/sgauge-ops/bc250-bc250-framecap/releases/latest):

```sh
mkdir -p "$HOME/bc250-framecap" ;
curl -fL https://github.com/sgauge-ops/bc250-framecap/releases/latest/download/framecap-linux-x86_64.tar.gz \
  | tar -xz -C "$HOME/bc250-framecap"
```

The archive contains the Vulkan layer manifest, release library, and an example
configuration. It does not replace a local `framecap.toml`; FrameCap uses
built-in defaults if no configuration file exists. This binary targets 64-bit
x86 Linux with glibc 2.35 or newer. Other architectures and native Wayland
shortcuts are not supported by this binary.

## Configure

To start with the example configuration, copy it once to `~/.config/` directory to
make it globally available, and review its machine-specific sensor overrides:

```sh
mkdir -p "$HOME/.config" ;
cp -n "$HOME/bc250-framecap/framecap.toml.example" "$HOME/.config/framecap.toml"
ln -sT "$HOME/.config/framecap.toml" "$HOME/bc250-framecap/framecap.toml"
```

Edit `$HOME/bc250-framecap/framecap.toml` and adjust your preferences.

## Test with vkcube

From your home directory, with a graphical session and `vkcube` available:

```sh
cd ~/ ;
VK_LAYER_PATH="$HOME/bc250-framecap/layer" VK_INSTANCE_LAYERS=VK_LAYER_FRAMECAP_observability vkcube --c 300
```

Framecap should show-up :

![vkcube](documentation/screenshots/vkcube%20overlay.png)

## Activate for a Steam game

Under **Properties > General > Launch Options**, add:

```sh
VK_LAYER_PATH="$HOME/bc250-framecap/layer" VK_INSTANCE_LAYERS=VK_LAYER_FRAMECAP_observability %command%
```

Steam replaces `%command%` with the game command. This activates the layer for
that game, not globally. Remove the launch options to disable it.

## Update

```sh
curl -fL https://github.com/sgauge-ops/bc250-bc250-framecap/releases/latest/download/framecap-linux-x86_64.tar.gz \
  | tar -xz -C "$HOME/bc250-framecap"
```

Extraction updates the library and manifest without replacing
`framecap.toml`.

# Local Build

## Requirements

Install the [Rust toolchain](https://rustup.rs/) and the Linux development
packages listed in the [installation guide](documentation/INSTALL.md).

## Clone and build

```sh
git clone https://github.com/sgauge-ops/bc250-framecap.git "$HOME/bc250-framecap-src"
cd "$HOME/bc250-framecap-src"
cargo build --locked
```

The source checkout includes a development manifest that points to
`target/debug/libframecap.so`. Activate this build with
`VK_LAYER_PATH="$HOME/bc250-framecap-src/layer"`; review the machine-specific sensor
overrides in its `framecap.toml` before use.

## Update and rebuild

```sh
cd "$HOME/bc250-framecap-src"
git pull --ff-only
cargo build --locked
```

# Going further

- [Documentation index](INDEX.md)
- [Installation and Steam setup](documentation/INSTALL.md)
- [Configuration and keyboard shortcuts](documentation/CONFIGURATION.md)
- [Troubleshooting and logs](documentation/TROUBLESHOOTING.md)
- [Known limitations and pending work](TODO.md)
- [Changelog](CHANGELOG.md)
