//! G-code generation for project tasks.
//!
//! Each task references one machining tool from the user library. A program
//! can contain one task or the whole enabled task timeline; tools are numbered
//! by first use and a tool change (`M6`) is emitted whenever the task tool
//! changes. Source tool numbers from Excellon are used only as a filter.

use std::fmt::Write as _;

use crate::geometry::{DrillHit, DrillSlot, ExcellonLayer, LayerGeometry, Path, Point};
use crate::operation::{Operation, OperationKind};
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
    /// Task blocks in emission order.
    pub tasks: Vec<GcodeProgramTask>,
    /// Tools in program order.
    pub tools: Vec<GcodeProgramTool>,
}

/// One task block inside a generated program.
#[derive(Debug, Clone, PartialEq)]
pub struct GcodeProgramTask {
    /// Task display name.
    pub name: String,
    /// Task type label, for example `Drill` or `Clearance`.
    pub kind: String,
    /// First line of the block (zero based).
    pub line_start: usize,
    /// Line after the block (zero based, exclusive).
    pub line_end: usize,
    /// Program tool number used by the task, if it was selected.
    pub tool_number: Option<usize>,
    /// Name of the task machining tool.
    pub tool_name: Option<String>,
}

/// One machining tool referenced by a generated program.
#[derive(Debug, Clone, PartialEq)]
pub struct GcodeProgramTool {
    /// Program tool number (`T1` is 1).
    pub number: usize,
    /// Tool display name.
    pub name: String,
    /// Tool diameter in millimetres.
    pub diameter_mm: f64,
    /// Tool type label.
    pub kind: String,
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
    tool_entries: Vec<GcodeProgramTool>,
    task_entries: Vec<GcodeProgramTask>,
    current_tool: Option<ToolId>,
    current_spindle: Option<u32>,
    last_safe_z: f64,
    task_count: usize,
    hole_count: usize,
    slot_count: usize,
    path_count: usize,
    helical_hole_count: usize,
    cut_length_mm: f64,
    origin: Point,
    rapid_feed_mm_min: f64,
    position: Point,
}

impl<'a> ProgramBuilder<'a> {
    fn new(project: &'a Project, library: &'a ToolLibrary, task_count: usize) -> Self {
        let mut text = String::new();
        let _ = writeln!(
            text,
            "(PCB-CAM {} - generated program)",
            env!("CARGO_PKG_VERSION")
        );
        let _ = writeln!(text, "(Project: {})", project.name());
        let _ = writeln!(text, "(Tasks: {task_count})");
        let origin = project.gcode_origin();
        let machining = *library.machining();
        let _ = writeln!(text, "(G-code 0: X{:.3} Y{:.3} mm)", origin.x, origin.y);
        let _ = writeln!(
            text,
            "(Travel feed: {:.0} mm/min)",
            machining.rapid_feed_mm_min
        );
        let _ = writeln!(text, "(Safe Z: {:.3} mm)", machining.safe_z_mm);
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
            tool_entries: Vec::new(),
            task_entries: Vec::new(),
            current_tool: None,
            current_spindle: None,
            last_safe_z: machining.safe_z_mm,
            task_count: 0,
            hole_count: 0,
            slot_count: 0,
            path_count: 0,
            helical_hole_count: 0,
            cut_length_mm: 0.0,
            origin,
            rapid_feed_mm_min: machining.rapid_feed_mm_min,
            position: origin,
        }
    }

    fn travel_xy(&mut self, x: f64, y: f64) {
        self.position = Point::new(x + self.origin.x, y + self.origin.y);
        if self.rapid_feed_mm_min > 0.0 {
            let _ = writeln!(
                self.text,
                "G01 X{x:.3} Y{y:.3} F{:.1}",
                self.rapid_feed_mm_min
            );
        } else {
            let _ = writeln!(self.text, "G00 X{x:.3} Y{y:.3}");
        }
    }

    fn gx(&self, value: f64) -> f64 {
        value - self.origin.x
    }

    fn gy(&self, value: f64) -> f64 {
        value - self.origin.y
    }

    fn append_task(&mut self, task: &Operation) -> Result<(), String> {
        task.validate()?;
        if !task.enabled() {
            return Err(format!("task `{}` is disabled", task.name()));
        }
        let _ = writeln!(self.text);
        let line_start = self.text.lines().count();
        let _ = writeln!(self.text, "({} - {})", task.kind_label(), task.name());
        match task.kind() {
            OperationKind::Drill(drill) => self.append_drill(task, drill)?,
            OperationKind::SpotDrill(spot) => self.append_spot_drill(task, spot)?,
            OperationKind::HoleMill(mill) => self.append_hole_mill(task, mill)?,
            OperationKind::Cutout(cutout) => self.append_cutout(task, cutout)?,
            OperationKind::Engrave(engrave) => self.append_engrave(task, engrave)?,
        }
        self.task_count += 1;
        self.task_entries.push(GcodeProgramTask {
            name: task.name().to_owned(),
            kind: task.kind_label().to_owned(),
            line_start,
            line_end: self.text.lines().count(),
            tool_number: self.tool_number(task.tool()),
            tool_name: self
                .library
                .tool(task.tool())
                .map(|tool| tool.name().to_owned()),
        });
        Ok(())
    }

    fn tool_number(&self, id: ToolId) -> Option<usize> {
        self.tools
            .iter()
            .position(|candidate| *candidate == id)
            .map(|index| index + 1)
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
        if tool.kind() != crate::ToolKind::Drill {
            return Err(format!(
                "drilling requires a drill bit; `{}` is {}",
                tool.name(),
                tool.kind().label()
            ));
        }
        let selected_holes: Vec<&DrillHit> = excellon
            .holes
            .iter()
            .filter(|hole| drill.source_tools.is_empty() || drill.source_tools.contains(&hole.tool))
            .collect();
        let selected_slots = excellon
            .slots
            .iter()
            .filter(|slot| drill.source_tools.is_empty() || drill.source_tools.contains(&slot.tool))
            .count();
        if selected_holes.is_empty() {
            return Err(format!(
                "task `{}` has no selected holes to drill",
                task.name()
            ));
        }
        if selected_slots > 0 {
            return Err(format!(
                "task `{}` selects {selected_slots} slot(s); use a hole-milling task for slots",
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

        let selected_holes = order_holes_nearest(selected_holes, self.position);
        for hole in selected_holes {
            if (hole.diameter_mm - tool.diameter_mm()).abs() > 1e-6 {
                return Err(format!(
                    "drill Ø{:.3} mm does not match source hole {:.3} mm; use hole milling or a matching drill",
                    tool.diameter_mm(),
                    hole.diameter_mm
                ));
            }
            let x = self.gx(hole.position.x);
            let y = self.gy(hole.position.y);
            self.travel_xy(x, y);
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

        let _ = writeln!(self.text, "G00 Z{:.3}", drill.retract_mm);
        self.last_safe_z = drill.retract_mm;
        Ok(())
    }

    fn append_hole_mill(
        &mut self,
        task: &Operation,
        mill: &crate::HoleMillOperation,
    ) -> Result<(), String> {
        let layer = self
            .project
            .layer(mill.layer)
            .ok_or_else(|| "hole-milling task layer no longer exists".to_owned())?;
        let LayerGeometry::Excellon(excellon) = layer.geometry() else {
            return Err("hole milling requires an Excellon layer".into());
        };
        let tool = self.task_tool(task)?.clone();
        if !tool.kind().can_interpolate() {
            return Err(format!(
                "hole milling requires an end mill or V-bit; `{}` is {}",
                tool.name(),
                tool.kind().label()
            ));
        }
        let selected_holes: Vec<&DrillHit> = excellon
            .holes
            .iter()
            .filter(|hole| mill.source_tools.is_empty() || mill.source_tools.contains(&hole.tool))
            .collect();
        let selected_slots: Vec<&DrillSlot> = excellon
            .slots
            .iter()
            .filter(|slot| mill.source_tools.is_empty() || mill.source_tools.contains(&slot.tool))
            .collect();
        if selected_holes.is_empty() && selected_slots.is_empty() {
            return Err(format!(
                "task `{}` filters out all holes and slots",
                task.name()
            ));
        }

        self.select_tool(&tool, mill.retract_mm, mill.spindle_rpm)?;
        let _ = writeln!(
            self.text,
            "(Source tools: {})",
            source_tool_list(&mill.source_tools)
        );
        let _ = writeln!(self.text, "G00 Z{:.3}", mill.retract_mm);

        for hole in selected_holes {
            if hole.diameter_mm + 1e-6 < tool.diameter_mm() {
                return Err(format!(
                    "machining tool `{}` is larger than source hole {:.3} mm",
                    tool.name(),
                    hole.diameter_mm
                ));
            }
            self.machine_hole(hole, &tool, mill)?;
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
            self.route_slot(
                slot,
                mill.depth_mm,
                mill.retract_mm,
                mill.feed_mm_min,
                mill.plunge_mm_min,
                mill.dwell_s,
            );
        }

        let _ = writeln!(self.text, "G00 Z{:.3}", mill.retract_mm);
        self.last_safe_z = mill.retract_mm;
        Ok(())
    }

    fn append_spot_drill(
        &mut self,
        task: &Operation,
        spot: &crate::SpotDrillOperation,
    ) -> Result<(), String> {
        let layer = self
            .project
            .layer(spot.layer)
            .ok_or_else(|| "spot drill task layer no longer exists".to_owned())?;
        let LayerGeometry::Excellon(excellon) = layer.geometry() else {
            return Err("spot drilling requires an Excellon layer".into());
        };
        let selected_holes: Vec<&DrillHit> = excellon
            .holes
            .iter()
            .filter(|hole| spot.source_tools.is_empty() || spot.source_tools.contains(&hole.tool))
            .collect();
        let skipped_slots = excellon
            .slots
            .iter()
            .filter(|slot| spot.source_tools.is_empty() || spot.source_tools.contains(&slot.tool))
            .count();
        if selected_holes.is_empty() {
            return Err(format!(
                "task `{}` has no selected holes to spot-drill",
                task.name()
            ));
        }
        let tool = self.task_tool(task)?.clone();
        self.select_tool(&tool, spot.retract_mm, spot.spindle_rpm)?;
        let _ = writeln!(
            self.text,
            "(Source holes: {})",
            source_tool_list(&spot.source_tools)
        );
        if skipped_slots > 0 {
            let _ = writeln!(
                self.text,
                "({skipped_slots} selected slot{} skipped: spot drilling uses hole centres)",
                if skipped_slots == 1 { "" } else { "s" }
            );
        }
        let _ = writeln!(self.text, "G00 Z{:.3}", spot.retract_mm);

        let selected_holes = order_holes_nearest(selected_holes, self.position);
        for hole in selected_holes {
            let x = self.gx(hole.position.x);
            let y = self.gy(hole.position.y);
            self.travel_xy(x, y);
            let _ = writeln!(
                self.text,
                "G01 Z-{:.3} F{:.1}",
                spot.depth_mm, spot.plunge_mm_min
            );
            if spot.dwell_s > 0.0 {
                let _ = writeln!(self.text, "G04 P{:.2}", spot.dwell_s);
            }
            let _ = writeln!(self.text, "G00 Z{:.3}", spot.retract_mm);
            self.hole_count += 1;
        }
        let _ = writeln!(self.text, "G00 Z{:.3}", spot.retract_mm);
        self.last_safe_z = spot.retract_mm;
        Ok(())
    }

    fn machine_hole(
        &mut self,
        hole: &DrillHit,
        tool: &Tool,
        mill: &crate::HoleMillOperation,
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
            self.helical_hole(hole, mill, hole_radius - tool_radius, tool.diameter_mm())?;
            self.helical_hole_count += 1;
            self.hole_count += 1;
            return Ok(());
        }

        let x = self.gx(hole.position.x);
        let y = self.gy(hole.position.y);
        self.travel_xy(x, y);
        let _ = writeln!(
            self.text,
            "G01 Z-{:.3} F{:.1}",
            mill.depth_mm, mill.plunge_mm_min
        );
        if mill.dwell_s > 0.0 {
            let _ = writeln!(self.text, "G04 P{:.2}", mill.dwell_s);
        }
        let _ = writeln!(self.text, "G00 Z{:.3}", mill.retract_mm);
        self.hole_count += 1;
        Ok(())
    }

    fn helical_hole(
        &mut self,
        hole: &DrillHit,
        mill: &crate::HoleMillOperation,
        path_radius: f64,
        tool_diameter_mm: f64,
    ) -> Result<(), String> {
        if path_radius <= 0.0 {
            return Err("invalid helical path radius".into());
        }
        let passes = (mill.depth_mm / mill.stepdown_mm).ceil().max(1.0);
        let cx = self.gx(hole.position.x);
        let cy = self.gy(hole.position.y);
        let left = cx - path_radius;
        let right = cx + path_radius;
        let depth = -mill.depth_mm;

        let _ = writeln!(
            self.text,
            "(Helical mill Ø{:.3} hole with Ø{:.3} tool, {} passes)",
            hole.diameter_mm, tool_diameter_mm, passes as u32
        );
        self.travel_xy(right, cy);
        for pass in 1..=passes as u32 {
            let z = depth * f64::from(pass) / passes;
            let _ = writeln!(self.text, "G01 Z{z:.3} F{:.1}", mill.plunge_mm_min);
            let _ = writeln!(
                self.text,
                "G02 X{left:.3} Y{cy:.3} I-{path_radius:.3} J0.000 F{:.1}",
                mill.feed_mm_min
            );
            let _ = writeln!(
                self.text,
                "G02 X{right:.3} Y{cy:.3} I{path_radius:.3} J0.000 F{:.1}",
                mill.feed_mm_min
            );
        }
        let _ = writeln!(self.text, "G00 Z{:.3}", mill.retract_mm);
        self.position = hole.position;
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
            return Err("edge cut requires a Gerber layer".into());
        };
        let tool = self.task_tool(task)?.clone();
        let paths = cutout.toolpaths(gerber);
        if paths.is_empty() {
            return Err("could not offset the layer contours".into());
        }

        self.select_tool(&tool, cutout.retract_mm, cutout.spindle_rpm)?;
        let _ = writeln!(self.text, "(Side: {})", cutout.side.label());
        let pass_count = cutout.pass_count();
        let _ = writeln!(
            self.text,
            "(Depth passes: {pass_count} × {:.3} mm step, total {:.3} mm)",
            cutout.stepdown_mm, cutout.depth_mm
        );
        if cutout.has_bridges() {
            let _ = writeln!(
                self.text,
                "(Bridges: {} per contour, {:.3} mm long, raised to Z-{:.3})",
                cutout.bridge_count, cutout.bridge_length_mm, cutout.bridge_start_depth_mm
            );
        }
        for pass in 1..=pass_count {
            let depth = cutout.pass_depth(pass);
            if pass_count > 1 {
                let _ = writeln!(
                    self.text,
                    "(Depth pass {pass}/{pass_count} at Z-{depth:.3})"
                );
            }
            for path in &paths {
                self.cut_contour(path, cutout, depth);
            }
        }
        self.last_safe_z = cutout.retract_mm;
        Ok(())
    }

    /// Cut one contour at `depth`, leaving shallow bridges where the task
    /// requests them.
    ///
    /// Passes that do not reach the bridge start depth cut the whole contour:
    /// they only remove material above the future tab.
    fn cut_contour(&mut self, path: &Path, cutout: &crate::CutoutOperation, depth: f64) {
        let bridge_depth = cutout.bridge_start_depth_mm;
        let bridges = if cutout.has_bridges() && depth > bridge_depth + 1e-9 {
            cutout.bridges(path)
        } else {
            Vec::new()
        };
        if bridges.is_empty() {
            self.cut_path(
                path,
                depth,
                cutout.retract_mm,
                cutout.feed_mm_min,
                cutout.plunge_mm_min,
            );
            return;
        }

        let perimeter = path.length();
        if perimeter <= 1e-9 {
            return;
        }
        let retract = cutout.retract_mm;
        let feed = cutout.feed_mm_min;
        let plunge = cutout.plunge_mm_min;
        let count = bridges.len();

        // Start just after the first bridge so the contour is entered on a cut
        // span; the same bridge is crossed again when the loop closes.
        let start_mm = bridges[0].end_mm;
        let Some(start) = path.point_at_distance(start_mm) else {
            return;
        };
        let _ = writeln!(self.text, "G00 Z{retract:.3}");
        self.travel_xy(self.gx(start.x), self.gy(start.y));
        let _ = writeln!(self.text, "G01 Z-{depth:.3} F{plunge:.1}");

        let mut cursor = start_mm;
        let mut cut_length_mm = 0.0;
        for step in 0..count {
            let next_bridge = &bridges[(step + 1) % count];
            let cut_end_mm = if step + 1 == count {
                bridges[0].start_mm + perimeter
            } else {
                next_bridge.start_mm
            };
            for point in path.points_between(cursor, cut_end_mm).iter().skip(1) {
                let _ = writeln!(
                    self.text,
                    "G01 X{:.3} Y{:.3} F{feed:.1}",
                    self.gx(point.x),
                    self.gy(point.y)
                );
            }
            cut_length_mm += (cut_end_mm - cursor).max(0.0);

            let bridge = next_bridge;
            let _ = writeln!(self.text, "G01 Z-{bridge_depth:.3} F{plunge:.1}");
            for point in path
                .points_between(bridge.start_mm, bridge.end_mm)
                .iter()
                .skip(1)
            {
                let _ = writeln!(
                    self.text,
                    "G01 X{:.3} Y{:.3} F{feed:.1}",
                    self.gx(point.x),
                    self.gy(point.y)
                );
            }
            cut_length_mm += bridge.length_mm();
            let _ = writeln!(self.text, "G01 Z-{depth:.3} F{plunge:.1}");
            cursor = bridge.end_mm;
        }
        let _ = writeln!(self.text, "G00 Z{retract:.3}");

        self.position = start;
        self.path_count += 1;
        self.cut_length_mm += cut_length_mm;
    }

    fn append_engrave(
        &mut self,
        task: &Operation,
        engrave: &crate::EngraveOperation,
    ) -> Result<(), String> {
        let layer = self
            .project
            .layer(engrave.layer)
            .ok_or_else(|| "clearance task layer no longer exists".to_owned())?;
        let LayerGeometry::Gerber(gerber) = layer.geometry() else {
            return Err("clearance requires a Gerber layer".into());
        };
        let tool = self.task_tool(task)?.clone();
        let boundary = engrave.boundary_path(self.project);
        let pass_count = engrave.pass_count();
        let passes = gerber.isolation_passes(
            engrave.outline_offset_mm,
            engrave.offset_mm,
            pass_count,
            engrave.stepover_mm,
            engrave.side.sign(),
            boundary.as_ref(),
        );
        if passes.iter().all(Vec::is_empty) {
            return Err("no clearance paths found in the selected layer".into());
        }

        self.select_tool(&tool, engrave.retract_mm, engrave.spindle_rpm)?;
        let _ = writeln!(
            self.text,
            "(Clearance {} side, offset {:.3} mm, width {:.3} mm, {} pass{}, step {:.3} mm)",
            engrave.side.label(),
            engrave.offset_mm,
            engrave.clearance_width_mm,
            pass_count,
            if pass_count == 1 { "" } else { "es" },
            engrave.stepover_mm
        );
        for pass in passes {
            let ordered = order_paths_nearest(pass, self.position);
            for path in &ordered {
                self.cut_path(
                    path,
                    engrave.depth_mm,
                    engrave.retract_mm,
                    engrave.feed_mm_min,
                    engrave.plunge_mm_min,
                );
            }
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
            let index = match self.tools.iter().position(|id| *id == tool.id()) {
                Some(index) => index,
                None => {
                    self.tools.push(tool.id());
                    let index = self.tools.len() - 1;
                    self.tool_entries.push(GcodeProgramTool {
                        number: index + 1,
                        name: tool.name().to_owned(),
                        diameter_mm: tool.diameter_mm(),
                        kind: tool.kind().label().to_owned(),
                    });
                    index
                }
            };
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

    fn route_slot(
        &mut self,
        slot: &DrillSlot,
        depth: f64,
        retract: f64,
        feed: f64,
        plunge: f64,
        dwell: f64,
    ) {
        let x = self.gx(slot.start.x);
        let y = self.gy(slot.start.y);
        self.travel_xy(x, y);
        let _ = writeln!(self.text, "G01 Z-{depth:.3} F{plunge:.1}");
        let _ = writeln!(
            self.text,
            "G01 X{:.3} Y{:.3} F{feed:.1}",
            self.gx(slot.end.x),
            self.gy(slot.end.y)
        );
        if dwell > 0.0 {
            let _ = writeln!(self.text, "G04 P{dwell:.2}");
        }
        let _ = writeln!(self.text, "G00 Z{retract:.3}");
        self.position = slot.end;
        self.slot_count += 1;
        self.cut_length_mm += slot.start.distance(slot.end);
    }

    fn cut_path(&mut self, path: &Path, depth: f64, retract: f64, feed: f64, plunge: f64) {
        let Some(start) = path.points.first().copied() else {
            return;
        };
        let _ = writeln!(self.text, "G00 Z{retract:.3}");
        let x = self.gx(start.x);
        let y = self.gy(start.y);
        self.travel_xy(x, y);
        let _ = writeln!(self.text, "G01 Z-{depth:.3} F{plunge:.1}");

        for point in path.points.iter().skip(1) {
            let _ = writeln!(
                self.text,
                "G01 X{:.3} Y{:.3} F{feed:.1}",
                self.gx(point.x),
                self.gy(point.y)
            );
        }
        if path.closed
            && let Some(last) = path.points.last()
            && last.distance(start) > 1e-7
        {
            let _ = writeln!(
                self.text,
                "G01 X{:.3} Y{:.3} F{feed:.1}",
                self.gx(start.x),
                self.gy(start.y)
            );
        }
        let _ = writeln!(self.text, "G00 Z{retract:.3}");

        self.position = if path.closed {
            start
        } else {
            path.points.last().copied().unwrap_or(start)
        };
        self.path_count += 1;
        self.cut_length_mm += path.length();
    }

    fn finish(mut self) -> GcodeProgram {
        let _ = writeln!(self.text);
        let _ = writeln!(self.text, "G00 Z{:.3}", self.last_safe_z.max(0.0));
        let _ = writeln!(self.text, "M5");
        self.travel_xy(0.0, 0.0);
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
            tasks: self.task_entries,
            tools: self.tool_entries,
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

fn order_holes_nearest(mut holes: Vec<&DrillHit>, start: Point) -> Vec<&DrillHit> {
    let mut ordered = Vec::with_capacity(holes.len());
    let mut current = start;
    while !holes.is_empty() {
        let index = holes
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                a.position
                    .distance(current)
                    .total_cmp(&b.position.distance(current))
            })
            .map(|(index, _)| index)
            .unwrap_or(0);
        let hole = holes.swap_remove(index);
        ordered.push(hole);
        current = hole.position;
    }
    ordered
}

fn order_paths_nearest(paths: Vec<Path>, start: Point) -> Vec<Path> {
    let mut remaining = paths;
    let mut ordered = Vec::with_capacity(remaining.len());
    let mut current = start;
    while !remaining.is_empty() {
        let mut best: Option<(usize, usize, Point, f64)> = None;
        for (index, path) in remaining.iter().enumerate() {
            let Some((segment, entry, distance)) = nearest_path_entry(path, current) else {
                continue;
            };
            if best.is_none_or(|(_, _, _, best_distance)| distance < best_distance) {
                best = Some((index, segment, entry, distance));
            }
        }
        let Some((index, segment, entry, _)) = best else {
            break;
        };
        let mut path = remaining.swap_remove(index);
        if path.closed && path.points.len() >= 2 {
            let n = path.points.len();
            let segment_start = path.points[segment];
            let segment_end = path.points[(segment + 1) % n];
            if entry.distance(segment_start) <= 1e-9 {
                path.points.rotate_left(segment);
            } else if entry.distance(segment_end) <= 1e-9 {
                path.points.rotate_left((segment + 1) % n);
            } else {
                let mut points = Vec::with_capacity(n + 1);
                points.push(entry);
                for offset in 1..=n {
                    points.push(path.points[(segment + offset) % n]);
                }
                path.points = points;
            }
        } else if !path.closed
            && path.points.last().is_some_and(|last| {
                last.distance(current) + 1e-12 < path.points[0].distance(current)
            })
        {
            path.points.reverse();
        }
        let entry = path.points.first().copied().unwrap_or(current);
        let exit = if path.closed {
            entry
        } else {
            path.points.last().copied().unwrap_or(entry)
        };
        ordered.push(path);
        current = exit;
    }
    ordered
}

/// Nearest entry point on a path: nearest segment point for closed paths,
/// nearest endpoint for open paths.
fn nearest_path_entry(path: &Path, current: Point) -> Option<(usize, Point, f64)> {
    let first = path.points.first().copied()?;
    if path.closed && path.points.len() >= 2 {
        let n = path.points.len();
        let mut best: Option<(usize, Point, f64)> = None;
        for index in 0..n {
            let a = path.points[index];
            let b = path.points[(index + 1) % n];
            let (point, distance) = nearest_on_segment(a, b, current);
            if best.is_none_or(|(_, _, best_distance)| distance < best_distance) {
                best = Some((index, point, distance));
            }
        }
        best
    } else {
        let last = *path.points.last()?;
        let start_distance = first.distance(current);
        let end_distance = last.distance(current);
        if end_distance + 1e-12 < start_distance {
            Some((0, last, end_distance))
        } else {
            Some((0, first, start_distance))
        }
    }
}

fn nearest_on_segment(a: Point, b: Point, point: Point) -> (Point, f64) {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared <= f64::EPSILON {
        return (a, a.distance(point));
    }
    let t = (((point.x - a.x) * dx + (point.y - a.y) * dy) / length_squared).clamp(0.0, 1.0);
    let closest = Point::new(a.x + dx * t, a.y + dy * t);
    (closest, closest.distance(point))
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
    use crate::operation::{
        CutoutOperation, DrillOperation, EngraveOperation, HoleMillOperation, OperationKind,
    };
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
        let mut mill = HoleMillOperation::new(layer_id, tool);
        mill.source_tools.extend([1, 2]);
        let operation = project.add_task("Hole mill large", OperationKind::HoleMill(mill));
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
        let mut cutout = CutoutOperation::new(layer_id, tool);
        cutout.offset_mm = 0.5;
        let task = project.add_task("Cutout", OperationKind::Cutout(cutout));
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
    fn gcode_origin_offsets_all_coordinates() {
        let (mut project, library, id) = project_with_helical();
        project.set_gcode_origin(Point::new(9.0, 8.0));
        let program = generate_task_gcode(&project, &library, project.task(id).unwrap()).unwrap();
        assert!(program.text.contains("(G-code 0: X9.000 Y8.000 mm)"));
        assert!(program.text.contains("X1.000 Y2.000"));
        assert!(!program.text.contains("X10.000 Y10.000"));
    }

    #[test]
    fn machining_defaults_control_travel_and_clearance() {
        let (project, mut library, id) = project_with_drills();
        let mut defaults = *library.machining();
        defaults.rapid_feed_mm_min = 1_234.0;
        defaults.safe_z_mm = 4.5;
        library.set_machining(defaults);
        let program = generate_task_gcode(&project, &library, project.task(id).unwrap()).unwrap();
        assert!(program.text.contains("(Travel feed: 1234 mm/min)"));
        assert!(program.text.contains("(Safe Z: 4.500 mm)"));
        assert!(program.text.contains("G01 X10.000 Y10.000 F1234.0"));
    }

    #[test]
    fn drilling_visits_nearest_hole_first() {
        let holes = [
            DrillHit {
                position: Point::new(20.0, 0.0),
                tool: 1,
                diameter_mm: 0.8,
            },
            DrillHit {
                position: Point::new(0.0, 0.0),
                tool: 1,
                diameter_mm: 0.8,
            },
            DrillHit {
                position: Point::new(10.0, 0.0),
                tool: 1,
                diameter_mm: 0.8,
            },
        ];
        let ordered = order_holes_nearest(holes.iter().collect(), Point::new(0.0, 0.0));
        let xs: Vec<f64> = ordered.iter().map(|hole| hole.position.x).collect();
        assert_eq!(xs, vec![0.0, 10.0, 20.0]);
    }

    #[test]
    fn clearance_orders_paths_and_rotates_entry() {
        let square = |x: f64, y: f64| {
            Path::closed(vec![
                Point::new(x, y),
                Point::new(x + 1.0, y),
                Point::new(x + 1.0, y + 1.0),
                Point::new(x, y + 1.0),
            ])
        };
        let ordered = order_paths_nearest(
            vec![square(10.0, 0.0), square(0.0, 0.0)],
            Point::new(11.0, 1.0),
        );
        assert_eq!(ordered[0].points[0], Point::new(11.0, 1.0));
        assert_eq!(ordered[1].points[0], Point::new(1.0, 1.0));
    }

    #[test]
    fn clearance_can_enter_on_a_segment() {
        let square = Path::closed(vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ]);
        let ordered = order_paths_nearest(vec![square], Point::new(5.0, -4.0));
        let entry = ordered[0].points[0];
        assert!((entry.x - 5.0).abs() < 1e-9);
        assert!(entry.y.abs() < 1e-9);
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
    fn drill_requires_a_matching_diameter() {
        let mut project = Project::new("Drill mismatch");
        let layer_id = project.add_layer(
            "Drills",
            None,
            None,
            LayerGeometry::Excellon(ExcellonLayer {
                tools: vec![DrillTool {
                    number: 1,
                    diameter_mm: 1.0,
                }],
                holes: vec![DrillHit {
                    position: Point::new(5.0, 5.0),
                    tool: 1,
                    diameter_mm: 1.0,
                }],
                slots: Vec::new(),
                source_units: Some(crate::Unit::Millimeter),
                warnings: Vec::new(),
            }),
        );
        let library = ToolLibrary::new();
        let tool = library.closest(ToolKind::Drill, 0.8).unwrap();
        let id = project.add_task(
            "Drill 0.8",
            OperationKind::Drill(DrillOperation::new(layer_id, tool)),
        );
        let error = generate_task_gcode(&project, &library, project.task(id).unwrap()).unwrap_err();
        assert!(error.contains("does not match"));
    }

    #[test]
    fn hole_milling_requires_an_interpolating_tool() {
        let (mut project, library, id) = project_with_helical();
        let task = project.task_mut(id).unwrap();
        let OperationKind::HoleMill(mill) = task.kind_mut() else {
            panic!("test task is not hole milling");
        };
        mill.tool = library.closest(ToolKind::Drill, 0.8).unwrap();
        let error = generate_task_gcode(&project, &library, project.task(id).unwrap()).unwrap_err();
        assert!(error.contains("requires an end mill"));
    }

    #[test]
    fn contour_cut_offsets_outside_the_outline() {
        let (mut project, library, id) = project_with_outline();
        let OperationKind::Cutout(cutout) = project.task_mut(id).unwrap().kind_mut() else {
            panic!("expected edge cut");
        };
        cutout.stepdown_mm = 5.0;
        let program = generate_task_gcode(&project, &library, project.task(id).unwrap()).unwrap();
        assert_eq!(program.task_count, 1);
        assert_eq!(program.path_count, 1);
        assert!(program.text.contains("T1 M6"));
        assert!(program.text.contains("X-0.500 Y-0.500"));
        assert!(program.text.contains("G01 Z-1.800"));
    }

    #[test]
    fn contour_cut_steps_down_through_depth_passes() {
        let (mut project, library, id) = project_with_outline();
        let OperationKind::Cutout(cutout) = project.task_mut(id).unwrap().kind_mut() else {
            panic!("expected edge cut");
        };
        cutout.stepdown_mm = 0.5;

        let program = generate_task_gcode(&project, &library, project.task(id).unwrap()).unwrap();
        assert_eq!(program.path_count, 4);
        assert!(
            program
                .text
                .contains("(Depth passes: 4 × 0.500 mm step, total 1.800 mm)")
        );
        assert!(program.text.contains("(Depth pass 4/4 at Z-1.800)"));
        assert_eq!(program.text.matches("G01 Z-0.500").count(), 1);
        assert_eq!(program.text.matches("G01 Z-1.000").count(), 1);
        assert_eq!(program.text.matches("G01 Z-1.500").count(), 1);
        assert_eq!(program.text.matches("G01 Z-1.800").count(), 1);
    }

    #[test]
    fn contour_cut_leaves_bridges_at_the_start_depth() {
        let (mut project, library, id) = project_with_outline();
        let OperationKind::Cutout(cutout) = project.task_mut(id).unwrap().kind_mut() else {
            panic!("expected edge cut");
        };
        cutout.stepdown_mm = 5.0;
        cutout.bridge_count = 4;
        cutout.bridge_length_mm = 2.0;
        cutout.bridge_start_depth_mm = 0.5;

        let program = generate_task_gcode(&project, &library, project.task(id).unwrap()).unwrap();
        assert_eq!(program.path_count, 1);
        assert!(
            program
                .text
                .contains("(Bridges: 4 per contour, 2.000 mm long, raised to Z-0.500)")
        );
        // Four bridge crossings raise the tool and four plunges return to the
        // full depth, in addition to the initial plunge.
        assert_eq!(program.text.matches("G01 Z-0.500").count(), 4);
        assert_eq!(program.text.matches("G01 Z-1.800").count(), 5);
        assert!(!program.text.contains("G00 Z-0.500"));
    }

    #[test]
    fn bridged_cutout_keeps_all_cutting_moves_on_the_contour() {
        use crate::preview::GcodePreview;

        let mut project = Project::new("Bridge diagonal");
        let path = Path::closed(vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
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
        let mut cutout = CutoutOperation::new(layer_id, tool);
        cutout.offset_mm = 0.5;
        cutout.stepdown_mm = 5.0;
        cutout.bridge_count = 1;
        cutout.bridge_length_mm = 2.0;
        cutout.bridge_start_depth_mm = 0.5;
        let id = project.add_task("Cutout", OperationKind::Cutout(cutout));

        let program = generate_task_gcode(&project, &library, project.task(id).unwrap()).unwrap();
        let preview = GcodePreview::from_program(&program);
        let diagonal = preview.moves.iter().find(|movement| {
            if !movement.kind.is_cutting() {
                return false;
            }
            let dx = (movement.end.x - movement.start.x).abs();
            let dy = (movement.end.y - movement.start.y).abs();
            dx > 1e-6 && dy > 1e-6
        });

        // The offset outline is rectangular, so every cutting move must stay on
        // an axis-aligned contour edge. A move that changes both X and Y is a
        // jump across the contour.
        assert!(
            diagonal.is_none(),
            "diagonal cutting move {:?} -> {:?}",
            diagonal.map(|movement| movement.start),
            diagonal.map(|movement| movement.end)
        );
    }

    #[test]
    fn bridges_are_left_only_on_passes_below_the_start_depth() {
        let (mut project, library, id) = project_with_outline();
        let OperationKind::Cutout(cutout) = project.task_mut(id).unwrap().kind_mut() else {
            panic!("expected edge cut");
        };
        cutout.depth_mm = 1.0;
        cutout.stepdown_mm = 0.5;
        cutout.bridge_count = 4;
        cutout.bridge_length_mm = 2.0;
        cutout.bridge_start_depth_mm = 0.5;

        let program = generate_task_gcode(&project, &library, project.task(id).unwrap()).unwrap();
        assert_eq!(program.path_count, 2);
        // The 0.5 mm pass cuts the whole contour. The 1.0 mm pass raises over
        // each bridge four times and plunges back after each crossing.
        assert_eq!(program.text.matches("G01 Z-0.500").count(), 5);
        assert_eq!(program.text.matches("G01 Z-1.000").count(), 5);
    }

    #[test]
    fn engrave_follows_paths() {
        let (project, library, id) = project_with_engrave();
        let program = generate_task_gcode(&project, &library, project.task(id).unwrap()).unwrap();
        assert!(program.path_count >= 1);
        assert!(program.text.contains("G01 Z-0.200"));
        assert!(!program.text.contains("X10.000 Y8.000"));
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
        engrave.clearance_width_mm = 0.6;
        engrave.stepover_mm = 0.2;

        let program =
            generate_task_gcode(&project, &library, project.task(engrave_id).unwrap()).unwrap();
        assert!(program.path_count >= 3);
        assert!(
            program
                .text
                .contains("width 0.600 mm, 3 passes, step 0.200 mm")
        );
        assert!(program.text.contains("G01 Z-0.200"));
    }

    #[test]
    fn spot_drill_emits_centres_and_ignores_hole_diameters() {
        let (mut project, library, _drill_id) = project_with_drills();
        let layer = project
            .tasks()
            .first()
            .map(|task| task.source_layer())
            .unwrap();
        let tool = library.closest(ToolKind::Cone, 6.0).unwrap();
        let spot_id = project.add_task(
            "Spot drill",
            OperationKind::SpotDrill(crate::SpotDrillOperation::new(layer, tool)),
        );
        let program =
            generate_task_gcode(&project, &library, project.task(spot_id).unwrap()).unwrap();
        assert_eq!(program.hole_count, 2);
        assert_eq!(program.slot_count, 0);
        assert_eq!(program.helical_hole_count, 0);
        assert!(program.text.contains("X10.000 Y10.000"));
        assert!(program.text.contains("X20.000 Y10.000"));
        assert!(program.text.contains("slot"));
        assert!(!program.text.contains("G02"));
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

        assert_eq!(program.tasks.len(), 2);
        assert_eq!(program.tasks[0].name, "Drill");
        assert_eq!(program.tasks[0].kind, "Drill");
        assert_eq!(program.tasks[0].tool_number, Some(1));
        assert_eq!(program.tasks[1].kind, "Edge cut");
        assert_eq!(program.tasks[1].tool_number, Some(2));
        assert!(program.tasks[0].line_start < program.tasks[0].line_end);
        assert!(program.tasks[1].line_end <= program.text.lines().count());
        assert!(
            program
                .text
                .lines()
                .skip(program.tasks[0].line_start)
                .take(2)
                .any(|line| line.contains("Drill"))
        );
        assert_eq!(program.tools.len(), 2);
        assert_eq!(program.tools[0].number, 1);
        assert_eq!(program.tools[1].number, 2);
        assert!(program.tools.iter().all(|tool| !tool.name.is_empty()));
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
