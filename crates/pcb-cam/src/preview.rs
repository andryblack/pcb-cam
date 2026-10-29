//! G-code preview state: playback clock, camera and program metadata.
//!
//! The preview is a separate screen, not a dialog. It owns a generated
//! [`GcodeProgram`], the parsed [`GcodePreview`](pcb_cam_core::preview::GcodePreview)
//! and the presentation state needed to simulate the program.

use std::f32::consts::FRAC_PI_2;
use std::path::PathBuf;

use eframe::egui::{Rect, Vec2, vec2};
use pcb_cam_core::preview::{GcodePreview, Point3, PreviewState, format_seconds};
use pcb_cam_core::{GcodeProgram, OperationId};

/// Which task selection produced the preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreviewMode {
    /// One selected task.
    Task(OperationId),
    /// All enabled tasks.
    All,
}

impl PreviewMode {
    pub(crate) const fn is_all(self) -> bool {
        matches!(self, Self::All)
    }
}

/// Playback speeds offered by the transport control.
pub(crate) const SPEEDS: [f32; 8] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0];

/// Camera presets for the 3D viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CameraPreset {
    /// Three-quarter isometric view.
    Iso,
    /// Top view, X right and Y up.
    Top,
    /// Front view, X right and Z up.
    Front,
}

impl CameraPreset {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Iso => "3D",
            Self::Top => "Top",
            Self::Front => "Front",
        }
    }
}

/// A simple 3-vector used by the orthographic preview camera.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Vec3 {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) z: f32,
}

impl Vec3 {
    pub(crate) const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub(crate) fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    fn normalized(self) -> Self {
        let length = self.length();
        if length <= f32::EPSILON {
            Self::default()
        } else {
            Self::new(self.x / length, self.y / length, self.z / length)
        }
    }
}

/// Orthographic orbit camera for the 3D toolpath.
#[derive(Debug, Clone)]
pub(crate) struct PreviewCamera {
    /// Rotation around the vertical axis, in radians.
    pub(crate) yaw: f32,
    /// Elevation above the stock plane, in radians.
    pub(crate) pitch: f32,
    /// Pixels per millimetre.
    pub(crate) zoom: f32,
    /// Screen-space pan from the centre of the viewport.
    pub(crate) pan: Vec2,
    /// World-space point at the centre of the viewport. Z is already scaled.
    pub(crate) target: Vec3,
}

impl Default for PreviewCamera {
    fn default() -> Self {
        Self {
            yaw: -50_f32.to_radians(),
            pitch: 34_f32.to_radians(),
            zoom: 4.0,
            pan: Vec2::ZERO,
            target: Vec3::default(),
        }
    }
}

impl PreviewCamera {
    /// Basis vectors of the camera: screen right, screen up and view depth.
    pub(crate) fn basis(&self) -> (Vec3, Vec3, Vec3) {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        let right = Vec3::new(-sin_yaw, cos_yaw, 0.0);
        let up = Vec3::new(-sin_pitch * cos_yaw, -sin_pitch * sin_yaw, cos_pitch);
        let forward = Vec3::new(-cos_pitch * cos_yaw, -cos_pitch * sin_yaw, -sin_pitch);
        (right.normalized(), up.normalized(), forward.normalized())
    }

    /// Apply an orbit drag.
    pub(crate) fn orbit(&mut self, delta: Vec2) {
        self.yaw -= delta.x * 0.008;
        self.pitch = (self.pitch - delta.y * 0.008)
            .clamp(1_f32.to_radians(), FRAC_PI_2 - 1_f32.to_radians());
        let tau = std::f32::consts::TAU;
        self.yaw = self.yaw.rem_euclid(tau);
    }

    /// Apply a pan drag.
    pub(crate) fn pan_by(&mut self, delta: Vec2) {
        self.pan += delta;
    }

    /// Zoom while keeping the projected world point under `anchor` fixed.
    pub(crate) fn zoom_at(&mut self, factor: f32, anchor: eframe::egui::Pos2, rect: Rect) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let previous = self.zoom;
        let next = (previous * factor).clamp(0.05, 4_000.0);
        let offset = anchor - (rect.center() + self.pan);
        self.pan += offset * (1.0 - next / previous);
        self.zoom = next;
    }

    /// Switch to a preset orientation and request a refit.
    pub(crate) fn set_preset(&mut self, preset: CameraPreset) {
        match preset {
            CameraPreset::Iso => {
                self.yaw = -50_f32.to_radians();
                self.pitch = 34_f32.to_radians();
            }
            CameraPreset::Top => {
                self.yaw = -FRAC_PI_2;
                self.pitch = FRAC_PI_2 - 0.001;
            }
            CameraPreset::Front => {
                self.yaw = -FRAC_PI_2;
                self.pitch = 1_f32.to_radians();
            }
        }
        self.pan = Vec2::ZERO;
    }

    /// Fit real-world bounds into the viewport, accounting for Z scaling.
    pub(crate) fn fit(&mut self, min: Point3, max: Point3, z_scale: f32, rect: Rect) {
        if rect.width() < 8.0 || rect.height() < 8.0 {
            return;
        }
        let center = Vec3::new(
            ((min.x + max.x) * 0.5) as f32,
            ((min.y + max.y) * 0.5) as f32,
            ((min.z + max.z) * 0.5) as f32 * z_scale,
        );
        let half = Vec3::new(
            ((max.x - min.x) * 0.5).max(0.01) as f32,
            ((max.y - min.y) * 0.5).max(0.01) as f32,
            ((max.z - min.z) * 0.5).max(0.01) as f32 * z_scale,
        );
        let (right, up, _) = self.basis();
        let mut extent_x = 0.0_f32;
        let mut extent_y = 0.0_f32;
        for sx in [-1.0, 1.0] {
            for sy in [-1.0, 1.0] {
                for sz in [-1.0, 1.0] {
                    let corner = Vec3::new(half.x * sx, half.y * sy, half.z * sz);
                    extent_x = extent_x.max(right.dot(corner).abs());
                    extent_y = extent_y.max(up.dot(corner).abs());
                }
            }
        }
        let margin = 0.86;
        let available = vec2(rect.width() * margin, rect.height() * margin);
        let zoom_x = available.x / (extent_x * 2.0).max(0.01);
        let zoom_y = available.y / (extent_y * 2.0).max(0.01);
        self.zoom = zoom_x.min(zoom_y).clamp(0.05, 4_000.0);
        self.target = center;
        self.pan = Vec2::ZERO;
    }
}

/// One open preview screen.
#[derive(Debug)]
pub(crate) struct PreviewSession {
    /// Task scope.
    pub(crate) mode: PreviewMode,
    /// Generated program on success.
    pub(crate) program: Option<GcodeProgram>,
    /// Parsed preview data on success.
    pub(crate) preview: Option<GcodePreview>,
    /// Generation error, if the program could not be produced.
    pub(crate) error: Option<String>,
    /// Longest source line in characters, for the G-code panel width.
    pub(crate) max_line_chars: usize,
    /// Playback clock in seconds.
    pub(crate) time_s: f64,
    /// Whether the clock is running.
    pub(crate) playing: bool,
    /// Playback speed multiplier.
    pub(crate) speed: f32,
    /// Keep the G-code panel on the current command.
    pub(crate) follow: bool,
    /// Draw rapid/travel moves.
    pub(crate) show_rapid: bool,
    /// Draw cutting moves.
    pub(crate) show_cut: bool,
    /// Camera state.
    pub(crate) camera: PreviewCamera,
    /// Vertical exaggeration used only by the 3D view.
    pub(crate) z_scale: f32,
    /// Refit the camera on the next frame.
    pub(crate) fit_pending: bool,
    /// Transient message shown in the preview header.
    pub(crate) notice: Option<String>,
    /// Whether the notice is an error.
    pub(crate) notice_error: bool,
    /// Path of the last export from this preview.
    pub(crate) exported_path: Option<PathBuf>,
}

impl PreviewSession {
    /// Open a preview for a generated program.
    pub(crate) fn new(mode: PreviewMode, program: GcodeProgram) -> Self {
        let preview = GcodePreview::from_program(&program);
        let max_line_chars = preview
            .lines
            .iter()
            .map(|line| line.text.chars().count())
            .max()
            .unwrap_or(80);
        let z_scale = 4.0;
        Self {
            mode,
            program: Some(program),
            preview: Some(preview),
            error: None,
            max_line_chars,
            time_s: 0.0,
            playing: false,
            speed: 4.0,
            follow: true,
            show_rapid: true,
            show_cut: true,
            camera: PreviewCamera::default(),
            z_scale,
            fit_pending: true,
            notice: None,
            notice_error: false,
            exported_path: None,
        }
    }

    /// Open an error state for a generation failure.
    pub(crate) fn with_error(mode: PreviewMode, error: impl Into<String>) -> Self {
        let mut session = Self {
            mode,
            program: None,
            preview: None,
            error: Some(error.into()),
            max_line_chars: 80,
            time_s: 0.0,
            playing: false,
            speed: 4.0,
            follow: true,
            show_rapid: true,
            show_cut: true,
            camera: PreviewCamera::default(),
            z_scale: 4.0,
            fit_pending: false,
            notice: None,
            notice_error: false,
            exported_path: None,
        };
        session.notice_error = true;
        session
    }

    /// Whether preview data is available.
    pub(crate) fn is_ready(&self) -> bool {
        self.preview.is_some()
    }

    /// Total simulated duration in seconds.
    pub(crate) fn duration_s(&self) -> f64 {
        self.preview.as_ref().map_or(0.0, GcodePreview::duration_s)
    }

    /// Progress in the range `0.0..=1.0`.
    pub(crate) fn progress(&self) -> f32 {
        self.preview
            .as_ref()
            .map_or(0.0, |preview| preview.progress(self.time_s))
    }

    /// Format the current and total times.
    pub(crate) fn time_label(&self) -> String {
        format!(
            "{} / {}",
            format_seconds(self.time_s),
            format_seconds(self.duration_s())
        )
    }

    /// Current source line.
    pub(crate) fn current_line(&self) -> usize {
        self.preview
            .as_ref()
            .map_or(0, |preview| preview.line_at(self.time_s))
    }

    /// Human-readable scope label.
    pub(crate) fn mode_label(&self) -> &'static str {
        if self.mode.is_all() {
            "combined program"
        } else {
            "single task"
        }
    }

    /// Current line count.
    pub(crate) fn line_count(&self) -> usize {
        self.preview
            .as_ref()
            .map_or(0, |preview| preview.lines.len())
    }

    /// Snapshot of the simulated machine.
    pub(crate) fn state(&self) -> Option<PreviewState> {
        self.preview
            .as_ref()
            .map(|preview| preview.state_at(self.time_s))
    }

    /// Advance the clock by `dt` real seconds.
    pub(crate) fn advance(&mut self, dt: f64) {
        let duration = self.duration_s();
        if !self.playing || duration <= 0.0 {
            return;
        }
        self.time_s = (self.time_s + dt * f64::from(self.speed)).min(duration);
        if self.time_s >= duration {
            self.playing = false;
        }
    }

    /// Start or pause playback.
    pub(crate) fn toggle_play(&mut self) {
        if !self.is_ready() {
            return;
        }
        if !self.playing && self.time_s >= self.duration_s() {
            self.time_s = 0.0;
        }
        self.playing = !self.playing;
    }

    /// Stop playback and rewind.
    pub(crate) fn stop(&mut self) {
        self.playing = false;
        self.time_s = 0.0;
    }

    /// Jump to an absolute time.
    pub(crate) fn seek_time(&mut self, time: f64) {
        self.time_s = time.clamp(0.0, self.duration_s());
    }

    /// Jump to a fraction of the program.
    pub(crate) fn seek_fraction(&mut self, fraction: f32) {
        self.seek_time(f64::from(fraction) * self.duration_s());
    }

    /// Jump to the start of a task block.
    pub(crate) fn seek_task(&mut self, task: usize) {
        if let Some((start, _)) = self
            .preview
            .as_ref()
            .and_then(|preview| preview.task_time_range(task))
        {
            self.seek_time(start);
        }
    }

    /// Step the clock by whole source lines.
    pub(crate) fn step_lines(&mut self, delta: isize) {
        let Some(preview) = &self.preview else {
            return;
        };
        self.playing = false;
        let current = preview.line_at(self.time_s) as isize + delta;
        let target = current.clamp(0, preview.lines.len().saturating_sub(1) as isize) as usize;
        self.seek_time(preview.time_for_line(target));
    }

    /// Choose the next or previous playback speed.
    pub(crate) fn nudge_speed(&mut self, direction: isize) {
        let current = SPEEDS
            .iter()
            .position(|speed| (*speed - self.speed).abs() < f32::EPSILON)
            .unwrap_or(4);
        let next = (current as isize + direction).clamp(0, SPEEDS.len().saturating_sub(1) as isize)
            as usize;
        self.speed = SPEEDS[next];
    }

    /// Current tool metadata.
    pub(crate) fn tool(&self, number: Option<usize>) -> Option<&pcb_cam_core::GcodeProgramTool> {
        let number = number?;
        self.program
            .as_ref()?
            .tools
            .iter()
            .find(|tool| tool.number == number)
    }

    /// Set a transient message in the preview header.
    pub(crate) fn set_notice(&mut self, message: impl Into<String>, error: bool) {
        self.notice = Some(message.into());
        self.notice_error = error;
    }

    /// Fraction of the path that has been completed, for the overlay.
    pub(crate) fn state_tool_label(&self) -> String {
        self.state()
            .and_then(|state| self.tool(state.tool))
            .map_or_else(
                || "No tool".to_owned(),
                |tool| format!("T{} {}", tool.number, tool.name),
            )
    }
}

/// Map a scroll wheel delta to a zoom factor.
pub(crate) fn scroll_zoom(delta: f32) -> f32 {
    (delta * 0.0025).exp()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcb_cam_core::GcodeProgramTask;
    use pcb_cam_core::GcodeProgramTool;

    fn program() -> GcodeProgram {
        GcodeProgram {
            text: "(PCB-CAM test)\n\
                   (G-code 0: X0.000 Y0.000 mm)\n\
                   (Travel feed: 3000 mm/min)\n\
                   (Safe Z: 3.000 mm)\n\
                   G21\n\
                   G90\n\
                   (Drill - Holes)\n\
                   T1 M6 (T1: Drill 0.80)\n\
                   S10000 M3\n\
                   G00 Z3.000\n\
                   G01 X10.000 Y10.000 F3000.0\n\
                   G01 Z-1.500 F120.0\n\
                   G04 P0.10\n\
                   G00 Z3.000\n\
                   M5\n\
                   G01 X0.000 Y0.000 F3000.0\n\
                   M30\n"
                .to_owned(),
            tool_count: 1,
            task_count: 1,
            hole_count: 1,
            slot_count: 0,
            path_count: 0,
            helical_hole_count: 0,
            cut_length_mm: 1.5,
            tasks: vec![GcodeProgramTask {
                name: "Holes".to_owned(),
                kind: "Drill".to_owned(),
                line_start: 6,
                line_end: 17,
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

    #[test]
    fn playback_advances_clamps_and_rewinds() {
        let mut session = PreviewSession::new(PreviewMode::All, program());
        assert!(session.is_ready());
        assert!(session.duration_s() > 0.0);
        session.playing = true;
        session.speed = 100.0;
        session.advance(0.5);
        assert!(session.time_s > 0.0);
        session.seek_time(f64::INFINITY);
        assert!((session.time_s - session.duration_s()).abs() < 1e-9);
        session.toggle_play();
        assert_eq!(session.time_s, 0.0);
        assert!(session.playing);
        session.stop();
        assert!(!session.playing);
        assert_eq!(session.time_s, 0.0);
    }

    #[test]
    fn seeking_updates_the_current_command() {
        let mut session = PreviewSession::new(PreviewMode::Task(OperationId::new(1)), program());
        let total = session.duration_s();
        session.seek_time(total * 0.5);
        let state = session.state().expect("state");
        assert!(state.position.z <= 3.0 + 1e-6);
        assert!(session.current_line() < session.line_count());
        session.seek_task(0);
        assert!(session.time_s < total);
        session.step_lines(1);
        assert!(!session.playing);
    }

    #[test]
    fn camera_fits_the_toolpath() {
        let session = PreviewSession::new(PreviewMode::All, program());
        let mut camera = PreviewCamera::default();
        let rect = Rect::from_min_size(eframe::egui::pos2(0.0, 0.0), vec2(800.0, 600.0));
        let data = session.preview.as_ref().unwrap();
        camera.fit(data.min, data.max, 4.0, rect);
        assert!(camera.zoom > 0.0);
        assert!(camera.target.x.is_finite());
        assert!(camera.target.z.is_finite());
        camera.set_preset(CameraPreset::Top);
        assert!((camera.pitch - FRAC_PI_2).abs() < 0.01);
    }
}
