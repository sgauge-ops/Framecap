# Introduction

Framecap is an experimental Linux Vulkan layer that provides an overlay with manual or automatic FPS regulation.

Framecap has been developped and validated with a bc250 mining board running CachyOS with [bc250 kernel](https://github.com/MastaG/linux-cachyos-bc250) and [cyan-skillfish governor](https://github.com/filippor/cyan-skillfish-governor).
Any other configuration is unknown at the moment.

Framecap offers the following modes, that can be cycled-through via [F9] (by default):
| mode | description |
| --- | --- |
| OFF | Overlay only, no frame capping |
| MANUAL | User defined frame capping, use [10] and [F11] respectively decrease and increase frame cap |
| AUTO (FPS) | Tries to minimize jitter by limiting FPS to its 10%-low fps + user defined hysteresis, use [10] and [F11] to adjust hysteresis |
| AUTO (VRM) | Automatic frame capping so VRM doesn't exceed a target temp + user defined hysteresis, use [10] and [F11] to adjust hysteresis |
| AUTO (FAN) | Automatic frame capping so fan speed doesn't exceed a target rpm + user defined hysteresis, use [10] and [F11] to adjust hysteresis |

The automatic algorithms are still experimental and being fine-tuned. Your feedback is welcome.

F12 cycles Framecap's overlay through full, FPS only, and off. Background logic (mode dependent) remains active when the overlay is hidden.

Framecap's behavior is largely configurable in `framecap.toml`

The supplied configuration selects the graphical dashboard. Set
`[overlay].renderer = "classic"` to use the original text overlay instead;
both layouts retain the same F12 display modes and controls.

# Short installation guide

## Download and build

```sh
git clone https://github.com/sgauge-ops/Framecap.git "$HOME/bc250-framecap" ;
cd "$HOME/bc250-framecap" ;
cargo build --locked --release
```

## Configure

Edit `$HOME/bc250-framecap/framecap.toml` for your machine. To make this
configuration available to games launched from other directories, create a
symbolic link in the configuration directory searched by Framecap:

```sh
mkdir -p "$HOME/.config" ;
ln -sT "$HOME/bc250-framecap/framecap.toml" "$HOME/.config/framecap.toml"
```

## Test with vkcube

From your home directory, with a graphical session and `vkcube` available:

```sh
cd ~/ ;
VK_LAYER_PATH="$HOME/bc250-framecap/layer" VK_INSTANCE_LAYERS=VK_LAYER_FRAMECAP_observability vkcube --c 300
```

Framecap should show-up :

![vkcube](documentation/vkcube%20overlay.png)

## Activate for a Steam game

Under **Properties > General > Launch Options**, add:

```sh
VK_LAYER_PATH="$HOME/bc250-framecap/layer" VK_INSTANCE_LAYERS=VK_LAYER_FRAMECAP_observability %command%
```

Steam replaces `%command%` with the game command. This activates the layer for
that game, not globally. Remove the launch options to disable it.

## Update and rebuild

```sh
cd "$HOME/bc250-framecap" ;
cp framecap.toml framecap.bak.toml ;
git pull ;
cargo build --locked --release
```

# Going further

- [Documentation index](INDEX.md)
- [Installation and Steam setup](documentation/INSTALL.md)
- [Configuration and keyboard shortcuts](documentation/CONFIGURATION.md)
- [Troubleshooting and logs](documentation/TROUBLESHOOTING.md)
- [Known limitations and pending work](TODO.md)
- [Changelog](CHANGELOG.md)
