//! Visual identity and egui style for PBC-CAM.

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Theme, ThemePreference,
    Vec2, vec2,
};

// --- Palette -------------------------------------------------------------

/// Window background behind the panels.
pub(crate) const APP_BG: Color32 = Color32::from_rgb(10, 13, 17);
/// CAD canvas background.
pub(crate) const CANVAS: Color32 = Color32::from_rgb(8, 11, 15);
/// Regular panel background.
pub(crate) const PANEL: Color32 = Color32::from_rgb(17, 22, 28);
/// Raised surfaces such as cards and popups.
pub(crate) const PANEL_RAISED: Color32 = Color32::from_rgb(22, 28, 36);
/// Hovered widget background.
pub(crate) const SURFACE_HOVER: Color32 = Color32::from_rgb(30, 39, 49);
/// Active/pressed widget background.
pub(crate) const SURFACE_ACTIVE: Color32 = Color32::from_rgb(38, 49, 61);
/// Subtle borders and separators.
pub(crate) const BORDER: Color32 = Color32::from_rgb(34, 44, 55);
/// Stronger borders for key surfaces.
pub(crate) const BORDER_STRONG: Color32 = Color32::from_rgb(48, 62, 77);
/// Primary text.
pub(crate) const TEXT: Color32 = Color32::from_rgb(222, 231, 241);
/// Secondary text.
pub(crate) const TEXT_MUTED: Color32 = Color32::from_rgb(133, 148, 164);
/// Disabled and decorative text.
pub(crate) const TEXT_FAINT: Color32 = Color32::from_rgb(83, 97, 112);
/// Copper accent.
pub(crate) const COPPER: Color32 = Color32::from_rgb(227, 164, 94);
/// Bright copper for highlights and the logo mark.
pub(crate) const COPPER_BRIGHT: Color32 = Color32::from_rgb(243, 191, 132);
/// Dark copper for borders.
pub(crate) const COPPER_DIM: Color32 = Color32::from_rgb(124, 87, 52);
/// Success and ready-state accent.
pub(crate) const ACCENT: Color32 = Color32::from_rgb(104, 204, 169);
/// Error text and status dot.
pub(crate) const ERROR: Color32 = Color32::from_rgb(233, 112, 112);
/// Selected task toolpath/position overlay on the canvas.
pub(crate) const TASK_OVERLAY: Color32 = Color32::from_rgb(255, 82, 82);
/// Minor canvas grid lines.
pub(crate) const GRID_MINOR: Color32 = Color32::from_rgb(18, 24, 31);
/// Major canvas grid lines.
pub(crate) const GRID_MAJOR: Color32 = Color32::from_rgb(29, 38, 48);
/// X axis on the canvas.
pub(crate) const AXIS_X: Color32 = Color32::from_rgb(121, 76, 76);
/// Y axis on the canvas.
pub(crate) const AXIS_Y: Color32 = Color32::from_rgb(79, 120, 92);

/// Apply the fixed PBC-CAM dark style to an egui context.
pub(crate) fn apply(ctx: &egui::Context) {
    let style = build_style();
    ctx.set_style_of(Theme::Dark, style.clone());
    ctx.set_style_of(Theme::Light, style);
    ctx.set_theme(ThemePreference::Dark);
}

fn build_style() -> egui::Style {
    let mut style = egui::Style {
        visuals: dark_visuals(),
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

fn dark_visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();

    visuals.panel_fill = APP_BG;
    visuals.window_fill = PANEL_RAISED;
    visuals.window_stroke = Stroke::new(1.0, BORDER_STRONG);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.faint_bg_color = PANEL_RAISED;
    visuals.extreme_bg_color = CANVAS;
    visuals.code_bg_color = CANVAS;
    visuals.hyperlink_color = ACCENT;
    visuals.warn_fg_color = COPPER;
    visuals.error_fg_color = Color32::from_rgb(233, 112, 112);
    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(227, 164, 94, 58);
    visuals.selection.stroke = Stroke::new(1.0, COPPER_BRIGHT);

    visuals.widgets.noninteractive = widget_visuals(PANEL, BORDER, TEXT, 5);
    visuals.widgets.inactive = widget_visuals(PANEL_RAISED, BORDER, TEXT_MUTED, 5);
    visuals.widgets.hovered = widget_visuals(SURFACE_HOVER, BORDER_STRONG, TEXT, 5);
    visuals.widgets.active = widget_visuals(SURFACE_ACTIVE, COPPER_DIM, COPPER_BRIGHT, 5);
    visuals.widgets.open = widget_visuals(SURFACE_ACTIVE, COPPER_DIM, TEXT, 5);

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
