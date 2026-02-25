# Talking Head

A Loom-style webcam bubble overlay for Wayland Linux. Shows your face in a draggable circle that floats above all windows — perfect for screen recordings, presentations, and live demos.

![Wayland](https://img.shields.io/badge/Wayland-only-blue)
![License](https://img.shields.io/badge/license-MPL--2.0-orange)

## Features

- **Circular webcam overlay** — floats above all windows using Wayland layer-shell
- **Draggable** — click and drag to reposition anywhere on screen
- **System tray integration** — left-click to toggle, right-click for camera selection
- **CLI control** — start, stop, toggle, resize from terminal or scripts
- **Camera selection** — switch between webcams via tray menu or CLI
- **Position & camera memory** — remembers your last position and camera between restarts
- **Auto resolution** — captures at optimal resolution (capped at 720p for performance)
- **Mirror mode** — horizontally flipped so it looks natural, like a real mirror
- **Click-through** — clicks outside the circle pass through to windows below
- **Single binary** — one Rust binary, daemon + CLI client, no runtime config files

## Requirements

- Wayland compositor with layer-shell support (Niri, Sway, Hyprland, etc.)
- GTK 4.12+
- GStreamer 1.20+ with `good` and `base` plugin sets
- A system tray that supports StatusNotifierItem (e.g., Waybar, AGS, DankMaterialShell)

### System dependencies (Fedora/RHEL)

```sh
sudo dnf install gtk4-devel gstreamer1-devel gstreamer1-plugins-base-devel \
  gstreamer1-plugins-good gtk4-layer-shell-devel
```

### System dependencies (Debian/Ubuntu)

```sh
sudo apt install libgtk-4-dev libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
  gstreamer1.0-plugins-good libgtk4-layer-shell-dev
```

### System dependencies (Arch)

```sh
sudo pacman -S gtk4 gstreamer gst-plugins-base gst-plugins-good gtk4-layer-shell
```

### System dependencies (NixOS)

Add to your environment or use a dev shell with:
`gtk4`, `gstreamer`, `gst-plugins-base`, `gst-plugins-good`, `gtk4-layer-shell`

## Installation

### From source (recommended)

```sh
cargo install --git https://github.com/licht1stein/talking-head
talking-head install
```

The `install` subcommand copies the binary, icon, and `.desktop` file into `~/.local/`:

```
~/.local/bin/talking-head
~/.local/share/icons/hicolor/512x512/apps/talking-head.png
~/.local/share/applications/talking-head.desktop
```

You may need to log out and back in for your desktop environment to pick up the new launcher entry.

### Build locally

```sh
git clone https://github.com/licht1stein/talking-head.git
cd talking-head
cargo build --release
./target/release/talking-head --help
```

## Usage

### Quick start

```sh
talking-head start           # start the daemon (overlay appears)
talking-head stop            # stop the daemon
talking-head toggle          # show/hide the overlay
```

### CLI reference

```
talking-head <COMMAND>

Commands:
  start    Start the daemon and show the camera overlay
  stop     Stop the daemon
  toggle   Toggle camera overlay visibility
  status   Print current daemon status as JSON
  size     Set the size of the camera overlay
  devices  List available webcam devices as JSON
  select   Open the webcam selection dialog
  install  Install to ~/.local (binary, icon, desktop entry)
```

#### `start`

```sh
talking-head start                        # default: medium size (200px), first camera
talking-head start -d /dev/video2         # specific camera
talking-head start -s large               # large size (300px)
talking-head start -s 250                 # custom size in pixels
talking-head start -f                     # foreground mode (don't daemonize)
```

Options:

- `-d, --device <PATH>` — webcam device path (default: last used or first available)
- `-s, --size <VALUE>` — `small` (128px), `medium` (200px), `large` (300px), or a pixel count
- `-f, --foreground` — run in foreground instead of daemonizing

#### `size`

```sh
talking-head size small     # 128px
talking-head size medium    # 200px
talking-head size large     # 300px
talking-head size 250       # 250px
```

#### `devices`

```sh
talking-head devices
```

Outputs a JSON array of available webcams:

```json
[
  { "name": "Laptop Webcam", "path": "/dev/video0" },
  { "name": "MX Brio", "path": "/dev/video3" }
]
```

#### `status`

```sh
talking-head status
```

Outputs JSON with the current daemon state (running, visibility, device, size, position).

### System tray

- **Left-click** — toggle overlay visibility
- **Right-click** — context menu:
  - Toggle Camera
  - Camera submenu (lists available webcams with resolution)
  - Quit

### Scripting examples

Toggle from a keybinding (e.g., in Sway/Hyprland config):

```
bindsym $mod+c exec talking-head toggle
```

Start on login (e.g., in your compositor's autostart):

```
exec talking-head start
```

## Configuration

Talking Head stores minimal state in `~/.config/talking-head/`:

- `device` — last used camera device path
- `position` — last overlay position (`right,top` margins in pixels)

There is no configuration file. All settings are controlled via CLI flags.

## How it works

- **GStreamer** captures from `v4l2src`, crops to square with `aspectratiocrop`, flips horizontally, scales to the overlay size, and delivers BGRA frames via `appsink`
- **GTK 4** renders frames into a `DrawingArea` with a Cairo circular clip path
- **gtk4-layer-shell** places the window in the `Top` layer with click-through outside the circle
- **ksni** registers a StatusNotifierItem for the system tray
- **Unix socket IPC** (`$XDG_RUNTIME_DIR/talking-head.sock`) connects CLI commands to the running daemon

## License

[Mozilla Public License 2.0](LICENSE)
