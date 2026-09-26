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


//! Ajustes: a dialog in the middle of the window, over the dimmed screen behind it. It is not a
//! screen, and it cannot be moved.
//!
//! **Five settings and no explanations.** Start with Windows, theme, language, prefix, export and
//! import, each under its name and nothing else. The sentence that used to sit under each one
//! lives in the guide now (Consejos → Ajustes), which is where someone goes when they want to
//! know. A dialog that explains every control ends up as a page, which is what this stopped being.
//!
//! Everything on it is a call into `AppState`, which is what saves and reports; nothing is written
//! from here. What `AppState` says while the dialog is open comes back as
//! [`AppState::settings_note`] and is shown inside it, because the banner is under the backdrop.
//!
//! It closes with Esc (handled in `App::logic`, after the folder picker it opens), with the cross,
//! or with a click on the dimmed window.

use crate::app::{text_tertiary, AppState, TransferDirection};
use crate::i18n::{fill_count, Lang};
use crate::settings::PREFIX_SUGGESTIONS;
use crate::theme::ThemeMode;
use crate::ui::controls::{self, Tone};
use crate::ui::glyphs::Glyph;
use crate::ui::keys;

/// The widest the dialog's contents get. Enough for the language row and the prefix row on one
/// line each; less on a small window, through [`controls::dialog_width`].
const WIDEST: f32 = 440.0;

/// Between two settings. The dialog has no headings above its groups, so this space is the only
/// thing that separates them; it is wider than anything inside a group.
const GROUP_GAP: f32 = 20.0;

pub fn show(ctx: &egui::Context, state: &mut AppState) {
    if !state.settings_open {
        // However it was closed, a half-typed prefix is not kept for next time: the field starts
        // again from the saved one.
        ctx.data_mut(|d| {
            d.remove::<String>(custom_prefix_id());
            d.insert_temp(fresh_id(), true);
        });
        return;
    }
    // Opened at the top every time. egui keeps a scroll position across frames and across runs,
    // and a dialog that opens half way down its own contents hides the first thing it offers.
    // Only the first frame finds the flag; every later one must leave the scroll to the wheel.
    let fresh = ctx.data_mut(|d| d.remove_temp::<bool>(fresh_id())).unwrap_or(false);
    let t = state.t();
    let lang = state.settings.lang;
    let mut close = false;

    let modal = controls::dialog(ctx, "settings").show(ctx, |ui| {
        ui.set_width(controls::dialog_width(ctx, WIDEST));
        // Everything under the title scrolls, so the smallest window still reaches every setting
        // and the dialog never runs past the window's edge. The title and the cross stay put.
        //
        // The room is set outright, before anything is laid out. A modal's area offers its contents
        // last frame's size, so a scroll area left to measure what is available never grows past
        // the height it opened at.
        let frame = 2.0 * f32::from(controls::PAGE_MARGIN);
        let room = (ctx.content_rect().height() - frame - 2.0 * controls::DIALOG_CLEARANCE).max(160.0);
        ui.set_max_height(room);

        ui.horizontal(|ui| {
            ui.label(controls::h3(t.settings_title));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let tip = keys::tip(t.banner_close_tip, keys::BACK, lang);
                if controls::icon_button(ui, Glyph::Close, &tip, true).clicked() {
                    close = true;
                }
            });
        });
        ui.add_space(controls::GAP_WIDE);

        // A bar that takes its own column, and only when the window is too short to need none:
        // floating over the contents, it sat on top of the start-up switch at the right edge.
        ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
        let mut scroll = egui::ScrollArea::vertical()
            .id_salt("settings")
            .max_height(room - TITLE_ROW)
            .auto_shrink([false, true]);
        if fresh {
            scroll = scroll.vertical_scroll_offset(0.0);
        }
        scroll.show(ui, |ui| {
            ui.set_width(ui.available_width());
            sections(ui, state);
        });
    });

    if close || modal.backdrop_response.clicked() {
        state.close_settings();
    }
}

/// The title row and the space under it.
const TITLE_ROW: f32 = controls::FIELD_HEIGHT + controls::GAP_WIDE;

/// The name over one setting.
fn label(ui: &mut egui::Ui, text: &str) {
    ui.label(controls::field_label(text));
    ui.add_space(controls::GAP);
}

fn sections(ui: &mut egui::Ui, state: &mut AppState) {
    let t = state.t();

    // In a dialog the switch goes to the far edge, where Windows puts it on a settings card: the
    // dialog's edge is right there, so it reads as the end of this line rather than as adrift.
    let mut autostart = state.autostart_enabled && !crate::EXPERIMENTAL;
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(t.autostart_checkbox);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(!crate::EXPERIMENTAL, |ui| {
                changed = controls::toggle(ui, &mut autostart).changed();
            });
        });
    });
    if changed {
        state.set_autostart(autostart);
    }
    ui.add_space(GROUP_GAP);

    label(ui, t.appearance_section);
    let labels = [t.theme_system, t.theme_light, t.theme_dark];
    let selected = ThemeMode::ALL
        .iter()
        .position(|m| *m == state.settings.theme_mode);
    if let Some(picked) = controls::segmented(ui, "theme", &labels, selected) {
        state.set_theme_mode(ThemeMode::ALL[picked]);
    }
    ui.add_space(GROUP_GAP);

    label(ui, t.language_section);
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
    ui.add_space(GROUP_GAP);

    label(ui, t.prefix_section);
    // Wrapped, so on a narrow window the typed-in prefix drops under the suggestions instead of
    // running off the dialog.
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(controls::GAP_WIDE, controls::GAP);
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
        custom_prefix(ui, state);
    });
    ui.add_space(controls::GAP_ROW);
    if controls::button(ui, t.apply_prefix, Tone::Normal, true).clicked() {
        state.apply_prefix_to_existing();
    }
    ui.add_space(GROUP_GAP);

    label(ui, t.transfer_section);
    // Wrapped so the two buttons stack instead of running off the edge on a narrow window.
    ui.horizontal_wrapped(|ui| {
        if controls::button(ui, t.export_button, Tone::Normal, true).clicked() {
            state.export_expansions();
        }
        if controls::button(ui, t.import_button, Tone::Normal, true).clicked() {
            state.import_expansions();
        }
    });

    // What the last action said — an export's count, an import's result, a refusal — right under
    // the buttons that caused it.
    if let Some(note) = &state.settings_note {
        ui.add_space(GROUP_GAP);
        let (closed, _) = crate::app::infobar(ui, note, t);
        if closed {
            state.settings_note = None;
        }
    }

    ui.add_space(GROUP_GAP + controls::GAP);
    let is_light = !ui.visuals().dark_mode;
    let quiet = text_tertiary(is_light);
    // Espanso's version, stated and left alone. No comparison against what this build was tested
    // with, and no nudge to update: in an office the right thing is usually to stay on a version
    // that works, and a badge implying otherwise would manufacture a worry nobody needs.
    let versions = match &state.espanso_version {
        Some(espanso) => format!("v{} · Espanso {espanso}", env!("CARGO_PKG_VERSION")),
        None => format!("v{}", env!("CARGO_PKG_VERSION")),
    };
    ui.label(egui::RichText::new(versions).small().color(quiet));
    ui.label(egui::RichText::new(t.made_by).small().color(quiet));
}

/// Set while the dialog is closed, so the frame that opens it knows it is the first.
fn fresh_id() -> egui::Id {
    egui::Id::new("settings_fresh")
}

fn custom_prefix_id() -> egui::Id {
    egui::Id::new("settings_custom_prefix")
}

/// The field for a prefix none of the suggestions offers, after its own short name.
fn custom_prefix(ui: &mut egui::Ui, state: &mut AppState) {
    let t = state.t();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = controls::GAP;
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
        let buffer_id = custom_prefix_id();
        let mut custom = ui
            .data(|d| d.get_temp::<String>(buffer_id))
            .unwrap_or_else(|| state.settings.prefix.clone());
        let response = ui.add(controls::text_field(&mut custom).desired_width(64.0));
        if response.changed() {
            ui.data_mut(|d| d.insert_temp(buffer_id, custom.clone()));
            if !custom.trim().is_empty() {
                // Still one save per keystroke, and deliberately so: a prefix is one to three
                // characters typed once in a blue moon, and deferring the commit to `lost_focus`
                // would silently drop the edit for anyone who closes this dialog with Esc.
                state.set_prefix(custom);
            }
        }
        // Dropped as soon as the field is done with, so it goes back to following the stored
        // prefix — including when that was changed from the row of suggestions just beside it.
        if response.lost_focus() {
            ui.data_mut(|d| d.remove::<String>(buffer_id));
        }
    });
}

/// How much of a folder name a line shows. Folder names come from the user and nothing stops one
/// being a whole sentence; a line does not wrap, so a name left whole would run off the dialog.
const FOLDER_NAME_MAX_CHARS: usize = 32;

/// The folder picker that stands in front of an export or an import.
///
/// Drawn from `App::ui` right after [`show`], not from inside it: it has to be the modal on top of
/// Ajustes, and the document it is holding has to outlive the frame the button was pressed in.
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
