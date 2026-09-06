/*
 * This file is part of EspansoManager.
 *
 * Copyright (C) 2026 Alex Palacios
 *
 * EspansoManager is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * EspansoManager is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with EspansoManager.  If not, see <https://www.gnu.org/licenses/>.
 */

//! The tips window: the five things people ask about, answered where they can find them.
//!
//! Everything here used to live somewhere else, or nowhere. The Ctrl/Shift hint was a line of text
//! pinned under the selection bar, where it repeated itself every time you selected a row and was
//! read exactly once; the rest was in nobody's head but ours. A tray app has no menu bar and no
//! help file, and the answers are short — so they get one quiet room, reached from a small button
//! and never shown unasked.
//!
//! Laid out in the same register as Settings: a heading in the secondary colour, the answer under
//! it in ordinary text, a hairline between. Nothing is boxed.

use crate::app::{secondary_text, text_tertiary, AppState, View};

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let t = state.t();
    let shortcut = state.search_shortcut;

    ui.horizontal(|ui| {
        if ui.button(format!("⬅ {}", t.back)).clicked() {
            state.view = View::List;
        }
        ui.add_space(2.0);
        ui.heading(t.tips_title);
    });
    ui.add_space(6.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let is_light = !ui.visuals().dark_mode;
            let state_shortcut = shortcut;

            tip(ui, t.tip_where_title, t.tip_where_body, None);
            tip(ui, t.tip_pin_title, t.tip_pin_body, Some(tray_illustration));
            tip(ui, t.tip_undo_title, t.tip_undo_body, None);
            tip(ui, t.tip_pause_title, t.tip_pause_body, None);
            tip(ui, t.tip_select_title, t.tip_select_body, None);
            // The only tip whose text depends on the machine: which shortcut was free.
            let search_body = crate::i18n::fill(
                t.tip_search_body,
                &[("keys", state_shortcut)],
            );
            tip(ui, t.tip_search_title, &search_body, None);

            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(t.tips_footer)
                    .small()
                    .color(text_tertiary(is_light)),
            );
            ui.add_space(10.0);
        });
}

/// One tip: its question, its answer, and occasionally a picture of the thing being described.
fn tip(
    ui: &mut egui::Ui,
    title: &str,
    body: &str,
    illustration: Option<fn(&mut egui::Ui)>,
) {
    let is_light = !ui.visuals().dark_mode;
    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(title)
            .strong()
            .color(secondary_text(is_light)),
    );
    ui.add_space(6.0);
    ui.label(body);
    if let Some(draw) = illustration {
        ui.add_space(10.0);
        draw(ui);
    }
    ui.add_space(16.0);
    ui.separator();
}

/// A drawing of the notification area: the overflow flyout above, the taskbar below, and the icon
/// being dragged from one to the other — the gesture the tip is asking for.
///
/// Painted rather than shipped as an image. A screenshot would be wrong the moment Windows changes
/// its taskbar, wrong on a different accent colour, wrong in the other theme, and blurry on a scaled
/// display. Forty lines of rectangles are right in all four cases and weigh nothing.
///
/// The clock is drawn too, and it earns its place: "next to the clock" is how everybody actually
/// describes where the tray is, and it is the one landmark that makes the strip unmistakably the
/// notification area rather than a generic bar. The flyout sits directly above the chevron it opens
/// from, and the destination is an empty outline — a place waiting to be filled.
pub fn tray_illustration(ui: &mut egui::Ui) {
    let is_light = !ui.visuals().dark_mode;
    let accent = crate::app::accent(ui.visuals());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(264.0, 112.0), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();

    let ink = crate::app::text_tertiary(is_light);
    let quiet = ink.gamma_multiply(0.55);
    let surface = crate::app::win_control_for(is_light);
    let edge = crate::app::line_strong(is_light);

    // --- the taskbar, along the bottom -----------------------------------------------------------
    let bar = egui::Rect::from_min_size(
        egui::pos2(rect.left(), rect.bottom() - 34.0),
        egui::vec2(rect.width(), 34.0),
    );
    painter.rect_filled(bar, egui::CornerRadius::same(5), surface);
    painter.rect_stroke(
        bar,
        egui::CornerRadius::same(5),
        egui::Stroke::new(1.0, edge),
        egui::StrokeKind::Inside,
    );

    // The clock: two stacked bars standing in for the time and the date, at the right-hand end.
    for (i, w) in [34.0_f32, 40.0].into_iter().enumerate() {
        let y = bar.center().y - 5.0 + i as f32 * 10.0;
        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(bar.right() - 12.0 - w, y), egui::vec2(w, 4.0)),
            egui::CornerRadius::same(2),
            quiet,
        );
    }

    // Two icons that are already pinned, then the empty slot ours is heading for, then the chevron.
    let chevron_x = bar.left() + 34.0;
    let landing_x = chevron_x + 34.0;
    for i in 0..2 {
        let slot = egui::Rect::from_center_size(
            egui::pos2(landing_x + 34.0 + i as f32 * 30.0, bar.center().y),
            egui::vec2(15.0, 15.0),
        );
        painter.rect_filled(slot, egui::CornerRadius::same(3), quiet);
    }
    let landing = egui::Rect::from_center_size(
        egui::pos2(landing_x, bar.center().y),
        egui::vec2(17.0, 17.0),
    );
    painter.rect_stroke(
        landing,
        egui::CornerRadius::same(3),
        egui::Stroke::new(1.5, accent),
        egui::StrokeKind::Inside,
    );

    // The chevron that opens the flyout, pointing up at it.
    let chevron = egui::pos2(chevron_x, bar.center().y + 2.0);
    painter.line_segment(
        [chevron + egui::vec2(-5.0, 2.0), chevron + egui::vec2(0.0, -3.0)],
        egui::Stroke::new(1.8, ink),
    );
    painter.line_segment(
        [chevron + egui::vec2(0.0, -3.0), chevron + egui::vec2(5.0, 2.0)],
        egui::Stroke::new(1.8, ink),
    );

    // --- the flyout, directly above the chevron it belongs to ------------------------------------
    let flyout = egui::Rect::from_min_size(
        egui::pos2(chevron_x - 20.0, rect.top()),
        egui::vec2(104.0, 36.0),
    );
    painter.rect_filled(flyout, egui::CornerRadius::same(7), surface);
    painter.rect_stroke(
        flyout,
        egui::CornerRadius::same(7),
        egui::Stroke::new(1.0, edge),
        egui::StrokeKind::Inside,
    );
    let ours_x = flyout.left() + 52.0;
    for i in 0..3 {
        let x = flyout.left() + 22.0 + i as f32 * 30.0;
        let slot = egui::Rect::from_center_size(egui::pos2(x, flyout.center().y), egui::vec2(16.0, 16.0));
        let mine = (x - ours_x).abs() < 1.0;
        painter.rect_filled(
            slot,
            egui::CornerRadius::same(3),
            if mine { accent } else { quiet },
        );
    }

    // --- the drag ---------------------------------------------------------------------------------
    let from = egui::pos2(ours_x, flyout.bottom() + 5.0);
    let to = egui::pos2(landing.center().x, landing.top() - 8.0);
    painter.line_segment([from, to], egui::Stroke::new(1.8, accent));
    let dir = (to - from).normalized();
    let side = egui::vec2(-dir.y, dir.x) * 4.0;
    painter.add(egui::Shape::convex_polygon(
        vec![to + dir * 4.0, to - dir * 3.0 + side, to - dir * 3.0 - side],
        accent,
        egui::Stroke::NONE,
    ));
}
