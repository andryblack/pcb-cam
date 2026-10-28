# PBC-CAM

**Cross-platform CAM workbench for PCB fabrication.**

PBC-CAM turns Gerber and Excellon fabrication data into CNC control programs. It is a native desktop application written in Rust with [egui](https://github.com/emilk/egui)/[eframe](https://github.com/emilk/egui/tree/main/crates/eframe), inspired by [FlatCAM](http://flatcam.org/).

![PBC-CAM application window with four fabrication layers and three tasks](docs/screenshot.png)

## Project status

**Milestone 3: task timeline and G-code — complete.**

A project is built from source layers and an ordered list of **tasks**. Each task
has its own type-specific settings. A CNC program can be generated from one
selected task or from all enabled tasks in order, with automatic tool changes
(`M6`) between different cutter diameters.

Implemented:

- Native application window on macOS, Windows and Linux (eframe + wgpu)
- Dark CAD workspace with browser, canvas, inspector and task timeline
- Gerber RS-274X import using [`gerber_parser`](https://crates.io/crates/gerber_parser) for the tokenizer, command AST and aperture macros:
  - lines, arcs, regions and flashes
  - standard circle/rectangle/obround/polygon apertures
  - parameterised aperture macros (for example KiCad `RoundRect`)
  - dark/clear polarity
  - units and coordinate-format conversion to millimetres
- Excellon import:
  - metric and inch headers
  - fixed-format and decimal coordinates
  - tool definitions, drill hits and `G85` slots
- Canvas layer rendering with fit-to-project, pan/zoom, grid, axes and scale bar
- Layer browser with visibility and selection; automatic drill tool table
- **Tasks** with type-specific settings:
  - **Drilling** — all holes and slots from an Excellon layer, depth, retract, feed, plunge, dwell, spindle
  - **Contour cut** — follows closed Gerber contours with an outside/inside/on-line cutter offset, tool diameter, depth and feeds
  - **Engraving** — follows Gerber strokes, region outlines and flashed-pad outlines with a small tool, depth and feeds
- Task timeline with selection, enable/disable, creation from the browser, toolbar, menu or timeline, and a type-specific parameter editor
- G-code export:
  - **selected task** only, for checking or running one operation
  - **all enabled tasks** as one program, in timeline order, with `T… M6` tool changes and shared tool numbering by diameter
- Undo/redo for imports, layers, tools and tasks
- Native file dialogs, drag-and-drop import and a bundled demo project
- GUI-independent core crate and a command-driven engine crate
- Cross-platform CI builds and a macOS application bundle script

Next:

- Engraving area fill / hatch patterns in addition to contour engraving
- Contour cut tabs and multi-pass depth stepping
- Isolation routing around copper
- Project save/load (`.pbcam` or JSON)
- G-code preview and machine simulation
- Tool-table editing and machine/post-processor profiles
- A larger Gerber/Excellon compatibility corpus and reference tests

### Known limitations

- Import geometry is flattened to millimetres at load time; the source path is kept on the layer.
- Gerber step-and-repeat, aperture blocks and negative image polarity are not fully supported yet; import reports warnings instead of failing.
- Regions are drawn as polygons; complex holes cut through a region are not boolean-subtracted.
- Contour offsets use mitred joins without a full self-intersection cleanup. Simple board outlines work; very concave contours may need a proper offset kernel.
- Engraving follows outlines and paths; filled/hardened area clearing is not implemented yet.
- Projects are not saved yet; generated G-code is currently the durable output.
- G-code is emitted in millimetres with explicit drill/plunge moves. It does not yet include machine-specific headers, tool-change preferences, work offsets or cutter compensation.

## Requirements

- Rust stable **1.95+** (see `rust-toolchain.toml`)
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

# Start with the bundled demo board (4 layers, 3 tasks)
cargo run -p pbc-cam -- --sample

# Open fabrication files directly
cargo run -p pbc-cam -- path/to/board-F_Cu.gbr path/to/board.drl

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

## Working with tasks

1. Import Gerber layers with **File → Import Gerber…** or the **Gerber** toolbar button.
2. Import an Excellon file with **File → Import Excellon…** or the **Excellon** button. Drill tools are added to the browser automatically.
3. Add a task from **+ Task** in the toolbar/timeline or from the browser when a suitable layer is selected:
   - **Drilling** needs an Excellon layer.
   - **Contour cut** needs a Gerber layer with closed contours.
   - **Engraving** needs a Gerber layer with paths.
4. Select the task and edit its parameters in the inspector with **Edit task…**. Each task type exposes only its own settings.
5. Export:
   - **File → Export Selected Task G-code…** or the inspector's **Export task G-code…** for one task.
   - **File → Export All Tasks G-code…** or the timeline's **Export all G-code** for the combined program with tool changes.

The demo project contains top copper, a rounded board outline with arcs, a
silkscreen legend and a drill file, set up as three tasks: drill, contour cut and
engrave.

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
│   │       ├── app.rs      # Application state, commands, task editor
│   │       ├── theme.rs    # PBC-CAM palette and egui style
│   │       ├── view.rs     # Canvas camera and unit formatting
│   │       └── ui/         # Menu, toolbar, browser, timeline, viewport
│   ├── pbc-cam-engine/     # Session, command registry, undo/redo
│   └── pbc-cam-core/       # GUI-independent domain and import
│       └── src/
│           ├── formats/    # Gerber and Excellon importers
│           ├── geometry.rs # Points, paths, apertures, offset contours
│           ├── gcode.rs    # Task and combined G-code post-processor
│           ├── layer.rs    # Source layers and colours
│           ├── operation.rs # Task timeline (Task aliases)
│           ├── project.rs  # Project, tools and tasks
│           └── units.rs    # Millimetres/inches
├── assets/
│   ├── samples/            # Synthetic demo Gerber/Excellon project
│   └── icon.*              # App icon (PNG/ICNS/ICO/SVG)
├── docs/                   # Screenshots
├── scripts/                # Icon generation and macOS packaging
└── .github/workflows/      # CI
```

## Architecture

The code follows a small version of the command-oriented layering used by mature
Rust CAD tools:

```text
pbc-cam-core  ←  pbc-cam-engine  ←  pbc-cam
(domain/import)   (session/commands)   (egui front end)
```

- **core** owns the project model, geometry, importers, task types, contour offsets and G-code generation and has no GUI dependencies.
- **engine** owns a `Session`, executes JSON commands such as `file.import_gerber`,
  `task.add_cutout`, `file.export_task_gcode` and `edit.undo`, and keeps undo/redo snapshots.
- **pbc-cam** is a thin egui front end. It calls commands instead of mutating the
  project directly, so future CLI and automation clients can reuse the same registry.

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| `Cmd/Ctrl+N` | New project |
| `Cmd/Ctrl+O` | Import Gerber |
| `Cmd/Ctrl+Shift+O` | Import Excellon |
| `Cmd/Ctrl+E` | Export selected task G-code |
| `Cmd/Ctrl+Shift+E` | Export all tasks G-code |
| `Cmd/Ctrl+Z` | Undo |
| `Cmd/Ctrl+Shift+Z` | Redo |
| `Delete` | Remove selected layer, tool or task |
| `F` or double-click | Fit project into the canvas |
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

Gerber import uses the `gerber_parser` crate by the MakerPnP project, licensed under MIT OR Apache-2.0.
