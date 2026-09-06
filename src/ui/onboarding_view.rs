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

use crate::app::{secondary_text, text_tertiary, AppState, View};
use crate::ui::controls;

/// What the trial expansion produces. Espanso's own wording, kept deliberately: someone who later
/// reads espanso's documentation should meet the same example.
pub const PROBE_TRIGGER: &str = ":espanso";
pub const PROBE_REPLACE: &str = "Hi there!";

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let t = state.t();
    let is_light = !ui.visuals().dark_mode;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(4.0);
            ui.heading(t.onboarding_title);
            ui.add_space(6.0);
            ui.label(t.onboarding_intro);
            ui.add_space(20.0);
            ui.separator();

            // --- 1. see it work ----------------------------------------------------------------
            section_title(ui, t.onboarding_try_title);
            ui.label(t.onboarding_try_body);
            ui.add_space(8.0);
            ui.add(
                controls::text_field(&mut state.onboarding_probe)
                    .desired_width(280.0)
                    .hint_text(t.onboarding_try_placeholder),
            );
            // Espanso replaces the trigger wherever the cursor happens to be, including in this
            // field — which is the whole point: nothing here is simulated. The moment the
            // replacement text turns up, it worked.
            if state.onboarding_probe.contains(PROBE_REPLACE) {
                state.onboarding_expanded = true;
            }
            if state.onboarding_expanded {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(t.onboarding_try_ok)
                        .color(crate::app::accent(ui.visuals())),
                );
            }
            ui.add_space(18.0);
            ui.separator();

            // --- 2. the one decision -----------------------------------------------------------
            section_title(ui, t.onboarding_autostart_title);
            let mut autostart = state.autostart_enabled;
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label(t.autostart_checkbox);
                ui.add_space(8.0);
                changed = controls::toggle(ui, &mut autostart).changed();
            });
            if changed {
                state.set_autostart(autostart);
            }
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(t.onboarding_autostart_note)
                    .small()
                    .color(text_tertiary(is_light)),
            );
            ui.add_space(18.0);
            ui.separator();

            // --- 3. where it lives -------------------------------------------------------------
            section_title(ui, t.onboarding_where_title);
            ui.label(t.onboarding_where_body);
            ui.add_space(12.0);
            crate::ui::tips_view::tray_illustration(ui);
            ui.add_space(20.0);
            ui.separator();
            ui.add_space(16.0);

            if controls::primary_button(ui, t.onboarding_start).clicked() {
                state.finish_onboarding();
                state.view = View::List;
            }
            ui.add_space(16.0);
        });
}

fn section_title(ui: &mut egui::Ui, text: &str) {
    let is_light = !ui.visuals().dark_mode;
    ui.add_space(14.0);
    ui.label(
        egui::RichText::new(text)
            .strong()
            .color(secondary_text(is_light)),
    );
    ui.add_space(6.0);
}
