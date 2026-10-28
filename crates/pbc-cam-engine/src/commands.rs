//! Built-in command implementations.

use std::path::{Path, PathBuf};

use pbc_cam_core::{
    ContourSide, CutoutOperation, DrillOperation, EngraveOperation, LayerGeometry, LayerId,
    OperationId, OperationKind, Project, ToolKind, UNTITLED_PROJECT, generate_all_gcode,
    generate_task_gcode, parse_excellon, parse_gerber,
};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::Session;

#[derive(Debug, Deserialize)]
struct ImportParams {
    path: Option<PathBuf>,
    content: Option<String>,
    name: Option<String>,
    color: Option<[u8; 3]>,
}

#[derive(Debug, Deserialize)]
struct ProjectNewParams {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LayerIdParams {
    id: u64,
}

#[derive(Debug, Deserialize)]
struct LayerVisibleParams {
    id: u64,
    visible: bool,
}

#[derive(Debug, Deserialize)]
struct LayerRenameParams {
    id: u64,
    name: String,
}

#[derive(Debug, Deserialize)]
struct UnitsParams {
    units: String,
}

#[derive(Debug, Deserialize)]
struct ToolCreateParams {
    name: Option<String>,
    kind: Option<String>,
    diameter: f64,
    feed: Option<f64>,
    plunge: Option<f64>,
    spindle: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ToolUpdateParams {
    id: u64,
    name: Option<String>,
    kind: Option<String>,
    diameter: Option<f64>,
    feed: Option<f64>,
    plunge: Option<f64>,
    spindle: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OperationAddDrillParams {
    layer: u64,
    name: Option<String>,
    depth: Option<f64>,
    retract: Option<f64>,
    feed: Option<f64>,
    plunge: Option<f64>,
    dwell: Option<f64>,
    spindle: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OperationAddCutoutParams {
    layer: u64,
    name: Option<String>,
    tool: Option<f64>,
    side: Option<String>,
    depth: Option<f64>,
    retract: Option<f64>,
    feed: Option<f64>,
    plunge: Option<f64>,
    spindle: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OperationUpdateCutoutParams {
    id: u64,
    name: Option<String>,
    tool: Option<f64>,
    side: Option<String>,
    depth: Option<f64>,
    retract: Option<f64>,
    feed: Option<f64>,
    plunge: Option<f64>,
    spindle: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OperationAddEngraveParams {
    layer: u64,
    name: Option<String>,
    tool: Option<f64>,
    depth: Option<f64>,
    retract: Option<f64>,
    feed: Option<f64>,
    plunge: Option<f64>,
    spindle: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OperationUpdateEngraveParams {
    id: u64,
    name: Option<String>,
    tool: Option<f64>,
    depth: Option<f64>,
    retract: Option<f64>,
    feed: Option<f64>,
    plunge: Option<f64>,
    spindle: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ExportTaskGcodeParams {
    path: PathBuf,
    task: u64,
}

#[derive(Debug, Deserialize)]
struct OperationUpdateDrillParams {
    id: u64,
    name: Option<String>,
    depth: Option<f64>,
    retract: Option<f64>,
    feed: Option<f64>,
    plunge: Option<f64>,
    dwell: Option<f64>,
    spindle: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OperationIdParams {
    id: u64,
}

#[derive(Debug, Deserialize)]
struct OperationEnabledParams {
    id: u64,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct OperationMoveParams {
    id: u64,
    to: usize,
}

#[derive(Debug, Deserialize)]
struct ExportGcodeParams {
    path: PathBuf,
}

pub(crate) fn project_new(session: &mut Session, params: &Value) -> Result<Value, String> {
    let params: ProjectNewParams = parse_params(params)?;
    let name = params.name.unwrap_or_else(|| UNTITLED_PROJECT.to_owned());
    session.replace_project(Project::new(name));
    Ok(json!({
        "name": session.project().name(),
    }))
}

pub(crate) fn import_gerber(session: &mut Session, params: &Value) -> Result<Value, String> {
    let params: ImportParams = parse_params(params)?;
    let (source, path, default_name) = read_import_source(&params, "Gerber")?;
    let layer_geometry =
        parse_gerber(&source).map_err(|error| format!("Gerber import failed: {error}"))?;
    let bounds = layer_geometry.bounds();
    let warnings = layer_geometry.warnings.clone();
    let objects = layer_geometry.object_count();
    let name = params.name.unwrap_or(default_name);

    let project = session.project_mut();
    if project.name() == UNTITLED_PROJECT {
        project.set_name(display_stem(&name));
    }
    let layer = project.add_layer(
        name.clone(),
        path,
        params.color,
        LayerGeometry::Gerber(layer_geometry),
    );

    Ok(json!({
        "layer": layer.value(),
        "name": name,
        "objects": objects,
        "warnings": warnings,
        "bounds": bounds_json(bounds),
    }))
}

pub(crate) fn import_excellon(session: &mut Session, params: &Value) -> Result<Value, String> {
    let params: ImportParams = parse_params(params)?;
    let (source, path, default_name) = read_import_source(&params, "Excellon")?;
    let layer_geometry =
        parse_excellon(&source).map_err(|error| format!("Excellon import failed: {error}"))?;
    let bounds = layer_geometry.bounds();
    let warnings = layer_geometry.warnings.clone();
    let objects = layer_geometry.hole_count() + layer_geometry.slot_count();
    let name = params.name.unwrap_or(default_name);
    let drill_tools = layer_geometry.tools.clone();

    let project = session.project_mut();
    if project.name() == UNTITLED_PROJECT {
        project.set_name(display_stem(&name));
    }
    let layer = project.add_layer(
        name.clone(),
        path,
        params.color,
        LayerGeometry::Excellon(layer_geometry),
    );

    let mut added_tools = 0;
    for tool in drill_tools {
        let already = project.tools().iter().any(|existing| {
            existing.source_layer() == Some(layer)
                && (existing.diameter_mm() - tool.diameter_mm).abs() < 1e-9
        });
        if !already && tool.diameter_mm > 0.0 {
            project.add_tool(
                format!("Drill T{} · {:.2} mm", tool.number, tool.diameter_mm),
                ToolKind::Drill,
                tool.diameter_mm,
                Some(layer),
            );
            added_tools += 1;
        }
    }

    Ok(json!({
        "layer": layer.value(),
        "name": name,
        "objects": objects,
        "tools": added_tools,
        "warnings": warnings,
        "bounds": bounds_json(bounds),
    }))
}

pub(crate) fn layer_set_visible(session: &mut Session, params: &Value) -> Result<Value, String> {
    let params: LayerVisibleParams = parse_params(params)?;
    let layer = LayerId::new(params.id);
    if !session
        .project_mut()
        .set_layer_visible(layer, params.visible)
    {
        return Err(format!("layer {} does not exist", params.id));
    }
    Ok(json!({"id": params.id, "visible": params.visible}))
}

pub(crate) fn layer_rename(session: &mut Session, params: &Value) -> Result<Value, String> {
    let params: LayerRenameParams = parse_params(params)?;
    let name = params.name.trim();
    if name.is_empty() {
        return Err("layer name must not be empty".into());
    }
    let layer = session
        .project_mut()
        .layer_mut(LayerId::new(params.id))
        .ok_or_else(|| format!("layer {} does not exist", params.id))?;
    layer.set_name(name);
    Ok(json!({"id": params.id, "name": layer.name()}))
}

pub(crate) fn layer_remove(session: &mut Session, params: &Value) -> Result<Value, String> {
    let params: LayerIdParams = parse_params(params)?;
    if !session.project_mut().remove_layer(LayerId::new(params.id)) {
        return Err(format!("layer {} does not exist", params.id));
    }
    Ok(json!({"id": params.id}))
}

pub(crate) fn units_set(session: &mut Session, params: &Value) -> Result<Value, String> {
    let params: UnitsParams = parse_params(params)?;
    let units = match params.units.as_str() {
        "mm" | "millimeter" | "millimetre" => pbc_cam_core::Unit::Millimeter,
        "in" | "inch" | "inches" => pbc_cam_core::Unit::Inch,
        other => return Err(format!("unsupported units `{other}`")),
    };
    session.project_mut().set_units(units);
    Ok(json!({"units": units.symbol()}))
}

pub(crate) fn tool_create(session: &mut Session, params: &Value) -> Result<Value, String> {
    let params: ToolCreateParams = parse_params(params)?;
    validate_diameter(params.diameter)?;
    let kind = parse_tool_kind(params.kind.as_deref().unwrap_or("drill"))?;
    let project = session.project_mut();
    let id = project.add_tool(
        params
            .name
            .unwrap_or_else(|| format!("{} {:.2} mm", kind.label(), params.diameter)),
        kind,
        params.diameter,
        None,
    );
    if let Some(tool) = project.tool_mut(id) {
        if let Some(feed) = params.feed {
            tool.set_feed_mm_min(feed);
        }
        if let Some(plunge) = params.plunge {
            tool.set_plunge_mm_min(plunge);
        }
        if let Some(spindle) = params.spindle {
            tool.set_spindle_rpm(spindle);
        }
    }
    Ok(json!({"id": id.value()}))
}

pub(crate) fn tool_update(session: &mut Session, params: &Value) -> Result<Value, String> {
    let params: ToolUpdateParams = parse_params(params)?;
    if let Some(diameter) = params.diameter {
        validate_diameter(diameter)?;
    }
    let tool = session
        .project_mut()
        .tool_mut(pbc_cam_core::ToolId::new(params.id))
        .ok_or_else(|| format!("tool {} does not exist", params.id))?;
    if let Some(name) = params.name {
        tool.set_name(name);
    }
    if let Some(kind) = params.kind {
        tool.set_kind(parse_tool_kind(&kind)?);
    }
    if let Some(diameter) = params.diameter {
        tool.set_diameter_mm(diameter);
    }
    if let Some(feed) = params.feed {
        tool.set_feed_mm_min(feed);
    }
    if let Some(plunge) = params.plunge {
        tool.set_plunge_mm_min(plunge);
    }
    if let Some(spindle) = params.spindle {
        tool.set_spindle_rpm(spindle);
    }
    Ok(json!({"id": params.id}))
}

pub(crate) fn tool_remove(session: &mut Session, params: &Value) -> Result<Value, String> {
    let params: LayerIdParams = parse_params(params)?;
    if !session
        .project_mut()
        .remove_tool(pbc_cam_core::ToolId::new(params.id))
    {
        return Err(format!("tool {} does not exist", params.id));
    }
    Ok(json!({"id": params.id}))
}

pub(crate) fn add_drill_enabled(session: &Session) -> Result<(), String> {
    let layer = session.project().layers().iter().find(|layer| {
        matches!(layer.geometry(), LayerGeometry::Excellon(excellon) if !excellon.holes.is_empty() || !excellon.slots.is_empty())
    });
    match layer {
        Some(_) => Ok(()),
        None => Err("import an Excellon layer with drills first".into()),
    }
}

pub(crate) fn add_cutout_enabled(session: &Session) -> Result<(), String> {
    if session.project().layers().iter().any(|layer| {
        matches!(
            layer.geometry(),
            LayerGeometry::Gerber(gerber) if gerber.has_closed_contours()
        )
    }) {
        Ok(())
    } else {
        Err("import a Gerber layer with closed contours first".into())
    }
}

pub(crate) fn add_engrave_enabled(session: &Session) -> Result<(), String> {
    if session.project().layers().iter().any(|layer| {
        matches!(
            layer.geometry(),
            LayerGeometry::Gerber(gerber) if gerber.has_engraving_paths()
        )
    }) {
        Ok(())
    } else {
        Err("import a Gerber layer with paths first".into())
    }
}

pub(crate) fn operation_add_cutout(session: &mut Session, params: &Value) -> Result<Value, String> {
    let params: OperationAddCutoutParams = parse_params(params)?;
    let layer_id = LayerId::new(params.layer);
    let project = session.project_mut();
    let layer = project
        .layer(layer_id)
        .ok_or_else(|| format!("layer {} does not exist", params.layer))?;
    let LayerGeometry::Gerber(gerber) = layer.geometry() else {
        return Err("contour cut requires a Gerber layer".into());
    };
    if !gerber.has_closed_contours() {
        return Err("the selected layer contains no closed contours".into());
    }
    let default_name = format!("Cut {}", layer.name());
    let name = params.name.unwrap_or(default_name);

    let mut cutout = CutoutOperation::new(layer_id);
    if let Some(tool) = params.tool {
        cutout.tool_diameter_mm = tool;
    }
    if let Some(side) = params.side.as_deref() {
        cutout.side = parse_side(side)?;
    }
    apply_milling_overrides(
        &mut cutout.depth_mm,
        &mut cutout.retract_mm,
        &mut cutout.feed_mm_min,
        &mut cutout.plunge_mm_min,
        &mut cutout.spindle_rpm,
        params.depth,
        params.retract,
        params.feed,
        params.plunge,
        params.spindle,
    );
    cutout.validate()?;
    let id = project.add_task(name.clone(), OperationKind::Cutout(cutout));
    Ok(json!({"id": id.value(), "name": name}))
}

pub(crate) fn operation_update_cutout(
    session: &mut Session,
    params: &Value,
) -> Result<Value, String> {
    let params: OperationUpdateCutoutParams = parse_params(params)?;
    let task = session
        .project_mut()
        .task_mut(OperationId::new(params.id))
        .ok_or_else(|| format!("task {} does not exist", params.id))?;
    if let Some(name) = params.name {
        task.set_name(name);
    }
    let OperationKind::Cutout(cutout) = task.kind_mut() else {
        return Err("task is not a contour cut".into());
    };
    if let Some(tool) = params.tool {
        cutout.tool_diameter_mm = tool;
    }
    if let Some(side) = params.side.as_deref() {
        cutout.side = parse_side(side)?;
    }
    apply_milling_overrides(
        &mut cutout.depth_mm,
        &mut cutout.retract_mm,
        &mut cutout.feed_mm_min,
        &mut cutout.plunge_mm_min,
        &mut cutout.spindle_rpm,
        params.depth,
        params.retract,
        params.feed,
        params.plunge,
        params.spindle,
    );
    cutout.validate()?;
    Ok(json!({"id": params.id}))
}

pub(crate) fn operation_add_engrave(
    session: &mut Session,
    params: &Value,
) -> Result<Value, String> {
    let params: OperationAddEngraveParams = parse_params(params)?;
    let layer_id = LayerId::new(params.layer);
    let project = session.project_mut();
    let layer = project
        .layer(layer_id)
        .ok_or_else(|| format!("layer {} does not exist", params.layer))?;
    let LayerGeometry::Gerber(gerber) = layer.geometry() else {
        return Err("engraving requires a Gerber layer".into());
    };
    if !gerber.has_engraving_paths() {
        return Err("the selected layer contains no paths".into());
    }
    let default_name = format!("Engrave {}", layer.name());
    let name = params.name.unwrap_or(default_name);

    let mut engrave = EngraveOperation::new(layer_id);
    if let Some(tool) = params.tool {
        engrave.tool_diameter_mm = tool;
    }
    apply_milling_overrides(
        &mut engrave.depth_mm,
        &mut engrave.retract_mm,
        &mut engrave.feed_mm_min,
        &mut engrave.plunge_mm_min,
        &mut engrave.spindle_rpm,
        params.depth,
        params.retract,
        params.feed,
        params.plunge,
        params.spindle,
    );
    engrave.validate()?;
    let id = project.add_task(name.clone(), OperationKind::Engrave(engrave));
    Ok(json!({"id": id.value(), "name": name}))
}

pub(crate) fn operation_update_engrave(
    session: &mut Session,
    params: &Value,
) -> Result<Value, String> {
    let params: OperationUpdateEngraveParams = parse_params(params)?;
    let task = session
        .project_mut()
        .task_mut(OperationId::new(params.id))
        .ok_or_else(|| format!("task {} does not exist", params.id))?;
    if let Some(name) = params.name {
        task.set_name(name);
    }
    let OperationKind::Engrave(engrave) = task.kind_mut() else {
        return Err("task is not an engraving task".into());
    };
    if let Some(tool) = params.tool {
        engrave.tool_diameter_mm = tool;
    }
    apply_milling_overrides(
        &mut engrave.depth_mm,
        &mut engrave.retract_mm,
        &mut engrave.feed_mm_min,
        &mut engrave.plunge_mm_min,
        &mut engrave.spindle_rpm,
        params.depth,
        params.retract,
        params.feed,
        params.plunge,
        params.spindle,
    );
    engrave.validate()?;
    Ok(json!({"id": params.id}))
}

pub(crate) fn operation_add_drill(session: &mut Session, params: &Value) -> Result<Value, String> {
    let params: OperationAddDrillParams = parse_params(params)?;
    let layer_id = LayerId::new(params.layer);
    let project = session.project_mut();
    let layer = project
        .layer(layer_id)
        .ok_or_else(|| format!("layer {} does not exist", params.layer))?;
    let LayerGeometry::Excellon(excellon) = layer.geometry() else {
        return Err("drilling tasks require an Excellon layer".into());
    };
    if excellon.holes.is_empty() && excellon.slots.is_empty() {
        return Err("the selected Excellon layer has no holes or slots".into());
    }
    let default_name = format!("Drill {}", layer.name());
    let name = params.name.unwrap_or(default_name);

    let mut drill = DrillOperation::new(layer_id);
    if let Some(depth) = params.depth {
        drill.depth_mm = depth;
    }
    if let Some(retract) = params.retract {
        drill.retract_mm = retract;
    }
    if let Some(feed) = params.feed {
        drill.feed_mm_min = feed;
    }
    if let Some(plunge) = params.plunge {
        drill.plunge_mm_min = plunge;
    }
    if let Some(dwell) = params.dwell {
        drill.dwell_s = dwell;
    }
    if let Some(spindle) = params.spindle {
        drill.spindle_rpm = spindle;
    }
    drill.validate()?;
    let id = project.add_operation(name.clone(), OperationKind::Drill(drill));
    Ok(json!({"id": id.value(), "name": name}))
}

pub(crate) fn operation_update_drill(
    session: &mut Session,
    params: &Value,
) -> Result<Value, String> {
    let params: OperationUpdateDrillParams = parse_params(params)?;
    let operation = session
        .project_mut()
        .operation_mut(OperationId::new(params.id))
        .ok_or_else(|| format!("operation {} does not exist", params.id))?;
    if let Some(name) = params.name {
        operation.set_name(name);
    }
    let OperationKind::Drill(drill) = operation.kind_mut() else {
        return Err("task is not a drilling task".into());
    };
    if let Some(depth) = params.depth {
        drill.depth_mm = depth;
    }
    if let Some(retract) = params.retract {
        drill.retract_mm = retract;
    }
    if let Some(feed) = params.feed {
        drill.feed_mm_min = feed;
    }
    if let Some(plunge) = params.plunge {
        drill.plunge_mm_min = plunge;
    }
    if let Some(dwell) = params.dwell {
        drill.dwell_s = dwell;
    }
    if let Some(spindle) = params.spindle {
        drill.spindle_rpm = spindle;
    }
    drill.validate()?;
    Ok(json!({"id": params.id}))
}

pub(crate) fn operation_remove(session: &mut Session, params: &Value) -> Result<Value, String> {
    let params: OperationIdParams = parse_params(params)?;
    if !session
        .project_mut()
        .remove_operation(OperationId::new(params.id))
    {
        return Err(format!("operation {} does not exist", params.id));
    }
    Ok(json!({"id": params.id}))
}

pub(crate) fn operation_set_enabled(
    session: &mut Session,
    params: &Value,
) -> Result<Value, String> {
    let params: OperationEnabledParams = parse_params(params)?;
    let operation = session
        .project_mut()
        .operation_mut(OperationId::new(params.id))
        .ok_or_else(|| format!("operation {} does not exist", params.id))?;
    operation.set_enabled(params.enabled);
    Ok(json!({"id": params.id, "enabled": params.enabled}))
}

pub(crate) fn operation_move(session: &mut Session, params: &Value) -> Result<Value, String> {
    let params: OperationMoveParams = parse_params(params)?;
    if !session
        .project_mut()
        .move_operation(OperationId::new(params.id), params.to)
    {
        return Err(format!("operation {} does not exist", params.id));
    }
    Ok(json!({"id": params.id, "to": params.to}))
}

pub(crate) fn export_task_gcode_enabled(session: &Session) -> Result<(), String> {
    if session.project().enabled_task_count() == 0 {
        Err("add and enable a task first".into())
    } else {
        Ok(())
    }
}

pub(crate) fn export_gcode_enabled(session: &Session) -> Result<(), String> {
    if session.project().enabled_task_count() == 0 {
        Err("add and enable a task first".into())
    } else {
        Ok(())
    }
}

pub(crate) fn export_task_gcode(session: &mut Session, params: &Value) -> Result<Value, String> {
    let params: ExportTaskGcodeParams = parse_params(params)?;
    let project = session.project();
    let task = project
        .task(OperationId::new(params.task))
        .ok_or_else(|| format!("task {} does not exist", params.task))?;
    let program = generate_task_gcode(project, task)?;
    write_program(&params.path, &program)?;
    Ok(program_json(
        &params.path,
        &program,
        Some(task.id().value()),
    ))
}

pub(crate) fn export_gcode(session: &mut Session, params: &Value) -> Result<Value, String> {
    let params: ExportGcodeParams = parse_params(params)?;
    let project = session.project();
    let program = generate_all_gcode(project)?;
    write_program(&params.path, &program)?;
    Ok(program_json(&params.path, &program, None))
}

fn write_program(path: &Path, program: &pbc_cam_core::GcodeProgram) -> Result<(), String> {
    std::fs::write(path, program.text.as_bytes())
        .map_err(|error| format!("cannot write `{}`: {error}", path.display()))
}

fn program_json(path: &Path, program: &pbc_cam_core::GcodeProgram, task: Option<u64>) -> Value {
    json!({
        "path": path,
        "task": task,
        "tools": program.tool_count,
        "tasks": program.task_count,
        "holes": program.hole_count,
        "slots": program.slot_count,
        "paths": program.path_count,
        "cut_length_mm": program.cut_length_mm,
        "bytes": program.text.len(),
    })
}

pub(crate) fn undo(session: &mut Session, _params: &Value) -> Result<Value, String> {
    if !session.undo() {
        return Err("nothing to undo".into());
    }
    Ok(json!({
        "can_undo": session.can_undo(),
        "can_redo": session.can_redo(),
    }))
}

pub(crate) fn redo(session: &mut Session, _params: &Value) -> Result<Value, String> {
    if !session.redo() {
        return Err("nothing to redo".into());
    }
    Ok(json!({
        "can_undo": session.can_undo(),
        "can_redo": session.can_redo(),
    }))
}

fn parse_side(side: &str) -> Result<ContourSide, String> {
    match side {
        "outside" => Ok(ContourSide::Outside),
        "inside" => Ok(ContourSide::Inside),
        "on_line" | "online" => Ok(ContourSide::OnLine),
        other => Err(format!("unknown contour side `{other}`")),
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_milling_overrides(
    depth: &mut f64,
    retract: &mut f64,
    feed: &mut f64,
    plunge: &mut f64,
    spindle: &mut u32,
    new_depth: Option<f64>,
    new_retract: Option<f64>,
    new_feed: Option<f64>,
    new_plunge: Option<f64>,
    new_spindle: Option<u32>,
) {
    if let Some(value) = new_depth {
        *depth = value;
    }
    if let Some(value) = new_retract {
        *retract = value;
    }
    if let Some(value) = new_feed {
        *feed = value;
    }
    if let Some(value) = new_plunge {
        *plunge = value;
    }
    if let Some(value) = new_spindle {
        *spindle = value;
    }
}

fn parse_params<T: DeserializeOwned>(params: &Value) -> Result<T, String> {
    serde_json::from_value(params.clone()).map_err(|error| format!("invalid parameters: {error}"))
}

fn read_import_source(
    params: &ImportParams,
    fallback_name: &str,
) -> Result<(String, Option<PathBuf>, String), String> {
    let path = params.path.clone();
    let source = if let Some(content) = &params.content {
        content.clone()
    } else {
        let path = path
            .as_ref()
            .ok_or_else(|| "provide either `path` or `content`".to_owned())?;
        let bytes = std::fs::read(path)
            .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
        String::from_utf8_lossy(&bytes).into_owned()
    };
    if source.trim().is_empty() {
        return Err("the imported file is empty".into());
    }

    let default_name = params.name.clone().unwrap_or_else(|| {
        path.as_deref()
            .and_then(Path::file_stem)
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("{fallback_name} layer"))
    });
    Ok((source, path, default_name))
}

fn display_stem(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.to_owned());
    if stem.is_empty() {
        UNTITLED_PROJECT.to_owned()
    } else {
        stem
    }
}

fn parse_tool_kind(kind: &str) -> Result<ToolKind, String> {
    match kind {
        "drill" => Ok(ToolKind::Drill),
        "isolation" => Ok(ToolKind::Isolation),
        "cutout" => Ok(ToolKind::Cutout),
        other => Err(format!("unknown tool kind `{other}`")),
    }
}

fn validate_diameter(diameter: f64) -> Result<(), String> {
    if !diameter.is_finite() || diameter <= 0.0 {
        return Err("tool diameter must be greater than zero".into());
    }
    Ok(())
}

fn bounds_json(bounds: Option<pbc_cam_core::Bounds>) -> Value {
    match bounds {
        Some(bounds) => json!({
            "min": [bounds.min.x, bounds.min.y],
            "max": [bounds.max.x, bounds.max.y],
        }),
        None => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GERBER: &str = "\
%FSLAX46Y46*%
%MOMM*%
%ADD10C,0.300*%
%ADD11C,1.600*%
D10*
X0Y0D02*
X10000000Y0D01*
D11*
X20000000Y10000000D03*
M02*
";

    const EXCELLON: &str = "\
M48
METRIC
T1C0.8
%
G90
T1
X10.0Y10.0
X20.0Y10.0
M30
";

    #[test]
    fn import_gerber_adds_layer_and_undoes() {
        let mut session = Session::new();
        let value = session
            .execute(
                "file.import_gerber",
                &json!({"content": GERBER, "name": "Top copper"}),
            )
            .unwrap();
        assert_eq!(session.project().layers().len(), 1);
        assert!(session.can_undo());
        assert!(value["objects"].as_u64().unwrap() > 0);

        session.execute("edit.undo", &json!({})).unwrap();
        assert!(session.project().layers().is_empty());
        assert!(session.can_redo());
        session.execute("edit.redo", &json!({})).unwrap();
        assert_eq!(session.project().layers().len(), 1);
    }

    #[test]
    fn import_excellon_adds_tools() {
        let mut session = Session::new();
        session
            .execute(
                "file.import_excellon",
                &json!({"content": EXCELLON, "name": "Drills"}),
            )
            .unwrap();
        assert_eq!(session.project().layers().len(), 1);
        assert_eq!(session.project().tools().len(), 1);
        assert_eq!(session.project().tools()[0].diameter_mm(), 0.8);
    }

    #[test]
    fn add_drill_operation_and_export_gcode() {
        let mut session = Session::new();
        let import = session
            .execute(
                "file.import_excellon",
                &json!({"content": EXCELLON, "name": "Drills"}),
            )
            .unwrap();
        let layer = import["layer"].as_u64().unwrap();
        let operation = session
            .execute("task.add_drill", &json!({"layer": layer, "depth": 1.2}))
            .unwrap();
        assert_eq!(session.project().operations().len(), 1);
        assert_eq!(operation["name"], "Drill Drills");

        let path = std::env::temp_dir().join(format!("pbc-cam-test-{}.nc", std::process::id()));
        let export = session
            .execute("file.export_gcode", &json!({"path": path}))
            .unwrap();
        assert_eq!(export["holes"], 2);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("X10.000 Y10.000"));
        assert!(text.contains("M30"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn bad_operation_parameters_leave_project_unchanged() {
        let mut session = Session::new();
        let import = session
            .execute(
                "file.import_excellon",
                &json!({"content": EXCELLON, "name": "Drills"}),
            )
            .unwrap();
        let layer = import["layer"].as_u64().unwrap();
        let before = session.project().clone();
        let error = session
            .execute("task.add_drill", &json!({"layer": layer, "depth": 0.0}))
            .unwrap_err();
        assert!(error.to_string().contains("depth"));
        assert_eq!(session.project().layers().len(), before.layers().len());
        assert!(session.project().operations().is_empty());
    }

    #[test]
    fn command_ids_are_unique() {
        let mut ids: Vec<_> = crate::commands().iter().map(|c| c.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), crate::commands().len());
    }
}
