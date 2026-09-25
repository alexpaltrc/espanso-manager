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

//! The Ajustes screen: prefix, language, theme, start-with-Windows, import and export.
//!
//! **Four groups and no more**, in the plan's order: when Windows starts, appearance and language,
//! sharing, and the prefix. The count is the design, not an accident of what happens to be
//! configurable — an app that grows a setting for every aesthetic decision ends up asking the user
//! to design it, which is the job this screen exists to have already done.
//!
//! Everything on it is a call into `AppState`, which is what saves and reports; nothing is written
//! from here. `show` is also where `ensure_espanso_version` is asked, because the espanso version
//! is shown on this screen and nowhere else — asking for it at start-up cost a blocking call
//! before the first frame for a line most people never look at.

use crate::app::{text_tertiary, AppState, TransferDirection, View};
use crate::i18n::{fill_count, Lang};
use crate::settings::PREFIX_SUGGESTIONS;
use crate::theme::ThemeMode;
use crate::ui::controls::{self, Tone};
use crate::ui::studio;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let t = state.t();
    // Fixed above the scroll area: this screen is taller than the window, and the way back has to
    // be one step from anywhere in it. See [`controls::page_header`].
    if controls::page_header(ui, t.back, t.settings_title, state.settings.lang) {
        state.view = View::List;
    }

    // Everything below scrolls, so shrinking the window never hides a setting.
    crate::ui::controls::page_scroll(ui, "settings", |ui| show_sections(ui, state));
}

/// One group of settings: its name, then what it sets, straight on the window — the way the editor
/// lays out its three parts.
///
/// This screen has been a stack of cards, then flat, then cards again. It is flat now because every
/// screen is: with a card around each group, the frames were the loudest thing on a page whose
/// whole content is four quiet choices. The space between groups, wider than any space inside one,
/// is what still makes a group read as a group.
fn section(ui: &mut egui::Ui, title: &str, contents: impl FnOnce(&mut egui::Ui)) {
    ui.label(controls::h3(title));
    ui.add_space(controls::GAP_ROW);
    contents(ui);
    ui.add_space(GROUP_GAP);
}

/// Between two groups. The editor's gap between its parts, for the same reason.
const GROUP_GAP: f32 = 28.0;

/// The label over one control inside a group, for the two occasions a group holds more than one
/// setting and the card's own title cannot name both.
fn sub_label(ui: &mut egui::Ui, text: &str) {
    ui.label(controls::field_label(text));
    ui.add_space(controls::GAP);
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
        let mut autostart = state.autostart_enabled && !crate::EXPERIMENTAL;
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
            ui.add_enabled_ui(!crate::EXPERIMENTAL, |ui| { changed = controls::toggle(ui, &mut autostart).changed(); });
        });
        if changed {
            state.set_autostart(autostart);
        }
        hint(ui, t.autostart_hint);
    });

    // Theme and language in one group, as the plan's four groups have it. They are the same kind of
    // decision — how the window looks and reads — and splitting them put two cards on screen where
    // one says it better.
    let appearance = studio::text(
        state,
        "Apariencia e idioma",
        "Appearance and language",
        "Hitsura at wika",
        "रूप और भाषा",
    );
    section(ui, appearance, |ui| {
        sub_label(ui, t.appearance_section);
        let labels = [t.theme_system, t.theme_light, t.theme_dark];
        let selected = ThemeMode::ALL
            .iter()
            .position(|m| *m == state.settings.theme_mode);
        if let Some(picked) = controls::segmented(ui, "theme", &labels, selected) {
            state.set_theme_mode(ThemeMode::ALL[picked]);
        }
        hint(ui, t.theme_hint);

        ui.add_space(controls::GAP_STACK);
        sub_label(ui, t.language_section);
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
            if controls::button(ui, t.export_button, Tone::Normal, true).clicked() {
                state.export_expansions();
            }
            if controls::button(ui, t.import_button, Tone::Normal, true).clicked() {
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

        ui.add_space(controls::GAP_WIDE);
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

        ui.add_space(controls::GAP_WIDE);
        if controls::button(ui, t.apply_prefix, Tone::Normal, true).clicked() {
            state.apply_prefix_to_existing();
        }
        hint(ui, t.apply_prefix_hint);
    });

    ui.add_space(controls::GAP_TIGHT);
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
    // The gutter the window already has is drawn *around* the scroll area, not inside it, so the
    // last line would otherwise end flush against the bottom edge. See the note in `App::ui`.
    ui.add_space(controls::GAP_STACK);
}

/// How much of a folder name a line shows. Folder names come from the user and nothing stops one
/// being a whole sentence; a line does not wrap, so a name left whole would run off the dialog.
const FOLDER_NAME_MAX_CHARS: usize = 32;

/// The folder picker that stands in front of an export or an import.
///
/// Drawn from `App::ui` next to the delete confirmation, not from [`show`], for the same reason
/// that one is: a modal belongs to the window rather than to the panel whose button opened it, and
/// the document it is holding has to outlive the frame the button was pressed in.
///
/// One line per folder, ticked or not, in the same line the folder menu and «Mover a una carpeta»
/// use — so a folder is chosen the same way everywhere, and the check says what is included
/// without a sentence explaining how to include it. The running total underneath moves as lines
/// are ticked, which is what confirms the click did something.
pub fn show_folder_picker(ctx: &egui::Context, state: &mut AppState) {
    let t = state.settings.t();
    let Some(pending) = &mut state.pending_transfer else {
        return;
    };
    let is_export = matches!(pending.direction, TransferDirection::Export);
    let show_bulk = pending.groups.len() > 3;

    let mut confirm = false;
    let mut cancel = false;

    // `pending` stays borrowed for as long as the lines are on screen, so the two buttons only
    // record what was pressed. The transfer itself starts below, once that borrow is over.
    let modal = controls::dialog(ctx, "transfer_folders").show(ctx, |ui| {
        let is_light = !ui.visuals().dark_mode;
        ui.set_width(controls::dialog_width(ctx, 460.0));
        ui.add(
            egui::Label::new(controls::h3(if is_export {
                t.transfer_pick_export_title
            } else {
                t.transfer_pick_import_title
            }))
            .wrap(),
        );

        // Only worth offering once clicking them off one at a time is real work. With two or three
        // folders these buttons would be two more things to read for nothing. Quiet, so they read
        // as shortcuts over the list rather than as two more folders.
        if show_bulk {
            ui.add_space(controls::GAP);
            controls::quiet_row(ui, |ui| {
                if controls::button(ui, t.transfer_pick_all, Tone::Quiet, true).clicked() {
                    for group in pending.groups.iter_mut() {
                        group.selected = true;
                    }
                }
                if controls::button(ui, t.transfer_pick_none, Tone::Quiet, true).clicked() {
                    for group in pending.groups.iter_mut() {
                        group.selected = false;
                    }
                }
            });
        }

        ui.add_space(controls::GAP_WIDE);
        // Capped and scrolling, so thirty folders cannot push the buttons off the bottom of the
        // window — the one thing that would leave this modal with no way out but Esc.
        egui::ScrollArea::vertical()
            .max_height(200.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                for group in pending.groups.iter_mut() {
                    let name = group.folder.as_deref().unwrap_or(t.no_folder);
                    let name = crate::app::truncate(name, FOLDER_NAME_MAX_CHARS);
                    let count = group.count.to_string();
                    if controls::check_line(ui, &name, Some(&count), group.selected).clicked() {
                        group.selected = !group.selected;
                    }
                }
            });

        let chosen: usize = pending
            .groups
            .iter()
            .filter(|group| group.selected)
            .map(|group| group.count)
            .sum();

        ui.add_space(controls::GAP_WIDE);
        ui.add(
            egui::Label::new(controls::muted(
                fill_count(
                    if is_export {
                        t.transfer_pick_export_total
                    } else {
                        t.transfer_pick_import_total
                    },
                    chosen,
                    &[],
                ),
                is_light,
            ))
            .wrap(),
        );

        ui.add_space(controls::GAP_STACK);
        controls::dialog_buttons(ui, |ui| {
            if controls::button(ui, t.cancel, Tone::Normal, true).clicked() {
                cancel = true;
            }
            // Nothing chosen is not an error to explain afterwards: the way out is that the action
            // cannot be taken, with the total above already saying why.
            let go = if is_export {
                t.transfer_pick_export_go
            } else {
                t.transfer_pick_import_go
            };
            if controls::button(ui, go, Tone::Primary, chosen > 0).clicked() {
                confirm = true;
            }
        });
    });

    if modal.backdrop_response.clicked() {
        cancel = true;
    }
    if confirm {
        state.confirm_pending_transfer();
    } else if cancel {
        state.cancel_pending_transfer();
    }
}
