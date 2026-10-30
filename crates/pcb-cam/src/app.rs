//! Application state and top-level egui layout.

use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::Duration;

use eframe::egui::{self, Align2, Button, Key, Modifiers, RichText, Stroke, Vec2, vec2};
use pcb_cam_core::{
    ContourSide, CopperGeometry, EngraveOperation, EngraveSide, FlipAxes, GerberLayer,
    GerberRegion, LayerGeometry, LayerId, OperationId, OperationKind, Path as CamPath, Point,
    Project, SourceFormat, SourceToolInfo, Task, Tool, ToolId, ToolKind, ToolLibrary, Unit,
    generate_all_gcode, generate_task_gcode, load_project, save_project,
};
use pcb_cam_engine::{Session, find_command};
use serde_json::{Value, json};

use crate::preview::{PreviewMode, PreviewSession};
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
pub(crate) const SHORTCUT_IMPORT: &str = if cfg!(target_os = "macos") {
    "Cmd+Shift+O"
} else {
    "Ctrl+Shift+O"
};
pub(crate) const SHORTCUT_SETTINGS: &str = if cfg!(target_os = "macos") {
    "Cmd+,"
} else {
    "Ctrl+,"
};
pub(crate) const SHORTCUT_SAVE: &str = if cfg!(target_os = "macos") {
    "Cmd+S"
} else {
    "Ctrl+S"
};
pub(crate) const SHORTCUT_SAVE_AS: &str = if cfg!(target_os = "macos") {
    "Cmd+Shift+S"
} else {
    "Ctrl+Shift+S"
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

/// A draggable project flip axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlipAxis {
    /// Vertical axis at `FlipAxes::x`, moved left/right.
    X,
    /// Horizontal axis at `FlipAxes::y`, moved up/down.
    Y,
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
        stepdown: f64,
        retract: f64,
        feed: f64,
        plunge: f64,
        spindle: u32,
        bridge_length: f64,
        bridge_start_depth: f64,
        bridge_count: u32,
    },
    Engrave {
        side: EngraveSide,
        outline_offset: f64,
        offset: f64,
        clearance_width: f64,
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
                stepdown: cutout.stepdown_mm,
                retract: cutout.retract_mm,
                feed: cutout.feed_mm_min,
                plunge: cutout.plunge_mm_min,
                spindle: cutout.spindle_rpm,
                bridge_length: cutout.bridge_length_mm,
                bridge_start_depth: cutout.bridge_start_depth_mm,
                bridge_count: cutout.bridge_count,
            },
            OperationKind::Engrave(engrave) => TaskDraftKind::Engrave {
                side: engrave.side,
                outline_offset: engrave.outline_offset_mm,
                offset: engrave.offset_mm,
                clearance_width: engrave.clearance_width_mm,
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
                stepdown,
                retract,
                feed,
                plunge,
                spindle,
                bridge_length,
                bridge_start_depth,
                bridge_count,
            } => (
                "task.update_cutout",
                json!({
                    "id": self.id.value(),
                    "name": self.name,
                    "tool": self.tool.value(),
                    "side": side.id(),
                    "offset": offset,
                    "depth": depth,
                    "stepdown": stepdown,
                    "retract": retract,
                    "feed": feed,
                    "plunge": plunge,
                    "spindle": spindle,
                    "bridge_length": bridge_length,
                    "bridge_start_depth": bridge_start_depth,
                    "bridge_count": bridge_count,
                }),
            ),
            TaskDraftKind::Engrave {
                side,
                outline_offset,
                offset,
                clearance_width,
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
                    "clearance_width": clearance_width,
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
    clearance_width_bits: u64,
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

#[derive(Debug)]
struct ClearanceJob {
    generation: u64,
    key: ClearanceKey,
    engrave: EngraveOperation,
    gerber: GerberLayer,
    boundary: Option<CamPath>,
    copper: Option<Arc<CopperGeometry>>,
}

#[derive(Debug)]
struct ClearanceWorker {
    slot: Arc<(Mutex<Option<ClearanceJob>>, Condvar)>,
    ready: mpsc::Receiver<(u64, ClearanceKey, Vec<CamPath>)>,
}

impl ClearanceWorker {
    fn new() -> Self {
        let slot = Arc::new((Mutex::new(None), Condvar::new()));
        let worker_slot = Arc::clone(&slot);
        let (sender, ready) = mpsc::channel();
        std::thread::Builder::new()
            .name("clearance-preview".to_owned())
            .spawn(move || {
                loop {
                    let job: ClearanceJob = {
                        let (lock, wake) = &*worker_slot;
                        let mut pending = lock.lock().expect("clearance preview lock");
                        while pending.is_none() {
                            pending = wake.wait(pending).expect("clearance preview wait");
                        }
                        pending.take().expect("clearance preview job")
                    };
                    let paths = job.engrave.toolpaths_with_copper(
                        &job.gerber,
                        job.boundary.as_ref(),
                        job.copper.as_deref(),
                    );
                    if sender.send((job.generation, job.key, paths)).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn clearance preview worker");
        Self { slot, ready }
    }

    fn submit(&self, job: ClearanceJob) {
        let (lock, wake) = &*self.slot;
        *lock.lock().expect("clearance preview lock") = Some(job);
        wake.notify_one();
    }
}

/// Editable absolute G-code zero point.
#[derive(Debug, Clone, Copy)]
pub(crate) struct OriginDialog {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

/// Settings window section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SettingsSection {
    #[default]
    Appearance,
    Machining,
}

/// Deferred action waiting for unsaved-change confirmation.
#[derive(Debug, Clone)]
pub(crate) enum PendingAction {
    Quit,
    NewProject,
    OpenProject(PathBuf),
    LoadDemo,
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
    pub(crate) settings_open: bool,
    pub(crate) settings_section: SettingsSection,
    pub(crate) appearance: theme::AppearanceSettings,
    pub(crate) task_draft: Option<TaskDraft>,
    pub(crate) tool_editor: Option<ToolEditor>,
    clearance_preview: Option<ClearancePreview>,
    clearance_requested: Option<ClearanceKey>,
    clearance_generation: u64,
    clearance_active_generation: Option<u64>,
    clearance_worker: ClearanceWorker,
    render_cache: HashMap<LayerId, Arc<Vec<GerberRegion>>>,
    copper_cache: HashMap<LayerId, Arc<CopperGeometry>>,
    pub(crate) fit_requested: bool,
    pub(crate) origin_pick: bool,
    pub(crate) origin_dialog: Option<OriginDialog>,
    pub(crate) origin_drag: bool,
    pub(crate) origin_preview: Option<Point>,
    pub(crate) flip_axis_drag: Option<FlipAxis>,
    pub(crate) flip_axes_preview: Option<FlipAxes>,
    pub(crate) flip_axes_dialog: Option<FlipAxes>,
    project_path: Option<PathBuf>,
    pub(crate) pending_action: Option<PendingAction>,
    allow_close: bool,
    pub(crate) recent_projects: Vec<PathBuf>,
    window_title: String,
    /// G-code preview screen, when open.
    pub(crate) preview: Option<PreviewSession>,
}

impl PbcCam {
    /// Build the application and install the PCB-CAM style.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let appearance: theme::AppearanceSettings = cc
            .storage
            .and_then(|storage| storage.get_string("appearance_settings"))
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        theme::apply(&cc.egui_ctx, &appearance);
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

        let recent_projects: Vec<PathBuf> = cc
            .storage
            .and_then(|storage| storage.get_string("recent_projects"))
            .and_then(|json| serde_json::from_str::<Vec<PathBuf>>(&json).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|path| path.is_file())
            .collect();

        Self {
            session,
            view: ViewState::default(),
            settings: ViewSettings::default(),
            selection: None,
            status: "Ready — import Gerber and Excellon files to begin".to_owned(),
            status_error: false,
            about_open: false,
            shortcuts_open: false,
            settings_open: false,
            settings_section: SettingsSection::Appearance,
            appearance,
            task_draft: None,
            tool_editor: None,
            clearance_preview: None,
            clearance_requested: None,
            clearance_generation: 0,
            clearance_active_generation: None,
            clearance_worker: ClearanceWorker::new(),
            render_cache: HashMap::new(),
            copper_cache: HashMap::new(),
            fit_requested: false,
            origin_pick: false,
            origin_dialog: None,
            origin_drag: false,
            origin_preview: None,
            flip_axis_drag: None,
            flip_axes_preview: None,
            flip_axes_dialog: None,
            project_path: None,
            pending_action: None,
            allow_close: false,
            recent_projects,
            window_title: WINDOW_TITLE.to_owned(),
            preview: None,
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
            self.project_path = None;
            self.origin_pick = false;
            self.origin_dialog = None;
            self.origin_drag = false;
            self.origin_preview = None;
            self.reset_flip_axes_interaction();
            self.invalidate_geometry_caches();
            self.view.reset();
            self.fit_requested = false;
            self.set_status("New project created");
        }
    }

    pub(crate) fn request_quit(&mut self, ctx: &egui::Context) {
        self.request_action(PendingAction::Quit, ctx);
    }

    pub(crate) fn request_new_project(&mut self, ctx: &egui::Context) {
        self.request_action(PendingAction::NewProject, ctx);
    }

    pub(crate) fn request_load_demo(&mut self, ctx: &egui::Context) {
        self.request_action(PendingAction::LoadDemo, ctx);
    }

    pub(crate) fn request_open_project(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.request_action(PendingAction::OpenProject(path), ctx);
    }

    fn request_action(&mut self, action: PendingAction, ctx: &egui::Context) {
        if self.session.dirty() {
            self.pending_action = Some(action);
            self.origin_dialog = None;
            self.origin_pick = false;
            self.origin_drag = false;
            self.origin_preview = None;
            self.reset_flip_axes_interaction();
        } else {
            self.apply_action(action, ctx);
        }
    }

    fn apply_action(&mut self, action: PendingAction, ctx: &egui::Context) {
        self.pending_action = None;
        match action {
            PendingAction::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            PendingAction::NewProject => self.new_project(),
            PendingAction::OpenProject(path) => {
                self.open_project(path);
            }
            PendingAction::LoadDemo => self.load_demo(),
        }
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        let close_requested = ctx.input(|input| input.viewport().close_requested());
        if !close_requested || self.allow_close {
            return;
        }
        if self.pending_action.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            return;
        }
        if self.session.dirty() {
            self.pending_action = Some(PendingAction::Quit);
            self.origin_dialog = None;
            self.origin_pick = false;
            self.origin_drag = false;
            self.origin_preview = None;
            self.reset_flip_axes_interaction();
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
    }

    pub(crate) fn add_recent_project(&mut self, path: &Path) {
        self.recent_projects.retain(|recent| recent != path);
        self.recent_projects.insert(0, path.to_path_buf());
        self.recent_projects.truncate(10);
    }

    pub(crate) fn clear_recent_projects(&mut self) {
        self.recent_projects.clear();
    }

    pub(crate) fn undo(&mut self) {
        if self.run_command("edit.undo", json!({})).is_some() {
            self.task_draft = None;
            self.invalidate_geometry_caches();
            self.set_status("Undo");
        }
    }

    pub(crate) fn redo(&mut self) {
        if self.run_command("edit.redo", json!({})).is_some() {
            self.task_draft = None;
            self.invalidate_geometry_caches();
            self.set_status("Redo");
        }
    }

    pub(crate) fn open_origin_dialog(&mut self) {
        let origin = self.project().gcode_origin();
        self.origin_pick = false;
        self.reset_flip_axes_interaction();
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

    pub(crate) fn flip_selected_layer(&mut self, horizontal: bool, vertical: bool) {
        let Some(Selection::Layer(id)) = self.selection else {
            self.set_error("Select a layer to flip");
            return;
        };
        self.flip_layers(Some(id), horizontal, vertical);
    }

    pub(crate) fn flip_layer(&mut self, id: LayerId, horizontal: bool, vertical: bool) {
        self.flip_layers(Some(id), horizontal, vertical);
    }

    pub(crate) fn flip_all_layers(&mut self, horizontal: bool, vertical: bool) {
        self.flip_layers(None, horizontal, vertical);
    }

    fn flip_layers(&mut self, layer: Option<LayerId>, horizontal: bool, vertical: bool) {
        let layer_name = layer
            .and_then(|id| self.project().layer(id))
            .map(|layer| layer.name().to_owned());
        let mut params = json!({
            "horizontal": horizontal,
            "vertical": vertical,
        });
        if let Some(id) = layer {
            params["id"] = json!(id.value());
        } else {
            params["all"] = json!(true);
        }

        let Some(value) = self.run_command("layer.flip", params) else {
            return;
        };
        self.invalidate_geometry_caches();

        let count = value.get("count").and_then(Value::as_u64).unwrap_or(0);
        let direction = flip_direction(horizontal, vertical);
        let message = match layer_name {
            Some(name) => format!("Flipped '{name}' {direction} around the flip axes"),
            None => format!(
                "Flipped {count} layer{} {direction} around the flip axes",
                if count == 1 { "" } else { "s" }
            ),
        };
        self.set_status(message);
    }

    pub(crate) fn open_flip_axes_dialog(&mut self) {
        let axes = match self.project().flip_axes() {
            Some(axes) => axes,
            None => match self.project().mirror_center() {
                Some(center) => FlipAxes::new(center.x, center.y),
                None => {
                    self.set_error("Import a layer first");
                    return;
                }
            },
        };
        self.origin_dialog = None;
        self.origin_pick = false;
        self.origin_drag = false;
        self.origin_preview = None;
        self.flip_axes_dialog = Some(axes);
        self.flip_axis_drag = None;
        self.flip_axes_preview = None;
    }

    pub(crate) fn set_flip_axes(&mut self, axes: FlipAxes) {
        if self.project().flip_axes() == Some(axes) {
            self.reset_flip_axes_interaction();
            return;
        }
        if self
            .run_command("project.set_flip_axes", json!({"x": axes.x, "y": axes.y}))
            .is_some()
        {
            self.invalidate_geometry_caches();
            self.reset_flip_axes_interaction();
            self.set_status(format!(
                "Flip axes: X {:.3} mm · Y {:.3} mm",
                axes.x, axes.y
            ));
        }
    }

    fn reset_flip_axes_interaction(&mut self) {
        self.flip_axis_drag = None;
        self.flip_axes_preview = None;
        self.flip_axes_dialog = None;
    }

    pub(crate) fn open_project_dialog(&mut self, ctx: &egui::Context) {
        let Some(path) = file_dialog("Open Project", "PCB-CAM project", &["pcbcam"]) else {
            return;
        };
        self.request_open_project(path, ctx);
    }

    pub(crate) fn open_project(&mut self, path: PathBuf) -> bool {
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                self.set_error(format!("Cannot read {}: {error}", path.display()));
                return false;
            }
        };
        let mut loaded = match load_project(&text) {
            Ok(loaded) => loaded,
            Err(error) => {
                self.set_error(format!("Cannot open {}: {error}", path.display()));
                return false;
            }
        };
        let mut library = self.session.tool_library().clone();
        for tool in loaded.tools.tools() {
            match library.tool(tool.id()) {
                Some(existing) if existing == tool => {}
                _ => {
                    let id = library.add(
                        tool.name().to_owned(),
                        tool.kind(),
                        tool.diameter_mm(),
                        tool.feed_mm_min(),
                        tool.plunge_mm_min(),
                        tool.spindle_rpm(),
                    );
                    loaded.project.remap_tool(tool.id(), id);
                }
            }
        }
        self.session.set_tool_library(library);
        self.session.load_project(loaded.project);
        self.project_path = Some(path.clone());
        self.add_recent_project(&path);
        self.selection = None;
        self.task_draft = None;
        self.origin_pick = false;
        self.origin_dialog = None;
        self.origin_drag = false;
        self.origin_preview = None;
        self.reset_flip_axes_interaction();
        self.invalidate_geometry_caches();
        self.fit_requested = true;
        self.set_status(format!("Opened {}", path.display()));
        true
    }

    pub(crate) fn save_project(&mut self) -> bool {
        if let Some(path) = self.project_path.clone() {
            self.save_project_to(path)
        } else {
            self.save_project_as_dialog()
        }
    }

    pub(crate) fn save_project_as_dialog(&mut self) -> bool {
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
        let Some(path) = rfd::FileDialog::new()
            .set_title("Save Project")
            .add_filter("PCB-CAM project", &["pcbcam"])
            .set_file_name(format!("{base}.pcbcam"))
            .save_file()
        else {
            return false;
        };
        let path = if path.extension().is_none() {
            path.with_extension("pcbcam")
        } else {
            path
        };
        self.save_project_to(path)
    }

    pub(crate) fn save_project_to(&mut self, path: PathBuf) -> bool {
        let text = match save_project(self.project(), self.session.tool_library()) {
            Ok(text) => text,
            Err(error) => {
                self.set_error(format!("Cannot save project: {error}"));
                return false;
            }
        };
        if let Err(error) = std::fs::write(&path, text) {
            self.set_error(format!("Cannot write {}: {error}", path.display()));
            return false;
        }
        self.project_path = Some(path.clone());
        self.add_recent_project(&path);
        self.session.mark_clean();
        self.set_status(format!("Saved {}", path.display()));
        true
    }

    pub(crate) fn import_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Import fabrication files")
            .add_filter(
                "PCB fabrication files",
                &[
                    "zip", "gbr", "ger", "gtl", "gbl", "gts", "gbs", "gto", "gbo", "gko", "gdl",
                    "gm1", "gtp", "gbp", "gba", "gta", "drl", "xln", "exc", "nc", "txt",
                ],
            )
            .add_filter("All files", &["*"])
            .pick_file()
        else {
            return;
        };
        self.import_path(path);
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
            self.project_path = None;
            self.invalidate_geometry_caches();
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
        if is_zip_file(&path) {
            self.import_zip(path);
            return;
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                self.set_error(format!("Cannot read {}: {error}", path.display()));
                return;
            }
        };
        let preferred = sniff_source_format(&text).or_else(|| SourceFormat::from_path(&path));
        let formats = match preferred {
            Some(format) => {
                let other = match format {
                    SourceFormat::Gerber => SourceFormat::Excellon,
                    SourceFormat::Excellon => SourceFormat::Gerber,
                };
                [Some(format), Some(other)]
            }
            None => [Some(SourceFormat::Gerber), Some(SourceFormat::Excellon)],
        };
        for format in formats.into_iter().flatten() {
            let id = match format {
                SourceFormat::Gerber => "file.import_gerber",
                SourceFormat::Excellon => "file.import_excellon",
            };
            if let Some(value) = self.run_command(id, json!({"path": path})) {
                self.apply_import_result(format, &value);
                return;
            }
        }
        self.set_error(format!(
            "Could not import {} as Gerber or Excellon",
            path.file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_default()
        ));
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

    /// Generate the selected task and enter the G-code preview.
    pub(crate) fn open_task_gcode_preview(&mut self) {
        let Some(Selection::Task(id)) = self.selection else {
            self.set_error("Select a task to preview");
            return;
        };
        self.open_task_preview_for(id);
    }

    fn open_task_preview_for(&mut self, id: OperationId) {
        let mode = PreviewMode::Task(id);
        let Some(task) = self.project().task(id) else {
            self.preview = Some(PreviewSession::with_error(mode, "Task no longer exists."));
            self.set_error("Task no longer exists");
            return;
        };
        let name = task.name().to_owned();
        let result = if !task.enabled() {
            Err(format!("Task `{name}` is disabled."))
        } else {
            let warnings = self.task_warnings(id);
            if warnings.is_empty() {
                generate_task_gcode(self.project(), self.session.tool_library(), task)
                    .map_err(|error| format!("{name}: {error}"))
            } else {
                Err(format!("{name}: {}", warnings.join(" ")))
            }
        };
        match result {
            Ok(program) => {
                self.preview = Some(PreviewSession::new(mode, program));
                self.set_status(format!("G-code preview · {name}"));
            }
            Err(message) => {
                self.preview = Some(PreviewSession::with_error(mode, message.clone()));
                self.set_error(message);
            }
        }
    }

    /// Generate all enabled tasks and enter the G-code preview.
    pub(crate) fn open_all_gcode_preview(&mut self) {
        let mode = PreviewMode::All;
        if self.project().enabled_task_count() == 0 {
            let message = "Add and enable a task first";
            self.preview = Some(PreviewSession::with_error(mode, message));
            self.set_error(message);
            return;
        }
        let blocked: Vec<String> = self
            .project()
            .tasks()
            .iter()
            .filter(|task| task.enabled())
            .filter_map(|task| {
                let warnings = self.task_warnings(task.id());
                (!warnings.is_empty()).then(|| format!("{}: {}", task.name(), warnings.join(" ")))
            })
            .collect();
        let result = if blocked.is_empty() {
            generate_all_gcode(self.project(), self.session.tool_library())
                .map_err(|error| format!("G-code generation failed: {error}"))
        } else {
            Err(blocked.join("\n"))
        };
        match result {
            Ok(program) => {
                let tasks = program.task_count;
                self.preview = Some(PreviewSession::new(mode, program));
                self.set_status(format!("G-code preview · {tasks} enabled tasks"));
            }
            Err(message) => {
                self.preview = Some(PreviewSession::with_error(mode, message.clone()));
                self.set_error(message);
            }
        }
    }

    /// Close the preview and return to the workspace.
    pub(crate) fn close_preview(&mut self) {
        if self.preview.take().is_some() {
            self.set_status("Closed G-code preview");
        }
    }

    /// Regenerate the preview after returning from an edit.
    pub(crate) fn regenerate_preview(&mut self) {
        let Some(mode) = self.preview.as_ref().map(|preview| preview.mode) else {
            return;
        };
        match mode {
            PreviewMode::Task(id) => self.open_task_preview_for(id),
            PreviewMode::All => self.open_all_gcode_preview(),
        }
    }

    /// Export the program currently shown in the preview.
    pub(crate) fn export_preview_gcode(&mut self) {
        let Some(mode) = self.preview.as_ref().map(|preview| preview.mode) else {
            return;
        };
        if !self.preview.as_ref().is_some_and(PreviewSession::is_ready) {
            let message = "Preview is not ready to export";
            self.set_error(message);
            if let Some(preview) = &mut self.preview {
                preview.set_notice(message, true);
            }
            return;
        }
        let suffix = match mode {
            PreviewMode::Task(id) => self.project().task(id).map_or_else(
                || "-task".to_owned(),
                |task| format!("-{}", task.kind_label()),
            ),
            PreviewMode::All => "-all".to_owned(),
        };
        let Some(path) = self.choose_gcode_path(&suffix) else {
            return;
        };
        let value = match mode {
            PreviewMode::Task(id) => self.run_command(
                "file.export_task_gcode",
                json!({"path": path, "task": id.value()}),
            ),
            PreviewMode::All => self.run_command("file.export_gcode", json!({"path": path})),
        };
        if let Some(value) = value {
            self.report_export(&value, if mode.is_all() { "Combined" } else { "Task" });
            let message = format!("Exported {}", path.display());
            if let Some(preview) = &mut self.preview {
                preview.exported_path = Some(path);
                preview.set_notice(message, false);
            }
        } else {
            let message = self.status.clone();
            if let Some(preview) = &mut self.preview {
                preview.set_notice(message, true);
            }
        }
    }

    fn tick_preview(&mut self, ctx: &egui::Context) {
        let Some(preview) = &mut self.preview else {
            return;
        };
        if preview.playing {
            let dt = ctx.input(|input| input.stable_dt).min(0.1);
            preview.advance(f64::from(dt));
            ctx.request_repaint();
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
        self.project_path = None;
        self.invalidate_geometry_caches();
        self.origin_pick = false;
        self.origin_dialog = None;
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
        self.project_path = None;
        self.selection = task_ids
            .get(1)
            .or_else(|| task_ids.last())
            .copied()
            .map(Selection::Task);
        self.fit_requested = true;
        self.set_status("Demo loaded — 4 layers, 4 tasks, source filters and machining tools");
    }

    fn show_unsaved_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending_action.clone() else {
            return;
        };
        let mut open = true;
        let mut save = false;
        let mut discard = false;
        let mut cancel = false;
        let target = match &action {
            PendingAction::Quit => "before quitting",
            PendingAction::NewProject => "before creating a new project",
            PendingAction::OpenProject(_) => "before opening another project",
            PendingAction::LoadDemo => "before loading the demo",
        };

        egui::Window::new("Unsaved changes")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(380.0)
            .frame(dialog_frame())
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!(
                        "Project `{}` has unsaved changes.",
                        self.project().name()
                    ))
                    .size(12.0)
                    .color(theme::text()),
                );
                ui.label(
                    RichText::new(format!("Save them {target}?"))
                        .size(11.0)
                        .color(theme::text_muted()),
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        save = true;
                    }
                    if ui.button("Discard").clicked() {
                        discard = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });

        if save {
            if self.save_project() {
                self.apply_action(action, ctx);
            }
        } else if discard {
            self.apply_action(action, ctx);
        } else if cancel || !open {
            self.pending_action = None;
        }
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
                        .color(theme::text_muted()),
                );
                ui.add_space(8.0);
                egui::Grid::new("gcode_origin_grid")
                    .num_columns(2)
                    .spacing(vec2(10.0, 6.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new("X").color(theme::text_muted()));
                        ui.add(
                            egui::DragValue::new(&mut dialog.x)
                                .speed(0.1)
                                .fixed_decimals(3)
                                .suffix(" mm"),
                        );
                        ui.end_row();
                        ui.label(RichText::new("Y").color(theme::text_muted()));
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
                        .color(theme::text_faint()),
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

    fn show_flip_axes_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut axes) = self.flip_axes_dialog else {
            return;
        };
        let mut open = true;
        let mut submit = false;
        let mut cancel = false;

        egui::Window::new("Flip axes")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(330.0)
            .frame(dialog_frame())
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        "Persistent axes used by every layer flip. They are created on the first flip and do not move on their own.",
                    )
                    .size(11.0)
                    .color(theme::text_muted()),
                );
                ui.add_space(8.0);
                egui::Grid::new("flip_axes_grid")
                    .num_columns(2)
                    .spacing(vec2(10.0, 6.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new("Vertical X").color(theme::text_muted()));
                        ui.add(
                            egui::DragValue::new(&mut axes.x)
                                .speed(0.1)
                                .fixed_decimals(3)
                                .suffix(" mm"),
                        )
                        .on_hover_text("Vertical axis: mirrors left/right");
                        ui.end_row();
                        ui.label(RichText::new("Horizontal Y").color(theme::text_muted()));
                        ui.add(
                            egui::DragValue::new(&mut axes.y)
                                .speed(0.1)
                                .fixed_decimals(3)
                                .suffix(" mm"),
                        )
                        .on_hover_text("Horizontal axis: mirrors top/bottom");
                        ui.end_row();
                    });
                ui.add_space(10.0);
                ui.label(
                    RichText::new(
                        "Drag the dashed axes on the canvas to move them; flipped layers follow the new positions.",
                    )
                    .size(10.5)
                    .color(theme::text_faint()),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Set").clicked() {
                        submit = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });

        if submit {
            self.set_flip_axes(axes);
            self.flip_axes_dialog = None;
        } else if cancel || !open {
            self.flip_axes_dialog = None;
        } else {
            self.flip_axes_dialog = Some(axes);
        }
    }

    fn sync_window_title(&mut self, ctx: &egui::Context) {
        let dirty = if self.session.dirty() { " *" } else { "" };
        let preview = if self.preview.is_some() {
            " — G-code preview"
        } else {
            ""
        };
        let title = format!("{APP_NAME} — {}{dirty}{preview}", self.project().name());
        if title != self.window_title {
            self.window_title.clone_from(&title);
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // Cmd+Q is the one shortcut that must work even while editing text.
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Q)) {
            self.request_quit(ctx);
            return;
        }

        // When a text field (including a DragValue in edit mode) has focus,
        // every key belongs to that field. In particular, the macOS Delete key
        // sends Backspace and must never trigger "remove selection".
        if ctx.text_edit_focused() {
            return;
        }

        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            if self.about_open || self.shortcuts_open {
                self.about_open = false;
                self.shortcuts_open = false;
            } else if self.preview.is_some() && self.pending_action.is_none() {
                self.close_preview();
                return;
            } else {
                self.task_draft = None;
                self.pending_action = None;
                self.origin_dialog = None;
                self.origin_preview = None;
                self.origin_drag = false;
                self.reset_flip_axes_interaction();
                if self.origin_pick {
                    self.origin_pick = false;
                    self.set_status("G-code 0-point pick cancelled");
                }
            }
        }

        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F1)) {
            self.about_open = true;
            return;
        }

        if self.about_open || self.shortcuts_open {
            return;
        }

        if self.preview.is_some() {
            self.handle_preview_shortcuts(ctx);
            return;
        }

        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::O)) {
            self.import_dialog();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::S)) {
            self.save_project_as_dialog();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::S)) {
            self.save_project();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Comma)) {
            self.settings_open = true;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::O)) {
            self.open_project_dialog(ctx);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::N)) {
            self.request_new_project(ctx);
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
            self.open_all_gcode_preview();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::E)) {
            self.open_task_gcode_preview();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F)) {
            self.fit_view();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::G)) {
            self.settings.show_grid = !self.settings.show_grid;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::H)) {
            self.flip_selected_layer(true, false);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::V)) {
            self.flip_selected_layer(false, true);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete))
            || ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Backspace))
        {
            self.remove_selection();
        }
    }

    fn handle_preview_shortcuts(&mut self, ctx: &egui::Context) {
        let mut export = false;
        let mut regenerate = false;
        if let Some(preview) = &mut self.preview {
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Space)) {
                preview.toggle_play();
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::ArrowLeft)) {
                preview.step_lines(-10);
            } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowLeft)) {
                preview.step_lines(-1);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::ArrowRight)) {
                preview.step_lines(10);
            } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowRight)) {
                preview.step_lines(1);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Home)) {
                preview.seek_time(0.0);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::End)) {
                preview.seek_time(f64::INFINITY);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Plus))
                || ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::Equals))
            {
                preview.nudge_speed(1);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Minus)) {
                preview.nudge_speed(-1);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F)) {
                preview.fit_pending = true;
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::E))
                || ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::E))
            {
                export = true;
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::R)) {
                regenerate = true;
            }
        }
        if export {
            self.export_preview_gcode();
        }
        if regenerate {
            self.regenerate_preview();
        }
    }

    fn show_dialogs(&mut self, ctx: &egui::Context) {
        ui::settings::show(ctx, self);
        self.show_unsaved_dialog(ctx);
        self.show_origin_dialog(ctx);
        self.show_flip_axes_dialog(ctx);

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
                                    .color(theme::text_muted()),
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
                        .color(theme::text()),
                    );
                    ui.add_space(10.0);

                    widgets::field_row(ui, "Version", VERSION);
                    widgets::field_row(ui, "License", "MIT");
                    widgets::field_row(ui, "Built with", "Rust + egui + gerber_parser");
                    widgets::field_row(ui, "Milestone", "G-code preview + machine simulation");
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
                                .color(theme::text_faint()),
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
                    shortcut_row(ui, SHORTCUT_OPEN, "Open project");
                    shortcut_row(ui, SHORTCUT_SAVE, "Save project");
                    shortcut_row(ui, SHORTCUT_SAVE_AS, "Save project as");
                    shortcut_row(ui, SHORTCUT_SETTINGS, "Settings");
                    shortcut_row(ui, SHORTCUT_IMPORT, "Import Gerber, Excellon or ZIP");
                    shortcut_row(ui, "Cmd/Ctrl+E", "Preview selected task G-code");
                    shortcut_row(ui, "Cmd/Ctrl+Shift+E", "Preview all tasks G-code");
                    shortcut_row(ui, SHORTCUT_QUIT, "Quit");

                    ui.add_space(10.0);
                    shortcut_section(ui, "Edit");
                    shortcut_row(ui, SHORTCUT_UNDO, "Undo");
                    shortcut_row(ui, SHORTCUT_REDO, "Redo");
                    shortcut_row(ui, "Delete", "Remove selected item");

                    ui.add_space(10.0);
                    shortcut_section(ui, "Layers");
                    shortcut_row(ui, "Shift+H", "Flip selected layer horizontally (mirror X)");
                    shortcut_row(ui, "Shift+V", "Flip selected layer vertically (mirror Y)");

                    ui.add_space(10.0);
                    shortcut_section(ui, "G-code preview");
                    shortcut_row(ui, "Space", "Play or pause playback");
                    shortcut_row(ui, "← / →", "Step one command; Shift steps ten");
                    shortcut_row(ui, "Home / End", "Jump to the start or end");
                    shortcut_row(ui, "+ / -", "Change playback speed");
                    shortcut_row(ui, "Cmd/Ctrl+E", "Export the previewed program");
                    shortcut_row(ui, "Cmd/Ctrl+R", "Regenerate the preview");
                    shortcut_row(ui, "Esc", "Return to the workspace");

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
    fn sync_clearance_preview(&mut self, ctx: &egui::Context) {
        while let Ok((generation, key, paths)) = self.clearance_worker.ready.try_recv() {
            if self.clearance_active_generation == Some(generation)
                && self.clearance_requested.as_ref() == Some(&key)
            {
                self.clearance_preview = Some(ClearancePreview { key, paths });
                self.clearance_requested = None;
                self.clearance_active_generation = None;
            }
        }

        let Some(Selection::Task(id)) = self.selection else {
            self.clearance_preview = None;
            self.clearance_requested = None;
            self.clearance_active_generation = None;
            return;
        };
        let Some(task) = self.project().task(id) else {
            self.clearance_preview = None;
            self.clearance_requested = None;
            self.clearance_active_generation = None;
            return;
        };
        let OperationKind::Engrave(engrave) = task.kind() else {
            self.clearance_preview = None;
            self.clearance_requested = None;
            self.clearance_active_generation = None;
            return;
        };
        let Some(layer) = self.project().layer(engrave.layer) else {
            self.clearance_preview = None;
            self.clearance_requested = None;
            self.clearance_active_generation = None;
            return;
        };
        let LayerGeometry::Gerber(gerber) = layer.geometry() else {
            self.clearance_preview = None;
            self.clearance_requested = None;
            self.clearance_active_generation = None;
            return;
        };
        let Some(tool) = self.session.tool_library().tool(engrave.tool) else {
            self.clearance_preview = None;
            self.clearance_requested = None;
            self.clearance_active_generation = None;
            return;
        };
        let engrave = engrave.clone();
        let gerber = gerber.clone();
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
            clearance_width_bits: engrave.clearance_width_mm.to_bits(),
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
            self.clearance_requested = None;
            return;
        }
        if self.clearance_requested.as_ref() == Some(&key) {
            ctx.request_repaint_after(Duration::from_millis(30));
            return;
        }

        self.clearance_requested = Some(key.clone());
        self.clearance_generation = self.clearance_generation.wrapping_add(1);
        let generation = self.clearance_generation;
        self.clearance_active_generation = Some(generation);
        let copper = self.copper_cache.get(&key.layer).cloned();
        self.clearance_worker.submit(ClearanceJob {
            generation,
            key,
            engrave,
            gerber,
            boundary: boundary_path,
            copper,
        });
        ctx.request_repaint_after(Duration::from_millis(30));
    }

    fn invalidate_geometry_caches(&mut self) {
        self.render_cache.clear();
        self.copper_cache.clear();
        self.clearance_preview = None;
        self.clearance_requested = None;
        self.clearance_active_generation = None;
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
            if self.render_cache.contains_key(&id) && self.copper_cache.contains_key(&id) {
                continue;
            }
            let copper = self
                .project()
                .layer(id)
                .and_then(|layer| match layer.geometry() {
                    LayerGeometry::Gerber(gerber) => gerber.copper_geometry_snapshot(),
                    LayerGeometry::Excellon(_) => None,
                });
            let regions = copper
                .as_ref()
                .map_or_else(Vec::new, CopperGeometry::regions);
            if let Some(copper) = copper {
                self.copper_cache.insert(id, Arc::new(copper));
            }
            self.render_cache.insert(id, Arc::new(regions));
        }
    }

    pub(crate) fn render_regions(&self, id: LayerId) -> Option<&[GerberRegion]> {
        self.render_cache.get(&id).map(|regions| regions.as_slice())
    }

    pub(crate) fn clearance_computing(&self) -> bool {
        self.clearance_requested.is_some()
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
                ui.label(RichText::new("Type").size(11.0).color(theme::text_muted()));
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
                ui.label(RichText::new(error).size(11.0).color(theme::error()));
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
        ui.label(
            RichText::new("Spindle")
                .size(11.0)
                .color(theme::text_muted()),
        );
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
        theme::sync(ui.ctx(), &self.appearance);
        self.handle_close_request(ui.ctx());
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

        if self.preview.is_some() {
            if !dropped.is_empty() {
                self.set_status("Close the G-code preview before importing files");
            }
            self.tick_preview(ui.ctx());
            ui::menu::show(ui, self);
            ui::status_bar::show(ui, self);
            ui::preview::show(ui, self);
            let ctx = ui.ctx().clone();
            self.show_dialogs(&ctx);
            return;
        }

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

        self.sync_render_cache();
        self.sync_clearance_preview(ui.ctx());

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
        if let Ok(json) = serde_json::to_string(&self.recent_projects) {
            storage.set_string("recent_projects", json);
        }
        if let Ok(json) = serde_json::to_string(&self.appearance) {
            storage.set_string("appearance_settings", json);
        }
    }
}

fn sniff_source_format(text: &str) -> Option<SourceFormat> {
    let upper = text.to_ascii_uppercase();
    let gerber_markers = ["%FS", "%MO", "%ADD", "G04", "M02*", "D10*", "D01*"];
    let excellon_markers = ["M48", "METRIC", "INCH,", "INCH\n", "T1C", "T01C"];
    let gerber = gerber_markers.iter().any(|marker| upper.contains(marker));
    let excellon = excellon_markers.iter().any(|marker| upper.contains(marker));
    match (gerber, excellon) {
        (true, false) => Some(SourceFormat::Gerber),
        (false, true) => Some(SourceFormat::Excellon),
        _ => None,
    }
}

fn is_zip_file(path: &Path) -> bool {
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        return true;
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    bytes.starts_with(b"PK\x03\x04")
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
        .fill(theme::panel_raised())
        .stroke(Stroke::new(1.0, theme::border_strong()))
        .corner_radius(10)
        .inner_margin(egui::Margin::same(18))
}

fn shortcut_section(ui: &mut egui::Ui, title: &str) {
    widgets::section_label(ui, title);
    ui.add_space(3.0);
}

fn flip_direction(horizontal: bool, vertical: bool) -> &'static str {
    match (horizontal, vertical) {
        (true, false) => "horizontally (mirror X)",
        (false, true) => "vertically (mirror Y)",
        (true, true) => "both ways (180°)",
        (false, false) => "",
    }
}

fn shortcut_row(ui: &mut egui::Ui, keys: &str, action: &str) {
    ui.horizontal(|ui| {
        egui::Frame::new()
            .fill(theme::canvas())
            .stroke(Stroke::new(1.0, theme::border()))
            .corner_radius(4)
            .inner_margin(egui::Margin::symmetric(6, 2))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(keys)
                        .size(10.5)
                        .monospace()
                        .color(theme::copper()),
                );
            });
        ui.label(RichText::new(action).size(11.5).color(theme::text_muted()));
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
        ui.label(RichText::new(label).size(11.0).color(theme::text_muted()));
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
    fn import_sniffs_fabrication_formats() {
        assert_eq!(
            sniff_source_format("%FSLAX46Y46*%\n%MOMM*%\nG04 test*\nM02*"),
            Some(SourceFormat::Gerber)
        );
        assert_eq!(
            sniff_source_format("M48\nMETRIC\nT1C0.8\n%\nT1\nX1.0Y2.0\nM30"),
            Some(SourceFormat::Excellon)
        );
        assert_eq!(sniff_source_format("hello"), None);
    }

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
