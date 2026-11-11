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

## Install RUST toolchain
```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Download and build Framecap

```sh
git clone https://github.com/sgauge-ops/bc250-framecap.git "$HOME/FrameCap" ;
cd "$HOME/FrameCap" ;
cargo build --locked
```

## Configure

Edit `$HOME/FrameCap/framecap.toml` for your machine. To make this
configuration available to games launched from other directories, create a
symbolic link in the configuration directory searched by Framecap:

```sh
mkdir -p "$HOME/.config" ;
ln -sT "$HOME/FrameCap/framecap.toml" "$HOME/.config/framecap.toml"
```

## Test with vkcube

From your home directory, with a graphical session and `vkcube` available:

```sh
cd ~/ ;
VK_LAYER_PATH="$HOME/FrameCap/layer" VK_INSTANCE_LAYERS=VK_LAYER_FRAMECAP_observability vkcube --c 300
```

Framecap should show-up :

![vkcube](documentation/screenshots/vkcube%20overlay.png)

## Activate for a Steam game

Under **Properties > General > Launch Options**, add:

```sh
VK_LAYER_PATH="$HOME/FrameCap/layer" VK_INSTANCE_LAYERS=VK_LAYER_FRAMECAP_observability %command%
```

Steam replaces `%command%` with the game command. This activates the layer for
that game, not globally. Remove the launch options to disable it.

## Update and rebuild

```sh
cd "$HOME/FrameCap" ;
cp framecap.toml framecap.bak.toml ;
git reset --hard HEAD ;
git pull ;
cargo build --locked
```

# Going further

- [Documentation index](INDEX.md)
- [Installation and Steam setup](documentation/INSTALL.md)
- [Configuration and keyboard shortcuts](documentation/CONFIGURATION.md)
- [Troubleshooting and logs](documentation/TROUBLESHOOTING.md)
- [Known limitations and pending work](TODO.md)
- [Changelog](CHANGELOG.md)
