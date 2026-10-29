//! G-code preview screen: header, program info, transport timeline and layout.

use eframe::egui::{
    self, Align, Align2, Button, Color32, CornerRadius, FontId, Frame, Layout, Margin, RichText,
    Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};
use pcb_cam_core::preview::{GcodePreview, format_seconds};

use crate::app::PbcCam;
use crate::preview::{PreviewMode, PreviewSession, SPEEDS};
use crate::theme;
use crate::ui::{gcode_panel, preview_viewport, widgets};

enum PreviewAction {
    Close,
    Export,
    Regenerate,
}

/// Draw the complete preview workspace.
pub(crate) fn show(ui: &mut Ui, app: &mut PbcCam) {
    let mut action = None;
    header(ui, app, &mut action);
    timeline(ui, app);
    info_panel(ui, app);
    if let Some(preview) = app.preview.as_mut() {
        gcode_panel::show(ui, preview);
    }
    egui::CentralPanel::no_frame().show(ui, |ui| {
        if let Some(preview) = app.preview.as_mut() {
            preview_viewport::show(ui, preview);
        }
    });
    match action {
        Some(PreviewAction::Close) => app.close_preview(),
        Some(PreviewAction::Export) => app.export_preview_gcode(),
        Some(PreviewAction::Regenerate) => app.regenerate_preview(),
        None => {}
    }
}

fn header(ui: &mut Ui, app: &PbcCam, action: &mut Option<PreviewAction>) {
    let frame = Frame::new()
        .fill(theme::panel())
        .inner_margin(Margin::symmetric(12, 7))
        .stroke(Stroke::new(1.0, theme::border()));

    egui::Panel::top("preview_header")
        .exact_size(78.0)
        .frame(frame)
        .show_separator_line(false)
        .show(ui, |ui| {
            let Some(preview) = app.preview.as_ref() else {
                return;
            };
            let ready = preview.is_ready();
            ui.horizontal(|ui| {
                let back = ui
                    .add(Button::new(RichText::new("‹ Workspace").size(11.5)))
                    .on_hover_text("Return to the design workspace (Esc)");
                if back.clicked() {
                    *action = Some(PreviewAction::Close);
                }
                ui.separator();
                widgets::pill(ui, "G-CODE PREVIEW", theme::copper());

                let title = match preview.mode {
                    PreviewMode::All => "All enabled tasks".to_owned(),
                    PreviewMode::Task(id) => app
                        .project()
                        .task(id)
                        .map(|task| task.name().to_owned())
                        .or_else(|| {
                            preview
                                .program
                                .as_ref()
                                .and_then(|program| program.tasks.first())
                                .map(|task| task.name.clone())
                        })
                        .unwrap_or_else(|| "Task".to_owned()),
                };
                ui.label(
                    RichText::new(title)
                        .size(13.0)
                        .strong()
                        .color(theme::text()),
                );
                ui.label(
                    RichText::new(format!("· {}", preview.mode_label()))
                        .size(9.5)
                        .color(theme::text_faint()),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let export = ui.add_enabled(
                        ready,
                        Button::new(RichText::new("Export G-code…").size(11.5)),
                    );
                    if export.clicked() {
                        *action = Some(PreviewAction::Export);
                    }
                    if !ready {
                        export.on_disabled_hover_text("The preview must be ready first");
                    }
                    if ui
                        .add(Button::new(RichText::new("Regenerate").size(11.0)))
                        .on_hover_text("Generate the preview again (Cmd/Ctrl+R)")
                        .clicked()
                    {
                        *action = Some(PreviewAction::Regenerate);
                    }
                });
            });

            ui.horizontal(|ui| {
                ui.add_space(2.0);
                let summary = preview_summary(app, preview);
                ui.add(
                    egui::Label::new(RichText::new(summary).size(9.5).color(theme::text_muted()))
                        .truncate(),
                );
                if let Some(notice) = &preview.notice {
                    ui.separator();
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
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new("Space play/pause · ←/→ step · Esc back")
                            .size(9.0)
                            .color(theme::text_faint()),
                    );
                });
            });
        });
}

fn preview_summary(app: &PbcCam, preview: &PreviewSession) -> String {
    let Some(program) = &preview.program else {
        return "Generation failed — see the details in the program panel".to_owned();
    };
    let lines = preview.line_count();
    let duration = preview.duration_s();
    let (cut_length, travel_length) = preview
        .preview
        .as_ref()
        .map_or((program.cut_length_mm, 0.0), |data| {
            (data.cut_length_mm, data.rapid_length_mm)
        });
    let disabled = app
        .project()
        .tasks()
        .iter()
        .filter(|task| !task.enabled())
        .count();
    let moves = preview.preview.as_ref().map_or(0, |data| data.moves.len());
    let mut summary = format!(
        "{} · {} tools · {} lines · {} holes · {} slots · {} paths · {} moves · cut {} · travel {} · est {}",
        task_count(program.task_count),
        program.tool_count,
        lines,
        program.hole_count,
        program.slot_count,
        program.path_count,
        moves,
        format_distance(cut_length),
        format_distance(travel_length),
        format_seconds(duration),
    );
    if disabled > 0 {
        summary.push_str(&format!(
            " · {disabled} disabled task{} skipped",
            if disabled == 1 { "" } else { "s" }
        ));
    }
    summary
}

fn info_panel(ui: &mut Ui, app: &mut PbcCam) {
    let Some(preview) = app.preview.as_mut() else {
        return;
    };
    let current_task = preview.state().and_then(|state| state.task);
    let frame = Frame::new()
        .fill(theme::panel())
        .inner_margin(Margin::symmetric(12, 10))
        .stroke(Stroke::new(1.0, theme::border()));

    egui::Panel::left("preview_info")
        .default_size(280.0)
        .min_size(230.0)
        .max_size(400.0)
        .resizable(true)
        .frame(frame)
        .show_separator_line(false)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                widgets::section_label(ui, "Program");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} lines", preview.line_count()))
                            .size(9.5)
                            .color(theme::text_faint()),
                    );
                });
            });
            ui.add_space(6.0);

            let mut seek_task = None;
            egui::ScrollArea::vertical()
                .id_salt("preview_info_scroll")
                .auto_shrink([false, false])
                .scroll_bar_visibility(
                    egui::containers::scroll_area::ScrollBarVisibility::AlwaysVisible,
                )
                .show(ui, |ui| {
                    if let Some(error) = &preview.error {
                        Frame::new()
                            .fill(Color32::from_rgba_unmultiplied(
                                theme::error().r(),
                                theme::error().g(),
                                theme::error().b(),
                                22,
                            ))
                            .stroke(Stroke::new(1.0, theme::error()))
                            .corner_radius(6)
                            .inner_margin(Margin::same(10))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(
                                    RichText::new("Generation failed")
                                        .size(11.0)
                                        .strong()
                                        .color(theme::error()),
                                );
                                ui.add_space(3.0);
                                ui.label(
                                    RichText::new(error)
                                        .size(10.0)
                                        .monospace()
                                        .color(theme::error()),
                                );
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(
                                        "Return to the workspace and fix the task warnings, then preview again.",
                                    )
                                    .size(10.0)
                                    .color(theme::text_muted()),
                                );
                            });
                        ui.add_space(10.0);
                    }

                    if let Some(data) = &preview.preview {
                        summary_card(ui, data);
                        if let Some(path) = &preview.exported_path {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(format!("Exported to {}", path.display()))
                                    .size(9.0)
                                    .color(theme::accent()),
                            );
                        }
                        ui.add_space(12.0);

                        widgets::section_label(ui, &format!("Tasks  ·  {}", data.tasks.len()));
                        ui.add_space(4.0);
                        for (index, task) in data.tasks.iter().enumerate() {
                            let selected = current_task == Some(index);
                            let response = Frame::new()
                                .fill(if selected {
                                    theme::surface_active()
                                } else {
                                    theme::panel_raised()
                                })
                                .stroke(Stroke::new(
                                    1.0,
                                    if selected {
                                        theme::copper_dim()
                                    } else {
                                        theme::border()
                                    },
                                ))
                                .corner_radius(5)
                                .inner_margin(Margin::symmetric(8, 4))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        let (dot, _) = ui
                                            .allocate_exact_size(vec2(7.0, 7.0), Sense::hover());
                                        ui.painter().circle_filled(
                                            dot.center(),
                                            3.0,
                                            task_color(&task.kind),
                                        );
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(format!(
                                                    "{}. {} · {}",
                                                    index + 1,
                                                    task.kind,
                                                    task.name
                                                ))
                                                .size(10.5)
                                                .strong()
                                                .color(if selected {
                                                    theme::copper_bright()
                                                } else {
                                                    theme::text()
                                                }),
                                            )
                                            .truncate(),
                                        );
                                    });
                                    ui.label(
                                        RichText::new(task_detail(task))
                                            .size(9.0)
                                            .color(theme::text_faint()),
                                    );
                                })
                                .response
                                .interact(Sense::click())
                                .on_hover_text("Jump to this task");
                            if response.clicked() {
                                seek_task = Some(index);
                            }
                            if selected {
                                ui.add_space(2.0);
                            }
                        }

                        ui.add_space(8.0);
                        widgets::section_label(ui, &format!("Tools  ·  {}", data.tools.len()));
                        ui.add_space(4.0);
                        for tool in &data.tools {
                            Frame::new()
                                .fill(theme::panel_raised())
                                .stroke(Stroke::new(1.0, theme::border()))
                                .corner_radius(5)
                                .inner_margin(Margin::symmetric(8, 4))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        widgets::pill(
                                            ui,
                                            &format!("T{}", tool.number),
                                            theme::copper(),
                                        );
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(format!(
                                                    "{} · {}",
                                                    tool.name, tool.kind
                                                ))
                                                .size(10.0)
                                                .color(theme::text()),
                                            )
                                            .truncate(),
                                        );
                                        ui.with_layout(
                                            Layout::right_to_left(Align::Center),
                                            |ui| {
                                                ui.label(
                                                    RichText::new(format!(
                                                        "Ø{:.2}",
                                                        tool.diameter_mm
                                                    ))
                                                    .size(9.5)
                                                    .monospace()
                                                    .color(theme::text_muted()),
                                                );
                                            },
                                        );
                                    });
                                });
                        }
                    }

                    ui.add_space(12.0);
                });

            if let Some(index) = seek_task {
                preview.seek_task(index);
            }
        });
}

fn summary_card(ui: &mut Ui, data: &GcodePreview) {
    widgets::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        widgets::field_row(ui, "Estimated time", &format_seconds(data.total_time_s));
        widgets::field_row(
            ui,
            "Cutting · travel",
            &format!(
                "{} · {}",
                format_distance(data.cut_length_mm),
                format_distance(data.rapid_length_mm)
            ),
        );
    });
}

fn task_detail(task: &pcb_cam_core::GcodeProgramTask) -> String {
    let lines = format!(
        "lines {}–{}",
        task.line_start + 1,
        task.line_end.max(task.line_start + 1)
    );
    match (&task.tool_number, &task.tool_name) {
        (Some(number), Some(name)) => format!("T{number} · {name} · {lines}"),
        (Some(number), None) => format!("T{number} · {lines}"),
        _ => lines,
    }
}

fn task_color(kind: &str) -> Color32 {
    match kind {
        "Drill" => theme::copper(),
        "Spot drill" => theme::copper_bright(),
        "Hole milling" | "Edge cut" => theme::accent(),
        "Clearance" => theme::copper_bright(),
        _ => theme::text_muted(),
    }
}

fn task_count(count: usize) -> String {
    format!("{count} task{}", if count == 1 { "" } else { "s" })
}

fn format_distance(mm: f64) -> String {
    if mm >= 1_000.0 {
        format!("{:.2} m", mm / 1_000.0)
    } else {
        format!("{mm:.1} mm")
    }
}

fn timeline(ui: &mut Ui, app: &mut PbcCam) {
    let Some(preview) = app.preview.as_mut() else {
        return;
    };
    let frame = Frame::new()
        .fill(theme::panel())
        .inner_margin(Margin::symmetric(12, 7))
        .stroke(Stroke::new(1.0, theme::border()));

    egui::Panel::bottom("preview_timeline")
        .exact_size(116.0)
        .frame(frame)
        .show_separator_line(false)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let label = if preview.playing { "Pause" } else { "Play" };
                if ui
                    .add_sized(
                        vec2(62.0, 24.0),
                        Button::new(RichText::new(label).size(11.0)),
                    )
                    .on_hover_text("Play or pause (Space)")
                    .clicked()
                {
                    preview.toggle_play();
                }
                if ui
                    .add_sized(
                        vec2(46.0, 24.0),
                        Button::new(RichText::new("Stop").size(10.5)),
                    )
                    .on_hover_text("Rewind to the start")
                    .clicked()
                {
                    preview.stop();
                }
                if ui
                    .add_sized(
                        vec2(30.0, 24.0),
                        Button::new(RichText::new("‹").size(13.0)),
                    )
                    .on_hover_text("Previous command (←)")
                    .clicked()
                {
                    preview.step_lines(-1);
                }
                if ui
                    .add_sized(
                        vec2(30.0, 24.0),
                        Button::new(RichText::new("›").size(13.0)),
                    )
                    .on_hover_text("Next command (→)")
                    .clicked()
                {
                    preview.step_lines(1);
                }

                ui.add_space(4.0);
                ui.label(
                    RichText::new(preview.time_label())
                        .size(11.0)
                        .monospace()
                        .color(theme::text()),
                );
                ui.label(
                    RichText::new(format!("line {}", preview.current_line() + 1))
                        .size(9.5)
                        .monospace()
                        .color(theme::text_faint()),
                );

                ui.add_space(6.0);
                ui.label(
                    RichText::new("Speed")
                        .size(9.5)
                        .color(theme::text_muted()),
                );
                let mut speed = preview.speed;
                egui::ComboBox::from_id_salt("preview_speed")
                    .width(62.0)
                    .selected_text(format_speed(speed))
                    .show_ui(ui, |ui| {
                        for option in SPEEDS {
                            ui.selectable_value(&mut speed, option, format_speed(option));
                        }
                    });
                if (speed - preview.speed).abs() > f32::EPSILON {
                    preview.speed = speed;
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{:.1}%", preview.progress() * 100.0))
                            .size(10.5)
                            .monospace()
                            .color(theme::copper()),
                    );
                    ui.label(
                        RichText::new(preview.state_tool_label())
                            .size(9.5)
                            .color(theme::text_muted()),
                    );
                });
            });

            ui.add_space(5.0);
            draw_scrubber(ui, preview);
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                legend_dot(ui, "Cutting", theme::preview_cut());
                legend_dot(ui, "Rapid / travel", theme::preview_rapid());
                ui.separator();
                ui.label(
                    RichText::new(
                        "Drag the timeline to scrub · Space play/pause · ←/→ step · Shift+←/→ 10 lines",
                    )
                    .size(9.0)
                    .color(theme::text_faint()),
                );
            });
        });
}

fn draw_scrubber(ui: &mut Ui, preview: &mut PreviewSession) {
    let width = ui.available_width().max(120.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, 28.0), Sense::click_and_drag());
    if response.hovered() {
        ui.ctx()
            .set_cursor_icon(eframe::egui::CursorIcon::PointingHand);
    }
    let Some(data) = preview.preview.as_ref() else {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), theme::canvas());
        return;
    };
    let duration = data.total_time_s.max(1e-9);
    let painter = ui.painter().clone();
    painter.rect_filled(rect, CornerRadius::same(5), theme::canvas());
    painter.rect_stroke(
        rect,
        CornerRadius::same(5),
        Stroke::new(1.0, theme::border()),
        StrokeKind::Inside,
    );

    let state = data.state_at(preview.time_s);
    let current_task = state.task;
    let inner = rect.shrink2(vec2(1.0, 1.0));
    for (index, task) in data.tasks.iter().enumerate() {
        let Some((start, end)) = data.task_time_range(index) else {
            continue;
        };
        let x0 = inner.left() + (start / duration) as f32 * inner.width();
        let x1 = inner.left() + (end / duration) as f32 * inner.width();
        if x1 - x0 < 0.2 {
            continue;
        }
        let color = task_color(&task.kind);
        let alpha = if current_task == Some(index) { 145 } else { 62 };
        let block = egui::Rect::from_min_max(
            pos2(x0, inner.top() + 1.0),
            pos2(x1 - 1.0, inner.bottom() - 1.0),
        );
        painter.rect_filled(
            block,
            CornerRadius::same(3),
            Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha),
        );
        if x1 - x0 > 26.0 {
            painter.text(
                pos2(x0 + 4.0, block.top() + 2.0),
                Align2::LEFT_TOP,
                format!("{}", index + 1),
                FontId::monospace(8.5),
                theme::text_muted(),
            );
        }
    }

    let play_x = inner.left() + preview.progress() * inner.width();
    if play_x < inner.right() - 0.5 {
        painter.rect_filled(
            egui::Rect::from_min_max(pos2(play_x, inner.top()), inner.right_bottom()),
            CornerRadius::same(3),
            Color32::from_rgba_unmultiplied(
                theme::app_bg().r(),
                theme::app_bg().g(),
                theme::app_bg().b(),
                150,
            ),
        );
    }
    painter.line_segment(
        [
            pos2(play_x, inner.top() - 1.0),
            pos2(play_x, inner.bottom() + 1.0),
        ],
        Stroke::new(2.0, theme::copper()),
    );
    painter.circle_filled(pos2(play_x, inner.center().y), 4.0, theme::copper_bright());

    if response.hovered()
        && let Some(pointer) = ui.input(|input| input.pointer.hover_pos())
    {
        let fraction = ((pointer.x - inner.left()) / inner.width()).clamp(0.0, 1.0) as f64;
        let hover_x = inner.left() + fraction as f32 * inner.width();
        painter.line_segment(
            [pos2(hover_x, inner.top()), pos2(hover_x, inner.bottom())],
            Stroke::new(1.0, with_alpha(theme::text(), 90)),
        );
        painter.text(
            pos2(
                hover_x.clamp(inner.left() + 26.0, inner.right() - 26.0),
                inner.top() - 2.0,
            ),
            Align2::CENTER_BOTTOM,
            format_seconds(fraction * duration),
            FontId::monospace(9.0),
            theme::text_muted(),
        );
    }

    if (response.clicked() || response.dragged())
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let fraction = ((pointer.x - inner.left()) / inner.width()).clamp(0.0, 1.0);
        preview.seek_fraction(fraction);
    }
}

fn legend_dot(ui: &mut Ui, label: &str, color: Color32) {
    let (dot, _) = ui.allocate_exact_size(vec2(7.0, 7.0), Sense::hover());
    ui.painter().circle_filled(dot.center(), 3.0, color);
    ui.label(RichText::new(label).size(9.0).color(theme::text_muted()));
}

fn format_speed(speed: f32) -> String {
    if (speed - speed.round()).abs() < f32::EPSILON {
        format!("{}×", speed.round() as i64)
    } else {
        format!("{speed:.2}×")
    }
}

fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}
