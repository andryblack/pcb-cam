//! Versioned project file serialization.

use serde::{Deserialize, Serialize};

use crate::{Project, ToolLibrary};

/// Identifier stored in `.pcbcam` files.
pub const PROJECT_FORMAT: &str = "pcb-cam-project";
/// Current project file version.
pub const PROJECT_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct ProjectFile {
    format: String,
    version: u32,
    project: Project,
    tools: ToolLibrary,
}

/// Project plus the tool definitions referenced by its tasks.
#[derive(Debug, Clone)]
pub struct LoadedProject {
    /// Restored project.
    pub project: Project,
    /// Tool snapshot stored with the project.
    pub tools: ToolLibrary,
}

/// Serialize a project to the versioned JSON format.
pub fn save_project(project: &Project, tools: &ToolLibrary) -> Result<String, String> {
    let file = ProjectFile {
        format: PROJECT_FORMAT.to_owned(),
        version: PROJECT_VERSION,
        project: project.clone(),
        tools: tools.clone(),
    };
    serde_json::to_string_pretty(&file)
        .map_err(|error| format!("cannot serialize project: {error}"))
}

/// Parse and validate a project file.
pub fn load_project(text: &str) -> Result<LoadedProject, String> {
    let file: ProjectFile =
        serde_json::from_str(text).map_err(|error| format!("invalid project file: {error}"))?;
    if file.format != PROJECT_FORMAT {
        return Err(format!("unsupported project format `{}`", file.format));
    }
    if file.version != PROJECT_VERSION {
        return Err(format!(
            "unsupported project version {} (expected {})",
            file.version, PROJECT_VERSION
        ));
    }
    validate(&file.project, &file.tools)?;
    Ok(LoadedProject {
        project: file.project,
        tools: file.tools,
    })
}

fn validate(project: &Project, tools: &ToolLibrary) -> Result<(), String> {
    let mut layer_ids = std::collections::BTreeSet::new();
    for layer in project.layers() {
        if !layer_ids.insert(layer.id()) {
            return Err(format!("duplicate layer id {}", layer.id().value()));
        }
    }
    let mut operation_ids = std::collections::BTreeSet::new();
    for operation in project.operations() {
        if !operation_ids.insert(operation.id()) {
            return Err(format!("duplicate task id {}", operation.id().value()));
        }
        if !layer_ids.contains(&operation.source_layer()) {
            return Err(format!(
                "task `{}` references missing layer {}",
                operation.name(),
                operation.source_layer().value()
            ));
        }
        if tools.tool(operation.tool()).is_none() {
            return Err(format!(
                "task `{}` references missing machining tool {}",
                operation.name(),
                operation.tool().value()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Aperture, DrillOperation, ExcellonLayer, FlipAxes, LayerGeometry, OperationKind, Point,
        Primitive, ToolId,
    };

    #[test]
    fn project_round_trips() {
        let mut project = Project::new("Round trip");
        project.set_gcode_origin(Point::new(2.0, -3.0));
        project.set_flip_axes(FlipAxes::new(4.0, -2.0));
        let layer = project.add_layer(
            "Copper",
            None,
            None,
            LayerGeometry::Excellon(ExcellonLayer::default()),
        );
        let tools = ToolLibrary::new();
        let task = project.add_task(
            "Drill",
            OperationKind::Drill(DrillOperation::new(layer, ToolId::new(1))),
        );
        let text = save_project(&project, &tools).unwrap();
        let loaded = load_project(&text).unwrap();
        assert_eq!(loaded.project.name(), "Round trip");
        assert_eq!(loaded.project.layers().len(), 1);
        assert_eq!(loaded.project.operations().len(), 1);
        assert_eq!(loaded.project.operations()[0].id(), task);
        assert_eq!(loaded.project.gcode_origin(), Point::new(2.0, -3.0));
        assert_eq!(loaded.project.flip_axes(), Some(FlipAxes::new(4.0, -2.0)));
        assert!(text.contains("\"format\": \"pcb-cam-project\""));
    }

    #[test]
    fn sample_gerber_round_trips() {
        let gerber =
            crate::parse_gerber(include_str!("../../../assets/samples/demo-F_Cu.gbr")).unwrap();
        let mut project = Project::new("Sample");
        let id = project.add_layer("Top", None, None, LayerGeometry::Gerber(gerber));
        let objects = project.layer(id).unwrap().object_count();
        let text = save_project(&project, &ToolLibrary::new()).unwrap();
        let loaded = load_project(&text).unwrap();
        assert_eq!(loaded.project.layer(id).unwrap().object_count(), objects);
        assert_eq!(loaded.project.layers()[0].name(), "Top");
    }

    #[test]
    fn all_task_kinds_round_trip() {
        let tools = ToolLibrary::new();
        let drill = tools.first_of_kind(crate::ToolKind::Drill).unwrap();
        let end_mill = tools.first_of_kind(crate::ToolKind::EndMill).unwrap();
        let v_bit = tools.first_of_kind(crate::ToolKind::VBit).unwrap();
        let cone = tools.first_of_kind(crate::ToolKind::Cone).unwrap();

        let mut project = Project::new("Tasks");
        let excellon = project.add_layer(
            "Drills",
            None,
            None,
            LayerGeometry::Excellon(ExcellonLayer::default()),
        );
        let gerber = project.add_layer(
            "Copper",
            None,
            None,
            LayerGeometry::Gerber(crate::GerberLayer::default()),
        );
        for (name, kind) in [
            (
                "Drill",
                OperationKind::Drill(DrillOperation::new(excellon, drill)),
            ),
            (
                "Spot",
                OperationKind::SpotDrill(crate::SpotDrillOperation::new(excellon, cone)),
            ),
            (
                "Hole",
                OperationKind::HoleMill(crate::HoleMillOperation::new(excellon, end_mill)),
            ),
            (
                "Cutout",
                OperationKind::Cutout(crate::CutoutOperation::new(gerber, end_mill)),
            ),
            (
                "Clearance",
                OperationKind::Engrave(crate::EngraveOperation::new(gerber, v_bit)),
            ),
        ] {
            project.add_operation(name, kind);
        }

        let text = save_project(&project, &tools).unwrap();
        let loaded = load_project(&text).unwrap();
        assert_eq!(loaded.project.operations().len(), 5);
        assert_eq!(loaded.project.operations()[4].name(), "Clearance");
        assert_eq!(loaded.project.operations()[3].kind().label(), "Edge cut");
    }

    #[test]
    fn wrong_version_is_rejected() {
        let project = Project::new("Version");
        let tools = ToolLibrary::new();
        let mut value: serde_json::Value =
            serde_json::from_str(&save_project(&project, &tools).unwrap()).unwrap();
        value["version"] = 99.into();
        let error = load_project(&value.to_string()).unwrap_err();
        assert!(error.contains("unsupported project version"));
    }

    #[test]
    fn missing_tool_is_rejected() {
        let mut project = Project::new("Missing tool");
        let layer = project.add_layer(
            "Copper",
            None,
            None,
            LayerGeometry::Excellon(ExcellonLayer::default()),
        );
        let task = project.add_task(
            "Drill",
            OperationKind::Drill(DrillOperation::new(layer, ToolId::new(999))),
        );
        assert_eq!(task.value(), 1);
        let mut tools = ToolLibrary::new();
        tools.set_tools(Vec::new());
        let text = save_project(&project, &tools).unwrap();
        let error = load_project(&text).unwrap_err();
        assert!(error.contains("missing machining tool"));
    }

    #[test]
    fn geometry_round_trips() {
        let mut project = Project::new("Geometry");
        project.add_layer(
            "Copper",
            None,
            None,
            LayerGeometry::Gerber(crate::GerberLayer {
                primitives: vec![Primitive::Flash {
                    position: Point::new(1.0, 2.0),
                    aperture: Aperture::circle(0.5),
                    rotation_degrees: 0.0,
                    polarity: crate::Polarity::Dark,
                }],
                source_units: Some(crate::Unit::Millimeter),
                warnings: vec!["warning".to_owned()],
            }),
        );
        let text = save_project(&project, &ToolLibrary::new()).unwrap();
        let loaded = load_project(&text).unwrap();
        assert_eq!(loaded.project.layers()[0].object_count(), 1);
    }
}
