# PCB-CAM

**Cross-platform CAM workbench for PCB fabrication.**

PCB-CAM turns Gerber and Excellon fabrication data into CNC control programs. It is a native desktop application written in Rust with [egui](https://github.com/emilk/egui)/[eframe](https://github.com/emilk/egui/tree/main/crates/eframe), inspired by [FlatCAM](http://flatcam.org/).

![PCB-CAM with source tools, a machining tool library and a task timeline](docs/screenshot.png)

![G-code preview with a 3D toolpath, syntax-highlighted program and transport timeline](docs/preview.png)

## Project status

**Milestone 5: G-code preview and machine simulation — complete.**

The project keeps two deliberately separate tool concepts:

- **Source tools** are read-only apertures and drill tools found in Gerber and
  Excellon files. They describe the fabrication data. Excellon source tools are
  used to filter which holes/slots a task machines.
- **Machining tools** live in the user's own library: drills, end mills and
  V-bits with diameter, feed, plunge and spindle defaults. Every task references
  exactly one machining tool. The library is shared between projects and
  persisted in the application settings.

G-code is generated for one task or for all enabled tasks in timeline order and
opened in an interactive preview before anything is written to disk. Tool changes
(`T… M6`) are driven by the task machining tools, not by source drill diameters.
The preview replays the emitted program, so it reflects exactly what the export
will contain.

Implemented:

- Native application window on macOS, Windows and Linux (eframe + wgpu)
- Dark/light/system CAD workspace with browser, canvas, inspector and task timeline
- Gerber RS-274X import using [`gerber_parser`](https://crates.io/crates/gerber_parser) for the tokenizer, command AST and aperture macros:
  - lines, arcs, regions and flashes
  - circle/rectangle/obround/polygon apertures
  - parameterised aperture macros (for example KiCad `RoundRect`)
  - dark/clear polarity
  - units and coordinate-format conversion to millimetres
- Excellon import:
  - metric and inch headers
  - fixed-format and decimal coordinates
  - tool definitions, drill hits, `G85` slots and `M15`/`M16` routed slots
- Canvas layer rendering with fit-to-project, pan/zoom, grid, axes and scale bar
- Layer mirroring for bottom-side artwork:
  - flip the selected layer or all layers horizontally (mirror X), vertically (mirror Y), or both (180°)
  - each layer stores independent FlipH/FlipV flags; the mirror transform is derived from them
  - project flip axes are created on the first flip (board-outline centre, or project bounds centre), then stay fixed
  - the axes are drawn as thin dashed guides; drag them on the canvas or set exact positions from Layer → Set Flip Axes…
  - moving the axes re-applies existing layer flips; drilling, clearance and edge-cut tasks follow automatically
  - undoable
  - a selected clearance-boundary layer is used as a shadow copy with the source layer's flips, so it stays registered without changing the boundary layer itself
- Correct triangulation of concave Gerber regions (earcutr) instead of convex fan fill
- Circular-aperture traces with round joins and caps, matching Gerber stroke geometry
- Layer browser with visibility and selection
- **Source tool browser**:
  - Excellon tools with number, diameter and object count
  - Gerber aperture summaries with object counts
  - read-only; Excellon tools filter drilling, hole-milling and spot-drilling tasks
  - `+` on a source tool opens a prefilled machining-tool dialog with its diameter and suggested drill/end-mill type
- **User machining tool library**:
  - add, edit and remove drills, end mills, V-bits and cone/spot drills
  - diameter, feed, plunge and spindle defaults
  - persisted across restarts and shared by all projects
  - used by every task; a tool in use cannot be removed
- **Tasks** with type-specific settings:
  - **Drilling** — source-hole filter, drill bit, vertical plunge only, depth, retract, feed, plunge, dwell, spindle
  - **Hole milling** — source-hole/slot filter, end mill or V-bit, helical interpolation for holes larger than the tool, routed slots, depth, retract, feed, plunge, dwell, helical step-down, spindle
  - **Spot drilling** — source-hole filter, Cone machining tool, depth, retract, feed, plunge, dwell, spindle; uses hole centres only and ignores diameters/slots
  - **Edge cut** — machining tool, outside/inside/on-line offset (defaults to tool radius), depth, retract, feed, plunge, spindle
  - **Clearance** — isolation routing around unioned copper, visible-bounds or Gerber-boundary-layer clipping, outline offset, copper offset, pass count, offset step, depth, retract, feed, plunge, spindle
- Drilling tasks can be created even when the library has no exact cutter; the closest tool is selected and the inspector shows warnings derived from the selected holes/slots. G-code export is blocked while a task has unresolved warnings.
- Task timeline with selection, enable/disable, creation, reordering and online editing in the inspector
- Selected-task canvas overlay in red: drill/slot positions, spot-drill centres, edge-cut offset toolpath or clearance paths
- **Helical interpolation** for holes larger than the selected end mill/V-bit. A 1.0 mm end mill can machine a 1.5 mm hole with a helical path; a drill bit is rejected because it cannot side-cut.
- G-code move ordering:
  - drilling and spot drilling visit the nearest remaining hole
  - clearance keeps pass order and orders contours by the nearest entry vertex
- **G-code preview and export**:
  - generating a program opens the preview screen instead of writing the file directly
  - preview one selected task or all enabled tasks in timeline order
  - automatic `T… M6` changes between different machining tools
  - XY travel at the configured travel feed; safe-Z clearance from machining defaults
  - 3D toolpath in the centre with distinct colours for cutting (orange) and rapid/travel (blue), plus the current tool position and holder
  - orbit, pan, zoom, 3D/top/front presets, fit, grid and adjustable Z exaggeration
  - left program panel with scope, estimated cycle time, cutting/travel distance, task blocks (click to jump) and a tool list
  - right G-code panel with syntax highlighting, virtualized scrolling, line numbers and a move-kind gutter
  - current command highlighted and followed while playing or scrubbing
  - bottom transport timeline with play/pause, stop, single-command stepping, scrub, task markers and playback speed (0.25×–32×)
  - estimated time from programmed feeds, dwell, tool changes and spindle starts
  - export from the preview header or menu; the preview stays open after export
- Undo/redo for imports, layers and tasks
- Native file dialogs, drag-and-drop import, Gerber/Excellon ZIP archives and a bundled demo project
- Versioned `.pcbcam` project files with layers, tasks, G-code origin and a tool-library snapshot
- Recent-project menu and unsaved-changes confirmation for quit, new and open
- Settings window (Cmd/Ctrl+,) with appearance, theme (system/dark/light) and canvas object colours
- Machining defaults for travel feed/safe Z, drilling, milling, clearance and the default clearance tool
- Clearance previews are computed on a background worker so large pass counts do not block the UI
- Copper union is cached and shared with canvas rendering; pass buffers are computed in parallel batches and island containment is precomputed
- Generation stops as soon as copper fully covers the work region, instead of buffering degenerate empty passes
- GUI-independent core crate and a command-driven engine crate
- Cross-platform CI builds and a macOS application bundle script

### Example: drilling and hole milling

Suppose an Excellon file contains holes of 0.3, 1.0 and 1.5 mm, and the user
library contains a 0.3 mm drill and a 1.0 mm end mill:

1. Create a drilling task, select **Drill 0.30 mm** and check only source tool
   `T1` (the 0.3 mm holes).
2. Create a **hole-milling** task on the same layer, select **End mill 1.00 mm**
   and check the source tools for the 1.0 and 1.5 mm holes.
3. Preview all tasks. The combined program changes from the 0.3 mm drill to the
   1.0 mm end mill once, plunges the 1.0 mm holes and helically machines the
   1.5 mm holes. Export it from the preview once the 3D playback and highlighted
   commands look right.

This separation is the point: the source tool says **which objects belong to a
task**, while the machining tool says **what cutter performs the work**.

### Example: bottom-side artwork

Mirroring prepares artwork for machining the second side of a board:

1. Import the bottom copper, silkscreen and drill layers, plus the board
   outline. The outline gives the mirror a stable centre.
2. Choose **Layer → Flip All Layers → Horizontal (mirror X)** to mirror
   left/right, **Vertical (mirror Y)** to mirror top/bottom, or **Both (180°)**
   when the board is turned over completely. A single layer can be flipped from
   the same menu or from the inspector.
3. Check the canvas: drills, clearance paths and edge-cut toolpaths are
   regenerated from the mirrored source geometry, so they follow automatically.
   **Undo** restores the original side.

Without an outline layer the project bounds centre is used for the initial
axes. The axes are captured on the first flip and afterwards can be dragged on
the canvas or moved from **Layer → Set Flip Axes…**. Automation clients can use
the `project.set_flip_axes` command to set them explicitly; moving the axes
re-applies existing flips. The G-code zero point is not mirrored; set it after
flipping when the origin must follow the new side.

Next:

- Use Gerber source tools (apertures) to filter cut-out and clearance objects
- Edge cut tabs and multi-pass depth stepping
- Machine/post-processor profiles and tool wear data
- Acceleration-aware cycle-time estimates and a machine envelope check
- Preview overlays for the source Gerber and Excellon layers

### Known limitations

- Import geometry is flattened to millimetres at load time; the source path is kept on the layer.
- Gerber step-and-repeat, aperture blocks and negative image polarity are not fully supported yet; import reports warnings instead of failing.
- Gerber source apertures are listed, but currently only **drilling, hole-milling and spot-drilling tasks** use source tools as filters. Edge-cut and clearance tasks process the whole layer.
- Gerber regions are triangulated; regions with multiple nested contours/islands may not be fully supported yet.
- Contour offsets use mitred joins without a full self-intersection cleanup.
- Clearance follows isolation routing: all copper primitives are unioned first, then buffered by the explicit copper offset for each pass, bounded by a clearance boundary. By default the boundary is the bounding box of visible layers; any Gerber layer can be selected instead. Its largest closed contour is used, or its bounds when it has no closed contour. With a boundary, the work region is the boundary inset by Outline offset. Each pass subtracts the copper clearance from the work region, emits the holes and nested island contours of the remaining region, and adds the outer contour when copper reaches the boundary; work region is stepped inward by Offset step for the next pass. New tasks default to: outline offset = tool radius, copper offset = tool radius, offset step = tool diameter. Tool width is not used by the path builder; the explicit offsets control the geometry. Traces ending inside pads therefore do not produce separate circles inside the pad; Outside expands the copper, Inside cuts into it.
- Slots narrower than the machining tool are rejected; wider slots are routed on the centreline, which may not clear the full slot width.
- Project files are JSON (`.pcbcam`, version 1). Loading merges missing machining tools into the shared library; conflicting tool definitions are cloned under a new id and task references are remapped. Recent files are stored in the application settings; quit/new/open prompt to save pending changes.
- G-code is emitted in millimetres with explicit drill/plunge moves. Coordinates are relative to the selected G-code 0-point; machine-specific headers and cutter compensation are not yet supported. XY travel uses `G01` with the configured travel feed rather than controller-defined `G00` rapid.
- The preview replays the generated subset (`G00`/`G01`/`G02`/`G03`, `G04`, `M3`/`M5`/`M6` and `F`/`S`/`T` words) at programmed feed rates from a safe-Z start. Tool changes and spindle starts use fixed simulated durations; acceleration, look-ahead and controller-specific timing are not modelled yet.

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
cargo run -p pcb-cam

# Start with the bundled demo board (4 layers, 4 tasks)
cargo run -p pcb-cam -- --sample

# Open fabrication files directly
cargo run -p pcb-cam -- path/to/board-F_Cu.gbr path/to/board.drl

# Or unpack/import every Gerber + drill file in a ZIP
cargo run -p pcb-cam -- path/to/gerber-drill.zip

# Optimized binary
cargo build --release --locked -p pcb-cam
./target/release/pcb-cam
```

The binary is `target/release/pcb-cam` (`pcb-cam.exe` on Windows).

### macOS application bundle

```bash
./scripts/package-macos.sh
open dist/PCB-CAM.app
```

The script builds a release binary, assembles `PCB-CAM.app` with the app icon and `Info.plist`, ad-hoc signs it and creates `dist/PCB-CAM-macos.zip`.

## Working with tools and tasks

1. Import with **File → Import…** (`Cmd/Ctrl+Shift+O`), the **Import** toolbar
   button, or by dropping files on the window. The importer detects Gerber,
   Excellon and ZIP archives from the content and extension automatically.
   ZIP archives import every Gerber/Excellon entry; README and order-instruction
   files are skipped.
2. Select a layer to inspect its **Source tools** in the browser and inspector.
   These are read-only and come from the fabrication file.
3. Open **Machining tools → + Machining tool** to add your own cutter, or select
   a library tool and use **Edit tool…** in the inspector. The library is saved
   automatically in the application settings.
4. Add a task from **+ Task** in the toolbar/timeline or from the browser when a
   suitable layer is selected:
   - **Drilling** needs an Excellon layer and a matching drill bit; it only plunges vertically.
   - **Hole milling** needs an Excellon layer and an end mill/V-bit; it can helically interpolate larger holes and route slots.
   - **Spot drill** needs an Excellon layer with holes; it punches only hole centres with a Cone tool.
   - **Edge cut** needs a Gerber layer with closed contours. KiCad-style Edge.Cuts files made of separate line and arc strokes are joined automatically.
   - **Clearance** needs a Gerber layer with paths.
5. Select the task. Edit it directly in the inspector: choose one
   **machining tool**, check the **source tools** for drilling/hole milling/spot drilling, and change
   depths/feeds. Changes are applied online; there is no separate task-edit dialog.
   While selected, the task is drawn in red on the canvas — drilling positions
   for drill tasks, or the offset/clearance trajectory for milling tasks.
6. Review machining defaults under **View → Settings… → Machining**:
   travel feed, safe Z, drill plunge/depth, mill feed/plunge/depth, spot
   depth, helical step-down, spindle and the default clearance tool. They
   seed new tasks; travel feed and safe Z also drive exported G-code.
7. Set the G-code zero point with **0-point** in the toolbar. Enter absolute
   X/Y values, drag the `G0` marker on the canvas, or use **Pick on canvas**.
   Double-clicking the marker reopens the value dialog. All exported
   coordinates are written relative to that point.
8. **File → Save Project** (or **Save Project As…**) stores a `.pcbcam` file;
   **File → Open Project…** restores layers, tasks, the G-code origin and
   referenced machining tools.
9. Preview and export:
   - **File → Preview Selected Task G-code…** or the inspector's **Preview task G-code…** for one task.
   - **File → Preview All Tasks G-code…** or the timeline's **Preview all G-code** for the combined program with tool changes.
   - The preview shows the 3D toolpath, program tasks and tools, the highlighted G-code and a scrub timeline. Use **Space** to play/pause, **←/→** to step commands, **Shift+←/→** to step ten, **Home/End** for the start/end and the speed selector for playback rate.
   - Export with **Export G-code…** in the preview header (or `Cmd/Ctrl+E`). **Esc** returns to the workspace.

The demo project contains top copper, a rounded board outline with arcs, a
silkscreen legend and a drill file. It is set up as four tasks: a drilling task for the 0.8 mm holes, a hole-milling
task for the 1.0/3.0 mm holes, an edge cut and a clearance pass.

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
│   ├── pcb-cam/            # Desktop application (eframe/egui)
│   │   └── src/
│   │       ├── app.rs      # Application state, commands, editors
│   │       ├── preview.rs  # Preview playback clock, 3D camera, preview state
│   │       ├── theme.rs    # PCB-CAM palette and egui style
│   │       ├── view.rs     # Canvas camera and unit formatting
│   │       └── ui/         # Menu, panels, 3D preview, G-code panel, timeline
│   ├── pcb-cam-engine/     # Session, command registry, undo/redo
│   └── pcb-cam-core/       # GUI-independent domain and import
│       └── src/
│           ├── formats/    # Gerber and Excellon importers
│           ├── geometry.rs # Points, paths, apertures, offset contours
│           ├── gcode.rs    # Task and combined G-code post-processor
│           ├── preview.rs  # Timed G-code parser for playback and highlighting
│           ├── layer.rs    # Source layers, colours, source-tool metadata
│           ├── operation.rs # Task timeline (Task aliases)
│           ├── project.rs  # Project and tasks
│           ├── tool.rs     # Machining tools and user tool library
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
pcb-cam-core  ←  pcb-cam-engine  ←  pcb-cam
(domain/import)   (session/commands)   (egui front end)
```

- **core** owns the project model, source geometry, source-tool metadata, the
  machining tool library type, contour offsets, G-code generation and the timed
  preview parser. It has no GUI dependencies.
- **engine** owns a `Session`, executes JSON commands such as `file.import_gerber`,
  `task.add_cutout`, `tool.create`, `file.export_task_gcode` and `edit.undo`, and
  keeps project undo/redo snapshots. The machining library belongs to the session
  and is intentionally not project-undoable.
- **pcb-cam** is a thin egui front end. It calls commands instead of mutating the
  project directly, so future CLI and automation clients can reuse the same registry.

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| `Cmd/Ctrl+,` | Settings |
| `Cmd/Ctrl+N` | New project |
| `Cmd/Ctrl+O` | Open project |
| `Cmd/Ctrl+S` | Save project |
| `Cmd/Ctrl+Shift+S` | Save project as |
| `Cmd/Ctrl+Shift+O` | Import Gerber/Excellon/ZIP |
| `Cmd/Ctrl+E` | Preview selected task G-code (export it from the preview) |
| `Cmd/Ctrl+Shift+E` | Preview all tasks G-code |
| `Cmd/Ctrl+Z` | Undo |
| `Cmd/Ctrl+Shift+Z` | Redo |
| `Delete` | Remove selected layer, tool or task |
| `F` or double-click | Fit project into the canvas |
| `G` | Toggle grid |
| `F1` | About |
| `Esc` | Close dialog / return from the G-code preview |

### G-code preview

| Shortcut | Action |
| --- | --- |
| `Space` | Play or pause playback |
| `←` / `→` | Step one command; hold `Shift` to step ten |
| `Home` / `End` | Jump to the start or end of the program |
| `+` / `-` | Increase or decrease playback speed |
| `F` | Fit the toolpath into the 3D view |
| `Cmd/Ctrl+E` | Export the previewed program |
| `Cmd/Ctrl+R` | Regenerate the preview |
| `Esc` | Return to the workspace |

Scroll to zoom around the pointer; drag to orbit the 3D view; `Shift`+drag to pan.
Double-click the 3D view to fit the toolpath.

## Icons

The application icon is checked in as PNG, ICNS and ICO. To regenerate it:

```bash
python3 -m pip install pillow
python3 scripts/gen_icons.py
```

The editable source is [`assets/icon.svg`](assets/icon.svg). On macOS the generator also produces the `.icns` via `iconutil`.

## License

PCB-CAM is released under the [MIT License](LICENSE).

Copyright © 2026 andryblack

Gerber import uses the `gerber_parser` crate by the MakerPnP project, licensed under MIT OR Apache-2.0.
