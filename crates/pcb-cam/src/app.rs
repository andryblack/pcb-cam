//! Application state and top-level egui layout.

use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui::{self, Align2, Button, Key, Modifiers, RichText, Stroke, Vec2, vec2};
use pcb_cam_core::{
    ContourSide, EngraveSide, GerberRegion, LayerGeometry, LayerId, OperationId, OperationKind,
    Path as CamPath, Point, Project, SourceFormat, SourceToolInfo, Task, Tool, ToolId, ToolKind,
    ToolLibrary, Unit,
};
use pcb_cam_engine::{Session, find_command};
use serde_json::{Value, json};

use crate::theme;
use crate::ui;
use crate::ui::widgets;
use crate::view::ViewState;

/// Product name used in the title bar and dialogs.
pub(crate) const APP_NAME: &str = "PCB-CAM";
/// Window title.
pub(crate) const WINDOW_TITLE: &str = "PCB-CAM — PCB Fabrication CAM";
/// Application version.
pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Shortcut labels shown in menus.
pub(crate) const SHORTCUT_NEW: &str = if cfg!(target_os = "macos") {
    "Cmd+N"
} else {
    "Ctrl+N"
};
pub(crate) const SHORTCUT_OPEN: &str = if cfg!(target_os = "macos") {
    "Cmd+O"
} else {
    "Ctrl+O"
};
pub(crate) const SHORTCUT_QUIT: &str = if cfg!(target_os = "macos") {
    "Cmd+Q"
} else {
    "Ctrl+Q"
};
pub(crate) const SHORTCUT_UNDO: &str = if cfg!(target_os = "macos") {
    "Cmd+Z"
} else {
    "Ctrl+Z"
};
pub(crate) const SHORTCUT_REDO: &str = if cfg!(target_os = "macos") {
    "Cmd+Shift+Z"
} else {
    "Ctrl+Shift+Z"
};

/// The active object in the browser/inspector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Selection {
    Layer(LayerId),
    Tool(ToolId),
    Task(OperationId),
}

/// Toggles that control the workspace chrome.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ViewSettings {
    pub(crate) show_grid: bool,
    pub(crate) show_origin: bool,
    pub(crate) show_project_panel: bool,
    pub(crate) show_inspector: bool,
    pub(crate) show_timeline: bool,
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self {
            show_grid: true,
            show_origin: true,
            show_project_panel: true,
            show_inspector: true,
            show_timeline: true,
        }
    }
}

/// Editable task state used by the inspector for online editing.
#[derive(Debug, Clone)]
pub(crate) struct TaskDraft {
    pub(crate) id: OperationId,
    pub(crate) name: String,
    pub(crate) tool: ToolId,
    pub(crate) kind: TaskDraftKind,
}

/// Type-specific editable task fields.
#[derive(Debug, Clone)]
pub(crate) enum TaskDraftKind {
    Drill {
        source_tools: BTreeSet<u32>,
        depth: f64,
        retract: f64,
        feed: f64,
        plunge: f64,
        dwell: f64,
        stepdown: f64,
        spindle: u32,
    },
    SpotDrill {
        source_tools: BTreeSet<u32>,
        depth: f64,
        retract: f64,
        feed: f64,
        plunge: f64,
        dwell: f64,
        spindle: u32,
    },
    HoleMill {
        source_tools: BTreeSet<u32>,
        depth: f64,
        retract: f64,
        feed: f64,
        plunge: f64,
        dwell: f64,
        stepdown: f64,
        spindle: u32,
    },
    Cutout {
        side: ContourSide,
        offset: f64,
        depth: f64,
        retract: f64,
        feed: f64,
        plunge: f64,
        spindle: u32,
    },
    Engrave {
        side: EngraveSide,
        outline_offset: f64,
        offset: f64,
        passes: u32,
        stepover: f64,
        depth: f64,
        retract: f64,
        feed: f64,
        plunge: f64,
        spindle: u32,
        boundary: Option<LayerId>,
    },
}

impl TaskDraft {
    fn from_task(project: &Project, task: &Task) -> Option<Self> {
        let kind = match task.kind() {
            OperationKind::Drill(drill) => {
                let mut source_tools = drill.source_tools.clone();
                if source_tools.is_empty() {
                    for tool in project.layer(drill.layer)?.source_tools() {
                        if let Some(number) = tool.number {
                            source_tools.insert(number);
                        }
                    }
                }
                TaskDraftKind::Drill {
                    source_tools,
                    depth: drill.depth_mm,
                    retract: drill.retract_mm,
                    feed: drill.feed_mm_min,
                    plunge: drill.plunge_mm_min,
                    dwell: drill.dwell_s,
                    stepdown: drill.stepdown_mm,
                    spindle: drill.spindle_rpm,
                }
            }
            OperationKind::SpotDrill(spot) => {
                let mut source_tools = spot.source_tools.clone();
                if source_tools.is_empty() {
                    for tool in project.layer(spot.layer)?.source_tools() {
                        if let Some(number) = tool.number {
                            source_tools.insert(number);
                        }
                    }
                }
                TaskDraftKind::SpotDrill {
                    source_tools,
                    depth: spot.depth_mm,
                    retract: spot.retract_mm,
                    feed: spot.feed_mm_min,
                    plunge: spot.plunge_mm_min,
                    dwell: spot.dwell_s,
                    spindle: spot.spindle_rpm,
                }
            }
            OperationKind::HoleMill(mill) => {
                let mut source_tools = mill.source_tools.clone();
                if source_tools.is_empty() {
                    for tool in project.layer(mill.layer)?.source_tools() {
                        if let Some(number) = tool.number {
                            source_tools.insert(number);
                        }
                    }
                }
                TaskDraftKind::HoleMill {
                    source_tools,
                    depth: mill.depth_mm,
                    retract: mill.retract_mm,
                    feed: mill.feed_mm_min,
                    plunge: mill.plunge_mm_min,
                    dwell: mill.dwell_s,
                    stepdown: mill.stepdown_mm,
                    spindle: mill.spindle_rpm,
                }
            }
            OperationKind::Cutout(cutout) => TaskDraftKind::Cutout {
                side: cutout.side,
                offset: cutout.offset_mm,
                depth: cutout.depth_mm,
                retract: cutout.retract_mm,
                feed: cutout.feed_mm_min,
                plunge: cutout.plunge_mm_min,
                spindle: cutout.spindle_rpm,
            },
            OperationKind::Engrave(engrave) => TaskDraftKind::Engrave {
                side: engrave.side,
                outline_offset: engrave.outline_offset_mm,
                offset: engrave.offset_mm,
                passes: engrave.passes,
                stepover: engrave.stepover_mm,
                depth: engrave.depth_mm,
                retract: engrave.retract_mm,
                feed: engrave.feed_mm_min,
                plunge: engrave.plunge_mm_min,
                spindle: engrave.spindle_rpm,
                boundary: engrave.boundary_layer,
            },
        };
        Some(Self {
            id: task.id(),
            name: task.name().to_owned(),
            tool: task.tool(),
            kind,
        })
    }

    fn command(&self) -> (&'static str, Value) {
        match &self.kind {
            TaskDraftKind::Drill {
                source_tools,
                depth,
                retract,
                feed,
                plunge,
                dwell,
                stepdown,
                spindle,
            } => (
                "task.update_drill",
                json!({
                    "id": self.id.value(),
                    "name": self.name,
                    "tool": self.tool.value(),
                    "source_tools": source_tools.iter().copied().collect::<Vec<_>>(),
                    "depth": depth,
                    "retract": retract,
                    "feed": feed,
                    "plunge": plunge,
                    "dwell": dwell,
                    "stepdown": stepdown,
                    "spindle": spindle,
                }),
            ),
            TaskDraftKind::SpotDrill {
                source_tools,
                depth,
                retract,
                feed,
                plunge,
                dwell,
                spindle,
            } => (
                "task.update_spot_drill",
                json!({
                    "id": self.id.value(),
                    "name": self.name,
                    "tool": self.tool.value(),
                    "source_tools": source_tools.iter().copied().collect::<Vec<_>>(),
                    "depth": depth,
                    "retract": retract,
                    "feed": feed,
                    "plunge": plunge,
                    "dwell": dwell,
                    "spindle": spindle,
                }),
            ),
            TaskDraftKind::HoleMill {
                source_tools,
                depth,
                retract,
                feed,
                plunge,
                dwell,
                stepdown,
                spindle,
            } => (
                "task.update_hole_mill",
                json!({
                    "id": self.id.value(),
                    "name": self.name,
                    "tool": self.tool.value(),
                    "source_tools": source_tools.iter().copied().collect::<Vec<_>>(),
                    "depth": depth,
                    "retract": retract,
                    "feed": feed,
                    "plunge": plunge,
                    "dwell": dwell,
                    "stepdown": stepdown,
                    "spindle": spindle,
                }),
            ),
            TaskDraftKind::Cutout {
                side,
                offset,
                depth,
                retract,
                feed,
                plunge,
                spindle,
            } => (
                "task.update_cutout",
                json!({
                    "id": self.id.value(),
                    "name": self.name,
                    "tool": self.tool.value(),
                    "side": side.id(),
                    "offset": offset,
                    "depth": depth,
                    "retract": retract,
                    "feed": feed,
                    "plunge": plunge,
                    "spindle": spindle,
                }),
            ),
            TaskDraftKind::Engrave {
                side,
                outline_offset,
                offset,
                passes,
                stepover,
                depth,
                retract,
                feed,
                plunge,
                spindle,
                boundary,
            } => (
                "task.update_engrave",
                json!({
                    "id": self.id.value(),
                    "name": self.name,
                    "tool": self.tool.value(),
                    "side": side.id(),
                    "outline_offset": outline_offset,
                    "offset": offset,
                    "passes": passes,
                    "stepover": stepover,
                    "depth": depth,
                    "retract": retract,
                    "feed": feed,
                    "plunge": plunge,
                    "spindle": spindle,
                    "boundary_default": boundary.is_none(),
                    "boundary_layer": boundary.map(|layer| layer.value()),
                }),
            ),
        }
    }
}

/// A modal draft of a machining tool.
#[derive(Debug, Clone)]
pub(crate) struct ToolEditor {
    pub(crate) id: Option<ToolId>,
    pub(crate) name: String,
    pub(crate) kind: ToolKind,
    pub(crate) diameter: f64,
    pub(crate) feed: f64,
    pub(crate) plunge: f64,
    pub(crate) spindle: u32,
    pub(crate) error: Option<String>,
}

impl ToolEditor {
    fn new() -> Self {
        Self {
            id: None,
            name: "End mill 1.00 mm".to_owned(),
            kind: ToolKind::EndMill,
            diameter: 1.0,
            feed: 300.0,
            plunge: 120.0,
            spindle: 12_000,
            error: None,
        }
    }

    fn from_tool(tool: &Tool) -> Self {
        Self {
            id: Some(tool.id()),
            name: tool.name().to_owned(),
            kind: tool.kind(),
            diameter: tool.diameter_mm(),
            feed: tool.feed_mm_min(),
            plunge: tool.plunge_mm_min(),
            spindle: tool.spindle_rpm(),
            error: None,
        }
    }

    fn from_source(source: &SourceToolInfo) -> Self {
        let kind = source.suggested_kind;
        let diameter = source.diameter_mm.unwrap_or(1.0).max(0.01);
        let (feed, plunge, spindle) = match kind {
            ToolKind::Drill => (200.0, 80.0, 10_000),
            ToolKind::Cone => (150.0, 50.0, 10_000),
            ToolKind::EndMill | ToolKind::VBit => (300.0, 120.0, 12_000),
        };
        Self {
            id: None,
            name: format!("{} {:.2} mm", kind.label(), diameter),
            kind,
            diameter,
            feed,
            plunge,
            spindle,
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClearanceKey {
    task: OperationId,
    layer: LayerId,
    tool: ToolId,
    diameter_bits: u64,
    outline_offset_bits: u64,
    offset_bits: u64,
    passes: u32,
    stepover_bits: u64,
    side_bits: u64,
    boundary_bits: Option<[u64; 4]>,
    boundary: Option<LayerId>,
}

#[derive(Debug)]
struct ClearancePreview {
    key: ClearanceKey,
    paths: Vec<CamPath>,
}

/// Editable absolute G-code zero point.
#[derive(Debug, Clone, Copy)]
pub(crate) struct OriginDialog {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

/// Top-level application state.
#[derive(Debug)]
pub struct PbcCam {
    pub(crate) session: Session,
    pub(crate) view: ViewState,
    pub(crate) settings: ViewSettings,
    pub(crate) selection: Option<Selection>,
    pub(crate) status: String,
    pub(crate) status_error: bool,
    pub(crate) about_open: bool,
    pub(crate) shortcuts_open: bool,
    pub(crate) task_draft: Option<TaskDraft>,
    pub(crate) tool_editor: Option<ToolEditor>,
    clearance_preview: Option<ClearancePreview>,
    render_cache: HashMap<LayerId, Arc<Vec<GerberRegion>>>,
    pub(crate) fit_requested: bool,
    pub(crate) origin_pick: bool,
    pub(crate) origin_dialog: Option<OriginDialog>,
    pub(crate) origin_drag: bool,
    pub(crate) origin_preview: Option<Point>,
    window_title: String,
}

impl PbcCam {
    /// Build the application and install the PCB-CAM style.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        log::info!("starting {APP_NAME} {VERSION}");

        let mut session = Session::new();
        if let Some(storage) = cc.storage
            && let Some(json) = storage.get_string("machining_tool_library")
            && let Ok(library) = serde_json::from_str::<ToolLibrary>(&json)
        {
            session.set_tool_library(library);
        }
        if session
            .tool_library()
            .first_of_kind(ToolKind::Cone)
            .is_none()
        {
            session.tool_library_mut().add(
                "Cone 6.00 mm",
                ToolKind::Cone,
                6.0,
                200.0,
                80.0,
                12_000,
            );
        }

        Self {
            session,
            view: ViewState::default(),
            settings: ViewSettings::default(),
            selection: None,
            status: "Ready — import Gerber and Excellon files to begin".to_owned(),
            status_error: false,
            about_open: false,
            shortcuts_open: false,
            task_draft: None,
            tool_editor: None,
            clearance_preview: None,
            render_cache: HashMap::new(),
            fit_requested: false,
            origin_pick: false,
            origin_dialog: None,
            origin_drag: false,
            origin_preview: None,
            window_title: WINDOW_TITLE.to_owned(),
        }
    }

    pub(crate) fn project(&self) -> &pcb_cam_core::Project {
        self.session.project()
    }

    pub(crate) fn run_command(&mut self, id: &str, params: Value) -> Option<Value> {
        match self.session.execute(id, &params) {
            Ok(value) => {
                self.validate_selection();
                Some(value)
            }
            Err(error) => {
                let label = find_command(id).map_or(id, |command| command.label);
                self.set_error(format!("{label}: {error}"));
                None
            }
        }
    }

    fn validate_selection(&mut self) {
        let valid = match self.selection {
            Some(Selection::Layer(id)) => self.project().layer(id).is_some(),
            Some(Selection::Tool(id)) => self.session.tool_library().tool(id).is_some(),
            Some(Selection::Task(id)) => self.project().task(id).is_some(),
            None => true,
        };
        if !valid {
            self.selection = None;
        }
    }

    pub(crate) fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
        self.status_error = false;
    }

    pub(crate) fn set_error(&mut self, message: impl Into<String>) {
        self.status = message.into();
        self.status_error = true;
    }

    pub(crate) fn new_project(&mut self) {
        if self
            .run_command("project.new", json!({"name": "Untitled"}))
            .is_some()
        {
            self.selection = None;
            self.task_draft = None;
            self.view.reset();
            self.fit_requested = false;
            self.set_status("New project created");
        }
    }

    pub(crate) fn undo(&mut self) {
        if self.run_command("edit.undo", json!({})).is_some() {
            self.task_draft = None;
            self.set_status("Undo");
        }
    }

    pub(crate) fn redo(&mut self) {
        if self.run_command("edit.redo", json!({})).is_some() {
            self.task_draft = None;
            self.set_status("Redo");
        }
    }

    pub(crate) fn open_origin_dialog(&mut self) {
        let origin = self.project().gcode_origin();
        self.origin_pick = false;
        self.origin_dialog = Some(OriginDialog {
            x: origin.x,
            y: origin.y,
        });
    }

    pub(crate) fn set_gcode_origin(&mut self, point: Point) {
        if self
            .run_command(
                "project.set_gcode_origin",
                json!({"x": point.x, "y": point.y}),
            )
            .is_some()
        {
            self.origin_pick = false;
            self.set_status(format!(
                "G-code 0-point: X{:.3} Y{:.3} mm",
                point.x, point.y
            ));
        }
    }

    pub(crate) fn fit_view(&mut self) {
        self.fit_requested = true;
        self.set_status("Fit view");
    }

    pub(crate) fn set_units(&mut self, units: Unit) {
        let symbol = units.symbol();
        if self
            .run_command("units.set", json!({"units": symbol}))
            .is_some()
        {
            self.set_status(format!("Display units: {}", units.name()));
        }
    }

    pub(crate) fn import_gerber_dialog(&mut self) {
        let Some(path) = file_dialog("Import Gerber", "Gerber", SourceFormat::Gerber.extensions())
        else {
            return;
        };
        self.import_path(path);
    }

    pub(crate) fn import_excellon_dialog(&mut self) {
        let Some(path) = file_dialog(
            "Import Excellon",
            "Excellon",
            SourceFormat::Excellon.extensions(),
        ) else {
            return;
        };
        self.import_path(path);
    }
    pub(crate) fn import_zip_dialog(&mut self) {
        let Some(path) = file_dialog("Import Gerber ZIP", "ZIP archive", &["zip"]) else {
            return;
        };
        self.import_zip(path);
    }

    /// Import every Gerber/Excellon file contained in a ZIP archive.
    pub(crate) fn import_zip(&mut self, path: PathBuf) -> bool {
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(error) => {
                self.set_error(format!("Cannot open {}: {error}", path.display()));
                return false;
            }
        };
        let mut archive = match zip::ZipArchive::new(file) {
            Ok(archive) => archive,
            Err(error) => {
                self.set_error(format!("Cannot read ZIP {}: {error}", path.display()));
                return false;
            }
        };

        let zip_name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Imported PCB".to_owned());
        if self.project().layers().is_empty() && self.project().tasks().is_empty() {
            let _ = self.run_command("project.new", json!({"name": zip_name}));
        }

        let mut imported = 0usize;
        let mut objects = 0usize;
        let mut warnings = 0usize;
        let mut skipped = 0usize;
        let mut preferred_layer = None;
        let mut fallback_layer = None;

        for index in 0..archive.len() {
            let mut entry = match archive.by_index(index) {
                Ok(entry) => entry,
                Err(error) => {
                    log::warn!("ZIP entry {index} cannot be read: {error}");
                    skipped += 1;
                    continue;
                }
            };
            let entry_name = entry.name().to_owned();
            if entry.is_dir() || skip_zip_entry(&entry_name) {
                skipped += 1;
                continue;
            }
            if entry.size() > 64 * 1024 * 1024 {
                log::warn!("ZIP entry `{entry_name}` is too large, skipped");
                skipped += 1;
                continue;
            }

            let mut bytes = Vec::new();
            if let Err(error) = entry.read_to_end(&mut bytes) {
                log::warn!("ZIP entry `{entry_name}` cannot be decompressed: {error}");
                skipped += 1;
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            let Some(format) = classify_zip_entry(&entry_name, &text) else {
                skipped += 1;
                continue;
            };
            let Some(stem) = Path::new(&entry_name)
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
            else {
                skipped += 1;
                continue;
            };
            let command = match format {
                SourceFormat::Gerber => "file.import_gerber",
                SourceFormat::Excellon => "file.import_excellon",
            };
            match self.run_command(command, json!({"content": text.as_ref(), "name": stem})) {
                Some(value) => {
                    imported += 1;
                    objects += value.get("objects").and_then(Value::as_u64).unwrap_or(0) as usize;
                    warnings += value
                        .get("warnings")
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len);
                    let layer = value.get("layer").and_then(Value::as_u64).map(LayerId::new);
                    if fallback_layer.is_none() {
                        fallback_layer = layer;
                    }
                    if format == SourceFormat::Gerber && preferred_layer.is_none() {
                        preferred_layer = layer;
                    }
                }
                None => {
                    skipped += 1;
                }
            }
        }

        if imported == 0 {
            self.set_error(format!(
                "ZIP: no Gerber/Excellon layers imported ({skipped} entries skipped)"
            ));
            return false;
        }

        if let Some(layer) = preferred_layer.or(fallback_layer) {
            self.selection = Some(Selection::Layer(layer));
        }
        self.fit_requested = true;
        let skipped_note = if skipped > 0 {
            format!(" · {skipped} skipped")
        } else {
            String::new()
        };
        let warning_note = if warnings > 0 {
            format!(" · {warnings} warnings")
        } else {
            String::new()
        };
        self.set_status(format!(
            "Imported ZIP: {imported} layers · {objects} objects{skipped_note}{warning_note}"
        ));
        true
    }

    pub(crate) fn import_path(&mut self, path: PathBuf) {
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
        {
            self.import_zip(path);
            return;
        }
        let Some(format) = SourceFormat::from_path(&path) else {
            self.set_error(format!(
                "Unknown file type: {}",
                path.file_name()
                    .map(|name| name.to_string_lossy())
                    .unwrap_or_default()
            ));
            return;
        };
        let id = match format {
            SourceFormat::Gerber => "file.import_gerber",
            SourceFormat::Excellon => "file.import_excellon",
        };
        if let Some(value) = self.run_command(id, json!({"path": path})) {
            self.apply_import_result(format, &value);
        }
    }

    pub(crate) fn import_content(
        &mut self,
        format: SourceFormat,
        content: &str,
        name: &str,
    ) -> bool {
        let id = match format {
            SourceFormat::Gerber => "file.import_gerber",
            SourceFormat::Excellon => "file.import_excellon",
        };
        match self.run_command(id, json!({"content": content, "name": name})) {
            Some(value) => {
                self.apply_import_result(format, &value);
                true
            }
            None => false,
        }
    }

    fn apply_import_result(&mut self, format: SourceFormat, value: &Value) {
        let layer = value.get("layer").and_then(Value::as_u64).map(LayerId::new);
        let objects = value.get("objects").and_then(Value::as_u64).unwrap_or(0);
        let warnings = value
            .get("warnings")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        if let Some(layer) = layer {
            self.selection = Some(Selection::Layer(layer));
        }
        self.fit_requested = true;
        let suffix = if warnings == 0 {
            String::new()
        } else {
            format!(
                " · {warnings} warning{}",
                if warnings == 1 { "" } else { "s" }
            )
        };
        self.set_status(format!(
            "Imported {} layer · {objects} objects{suffix}",
            format.label()
        ));
    }

    /// Dynamic warnings for a task, derived from its selected source objects
    /// and the machining tool in the user library.
    pub(crate) fn task_warnings(&self, id: OperationId) -> Vec<String> {
        let Some(task) = self.project().task(id) else {
            return vec!["Task no longer exists.".to_owned()];
        };
        let (layer_id, tool_id) = match task.kind() {
            OperationKind::Drill(drill) => (drill.layer, drill.tool),
            OperationKind::HoleMill(mill) => (mill.layer, mill.tool),
            _ => return Vec::new(),
        };
        let Some(layer) = self.project().layer(layer_id) else {
            return vec!["Source layer no longer exists.".to_owned()];
        };
        let LayerGeometry::Excellon(excellon) = layer.geometry() else {
            return vec!["Task source is not an Excellon layer.".to_owned()];
        };
        let Some(tool) = self.session.tool_library().tool(tool_id) else {
            return vec!["Machining tool no longer exists.".to_owned()];
        };
        match task.kind() {
            OperationKind::Drill(drill) => drill.warnings(excellon, tool),
            OperationKind::HoleMill(mill) => mill.warnings(excellon, tool),
            _ => Vec::new(),
        }
    }

    pub(crate) fn add_drill_task(&mut self) {
        let layer = self.drill_layer_for_new_task();
        self.add_task_with_layer("task.add_drill", layer, "Drilling");
    }

    pub(crate) fn add_spot_drill_task(&mut self) {
        let layer = self.spot_drill_layer_for_new_task();
        self.add_task_with_layer("task.add_spot_drill", layer, "Spot drill");
    }

    pub(crate) fn add_hole_mill_task(&mut self) {
        let layer = self.hole_mill_layer_for_new_task();
        self.add_task_with_layer("task.add_hole_mill", layer, "Hole milling");
    }

    pub(crate) fn add_cutout_task(&mut self) {
        let layer = self.cutout_layer_for_new_task();
        self.add_task_with_layer("task.add_cutout", layer, "Edge cut");
    }

    pub(crate) fn add_engrave_task(&mut self) {
        let layer = self.engrave_layer_for_new_task();
        self.add_task_with_layer("task.add_engrave", layer, "Clearance");
    }

    fn add_task_with_layer(&mut self, command: &str, layer: Option<LayerId>, label: &str) {
        let Some(layer) = layer else {
            self.set_error(format!("No suitable source layer for a {label} task"));
            return;
        };
        let layer_name = self
            .project()
            .layer(layer)
            .map_or("layer".to_owned(), |layer| layer.name().to_owned());
        if let Some(value) = self.run_command(command, json!({"layer": layer.value()})) {
            let task_id = value
                .get("id")
                .and_then(Value::as_u64)
                .map(OperationId::new);
            if let Some(id) = task_id {
                self.selection = Some(Selection::Task(id));
            }
            let warning = task_id.and_then(|id| self.task_warnings(id).into_iter().next());
            if let Some(warning) = warning {
                self.set_status(format!(
                    "{label} task added for {layer_name} · warning: {warning}"
                ));
            } else {
                self.set_status(format!("{label} task added for {layer_name}"));
            }
        }
    }

    pub(crate) fn can_add_drill_task(&self) -> bool {
        self.drill_layer_for_new_task().is_some()
    }

    pub(crate) fn can_add_spot_drill_task(&self) -> bool {
        self.spot_drill_layer_for_new_task().is_some()
    }

    pub(crate) fn can_add_hole_mill_task(&self) -> bool {
        self.hole_mill_layer_for_new_task().is_some()
    }

    pub(crate) fn can_add_cutout_task(&self) -> bool {
        self.cutout_layer_for_new_task().is_some()
    }

    pub(crate) fn can_add_engrave_task(&self) -> bool {
        self.engrave_layer_for_new_task().is_some()
    }

    fn drill_layer_for_new_task(&self) -> Option<LayerId> {
        if let Some(Selection::Layer(id)) = self.selection
            && let Some(layer) = self.project().layer(id)
            && matches!(
                layer.geometry(),
                LayerGeometry::Excellon(excellon)
                    if !excellon.holes.is_empty() || !excellon.slots.is_empty()
            )
        {
            return Some(id);
        }
        self.project()
            .layers()
            .iter()
            .find(|layer| {
                matches!(
                    layer.geometry(),
                    LayerGeometry::Excellon(excellon)
                        if !excellon.holes.is_empty() || !excellon.slots.is_empty()
                )
            })
            .map(|layer| layer.id())
    }

    fn spot_drill_layer_for_new_task(&self) -> Option<LayerId> {
        if let Some(Selection::Layer(id)) = self.selection
            && let Some(layer) = self.project().layer(id)
            && matches!(
                layer.geometry(),
                LayerGeometry::Excellon(excellon) if !excellon.holes.is_empty()
            )
        {
            return Some(id);
        }
        self.project()
            .layers()
            .iter()
            .find(|layer| {
                matches!(
                    layer.geometry(),
                    LayerGeometry::Excellon(excellon) if !excellon.holes.is_empty()
                )
            })
            .map(|layer| layer.id())
    }

    fn hole_mill_layer_for_new_task(&self) -> Option<LayerId> {
        if let Some(Selection::Layer(id)) = self.selection
            && let Some(layer) = self.project().layer(id)
            && matches!(
                layer.geometry(),
                LayerGeometry::Excellon(excellon)
                    if !excellon.holes.is_empty() || !excellon.slots.is_empty()
            )
        {
            return Some(id);
        }
        self.project()
            .layers()
            .iter()
            .find(|layer| {
                matches!(
                    layer.geometry(),
                    LayerGeometry::Excellon(excellon)
                        if !excellon.holes.is_empty() || !excellon.slots.is_empty()
                )
            })
            .map(|layer| layer.id())
    }

    fn cutout_layer_for_new_task(&self) -> Option<LayerId> {
        if let Some(Selection::Layer(id)) = self.selection {
            return self.layer_has_closed_contours(id).then_some(id);
        }
        let layers = self.project().layers();
        layers
            .iter()
            .find(|layer| {
                let name = layer.name().to_ascii_lowercase();
                (name.contains("edge")
                    || name.contains("outline")
                    || name.contains("gm1")
                    || name.contains("gko"))
                    && self.layer_has_closed_contours(layer.id())
            })
            .or_else(|| {
                layers
                    .iter()
                    .find(|layer| self.layer_has_closed_contours(layer.id()))
            })
            .map(|layer| layer.id())
    }

    fn engrave_layer_for_new_task(&self) -> Option<LayerId> {
        if let Some(Selection::Layer(id)) = self.selection
            && self.layer_has_paths(id)
        {
            return Some(id);
        }
        let layers = self.project().layers();
        layers
            .iter()
            .find(|layer| {
                let name = layer.name().to_ascii_lowercase();
                (name.contains("silk")
                    || name.contains("gto")
                    || name.contains("gbo")
                    || name.contains("text"))
                    && self.layer_has_paths(layer.id())
            })
            .or_else(|| layers.iter().find(|layer| self.layer_has_paths(layer.id())))
            .map(|layer| layer.id())
    }

    fn layer_has_closed_contours(&self, id: LayerId) -> bool {
        self.project()
            .layer(id)
            .is_some_and(|layer| match layer.geometry() {
                LayerGeometry::Gerber(gerber) => gerber.has_closed_contours(),
                LayerGeometry::Excellon(_) => false,
            })
    }

    fn layer_has_paths(&self, id: LayerId) -> bool {
        self.project()
            .layer(id)
            .is_some_and(|layer| match layer.geometry() {
                LayerGeometry::Gerber(gerber) => gerber.has_engraving_paths(),
                LayerGeometry::Excellon(_) => false,
            })
    }

    pub(crate) fn export_task_gcode_dialog(&mut self) {
        let Some(Selection::Task(id)) = self.selection else {
            self.set_error("Select a task to export");
            return;
        };
        let Some(task) = self.project().task(id) else {
            return;
        };
        let Some(path) = self.choose_gcode_path(&format!("-{}", task.kind_label())) else {
            return;
        };
        if let Some(value) = self.run_command(
            "file.export_task_gcode",
            json!({"path": path, "task": id.value()}),
        ) {
            self.report_export(&value, "Task");
        }
    }

    pub(crate) fn export_all_gcode_dialog(&mut self) {
        if self.project().enabled_task_count() == 0 {
            self.set_error("Add and enable a task first");
            return;
        }
        let Some(path) = self.choose_gcode_path("-all") else {
            return;
        };
        if let Some(value) = self.run_command("file.export_gcode", json!({"path": path})) {
            self.report_export(&value, "Combined");
        }
    }

    fn choose_gcode_path(&self, suffix: &str) -> Option<PathBuf> {
        let base: String = self
            .project()
            .name()
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                    ch
                } else {
                    '_'
                }
            })
            .collect();
        let base = base.trim_matches('_');
        let base = if base.is_empty() { "board" } else { base };
        let default_name = format!("{base}{suffix}.nc");
        rfd::FileDialog::new()
            .set_title("Export G-code")
            .add_filter("G-code", &["nc", "gcode", "tap"])
            .set_file_name(default_name)
            .save_file()
    }

    fn report_export(&mut self, value: &Value, kind: &str) {
        let tasks = value.get("tasks").and_then(Value::as_u64).unwrap_or(0);
        let tools = value.get("tools").and_then(Value::as_u64).unwrap_or(0);
        let paths = value.get("paths").and_then(Value::as_u64).unwrap_or(0);
        let holes = value.get("holes").and_then(Value::as_u64).unwrap_or(0);
        let slots = value.get("slots").and_then(Value::as_u64).unwrap_or(0);
        self.set_status(format!(
            "{kind} G-code exported · {tasks} tasks · {tools} tools · {holes} holes · {slots} slots · {paths} paths"
        ));
        log::info!("exported {kind} G-code: {value}");
    }

    pub(crate) fn remove_selection(&mut self) {
        let Some(selection) = self.selection else {
            return;
        };
        let result = match selection {
            Selection::Layer(id) => self.run_command("layer.remove", json!({"id": id.value()})),
            Selection::Tool(id) => self.run_command("tool.remove", json!({"id": id.value()})),
            Selection::Task(id) => self.run_command("task.remove", json!({"id": id.value()})),
        };
        if result.is_some() {
            self.selection = None;
            self.task_draft = None;
            self.set_status("Removed");
        }
    }

    /// Load the bundled demo project.
    pub fn load_demo(&mut self) {
        let top = include_str!("../../../assets/samples/demo-F_Cu.gbr");
        let outline = include_str!("../../../assets/samples/demo-Edge_Cuts.gbr");
        let silk = include_str!("../../../assets/samples/demo-Silkscreen.gbr");
        let drills = include_str!("../../../assets/samples/demo.drl");

        if self
            .run_command("project.new", json!({"name": "Demo Board"}))
            .is_none()
        {
            return;
        }
        self.selection = None;
        if !self.import_content(SourceFormat::Gerber, top, "Demo Top Copper") {
            return;
        }
        if !self.import_content(SourceFormat::Gerber, outline, "Demo Board Outline") {
            return;
        }
        if !self.import_content(SourceFormat::Gerber, silk, "Demo Silkscreen") {
            return;
        }
        if !self.import_content(SourceFormat::Excellon, drills, "Demo Drills") {
            return;
        }

        let layer_id = |name: &str| {
            self.project()
                .layers()
                .iter()
                .find(|layer| layer.name() == name)
                .map(|layer| layer.id().value())
        };
        let drill = layer_id("Demo Drills");
        let outline = layer_id("Demo Board Outline");
        let silk = layer_id("Demo Silkscreen");

        let library = self.session.tool_library();
        let small_drill = library
            .closest(ToolKind::Drill, 0.8)
            .map(|tool| tool.value());
        let end_mill = library
            .closest(ToolKind::EndMill, 1.0)
            .map(|tool| tool.value());
        let v_bit = library
            .closest(ToolKind::VBit, 0.2)
            .or_else(|| library.closest(ToolKind::EndMill, 0.2))
            .map(|tool| tool.value());

        let mut task_ids = Vec::new();
        for (command, params) in [
            (
                "task.add_drill",
                json!({
                    "layer": drill,
                    "tool": small_drill,
                    "source_tools": [1],
                    "depth": 1.6,
                    "name": "Drill 0.8",
                }),
            ),
            (
                "task.add_hole_mill",
                json!({
                    "layer": drill,
                    "tool": end_mill,
                    "source_tools": [2, 3],
                    "depth": 1.8,
                    "name": "Hole mill 1.0 / 3.0",
                }),
            ),
            (
                "task.add_cutout",
                json!({
                    "layer": outline,
                    "tool": end_mill,
                    "side": "outside",
                    "depth": 1.8,
                }),
            ),
            (
                "task.add_engrave",
                json!({
                    "layer": silk,
                    "tool": v_bit,
                    "depth": 0.2,
                }),
            ),
        ] {
            let Some(value) = self.run_command(command, params) else {
                return;
            };
            if let Some(id) = value.get("id").and_then(Value::as_u64) {
                task_ids.push(OperationId::new(id));
            }
        }
        self.selection = task_ids
            .get(1)
            .or_else(|| task_ids.last())
            .copied()
            .map(Selection::Task);
        self.fit_requested = true;
        self.set_status("Demo loaded — 4 layers, 4 tasks, source filters and machining tools");
    }

    fn show_origin_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.origin_dialog else {
            return;
        };
        let mut open = true;
        let mut submit = false;
        let mut pick = false;
        let mut cancel = false;

        egui::Window::new("G-code 0-point")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(320.0)
            .frame(dialog_frame())
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("Absolute board coordinates that become X0 Y0.")
                        .size(11.0)
                        .color(theme::TEXT_MUTED),
                );
                ui.add_space(8.0);
                egui::Grid::new("gcode_origin_grid")
                    .num_columns(2)
                    .spacing(vec2(10.0, 6.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new("X").color(theme::TEXT_MUTED));
                        ui.add(
                            egui::DragValue::new(&mut dialog.x)
                                .speed(0.1)
                                .fixed_decimals(3)
                                .suffix(" mm"),
                        );
                        ui.end_row();
                        ui.label(RichText::new("Y").color(theme::TEXT_MUTED));
                        ui.add(
                            egui::DragValue::new(&mut dialog.y)
                                .speed(0.1)
                                .fixed_decimals(3)
                                .suffix(" mm"),
                        );
                        ui.end_row();
                    });
                ui.add_space(10.0);
                ui.label(
                    RichText::new("Drag the G0 marker on the canvas, or pick a point.")
                        .size(10.5)
                        .color(theme::TEXT_FAINT),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Set").clicked() {
                        submit = true;
                    }
                    if ui.button("Pick on canvas").clicked() {
                        pick = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });

        if submit {
            self.set_gcode_origin(Point::new(dialog.x, dialog.y));
            self.origin_dialog = None;
        } else if pick {
            self.origin_dialog = None;
            self.origin_pick = true;
            self.set_status("Click the canvas to place the G-code 0-point");
        } else if cancel || !open {
            self.origin_dialog = None;
            self.origin_pick = false;
        } else {
            self.origin_dialog = Some(dialog);
        }
    }

    fn sync_window_title(&mut self, ctx: &egui::Context) {
        let dirty = if self.session.dirty() { " *" } else { "" };
        let title = format!("{APP_NAME} — {}{dirty}", self.project().name());
        if title != self.window_title {
            self.window_title.clone_from(&title);
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
    }

    pub(crate) fn quit(ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // Cmd+Q is the one shortcut that must work even while editing text.
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Q)) {
            Self::quit(ctx);
            return;
        }

        // When a text field (including a DragValue in edit mode) has focus,
        // every key belongs to that field. In particular, the macOS Delete key
        // sends Backspace and must never trigger "remove selection".
        if ctx.text_edit_focused() {
            return;
        }

        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.about_open = false;
            self.shortcuts_open = false;
            self.task_draft = None;
            self.origin_dialog = None;
            self.origin_preview = None;
            self.origin_drag = false;
            if self.origin_pick {
                self.origin_pick = false;
                self.set_status("G-code 0-point pick cancelled");
            }
        }

        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F1)) {
            self.about_open = true;
            return;
        }

        if self.about_open || self.shortcuts_open {
            return;
        }

        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::O)) {
            self.import_excellon_dialog();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::O)) {
            self.import_gerber_dialog();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::N)) {
            self.new_project();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z))
            || ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Y))
        {
            self.redo();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Z)) {
            self.undo();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::E)) {
            self.export_all_gcode_dialog();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::E)) {
            self.export_task_gcode_dialog();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F)) {
            self.fit_view();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::G)) {
            self.settings.show_grid = !self.settings.show_grid;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete))
            || ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Backspace))
        {
            self.remove_selection();
        }
    }

    fn show_dialogs(&mut self, ctx: &egui::Context) {
        self.show_origin_dialog(ctx);

        if self.about_open {
            let mut open = true;
            let mut close_clicked = false;

            egui::Window::new(format!("About {APP_NAME}"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .default_width(400.0)
                .frame(dialog_frame())
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        widgets::brand_mark(ui, 58.0);
                        ui.add_space(4.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(APP_NAME).size(23.0).strong());
                            ui.label(
                                RichText::new("PCB fabrication CAM")
                                    .size(11.0)
                                    .color(theme::TEXT_MUTED),
                            );
                        });
                    });

                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(
                            "Cross-platform CAM workbench for turning Gerber and Excellon \
                             fabrication data into CNC control programs.",
                        )
                        .size(12.5)
                        .color(theme::TEXT),
                    );
                    ui.add_space(10.0);

                    widgets::field_row(ui, "Version", VERSION);
                    widgets::field_row(ui, "License", "MIT");
                    widgets::field_row(ui, "Built with", "Rust + egui + gerber_parser");
                    widgets::field_row(ui, "Milestone", "Source filters + machining tool library");
                    widgets::field_row(
                        ui,
                        "In this project",
                        &format!(
                            "{} layers · {} tasks",
                            self.project().layers().len(),
                            self.project().tasks().len()
                        ),
                    );

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("© 2026 andryblack")
                                .size(10.5)
                                .color(theme::TEXT_FAINT),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Close").clicked() {
                                close_clicked = true;
                            }
                        });
                    });
                });

            self.about_open = open && !close_clicked;
        }

        if self.shortcuts_open {
            let mut open = true;
            let mut close_clicked = false;

            egui::Window::new("Keyboard shortcuts")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .default_width(380.0)
                .frame(dialog_frame())
                .show(ctx, |ui| {
                    shortcut_section(ui, "Project");
                    shortcut_row(ui, SHORTCUT_NEW, "New project");
                    shortcut_row(ui, SHORTCUT_OPEN, "Import Gerber");
                    shortcut_row(ui, "Cmd/Ctrl+Shift+O", "Import Excellon");
                    shortcut_row(ui, "Cmd/Ctrl+E", "Export selected task G-code");
                    shortcut_row(ui, "Cmd/Ctrl+Shift+E", "Export all tasks G-code");
                    shortcut_row(ui, SHORTCUT_QUIT, "Quit");

                    ui.add_space(10.0);
                    shortcut_section(ui, "Edit");
                    shortcut_row(ui, SHORTCUT_UNDO, "Undo");
                    shortcut_row(ui, SHORTCUT_REDO, "Redo");
                    shortcut_row(ui, "Delete", "Remove selected item");

                    ui.add_space(10.0);
                    shortcut_section(ui, "Canvas");
                    shortcut_row(ui, "Scroll", "Zoom around pointer");
                    shortcut_row(ui, "Drag", "Pan");
                    shortcut_row(ui, "F or double-click", "Fit project");
                    shortcut_row(ui, "G", "Toggle grid");
                    shortcut_row(ui, "F1", "About");
                    shortcut_row(ui, "Esc", "Close dialog");

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            close_clicked = true;
                        }
                    });
                });

            self.shortcuts_open = open && !close_clicked;
        }

        self.show_tool_editor(ctx);
    }

    /// Keep the inline inspector draft in sync with the selected task.
    pub(crate) fn sync_task_draft(&mut self) {
        let selected = match self.selection {
            Some(Selection::Task(id)) => Some(id),
            _ => None,
        };
        let Some(id) = selected else {
            self.task_draft = None;
            return;
        };
        if self.task_draft.as_ref().map(|draft| draft.id) != Some(id) {
            self.task_draft = self
                .project()
                .task(id)
                .and_then(|task| TaskDraft::from_task(self.project(), task));
        }
    }

    /// Apply the inline inspector draft to the task.
    pub(crate) fn commit_task_draft(&mut self) {
        let Some(draft) = self.task_draft.clone() else {
            return;
        };
        let empty_filter = match &draft.kind {
            TaskDraftKind::Drill { source_tools, .. }
            | TaskDraftKind::SpotDrill { source_tools, .. }
            | TaskDraftKind::HoleMill { source_tools, .. } => source_tools.is_empty(),
            _ => false,
        };
        if empty_filter {
            self.set_error("Task: select at least one source tool");
            return;
        }
        let (command, params) = draft.command();
        if self.run_command(command, params).is_some() {
            self.task_draft = self
                .project()
                .task(draft.id)
                .and_then(|task| TaskDraft::from_task(self.project(), task));
            self.set_status("Task updated");
        }
    }

    /// Recompute the selected clearance overlay only when its inputs change,
    /// not on every pan/zoom frame.
    fn sync_clearance_preview(&mut self) {
        let Some(Selection::Task(id)) = self.selection else {
            self.clearance_preview = None;
            return;
        };
        let Some(task) = self.project().task(id) else {
            self.clearance_preview = None;
            return;
        };
        let OperationKind::Engrave(engrave) = task.kind() else {
            self.clearance_preview = None;
            return;
        };
        let Some(layer) = self.project().layer(engrave.layer) else {
            self.clearance_preview = None;
            return;
        };
        let LayerGeometry::Gerber(gerber) = layer.geometry() else {
            self.clearance_preview = None;
            return;
        };
        let Some(tool) = self.session.tool_library().tool(engrave.tool) else {
            self.clearance_preview = None;
            return;
        };
        let boundary_path = engrave.boundary_path(self.project());
        let boundary_bits = boundary_path
            .as_ref()
            .and_then(|path| path.bounds())
            .map(|bounds| {
                [
                    bounds.min.x.to_bits(),
                    bounds.min.y.to_bits(),
                    bounds.max.x.to_bits(),
                    bounds.max.y.to_bits(),
                ]
            });
        let key = ClearanceKey {
            task: id,
            layer: engrave.layer,
            tool: engrave.tool,
            diameter_bits: tool.diameter_mm().to_bits(),
            outline_offset_bits: engrave.outline_offset_mm.to_bits(),
            offset_bits: engrave.offset_mm.to_bits(),
            passes: engrave.passes,
            stepover_bits: engrave.stepover_mm.to_bits(),
            side_bits: engrave.side.sign().to_bits(),
            boundary_bits,
            boundary: engrave.boundary_layer,
        };
        if self
            .clearance_preview
            .as_ref()
            .is_some_and(|preview| preview.key == key)
        {
            return;
        }
        let paths = engrave.toolpaths(gerber, boundary_path.as_ref());
        self.clearance_preview = Some(ClearancePreview { key, paths });
    }

    /// Union Gerber layers once and cache the filled polygons used by the canvas.
    pub(crate) fn sync_render_cache(&mut self) {
        let ids: Vec<LayerId> = self
            .project()
            .layers()
            .iter()
            .filter(|layer| layer.visible())
            .filter(|layer| matches!(layer.geometry(), LayerGeometry::Gerber(_)))
            .map(|layer| layer.id())
            .collect();
        for id in ids {
            if self.render_cache.contains_key(&id) {
                continue;
            }
            let regions = self
                .project()
                .layer(id)
                .and_then(|layer| match layer.geometry() {
                    LayerGeometry::Gerber(gerber) => Some(gerber.unioned_regions()),
                    LayerGeometry::Excellon(_) => None,
                })
                .unwrap_or_default();
            self.render_cache.insert(id, Arc::new(regions));
        }
    }

    pub(crate) fn render_regions(&self, id: LayerId) -> Option<&[GerberRegion]> {
        self.render_cache.get(&id).map(|regions| regions.as_slice())
    }

    pub(crate) fn clearance_preview_paths(&self, task: OperationId) -> Option<&[CamPath]> {
        self.clearance_preview
            .as_ref()
            .filter(|preview| preview.key.task == task)
            .map(|preview| preview.paths.as_slice())
    }

    pub(crate) fn add_tool(&mut self) {
        self.tool_editor = Some(ToolEditor::new());
    }

    pub(crate) fn add_tool_from_source(&mut self, source: &SourceToolInfo) {
        self.tool_editor = Some(ToolEditor::from_source(source));
    }

    pub(crate) fn open_tool_editor(&mut self) {
        let Some(Selection::Tool(id)) = self.selection else {
            self.set_error("Select a machining tool first");
            return;
        };
        let Some(tool) = self.session.tool_library().tool(id) else {
            return;
        };
        self.tool_editor = Some(ToolEditor::from_tool(tool));
    }

    fn show_tool_editor(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.tool_editor.clone() else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;

        egui::Window::new(if editor.id.is_some() {
            "Machining tool"
        } else {
            "New machining tool"
        })
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .default_width(400.0)
        .frame(dialog_frame())
        .show(ctx, |ui| {
            widgets::section_label(ui, "Tool");
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.add(egui::TextEdit::singleline(&mut editor.name).desired_width(f32::INFINITY));
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Type").size(11.0).color(theme::TEXT_MUTED));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for kind in [
                        ToolKind::Cone,
                        ToolKind::VBit,
                        ToolKind::EndMill,
                        ToolKind::Drill,
                    ] {
                        if ui
                            .add(Button::selectable(editor.kind == kind, kind.label()))
                            .clicked()
                        {
                            editor.kind = kind;
                        }
                    }
                });
            });
            metric_row(ui, "Diameter", &mut editor.diameter, 0.05..=25.0, " mm");
            metric_row(ui, "Feed", &mut editor.feed, 1.0..=10_000.0, " mm/min");
            metric_row(ui, "Plunge", &mut editor.plunge, 1.0..=5_000.0, " mm/min");
            spindle_row(ui, &mut editor.spindle);

            if let Some(error) = &editor.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(11.0).color(theme::ERROR));
            }

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Save").clicked() {
                        apply = true;
                    }
                });
            });
        });

        if apply {
            let command = if editor.id.is_some() {
                "tool.update"
            } else {
                "tool.create"
            };
            let mut params = json!({
                "name": editor.name,
                "kind": editor.kind.id(),
                "diameter": editor.diameter,
                "feed": editor.feed,
                "plunge": editor.plunge,
                "spindle": editor.spindle,
            });
            if let Some(id) = editor.id {
                params["id"] = json!(id.value());
            }
            match self.run_command(command, params) {
                Some(value) => {
                    if let Some(id) = value.get("id").and_then(Value::as_u64) {
                        self.selection = Some(Selection::Tool(ToolId::new(id)));
                    }
                    self.tool_editor = None;
                    self.set_status("Machining tool saved");
                }
                None => {
                    editor.error = Some(self.status.clone());
                    self.tool_editor = Some(editor);
                }
            }
            return;
        }

        if cancel || !open {
            self.tool_editor = None;
        } else {
            self.tool_editor = Some(editor);
        }
    }
}

fn spindle_row(ui: &mut egui::Ui, spindle: &mut u32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Spindle").size(11.0).color(theme::TEXT_MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::DragValue::new(spindle)
                    .range(0..=60_000)
                    .speed(100)
                    .suffix(" rpm"),
            );
        });
    });
}

impl eframe::App for PbcCam {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_shortcuts(ui.ctx());
        self.sync_window_title(ui.ctx());

        let dropped: Vec<PathBuf> = ui.ctx().input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| PathBuf::from(file.path()))
                .collect()
        });
        for path in dropped {
            self.import_path(path);
        }

        ui::menu::show(ui, self);
        ui::toolbar::show(ui, self);
        ui::status_bar::show(ui, self);
        if self.settings.show_timeline {
            ui::timeline::show(ui, self);
        }

        if self.settings.show_project_panel {
            ui::project_panel::show(ui, self);
        }
        if self.settings.show_inspector {
            ui::inspector::show(ui, self);
        }

        self.sync_clearance_preview();

        egui::CentralPanel::no_frame().show(ui, |ui| {
            ui::viewport::show(ui, self);
        });

        let ctx = ui.ctx().clone();
        self.show_dialogs(&ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Ok(json) = serde_json::to_string(self.session.tool_library()) {
            storage.set_string("machining_tool_library", json);
        }
    }
}

fn skip_zip_entry(name: &str) -> bool {
    let name = name.replace('\\', "/");
    name.starts_with("__MACOSX/")
        || name.starts_with("._")
        || name.contains("/._")
        || name.ends_with('/')
}

fn classify_zip_entry(name: &str, text: &str) -> Option<SourceFormat> {
    let extension = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(
        extension.as_str(),
        "txt" | "md" | "pdf" | "html" | "htm" | "csv" | "json" | "xml"
    ) {
        // .txt is in the Excellon extension list, but ZIPs often contain
        // ordering instructions. Only accept it when it looks like Excellon.
        let head = text.trim_start_matches('\u{feff}').trim_start();
        if extension == "txt" && head.starts_with("M48") {
            return Some(SourceFormat::Excellon);
        }
        return None;
    }
    if let Some(format) = SourceFormat::from_path(Path::new(name)) {
        return Some(format);
    }
    let head = text.trim_start_matches('\u{feff}').trim_start();
    if head.starts_with("%FS") || (head.starts_with("G04") && head.contains("%MO")) {
        return Some(SourceFormat::Gerber);
    }
    if head.starts_with("M48") {
        return Some(SourceFormat::Excellon);
    }
    None
}

pub(crate) fn dialog_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::PANEL_RAISED)
        .stroke(Stroke::new(1.0, theme::BORDER_STRONG))
        .corner_radius(10)
        .inner_margin(egui::Margin::same(18))
}

fn shortcut_section(ui: &mut egui::Ui, title: &str) {
    widgets::section_label(ui, title);
    ui.add_space(3.0);
}

fn shortcut_row(ui: &mut egui::Ui, keys: &str, action: &str) {
    ui.horizontal(|ui| {
        egui::Frame::new()
            .fill(theme::CANVAS)
            .stroke(Stroke::new(1.0, theme::BORDER))
            .corner_radius(4)
            .inner_margin(egui::Margin::symmetric(6, 2))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(keys)
                        .size(10.5)
                        .monospace()
                        .color(theme::COPPER),
                );
            });
        ui.label(RichText::new(action).size(11.5).color(theme::TEXT_MUTED));
    });
}

fn metric_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    suffix: &str,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(11.0).color(theme::TEXT_MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::DragValue::new(value)
                    .range(range)
                    .speed(0.1)
                    .max_decimals(3)
                    .suffix(suffix),
            );
        });
    });
}

fn file_dialog(title: &str, filter: &str, extensions: &[&str]) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(title)
        .add_filter(filter, extensions)
        .pick_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_entries_are_classified_by_extension_and_content() {
        assert_eq!(
            classify_zip_entry("Gerber_TopLayer.GTL", "G04*%"),
            Some(SourceFormat::Gerber)
        );
        assert_eq!(
            classify_zip_entry("Gerber_DocumentLayer.GDL", "G04*%"),
            Some(SourceFormat::Gerber)
        );
        assert_eq!(
            classify_zip_entry("Drill_PTH_Through.DRL", "M48*"),
            Some(SourceFormat::Excellon)
        );
        assert_eq!(
            classify_zip_entry("board.pho", "%FSLAX46Y46*%\n%MOMM*%"),
            Some(SourceFormat::Gerber)
        );
        assert_eq!(
            classify_zip_entry("How-to-order-PCB.txt", "How to Order PCB"),
            None
        );
        assert_eq!(
            classify_zip_entry("drill.txt", "M48\nMETRIC\n"),
            Some(SourceFormat::Excellon)
        );
    }

    #[test]
    fn macos_metadata_entries_are_skipped() {
        assert!(skip_zip_entry("__MACOSX/._Gerber.GTL"));
        assert!(skip_zip_entry("._Gerber.GTL"));
        assert!(!skip_zip_entry("Gerber_TopLayer.GTL"));
    }
}
