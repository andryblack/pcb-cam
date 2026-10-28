//! Application state and top-level egui layout.

use std::path::PathBuf;

use eframe::egui::{self, Align2, Button, Key, Modifiers, RichText, Stroke, Vec2};
use pbc_cam_core::{
    ContourSide, LayerGeometry, LayerId, OperationId, OperationKind, Project, SourceFormat, Task,
    ToolId, Unit,
};
use pbc_cam_engine::{Session, find_command};
use serde_json::{Value, json};

use crate::theme;
use crate::ui;
use crate::ui::widgets;
use crate::view::ViewState;

/// Product name used in the title bar and dialogs.
pub(crate) const APP_NAME: &str = "PBC-CAM";
/// Window title.
pub(crate) const WINDOW_TITLE: &str = "PBC-CAM — PCB Fabrication CAM";
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

/// The type-specific fields of the task editor.
#[derive(Debug, Clone)]
pub(crate) enum TaskEditorKind {
    Drill {
        depth: f64,
        retract: f64,
        feed: f64,
        plunge: f64,
        dwell: f64,
        spindle: u32,
    },
    Cutout {
        tool: f64,
        side: ContourSide,
        depth: f64,
        retract: f64,
        feed: f64,
        plunge: f64,
        spindle: u32,
    },
    Engrave {
        tool: f64,
        depth: f64,
        retract: f64,
        feed: f64,
        plunge: f64,
        spindle: u32,
    },
}

/// A modal draft of one task's parameters.
#[derive(Debug, Clone)]
pub(crate) struct TaskEditor {
    pub(crate) id: OperationId,
    pub(crate) name: String,
    pub(crate) layer: LayerId,
    pub(crate) layer_name: String,
    pub(crate) kind: TaskEditorKind,
    pub(crate) error: Option<String>,
}

impl TaskEditor {
    fn from_task(project: &Project, task: &Task) -> Option<Self> {
        let layer = project.layer(task.source_layer())?;
        let kind = match task.kind() {
            OperationKind::Drill(drill) => TaskEditorKind::Drill {
                depth: drill.depth_mm,
                retract: drill.retract_mm,
                feed: drill.feed_mm_min,
                plunge: drill.plunge_mm_min,
                dwell: drill.dwell_s,
                spindle: drill.spindle_rpm,
            },
            OperationKind::Cutout(cutout) => TaskEditorKind::Cutout {
                tool: cutout.tool_diameter_mm,
                side: cutout.side,
                depth: cutout.depth_mm,
                retract: cutout.retract_mm,
                feed: cutout.feed_mm_min,
                plunge: cutout.plunge_mm_min,
                spindle: cutout.spindle_rpm,
            },
            OperationKind::Engrave(engrave) => TaskEditorKind::Engrave {
                tool: engrave.tool_diameter_mm,
                depth: engrave.depth_mm,
                retract: engrave.retract_mm,
                feed: engrave.feed_mm_min,
                plunge: engrave.plunge_mm_min,
                spindle: engrave.spindle_rpm,
            },
        };
        Some(Self {
            id: task.id(),
            name: task.name().to_owned(),
            layer: task.source_layer(),
            layer_name: layer.name().to_owned(),
            kind,
            error: None,
        })
    }
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
    pub(crate) task_editor: Option<TaskEditor>,
    pub(crate) fit_requested: bool,
    window_title: String,
}

impl PbcCam {
    /// Build the application and install the PBC-CAM style.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        log::info!("starting {APP_NAME} {VERSION}");

        Self {
            session: Session::new(),
            view: ViewState::default(),
            settings: ViewSettings::default(),
            selection: None,
            status: "Ready — import Gerber and Excellon files to begin".to_owned(),
            status_error: false,
            about_open: false,
            shortcuts_open: false,
            task_editor: None,
            fit_requested: false,
            window_title: WINDOW_TITLE.to_owned(),
        }
    }

    pub(crate) fn project(&self) -> &pbc_cam_core::Project {
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
            Some(Selection::Tool(id)) => self.project().tool(id).is_some(),
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
            self.task_editor = None;
            self.view.reset();
            self.fit_requested = false;
            self.set_status("New project created");
        }
    }

    pub(crate) fn undo(&mut self) {
        if self.run_command("edit.undo", json!({})).is_some() {
            self.task_editor = None;
            self.set_status("Undo");
        }
    }

    pub(crate) fn redo(&mut self) {
        if self.run_command("edit.redo", json!({})).is_some() {
            self.task_editor = None;
            self.set_status("Redo");
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

    pub(crate) fn import_path(&mut self, path: PathBuf) {
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

    pub(crate) fn add_drill_task(&mut self) {
        let layer = self.drill_layer_for_new_task();
        self.add_task_with_layer("task.add_drill", layer, "Drilling");
    }

    pub(crate) fn add_cutout_task(&mut self) {
        let layer = self.cutout_layer_for_new_task();
        self.add_task_with_layer("task.add_cutout", layer, "Contour cut");
    }

    pub(crate) fn add_engrave_task(&mut self) {
        let layer = self.engrave_layer_for_new_task();
        self.add_task_with_layer("task.add_engrave", layer, "Engraving");
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
            if let Some(id) = value.get("id").and_then(Value::as_u64) {
                self.selection = Some(Selection::Task(OperationId::new(id)));
            }
            self.set_status(format!("{label} task added for {layer_name}"));
        }
    }

    pub(crate) fn can_add_drill_task(&self) -> bool {
        self.drill_layer_for_new_task().is_some()
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

    fn cutout_layer_for_new_task(&self) -> Option<LayerId> {
        if let Some(Selection::Layer(id)) = self.selection
            && self.layer_has_closed_contours(id)
        {
            return Some(id);
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
            self.task_editor = None;
            self.set_status("Removed");
        }
    }

    pub(crate) fn open_task_editor(&mut self) {
        let Some(Selection::Task(id)) = self.selection else {
            self.set_error("Select a task first");
            return;
        };
        let Some(task) = self.project().task(id) else {
            return;
        };
        self.task_editor = TaskEditor::from_task(self.project(), task);
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

        let mut last_task = None;
        for (command, params) in [
            ("task.add_drill", json!({"layer": drill, "depth": 1.6})),
            (
                "task.add_cutout",
                json!({"layer": outline, "tool": 1.0, "side": "outside", "depth": 1.8}),
            ),
            (
                "task.add_engrave",
                json!({"layer": silk, "tool": 0.3, "depth": 0.2}),
            ),
        ] {
            let Some(value) = self.run_command(command, params) else {
                return;
            };
            last_task = value
                .get("id")
                .and_then(Value::as_u64)
                .map(OperationId::new);
        }
        self.selection = last_task.map(Selection::Task);
        self.fit_requested = true;
        self.set_status(
            "Demo loaded — 4 layers, 3 tasks; G-code can be exported per task or combined",
        );
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
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.about_open = false;
            self.shortcuts_open = false;
            self.task_editor = None;
        }

        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Q)) {
            Self::quit(ctx);
            return;
        }

        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F1)) {
            self.about_open = true;
            return;
        }

        if self.about_open || self.shortcuts_open || self.task_editor.is_some() {
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
                    widgets::field_row(ui, "Milestone", "Task timeline + combined G-code");
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

        self.show_task_editor(ctx);
    }

    fn show_task_editor(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.task_editor.clone() else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        let kind_name = task_editor_kind_name(&editor.kind);

        egui::Window::new(format!("{kind_name} task"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(410.0)
            .frame(dialog_frame())
            .show(ctx, |ui| {
                widgets::section_label(ui, "Task");
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.add(
                        egui::TextEdit::singleline(&mut editor.name).desired_width(f32::INFINITY),
                    );
                });
                ui.label(
                    RichText::new(format!(
                        "{kind_name} · source: {} · #{}",
                        editor.layer_name,
                        editor.layer.value()
                    ))
                    .size(10.5)
                    .color(theme::TEXT_MUTED),
                );

                ui.add_space(10.0);
                widgets::hairline(ui);
                ui.add_space(8.0);
                widgets::section_label(ui, "Settings");
                ui.add_space(4.0);

                match &mut editor.kind {
                    TaskEditorKind::Drill {
                        depth,
                        retract,
                        feed,
                        plunge,
                        dwell,
                        spindle,
                    } => {
                        metric_row(ui, "Depth", depth, 0.01..=50.0, " mm");
                        metric_row(ui, "Retract", retract, 0.1..=100.0, " mm");
                        metric_row(ui, "Feed", feed, 1.0..=10_000.0, " mm/min");
                        metric_row(ui, "Plunge", plunge, 1.0..=5_000.0, " mm/min");
                        metric_row(ui, "Dwell", dwell, 0.0..=10.0, " s");
                        spindle_row(ui, spindle);
                    }
                    TaskEditorKind::Cutout {
                        tool,
                        side,
                        depth,
                        retract,
                        feed,
                        plunge,
                        spindle,
                    } => {
                        metric_row(ui, "Tool diameter", tool, 0.05..=10.0, " mm");
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Cutter side")
                                    .size(11.0)
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    for candidate in [
                                        ContourSide::OnLine,
                                        ContourSide::Inside,
                                        ContourSide::Outside,
                                    ] {
                                        if ui
                                            .add(Button::selectable(
                                                *side == candidate,
                                                candidate.label(),
                                            ))
                                            .clicked()
                                        {
                                            *side = candidate;
                                        }
                                    }
                                },
                            );
                        });
                        metric_row(ui, "Depth", depth, 0.01..=50.0, " mm");
                        metric_row(ui, "Retract", retract, 0.1..=100.0, " mm");
                        metric_row(ui, "Feed", feed, 1.0..=10_000.0, " mm/min");
                        metric_row(ui, "Plunge", plunge, 1.0..=5_000.0, " mm/min");
                        spindle_row(ui, spindle);
                    }
                    TaskEditorKind::Engrave {
                        tool,
                        depth,
                        retract,
                        feed,
                        plunge,
                        spindle,
                    } => {
                        metric_row(ui, "Tool diameter", tool, 0.05..=10.0, " mm");
                        metric_row(ui, "Depth", depth, 0.01..=50.0, " mm");
                        metric_row(ui, "Retract", retract, 0.1..=100.0, " mm");
                        metric_row(ui, "Feed", feed, 1.0..=10_000.0, " mm/min");
                        metric_row(ui, "Plunge", plunge, 1.0..=5_000.0, " mm/min");
                        spindle_row(ui, spindle);
                    }
                }

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
                        if ui.button("Apply").clicked() {
                            apply = true;
                        }
                    });
                });
            });

        if apply {
            let (command, params) = match &editor.kind {
                TaskEditorKind::Drill {
                    depth,
                    retract,
                    feed,
                    plunge,
                    dwell,
                    spindle,
                } => (
                    "task.update_drill",
                    json!({
                        "id": editor.id.value(),
                        "name": editor.name,
                        "depth": depth,
                        "retract": retract,
                        "feed": feed,
                        "plunge": plunge,
                        "dwell": dwell,
                        "spindle": spindle,
                    }),
                ),
                TaskEditorKind::Cutout {
                    tool,
                    side,
                    depth,
                    retract,
                    feed,
                    plunge,
                    spindle,
                } => (
                    "task.update_cutout",
                    json!({
                        "id": editor.id.value(),
                        "name": editor.name,
                        "tool": tool,
                        "side": side.id(),
                        "depth": depth,
                        "retract": retract,
                        "feed": feed,
                        "plunge": plunge,
                        "spindle": spindle,
                    }),
                ),
                TaskEditorKind::Engrave {
                    tool,
                    depth,
                    retract,
                    feed,
                    plunge,
                    spindle,
                } => (
                    "task.update_engrave",
                    json!({
                        "id": editor.id.value(),
                        "name": editor.name,
                        "tool": tool,
                        "depth": depth,
                        "retract": retract,
                        "feed": feed,
                        "plunge": plunge,
                        "spindle": spindle,
                    }),
                ),
            };
            match self.run_command(command, params) {
                Some(_) => {
                    self.task_editor = None;
                    self.set_status("Task updated");
                }
                None => {
                    editor.error = Some(self.status.clone());
                    self.task_editor = Some(editor);
                }
            }
            return;
        }

        if cancel || !open {
            self.task_editor = None;
        } else {
            self.task_editor = Some(editor);
        }
    }
}

fn task_editor_kind_name(kind: &TaskEditorKind) -> &'static str {
    match kind {
        TaskEditorKind::Drill { .. } => "Drill",
        TaskEditorKind::Cutout { .. } => "Contour cut",
        TaskEditorKind::Engrave { .. } => "Engrave",
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

        egui::CentralPanel::no_frame().show(ui, |ui| {
            ui::viewport::show(ui, self);
        });

        let ctx = ui.ctx().clone();
        self.show_dialogs(&ctx);
    }
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
