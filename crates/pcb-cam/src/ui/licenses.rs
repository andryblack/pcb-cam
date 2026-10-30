//! In-app viewer for the bundled third-party license notices.

use eframe::egui::{self, Align, Align2, FontId, Layout, RichText, ScrollArea, TextWrapMode, Vec2};

use crate::licenses;
use crate::theme;

/// Cached lines and metrics for the notices viewer.
#[derive(Debug)]
pub(crate) struct LicensesViewer {
    pub(crate) open: bool,
    /// All notice lines, kept as `&'static str` so the viewer can virtualize.
    pub(crate) lines: Vec<&'static str>,
    /// Longest line in characters, used to size the horizontal scroll area.
    pub(crate) max_chars: usize,
}

impl Default for LicensesViewer {
    fn default() -> Self {
        let lines: Vec<&'static str> = licenses::TEXT.lines().collect();
        let max_chars = lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0);
        Self {
            open: false,
            lines,
            max_chars,
        }
    }
}

pub(crate) fn show(ctx: &egui::Context, viewer: &mut LicensesViewer) {
    if !viewer.open {
        return;
    }

    let mut open = true;
    let mut close_clicked = false;

    egui::Window::new("Third-party licenses")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .default_width(860.0)
        .default_height(620.0)
        .min_width(520.0)
        .min_height(320.0)
        .frame(crate::app::dialog_frame())
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Third-party licenses").size(17.0).strong());
                    ui.label(
                        RichText::new(
                            "Generated from Cargo.lock and shipped with the application \
                             as THIRD_PARTY_LICENSES.md.",
                        )
                        .size(10.5)
                        .color(theme::text_muted()),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        close_clicked = true;
                    }
                    if ui.button("Copy all").clicked() {
                        ui.ctx().copy_text(licenses::TEXT.to_owned());
                    }
                });
            });

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);

            let font = FontId::monospace(10.5);
            let row_height = ui.ctx().fonts_mut(|fonts| fonts.row_height(&font));
            let char_width = ui.ctx().fonts_mut(|fonts| fonts.glyph_width(&font, 'M'));
            let content_width =
                (viewer.max_chars as f32 * char_width + 24.0).max(ui.available_width());

            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                ScrollArea::both()
                    .id_salt("third_party_licenses_scroll")
                    .auto_shrink([false, false])
                    .show_rows(ui, row_height, viewer.lines.len(), |ui, range| {
                        ui.set_min_width(content_width);
                        for index in range {
                            let text = viewer.lines[index];
                            ui.label(
                                RichText::new(if text.is_empty() { " " } else { text })
                                    .font(font.clone())
                                    .color(theme::text_muted()),
                            );
                        }
                    });
            });
        });

    viewer.open = open && !close_clicked;
}

#[cfg(test)]
mod tests {
    use eframe::egui::{self, Pos2, Rect, vec2};

    use super::{LicensesViewer, show};

    #[test]
    fn bundled_notices_are_non_empty() {
        let viewer = LicensesViewer::default();
        assert!(viewer.lines.len() > 1_000);
        assert!(viewer.max_chars > 40);
    }

    #[test]
    fn viewer_renders_one_frame() {
        let ctx = egui::Context::default();
        let mut viewer = LicensesViewer {
            open: true,
            ..LicensesViewer::default()
        };
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
            ..Default::default()
        };
        let mut output = ctx.run_pass(input, |ctx| show(ctx, &mut viewer));
        output.textures_delta.clear();
        assert!(viewer.open);
    }
}
