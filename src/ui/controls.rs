//! The handful of controls used on more than one screen, shaped after the ones Windows draws itself.
//!
//! ## The segmented control
//!
//! Wherever a setting has a few choices and they all fit on one line, Windows 11 draws them as one
//! outlined track with the chosen option lit — not as loose radio buttons, and not as free-floating
//! toggles. This app was doing that job in three different shapes: painted icon segments for the
//! list density, `selectable_label`s for the theme and the language, and radio buttons for
//! text-or-date. They behave identically, so they now look identical too; the density switch keeps
//! its pictograms but shares this file's background painter, which is what guarantees the two stay
//! in step rather than merely starting out similar.
//!
//! ## Why the text field is a helper and not `add_sized`
//!
//! A `TextEdit` is as tall as its text plus its margin — about 22 points — while the buttons beside
//! it are 30, so the obvious fix was to hand `add_sized` the height we wanted. That stretches the
//! *frame* and leaves the text exactly where it was: at the top, with all the extra height dumped
//! underneath. It is why the trigger boxes looked like the words in them had floated upward.
//!
//! Growing the margin instead makes the field genuinely taller, the way a Windows field is, and the
//! text stays in the middle because the middle is where it was all along.

use crate::app::{
    accent, hover_tint, line_strong, mix, readable_on, secondary_text, selection_tint,
    win_background_for, win_control_for,
};

/// Height of a text field and of one segment, before the container's own 2-point margin. Matches
/// `interact_size.y`, so a field, a button and a segmented control all line up in the same row.
pub const FIELD_HEIGHT: f32 = 30.0;

/// A single-line field with Windows' proportions.
///
/// `vertical_align` is belt and braces: the margin alone already centres the text, but a caller who
/// wraps this in `add_sized` would otherwise reintroduce the very bug this exists to fix.
pub fn text_field(text: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text)
        .margin(egui::Margin::symmetric(10, 7))
        .vertical_align(egui::Align::Center)
}

/// A multi-line field with the same horizontal padding. The text starts at the top here, which is
/// correct — a paragraph is read from its first line, not from its middle.
pub fn multiline_field(text: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::multiline(text).margin(egui::Margin::symmetric(10, 8))
}

/// The one filled button on a screen: the thing you came to that screen to do.
pub fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let accent_color = accent(ui.visuals());
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(readable_on(accent_color)))
            .fill(accent_color)
            .stroke(egui::Stroke::NONE),
    )
}

/// Paints one segment's background: the lit fill when it is the chosen one, a hover wash when the
/// pointer is over it, nothing otherwise.
///
/// The chosen side is *tinted* rather than filled with raw accent. A segmented control is a position
/// of a switch, not the primary action on the screen; giving it the same weight as the filled button
/// would leave two things shouting at once.
pub fn segment_background(ui: &egui::Ui, rect: egui::Rect, selected: bool, hovered: bool) {
    let is_light = !ui.visuals().dark_mode;
    let accent_color = accent(ui.visuals());
    let radius = egui::CornerRadius::same(3);
    let painter = ui.painter();

    let bg = if selected {
        selection_tint(is_light, accent_color)
    } else if hovered {
        hover_tint(is_light)
    } else {
        egui::Color32::TRANSPARENT
    };
    painter.rect_filled(rect, radius, bg);

    if selected {
        painter.rect_stroke(
            rect,
            radius,
            egui::Stroke::new(
                1.0,
                mix(
                    win_background_for(is_light),
                    accent_color,
                    if is_light { 0.28 } else { 0.32 },
                ),
            ),
            egui::StrokeKind::Inside,
        );
    }
}

/// The outlined track the segments sit in.
pub fn segment_track(is_light: bool) -> egui::Frame {
    egui::Frame::default()
        .fill(win_control_for(is_light))
        .stroke(egui::Stroke::new(1.0, line_strong(is_light)))
        .corner_radius(5u8)
        .inner_margin(egui::Margin::same(2))
}

/// A row of mutually exclusive options in one track. Returns the index the user just picked, or
/// `None` if they picked nothing or re-picked what was already chosen.
///
/// `selected` is an `Option` because a value can legitimately be none of the offered ones — a custom
/// prefix typed by hand, for instance. In that case no segment is lit, which says so honestly rather
/// than lighting whichever one happens to be first.
pub fn segmented<S: AsRef<str>>(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    labels: &[S],
    selected: Option<usize>,
) -> Option<usize> {
    if labels.is_empty() {
        return None;
    }

    const HEIGHT: f32 = FIELD_HEIGHT - 4.0;
    const GAP: f32 = 2.0;
    const PADDING: f32 = 12.0;

    let is_light = !ui.visuals().dark_mode;
    let font = egui::TextStyle::Button.resolve(ui.style());
    let galleys: Vec<_> = labels
        .iter()
        .map(|label| {
            ui.painter().layout_no_wrap(
                label.as_ref().to_owned(),
                font.clone(),
                egui::Color32::PLACEHOLDER,
            )
        })
        .collect();

    let mut widths: Vec<f32> = galleys
        .iter()
        .map(|g| (g.size().x + PADDING * 2.0).max(HEIGHT))
        .collect();
    let gaps = GAP * (labels.len() - 1) as f32;
    let natural: f32 = widths.iter().sum::<f32>() + gaps;

    // On a narrow window the track shrinks rather than running off the edge. Each segment then
    // clips its own text instead of writing over its neighbour.
    let room = ui.available_width() - 8.0;
    if natural > room && room > gaps {
        let factor = (room - gaps) / (natural - gaps);
        for w in &mut widths {
            *w *= factor;
        }
    }
    let total: f32 = widths.iter().sum::<f32>() + gaps;

    let mut picked = None;
    // Stable ids, so two segmented controls on one screen can never collide and steal each
    // other's clicks when the layout around them shifts.
    ui.push_id(id_salt, |ui| {
    segment_track(is_light).show(ui, |ui| {
        // An explicit allocation, not `ui.horizontal`: that claims the parent's whole width before
        // laying anything out, which would stretch the track across the window.
        ui.allocate_ui_with_layout(
            egui::vec2(total, HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                for (i, (galley, width)) in galleys.into_iter().zip(widths).enumerate() {
                    let is_selected = selected == Some(i);
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(width, HEIGHT),
                        egui::Sense::click(),
                    );

                    if ui.is_rect_visible(rect) {
                        segment_background(ui, rect, is_selected, response.hovered());
                        let ink = if is_selected {
                            ui.visuals().text_color()
                        } else {
                            secondary_text(is_light)
                        };
                        let at = rect.center() - galley.size() * 0.5;
                        ui.painter()
                            .with_clip_rect(rect.intersect(ui.clip_rect()))
                            .galley(at, galley, ink);
                    }

                    if response.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if response.clicked() && !is_selected {
                        picked = Some(i);
                    }
                }
            },
        );
    });
    });

    picked
}


/// A Windows 11 toggle switch.
///
/// A checkbox and a switch say slightly different things: a checkbox marks an item in a list, a
/// switch turns a thing on. "Start with Windows" is the second kind, and it is the only setting in
/// this app that is, so it gets the control Windows itself uses for it.
///
/// The travel is animated through egui's own `animate_bool_with_time`, the same easing already
/// driving the selection bar — one animation system for the whole app, and it costs a repaint only
/// while the knob is actually moving.
pub fn toggle(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let size = egui::vec2(40.0, 20.0);
    let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    if ui.is_rect_visible(rect) {
        let is_light = !ui.visuals().dark_mode;
        let accent_color = accent(ui.visuals());
        let t = ui.ctx().animate_bool_with_time(response.id, *on, 0.12);

        let off_fill = win_control_for(is_light);
        let fill = mix(off_fill, accent_color, t);
        let stroke_color = mix(line_strong(is_light), accent_color, t);
        let radius = egui::CornerRadius::same((rect.height() * 0.5) as u8);

        let painter = ui.painter();
        painter.rect_filled(rect, radius, fill);
        painter.rect_stroke(
            rect,
            radius,
            egui::Stroke::new(1.0, stroke_color),
            egui::StrokeKind::Inside,
        );

        // Off, the knob is the ink colour on the control surface; on, it is whatever reads against
        // the accent — which is how it stays visible on a pale accent as well as a dark one.
        let knob_x = egui::lerp((rect.left() + 10.0)..=(rect.right() - 10.0), t);
        let knob_r = egui::lerp(5.0..=6.0, t);
        let knob_color = if t > 0.5 {
            readable_on(accent_color)
        } else {
            secondary_text(is_light)
        };
        painter.circle_filled(egui::pos2(knob_x, rect.center().y), knob_r, knob_color);
    }

    response
}
