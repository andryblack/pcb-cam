//! Visual identity, runtime palette and egui style for PCB-CAM.

use std::cell::{Cell, RefCell};

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Theme, ThemePreference,
    Vec2, vec2,
};
use serde::{Deserialize, Serialize};

/// Application colour theme.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    /// Follow the operating-system preference.
    System,
    /// Always use the dark palette.
    #[default]
    Dark,
    /// Always use the light palette.
    Light,
}

impl ThemeMode {
    /// Human-readable label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }

    const fn preference(self) -> ThemePreference {
        match self {
            Self::System => ThemePreference::System,
            Self::Dark => ThemePreference::Dark,
            Self::Light => ThemePreference::Light,
        }
    }
}

/// Colours used for canvas objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasColors {
    /// Fill of unioned Gerber geometry.
    pub gerber_fill: [u8; 3],
    /// Outline of unioned Gerber geometry.
    pub gerber_outline: [u8; 3],
    /// Excellon holes and slots.
    pub excellon: [u8; 3],
    /// Selected-task toolpath overlay.
    pub task_overlay: [u8; 3],
}

impl Default for CanvasColors {
    fn default() -> Self {
        Self {
            gerber_fill: [204, 245, 142],
            gerber_outline: [100, 174, 94],
            excellon: [190, 203, 215],
            task_overlay: [255, 0, 0],
        }
    }
}

/// Persisted appearance settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceSettings {
    /// Colour theme or system preference.
    pub theme: ThemeMode,
    /// Canvas object colours.
    pub colors: CanvasColors,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Dark,
            colors: CanvasColors::default(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Palette {
    app_bg: Color32,
    canvas: Color32,
    panel: Color32,
    panel_raised: Color32,
    surface_hover: Color32,
    surface_active: Color32,
    border: Color32,
    border_strong: Color32,
    text: Color32,
    text_muted: Color32,
    text_faint: Color32,
    copper: Color32,
    copper_bright: Color32,
    copper_dim: Color32,
    accent: Color32,
    error: Color32,
    grid_minor: Color32,
    grid_major: Color32,
    axis_x: Color32,
    axis_y: Color32,
    gerber_fill: Color32,
    gerber_outline: Color32,
    excellon: Color32,
    task_overlay: Color32,
}

impl Palette {
    fn dark(colors: CanvasColors) -> Self {
        Self {
            app_bg: Color32::from_rgb(10, 13, 17),
            canvas: Color32::from_rgb(8, 11, 15),
            panel: Color32::from_rgb(17, 22, 28),
            panel_raised: Color32::from_rgb(22, 28, 36),
            surface_hover: Color32::from_rgb(30, 39, 49),
            surface_active: Color32::from_rgb(38, 49, 61),
            border: Color32::from_rgb(34, 44, 55),
            border_strong: Color32::from_rgb(48, 62, 77),
            text: Color32::from_rgb(222, 231, 241),
            text_muted: Color32::from_rgb(133, 148, 164),
            text_faint: Color32::from_rgb(83, 97, 112),
            copper: Color32::from_rgb(227, 164, 94),
            copper_bright: Color32::from_rgb(243, 191, 132),
            copper_dim: Color32::from_rgb(124, 87, 52),
            accent: Color32::from_rgb(104, 204, 169),
            error: Color32::from_rgb(233, 112, 112),
            grid_minor: Color32::from_rgb(18, 24, 31),
            grid_major: Color32::from_rgb(29, 38, 48),
            axis_x: Color32::from_rgb(121, 76, 76),
            axis_y: Color32::from_rgb(79, 120, 92),
            gerber_fill: rgb(colors.gerber_fill),
            gerber_outline: rgb(colors.gerber_outline),
            excellon: rgb(colors.excellon),
            task_overlay: rgb(colors.task_overlay),
        }
    }

    fn light(colors: CanvasColors) -> Self {
        Self {
            app_bg: Color32::from_rgb(238, 241, 245),
            canvas: Color32::from_rgb(250, 251, 252),
            panel: Color32::from_rgb(247, 248, 250),
            panel_raised: Color32::from_rgb(255, 255, 255),
            surface_hover: Color32::from_rgb(232, 236, 241),
            surface_active: Color32::from_rgb(220, 226, 233),
            border: Color32::from_rgb(210, 216, 224),
            border_strong: Color32::from_rgb(185, 193, 203),
            text: Color32::from_rgb(30, 37, 46),
            text_muted: Color32::from_rgb(92, 105, 120),
            text_faint: Color32::from_rgb(140, 152, 166),
            copper: Color32::from_rgb(174, 101, 31),
            copper_bright: Color32::from_rgb(202, 125, 45),
            copper_dim: Color32::from_rgb(210, 170, 130),
            accent: Color32::from_rgb(20, 140, 105),
            error: Color32::from_rgb(190, 45, 45),
            grid_minor: Color32::from_rgb(238, 241, 245),
            grid_major: Color32::from_rgb(215, 222, 230),
            axis_x: Color32::from_rgb(170, 80, 80),
            axis_y: Color32::from_rgb(70, 140, 90),
            gerber_fill: rgb(colors.gerber_fill),
            gerber_outline: rgb(colors.gerber_outline),
            excellon: rgb(colors.excellon),
            task_overlay: rgb(colors.task_overlay),
        }
    }
}

fn rgb(color: [u8; 3]) -> Color32 {
    Color32::from_rgb(color[0], color[1], color[2])
}

thread_local! {
    static CURRENT: RefCell<Palette> = RefCell::new(Palette::dark(CanvasColors::default()));
    static ACTIVE_THEME: Cell<Theme> = const { Cell::new(Theme::Dark) };
}

fn palette() -> Palette {
    CURRENT.with(|palette| *palette.borrow())
}

macro_rules! color_accessor {
    ($($name:ident => $field:ident),* $(,)?) => {
        $(
            pub(crate) fn $name() -> Color32 {
                palette().$field
            }
        )*
    };
}

color_accessor! {
    app_bg => app_bg,
    canvas => canvas,
    panel => panel,
    panel_raised => panel_raised,
    surface_active => surface_active,
    border => border,
    border_strong => border_strong,
    text => text,
    text_muted => text_muted,
    text_faint => text_faint,
    copper => copper,
    copper_bright => copper_bright,
    copper_dim => copper_dim,
    accent => accent,
    error => error,
    grid_minor => grid_minor,
    grid_major => grid_major,
    axis_x => axis_x,
    axis_y => axis_y,
    gerber_fill => gerber_fill,
    gerber_outline => gerber_outline,
    excellon => excellon,
    task_overlay => task_overlay,
}

/// Currently active resolved theme.
pub(crate) fn active_theme() -> Theme {
    ACTIVE_THEME.with(Cell::get)
}

fn resolved_theme(ctx: &egui::Context, mode: ThemeMode) -> Theme {
    match mode {
        ThemeMode::Dark => Theme::Dark,
        ThemeMode::Light => Theme::Light,
        ThemeMode::System => ctx.system_theme().unwrap_or(Theme::Dark),
    }
}

/// Apply appearance settings to the egui context and runtime palette.
pub(crate) fn apply(ctx: &egui::Context, appearance: &AppearanceSettings) {
    let dark = build_style(&Palette::dark(appearance.colors));
    let light = build_style(&Palette::light(appearance.colors));
    ctx.set_style_of(Theme::Dark, dark);
    ctx.set_style_of(Theme::Light, light);
    ctx.set_theme(appearance.theme.preference());

    let theme = resolved_theme(ctx, appearance.theme);
    let palette = match theme {
        Theme::Dark => Palette::dark(appearance.colors),
        Theme::Light => Palette::light(appearance.colors),
    };
    CURRENT.with(|current| *current.borrow_mut() = palette);
    ACTIVE_THEME.with(|active| active.set(theme));
}

/// Reapply the palette when the system theme changes.
pub(crate) fn sync(ctx: &egui::Context, appearance: &AppearanceSettings) {
    if resolved_theme(ctx, appearance.theme) != active_theme() {
        apply(ctx, appearance);
    }
}

fn build_style(palette: &Palette) -> egui::Style {
    let mut style = egui::Style {
        visuals: visuals(palette),
        animation_time: 0.10,
        ..Default::default()
    };
    style.spacing.item_spacing = vec2(8.0, 6.0);
    style.spacing.button_padding = vec2(9.0, 4.0);
    style.spacing.interact_size = Vec2::new(24.0, 24.0);
    style.spacing.menu_margin = egui::Margin::symmetric(6, 4);
    style.spacing.indent = 14.0;
    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(20.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(13.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(11.5, FontFamily::Monospace),
        ),
    ]
    .into();
    style
}

fn visuals(palette: &Palette) -> egui::Visuals {
    let dark = palette.canvas.r() < 128;
    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    visuals.panel_fill = palette.app_bg;
    visuals.window_fill = palette.panel_raised;
    visuals.window_stroke = Stroke::new(1.0, palette.border_strong);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.faint_bg_color = palette.panel_raised;
    visuals.extreme_bg_color = palette.canvas;
    visuals.code_bg_color = palette.canvas;
    visuals.hyperlink_color = palette.accent;
    visuals.warn_fg_color = palette.copper;
    visuals.error_fg_color = palette.error;
    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(
        palette.copper.r(),
        palette.copper.g(),
        palette.copper.b(),
        58,
    );
    visuals.selection.stroke = Stroke::new(1.0, palette.copper_bright);

    visuals.widgets.noninteractive = widget_visuals(palette.panel, palette.border, palette.text, 5);
    visuals.widgets.inactive =
        widget_visuals(palette.panel_raised, palette.border, palette.text_muted, 5);
    visuals.widgets.hovered = widget_visuals(
        palette.surface_hover,
        palette.border_strong,
        palette.text,
        5,
    );
    visuals.widgets.active = widget_visuals(
        palette.surface_active,
        palette.copper_dim,
        palette.copper_bright,
        5,
    );
    visuals.widgets.open =
        widget_visuals(palette.surface_active, palette.copper_dim, palette.text, 5);

    visuals
}

fn widget_visuals(
    bg_fill: Color32,
    bg_stroke: Color32,
    fg_stroke: Color32,
    radius: u8,
) -> egui::style::WidgetVisuals {
    egui::style::WidgetVisuals {
        bg_fill,
        weak_bg_fill: bg_fill,
        bg_stroke: Stroke::new(1.0, bg_stroke),
        corner_radius: CornerRadius::same(radius),
        fg_stroke: Stroke::new(1.0, fg_stroke),
        expansion: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_switches_runtime_palette() {
        let ctx = egui::Context::default();
        let mut settings = AppearanceSettings::default();
        settings.colors.gerber_fill = [11, 22, 33];
        apply(&ctx, &settings);
        assert_eq!(active_theme(), Theme::Dark);
        assert_eq!(gerber_fill(), Color32::from_rgb(11, 22, 33));
        assert_eq!(canvas(), Color32::from_rgb(8, 11, 15));

        settings.theme = ThemeMode::Light;
        apply(&ctx, &settings);
        assert_eq!(active_theme(), Theme::Light);
        assert_eq!(canvas(), Color32::from_rgb(250, 251, 252));
        assert_eq!(gerber_fill(), Color32::from_rgb(11, 22, 33));
    }

    #[test]
    fn appearance_settings_round_trip() {
        let settings = AppearanceSettings {
            theme: ThemeMode::Light,
            colors: CanvasColors {
                gerber_fill: [1, 2, 3],
                gerber_outline: [4, 5, 6],
                excellon: [7, 8, 9],
                task_overlay: [10, 11, 12],
            },
        };
        let json = serde_json::to_string(&settings).unwrap();
        let restored: AppearanceSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, settings);
        assert_eq!(ThemeMode::System.label(), "System");
    }
}
