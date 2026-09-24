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
//! edge, and the three cards scroll in what is left. On a short window the cards scroll; the button
//! never does, and never ends up half cut off by the frame.

use crate::app::{AppState, View};
use crate::ui::controls::{self, Tone};
use crate::ui::studio::text;

/// What the trial expansion produces. Espanso's own wording, kept deliberately: someone who later
/// reads espanso's documentation should meet the same example.
pub const PROBE_TRIGGER: &str = ":espanso";
pub const PROBE_REPLACE: &str = "Hi there!";

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let t = state.t();

    let mut starting = false;
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        starting = controls::primary_button(ui, t.onboarding_start).clicked();
        ui.add_space(controls::GAP_WIDE);
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            controls::page_scroll(ui, "onboarding", |ui| {
                    // A cap, not a width: `set_max_width` takes the number as given, so on a
                    // window narrower than the cap the cards would run past the edge.
                    ui.set_max_width(ui.available_width().min(720.0));
                    cards(ui, state);
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

    controls::tag_frame(is_light).show(ui, |ui| {
        ui.label(controls::small_muted(t.onboarding_title, is_light));
    });
    ui.add_space(controls::GAP_WIDE);
    let headline = text(
        state,
        "Tus palabras, en un atajo.",
        "Your words, in a shortcut.",
        "Ang mga salita mo, sa isang shortcut.",
        "आपके शब्द, एक शॉर्टकट में।",
    );
    ui.add(egui::Label::new(controls::h2(headline)).wrap());
    ui.add_space(controls::GAP_TIGHT);
    ui.add(egui::Label::new(controls::muted(t.onboarding_intro, is_light)).wrap());
    ui.add_space(controls::GAP_STACK);

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

/// One step of the welcome, in the same card every other screen uses.
fn card(ui: &mut egui::Ui, title: &str, contents: impl FnOnce(&mut egui::Ui)) {
    let is_light = !ui.visuals().dark_mode;
    controls::section_frame(is_light).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.add(egui::Label::new(controls::h3(title)).wrap());
        ui.add_space(controls::GAP_ROW);
        contents(ui);
    });
    ui.add_space(controls::GAP_SECTION);
}
