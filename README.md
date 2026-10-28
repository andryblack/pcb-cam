# PBC-CAM

**Cross-platform CAM workbench for PCB fabrication.**

PBC-CAM turns Gerber and Excellon fabrication data into CNC control programs. It is a native desktop application written in Rust with [egui](https://github.com/emilk/egui)/[eframe](https://github.com/emilk/egui/tree/main/crates/eframe), inspired by [FlatCAM](http://flatcam.org/).

![PBC-CAM application window](docs/screenshot.png)

## Project status

**Milestone 1: application shell — complete.**

The native window, CAD workspace, project model and interactive canvas are in place. Gerber/Excellon import and toolpath generation are the next milestones.

Implemented:

- Native window on macOS, Windows and Linux (eframe + wgpu)
- Dark CAD workspace with project, layer and inspector panels
- Interactive millimetre/inch grid with pan and zoom
- Menu bar, toolbar, status bar and about/shortcuts dialogs
- GUI-independent core crate with project, layer and unit model
- Application icon, MIT license, CI builds for all three desktop platforms

Next:

- Gerber RS-274X / X2 importer
- Excellon drill and rout importer
- Board geometry, selection and measurements
- Isolation routing, drilling and contour toolpaths
- G-code post-processor and export
- Project save/load and recent files

## Requirements

- Rust stable **1.95+** (the repository contains a `rust-toolchain.toml`)
- A GPU and graphics driver supported by `wgpu`
- Linux system libraries:

```bash
sudo apt-get install -y \
  libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev \
  libx11-dev libx11-xcb-dev libxcb1-dev libxcb-render0-dev \
  libxcb-shape0-dev libxcb-xfixes0-dev libxrandr-dev libxi-dev \
  libxcursor-dev libegl1-mesa-dev libgl1-mesa-dev libvulkan-dev
```

## Build and run

```bash
# Development build and run
cargo run -p pbc-cam

# Optimized binary
cargo build --release --locked -p pbc-cam
./target/release/pbc-cam
```

The binary is `target/release/pbc-cam` (`pbc-cam.exe` on Windows).

### macOS application bundle

```bash
./scripts/package-macos.sh
open dist/PBC-CAM.app
```

The script builds a release binary, assembles `PBC-CAM.app` with the app icon and `Info.plist`, ad-hoc signs it and creates `dist/PBC-CAM-macos.zip`.

### CI

GitHub Actions runs formatting, clippy and tests on every push/PR, and builds release binaries for Linux, Windows and macOS. Build artifacts are available from the workflow run.

## Quality checks

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

## Project layout

```text
.
├── crates/
│   ├── pbc-cam/            # Desktop application (eframe/egui)
│   │   └── src/
│   │       ├── app.rs      # Application state and root layout
│   │       ├── theme.rs    # PBC-CAM palette and egui style
│   │       ├── view.rs     # Canvas camera and unit formatting
│   │       └── ui/         # Menu, toolbar, panels, viewport, status bar
│   └── pbc-cam-core/       # GUI-independent domain model
│       └── src/
│           ├── layer.rs    # Layers and source formats
│           ├── project.rs  # Project container
│           └── units.rs    # Millimetres/inches
├── assets/                 # App icon (PNG/ICNS/ICO/SVG)
├── docs/                   # Screenshots and documentation assets
├── scripts/                # Icon generation and macOS packaging
└── .github/workflows/      # CI
```

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| `Cmd/Ctrl+N` | New project |
| `Cmd/Ctrl+Q` | Quit |
| `F` or double-click | Reset canvas view |
| `G` | Toggle grid |
| `F1` | About |
| `Esc` | Close dialog |

Scroll to zoom around the pointer; drag to pan.

## Icons

The application icon is checked in as PNG, ICNS and ICO. To regenerate it:

```bash
python3 -m pip install pillow
python3 scripts/gen_icons.py
```

The editable source is [`assets/icon.svg`](assets/icon.svg). On macOS the generator also produces the `.icns` via `iconutil`.

## License

PBC-CAM is released under the [MIT License](LICENSE).

Copyright © 2026 andryblack
