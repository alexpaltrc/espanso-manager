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

//! The first-run screen.
//!
//! Espanso ships a three-window wizard of its own — a welcome page, a start-with-Windows page, and
//! a "here is your tray icon" page. It is fine, and it is also three windows in a different visual
//! language than everything around it, shown to someone who has not yet done anything. All three
//! pages together carry exactly two decisions and one demonstration, so that is what this is: one
//! screen, in this app's own design, that never appears again.
//!
//! Espanso's own wizard is suppressed rather than raced — see `main.rs`, which writes the flags it
//! looks for before the daemon ever starts.

//!
//! **The one way forward is always on screen.** «Empezar» is laid out first, against the bottom
//! edge, and the three parts scroll in what is left. On a short window they scroll; the button
//! never does, and never ends up half cut off by the frame.

use crate::app::{AppState, View};
use crate::ui::controls::{self, Tone};

/// What the trial expansion produces. Espanso's own wording, kept deliberately: someone who later
/// reads espanso's documentation should meet the same example.
pub const PROBE_TRIGGER: &str = ":espanso";
pub const PROBE_REPLACE: &str = "Hi there!";

/// The welcome is read, not filled in: past this width the lines get too long to follow.
const COLUMN_MAX_WIDTH: f32 = 720.0;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let t = state.t();

    // One column, centred: the title, the text and «Empezar» all keep to it, so on a wide window
    // the screen is a page with margins rather than three lines hugging the left edge.
    let full = ui.available_width();
    let width = full.min(COLUMN_MAX_WIDTH);
    let margin = ((full - width) * 0.5).max(0.0);

    let mut starting = false;
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(margin);
            starting = controls::primary_button(ui, t.onboarding_start).clicked();
        });
        ui.add_space(controls::GAP_WIDE);
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            controls::page_scroll(ui, "onboarding", |ui| {
                // Measured again inside: the scroll bar takes its share of the width.
                let full = ui.available_width();
                let width = full.min(COLUMN_MAX_WIDTH);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space(((full - width) * 0.5).max(0.0));
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        cards(ui, state);
                    });
                });
            });
        });
    });
    if starting {
        state.finish_onboarding();
        state.view = View::List;
    }
}

fn cards(ui: &mut egui::Ui, state: &mut AppState) {
    let t = state.t();
    let is_light = !ui.visuals().dark_mode;

    // The title stands where every other screen has its own, centred on the first row; the one
    // line under it says what the app is for, and nothing more is said before the three parts.
    ui.vertical_centered(|ui| {
        ui.add(egui::Label::new(controls::h1(t.onboarding_title)).wrap());
    });
    ui.add_space(controls::GAP_STACK);
    ui.add(egui::Label::new(controls::muted(t.onboarding_intro, is_light)).wrap());
    ui.add_space(PART_GAP);

    // --- 1. see it work --------------------------------------------------------------------------
    card(ui, t.onboarding_try_title, |ui| {
        ui.add(egui::Label::new(t.onboarding_try_body).wrap());
        ui.add_space(controls::GAP_ROW);
        let width = ui.available_width().min(320.0);
        ui.add(
            controls::text_field(&mut state.onboarding_probe)
                .desired_width(width)
                .hint_text(t.onboarding_try_placeholder),
        );
        // Espanso replaces the trigger wherever the cursor happens to be, including in this
        // field — which is the whole point: nothing here is simulated. The moment the
        // replacement text turns up, it worked.
        if state.onboarding_probe.contains(PROBE_REPLACE) {
            state.onboarding_expanded = true;
        }
        if state.onboarding_expanded {
            ui.add_space(controls::GAP_ROW);
            controls::notice(ui, t.onboarding_try_ok, Tone::Primary);
        }
    });

    // --- 2. the one decision ---------------------------------------------------------------------
    card(ui, t.onboarding_autostart_title, |ui| {
        let mut autostart = state.autostart_enabled;
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(t.autostart_checkbox);
            ui.add_space(controls::GAP);
            changed = controls::toggle(ui, &mut autostart).changed();
        });
        if changed {
            state.set_autostart(autostart);
        }
        ui.add_space(controls::GAP_TIGHT);
        let is_light = !ui.visuals().dark_mode;
        ui.add(egui::Label::new(controls::small_muted(t.onboarding_autostart_note, is_light)).wrap());
    });

    // --- 3. where it lives -----------------------------------------------------------------------
    card(ui, t.onboarding_where_title, |ui| {
        ui.add(egui::Label::new(t.onboarding_where_body).wrap());
        ui.add_space(controls::GAP_WIDE);
        crate::ui::tips_view::tray_illustration(ui);
    });
}

/// Space between the three parts: the same pause the editor, the settings and the folder page use.
const PART_GAP: f32 = 28.0;

/// One part of the welcome: a heading and what belongs under it, with no card around them.
fn card(ui: &mut egui::Ui, title: &str, contents: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.add(egui::Label::new(controls::h3(title)).wrap());
        ui.add_space(controls::GAP_ROW);
        contents(ui);
    });
    ui.add_space(PART_GAP);
}
