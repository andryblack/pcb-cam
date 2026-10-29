//! Syntax-highlighted G-code panel for the preview screen.

use eframe::egui::{
    self, Align, Align2, Button, Color32, CornerRadius, FontId, Frame, Layout, Margin, Painter,
    Rect, RichText, Sense, Stroke, Ui, pos2, text::LayoutJob, text::TextFormat, text::TextWrapping,
    vec2,
};
use pcb_cam_core::preview::{MoveKind, PreviewLine};

use crate::preview::PreviewSession;
use crate::theme;
use crate::ui::widgets;

const GUTTER_WIDTH: f32 = 52.0;
const ROW_HEIGHT: f32 = 16.5;
const FONT_SIZE: f32 = 10.5;

/// Draw the right-hand G-code panel.
pub(crate) fn show(ui: &mut Ui, preview: &mut PreviewSession) {
    let frame = Frame::new()
        .fill(theme::panel())
        .inner_margin(Margin::symmetric(0, 0))
        .stroke(Stroke::new(1.0, theme::border()));

    egui::Panel::right("preview_gcode")
        .default_size(400.0)
        .min_size(300.0)
        .max_size(660.0)
        .resizable(true)
        .frame(frame)
        .show_separator_line(false)
        .show(ui, |ui| {
            header(ui, preview);
            ui.separator();
            if preview.preview.is_none() {
                ui.add_space(12.0);
                widgets::hint_card(
                    ui,
                    "No program",
                    "The G-code panel is empty because the preview could not be generated.",
                );
                return;
            }
            let clicked = lines(ui, preview);
            if let Some(line) = clicked {
                preview.playing = false;
                let time = preview
                    .preview
                    .as_ref()
                    .map_or(0.0, |data| data.time_for_line(line));
                preview.seek_time(time);
            }
        });
}

fn header(ui: &mut Ui, preview: &mut PreviewSession) {
    Frame::new()
        .fill(theme::panel_raised())
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                widgets::section_label(ui, "G-code");
                widgets::pill(ui, &preview.line_count().to_string(), theme::text_muted());
                ui.label(
                    RichText::new(format!("line {}", preview.current_line() + 1))
                        .size(9.5)
                        .color(theme::copper()),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let ready = preview.program.is_some();
                    let copy = ui.add_enabled(ready, Button::new(RichText::new("Copy").size(10.0)));
                    if copy.clicked()
                        && let Some(program) = &preview.program
                    {
                        ui.ctx().copy_text(program.text.clone());
                        preview.set_notice("Copied the complete G-code program", false);
                    }
                    ui.checkbox(&mut preview.follow, RichText::new("Follow").size(10.0))
                        .on_hover_text("Keep the current command centred while playing");
                });
            });
            if let Some(notice) = &preview.notice {
                ui.add_space(2.0);
                ui.label(
                    RichText::new(notice)
                        .size(9.5)
                        .color(if preview.notice_error {
                            theme::error()
                        } else {
                            theme::accent()
                        }),
                );
            }
        });
}

fn lines(ui: &mut Ui, preview: &mut PreviewSession) -> Option<usize> {
    let mut clicked = None;
    let data = preview.preview.as_ref()?;
    let font = FontId::monospace(FONT_SIZE);
    let char_width = ui.ctx().fonts_mut(|fonts| fonts.glyph_width(&font, 'M'));
    let total = data.lines.len();
    let max_chars = preview.max_line_chars.max(80);
    let content_width =
        (GUTTER_WIDTH + max_chars as f32 * char_width + 28.0).max(ui.available_width());
    let content_height = total as f32 * ROW_HEIGHT;
    let current = data.line_at(preview.time_s);
    let follow = preview.follow;
    let viewport_height = ui.available_height();
    let offset = (current as f32 * ROW_HEIGHT - viewport_height * 0.45)
        .clamp(0.0, (content_height - viewport_height).max(0.0));
    let gutter_color = theme::text_faint();
    let current_bg = theme::surface_active();
    let hover_bg = theme::surface_hover();
    let stripe = theme::copper();

    let mut scroll = egui::ScrollArea::both()
        .id_salt("preview_gcode_scroll")
        .auto_shrink([false, false]);
    if follow {
        scroll = scroll.vertical_scroll_offset(offset);
    }
    scroll.show_viewport(ui, |ui, viewport| {
        ui.set_min_size(vec2(content_width, content_height));
        let top = ui.max_rect().top();
        let left = ui.max_rect().left();
        let first = (viewport.min.y / ROW_HEIGHT).floor().max(0.0) as usize;
        let last = ((viewport.max.y / ROW_HEIGHT).ceil() as usize + 1).min(total);
        let clip = ui.clip_rect();
        let painter = ui.painter().clone();

        for index in first..last {
            let row = Rect::from_min_size(
                pos2(left, top + index as f32 * ROW_HEIGHT),
                vec2(content_width, ROW_HEIGHT),
            );
            if !row.intersects(clip.expand(2.0)) {
                continue;
            }
            let response = ui.interact(
                row,
                ui.scope_id().with(("preview_gcode_line", index)),
                Sense::click(),
            );
            if index == current {
                painter.rect_filled(row, CornerRadius::ZERO, current_bg);
                painter.rect_filled(
                    Rect::from_min_size(row.min, vec2(2.0, row.height())),
                    CornerRadius::ZERO,
                    stripe,
                );
            } else if response.hovered() {
                painter.rect_filled(row, CornerRadius::ZERO, with_alpha(hover_bg, 170));
            }
            if response.clicked() {
                clicked = Some(index);
            }

            let line = &data.lines[index];
            draw_marker(&painter, data, line, row, index == current);
            painter.text(
                pos2(row.left() + GUTTER_WIDTH - 8.0, row.center().y),
                Align2::RIGHT_CENTER,
                format!("{}", index + 1),
                FontId::monospace(9.0),
                gutter_color,
            );
            let job = highlight(line, &font);
            let galley = ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));
            painter.galley(
                pos2(row.left() + GUTTER_WIDTH, row.top() + 1.0),
                galley,
                theme::text(),
            );
        }
    });

    clicked
}

fn draw_marker(
    painter: &Painter,
    data: &pcb_cam_core::preview::GcodePreview,
    line: &PreviewLine,
    row: Rect,
    current: bool,
) {
    let Some(index) = line.move_index else {
        return;
    };
    let Some(movement) = data.moves.get(index) else {
        return;
    };
    let color = match movement.kind {
        kind if kind.is_cutting() => theme::preview_cut(),
        MoveKind::Retract => theme::text_faint(),
        _ => theme::preview_rapid(),
    };
    let center = pos2(row.left() + 8.0, row.center().y);
    if current {
        painter.circle_filled(center, 3.0, color);
    } else {
        painter.circle_filled(center, 2.0, with_alpha(color, 150));
    }
}

fn highlight(line: &PreviewLine, font: &FontId) -> LayoutJob {
    let mut job = LayoutJob {
        wrap: TextWrapping {
            max_width: f32::INFINITY,
            ..Default::default()
        },
        ..Default::default()
    };
    let text = &line.text;
    let base = theme::text();
    let mut index = 0;
    while index < text.len() {
        let rest = &text[index..];
        let Some(character) = rest.chars().next() else {
            break;
        };
        if character == '(' {
            let end = comment_end(rest);
            append(&mut job, &rest[..end], font, theme::text_faint(), true);
            index += end;
        } else if character == ';' {
            append(&mut job, rest, font, theme::text_faint(), true);
            break;
        } else if character.is_whitespace() {
            append(&mut job, &rest[..character.len_utf8()], font, base, false);
            index += character.len_utf8();
        } else {
            let end = rest
                .char_indices()
                .find(|(_, character)| {
                    character.is_whitespace() || *character == '(' || *character == ';'
                })
                .map_or(rest.len(), |(index, _)| index);
            let word = &rest[..end];
            append(&mut job, word, font, word_color(word), false);
            index += end;
        }
    }
    job
}

fn comment_end(rest: &str) -> usize {
    let mut depth = 0usize;
    for (index, character) in rest.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return index + character.len_utf8();
                }
            }
            _ => {}
        }
    }
    rest.len()
}

fn append(job: &mut LayoutJob, text: &str, font: &FontId, color: Color32, italics: bool) {
    if text.is_empty() {
        return;
    }
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: font.clone(),
            color,
            italics,
            ..Default::default()
        },
    );
}

fn word_color(word: &str) -> Color32 {
    match word
        .chars()
        .next()
        .map(|character| character.to_ascii_uppercase())
    {
        Some('G') => theme::copper(),
        Some('M') => theme::accent(),
        Some('T') => theme::copper_bright(),
        Some('F' | 'S') => theme::accent(),
        Some('X' | 'Y' | 'Z' | 'I' | 'J' | 'K' | 'R') => theme::text(),
        Some('N') => theme::text_muted(),
        _ => theme::text_muted(),
    }
}

fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}
