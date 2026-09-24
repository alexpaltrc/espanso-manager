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

//! Everything that can be done *to* a folder rather than inside it: rename it, change the prefix
//! of its triggers, export it, delete it.
//!
//! A screen and not a permanent panel. These are four things a person does to a folder perhaps
//! twice in its life; a side panel showing them at all times spends a fifth of the window on them
//! for ever, and the library is what the window is *for*. Reached from a folder chip, one step
//! back — and the step back restores the filter, so the user lands where they came from.
//!
//! ## What this screen owes the user
//!
//! **The prefix change is shown before it happens.** Every trigger that would move is listed as
//! `antes → después` with a count, and the Apply button is dead while any of them would collide
//! with an expansion that already exists. The plan is recomputed on every keystroke rather than on
//! Apply, which is the whole point: a conflict has to be visible *while* typing the prefix that
//! causes it.
//!
//! **Deleting a folder deletes its expansions**, and that is said in the same breath as the button
//! that does it, not discovered in the confirmation afterwards. The semantics are not changed here
//! — see [`crate::app::AppState::request_delete_folder`] — only made impossible to miss.
//!
//! Nothing here writes to disk. Every action is a call into `AppState`.

use crate::app::{self, AppState, View};
use crate::ui::controls::{self, Tone};
use crate::ui::studio;

/// A prefix the user is trying out, together with what it would do.
///
/// The plan travels with the value instead of being recomputed while drawing, so the list of
/// changes and the error below it always describe the text that is actually in the field.
#[derive(Clone)]
struct PrefixDraft {
    value: String,
    plan: Result<Vec<crate::folders::Rename>, crate::folders::PrefixError>,
}

fn draft_key(folder: &str) -> egui::Id {
    egui::Id::new("folder-prefix-draft").with(folder)
}

/// How many `antes → después` lines are shown before the rest are summed up.
const RENAMES_SHOWN: usize = 6;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let View::FolderOptions(name) = &state.view else {
        return;
    };
    let folder = name.clone();
    let is_light = !ui.visuals().dark_mode;
    let t = state.t();

    // Counts and prefixes are copied out before anything else happens: the cache handle borrows
    // nothing from `state`, but `info` borrows from the cache, and every action below needs
    // `&mut state`.
    let list = state.list();
    let known = list.folder_names.iter().any(|f| f == &folder);
    // A folder that has been declared but holds nothing is absent from `folders::summarize`, which
    // only ever sees folders through the expansions filed in them. That is not a missing folder,
    // it is an empty one, and this screen has to be able to rename and delete it.
    let (count, prefixes): (usize, Vec<String>) = match list.folders.get(&folder) {
        Some(info) => (info.count, info.prefixes.iter().cloned().collect()),
        None => (0, Vec::new()),
    };
    drop(list);

    // The folder was renamed or deleted out from under this screen. Nothing to offer.
    if !known {
        state.view = View::List;
        return;
    }

    // The name field is live the moment the screen opens — this is the screen for renaming, so
    // asking the user to press "rename" first would be a click that buys nothing.
    if state
        .renaming_folder
        .as_ref()
        .is_none_or(|(old, _)| old != &folder)
    {
        state.start_rename_folder(&folder);
    }

    let mut go_back = false;

    controls::page_scroll(ui, ("folder-options", &folder), |ui| {
            // No chevron in front of it. A glyph here would have to be one the icon family really
            // draws on every machine, and the word alone has never been ambiguous.
            if controls::button(ui, t.back, Tone::Quiet, true).clicked() {
                go_back = true;
            }
            ui.add_space(controls::GAP_WIDE);

            ui.add(egui::Label::new(controls::h2(&folder)).wrap());
            ui.add_space(controls::GAP_TIGHT);
            ui.label(controls::muted(
                crate::i18n::fill(
                    studio::plural(
                        count,
                        studio::text(
                            state,
                            "{n} expansión en esta carpeta",
                            "{n} expansion in this folder",
                            "{n} expansion sa folder na ito",
                            "इस फ़ोल्डर में {n} विस्तार",
                        ),
                        studio::text(
                            state,
                            "{n} expansiones en esta carpeta",
                            "{n} expansions in this folder",
                            "{n} expansion sa folder na ito",
                            "इस फ़ोल्डर में {n} विस्तार",
                        ),
                    ),
                    &[("n", &count.to_string())],
                ),
                is_light,
            ));
            ui.add_space(controls::GAP_STACK);

            name_section(ui, state, is_light);
            ui.add_space(controls::GAP_SECTION);
            prefix_section(ui, state, &folder, &prefixes, is_light);
            ui.add_space(controls::GAP_SECTION);
            export_section(ui, state, &folder, count, is_light);
            ui.add_space(controls::GAP_SECTION);
            delete_section(ui, state, &folder, count, is_light);
            ui.add_space(controls::GAP_STACK);
        });

    if go_back {
        state.cancel_rename_folder();
        studio::set_filter(ui.ctx(), state, Some(&folder));
    }
}

// ----------------------------------------------------------------------------------------------
// Name
// ----------------------------------------------------------------------------------------------

fn name_section(ui: &mut egui::Ui, state: &mut AppState, is_light: bool) {
    let t = state.t();
    controls::section_frame(is_light).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(controls::h3(studio::text(
            state,
            "Nombre de la carpeta",
            "Folder name",
            "Pangalan ng folder",
            "फ़ोल्डर का नाम",
        )));
        ui.add_space(controls::GAP);
        ui.label(controls::muted(
            studio::text(
                state,
                "Cambiar el nombre no toca los atajos ni el texto de sus expansiones.",
                "Renaming changes neither the shortcuts nor the text of its expansions.",
                "Ang pagpapalit ng pangalan ay hindi nakakaapekto sa mga shortcut o teksto.",
                "नाम बदलने से शॉर्टकट या टेक्स्ट नहीं बदलते।",
            ),
            is_light,
        ));
        ui.add_space(controls::GAP_ROW);

        let Some((old, draft)) = state.renaming_folder.as_mut() else {
            return;
        };
        let old = old.clone();
        let field = ui.add(
            controls::text_field(draft)
                .id(egui::Id::new("folder-rename-field"))
                .desired_width(ui.available_width()),
        );
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

        let changed = state
            .renaming_folder
            .as_ref()
            .is_some_and(|(_, new)| !new.trim().is_empty() && new.trim() != old);

        ui.add_space(controls::GAP_ROW);
        ui.horizontal_wrapped(|ui| {
            if controls::button(ui, t.save, Tone::Normal, changed).clicked() || (enter && changed) {
                state.commit_rename_folder();
            }
            if controls::button(ui, t.cancel, Tone::Quiet, changed).clicked() {
                if let Some((old, new)) = state.renaming_folder.as_mut() {
                    *new = old.clone();
                }
            }
        });
    });
}

// ----------------------------------------------------------------------------------------------
// Prefix
// ----------------------------------------------------------------------------------------------

fn prefix_section(
    ui: &mut egui::Ui,
    state: &mut AppState,
    folder: &str,
    prefixes: &[String],
    is_light: bool,
) {
    let t = state.t();
    let none_label = studio::text(state, "Sin prefijo", "No prefix", "Walang prefix", "कोई उपसर्ग नहीं");

    controls::section_frame(is_light).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(controls::h3(studio::text(
            state,
            "Prefijo de sus atajos",
            "Prefix of its shortcuts",
            "Prefix ng mga shortcut nito",
            "इसके शॉर्टकट का उपसर्ग",
        )));
        ui.add_space(controls::GAP);

        if prefixes.is_empty() {
            ui.label(controls::muted(
                studio::text(
                    state,
                    "Esta carpeta todavía no tiene expansiones, así que no hay prefijo que cambiar.",
                    "This folder has no expansions yet, so there is no prefix to change.",
                    "Wala pang expansion ang folder na ito, kaya walang prefix na mapapalitan.",
                    "इस फ़ोल्डर में अभी कोई विस्तार नहीं है, इसलिए बदलने को उपसर्ग नहीं है।",
                ),
                is_light,
            ));
            return;
        }

        ui.horizontal_wrapped(|ui| {
            ui.label(controls::small_muted(
                studio::text(state, "Ahora", "Currently", "Ngayon", "अभी"),
                is_light,
            ));
            for p in prefixes {
                controls::tag_frame(is_light).show(ui, |ui| {
                    let shown = if p.is_empty() { none_label } else { p.as_str() };
                    ui.label(controls::code(shown, ui.visuals()));
                });
            }
        });
        if prefixes.len() > 1 {
            ui.add_space(controls::GAP_TIGHT);
            ui.label(controls::small_muted(
                studio::text(
                    state,
                    "Esta carpeta usa varios prefijos. Al aplicar uno nuevo, todos pasarán a usarlo.",
                    "This folder uses several prefixes. Applying a new one moves them all onto it.",
                    "Iba't ibang prefix ang ginagamit dito. Ang bagong prefix ay gagamitin ng lahat.",
                    "यह फ़ोल्डर कई उपसर्ग उपयोग करता है। नया लागू करने पर सभी वही अपनाएँगे।",
                ),
                is_light,
            ));
        }
        ui.add_space(controls::GAP_STACK);

        let key = draft_key(folder);
        let mut draft = ui.ctx().data(|d| d.get_temp::<PrefixDraft>(key)).unwrap_or_else(|| {
            let value = prefixes
                .first()
                .cloned()
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| state.settings.prefix.clone());
            let plan = crate::folders::plan(&state.match_file.entries, &state.settings, folder, &value);
            PrefixDraft { value, plan }
        });

        ui.label(controls::field_label(studio::text(
            state,
            "Nuevo prefijo",
            "New prefix",
            "Bagong prefix",
            "नया उपसर्ग",
        )));
        ui.add_space(controls::GAP_TIGHT);
        let field = ui.add(
            controls::text_field(&mut draft.value)
                .id(egui::Id::new(("folder-prefix-field", folder)))
                .desired_width(ui.available_width().min(260.0)),
        );
        if field.changed() {
            draft.plan =
                crate::folders::plan(&state.match_file.entries, &state.settings, folder, &draft.value);
        }
        ui.ctx().data_mut(|d| d.insert_temp(key, draft.clone()));

        ui.add_space(controls::GAP_ROW);
        match &draft.plan {
            Ok(changes) if changes.is_empty() => {
                ui.label(controls::muted(t.prefix_already_applied, is_light));
            }
            Ok(changes) => {
                ui.label(controls::small_muted(
                    studio::text(state, "Antes → Después", "Before → After", "Dati → Bago", "पहले → बाद"),
                    is_light,
                ));
                ui.add_space(controls::GAP_TIGHT);
                controls::inset_frame(ui).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for change in changes.iter().take(RENAMES_SHOWN) {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!("{}  →  {}", change.old, change.new))
                                    .monospace()
                                    .size(13.0),
                            )
                            .wrap(),
                        );
                    }
                    if changes.len() > RENAMES_SHOWN {
                        ui.add_space(controls::GAP_TIGHT);
                        ui.label(controls::small_muted(
                            crate::i18n::fill(
                                t.see_more,
                                &[("n", &(changes.len() - RENAMES_SHOWN).to_string())],
                            ),
                            is_light,
                        ));
                    }
                });
                ui.add_space(controls::GAP_ROW);
                ui.label(controls::small_muted(
                    studio::text(
                        state,
                        "Solo cambian los disparadores de esta carpeta. El texto se conserva.",
                        "Only this folder's triggers change. The replacement text stays the same.",
                        "Mga trigger lang ng folder na ito ang magbabago. Pareho ang teksto.",
                        "केवल इस फ़ोल्डर के ट्रिगर बदलेंगे। टेक्स्ट वही रहेगा।",
                    ),
                    is_light,
                ));
                ui.add_space(controls::GAP_ROW);
                let label = format!("{}  ·  {}", t.apply_prefix, changes.len());
                if controls::button(ui, &label, Tone::Primary, true).clicked() {
                    let value = draft.value.clone();
                    state.apply_folder_prefix(folder, &value);
                    ui.ctx().data_mut(|d| d.remove::<PrefixDraft>(key));
                }
            }
            Err(error) => {
                // Present, visible and dead: the button has to stay on screen or the explanation
                // for why nothing happens leaves with it.
                controls::notice(ui, &studio::prefix_error(state, error), Tone::Danger);
                ui.add_space(controls::GAP_ROW);
                controls::button(ui, t.apply_prefix, Tone::Primary, false);
            }
        }
    });
}

// ----------------------------------------------------------------------------------------------
// Export and delete
// ----------------------------------------------------------------------------------------------

fn export_section(
    ui: &mut egui::Ui,
    state: &mut AppState,
    folder: &str,
    count: usize,
    is_light: bool,
) {
    controls::section_frame(is_light).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(controls::h3(studio::text(
            state,
            "Exportar esta carpeta",
            "Export this folder",
            "I-export ang folder na ito",
            "इस फ़ोल्डर को निर्यात करें",
        )));
        ui.add_space(controls::GAP);
        ui.label(controls::muted(
            studio::text(
                state,
                "Guarda sus expansiones en un archivo para llevarlas a otro equipo.",
                "Saves its expansions to a file so you can take them to another machine.",
                "Ise-save ang mga expansion nito sa file para madala sa ibang makina.",
                "दूसरे कंप्यूटर पर ले जाने के लिए इसके विस्तार एक फ़ाइल में सहेजता है।",
            ),
            is_light,
        ));
        ui.add_space(controls::GAP_ROW);
        let label = state.t().export_button;
        if controls::button(ui, label, Tone::Normal, count > 0).clicked() {
            state.export_folder(folder);
        }
    });
}

fn delete_section(
    ui: &mut egui::Ui,
    state: &mut AppState,
    folder: &str,
    count: usize,
    is_light: bool,
) {
    controls::section_frame(is_light).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(
            controls::h3(studio::text(
                state,
                "Eliminar carpeta",
                "Delete folder",
                "Tanggalin ang folder",
                "फ़ोल्डर हटाएँ",
            ))
            .color(app::danger(is_light)),
        );
        ui.add_space(controls::GAP_ROW);

        // The one sentence this screen exists to make unmissable. Said here, before the button,
        // and again in the confirmation — the semantics are not being changed, only stated.
        let warning = if count == 0 {
            studio::text(
                state,
                "Esta carpeta está vacía: no se eliminará ninguna expansión.",
                "This folder is empty: no expansion will be deleted.",
                "Walang laman ang folder na ito: walang expansion na matatanggal.",
                "यह फ़ोल्डर खाली है: कोई विस्तार नहीं हटेगा।",
            )
            .to_owned()
        } else {
            crate::i18n::fill(
                studio::plural(
                    count,
                    studio::text(
                        state,
                        "También se eliminará su {n} expansión. Si quieres conservarla, muévela antes a otra carpeta.",
                        "Its {n} expansion will be deleted too. To keep it, move it to another folder first.",
                        "Matatanggal din ang {n} expansion nito. Para mapanatili, ilipat muna sa ibang folder.",
                        "इसका {n} विस्तार भी हट जाएगा। रखने के लिए पहले किसी दूसरे फ़ोल्डर में ले जाएँ।",
                    ),
                    studio::text(
                        state,
                        "También se eliminarán sus {n} expansiones. Si quieres conservarlas, muévelas antes a otra carpeta.",
                        "Its {n} expansions will be deleted too. To keep them, move them to another folder first.",
                        "Matatanggal din ang {n} expansion nito. Para mapanatili, ilipat muna sa ibang folder.",
                        "इसके {n} विस्तार भी हट जाएँगे। रखने के लिए पहले किसी दूसरे फ़ोल्डर में ले जाएँ।",
                    ),
                ),
                &[("n", &count.to_string())],
            )
        };
        controls::notice(ui, &warning, Tone::Danger);
        ui.add_space(controls::GAP_ROW);

        let label = if count == 0 {
            state.t().delete.to_owned()
        } else {
            studio::text(
                state,
                "Revisar eliminación",
                "Review deletion",
                "Suriin ang pagtanggal",
                "हटाना जाँचें",
            )
            .to_owned()
        };
        if controls::button(ui, &label, Tone::Danger, true).clicked() {
            state.request_delete_folder(folder);
        }
    });
}
