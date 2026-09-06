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

//! The one form: creating an expansion and editing one, which are the same screen with a different
//! title and a different verb.
//!
//! `EditState` is the whole form — every field the user can touch — and it lives in
//! `View::Edit`, so a half-written expansion survives switching screens and coming back.
//! `build_match` turns it into the `SimpleMatch` that gets saved, and `new_for_edit` turns a stored
//! one back into a form; those two have to stay each other's inverse, or reopening an expansion
//! shows something the user did not write.
//!
//! The trigger is split into prefix and body for editing and rejoined on save, so changing the
//! global prefix is a change to one setting and not to every trigger in the file.
//!
//! The date half of the form is a preset picker over [`crate::yaml::presets`] plus, when none of
//! them fits, the block builder in [`crate::ui::date_blocks`]. The month-language control appears
//! wherever a month is actually spelled out, presets and custom alike, because the version that
//! sat at the foot of the form read as missing.
//!
//! `shadowing_pair` warns about one relation only — one trigger being a prefix of another — and
//! deliberately does not warn about the pair the prefix box itself produces. Read its own comment
//! before adding a case to it; espanso's matcher makes most of the pairs that look dangerous safe.

use crate::app::{AppState, View};
use crate::datefmt;
use crate::i18n::{fill, Lang, Strings};
use crate::yaml::model::{SimpleMatch, VarEntry};
use crate::ui::controls;
use crate::yaml::presets::{build_custom_date_var, month_lang_of, DatePreset};

const DATE_VAR_NAME: &str = "fecha";
/// Reserves room below the big text area for the label/radios above and the button row below,
/// so the textarea fills whatever's left of the window instead of a fixed small box.
const TEXT_AREA_RESERVED_HEIGHT: f32 = 230.0;
const TEXT_AREA_MIN_HEIGHT: f32 = 160.0;
/// Width of the prefix box. Wide enough for the longest suggestion ("//") with the field padding
/// around it, and no wider — the prefix is one or two symbols, not a sentence.
const PREFIX_FIELD_WIDTH: f32 = 72.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Text {
        replace: String,
    },
    Date {
        preset: Option<DatePreset>,
        custom_format: String,
        custom_tz: String,
        /// Which language a written-out month or weekday is spelled in. English by default, to
        /// match the interface's own default, and stored in the expansion itself so it reads the
        /// same on every machine it's shared with.
        month_lang: Lang,
    },
}

/// Splits a stored trigger into its symbol prefix and the word after it.
///
/// The prefix is the leading run of non-alphanumeric characters, so `"::hola"` becomes
/// `("::", "hola")`. Keeping the two apart is what makes "apply this prefix everywhere" predictable:
/// the prefix is *replaced* rather than something new being stapled on to whatever was already
/// there, no matter whether the old one was one symbol or three.
pub fn split_trigger(trigger: &str) -> (String, String) {
    let word_start = trigger
        .char_indices()
        .find(|(_, c)| c.is_alphanumeric())
        .map(|(i, _)| i)
        .unwrap_or(trigger.len());
    (
        trigger[..word_start].to_string(),
        trigger[word_start..].to_string(),
    )
}

/// How a worked example of a date is drawn: monospaced so it reads as literal output rather than as
/// prose, and muted so it supports the option it sits beside instead of competing with it.
fn sample_text(sample: &str) -> egui::RichText {
    egui::RichText::new(sample).monospace().weak()
}

#[derive(Debug, Clone)]
pub struct EditState {
    /// `None` while creating a brand new expansion.
    pub editing_index: Option<usize>,
    /// Held apart from the word so the two can be edited independently; recombined on save.
    pub prefix: String,
    pub word: String,
    pub kind: Kind,
    /// The half of the Texto/Fecha switch that is not on screen, kept so that crossing the switch
    /// cannot destroy work. Until Guardar is pressed, the replacement text — or a date format built
    /// block by block — exists nowhere else, and one mis-click used to be enough to lose it with no
    /// way back. Scratch state only: `build_match` never reads it, so the YAML written is the same
    /// as it always was.
    pub stashed_kind: Option<Kind>,
    /// Optional folder this expansion belongs to. Entirely our own bookkeeping, never written
    /// into espanso's YAML.
    pub folder: Option<String>,
    pub choosing_new_folder: bool,
    pub new_folder_input: String,
}

impl EditState {
    pub fn new_for_create(default_prefix: &str) -> Self {
        Self {
            editing_index: None,
            prefix: default_prefix.to_string(),
            word: String::new(),
            kind: Kind::Text {
                replace: String::new(),
            },
            stashed_kind: None,
            folder: None,
            choosing_new_folder: false,
            new_folder_input: String::new(),
        }
    }

    pub fn new_for_edit(index: usize, m: &SimpleMatch, folder: Option<String>) -> Self {
        let kind = if let Some(var) = m.as_date_var() {
            Kind::Date {
                preset: DatePreset::detect(var),
                custom_format: var.param_str("format").unwrap_or("").to_string(),
                custom_tz: var.param_str("tz").unwrap_or("").to_string(),
                month_lang: month_lang_of(var),
            }
        } else {
            Kind::Text {
                replace: m.replace.clone(),
            }
        };
        let (prefix, word) = split_trigger(&m.trigger);
        Self {
            editing_index: Some(index),
            prefix,
            word,
            kind,
            stashed_kind: None,
            folder,
            choosing_new_folder: false,
            new_folder_input: String::new(),
        }
    }

    /// Resolves the effective folder, taking the "escribiendo una carpeta nueva" text box into
    /// account if that's the mode the user was in when they hit Guardar.
    pub fn resolved_folder(&self) -> Option<String> {
        if self.choosing_new_folder {
            let name = self.new_folder_input.trim();
            if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            }
        } else {
            self.folder.clone()
        }
    }

    /// The trigger as espanso will see it: the prefix and the word joined back together.
    ///
    /// The word is stripped of any leading symbols first, by the same rule that splits a stored
    /// trigger into its two halves. Without that the two boxes fought each other: typing `:hola`
    /// into a box labelled "palabra", next to a prefix box already showing `:`, produced `::hola` —
    /// a trigger nobody asked for, and one that then had to be hunted down in Settings. The prefix
    /// box owns the prefix; anything typed in the word box is a word.
    ///
    /// Returns empty when the word has no letters or digits at all, so a prefix on its own is
    /// caught by the same check that catches an empty trigger rather than being saved as one.
    pub fn trigger(&self) -> String {
        let (_, word) = split_trigger(self.word.trim());
        if word.is_empty() {
            return String::new();
        }
        format!("{}{}", self.prefix.trim(), word)
    }

    pub fn build_match(&self) -> SimpleMatch {
        match &self.kind {
            Kind::Text { replace } => SimpleMatch {
                trigger: self.trigger(),
                replace: replace.clone(),
                vars: Vec::new(),
                label: None,
            },
            Kind::Date {
                preset,
                custom_format,
                custom_tz,
                month_lang,
            } => {
                let var: VarEntry = match preset {
                    Some(p) => p.build_var(DATE_VAR_NAME, *month_lang),
                    None => build_custom_date_var(
                        DATE_VAR_NAME,
                        custom_format,
                        Some(custom_tz.as_str()),
                        *month_lang,
                    ),
                };
                SimpleMatch {
                    trigger: self.trigger(),
                    replace: format!("{{{{{DATE_VAR_NAME}}}}}"),
                    vars: vec![var],
                    label: None,
                }
            }
        }
    }
}

/// Where the worked examples start, measured from the left edge of the option list.
///
/// The five options and their samples read as two columns, the way the trigger and its preview do
/// in the main list: every example begins on the same invisible line, so the eye compares four
/// dates instead of hunting for each one at a different indent.
///
/// This is not a constant, because the labels are translated — "Fecha y hora en UTC (ISO 8601)" is
/// a different width in each of the four languages, and a number picked for Spanish would either
/// crowd Hindi or waste half the row in English. It is the widest label plus the fixed chrome a
/// radio button puts around its text, and that chrome is *learned from a real one* (see
/// [`date_option`]) rather than reconstructed from egui's internal spacing, so a future change to
/// that spacing cannot quietly break the alignment.
fn option_column(ui: &egui::Ui, labels: &[&str]) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let widest = labels
        .iter()
        .map(|label| {
            ui.painter()
                .layout_no_wrap((*label).to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    let chrome = ui
        .ctx()
        .data(|d| d.get_temp::<f32>(radio_chrome_id()))
        .unwrap_or(48.0);
    widest + chrome + SAMPLE_GUTTER
}

/// The clear space between the longest option and where the examples begin.
///
/// Wider than the ordinary gap between two widgets, and deliberately so: this is the seam between
/// two columns, not the join between two things on one line. At a single gap the longest option and
/// its example touched, and the whole point of the alignment — four results read as a list — was
/// lost on the row that needed it most.
const SAMPLE_GUTTER: f32 = 30.0;

fn radio_chrome_id() -> egui::Id {
    egui::Id::new("radio_chrome")
}

/// One row of the date picker: the option, then the date it would produce right now, the latter
/// pushed out to `column` so all of them line up.
///
/// `sample` is optional because "Personalizado" has nothing to show until it is selected and has
/// blocks in it.
///
/// Returns `true` on the frame the option is clicked.
fn date_option(
    ui: &mut egui::Ui,
    selected: bool,
    label: &str,
    sample: Option<String>,
    column: f32,
) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        let placed = ui.radio(selected, label);
        clicked = placed.clicked();

        // What a radio button adds around its own text — the circle, the gap, the padding. Taken
        // from one that has actually been laid out, and remembered, so the next frame can size the
        // column before drawing anything.
        //
        // Measured once and then left alone. It is the same number for every row and every frame —
        // it comes from the style, not the text — so re-deriving it meant laying every label out a
        // second time, five extra text layouts per frame, to arrive at the number already stored.
        if ui.ctx().data(|d| d.get_temp::<f32>(radio_chrome_id())).is_none() {
            let font = egui::TextStyle::Button.resolve(ui.style());
            let text_width = ui
                .painter()
                .layout_no_wrap(label.to_owned(), font, egui::Color32::PLACEHOLDER)
                .size()
                .x;
            let chrome = placed.rect.width() - text_width;
            if chrome > 0.0 {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(radio_chrome_id(), chrome));
            }
        }

        if let Some(sample) = sample {
            let padding = column - placed.rect.width() - ui.spacing().item_spacing.x;
            if padding > 0.0 {
                ui.add_space(padding);
            }
            ui.label(sample_text(&sample));
        }
    });
    clicked
}

/// The pair of triggers that cannot both work, if this one makes such a pair with an existing
/// expansion — shortest first.
///
/// Espanso fires a match the moment what you have typed ends with its trigger; it does not wait to
/// see whether you were going to type something longer, because it has no way of knowing when you
/// have stopped. So between `:test` and `:testing`, only `:test` can ever fire: by the fifth
/// character it has already replaced itself, and the `ing` lands in whatever it left behind.
///
/// This matters in both directions, which is why the answer is a pair rather than a yes. Adding
/// `:testing` next to an existing `:test` gives you an expansion that never works; adding `:test`
/// next to an existing `:testing` silently breaks the one you already had. Naming the short one and
/// the long one covers both of those directions without needing two different warnings.
///
/// Only the *prefix* relation is a conflict, and deliberately so. `:hola` and `::hola` look like
/// they collide — typing the long one does end with the short one — but espanso's rolling matcher
/// keeps one live path per position and returns on the first that completes, checking the paths
/// that started earliest first; the longest trigger is therefore always the one that wins, and both
/// expansions keep working. Warning about that pair would put a warning on exactly what the prefix
/// box and "apply this prefix everywhere" produce, which is the one thing here that is not a
/// mistake.
fn shadowing_pair(
    entries: &[crate::yaml::model::MatchEntry],
    editing: Option<usize>,
    trigger: &str,
) -> Option<(String, String)> {
    if trigger.is_empty() {
        return None;
    }
    entries.iter().enumerate().find_map(|(i, entry)| {
        if Some(i) == editing {
            return None;
        }
        let other = entry.trigger_str();
        // An exact match is a plain duplicate, and saving already refuses that one by name.
        if other.is_empty() || other == trigger {
            return None;
        }
        if trigger.starts_with(other) {
            Some((other.to_string(), trigger.to_string()))
        } else if other.starts_with(trigger) {
            Some((trigger.to_string(), other.to_string()))
        } else {
            None
        }
    })
}

/// The month a format spells out in words has to be spelled out in *some* language, and this is
/// where that is chosen.
///
/// It appears in two places — under the presets that write a month out, and inside the block
/// builder as soon as a "Mes (nombre)" block is in the row — so it lives in one function. The
/// alternative, one control at the foot of the form serving both, is what it used to be: for a
/// custom format it ended up below two palettes, nowhere near the block that needed it, and read
/// as missing entirely.
pub fn month_language(ui: &mut egui::Ui, t: &'static Strings, month_lang: &mut Lang) {
    field_label(ui, t.month_language_label);
    // Each language written in its own script, exactly as in the interface's own language picker
    // in Settings — and in the same control, so the two are recognisably the same question asked
    // twice rather than two different-looking widgets.
    let labels: Vec<&str> = Lang::ALL
        .iter()
        .map(|lang| lang.picker_label(ui.ctx()))
        .collect();
    let selected = Lang::ALL.iter().position(|l| l == month_lang);
    if let Some(picked) = controls::segmented(ui, "month_lang", &labels, selected) {
        *month_lang = Lang::ALL[picked];
    }
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(t.month_language_hint)
            .small()
            .color(crate::app::text_tertiary(!ui.visuals().dark_mode)),
    );
}

/// The caption above a control. Small and in the secondary colour, matching the section headings in
/// Settings and the folder names in the list — one register for "this names what is below it", used
/// everywhere, so the eye learns it once.
fn field_label(ui: &mut egui::Ui, text: &str) {
    let is_light = !ui.visuals().dark_mode;
    ui.label(
        egui::RichText::new(text)
            .strong()
            .color(crate::app::secondary_text(is_light)),
    );
    ui.add_space(4.0);
}

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(edit) = (match &state.view {
        View::Edit(e) => Some(e.clone()),
        _ => None,
    }) else {
        return;
    };

    // Everything below sits in a scroll area: in a short window the form must stay reachable
    // rather than quietly running off the bottom edge.
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
    show_form(ui, state, edit);
    });
}

fn show_form(ui: &mut egui::Ui, state: &mut AppState, mut edit: EditState) {
    let t = state.t();
    let is_new = edit.editing_index.is_none();
    ui.heading(if is_new {
        t.edit_title_new
    } else {
        t.edit_title_existing
    });
    ui.add_space(10.0);

    field_label(ui, t.when_you_type);
    // Prefix and word are separate boxes so it is obvious which part is which — and so that
    // "apply this prefix everywhere" has an unambiguous piece to replace.
    //
    // Sized by their own margin rather than by `add_sized`. Handing `add_sized` a taller box only
    // stretches the frame: the text stays pinned to the top with the extra height below it, which
    // is exactly why the words in these two boxes looked like they had floated upward.
    ui.horizontal(|ui| {
        ui.add(
            controls::text_field(&mut edit.prefix)
                .desired_width(PREFIX_FIELD_WIDTH)
                .hint_text(t.prefix_field_hint),
        );
        ui.add(
            controls::text_field(&mut edit.word)
                .desired_width(ui.available_width().max(80.0))
                .hint_text(t.word_field_hint),
        );
    });
    ui.add_space(4.0);
    let is_light = !ui.visuals().dark_mode;
    ui.label(
        egui::RichText::new(fill(t.trigger_preview, &[("trigger", &edit.trigger())]))
            .small()
            .color(crate::app::text_tertiary(is_light)),
    );

    // Said here, while it is still being typed, rather than at save time. By the time you press
    // Guardar you have already decided; the useful moment to learn that a trigger cannot work is
    // the moment you are choosing it. It does not block saving — you may be about to delete the
    // other one, and the app has no business guessing.
    if let Some((short, long)) =
        shadowing_pair(&state.match_file.entries, edit.editing_index, &edit.trigger())
    {
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(fill(
                t.trigger_shadow,
                &[("short", &short), ("long", &long)],
            ))
            .small()
            .color(crate::app::caution(is_light)),
        );
    }

    ui.add_space(16.0);

    field_label(ui, t.will_be_replaced_by);
    let was_date = matches!(edit.kind, Kind::Date { .. });
    let mut is_date_mode = was_date;
    // Two positions of one switch, so drawn as one. Loose radio buttons were the last place in the
    // app still asking "which of these two?" in a different shape from everywhere else.
    let kinds = [t.kind_text, t.kind_date];
    if let Some(picked) = controls::segmented(ui, "kind", &kinds, Some(is_date_mode as usize)) {
        is_date_mode = picked == 1;
    }

    // Crossing the switch puts the half being left behind into `stashed_kind` and brings back
    // whatever was waiting there. It used to throw the old half away and start the new one from
    // its defaults, which meant a mis-click on a two-position control emptied the replacement box
    // of an expansion being edited, and clicking straight back gave an empty box — the text was
    // gone, and pressing Guardar wrote that emptiness over the stored version.
    //
    // Coming to the switch for the first time still starts from the defaults, so nothing looks any
    // different until you cross back.
    if is_date_mode != was_date {
        let fresh = if is_date_mode {
            Kind::Date {
                preset: Some(DatePreset::IsoDateOnly),
                custom_format: DatePreset::IsoDateOnly.format().to_string(),
                custom_tz: String::new(),
                month_lang: Lang::En,
            }
        } else {
            Kind::Text {
                replace: String::new(),
            }
        };
        let restored = match edit.stashed_kind.take() {
            Some(kind) if matches!(&kind, Kind::Date { .. }) == is_date_mode => kind,
            _ => fresh,
        };
        edit.stashed_kind = Some(std::mem::replace(&mut edit.kind, restored));
    }

    ui.add_space(10.0);
    match &mut edit.kind {
        Kind::Text { replace } => {
            let height = (ui.available_height() - TEXT_AREA_RESERVED_HEIGHT).max(TEXT_AREA_MIN_HEIGHT);
            ui.add_sized(
                egui::vec2(ui.available_width(), height),
                controls::multiline_field(replace),
            );
        }
        Kind::Date {
            preset,
            custom_format,
            custom_tz,
            month_lang,
        } => {
            // Every option carries the date it would produce *right now*, right next to it. That is
            // the whole explanation most people need: you pick the one that looks like what you
            // want, instead of decoding "%B %-d, %Y" to find out.
            let mut labels: Vec<&str> = DatePreset::ALL.iter().map(|p| p.label(t)).collect();
            labels.push(t.preset_custom);
            let column = option_column(ui, &labels);

            for p in DatePreset::ALL {
                if date_option(
                    ui,
                    *preset == Some(p),
                    p.label(t),
                    Some(p.sample(*month_lang)),
                    column,
                ) {
                    *preset = Some(p);
                    *custom_format = p.format().to_string();
                    *custom_tz = p.tz().unwrap_or("").to_string();
                }
            }

            {
                let sample = preset.is_none().then(|| {
                    let tz = (!custom_tz.trim().is_empty()).then(|| custom_tz.trim());
                    datefmt::sample(custom_format, tz, *month_lang)
                });
                if date_option(ui, preset.is_none(), t.preset_custom, sample, column)
                    && preset.is_some()
                {
                    // Arriving at "Personalizado" starts from nothing.
                    //
                    // It used to inherit whatever preset was selected a moment ago, which meant the
                    // block builder only appeared if that preset happened to be one the blocks can
                    // express — coming from the ISO date it showed up, coming from any of the other
                    // three it silently did not. Starting empty removes that entirely, and it is
                    // the better first impression anyway: an empty row that says where to drop
                    // things explains itself, a row pre-filled with somebody else's format does not.
                    *preset = None;
                    custom_format.clear();
                    custom_tz.clear();
                }
            }
            if preset.is_none() {
                ui.indent("custom_date", |ui| {
                    // The builder is the whole of it now. The strftime field, the time-zone box and
                    // the "Ejemplo: %Y-%m-%d" line that used to sit underneath are gone: they were
                    // the old way of doing this, kept as a safety net, and leaving them there just
                    // asked people to learn the notation the blocks exist to replace. The time zone
                    // is no longer typed either — it follows from whether an "Hora (UTC)" block is
                    // in the row.
                    // Only formats the blocks can carry *back out* go to the builder. The blocks
                    // hold one time zone between them — UTC or the machine's own — and when they
                    // are rebuilt they write the whole `tz` field from that. So a match carrying
                    // `tz: Europe/Madrid` handed to the builder would come back with its time zone
                    // deleted the first time any block was touched: the same silent loss the
                    // advanced field below exists to prevent, which is where such a format goes.
                    //
                    // The comparison ignores case for the same reason the preview does: espanso
                    // accepts `tz: utc`, and the two must not disagree about what it means.
                    let tz = custom_tz.trim();
                    let tz_is_utc = tz.eq_ignore_ascii_case("UTC");
                    let expressible = tz.is_empty() || tz_is_utc;
                    match expressible
                        .then(|| crate::ui::date_blocks::parse(custom_format, tz_is_utc))
                        .flatten()
                    {
                        Some(blocks) => {
                            if let Some(rebuilt) =
                                crate::ui::date_blocks::show(ui, t, &blocks, month_lang)
                            {
                                *custom_format = crate::ui::date_blocks::to_format(&rebuilt);
                                *custom_tz = crate::ui::date_blocks::timezone_for(&rebuilt)
                                    .unwrap_or("")
                                    .to_string();
                            }
                        }
                        None => {
                            // An imported or hand-edited format the blocks cannot express. Shown
                            // rather than silently rewritten, since rewriting it would lose part of
                            // what the author meant.
                            ui.label(
                                egui::RichText::new(t.blocks_advanced)
                                    .color(crate::app::secondary_text(!ui.visuals().dark_mode)),
                            );
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.label(t.format_label);
                                ui.add(controls::text_field(custom_format).desired_width(220.0));
                            });
                            // The third place this question has to be asked, and the one it was
                            // missing from. A format the blocks cannot express is the *only* kind
                            // that can carry `%A`, `%b` or `%h`, so this branch is where a written
                            // month or weekday is most likely to turn up — and saving pins a
                            // `locale:` for it either way. Without the picker an imported
                            // `%A %-d %B %Y` that named no language quietly became English, with
                            // nothing on screen to say so or to say otherwise.
                            if datefmt::needs_locale(custom_format) {
                                ui.add_space(14.0);
                                month_language(ui, t, month_lang);
                            }
                        }
                    }
                });
            }

            // Under the presets, the same question the block builder asks about its own row: a
            // format that writes the month in words has to say which words.
            if (*preset).is_some_and(|p| datefmt::needs_locale(p.format())) {
                ui.add_space(14.0);
                month_language(ui, t, month_lang);
            }
        }
    }

    ui.add_space(16.0);
    // The label already says "(optional)", so no second reminder underneath.
    field_label(ui, t.folder_optional);
    let folder_names = state.settings.all_folder_names();
    let selected_text = if edit.choosing_new_folder {
        if edit.new_folder_input.trim().is_empty() {
            t.new_folder_placeholder.to_string()
        } else {
            edit.new_folder_input.clone()
        }
    } else {
        edit.folder
            .clone()
            .unwrap_or_else(|| t.no_folder.to_string())
    };
    egui::ComboBox::from_id_salt("edit_folder_combo")
        .selected_text(selected_text)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(
                    edit.folder.is_none() && !edit.choosing_new_folder,
                    t.no_folder,
                )
                .clicked()
            {
                edit.folder = None;
                edit.choosing_new_folder = false;
            }
            for name in &folder_names {
                let selected =
                    !edit.choosing_new_folder && edit.folder.as_deref() == Some(name.as_str());
                if ui.selectable_label(selected, name).clicked() {
                    edit.folder = Some(name.clone());
                    edit.choosing_new_folder = false;
                }
            }
            if ui
                .selectable_label(edit.choosing_new_folder, t.add_new_folder)
                .clicked()
            {
                edit.choosing_new_folder = true;
            }
        });
    if edit.choosing_new_folder {
        ui.horizontal(|ui| {
            ui.label(t.name_label);
            ui.add(controls::text_field(&mut edit.new_folder_input).desired_width(200.0));
        });
    }

    let mut keep_editing = true;

    ui.add_space(18.0);
    ui.separator();
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        // Saving is what this screen is for, so it gets the one filled button — the same weight
        // "Nueva expansiÃ³n" carries in the list.
        if controls::primary_button(ui, t.save).clicked() {
            state.save_edit(&edit);
            keep_editing = false;
        }
        if ui.button(t.cancel).clicked() {
            state.view = View::List;
            keep_editing = false;
        }
        if !is_new {
            // Destructive, so it sits at the far edge rather than beside the two buttons it must
            // never be hit instead of.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let danger = crate::app::danger(!ui.visuals().dark_mode);
                if ui
                    .button(egui::RichText::new(t.delete).color(danger))
                    .clicked()
                {
                    if let Some(index) = edit.editing_index {
                        state.request_delete(index);
                        keep_editing = false;
                    }
                }
            });
        }
    });
    ui.add_space(6.0);

    if keep_editing {
        state.view = View::Edit(edit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yaml::model::{MatchEntry, SimpleMatch};

    fn stored(triggers: &[&str]) -> Vec<MatchEntry> {
        triggers
            .iter()
            .map(|t| {
                MatchEntry::Simple(SimpleMatch {
                    trigger: (*t).to_string(),
                    replace: "x".to_string(),
                    vars: Vec::new(),
                    label: None,
                })
            })
            .collect()
    }

    #[test]
    fn the_short_trigger_is_named_first_whichever_one_is_being_added() {
        let existing = stored(&[":test"]);
        assert_eq!(
            shadowing_pair(&existing, None, ":testing"),
            Some((":test".to_string(), ":testing".to_string()))
        );
        let existing = stored(&[":testing"]);
        assert_eq!(
            shadowing_pair(&existing, None, ":test"),
            Some((":test".to_string(), ":testing".to_string()))
        );
    }

    #[test]
    fn the_same_word_under_a_different_prefix_is_not_a_warning() {
        // espanso's matcher completes the longest live path first, so `::hola` wins over `:hola`
        // and both keep working. This is what the prefix box produces on purpose; warning about it
        // would be warning about the feature.
        assert_eq!(shadowing_pair(&stored(&[":hola"]), None, "::hola"), None);
        assert_eq!(shadowing_pair(&stored(&["::hola"]), None, ":hola"), None);
        assert_eq!(shadowing_pair(&stored(&[":hola"]), None, "x:hola"), None);
    }

    #[test]
    fn an_exact_duplicate_is_left_to_the_check_that_names_it() {
        assert_eq!(shadowing_pair(&stored(&[":test"]), None, ":test"), None);
    }

    #[test]
    fn the_row_being_edited_never_shadows_itself() {
        let existing = stored(&[":test", ":other"]);
        assert_eq!(shadowing_pair(&existing, Some(0), ":test"), None);
    }
}
