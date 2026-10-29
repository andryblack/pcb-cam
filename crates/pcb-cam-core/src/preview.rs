//! Parsed G-code for the preview and machine simulation.
//!
//! The generator emits a deliberately small subset of RS-274: absolute
//! millimetre moves, `G00`/`G01` and the `G02` helical arcs used for hole
//! milling. This module turns the text back into timed moves so that the UI can
//! draw a 3D toolpath, highlight the current command and scrub through the
//! program. Unknown words are ignored, which keeps the preview usable for
//! hand-edited files that contain extra controller setup commands.

use std::f64::consts::TAU;

use crate::Point;
use crate::gcode::{GcodeProgram, GcodeProgramTask, GcodeProgramTool};

/// Simulated tool change time in seconds.
pub const TOOL_CHANGE_SECONDS: f64 = 2.0;
/// Simulated spindle ramp-up time in seconds.
pub const SPINDLE_START_SECONDS: f64 = 0.5;
/// Fallback rapid rate when a program has no travel-feed header.
pub const DEFAULT_RAPID_FEED_MM_MIN: f64 = 6_000.0;

const EPSILON: f64 = 1e-9;

/// A point in machine space, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point3 {
    /// X coordinate.
    pub x: f64,
    /// Y coordinate.
    pub y: f64,
    /// Z coordinate; negative values are below the stock surface.
    pub z: f64,
}

impl Point3 {
    /// Construct a point.
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// Euclidean distance to another point.
    pub fn distance(self, other: Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    /// Linear interpolation between two points.
    pub fn lerp(self, other: Self, t: f64) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
            self.z + (other.z - self.z) * t,
        )
    }

    fn approx_eq(self, other: Self) -> bool {
        self.distance(other) < EPSILON
    }
}

/// How a move relates to material removal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveKind {
    /// Rapid positioning at traverse rate.
    Rapid,
    /// Tool-down or safe-Z travel between features.
    Travel,
    /// Vertical entry into material.
    Plunge,
    /// Cutting move in material.
    Cut,
    /// Vertical retraction.
    Retract,
}

impl MoveKind {
    /// Whether the move removes material.
    pub const fn is_cutting(self) -> bool {
        matches!(self, Self::Plunge | Self::Cut)
    }

    /// Short label used by the legend and statistics.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rapid => "Rapid",
            Self::Travel => "Travel",
            Self::Plunge => "Plunge",
            Self::Cut => "Cut",
            Self::Retract => "Retract",
        }
    }
}

/// One timed motion block.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewMove {
    /// Zero-based G-code line that produced the move.
    pub line: usize,
    /// Start point.
    pub start: Point3,
    /// End point.
    pub end: Point3,
    /// Polyline used for drawing; includes the start and end points.
    pub points: Vec<Point3>,
    /// Movement category.
    pub kind: MoveKind,
    /// Programmed feed rate in millimetres per minute.
    pub feed_mm_min: f64,
    /// Time at which the move starts, in seconds.
    pub time_start_s: f64,
    /// Time at which the move ends, in seconds.
    pub time_end_s: f64,
    /// Length in millimetres.
    pub length_mm: f64,
    /// Program tool number, if a tool is selected.
    pub tool: Option<usize>,
    /// Index into [`GcodePreview::tasks`], if the move belongs to a task.
    pub task: Option<usize>,
    /// Whether the move was generated from a `G02`/`G03` arc.
    pub arc: bool,
}

impl PreviewMove {
    /// Duration of the move in seconds.
    pub fn duration_s(&self) -> f64 {
        (self.time_end_s - self.time_start_s).max(0.0)
    }

    /// Interpolate the tool position along the move at an absolute time.
    pub fn position_at(&self, time: f64) -> Point3 {
        if self.points.len() < 2 || self.length_mm <= EPSILON {
            return self.end;
        }
        let fraction = if self.duration_s() <= EPSILON {
            1.0
        } else {
            ((time - self.time_start_s) / self.duration_s()).clamp(0.0, 1.0)
        };
        let target = self.length_mm * fraction;
        let mut travelled = 0.0;
        for pair in self.points.windows(2) {
            let segment = pair[0].distance(pair[1]);
            if segment <= EPSILON {
                continue;
            }
            if travelled + segment >= target {
                let local = (target - travelled) / segment;
                return pair[0].lerp(pair[1], local);
            }
            travelled += segment;
        }
        self.end
    }
}

/// One line of the source program with its execution window.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewLine {
    /// Original line text.
    pub text: String,
    /// Time at which execution of the line starts.
    pub time_s: f64,
    /// Time spent on the line (motion, dwell, tool change or spindle start).
    pub duration_s: f64,
    /// Move produced by the line, if any.
    pub move_index: Option<usize>,
    /// Tool selected after the line has executed.
    pub tool: Option<usize>,
    /// Task containing the line, if any.
    pub task: Option<usize>,
    /// Active feed rate after the line has executed.
    pub feed_mm_min: Option<f64>,
}

/// A run of consecutive moves with the same colour class.
///
/// Splitting the program into runs lets the 3D view draw long toolpaths with a
/// handful of line shapes instead of one shape per move.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewPath {
    /// Whether the run removes material.
    pub cutting: bool,
    /// Polyline for the run.
    pub points: Vec<Point3>,
    /// First move index (inclusive).
    pub move_start: usize,
    /// Last move index (exclusive).
    pub move_end: usize,
}

/// Snapshot of the simulated machine at a point in time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewState {
    /// Current tool-tip position.
    pub position: Point3,
    /// Selected program tool.
    pub tool: Option<usize>,
    /// Task being executed.
    pub task: Option<usize>,
    /// Current source line.
    pub line: usize,
    /// Move being executed, if the machine is in motion.
    pub move_index: Option<usize>,
    /// Active feed rate.
    pub feed_mm_min: Option<f64>,
    /// Whether the active move is cutting.
    pub cutting: bool,
}

/// Parsed program ready for rendering and playback.
#[derive(Debug, Clone, PartialEq)]
pub struct GcodePreview {
    /// Every source line.
    pub lines: Vec<PreviewLine>,
    /// Timed moves in program order.
    pub moves: Vec<PreviewMove>,
    /// Drawable runs of moves.
    pub paths: Vec<PreviewPath>,
    /// Task blocks reported by the generator.
    pub tasks: Vec<GcodeProgramTask>,
    /// Tools referenced by the program.
    pub tools: Vec<GcodeProgramTool>,
    /// Initial tool position before the first move.
    pub initial_position: Point3,
    /// G-code origin in board coordinates.
    pub origin: Point,
    /// Safe-Z value from the program header.
    pub safe_z_mm: f64,
    /// Travel feed from the program header.
    pub rapid_feed_mm_min: f64,
    /// Bounds of the toolpath including the stock surface.
    pub min: Point3,
    /// Bounds of the toolpath including the stock surface.
    pub max: Point3,
    /// Total simulated time in seconds.
    pub total_time_s: f64,
    /// Cutting distance in millimetres.
    pub cut_length_mm: f64,
    /// Travel distance in millimetres.
    pub rapid_length_mm: f64,
    /// Number of arc moves.
    pub arc_count: usize,
    /// Number of tool changes.
    pub tool_change_count: usize,
}

impl GcodePreview {
    /// Parse a generated program and its structured metadata.
    pub fn from_program(program: &GcodeProgram) -> Self {
        Self::parse(&program.text, &program.tasks, &program.tools)
    }

    /// Parse G-code text.
    pub fn parse(text: &str, tasks: &[GcodeProgramTask], tools: &[GcodeProgramTool]) -> Self {
        let (origin, rapid_feed_mm_min, safe_z_mm) = header_metadata(text);
        let initial_position = Point3::new(0.0, 0.0, safe_z_mm);

        let mut parser = Parser {
            lines: Vec::new(),
            moves: Vec::new(),
            tasks: tasks.to_vec(),
            tools: tools.to_vec(),
            safe_z_mm,
            rapid_feed_mm_min,
            position: initial_position,
            absolute: true,
            unit_scale: 1.0,
            modal_motion: None,
            feed_mm_min: None,
            current_tool: None,
            time_s: 0.0,
            min: initial_position,
            max: initial_position,
            cut_length_mm: 0.0,
            rapid_length_mm: 0.0,
            arc_count: 0,
            tool_change_count: 0,
        };
        parser.include_surface();
        parser.run(text);

        let mut preview = Self {
            lines: parser.lines,
            moves: parser.moves,
            paths: Vec::new(),
            tasks: parser.tasks,
            tools: parser.tools,
            initial_position,
            origin,
            safe_z_mm,
            rapid_feed_mm_min,
            min: parser.min,
            max: parser.max,
            total_time_s: parser.time_s,
            cut_length_mm: parser.cut_length_mm,
            rapid_length_mm: parser.rapid_length_mm,
            arc_count: parser.arc_count,
            tool_change_count: parser.tool_change_count,
        };
        preview.paths = build_paths(&preview.moves);
        preview
    }

    /// Simulated duration in seconds.
    pub fn duration_s(&self) -> f64 {
        self.total_time_s
    }

    /// Index of the last move started at or before `time`.
    pub fn move_index_at(&self, time: f64) -> Option<usize> {
        let started = self
            .moves
            .partition_point(|movement| movement.time_start_s <= time + EPSILON);
        (started > 0).then(|| started - 1)
    }

    /// Source line highlighted at `time`.
    pub fn line_at(&self, time: f64) -> usize {
        if self.lines.is_empty() {
            return 0;
        }
        let time = time.clamp(0.0, self.total_time_s);
        if let Some(index) = self.move_index_at(time) {
            let movement = &self.moves[index];
            if time >= movement.time_start_s - EPSILON && time < movement.time_end_s + EPSILON {
                return movement.line.min(self.lines.len() - 1);
            }
        }
        let started = self
            .lines
            .partition_point(|line| line.time_s <= time + EPSILON);
        started.saturating_sub(1).min(self.lines.len() - 1)
    }

    /// Simulated machine state at `time`.
    pub fn state_at(&self, time: f64) -> PreviewState {
        if self.lines.is_empty() {
            return PreviewState {
                position: self.initial_position,
                tool: None,
                task: None,
                line: 0,
                move_index: None,
                feed_mm_min: None,
                cutting: false,
            };
        }
        let time = time.clamp(0.0, self.total_time_s);
        let line_index = self.line_at(time);
        let line = &self.lines[line_index];
        let move_index = self.move_index_at(time);
        let inside_move = move_index.is_some_and(|index| {
            let movement = &self.moves[index];
            time >= movement.time_start_s - EPSILON && time <= movement.time_end_s + EPSILON
        });

        if inside_move {
            let index = move_index.expect("inside_move implies an index");
            let movement = &self.moves[index];
            PreviewState {
                position: movement.position_at(time),
                tool: movement.tool,
                task: movement.task,
                line: movement.line.min(self.lines.len().saturating_sub(1)),
                move_index: Some(index),
                feed_mm_min: Some(movement.feed_mm_min),
                cutting: movement.kind.is_cutting(),
            }
        } else {
            let position = move_index
                .map(|index| self.moves[index].end)
                .unwrap_or(self.initial_position);
            PreviewState {
                position,
                tool: line.tool,
                task: line.task,
                line: line_index,
                move_index,
                feed_mm_min: line.feed_mm_min,
                cutting: false,
            }
        }
    }

    /// Start time of a source line.
    pub fn time_for_line(&self, line: usize) -> f64 {
        self.lines.get(line).map_or(0.0, |line| line.time_s)
    }

    /// End time of a source line.
    pub fn end_time_for_line(&self, line: usize) -> f64 {
        self.lines
            .get(line)
            .map_or(0.0, |line| line.time_s + line.duration_s)
    }

    /// Execution window of a task block.
    pub fn task_time_range(&self, task: usize) -> Option<(f64, f64)> {
        let task = self.tasks.get(task)?;
        let start = self.lines.get(task.line_start)?.time_s;
        let last = task.line_end.saturating_sub(1);
        let end = self
            .lines
            .get(last)
            .map(|line| line.time_s + line.duration_s)
            .unwrap_or(start);
        Some((start, end.max(start)))
    }

    /// Progress in the range `0.0..=1.0`.
    pub fn progress(&self, time: f64) -> f32 {
        if self.total_time_s <= EPSILON {
            0.0
        } else {
            (time / self.total_time_s).clamp(0.0, 1.0) as f32
        }
    }
}

struct Parser {
    lines: Vec<PreviewLine>,
    moves: Vec<PreviewMove>,
    tasks: Vec<GcodeProgramTask>,
    tools: Vec<GcodeProgramTool>,
    safe_z_mm: f64,
    rapid_feed_mm_min: f64,
    position: Point3,
    absolute: bool,
    unit_scale: f64,
    modal_motion: Option<u8>,
    feed_mm_min: Option<f64>,
    current_tool: Option<usize>,
    time_s: f64,
    min: Point3,
    max: Point3,
    cut_length_mm: f64,
    rapid_length_mm: f64,
    arc_count: usize,
    tool_change_count: usize,
}

impl Parser {
    #[allow(clippy::too_many_lines)]
    fn run(&mut self, text: &str) {
        let mut task_cursor = 0usize;

        for (line_index, raw) in text.lines().enumerate() {
            while task_cursor < self.tasks.len() && line_index >= self.tasks[task_cursor].line_end {
                task_cursor += 1;
            }
            let line_task = (task_cursor < self.tasks.len()
                && line_index >= self.tasks[task_cursor].line_start)
                .then_some(task_cursor);

            let line_start = self.time_s;
            let code = strip_comments(raw);
            let words = Words::parse(&code);
            let mut delay = 0.0;
            let mut line_move = None;
            let mut end = self.position;

            for value in &words.g {
                match value.round() as i32 {
                    0..=3 => self.modal_motion = Some(value.round() as u8),
                    4 => {
                        if let Some(dwell) = words.p {
                            delay += dwell.max(0.0);
                        }
                    }
                    20 => self.unit_scale = 25.4,
                    21 => self.unit_scale = 1.0,
                    90 => self.absolute = true,
                    91 => self.absolute = false,
                    _ => {}
                }
            }

            if let Some(tool) = words.t {
                self.current_tool = Some(tool.round().max(0.0) as usize);
            }

            for value in &words.m {
                match value.round() as i32 {
                    3 | 4 => delay += SPINDLE_START_SECONDS,
                    6 => {
                        delay += TOOL_CHANGE_SECONDS;
                        self.tool_change_count += 1;
                    }
                    _ => {}
                }
            }

            if let Some(feed) = words.f {
                self.feed_mm_min = Some((feed * self.unit_scale).max(0.0));
            }

            let target = self.target_position(&words);
            if let Some(motion) = self.motion(&words)
                && let Some(movement) =
                    self.build_move(line_index, line_task, motion, self.position, target, &words)
            {
                let index = self.moves.len();
                self.time_s = movement.time_end_s;
                for point in &movement.points {
                    self.include(*point);
                }
                if movement.kind.is_cutting() {
                    self.cut_length_mm += movement.length_mm;
                } else {
                    self.rapid_length_mm += movement.length_mm;
                }
                if movement.arc {
                    self.arc_count += 1;
                }
                end = movement.end;
                self.moves.push(movement);
                line_move = Some(index);
            }

            // A tool change or dwell after a motion on the same line extends
            // the line without moving the tool.
            self.time_s += delay;
            let tool = self.current_tool;
            self.lines.push(PreviewLine {
                text: raw.to_owned(),
                time_s: line_start,
                duration_s: self.time_s - line_start,
                move_index: line_move,
                tool,
                task: line_task,
                feed_mm_min: self.feed_mm_min,
            });
            self.position = end;
            self.include(end);
        }
    }

    fn motion(&self, words: &Words) -> Option<u8> {
        if !words.g.is_empty()
            && let Some(value) = words
                .g
                .iter()
                .rev()
                .find(|value| (0.0..=3.0).contains(*value))
        {
            return Some(value.round() as u8);
        }
        let has_axis = words.x.is_some()
            || words.y.is_some()
            || words.z.is_some()
            || words.i.is_some()
            || words.j.is_some();
        has_axis.then_some(self.modal_motion).flatten()
    }

    fn target_position(&self, words: &Words) -> Point3 {
        let axis = |value: Option<f64>, current: f64| {
            value.map(|value| {
                if self.absolute {
                    value * self.unit_scale
                } else {
                    current + value * self.unit_scale
                }
            })
        };
        Point3::new(
            axis(words.x, self.position.x).unwrap_or(self.position.x),
            axis(words.y, self.position.y).unwrap_or(self.position.y),
            axis(words.z, self.position.z).unwrap_or(self.position.z),
        )
    }

    fn build_move(
        &self,
        line: usize,
        task: Option<usize>,
        motion: u8,
        start: Point3,
        end: Point3,
        words: &Words,
    ) -> Option<PreviewMove> {
        let xy_distance = (end.x - start.x).hypot(end.y - start.y);
        let z_delta = end.z - start.z;
        if motion < 2 && xy_distance <= EPSILON && z_delta.abs() <= EPSILON {
            return None;
        }

        let kind = match motion {
            0 => {
                if xy_distance <= EPSILON && z_delta > 0.0 {
                    MoveKind::Retract
                } else {
                    MoveKind::Rapid
                }
            }
            1 => {
                if xy_distance <= EPSILON {
                    if z_delta < 0.0 {
                        MoveKind::Plunge
                    } else {
                        MoveKind::Retract
                    }
                } else if end.z < -EPSILON {
                    MoveKind::Cut
                } else {
                    MoveKind::Travel
                }
            }
            _ => {
                if end.z < -EPSILON {
                    MoveKind::Cut
                } else {
                    MoveKind::Travel
                }
            }
        };

        let rate = if motion == 0 {
            self.rapid_feed_mm_min.max(1.0)
        } else {
            self.feed_mm_min
                .filter(|feed| *feed > 0.0)
                .unwrap_or(self.rapid_feed_mm_min.max(1.0))
        };

        let (points, length_mm, arc) = if motion >= 2 {
            self.arc_points(start, end, words, motion == 2).map_or_else(
                || (vec![start, end], start.distance(end), false),
                |(points, length)| (points, length, true),
            )
        } else {
            (vec![start, end], start.distance(end), false)
        };

        if length_mm <= EPSILON {
            return None;
        }
        let duration = length_mm / rate * 60.0;
        if !duration.is_finite() || duration < 0.0 {
            return None;
        }
        let time_start_s = self.time_s;
        Some(PreviewMove {
            line,
            start,
            end,
            points,
            kind,
            feed_mm_min: rate,
            time_start_s,
            time_end_s: time_start_s + duration,
            length_mm,
            tool: self.current_tool,
            task,
            arc,
        })
    }

    fn arc_points(
        &self,
        start: Point3,
        end: Point3,
        words: &Words,
        clockwise: bool,
    ) -> Option<(Vec<Point3>, f64)> {
        let i = words.i? * self.unit_scale;
        let j = words.j? * self.unit_scale;
        let radius = i.hypot(j);
        if radius <= EPSILON {
            return None;
        }
        let center = Point3::new(start.x + i, start.y + j, start.z);
        let start_angle = (start.y - center.y).atan2(start.x - center.x);
        let end_angle = (end.y - center.y).atan2(end.x - center.x);
        let full_circle = (end.x - start.x).hypot(end.y - start.y) <= EPSILON;
        let mut sweep = end_angle - start_angle;
        if clockwise {
            while sweep > -EPSILON {
                sweep -= TAU;
            }
        } else {
            while sweep < EPSILON {
                sweep += TAU;
            }
        }
        if full_circle {
            sweep = if clockwise { -TAU } else { TAU };
        }
        let steps = ((sweep.abs() / (TAU / 96.0)).ceil() as usize).clamp(8, 256);
        let mut points = Vec::with_capacity(steps + 1);
        points.push(start);
        for step in 1..=steps {
            let angle = start_angle + sweep * (step as f64 / steps as f64);
            points.push(Point3::new(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
                start.z + (end.z - start.z) * (step as f64 / steps as f64),
            ));
        }
        if let Some(last) = points.last_mut() {
            *last = end;
        }
        let length = sweep.abs() * radius;
        Some((points, length))
    }

    fn include(&mut self, point: Point3) {
        if !point.x.is_finite() || !point.y.is_finite() || !point.z.is_finite() {
            return;
        }
        self.min.x = self.min.x.min(point.x);
        self.min.y = self.min.y.min(point.y);
        self.min.z = self.min.z.min(point.z);
        self.max.x = self.max.x.max(point.x);
        self.max.y = self.max.y.max(point.y);
        self.max.z = self.max.z.max(point.z);
        self.max.z = self.max.z.max(0.0);
    }

    fn include_surface(&mut self) {
        self.min.z = self.min.z.min(0.0);
        self.max.z = self.max.z.max(self.safe_z_mm).max(0.0);
    }
}

fn build_paths(moves: &[PreviewMove]) -> Vec<PreviewPath> {
    let mut paths: Vec<PreviewPath> = Vec::new();
    for (index, movement) in moves.iter().enumerate() {
        let cutting = movement.kind.is_cutting();
        let merge = paths.last().is_some_and(|path| {
            path.cutting == cutting
                && path.move_end == index
                && path
                    .points
                    .last()
                    .is_some_and(|last| last.approx_eq(movement.start))
        });
        if merge {
            let path = paths.last_mut().expect("merge implies a path");
            path.points.extend(movement.points.iter().skip(1).copied());
            path.move_end = index + 1;
        } else {
            paths.push(PreviewPath {
                cutting,
                points: movement.points.clone(),
                move_start: index,
                move_end: index + 1,
            });
        }
    }
    paths
}

fn strip_comments(line: &str) -> String {
    let mut code = String::with_capacity(line.len());
    let mut depth = 0usize;
    for character in line.chars() {
        match character {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            ';' if depth == 0 => break,
            _ if depth > 0 => {}
            _ => code.push(character),
        }
    }
    code
}

#[derive(Default)]
struct Words {
    g: Vec<f64>,
    m: Vec<f64>,
    t: Option<f64>,
    f: Option<f64>,
    p: Option<f64>,
    x: Option<f64>,
    y: Option<f64>,
    z: Option<f64>,
    i: Option<f64>,
    j: Option<f64>,
}

impl Words {
    fn parse(code: &str) -> Self {
        let mut words = Self::default();
        for token in code.split_whitespace() {
            let mut characters = token.chars();
            let Some(letter) = characters.next() else {
                continue;
            };
            let letter = letter.to_ascii_uppercase();
            let Ok(value) = characters.as_str().parse::<f64>() else {
                continue;
            };
            match letter {
                'G' => words.g.push(value),
                'M' => words.m.push(value),
                'T' => words.t = Some(value),
                'F' => words.f = Some(value),
                'P' => words.p = Some(value),
                'X' => words.x = Some(value),
                'Y' => words.y = Some(value),
                'Z' => words.z = Some(value),
                'I' => words.i = Some(value),
                'J' => words.j = Some(value),
                _ => {}
            }
        }
        words
    }
}

fn header_metadata(text: &str) -> (Point, f64, f64) {
    let mut origin = Point::new(0.0, 0.0);
    let mut rapid = DEFAULT_RAPID_FEED_MM_MIN;
    let mut safe_z = 3.0;
    for line in text.lines().take(16) {
        if let Some(rest) = line.strip_prefix("(G-code 0:") {
            if let Some((x, y)) = parse_xy(rest) {
                origin = Point::new(x, y);
            }
        } else if let Some(rest) = line.strip_prefix("(Travel feed:")
            && let Some(value) = rest
                .split_whitespace()
                .find_map(|token| token.trim_end_matches(')').parse::<f64>().ok())
            && value > 0.0
        {
            rapid = value;
        } else if let Some(rest) = line.strip_prefix("(Safe Z:")
            && let Some(value) = rest
                .split_whitespace()
                .find_map(|token| token.trim_end_matches(')').parse::<f64>().ok())
        {
            safe_z = value;
        }
    }
    (origin, rapid, safe_z)
}

fn parse_xy(rest: &str) -> Option<(f64, f64)> {
    let mut x = None;
    let mut y = None;
    for token in rest.split_whitespace() {
        let token = token.trim_end_matches(')');
        if let Some(value) = token.strip_prefix('X') {
            x = value.parse().ok();
        } else if let Some(value) = token.strip_prefix('Y') {
            y = value.parse().ok();
        }
    }
    Some((x?, y?))
}

/// Format seconds as `M:SS.s`.
pub fn format_seconds(seconds: f64) -> String {
    let tenths = (seconds.max(0.0) * 10.0).round();
    let minutes = (tenths / 600.0).floor();
    let remainder = (tenths - minutes * 600.0) / 10.0;
    format!("{minutes:.0}:{remainder:04.1}")
}

/// Number of full turns represented by a clockwise sweep, used by tests.
#[cfg(test)]
fn clockwise_sweep(start: f64, end: f64) -> f64 {
    let mut sweep = end - start;
    while sweep > -EPSILON {
        sweep -= TAU;
    }
    sweep
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use super::*;
    use crate::gcode::{GcodeProgramTask, GcodeProgramTool};

    fn program(text: &str) -> GcodeProgram {
        GcodeProgram {
            text: text.to_owned(),
            tool_count: 1,
            task_count: 1,
            hole_count: 1,
            slot_count: 0,
            path_count: 0,
            helical_hole_count: 0,
            cut_length_mm: 0.0,
            tasks: vec![GcodeProgramTask {
                name: "Drill".to_owned(),
                kind: "Drill".to_owned(),
                line_start: 9,
                line_end: 21,
                tool_number: Some(1),
                tool_name: Some("Drill 0.80".to_owned()),
            }],
            tools: vec![GcodeProgramTool {
                number: 1,
                name: "Drill 0.80".to_owned(),
                diameter_mm: 0.8,
                kind: "Drill".to_owned(),
            }],
        }
    }

    fn sample() -> GcodeProgram {
        program(
            "(PCB-CAM test)\n\
             (G-code 0: X0.000 Y0.000 mm)\n\
             (Travel feed: 3000 mm/min)\n\
             (Safe Z: 3.000 mm)\n\
             G21\n\
             G90\n\
             G17\n\
             G94\n\
             \n\
             (Drill - Holes)\n\
             T1 M6 (T1: Drill 0.80)\n\
             S10000 M3\n\
             G00 Z3.000\n\
             G01 X10.000 Y10.000 F3000.0\n\
             G01 Z-1.500 F120.0\n\
             G04 P0.10\n\
             G00 Z3.000\n\
             G00 Z3.000\n\
             M5\n\
             G01 X0.000 Y0.000 F3000.0\n\
             M30\n",
        )
    }

    #[test]
    fn parses_moves_and_times() {
        let preview = GcodePreview::from_program(&sample());
        assert_eq!(preview.tools.len(), 1);
        assert_eq!(preview.tasks.len(), 1);
        assert!(preview.moves.len() >= 3);
        assert!(preview.total_time_s > 0.0);
        assert!(preview.line_at(0.0) >= 9);
        assert!(preview.cut_length_mm > 0.0);
        assert!(preview.rapid_length_mm > 0.0);
        let state = preview.state_at(preview.total_time_s);
        assert!((state.position.x - 0.0).abs() < 1e-6);
        assert!((state.position.y - 0.0).abs() < 1e-6);
    }

    #[test]
    fn travel_and_cut_moves_have_distinct_kinds() {
        let preview = GcodePreview::from_program(&sample());
        let travel = preview
            .moves
            .iter()
            .find(|movement| movement.kind == MoveKind::Travel)
            .expect("travel move");
        assert!((travel.feed_mm_min - 3000.0).abs() < 0.01);
        let plunge = preview
            .moves
            .iter()
            .find(|movement| movement.kind == MoveKind::Plunge)
            .expect("plunge move");
        assert!(plunge.kind.is_cutting());
        let retract = preview
            .moves
            .iter()
            .find(|movement| movement.kind == MoveKind::Retract)
            .expect("retract move");
        assert!(!retract.kind.is_cutting());
    }

    #[test]
    fn interpolates_tool_position_along_a_cut() {
        let preview = GcodePreview::from_program(&sample());
        let cut = preview
            .moves
            .iter()
            .find(|movement| movement.kind == MoveKind::Plunge)
            .expect("plunge");
        let middle = cut.position_at(cut.time_start_s + cut.duration_s() * 0.5);
        assert!((middle.z - 0.75).abs() < 0.05);
    }

    #[test]
    fn parses_helical_arcs() {
        let text = "(G-code 0: X0.000 Y0.000 mm)\n\
                    (Travel feed: 3000 mm/min)\n\
                    (Safe Z: 3.000 mm)\n\
                    G21\n\
                    G90\n\
                    T1 M6\n\
                    G00 Z1.000\n\
                    G01 X6.000 Y5.000 F3000.0\n\
                    G01 Z-0.500 F120.0\n\
                    G02 X4.000 Y5.000 I-1.000 J0.000 F300.0\n\
                    G02 X6.000 Y5.000 I1.000 J0.000 F300.0\n\
                    G00 Z3.000\n\
                    M30\n";
        let preview = GcodePreview::parse(
            text,
            &[],
            &[GcodeProgramTool {
                number: 1,
                name: "End mill".to_owned(),
                diameter_mm: 1.0,
                kind: "End mill".to_owned(),
            }],
        );
        let arcs: Vec<&PreviewMove> = preview
            .moves
            .iter()
            .filter(|movement| movement.arc)
            .collect();
        assert_eq!(arcs.len(), 2);
        assert!(arcs[0].kind.is_cutting());
        assert!((arcs[0].length_mm - PI).abs() < 0.01);
        assert!(arcs[0].points.len() > 8);
        assert_eq!(preview.arc_count, 2);
    }

    #[test]
    fn task_ranges_map_to_lines_and_times() {
        let preview = GcodePreview::from_program(&sample());
        let (start, end) = preview.task_time_range(0).expect("task range");
        assert!(start <= end);
        assert!(preview.time_for_line(9) >= start - 1e-9);
        assert_eq!(preview.state_at(end).task, Some(0));
    }

    #[test]
    fn paths_group_by_colour_class() {
        let preview = GcodePreview::from_program(&sample());
        assert!(!preview.paths.is_empty());
        assert!(preview.paths.iter().any(|path| path.cutting));
        assert!(preview.paths.iter().any(|path| !path.cutting));
    }

    #[test]
    fn relative_coordinates_and_inches_are_supported() {
        let text = "G20\nG91\nG01 X1.0 Y0.0 F60.0\nG01 X0.0 Y1.0 F60.0\nM30\n";
        let preview = GcodePreview::parse(text, &[], &[]);
        let last = preview.state_at(preview.total_time_s);
        assert!((last.position.x - 25.4).abs() < 1e-6);
        assert!((last.position.y - 25.4).abs() < 1e-6);
    }

    #[test]
    fn formats_times() {
        assert_eq!(format_seconds(0.0), "0:00.0");
        assert_eq!(format_seconds(65.4), "1:05.4");
        assert_eq!(format_seconds(59.96), "1:00.0");
    }

    #[test]
    fn empty_program_has_a_safe_state() {
        let preview = GcodePreview::parse("", &[], &[]);
        assert_eq!(preview.lines.len(), 0);
        assert_eq!(preview.total_time_s, 0.0);
        let state = preview.state_at(0.0);
        assert_eq!(state.position, preview.initial_position);
        assert_eq!(state.line, 0);
    }

    #[test]
    fn clockwise_helper_produces_negative_sweeps() {
        assert!(clockwise_sweep(0.0, PI) < 0.0);
        assert!((clockwise_sweep(0.0, PI) + PI).abs() < 1e-9);
    }
}
