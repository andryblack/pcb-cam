//! G-code generation for project operations.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::geometry::{DrillHit, DrillSlot, ExcellonLayer, LayerGeometry, Point};
use crate::operation::{Operation, OperationKind};
use crate::project::Project;

/// A generated NC program and its statistics.
#[derive(Debug, Clone, PartialEq)]
pub struct GcodeProgram {
    /// Complete G-code text.
    pub text: String,
    /// Number of tools used.
    pub tool_count: usize,
    /// Number of drilled holes.
    pub hole_count: usize,
    /// Number of routed slots.
    pub slot_count: usize,
}

#[derive(Debug)]
struct ToolGroup {
    diameter_mm: f64,
    holes: Vec<DrillHit>,
    slots: Vec<DrillSlot>,
}

/// Generate a millimetre drilling program for a drill operation.
pub fn generate_drill_gcode(
    project: &Project,
    operation: &Operation,
) -> Result<GcodeProgram, String> {
    if !operation.enabled() {
        return Err("operation is disabled".into());
    }
    let OperationKind::Drill(drill) = operation.kind();
    drill.validate()?;

    let layer = project
        .layer(drill.layer)
        .ok_or_else(|| "operation layer no longer exists".to_owned())?;
    let LayerGeometry::Excellon(excellon) = layer.geometry() else {
        return Err("drill operation requires an Excellon layer".into());
    };
    if excellon.holes.is_empty() && excellon.slots.is_empty() {
        return Err("the Excellon layer contains no holes or slots".into());
    }

    let mut groups: BTreeMap<i64, ToolGroup> = BTreeMap::new();
    for hole in &excellon.holes {
        let key = diameter_key(hole.diameter_mm)?;
        groups
            .entry(key)
            .or_insert_with(|| ToolGroup {
                diameter_mm: hole.diameter_mm,
                holes: Vec::new(),
                slots: Vec::new(),
            })
            .holes
            .push(hole.clone());
    }
    for slot in &excellon.slots {
        let key = diameter_key(slot.diameter_mm)?;
        groups
            .entry(key)
            .or_insert_with(|| ToolGroup {
                diameter_mm: slot.diameter_mm,
                holes: Vec::new(),
                slots: Vec::new(),
            })
            .slots
            .push(slot.clone());
    }

    let mut text = String::new();
    writeln!(
        text,
        "(PBC-CAM {} - generated drilling program)",
        env!("CARGO_PKG_VERSION")
    )
    .map_err(|error| error.to_string())?;
    writeln!(text, "(Project: {})", project.name()).map_err(|error| error.to_string())?;
    writeln!(text, "(Operation: {})", operation.name()).map_err(|error| error.to_string())?;
    writeln!(text, "G21").map_err(|error| error.to_string())?;
    writeln!(text, "G90").map_err(|error| error.to_string())?;
    writeln!(text, "G17").map_err(|error| error.to_string())?;
    writeln!(text, "G94").map_err(|error| error.to_string())?;
    writeln!(text).map_err(|error| error.to_string())?;

    let mut tool_count = 0;
    let mut hole_count = 0;
    let mut slot_count = 0;

    for (index, group) in groups.values().enumerate() {
        let tool = index + 1;
        tool_count += 1;
        hole_count += group.holes.len();
        slot_count += group.slots.len();

        writeln!(
            text,
            "(Tool T{tool}: {:.3} mm, {} holes, {} slots)",
            group.diameter_mm,
            group.holes.len(),
            group.slots.len()
        )
        .map_err(|error| error.to_string())?;
        writeln!(text, "T{tool} M6").map_err(|error| error.to_string())?;
        writeln!(text, "S{} M3", drill.spindle_rpm).map_err(|error| error.to_string())?;
        writeln!(text, "G00 Z{:.3}", drill.retract_mm).map_err(|error| error.to_string())?;

        for hole in &group.holes {
            writeln!(text, "G00 X{:.3} Y{:.3}", hole.position.x, hole.position.y)
                .map_err(|error| error.to_string())?;
            writeln!(
                text,
                "G01 Z-{:.3} F{:.1}",
                drill.depth_mm, drill.plunge_mm_min
            )
            .map_err(|error| error.to_string())?;
            if drill.dwell_s > 0.0 {
                writeln!(text, "G04 P{:.2}", drill.dwell_s).map_err(|error| error.to_string())?;
            }
            writeln!(text, "G00 Z{:.3}", drill.retract_mm).map_err(|error| error.to_string())?;
        }

        for slot in &group.slots {
            route_slot(&mut text, drill, slot)?;
        }

        writeln!(text, "G00 Z{:.3}", drill.retract_mm).map_err(|error| error.to_string())?;
        writeln!(text, "M5").map_err(|error| error.to_string())?;
        writeln!(text).map_err(|error| error.to_string())?;
    }

    writeln!(text, "G00 Z{:.3}", drill.retract_mm).map_err(|error| error.to_string())?;
    writeln!(text, "G00 X0.000 Y0.000").map_err(|error| error.to_string())?;
    writeln!(text, "M30").map_err(|error| error.to_string())?;

    Ok(GcodeProgram {
        text,
        tool_count,
        hole_count,
        slot_count,
    })
}

fn route_slot(
    text: &mut String,
    drill: &crate::DrillOperation,
    slot: &DrillSlot,
) -> Result<(), String> {
    writeln!(text, "G00 X{:.3} Y{:.3}", slot.start.x, slot.start.y)
        .map_err(|error| error.to_string())?;
    writeln!(
        text,
        "G01 Z-{:.3} F{:.1}",
        drill.depth_mm, drill.plunge_mm_min
    )
    .map_err(|error| error.to_string())?;
    writeln!(
        text,
        "G01 X{:.3} Y{:.3} F{:.1}",
        slot.end.x, slot.end.y, drill.feed_mm_min
    )
    .map_err(|error| error.to_string())?;
    if drill.dwell_s > 0.0 {
        writeln!(text, "G04 P{:.2}", drill.dwell_s).map_err(|error| error.to_string())?;
    }
    writeln!(text, "G00 Z{:.3}", drill.retract_mm).map_err(|error| error.to_string())?;
    Ok(())
}

fn diameter_key(diameter_mm: f64) -> Result<i64, String> {
    if !diameter_mm.is_finite() || diameter_mm <= 0.0 {
        return Err("a drill hit has an invalid tool diameter".into());
    }
    Ok((diameter_mm * 1_000_000.0).round() as i64)
}

/// Format one coordinate for previews.
pub fn format_point(point: Point) -> String {
    format!("{:.3}, {:.3}", point.x, point.y)
}

/// Count drill hits in an Excellon layer.
pub fn drill_hit_count(layer: &ExcellonLayer) -> usize {
    layer.holes.len() + layer.slots.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Project;
    use crate::geometry::{DrillHit, DrillSlot, DrillTool, ExcellonLayer, LayerGeometry, Point};
    use crate::operation::{DrillOperation, OperationKind};

    fn project_with_drills() -> (Project, crate::OperationId) {
        let mut project = Project::new("Test");
        let layer = LayerGeometry::Excellon(ExcellonLayer {
            tools: vec![DrillTool {
                number: 1,
                diameter_mm: 0.8,
            }],
            holes: vec![
                DrillHit {
                    position: Point::new(10.0, 10.0),
                    tool: 1,
                    diameter_mm: 0.8,
                },
                DrillHit {
                    position: Point::new(20.0, 10.0),
                    tool: 1,
                    diameter_mm: 0.8,
                },
            ],
            slots: vec![DrillSlot {
                start: Point::new(30.0, 10.0),
                end: Point::new(35.0, 10.0),
                tool: 1,
                diameter_mm: 1.0,
            }],
            source_units: Some(crate::Unit::Millimeter),
            warnings: Vec::new(),
        });
        let layer_id = project.add_layer("Drills", None, None, layer);
        let operation =
            project.add_operation("Drill", OperationKind::Drill(DrillOperation::new(layer_id)));
        (project, operation)
    }

    #[test]
    fn gcode_contains_tools_and_holes() {
        let (project, id) = project_with_drills();
        let program = generate_drill_gcode(&project, project.operation(id).unwrap()).unwrap();
        assert_eq!(program.hole_count, 2);
        assert_eq!(program.slot_count, 1);
        assert_eq!(program.tool_count, 2);
        assert!(program.text.contains("G21"));
        assert!(program.text.contains("T1 M6"));
        assert!(program.text.contains("X10.000 Y10.000"));
        assert!(program.text.contains("G01 X35.000 Y10.000"));
        assert!(program.text.ends_with("M30\n"));
    }

    #[test]
    fn disabled_operation_is_refused() {
        let (mut project, id) = project_with_drills();
        project.operation_mut(id).unwrap().set_enabled(false);
        let error = generate_drill_gcode(&project, project.operation(id).unwrap()).unwrap_err();
        assert!(error.contains("disabled"));
    }
}
