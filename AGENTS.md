# AGENTS.md

Repository instructions for coding agents. Read this before changing PCB-CAM.
The public, human-facing documentation is [README.md](README.md); keep it
accurate but do not turn it into an agent notebook.

## Project

PCB-CAM is a native Rust desktop CAM application (edition 2024, MSRV 1.95,
egui/eframe) that turns Gerber and Excellon fabrication data into G-code. The
current release line is 0.1.0 (Milestone 5).

## Commands

```bash
# Run
cargo run -p pcb-cam
cargo run -p pcb-cam -- --sample          # bundled demo; --demo is an alias
cargo run -p pcb-cam -- board.gbr board.drl
cargo run -p pcb-cam -- gerbers.zip

# Release build
cargo build --release --locked -p pcb-cam
```

### Required checks

Run all three before finishing any code change; CI uses the same commands and
treats clippy warnings as errors.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Useful narrower runs:

```bash
cargo test -p pcb-cam-core
cargo test -p pcb-cam-engine
cargo test -p pcb-cam-engine --test samples   # end-to-end demo smoke test
```

### Packaging and assets

```bash
./scripts/package-macos.sh       # macOS only; writes dist/PCB-CAM.app and zip
python3 scripts/gen_icons.py     # needs Pillow; rewrites assets/icon.*
```

### Licenses

```bash
cargo install --locked cargo-about --version 0.9.3 --features cli
./scripts/gen-licenses.sh          # rewrite THIRD_PARTY_LICENSES.md
./scripts/gen-licenses.sh --check  # CI verifies the committed file
```

- `THIRD_PARTY_LICENSES.md` is generated from `Cargo.lock`; do not edit it by
  hand. Font-specific notices are maintained in `third-party/fonts.md` and are
  appended by the script.
- `about.toml` is the license allow-list. Do not add a dependency whose only
  option is GPL, LGPL, AGPL, MPL or another copyleft license without an explicit
  decision; dual-licensed crates may use their permissive option.
- Regenerate the notices whenever `Cargo.lock` changes, and commit the result.
- Release artifacts must contain both `LICENSE` and `THIRD_PARTY_LICENSES.md`.
  CI stages them next to the binaries, and the macOS bundle stores them in
  `Contents/Resources`.
- The About window embeds the notices via `include_str!` and exposes them from
  **Help → Third-party licenses**. Rebuild after regenerating the file; no
  separate UI update is needed.

## Architecture

```text
pcb-cam-core  ←  pcb-cam-engine  ←  pcb-cam
(domain/import)   (session/commands)   (egui front end)
```

| Crate | Owns | Rules |
| --- | --- | --- |
| `pcb-cam-core` | Project model, Gerber/Excellon import, geometry, toolpaths, G-code, `.pcbcam` persistence, timed preview parser | No GUI dependencies. Do not add egui/eframe. |
| `pcb-cam-engine` | `Session`, JSON command registry, command handlers, undo/redo | Model changes and user-visible actions belong here. |
| `pcb-cam` | eframe application, canvas, panels, dialogs, 3D preview | Thin front end over engine commands. |

- **Every user-visible action should be a command.** Add a `CommandSpec` in
  `crates/pcb-cam-engine/src/command.rs`, implement the handler in
  `commands.rs`, then call it from the UI through
  `PcbCam::run_command`. Mark project-mutating commands `undoable: true`;
  tool-library and export commands stay non-undoable.
- **Do not mutate the project directly from UI code.** The UI reads
  `session.project()` and changes state through commands. The machining tool
  library belongs to the `Session` and is intentionally not project-undoable.
- **Keep command ids and serialized field names stable.** They are the
  automation API and the `.pcbcam` format. Renaming requires a compatibility
  path; `PROJECT_VERSION` is currently `1`.
- When adding a persisted task field, give it a serde default (for example
  `#[serde(default = "...")]`) and update the add/update commands, the
  inspector, timeline summaries and tests. Do not bump the project version
  without a migration.
- Internal names are not always UI names: **Clearance** is
  `EngraveOperation`/`task.add_engrave`, and **Edge cut** is
  `CutoutOperation`/`task.add_cutout`. Keep the UI labels and serialized ids as
  they are.
- Coordinates and dimensions are millimetres. Task depths and stepdowns are
  positive; the G-code post-processor writes negative Z values.
- If G-code emission gains new words or move kinds, update the preview parser
  (`pcb-cam-core/src/preview.rs`) and its tests as well.
- Prefer existing patterns over new abstractions. Add dependencies in the root
  `Cargo.toml` workspace table unless they are crate-specific.

## Testing

- Put unit tests next to the logic with `#[cfg(test)]`, especially for geometry,
  G-code generation, commands and persistence.
- The demo project is a shipped fixture: `crates/pcb-cam-engine/tests/samples.rs`
  exercises import, drilling, hole milling, edge cutting, clearance, tool
  changes and export. Spot drilling is covered by command unit tests. Keep the
  suite green.
- Loading older `.pcbcam` files must stay backward compatible; extend the
  persistence tests when the schema changes.
- No `unsafe`: all crates forbid it.

## Documentation

- [README.md](README.md) is the public entry point. Update it when commands,
  user-facing behaviour or known limitations change.
- Version comes from `workspace.package.version`; the milestone label also
  appears in `crates/pcb-cam/src/ui/menu.rs`, the About dialog and the README
  status. Keep them in sync on release.
- Keep `docs/` limited to images referenced by documentation. Currently that is
  only `docs/preview.png`; do not add unreferenced screenshots or stale assets.
- Keep `assets/samples/README.md` in sync when the demo files or demo tasks
  change.
- Code comments should explain contracts and non-obvious constraints, not
  restate the code.

## Workflow

- Do not create commits, rewrite history or push unless explicitly asked. Leave
  changes in the working tree for review.
- Do not commit build output (`target/`, `dist/`) or local editor files.
- Before finishing: run the required checks, review the diff, and update the
  docs affected by the change.
