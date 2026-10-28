//! G-code generation for project tasks.
//!
//! A program can contain one task or the whole enabled task timeline. Tools
//! are numbered by their cutting diameter and a tool change (`M6`) is emitted
//! whenever consecutive tasks or drill groups need another tool.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::geometry::{
    DrillHit, DrillSlot, ExcellonLayer, LayerGeometry, Path, Point, offset_closed_path,
};
use crate::operation::{ContourSide, Operation, OperationKind};
use crate::project::Project;

/// A generated NC program and its statistics.
#[derive(Debug, Clone, PartialEq)]
pub struct GcodeProgram {
    /// Complete G-code text.
    pub text: String,
    /// Number of distinct tools used.
    pub tool_count: usize,
    /// Number of tasks included.
    pub task_count: usize,
    /// Number of drilled holes.
    pub hole_count: usize,
    /// Number of routed slots.
    pub slot_count: usize,
    /// Number of contour/path passes.
    pub path_count: usize,
    /// Total cutting length in millimetres.
    pub cut_length_mm: f64,
}

/// Generate a program for one task.
pub fn generate_task_gcode(project: &Project, task: &Operation) -> Result<GcodeProgram, String> {
    if !task.enabled() {
        return Err("task is disabled".into());
    }
    generate_tasks_gcode(project, &[task])
}

/// Generate a combined program for all enabled tasks in project order.
pub fn generate_all_gcode(project: &Project) -> Result<GcodeProgram, String> {
    let tasks: Vec<&Operation> = project
        .tasks()
        .iter()
        .filter(|task| task.enabled())
        .collect();
    if tasks.is_empty() {
        return Err("no enabled tasks".into());
    }
    generate_tasks_gcode(project, &tasks)
}

/// Generate a program for an explicit list of tasks.
pub fn generate_tasks_gcode(
    project: &Project,
    tasks: &[&Operation],
) -> Result<GcodeProgram, String> {
    if tasks.is_empty() {
        return Err("no tasks to export".into());
    }
    let mut builder = ProgramBuilder::new(project, tasks.len());
    for task in tasks {
        builder.append_task(task)?;
    }
    Ok(builder.finish())
}

/// Backwards-compatible wrapper for drilling tasks.
pub fn generate_drill_gcode(project: &Project, task: &Operation) -> Result<GcodeProgram, String> {
    if !matches!(task.kind(), OperationKind::Drill(_)) {
        return Err("task is not a drilling task".into());
    }
    generate_task_gcode(project, task)
}

struct ProgramBuilder<'a> {
    project: &'a Project,
    text: String,
    tools: Vec<f64>,
    current_tool: Option<usize>,
    current_spindle: Option<u32>,
    last_safe_z: f64,
    task_count: usize,
    hole_count: usize,
    slot_count: usize,
    path_count: usize,
    cut_length_mm: f64,
}

impl<'a> ProgramBuilder<'a> {
    fn new(project: &'a Project, task_count: usize) -> Self {
        let mut text = String::new();
        let _ = writeln!(
            text,
            "(PBC-CAM {} - generated program)",
            env!("CARGO_PKG_VERSION")
        );
        let _ = writeln!(text, "(Project: {})", project.name());
        let _ = writeln!(text, "(Tasks: {task_count})");
        let _ = writeln!(text, "G21");
        let _ = writeln!(text, "G90");
        let _ = writeln!(text, "G17");
        let _ = writeln!(text, "G94");
        let _ = writeln!(text);

        Self {
            project,
            text,
            tools: Vec::new(),
            current_tool: None,
            current_spindle: None,
            last_safe_z: 3.0,
            task_count: 0,
            hole_count: 0,
            slot_count: 0,
            path_count: 0,
            cut_length_mm: 0.0,
        }
    }

    fn append_task(&mut self, task: &Operation) -> Result<(), String> {
        task.validate()?;
        if !task.enabled() {
            return Err(format!("task `{}` is disabled", task.name()));
        }
        let _ = writeln!(self.text);
        let _ = writeln!(self.text, "({} - {})", task.kind_label(), task.name());
        match task.kind() {
            OperationKind::Drill(drill) => self.append_drill(task, drill)?,
            OperationKind::Cutout(cutout) => self.append_cutout(task, cutout)?,
            OperationKind::Engrave(engrave) => self.append_engrave(task, engrave)?,
        }
        self.task_count += 1;
        Ok(())
    }

    fn append_drill(
        &mut self,
        _task: &Operation,
        drill: &crate::DrillOperation,
    ) -> Result<(), String> {
        let layer = self
            .project
            .layer(drill.layer)
            .ok_or_else(|| "drill task layer no longer exists".to_owned())?;
        let LayerGeometry::Excellon(excellon) = layer.geometry() else {
            return Err("drill task requires an Excellon layer".into());
        };
        let groups = drill_groups(excellon)?;
        if groups.is_empty() {
            return Err("the Excellon layer contains no holes or slots".into());
        }

        for group in groups.values() {
            self.select_tool(
                group.diameter_mm,
                drill.retract_mm,
                drill.spindle_rpm,
                "drill",
            );
            let _ = writeln!(self.text, "G00 Z{:.3}", drill.retract_mm);

            for hole in &group.holes {
                let _ = writeln!(
                    self.text,
                    "G00 X{:.3} Y{:.3}",
                    hole.position.x, hole.position.y
                );
                let _ = writeln!(
                    self.text,
                    "G01 Z-{:.3} F{:.1}",
                    drill.depth_mm, drill.plunge_mm_min
                );
                if drill.dwell_s > 0.0 {
                    let _ = writeln!(self.text, "G04 P{:.2}", drill.dwell_s);
                }
                let _ = writeln!(self.text, "G00 Z{:.3}", drill.retract_mm);
                self.hole_count += 1;
            }

            for slot in &group.slots {
                self.route_slot(drill, slot);
            }

            let _ = writeln!(self.text, "G00 Z{:.3}", drill.retract_mm);
        }

        self.last_safe_z = drill.retract_mm;
        Ok(())
    }

    fn append_cutout(
        &mut self,
        _task: &Operation,
        cutout: &crate::CutoutOperation,
    ) -> Result<(), String> {
        let layer = self
            .project
            .layer(cutout.layer)
            .ok_or_else(|| "contour task layer no longer exists".to_owned())?;
        let LayerGeometry::Gerber(gerber) = layer.geometry() else {
            return Err("contour cut requires a Gerber layer".into());
        };
        let contours = gerber.closed_contours();
        if contours.is_empty() {
            return Err("no closed contours found in the selected layer".into());
        }

        let distance = match cutout.side {
            ContourSide::Outside => cutout.tool_diameter_mm * 0.5,
            ContourSide::Inside => -cutout.tool_diameter_mm * 0.5,
            ContourSide::OnLine => 0.0,
        };
        let paths: Vec<Path> = contours
            .into_iter()
            .filter_map(|contour| offset_closed_path(&contour, distance))
            .filter(|path| path.points.len() >= 2)
            .collect();
        if paths.is_empty() {
            return Err("could not offset the layer contours".into());
        }

        self.select_tool(
            cutout.tool_diameter_mm,
            cutout.retract_mm,
            cutout.spindle_rpm,
            "contour cut",
        );
        let _ = writeln!(self.text, "(Side: {})", cutout.side.label());
        for path in &paths {
            self.cut_path(
                path,
                cutout.depth_mm,
                cutout.retract_mm,
                cutout.feed_mm_min,
                cutout.plunge_mm_min,
            );
        }
        self.last_safe_z = cutout.retract_mm;
        Ok(())
    }

    fn append_engrave(
        &mut self,
        _task: &Operation,
        engrave: &crate::EngraveOperation,
    ) -> Result<(), String> {
        let layer = self
            .project
            .layer(engrave.layer)
            .ok_or_else(|| "engrave task layer no longer exists".to_owned())?;
        let LayerGeometry::Gerber(gerber) = layer.geometry() else {
            return Err("engraving requires a Gerber layer".into());
        };
        let paths = gerber.engraving_paths();
        if paths.is_empty() {
            return Err("no paths found in the selected layer".into());
        }

        self.select_tool(
            engrave.tool_diameter_mm,
            engrave.retract_mm,
            engrave.spindle_rpm,
            "engrave",
        );
        for path in &paths {
            self.cut_path(
                path,
                engrave.depth_mm,
                engrave.retract_mm,
                engrave.feed_mm_min,
                engrave.plunge_mm_min,
            );
        }
        self.last_safe_z = engrave.retract_mm;
        Ok(())
    }

    fn select_tool(&mut self, diameter_mm: f64, safe_z: f64, rpm: u32, kind: &str) {
        let index = self
            .tools
            .iter()
            .position(|tool| (tool - diameter_mm).abs() < 1e-6)
            .unwrap_or_else(|| {
                self.tools.push(diameter_mm);
                self.tools.len() - 1
            });
        if self.current_tool != Some(index) {
            if self.current_tool.is_some() {
                let _ = writeln!(self.text, "G00 Z{safe_z:.3}");
                let _ = writeln!(self.text, "M5");
            }
            let number = index + 1;
            let _ = writeln!(
                self.text,
                "T{number} M6 (T{number}: {diameter_mm:.3} mm, {kind})"
            );
            self.current_tool = Some(index);
            self.current_spindle = None;
        }
        if self.current_spindle != Some(rpm) {
            let _ = writeln!(self.text, "S{rpm} M3");
            self.current_spindle = Some(rpm);
        }
    }

    fn route_slot(&mut self, drill: &crate::DrillOperation, slot: &DrillSlot) {
        let _ = writeln!(self.text, "G00 X{:.3} Y{:.3}", slot.start.x, slot.start.y);
        let _ = writeln!(
            self.text,
            "G01 Z-{:.3} F{:.1}",
            drill.depth_mm, drill.plunge_mm_min
        );
        let _ = writeln!(
            self.text,
            "G01 X{:.3} Y{:.3} F{:.1}",
            slot.end.x, slot.end.y, drill.feed_mm_min
        );
        if drill.dwell_s > 0.0 {
            let _ = writeln!(self.text, "G04 P{:.2}", drill.dwell_s);
        }
        let _ = writeln!(self.text, "G00 Z{:.3}", drill.retract_mm);
        self.slot_count += 1;
        self.cut_length_mm += slot.start.distance(slot.end);
    }

    fn cut_path(&mut self, path: &Path, depth: f64, retract: f64, feed: f64, plunge: f64) {
        let Some(start) = path.points.first().copied() else {
            return;
        };
        let _ = writeln!(self.text, "G00 Z{retract:.3}");
        let _ = writeln!(self.text, "G00 X{:.3} Y{:.3}", start.x, start.y);
        let _ = writeln!(self.text, "G01 Z-{depth:.3} F{plunge:.1}");

        for point in path.points.iter().skip(1) {
            let _ = writeln!(self.text, "G01 X{:.3} Y{:.3} F{feed:.1}", point.x, point.y);
        }
        if path.closed
            && let Some(last) = path.points.last()
            && last.distance(start) > 1e-7
        {
            let _ = writeln!(self.text, "G01 X{:.3} Y{:.3} F{feed:.1}", start.x, start.y);
        }
        let _ = writeln!(self.text, "G00 Z{retract:.3}");

        self.path_count += 1;
        self.cut_length_mm += path.length();
    }

    fn finish(mut self) -> GcodeProgram {
        let _ = writeln!(self.text);
        let _ = writeln!(self.text, "G00 Z{:.3}", self.last_safe_z.max(0.0));
        let _ = writeln!(self.text, "M5");
        let _ = writeln!(self.text, "G00 X0.000 Y0.000");
        let _ = writeln!(self.text, "M30");

        GcodeProgram {
            text: self.text,
            tool_count: self.tools.len(),
            task_count: self.task_count,
            hole_count: self.hole_count,
            slot_count: self.slot_count,
            path_count: self.path_count,
            cut_length_mm: self.cut_length_mm,
        }
    }
}

struct ToolGroup {
    diameter_mm: f64,
    holes: Vec<DrillHit>,
    slots: Vec<DrillSlot>,
}

fn drill_groups(excellon: &ExcellonLayer) -> Result<BTreeMap<i64, ToolGroup>, String> {
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
    Ok(groups)
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

/// Count drill hits and slots in an Excellon layer.
pub fn drill_hit_count(layer: &ExcellonLayer) -> usize {
    layer.holes.len() + layer.slots.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{
        Aperture, DrillHit, DrillSlot, DrillTool, GerberLayer, LayerGeometry, Point, Primitive,
    };
    use crate::operation::{CutoutOperation, DrillOperation, EngraveOperation, OperationKind};
    use crate::{ContourSide, Project};

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

    fn project_with_outline() -> (Project, crate::OperationId) {
        let mut project = Project::new("Outline");
        let path = Path::closed(vec![
            Point::new(0.0, 0.0),
            Point::new(80.0, 0.0),
            Point::new(80.0, 50.0),
            Point::new(0.0, 50.0),
        ]);
        let layer = LayerGeometry::Gerber(GerberLayer {
            primitives: vec![Primitive::Stroke {
                path,
                aperture: Aperture::circle(0.2),
                polarity: crate::Polarity::Dark,
            }],
            source_units: Some(crate::Unit::Millimeter),
            warnings: Vec::new(),
        });
        let layer_id = project.add_layer("Outline", None, None, layer);
        let task = project.add_operation(
            "Cutout",
            OperationKind::Cutout(CutoutOperation {
                layer: layer_id,
                side: ContourSide::Outside,
                ..CutoutOperation::new(layer_id)
            }),
        );
        (project, task)
    }

    fn project_with_engrave() -> (Project, crate::OperationId) {
        let mut project = Project::new("Engrave");
        let path = Path::new(vec![
            Point::new(1.0, 1.0),
            Point::new(10.0, 1.0),
            Point::new(10.0, 8.0),
        ]);
        let layer = LayerGeometry::Gerber(GerberLayer {
            primitives: vec![Primitive::Stroke {
                path,
                aperture: Aperture::circle(0.2),
                polarity: crate::Polarity::Dark,
            }],
            source_units: Some(crate::Unit::Millimeter),
            warnings: Vec::new(),
        });
        let layer_id = project.add_layer("Silk", None, None, layer);
        let task = project.add_operation(
            "Engrave",
            OperationKind::Engrave(EngraveOperation::new(layer_id)),
        );
        (project, task)
    }

    #[test]
    fn drill_program_contains_tools_and_holes() {
        let (project, id) = project_with_drills();
        let program = generate_task_gcode(&project, project.task(id).unwrap()).unwrap();
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
    fn contour_cut_offsets_outside_the_outline() {
        let (project, id) = project_with_outline();
        let program = generate_task_gcode(&project, project.task(id).unwrap()).unwrap();
        assert_eq!(program.task_count, 1);
        assert_eq!(program.path_count, 1);
        assert!(program.text.contains("T1 M6"));
        assert!(program.text.contains("X-0.500 Y-0.500"));
        assert!(program.text.contains("G01 Z-1.800"));
    }

    #[test]
    fn engrave_follows_paths() {
        let (project, id) = project_with_engrave();
        let program = generate_task_gcode(&project, project.task(id).unwrap()).unwrap();
        assert_eq!(program.path_count, 1);
        assert!(program.text.contains("X10.000 Y8.000"));
    }

    #[test]
    fn combined_program_changes_tools() {
        let mut project = Project::new("Combined");
        let drill_layer = LayerGeometry::Excellon(ExcellonLayer {
            tools: vec![DrillTool {
                number: 1,
                diameter_mm: 0.8,
            }],
            holes: vec![DrillHit {
                position: Point::new(10.0, 10.0),
                tool: 1,
                diameter_mm: 0.8,
            }],
            slots: Vec::new(),
            source_units: Some(crate::Unit::Millimeter),
            warnings: Vec::new(),
        });
        let drill_id = project.add_layer("Drills", None, None, drill_layer);
        project.add_operation("Drill", OperationKind::Drill(DrillOperation::new(drill_id)));

        let outline_path = Path::closed(vec![
            Point::new(0.0, 0.0),
            Point::new(80.0, 0.0),
            Point::new(80.0, 50.0),
            Point::new(0.0, 50.0),
        ]);
        let outline_layer = LayerGeometry::Gerber(GerberLayer {
            primitives: vec![Primitive::Stroke {
                path: outline_path,
                aperture: Aperture::circle(0.2),
                polarity: crate::Polarity::Dark,
            }],
            source_units: Some(crate::Unit::Millimeter),
            warnings: Vec::new(),
        });
        let outline_id = project.add_layer("Outline", None, None, outline_layer);
        project.add_operation(
            "Cutout",
            OperationKind::Cutout(CutoutOperation::new(outline_id)),
        );

        let program = generate_all_gcode(&project).unwrap();
        assert_eq!(program.task_count, 2);
        assert_eq!(program.tool_count, 2);
        assert_eq!(program.text.matches("M6").count(), 2);
    }

    #[test]
    fn disabled_task_is_refused() {
        let (mut project, id) = project_with_drills();
        project.operation_mut(id).unwrap().set_enabled(false);
        let error = generate_task_gcode(&project, project.task(id).unwrap()).unwrap_err();
        assert!(error.contains("disabled"));
    }
}
