//! G-code generation for project tasks.
//!
//! Each task references one machining tool from the user library. A program
//! can contain one task or the whole enabled task timeline; tools are numbered
//! by first use and a tool change (`M6`) is emitted whenever the task tool
//! changes. Source tool numbers from Excellon are used only as a filter.

use std::fmt::Write as _;

use crate::geometry::{
    DrillHit, DrillSlot, ExcellonLayer, LayerGeometry, Path, Point, offset_closed_path,
};
use crate::operation::{ContourSide, Operation, OperationKind};
use crate::project::Project;
use crate::tool::{Tool, ToolId, ToolLibrary};

/// A generated NC program and its statistics.
#[derive(Debug, Clone, PartialEq)]
pub struct GcodeProgram {
    /// Complete G-code text.
    pub text: String,
    /// Number of distinct machining tools used.
    pub tool_count: usize,
    /// Number of tasks included.
    pub task_count: usize,
    /// Number of drilled holes (including helical holes).
    pub hole_count: usize,
    /// Number of routed slots.
    pub slot_count: usize,
    /// Number of contour/path passes.
    pub path_count: usize,
    /// Number of holes enlarged by circular interpolation.
    pub helical_hole_count: usize,
    /// Total cutting length in millimetres (approximate for arcs).
    pub cut_length_mm: f64,
}

/// Generate a program for one task.
pub fn generate_task_gcode(
    project: &Project,
    library: &ToolLibrary,
    task: &Operation,
) -> Result<GcodeProgram, String> {
    if !task.enabled() {
        return Err("task is disabled".into());
    }
    generate_tasks_gcode(project, library, &[task])
}

/// Generate a combined program for all enabled tasks in project order.
pub fn generate_all_gcode(
    project: &Project,
    library: &ToolLibrary,
) -> Result<GcodeProgram, String> {
    let tasks: Vec<&Operation> = project
        .tasks()
        .iter()
        .filter(|task| task.enabled())
        .collect();
    if tasks.is_empty() {
        return Err("no enabled tasks".into());
    }
    generate_tasks_gcode(project, library, &tasks)
}

/// Generate a program for an explicit list of tasks.
pub fn generate_tasks_gcode(
    project: &Project,
    library: &ToolLibrary,
    tasks: &[&Operation],
) -> Result<GcodeProgram, String> {
    if tasks.is_empty() {
        return Err("no tasks to export".into());
    }
    let mut builder = ProgramBuilder::new(project, library, tasks.len());
    for task in tasks {
        builder.append_task(task)?;
    }
    Ok(builder.finish())
}

/// Backwards-compatible wrapper for drilling tasks.
pub fn generate_drill_gcode(
    project: &Project,
    library: &ToolLibrary,
    task: &Operation,
) -> Result<GcodeProgram, String> {
    if !matches!(task.kind(), OperationKind::Drill(_)) {
        return Err("task is not a drilling task".into());
    }
    generate_task_gcode(project, library, task)
}

struct ProgramBuilder<'a> {
    project: &'a Project,
    library: &'a ToolLibrary,
    text: String,
    tools: Vec<ToolId>,
    current_tool: Option<ToolId>,
    current_spindle: Option<u32>,
    last_safe_z: f64,
    task_count: usize,
    hole_count: usize,
    slot_count: usize,
    path_count: usize,
    helical_hole_count: usize,
    cut_length_mm: f64,
}

impl<'a> ProgramBuilder<'a> {
    fn new(project: &'a Project, library: &'a ToolLibrary, task_count: usize) -> Self {
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
            library,
            text,
            tools: Vec::new(),
            current_tool: None,
            current_spindle: None,
            last_safe_z: 3.0,
            task_count: 0,
            hole_count: 0,
            slot_count: 0,
            path_count: 0,
            helical_hole_count: 0,
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
        task: &Operation,
        drill: &crate::DrillOperation,
    ) -> Result<(), String> {
        let layer = self
            .project
            .layer(drill.layer)
            .ok_or_else(|| "drill task layer no longer exists".to_owned())?;
        let LayerGeometry::Excellon(excellon) = layer.geometry() else {
            return Err("drill task requires an Excellon layer".into());
        };
        let tool = self.task_tool(task)?.clone();
        let selected_holes: Vec<&DrillHit> = excellon
            .holes
            .iter()
            .filter(|hole| drill.source_tools.is_empty() || drill.source_tools.contains(&hole.tool))
            .collect();
        let selected_slots: Vec<&DrillSlot> = excellon
            .slots
            .iter()
            .filter(|slot| drill.source_tools.is_empty() || drill.source_tools.contains(&slot.tool))
            .collect();
        if selected_holes.is_empty() && selected_slots.is_empty() {
            return Err(format!(
                "task `{}` filters out all holes and slots",
                task.name()
            ));
        }

        self.select_tool(&tool, drill.retract_mm, drill.spindle_rpm)?;
        let _ = writeln!(
            self.text,
            "(Source tools: {})",
            source_tool_list(&drill.source_tools)
        );
        let _ = writeln!(self.text, "G00 Z{:.3}", drill.retract_mm);

        for hole in selected_holes {
            if hole.diameter_mm + 1e-6 < tool.diameter_mm() {
                return Err(format!(
                    "machining tool `{}` is larger than source hole {:.3} mm",
                    tool.name(),
                    hole.diameter_mm
                ));
            }
            self.machine_hole(hole, &tool, drill)?;
        }
        for slot in selected_slots {
            if slot.diameter_mm + 1e-6 < tool.diameter_mm() {
                return Err(format!(
                    "machining tool `{}` is larger than source slot {:.3} mm",
                    tool.name(),
                    slot.diameter_mm
                ));
            }
            if slot.diameter_mm > tool.diameter_mm() + 1e-6 {
                let _ = writeln!(
                    self.text,
                    "(Slot {:.3} mm cut on the centreline with a {:.3} mm tool)",
                    slot.diameter_mm,
                    tool.diameter_mm()
                );
            }
            self.route_slot(drill, slot);
        }

        let _ = writeln!(self.text, "G00 Z{:.3}", drill.retract_mm);
        self.last_safe_z = drill.retract_mm;
        Ok(())
    }

    fn machine_hole(
        &mut self,
        hole: &DrillHit,
        tool: &Tool,
        drill: &crate::DrillOperation,
    ) -> Result<(), String> {
        let tool_radius = tool.diameter_mm() * 0.5;
        let hole_radius = hole.diameter_mm * 0.5;
        if hole_radius - tool_radius > 1e-6 {
            if !tool.kind().can_interpolate() {
                return Err(format!(
                    "drill tool `{}` cannot enlarge a {:.3} mm hole; select an end mill or V-bit",
                    tool.name(),
                    hole.diameter_mm
                ));
            }
            self.helical_hole(hole, drill, hole_radius - tool_radius, tool.diameter_mm())?;
            self.helical_hole_count += 1;
            self.hole_count += 1;
            return Ok(());
        }

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
        Ok(())
    }

    fn helical_hole(
        &mut self,
        hole: &DrillHit,
        drill: &crate::DrillOperation,
        path_radius: f64,
        tool_diameter_mm: f64,
    ) -> Result<(), String> {
        if path_radius <= 0.0 {
            return Err("invalid helical path radius".into());
        }
        let passes = (drill.depth_mm / drill.stepdown_mm).ceil().max(1.0);
        let cx = hole.position.x;
        let cy = hole.position.y;
        let left = cx - path_radius;
        let right = cx + path_radius;
        let depth = -drill.depth_mm;

        let _ = writeln!(
            self.text,
            "(Helical mill Ø{:.3} hole with Ø{:.3} tool, {} passes)",
            hole.diameter_mm, tool_diameter_mm, passes as u32
        );
        let _ = writeln!(self.text, "G00 X{right:.3} Y{cy:.3}");
        for pass in 1..=passes as u32 {
            let z = depth * f64::from(pass) / passes;
            let _ = writeln!(self.text, "G01 Z{z:.3} F{:.1}", drill.plunge_mm_min);
            let _ = writeln!(
                self.text,
                "G02 X{left:.3} Y{cy:.3} I-{path_radius:.3} J0.000 F{:.1}",
                drill.feed_mm_min
            );
            let _ = writeln!(
                self.text,
                "G02 X{right:.3} Y{cy:.3} I{path_radius:.3} J0.000 F{:.1}",
                drill.feed_mm_min
            );
        }
        let _ = writeln!(self.text, "G00 Z{:.3}", drill.retract_mm);
        self.path_count += 1;
        self.cut_length_mm += std::f64::consts::TAU * path_radius * passes;
        Ok(())
    }

    fn append_cutout(
        &mut self,
        task: &Operation,
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
        let tool = self.task_tool(task)?.clone();

        let distance = match cutout.side {
            ContourSide::Outside => tool.diameter_mm() * 0.5,
            ContourSide::Inside => -tool.diameter_mm() * 0.5,
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

        self.select_tool(&tool, cutout.retract_mm, cutout.spindle_rpm)?;
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
        task: &Operation,
        engrave: &crate::EngraveOperation,
    ) -> Result<(), String> {
        let layer = self
            .project
            .layer(engrave.layer)
            .ok_or_else(|| "engrave task layer no longer exists".to_owned())?;
        let LayerGeometry::Gerber(gerber) = layer.geometry() else {
            return Err("engraving requires a Gerber layer".into());
        };
        let paths = engrave.toolpaths(gerber);
        if paths.is_empty() {
            return Err("no paths found in the selected layer".into());
        }
        let tool = self.task_tool(task)?.clone();

        self.select_tool(&tool, engrave.retract_mm, engrave.spindle_rpm)?;
        let _ = writeln!(
            self.text,
            "(Engrave {} side, offset {:.3} mm, {} pass{}, step {:.3} mm)",
            engrave.side.label(),
            engrave.offset_mm,
            engrave.passes,
            if engrave.passes == 1 { "" } else { "es" },
            engrave.stepover_mm
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

    fn task_tool(&self, task: &Operation) -> Result<&Tool, String> {
        let tool_id = task.tool();
        self.library
            .tool(tool_id)
            .ok_or_else(|| format!("task `{}` references a missing tool", task.name()))
    }

    fn select_tool(&mut self, tool: &Tool, safe_z: f64, rpm: u32) -> Result<(), String> {
        if self.current_tool != Some(tool.id()) {
            if self.current_tool.is_some() {
                let _ = writeln!(self.text, "G00 Z{safe_z:.3}");
                let _ = writeln!(self.text, "M5");
            }
            let index = self
                .tools
                .iter()
                .position(|id| *id == tool.id())
                .unwrap_or_else(|| {
                    self.tools.push(tool.id());
                    self.tools.len() - 1
                });
            let number = index + 1;
            let _ = writeln!(
                self.text,
                "T{number} M6 (T{number}: {}, Ø{:.3} mm, {})",
                tool.name(),
                tool.diameter_mm(),
                tool.kind().label()
            );
            self.current_tool = Some(tool.id());
            self.current_spindle = None;
        }
        if self.current_spindle != Some(rpm) {
            let _ = writeln!(self.text, "S{rpm} M3");
            self.current_spindle = Some(rpm);
        }
        Ok(())
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
            helical_hole_count: self.helical_hole_count,
            cut_length_mm: self.cut_length_mm,
        }
    }
}

fn source_tool_list(tools: &std::collections::BTreeSet<u32>) -> String {
    if tools.is_empty() {
        "all".to_owned()
    } else {
        tools
            .iter()
            .map(|number| format!("T{number}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
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
    use crate::Project;
    use crate::geometry::{
        Aperture, DrillHit, DrillSlot, DrillTool, GerberLayer, LayerGeometry, Point, Primitive,
    };
    use crate::operation::{CutoutOperation, DrillOperation, EngraveOperation, OperationKind};
    use crate::tool::ToolKind;

    fn project_with_drills() -> (Project, ToolLibrary, crate::OperationId) {
        let mut project = Project::new("Test");
        let layer = LayerGeometry::Excellon(ExcellonLayer {
            tools: vec![
                DrillTool {
                    number: 1,
                    diameter_mm: 0.8,
                },
                DrillTool {
                    number: 2,
                    diameter_mm: 1.0,
                },
            ],
            holes: vec![
                DrillHit {
                    position: Point::new(10.0, 10.0),
                    tool: 1,
                    diameter_mm: 0.8,
                },
                DrillHit {
                    position: Point::new(20.0, 10.0),
                    tool: 2,
                    diameter_mm: 1.0,
                },
            ],
            slots: vec![DrillSlot {
                start: Point::new(30.0, 10.0),
                end: Point::new(35.0, 10.0),
                tool: 2,
                diameter_mm: 1.0,
            }],
            source_units: Some(crate::Unit::Millimeter),
            warnings: Vec::new(),
        });
        let layer_id = project.add_layer("Drills", None, None, layer);
        let library = ToolLibrary::new();
        let tool = library.closest(ToolKind::Drill, 0.8).unwrap();
        let mut drill = DrillOperation::new(layer_id, tool);
        drill.source_tools.insert(1);
        let operation = project.add_task("Drill", OperationKind::Drill(drill));
        (project, library, operation)
    }

    fn project_with_helical() -> (Project, ToolLibrary, crate::OperationId) {
        let mut project = Project::new("Helical");
        let layer = LayerGeometry::Excellon(ExcellonLayer {
            tools: vec![
                DrillTool {
                    number: 1,
                    diameter_mm: 1.0,
                },
                DrillTool {
                    number: 2,
                    diameter_mm: 1.5,
                },
            ],
            holes: vec![
                DrillHit {
                    position: Point::new(10.0, 10.0),
                    tool: 1,
                    diameter_mm: 1.0,
                },
                DrillHit {
                    position: Point::new(20.0, 10.0),
                    tool: 2,
                    diameter_mm: 1.5,
                },
            ],
            slots: Vec::new(),
            source_units: Some(crate::Unit::Millimeter),
            warnings: Vec::new(),
        });
        let layer_id = project.add_layer("Drills", None, None, layer);
        let library = ToolLibrary::new();
        let tool = library.closest(ToolKind::EndMill, 1.0).unwrap();
        let mut drill = DrillOperation::new(layer_id, tool);
        drill.source_tools.extend([1, 2]);
        let operation = project.add_task("Drill large", OperationKind::Drill(drill));
        (project, library, operation)
    }

    fn project_with_outline() -> (Project, ToolLibrary, crate::OperationId) {
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
        let library = ToolLibrary::new();
        let tool = library.closest(ToolKind::EndMill, 1.0).unwrap();
        let task = project.add_task(
            "Cutout",
            OperationKind::Cutout(CutoutOperation::new(layer_id, tool)),
        );
        (project, library, task)
    }

    fn project_with_engrave() -> (Project, ToolLibrary, crate::OperationId) {
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
        let library = ToolLibrary::new();
        let tool = library.closest(ToolKind::VBit, 0.2).unwrap();
        let task = project.add_task(
            "Engrave",
            OperationKind::Engrave(EngraveOperation::new(layer_id, tool)),
        );
        (project, library, task)
    }

    #[test]
    fn drill_program_uses_task_tool_and_source_filter() {
        let (project, library, id) = project_with_drills();
        let program = generate_task_gcode(&project, &library, project.task(id).unwrap()).unwrap();
        assert_eq!(program.hole_count, 1);
        assert_eq!(program.slot_count, 0);
        assert_eq!(program.tool_count, 1);
        assert!(program.text.contains("T1 M6"));
        assert!(program.text.contains("X10.000 Y10.000"));
        assert!(program.text.contains("Source tools: T1"));
        assert!(program.text.ends_with("M30\n"));
    }

    #[test]
    fn end_mill_can_helically_clear_a_larger_hole() {
        let (project, library, id) = project_with_helical();
        let program = generate_task_gcode(&project, &library, project.task(id).unwrap()).unwrap();
        assert_eq!(program.hole_count, 2);
        assert_eq!(program.helical_hole_count, 1);
        assert!(program.text.contains("Helical mill"));
        assert!(program.text.contains("Ø1.500 hole"));
        assert!(program.text.contains("Ø1.000 tool"));
        assert!(program.text.contains("G02"));
    }

    #[test]
    fn drill_tool_cannot_enlarge_a_hole() {
        let (mut project, library, id) = project_with_helical();
        let _ = &mut project;
        let task = project.task_mut(id).unwrap();
        let OperationKind::Drill(drill) = task.kind_mut() else {
            panic!("test task is not drilling");
        };
        drill.tool = library.closest(ToolKind::Drill, 0.8).unwrap();
        let error = generate_task_gcode(&project, &library, project.task(id).unwrap()).unwrap_err();
        assert!(error.contains("cannot enlarge"));
    }

    #[test]
    fn contour_cut_offsets_outside_the_outline() {
        let (project, library, id) = project_with_outline();
        let program = generate_task_gcode(&project, &library, project.task(id).unwrap()).unwrap();
        assert_eq!(program.task_count, 1);
        assert_eq!(program.path_count, 1);
        assert!(program.text.contains("T1 M6"));
        assert!(program.text.contains("X-0.500 Y-0.500"));
        assert!(program.text.contains("G01 Z-1.800"));
    }

    #[test]
    fn engrave_follows_paths() {
        let (project, library, id) = project_with_engrave();
        let program = generate_task_gcode(&project, &library, project.task(id).unwrap()).unwrap();
        assert_eq!(program.path_count, 1);
        assert!(program.text.contains("X10.000 Y8.000"));
    }

    #[test]
    fn engrave_multiple_offset_passes_are_emitted() {
        let (mut project, library, cutout_id) = project_with_outline();
        let layer = project.task(cutout_id).unwrap().source_layer();
        let tool = library.closest(crate::ToolKind::VBit, 0.2).unwrap();
        let engrave_id = project.add_task(
            "Engrave outline",
            OperationKind::Engrave(EngraveOperation::new(layer, tool)),
        );
        let OperationKind::Engrave(engrave) = project.task_mut(engrave_id).unwrap().kind_mut()
        else {
            panic!("expected engrave task");
        };
        engrave.side = crate::EngraveSide::Outside;
        engrave.offset_mm = 0.1;
        engrave.passes = 3;
        engrave.stepover_mm = 0.2;

        let program =
            generate_task_gcode(&project, &library, project.task(engrave_id).unwrap()).unwrap();
        assert_eq!(program.path_count, 3);
        assert!(program.text.contains("offset 0.100 mm, 3 passes"));
        assert!(program.text.contains("X-0.100 Y-0.100"));
    }

    #[test]
    fn combined_program_changes_tools() {
        let (mut project, library, _drill_id) = project_with_drills();

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
        let cutout_tool = library.closest(ToolKind::EndMill, 1.0).unwrap();
        project.add_task(
            "Cutout",
            OperationKind::Cutout(CutoutOperation::new(outline_id, cutout_tool)),
        );

        let program = generate_all_gcode(&project, &library).unwrap();
        assert_eq!(program.task_count, 2);
        assert_eq!(program.tool_count, 2);
        assert_eq!(program.text.matches("M6").count(), 2);
    }

    #[test]
    fn disabled_task_is_refused() {
        let (mut project, library, id) = project_with_drills();
        project.operation_mut(id).unwrap().set_enabled(false);
        let error = generate_task_gcode(&project, &library, project.task(id).unwrap()).unwrap_err();
        assert!(error.contains("disabled"));
    }

    #[test]
    fn tool_library_is_required() {
        let (_project, _library, _id) = project_with_drills();
        let mut empty = ToolLibrary::new();
        let ids: Vec<_> = empty.tools().iter().map(|tool| tool.id()).collect();
        for id in ids {
            empty.remove(id);
        }
        let (project, _library, id) = project_with_drills();
        let error = generate_task_gcode(&project, &empty, project.task(id).unwrap()).unwrap_err();
        assert!(error.contains("missing tool"));
    }

    #[test]
    fn source_tool_filter_ignores_other_holes() {
        let (project, library, id) = project_with_drills();
        let program = generate_task_gcode(&project, &library, project.task(id).unwrap()).unwrap();
        assert!(!program.text.contains("X20.000 Y10.000"));
    }
}
