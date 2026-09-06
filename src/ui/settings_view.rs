//! The Ajustes screen: prefix, language, theme, start-with-Windows, import and export.
//!
//! Everything on it is a call into `AppState`, which is what saves and reports; nothing is written
//! from here. `show` is also where `ensure_espanso_version` is asked, because the espanso version
//! is shown on this screen and nowhere else — asking for it at start-up cost a blocking call
//! before the first frame for a line most people never look at.

use crate::app::{secondary_text, text_tertiary, AppState, View};
use crate::i18n::Lang;
use crate::settings::PREFIX_SUGGESTIONS;
use crate::theme::ThemeMode;
use crate::ui::controls;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let t = state.t();

    ui.horizontal(|ui| {
        if ui.button(format!("⬅ {}", t.back)).clicked() {
            state.view = View::List;
        }
        ui.add_space(2.0);
        ui.heading(t.settings_title);
    });
    ui.add_space(6.0);

    // Everything below scrolls, so shrinking the window never hides a setting.
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| show_sections(ui, state));
}

/// A section heading in the same register as a folder name in the list: set in the secondary colour
/// and carried by weight, not by a box around what follows.
///
/// Settings used to be a stack of bordered cards — the same nested-boxes problem the list had, and
/// the reason this screen looked busier than it is. The border is gone; the heading and the space
/// under it do the grouping, and a hairline closes each section off.
fn section(ui: &mut egui::Ui, title: &str, contents: impl FnOnce(&mut egui::Ui)) {
    let is_light = !ui.visuals().dark_mode;
    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(title)
            .strong()
            .color(secondary_text(is_light)),
    );
    ui.add_space(8.0);
    // No indent. Windows aligns a settings group flush with the heading above it; stepping the
    // controls in put a stagger on every left edge and bought nothing the heading did not already
    // say.
    contents(ui);
    ui.add_space(16.0);
    ui.separator();
}

/// Explanatory text under a control. Tertiary and in the caption size — the one place a smaller
/// size is right, because a hint supports the setting above it rather than naming anything itself.
fn hint(ui: &mut egui::Ui, text: &str) {
    let is_light = !ui.visuals().dark_mode;
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(text)
            .small()
            .color(text_tertiary(is_light)),
    );
}

fn show_sections(ui: &mut egui::Ui, state: &mut AppState) {
    let t = state.t();

    section(ui, t.autostart_section, |ui| {
        let mut autostart = state.autostart_enabled;
        // A switch rather than a checkbox: this turns a behaviour on, it does not tick an item off
        // a list, and Windows draws the two differently for exactly that reason.
        let mut changed = false;
        // The switch sits right after the words it belongs to. Pushed to the far edge it was
        // technically where Windows puts one - except Windows puts it at the edge of a card, and
        // with the cards gone it was just a switch adrift at the window border, a long way from
        // anything explaining it.
        ui.horizontal(|ui| {
            ui.label(t.autostart_checkbox);
            ui.add_space(8.0);
            changed = controls::toggle(ui, &mut autostart).changed();
        });
        if changed {
            state.set_autostart(autostart);
        }
        hint(ui, t.autostart_hint);
    });

    section(ui, t.appearance_section, |ui| {
        let labels = [t.theme_system, t.theme_light, t.theme_dark];
        let selected = ThemeMode::ALL
            .iter()
            .position(|m| *m == state.settings.theme_mode);
        if let Some(picked) = controls::segmented(ui, "theme", &labels, selected) {
            state.set_theme_mode(ThemeMode::ALL[picked]);
        }
        hint(ui, t.theme_hint);
    });

    section(ui, t.language_section, |ui| {
        // Each option is written in its own language, so someone who cannot read the current
        // interface language can still find theirs — falling back to a Latin spelling for any
        // script the loaded fonts can't draw yet.
        let labels: Vec<&str> = Lang::ALL
            .iter()
            .map(|lang| lang.picker_label(ui.ctx()))
            .collect();
        let selected = Lang::ALL.iter().position(|l| *l == state.settings.lang);
        if let Some(picked) = controls::segmented(ui, "language", &labels, selected) {
            state.set_language(Lang::ALL[picked]);
        }
        hint(ui, t.language_hint);
    });

    section(ui, t.transfer_section, |ui| {
        // Wrapped so the two buttons stack instead of running off the edge on a narrow window.
        ui.horizontal_wrapped(|ui| {
            if ui.button(t.export_button).clicked() {
                state.export_expansions();
            }
            if ui.button(t.import_button).clicked() {
                state.import_expansions();
            }
        });
        hint(ui, t.transfer_hint);
    });

    section(ui, t.prefix_section, |ui| {
        // No segment is lit when the prefix was typed by hand and matches none of these — which is
        // the honest answer, rather than lighting whichever one happens to be closest.
        let labels: Vec<String> = PREFIX_SUGGESTIONS
            .iter()
            .map(|p| format!("\"{p}\""))
            .collect();
        let selected = PREFIX_SUGGESTIONS
            .iter()
            .position(|p| *p == state.settings.prefix);
        if let Some(picked) = controls::segmented(ui, "prefix", &labels, selected) {
            state.set_prefix(PREFIX_SUGGESTIONS[picked].to_string());
        }
        hint(ui, t.prefix_hint);

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(t.custom_label);
            // The field keeps its own buffer while it is being typed in, instead of being re-cloned
            // from the committed prefix on every frame.
            //
            // `set_prefix` refuses an empty value — rightly, since every trigger needs one — and
            // the old field then redrew itself from the prefix that was still stored, while egui's
            // cursor stayed at the start of what the user had just cleared. Backspacing ":" away
            // and typing ";" put the new character *in front of* the old one: the prefix became
            // ";:", saved without a word, and every expansion created afterwards took it.
            //
            // Nothing is committed until it is usable, so the empty moment in the middle of an edit
            // now stays on screen, which is what it always looked like it was doing.
            let buffer_id = egui::Id::new("settings_custom_prefix");
            let mut custom = ui
                .data(|d| d.get_temp::<String>(buffer_id))
                .unwrap_or_else(|| state.settings.prefix.clone());
            let response = ui.add(controls::text_field(&mut custom).desired_width(90.0));
            if response.changed() {
                ui.data_mut(|d| d.insert_temp(buffer_id, custom.clone()));
                if !custom.trim().is_empty() {
                    // Still one save per keystroke, and deliberately so: a prefix is one to three
                    // characters typed once in a blue moon, and deferring the commit to `lost_focus`
                    // would silently drop the edit for anyone who closes this screen with Esc.
                    state.set_prefix(custom);
                }
            }
            // Dropped as soon as the field is done with, so it goes back to following the stored
            // prefix — including when that was changed from the row of suggestions just above.
            if response.lost_focus() {
                ui.data_mut(|d| d.remove::<String>(buffer_id));
            }
        });

        ui.add_space(10.0);
        if ui.button(t.apply_prefix).clicked() {
            state.apply_prefix_to_existing();
        }
        hint(ui, t.apply_prefix_hint);
    });

    ui.add_space(12.0);
    let is_light = !ui.visuals().dark_mode;
    let quiet = text_tertiary(is_light);
    ui.label(
        egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
            .small()
            .color(quiet),
    );
    // Espanso's version, stated and left alone. No comparison against what this build was tested
    // with, and no nudge to update: in an office the right thing is usually to stay on a version
    // that works, and a badge implying otherwise would manufacture a worry nobody needs.
    if let Some(version) = &state.espanso_version {
        ui.label(
            egui::RichText::new(format!("Espanso {version}"))
                .small()
                .color(quiet),
        );
    }
    ui.label(egui::RichText::new(t.made_by).small().color(quiet));
    ui.label(egui::RichText::new(t.credits_espanso).small().color(quiet));
    ui.add_space(10.0);
}
