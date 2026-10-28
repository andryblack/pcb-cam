//! Application state and top-level egui layout.

use std::path::PathBuf;

use eframe::egui::{self, Align2, Key, Modifiers, RichText, Stroke, Vec2};
use pbc_cam_core::{
    DrillOperation, LayerGeometry, LayerId, OperationId, OperationKind, SourceFormat, ToolId, Unit,
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
    Operation(OperationId),
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

/// A modal draft of a drilling operation's parameters.
#[derive(Debug, Clone)]
pub(crate) struct OperationEditor {
    pub(crate) id: OperationId,
    pub(crate) name: String,
    pub(crate) layer: LayerId,
    pub(crate) layer_name: String,
    pub(crate) depth: f64,
    pub(crate) retract: f64,
    pub(crate) feed: f64,
    pub(crate) plunge: f64,
    pub(crate) dwell: f64,
    pub(crate) spindle: u32,
    pub(crate) error: Option<String>,
}

impl OperationEditor {
    fn from_drill(
        id: OperationId,
        name: &str,
        layer: LayerId,
        layer_name: &str,
        drill: &DrillOperation,
    ) -> Self {
        Self {
            id,
            name: name.to_owned(),
            layer,
            layer_name: layer_name.to_owned(),
            depth: drill.depth_mm,
            retract: drill.retract_mm,
            feed: drill.feed_mm_min,
            plunge: drill.plunge_mm_min,
            dwell: drill.dwell_s,
            spindle: drill.spindle_rpm,
            error: None,
        }
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
    pub(crate) operation_editor: Option<OperationEditor>,
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
            operation_editor: None,
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
            Some(Selection::Operation(id)) => self.project().operation(id).is_some(),
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
            self.operation_editor = None;
            self.view.reset();
            self.fit_requested = false;
            self.set_status("New project created");
        }
    }

    pub(crate) fn undo(&mut self) {
        if self.run_command("edit.undo", json!({})).is_some() {
            self.operation_editor = None;
            self.set_status("Undo");
        }
    }

    pub(crate) fn redo(&mut self) {
        if self.run_command("edit.redo", json!({})).is_some() {
            self.operation_editor = None;
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

    pub(crate) fn add_drill_operation(&mut self) {
        let layer = self.drill_layer_for_new_operation();
        let Some(layer) = layer else {
            self.set_error("Import an Excellon layer with drills first");
            return;
        };
        if let Some(value) =
            self.run_command("operation.add_drill", json!({"layer": layer.value()}))
        {
            if let Some(id) = value.get("id").and_then(Value::as_u64) {
                self.selection = Some(Selection::Operation(OperationId::new(id)));
            }
            let layer_name = self
                .project()
                .layer(layer)
                .map_or("drills".to_owned(), |layer| layer.name().to_owned());
            self.set_status(format!("Drilling operation added for {layer_name}"));
        }
    }

    pub(crate) fn can_add_drill_operation(&self) -> bool {
        self.drill_layer_for_new_operation().is_some()
    }

    fn drill_layer_for_new_operation(&self) -> Option<LayerId> {
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

    pub(crate) fn export_gcode_dialog(&mut self) {
        if self.project().enabled_drill_count() == 0 {
            self.set_error("Add and enable a drilling operation first");
            return;
        }
        let mut dialog = rfd::FileDialog::new()
            .set_title("Export G-code")
            .add_filter("G-code", &["nc", "gcode", "tap"]);
        let default_name = format!(
            "{}.nc",
            self.project()
                .name()
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                        ch
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
                .trim_matches('_')
                .to_owned()
        );
        let default_name = if default_name == ".nc" {
            "board.nc".to_owned()
        } else {
            default_name
        };
        dialog = dialog.set_file_name(default_name);
        let Some(path) = dialog.save_file() else {
            return;
        };
        let operation = match self.selection {
            Some(Selection::Operation(id))
                if self
                    .project()
                    .operation(id)
                    .is_some_and(|operation| operation.enabled()) =>
            {
                Some(id.value())
            }
            _ => None,
        };
        if let Some(value) = self.run_command(
            "file.export_gcode",
            json!({"path": path, "operation": operation}),
        ) {
            let holes = value.get("holes").and_then(Value::as_u64).unwrap_or(0);
            let slots = value.get("slots").and_then(Value::as_u64).unwrap_or(0);
            self.set_status(format!("G-code exported · {holes} holes · {slots} slots"));
            log::info!("exported G-code: {value}");
        }
    }

    pub(crate) fn remove_selection(&mut self) {
        let Some(selection) = self.selection else {
            return;
        };
        let result = match selection {
            Selection::Layer(id) => self.run_command("layer.remove", json!({"id": id.value()})),
            Selection::Tool(id) => self.run_command("tool.remove", json!({"id": id.value()})),
            Selection::Operation(id) => {
                self.run_command("operation.remove", json!({"id": id.value()}))
            }
        };
        if result.is_some() {
            self.selection = None;
            self.operation_editor = None;
            self.set_status("Removed");
        }
    }

    pub(crate) fn open_operation_editor(&mut self) {
        let Some(Selection::Operation(id)) = self.selection else {
            self.set_error("Select a drilling operation first");
            return;
        };
        let Some(operation) = self.project().operation(id) else {
            return;
        };
        let OperationKind::Drill(drill) = operation.kind();
        let layer_name = self
            .project()
            .layer(drill.layer)
            .map_or("Excellon layer".to_owned(), |layer| layer.name().to_owned());
        self.operation_editor = Some(OperationEditor::from_drill(
            id,
            operation.name(),
            drill.layer,
            &layer_name,
            drill,
        ));
    }

    /// Load the bundled demo project.
    pub fn load_demo(&mut self) {
        let top = include_str!("../../../assets/samples/demo-F_Cu.gbr");
        let outline = include_str!("../../../assets/samples/demo-Edge_Cuts.gbr");
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
        if !self.import_content(SourceFormat::Excellon, drills, "Demo Drills") {
            return;
        }
        let layer = self
            .project()
            .layers()
            .last()
            .map(|layer| layer.id().value());
        if let Some(value) =
            self.run_command("operation.add_drill", json!({"layer": layer, "depth": 1.6}))
            && let Some(id) = value.get("id").and_then(Value::as_u64)
        {
            self.selection = Some(Selection::Operation(OperationId::new(id)));
        }
        self.fit_requested = true;
        self.set_status("Demo project loaded — import, layers, drill operation and G-code export");
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
            self.operation_editor = None;
        }

        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Q)) {
            Self::quit(ctx);
            return;
        }

        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F1)) {
            self.about_open = true;
            return;
        }

        if self.about_open || self.shortcuts_open || self.operation_editor.is_some() {
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
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::E)) {
            self.export_gcode_dialog();
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
                    widgets::field_row(ui, "Milestone", "Gerber/Excellon import + drilling");
                    widgets::field_row(
                        ui,
                        "In this project",
                        &format!(
                            "{} layers · {} operations",
                            self.project().layers().len(),
                            self.project().operations().len()
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
                    shortcut_row(ui, "Cmd/Ctrl+E", "Export G-code");
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

        self.show_operation_editor(ctx);
    }

    fn show_operation_editor(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.operation_editor.clone() else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;

        egui::Window::new("Drill operation")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(390.0)
            .frame(dialog_frame())
            .show(ctx, |ui| {
                widgets::section_label(ui, "Operation");
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.add(
                        egui::TextEdit::singleline(&mut editor.name).desired_width(f32::INFINITY),
                    );
                });
                ui.label(
                    RichText::new(format!(
                        "Source: {} · #{}",
                        editor.layer_name,
                        editor.layer.value()
                    ))
                    .size(10.5)
                    .color(theme::TEXT_MUTED),
                );

                ui.add_space(10.0);
                widgets::hairline(ui);
                ui.add_space(8.0);
                widgets::section_label(ui, "Cut parameters");
                ui.add_space(4.0);

                metric_row(ui, "Depth", &mut editor.depth, 0.01..=50.0, " mm");
                metric_row(ui, "Retract", &mut editor.retract, 0.1..=100.0, " mm");
                metric_row(ui, "Feed", &mut editor.feed, 1.0..=10_000.0, " mm/min");
                metric_row(ui, "Plunge", &mut editor.plunge, 1.0..=5_000.0, " mm/min");
                metric_row(ui, "Dwell", &mut editor.dwell, 0.0..=10.0, " s");
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Spindle").size(11.0).color(theme::TEXT_MUTED));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add(
                            egui::DragValue::new(&mut editor.spindle)
                                .range(0..=60_000)
                                .speed(100)
                                .suffix(" rpm"),
                        );
                    });
                });

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
            let params = json!({
                "id": editor.id.value(),
                "name": editor.name,
                "depth": editor.depth,
                "retract": editor.retract,
                "feed": editor.feed,
                "plunge": editor.plunge,
                "dwell": editor.dwell,
                "spindle": editor.spindle,
            });
            match self.run_command("operation.update_drill", params) {
                Some(_) => {
                    self.operation_editor = None;
                    self.set_status("Operation updated");
                }
                None => {
                    editor.error = Some(self.status.clone());
                    self.operation_editor = Some(editor);
                }
            }
            return;
        }

        if cancel || !open {
            self.operation_editor = None;
        } else {
            self.operation_editor = Some(editor);
        }
    }
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
