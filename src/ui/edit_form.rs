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
//!
//! ## Two numbered steps, and a third card that is not one
//!
//! The form is **1. Cuando escribes** and **2. Aparece este texto**, in that order and named with
//! those words, because they are the two halves of what an expansion is and the same two words head
//! the library's columns. The folder follows in a card of its own and is deliberately *not*
//! numbered: it changes nothing about what espanso does, and a step three would say it did.
//!
//! ## Validation is a thing the form knows, not a thing saving discovers
//!
//! [`problem_with`] is worked out once per frame, before anything is drawn, and two places read it:
//! the message beside the field that causes it, and whether Guardar can be pressed at all. That is
//! the plan's rule — offer saving only when the form is valid — and it is the only arrangement that
//! makes the rule actionable, because a button that is dead for a reason printed somewhere else is
//! one nobody can act on. [`crate::app::AppState::save_edit`] still checks for itself: it is reached
//! from Ctrl+S and from tests as well as from this button.

use crate::app::{AppState, View};
use crate::datefmt;
use crate::i18n::{fill, Lang, Strings};
use crate::yaml::model::{SimpleMatch, VarEntry};
use crate::ui::controls::{self, Tone};
use crate::ui::studio;
use crate::yaml::presets::{build_custom_date_var, month_lang_of, DatePreset};

const DATE_VAR_NAME: &str = "fecha";
/// How many lines of the replacement box are visible before it has anything in it.
///
/// It is a floor, not a ceiling: the box grows as the text does and the page scrolls. Six is about
/// an email signature — enough that it does not read as a one-line field, few enough that the
/// preview underneath is still on screen in an ordinary window.
const TEXT_AREA_ROWS: f32 = 6.0;
/// The tallest the preview of the result gets before it scrolls inside itself. Rounded down to a
/// whole number of lines where it is used, so the last line shown is a whole line.
const PREVIEW_MAX_HEIGHT: f32 = 168.0;
/// The widest the form is allowed to get, however wide the window is.
///
/// A replacement box eight hundred points across is not eight hundred points more useful: past a
/// point a line of text stops being comfortable to read, and the two boxes of step 1 start to look
/// like two unrelated things at opposite ends of the screen. Wider windows get margin, not wider
/// boxes — which is also the plan's rule about not scaling controls indiscriminately, read the
/// other way round.
const FORM_MAX_WIDTH: f32 = 880.0;
/// Diameter of the circle a step number sits in.
const BADGE: f32 = 24.0;
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
///
/// A trigger with no letter or digit anywhere in it — `":--"`, `":_"`, `":—"` — is **all word and
/// no prefix**. Nothing in it says where a prefix would end, and calling the whole thing a prefix
/// left it with no word at all: the editor then refused to save it as an empty trigger, which is
/// why `":--" → "—"` could not be created. The prefix operations know the same rule and leave such
/// triggers alone; see [`crate::folders`].
pub fn split_trigger(trigger: &str) -> (String, String) {
    match trigger.char_indices().find(|(_, c)| c.is_alphanumeric()) {
        Some((word_start, _)) => (
            trigger[..word_start].to_string(),
            trigger[word_start..].to_string(),
        ),
        None => (String::new(), trigger.to_string()),
    }
}

/// How a worked example of a date is drawn: monospaced so it reads as literal output rather than as
/// prose, and muted so it supports the option it sits beside instead of competing with it.
fn sample_text(sample: &str) -> egui::RichText {
    egui::RichText::new(sample).monospace().weak()
}

#[derive(Debug, Clone, PartialEq)]
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
    /// Returns empty only when the word box is empty, so a prefix on its own is caught by the same
    /// check that catches an empty trigger rather than being saved as one. A word box holding
    /// nothing but symbols — `--`, `_`, `—` — is a word: [`split_trigger`] hands it back whole
    /// rather than eating it as a second prefix, so `:` and `--` make `:--`.
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
    let stack_sample = sample.as_ref().is_some_and(|s| {
        let width = ui.painter().layout_no_wrap(s.clone(),egui::TextStyle::Monospace.resolve(ui.style()),egui::Color32::PLACEHOLDER).size().x;
        column + width > ui.available_width()
    });
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

        if let Some(sample) = sample.as_ref().filter(|_| !stack_sample) {
            let padding = column - placed.rect.width() - ui.spacing().item_spacing.x;
            if padding > 0.0 {
                ui.add_space(padding);
            }
            ui.label(sample_text(&sample));
        }
    });
    if let Some(sample) = sample.as_ref().filter(|_| stack_sample) {
        ui.indent(egui::Id::new(label).with("sample"),|ui| { ui.add(egui::Label::new(sample_text(sample)).wrap()); });
    }
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

// --- What is wrong, if anything ----------------------------------------------------------------

/// Why the form cannot be saved yet. Three things, and no more than three: a rule that stops
/// somebody saving has to be one they can see the point of at the moment it stops them.
enum Problem {
    /// Nothing in the word box. A step not taken rather than a mistake made, and said as one.
    Empty,
    /// A letter or a digit in the prefix box.
    ///
    /// Worth blocking rather than quietly allowing, because [`split_trigger`] calls the *leading
    /// run of symbols* the prefix: `ab` and `hola` save as `abhola` and reopen as no prefix and the
    /// word `abhola`, and applying a folder prefix to it afterwards would eat the `ab`. By
    /// construction no stored expansion can arrive in this state, so this only ever catches
    /// something typed here a moment ago.
    PrefixHasLetters,
    /// Some other expansion already answers to this trigger. Espanso keeps the first match it finds
    /// for a trigger, so the second one would never fire.
    Duplicate(String),
}

fn problem_with(state: &AppState, edit: &EditState) -> Option<Problem> {
    let trigger = edit.trigger();
    if trigger.is_empty() {
        return Some(Problem::Empty);
    }
    if edit.prefix.trim().chars().any(char::is_alphanumeric) {
        return Some(Problem::PrefixHasLetters);
    }
    // Advanced matches count too, exactly as they do in `AppState::save_edit`. They are not in the
    // list, so a trigger colliding with one would look free here and then never fire.
    let taken = state
        .match_file
        .entries
        .iter()
        .enumerate()
        .any(|(i, e)| Some(i) != edit.editing_index && e.trigger_str() == trigger);
    taken.then_some(Problem::Duplicate(trigger))
}

fn problem_text(state: &AppState, t: &'static Strings, problem: &Problem) -> String {
    match problem {
        Problem::Empty => studio::text(
            state,
            "Escribe el atajo que quieres usar.",
            "Type the shortcut you want to use.",
            "I-type ang shortcut na gusto mong gamitin.",
            "जो शॉर्टकट चाहिए वह लिखें।",
        )
        .to_string(),
        Problem::PrefixHasLetters => studio::text(
            state,
            "El prefijo solo lleva símbolos, como : o //. Las letras van en el atajo.",
            "The prefix takes symbols only, such as : or //. Letters belong in the shortcut.",
            "Simbolo lang ang prefix, gaya ng : o //. Sa shortcut napupunta ang mga letra.",
            "उपसर्ग में केवल चिह्न आते हैं, जैसे : या //। अक्षर शॉर्टकट में लिखें।",
        )
        .to_string(),
        Problem::Duplicate(trigger) => fill(t.trigger_duplicate, &[("name", trigger)]),
    }
}

// --- The screen ---------------------------------------------------------------------------------

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let View::Edit(edit) = &state.view else {
        return;
    };
    let edit = edit.clone();
    let t = state.t();
    let is_light = !ui.visuals().dark_mode;

    // Worked out before a single widget is drawn, so that the message inside the form and the state
    // of the button at the foot are one answer rather than two that agree most of the time. Both
    // describe the form as the frame found it; a keystroke arriving this frame is answered on the
    // next one, which `show_form` asks for as soon as anything changes.
    let problem = problem_with(state, &edit);
    let can_save = problem.is_none();

    let mut save = false;
    let mut cancel = false;
    let mut delete = false;

    // The actions get a strip of their own at the foot of the window, in the same place whatever
    // the form's height: a form long enough to scroll must not be able to scroll its own Guardar
    // out of reach.
    egui::Panel::bottom("editor-actions")
        .resizable(false)
        .frame(egui::Frame::default().inner_margin(egui::Margin {
            left: 0,
            right: 0,
            top: controls::GAP_SECTION as i8,
            bottom: 2,
        }))
        .show(ui, |ui| {
            // The rule sits on the panel's own top edge, which is the inner margin above where the
            // content begins — it separates the strip from the form, so it cannot be inside it.
            ui.painter().hline(
                ui.max_rect().x_range(),
                ui.max_rect().top() - controls::GAP_SECTION,
                egui::Stroke::new(1.0, crate::app::hairline(is_light)),
            );
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = controls::GAP_ROW;
                let saving = controls::button(ui, t.save, Tone::Primary, can_save);
                save = match &problem {
                    // Says why it cannot be pressed at the moment somebody tries to press it, which
                    // is a different moment from reading the form.
                    Some(problem) => saving.on_disabled_hover_text(problem_text(state, t, problem)),
                    None => saving.on_hover_text("Ctrl+S"),
                }
                .clicked();
                cancel = controls::button(ui, t.cancel, Tone::Normal, true).clicked();
                if edit.editing_index.is_some() {
                    // Across the strip from Guardar rather than beside it. It is the one action
                    // here that cannot be taken back from this screen.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        delete = controls::button(ui, t.delete, Tone::Danger, true)
                            .on_hover_text(t.delete_tip)
                            .clicked();
                    });
                }
            });
        });

    controls::page_scroll(ui, "editor-body", |ui| {
            let full = ui.available_width();
            let width = full.min(FORM_MAX_WIDTH);
            ui.horizontal(|ui| {
                // The margin is the whole point of the horizontal, so nothing else may add to it.
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space(((full - width) * 0.5).max(0.0));
                ui.vertical(|ui| {
                    ui.set_width(width);
                    show_form(ui, state, edit, problem.as_ref(), is_light);
                });
            });
        });

    // Read on every frame so the key never falls through to another screen, and acted on only when
    // the button beside it could have been pressed: the shortcut is the button, not a way past it.
    let ctrl_s = ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S));
    save |= ctrl_s && can_save;

    if let View::Edit(edit) = &state.view {
        let edit = edit.clone();
        if save {
            let trigger = edit.trigger();
            state.save_edit(&edit);
            // Only on the way out. A refused save leaves the form open with its banner, and the
            // library has nothing to point at.
            if matches!(state.view, View::List) {
                super::list_view::mark_saved(ui.ctx(), &trigger);
            }
        } else if cancel {
            state.view = View::List;
        } else if delete {
            if let Some(index) = edit.editing_index {
                state.request_delete(index);
            }
        }
    }
}

/// The heading of one of the two steps: its number in a ring, then its name.
///
/// The number is painted rather than written into the translated string. Four languages would each
/// have to remember to start with "1." and to write it the same way, and the two headings would
/// then begin at whatever x their own punctuation happened to leave them at.
fn step_heading(ui: &mut egui::Ui, number: &str, title: &str, is_light: bool) {
    let accent = crate::app::accent(ui.visuals());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = controls::GAP_ROW;
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(BADGE), egui::Sense::hover());
        let centre = rect.center();
        ui.painter()
            .circle_filled(centre, BADGE / 2.0, crate::app::selection_tint(is_light, accent));
        ui.painter()
            .circle_stroke(centre, BADGE / 2.0, egui::Stroke::new(1.0, accent));
        ui.painter().text(
            centre,
            egui::Align2::CENTER_CENTER,
            number,
            egui::FontId::proportional(13.0),
            accent,
        );
        ui.add(egui::Label::new(controls::h3(title)).wrap());
    });
}

/// "Escribirás: `:firma`" — the two boxes read back as the one thing espanso will watch for.
///
/// Drawn in pieces rather than as one filled-in sentence, so the trigger itself carries the accent
/// and the monospace: it is a literal string to type, and the words around it are prose. The
/// template is split at its own placeholder, which leaves each of the four languages its own word
/// order — and falls back to filling the template whole if a translation ever loses the placeholder.
fn trigger_line(ui: &mut egui::Ui, t: &'static Strings, trigger: &str, is_light: bool) {
    let shown = if trigger.is_empty() { "…" } else { trigger };
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = controls::GAP_TIGHT;
        match t.trigger_preview.split_once("{trigger}") {
            Some((before, after)) => {
                let (before, after) = (before.trim_end(), after.trim_start());
                if !before.is_empty() {
                    ui.label(controls::muted(before, is_light));
                }
                ui.label(controls::code(shown, ui.visuals()).strong());
                if !after.is_empty() {
                    ui.label(controls::muted(after, is_light));
                }
            }
            None => {
                ui.label(controls::muted(
                    fill(t.trigger_preview, &[("trigger", shown)]),
                    is_light,
                ));
            }
        }
    });
}

/// Step 1: the two boxes that make the trigger, what they add up to, and anything wrong with it.
fn when_you_type(
    ui: &mut egui::Ui,
    state: &AppState,
    edit: &mut EditState,
    problem: Option<&Problem>,
    is_light: bool,
) {
    let t = state.t();
    controls::section_frame(is_light).show(ui, |ui| {
        ui.set_width(ui.available_width());
        step_heading(
            ui,
            "1",
            studio::text(
                state,
                "Cuando escribes",
                "When you type",
                "Kapag tina-type mo",
                "जब आप लिखते हैं",
            ),
            is_light,
        );
        ui.add_space(controls::GAP_WIDE);

        // Prefix and word are separate boxes so it is obvious which part is which — and so that
        // "apply this prefix everywhere" has an unambiguous piece to replace.
        //
        // Sized by their own margin rather than by `add_sized`. Handing `add_sized` a taller box
        // only stretches the frame: the text stays pinned to the top with the extra height below
        // it, which is exactly why the words in these two boxes once looked like they had floated
        // upward.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = controls::GAP;
            ui.add(
                controls::text_field(&mut edit.prefix)
                    .desired_width(PREFIX_FIELD_WIDTH)
                    .hint_text(t.prefix_field_hint),
            );
            ui.add(
                controls::text_field(&mut edit.word)
                    .desired_width((ui.available_width() - controls::GAP).max(90.0))
                    .hint_text(t.word_field_hint),
            );
        });

        ui.add_space(controls::GAP_ROW);
        trigger_line(ui, t, &edit.trigger(), is_light);

        if let Some(problem) = problem {
            ui.add_space(controls::GAP_ROW);
            let message = problem_text(state, t, problem);
            match problem {
                // Not an error. It is the step nobody has taken yet, and a brand new form that
                // opens already scolding in red is telling the user off for arriving.
                Problem::Empty => {
                    ui.label(controls::muted(message, is_light));
                }
                _ => controls::notice(ui, &message, Tone::Danger),
            }
        }

        // Said here, while it is still being typed, rather than at save time. By the time you press
        // Guardar you have already decided; the useful moment to learn that a trigger cannot work is
        // the moment you are choosing it. It does not block saving — you may be about to delete the
        // other one, and the app has no business guessing.
        if let Some((short, long)) =
            shadowing_pair(&state.match_file.entries, edit.editing_index, &edit.trigger())
        {
            ui.add_space(controls::GAP_ROW);
            ui.label(
                egui::RichText::new(fill(
                    t.trigger_shadow,
                    &[("short", &short), ("long", &long)],
                ))
                .color(crate::app::caution(is_light)),
            );
        }
    });
}

/// Step 2: text or date, whichever this is, and what it will actually produce.
fn appears_here(ui: &mut egui::Ui, state: &AppState, edit: &mut EditState, is_light: bool) {
    let t = state.t();
    controls::section_frame(is_light).show(ui, |ui| {
        ui.set_width(ui.available_width());
        step_heading(
            ui,
            "2",
            studio::text(
                state,
                "Aparece este texto",
                "This text appears",
                "Lalabas ang tekstong ito",
                "यह टेक्स्ट दिखता है",
            ),
            is_light,
        );
        ui.add_space(controls::GAP_WIDE);

        let was_date = matches!(edit.kind, Kind::Date { .. });
        let mut is_date_mode = was_date;
        // Two positions of one switch, so drawn as one.
        let kinds = [t.kind_text, t.kind_date];
        if let Some(picked) = controls::segmented(ui, "kind", &kinds, Some(is_date_mode as usize)) {
            is_date_mode = picked == 1;
        }

        // Crossing the switch puts the half being left behind into `stashed_kind` and brings back
        // whatever was waiting there. It used to throw the old half away and start the new one from
        // its defaults, which meant a mis-click on a two-position control emptied the replacement
        // box of an expansion being edited, and clicking straight back gave an empty box — the text
        // was gone, and pressing Guardar wrote that emptiness over the stored version.
        //
        // Coming to the switch for the first time still starts from the defaults, so nothing looks
        // any different until you cross back.
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

        ui.add_space(controls::GAP_WIDE);
        match &mut edit.kind {
            Kind::Text { replace } => {
                // A floor, not a box: `add_sized` gives the field at least this much and the field
                // grows past it as the text does, with the page scrolling rather than the words
                // being cut off.
                let height = ui.text_style_height(&egui::TextStyle::Body) * TEXT_AREA_ROWS + 16.0;
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
            } => date_options(ui, t, preset, custom_format, custom_tz, month_lang),
        }

        ui.add_space(controls::GAP_STACK);
        preview_pair(ui, state, edit, is_light);
    });
}

/// The four real formats, the way into building a fifth, and the way out for one the blocks cannot
/// express. Unchanged by the renovation except for where it sits: what a format *means* is not a
/// thing a new coat of paint may touch.
fn date_options(
    ui: &mut egui::Ui,
    t: &'static Strings,
    preset: &mut Option<DatePreset>,
    custom_format: &mut String,
    custom_tz: &mut String,
    month_lang: &mut Lang,
) {
    // Every option carries the date it would produce *right now*, right next to it. That is the
    // whole explanation most people need: you pick the one that looks like what you want, instead
    // of decoding "%B %-d, %Y" to find out.
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
        if date_option(ui, preset.is_none(), t.preset_custom, sample, column) && preset.is_some() {
            // Arriving at "Personalizado" starts from nothing.
            //
            // It used to inherit whatever preset was selected a moment ago, which meant the block
            // builder only appeared if that preset happened to be one the blocks can express —
            // coming from the ISO date it showed up, coming from any of the other three it silently
            // did not. Starting empty removes that entirely, and it is the better first impression
            // anyway: an empty row that says where to drop things explains itself, a row pre-filled
            // with somebody else's format does not.
            *preset = None;
            custom_format.clear();
            custom_tz.clear();
        }
    }

    if preset.is_none() {
        ui.indent("custom_date", |ui| {
            // The builder is the whole of it now. The strftime field, the time-zone box and the
            // "Ejemplo: %Y-%m-%d" line that used to sit underneath are gone: they were the old way
            // of doing this, kept as a safety net, and leaving them there just asked people to
            // learn the notation the blocks exist to replace. The time zone is no longer typed
            // either — it follows from whether an "Hora (UTC)" block is in the row.
            //
            // Only formats the blocks can carry *back out* go to the builder. The blocks hold one
            // time zone between them — UTC or the machine's own — and when they are rebuilt they
            // write the whole `tz` field from that. So a match carrying `tz: Europe/Madrid` handed
            // to the builder would come back with its time zone deleted the first time any block
            // was touched: the same silent loss the advanced field below exists to prevent, which
            // is where such a format goes.
            //
            // The comparison ignores case for the same reason the preview does: espanso accepts
            // `tz: utc`, and the two must not disagree about what it means.
            let tz = custom_tz.trim();
            let tz_is_utc = tz.eq_ignore_ascii_case("UTC");
            let expressible = tz.is_empty() || tz_is_utc;
            match expressible
                .then(|| crate::ui::date_blocks::parse(custom_format, tz_is_utc))
                .flatten()
            {
                Some(blocks) => {
                    if let Some(rebuilt) = crate::ui::date_blocks::show(ui, t, &blocks, month_lang) {
                        *custom_format = crate::ui::date_blocks::to_format(&rebuilt);
                        *custom_tz = crate::ui::date_blocks::timezone_for(&rebuilt)
                            .unwrap_or("")
                            .to_string();
                    }
                }
                None => {
                    // An imported or hand-edited format the blocks cannot express. Shown rather
                    // than silently rewritten, since rewriting it would lose part of what the
                    // author meant.
                    let is_light = !ui.visuals().dark_mode;
                    ui.label(controls::muted(t.blocks_advanced, is_light));
                    ui.add_space(controls::GAP);
                    ui.horizontal(|ui| {
                        ui.label(controls::field_label(t.format_label));
                        ui.add(controls::text_field(custom_format).desired_width(220.0));
                    });
                    // The third place this question has to be asked, and the one it was missing
                    // from. A format the blocks cannot express is the *only* kind that can carry
                    // `%A`, `%b` or `%h`, so this branch is where a written month or weekday is
                    // most likely to turn up — and saving pins a `locale:` for it either way.
                    // Without the picker an imported `%A %-d %B %Y` that named no language quietly
                    // became English, with nothing on screen to say so or to say otherwise.
                    if datefmt::needs_locale(custom_format) {
                        ui.add_space(controls::GAP_SECTION);
                        month_language(ui, t, month_lang);
                    }
                }
            }
        });
    }

    // Under the presets, the same question the block builder asks about its own row: a format that
    // writes the month in words has to say which words.
    if (*preset).is_some_and(|p| datefmt::needs_locale(p.format())) {
        ui.add_space(controls::GAP_SECTION);
        month_language(ui, t, month_lang);
    }
}

/// What the expansion will actually produce, under the trigger that produces it.
///
/// Not a second copy of the box above it: the pairing is the point, and it is the same "you type
/// this, this appears" the library's two columns are headed with. For a date it is the only place
/// the result can be seen at all — nobody reads `%-d de %B de %Y` and sees a date. The sample comes
/// from [`crate::datefmt`], which is the code that runs when the expansion is used, so what is on
/// screen is what espanso will write.
fn preview_pair(ui: &mut egui::Ui, state: &AppState, edit: &EditState, is_light: bool) {
    let trigger = edit.trigger();
    let result = match &edit.kind {
        Kind::Text { replace } => replace.clone(),
        Kind::Date {
            preset,
            custom_format,
            custom_tz,
            month_lang,
        } => match preset {
            Some(p) => p.sample(*month_lang),
            None => datefmt::sample(
                custom_format,
                (!custom_tz.trim().is_empty()).then(|| custom_tz.trim()),
                *month_lang,
            ),
        },
    };

    ui.label(controls::small_muted(
        studio::text(state, "Vista previa", "Preview", "Preview", "पूर्वावलोकन"),
        is_light,
    ));
    ui.add_space(controls::GAP_TIGHT);
    controls::inset_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.add(
            egui::Label::new(
                controls::code(
                    if trigger.is_empty() { "…" } else { &trigger },
                    ui.visuals(),
                )
                .strong(),
            )
            .wrap()
            .selectable(false),
        );
        ui.add_space(controls::GAP);
        let (rule, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(rule, 0, crate::app::hairline(is_light));
        ui.add_space(controls::GAP);
        // Rounded down to whole lines, so the box never shows the top half of a line of text.
        let line = ui.text_style_height(&egui::TextStyle::Body);
        let cap = (PREVIEW_MAX_HEIGHT / line).floor().max(3.0) * line;
        egui::ScrollArea::vertical()
            .id_salt("editor-preview")
            .max_height(cap)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                if result.is_empty() {
                    ui.label(controls::muted(
                        studio::text(
                            state,
                            "Tu texto aparecerá aquí…",
                            "Your text will appear here…",
                            "Lalabas ang teksto mo rito…",
                            "आपका टेक्स्ट यहाँ दिखेगा…",
                        ),
                        is_light,
                    ));
                } else {
                    ui.add(egui::Label::new(&result).wrap().selectable(true));
                }
            });
    });

    ui.add_space(controls::GAP);
    let footnote = match &edit.kind {
        Kind::Text { .. } => {
            let n = result.chars().count();
            let template = studio::plural(
                n,
                studio::text(state, "{n} carácter", "{n} character", "{n} karakter", "{n} अक्षर"),
                studio::text(
                    state,
                    "{n} caracteres",
                    "{n} characters",
                    "{n} karakter",
                    "{n} अक्षर",
                ),
            );
            fill(template, &[("n", &n.to_string())])
        }
        // Said out loud because the preview shows *today*, and a date expansion that wrote today's
        // date for ever would be a bug rather than a feature.
        Kind::Date { .. } => studio::text(
            state,
            "La fecha se calcula cada vez que escribes el atajo.",
            "The date is worked out each time you type the shortcut.",
            "Kinakalkula ang petsa sa tuwing ita-type mo ang shortcut.",
            "हर बार शॉर्टकट लिखने पर दिनांक की गणना होती है।",
        )
        .to_string(),
    };
    ui.label(controls::small_muted(footnote, is_light));
}

/// Where this expansion is filed. Deliberately *not* numbered: it changes nothing about what
/// espanso does, and a step three would say that it did.
fn folder_card(ui: &mut egui::Ui, state: &AppState, edit: &mut EditState, is_light: bool) {
    let t = state.t();
    controls::section_frame(is_light).show(ui, |ui| {
        ui.set_width(ui.available_width());
        // The string carries the colon it needs as a field label; as a heading it does not.
        ui.label(controls::h3(t.folder_optional.trim_end_matches(':')));
        ui.add_space(controls::GAP_TIGHT);
        ui.label(controls::muted(
            studio::text(
                state,
                "Solo sirve para encontrarla aquí. Espanso no usa las carpetas.",
                "Only for finding it here. Espanso itself does not use folders.",
                "Para lang mahanap ito rito. Hindi gumagamit ng folder ang Espanso.",
                "यह केवल इसे यहाँ खोजने के लिए है। Espanso फ़ोल्डर का उपयोग नहीं करता।",
            ),
            is_light,
        ));
        ui.add_space(controls::GAP_ROW);

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
        // Truncating rather than extending: a long folder name may not push the control out past
        // the edge of the card it is in.
        egui::ComboBox::from_id_salt("edit_folder_combo")
            .selected_text(selected_text)
            .truncate()
            .width(ui.available_width())
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
        // The box for the new name appears next to the choice that asked for it, which is the
        // plan's rule about creating a folder from here rather than going somewhere else to do it.
        if edit.choosing_new_folder {
            ui.add_space(controls::GAP_ROW);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = controls::GAP;
                ui.label(controls::field_label(t.name_label));
                ui.add(
                    controls::text_field(&mut edit.new_folder_input)
                        .desired_width((ui.available_width() - controls::GAP).max(120.0))
                        .hint_text(t.new_folder_placeholder),
                );
            });
        }
    });
}

/// The form itself: a title, two numbered steps and the folder.
///
/// It takes `edit` by value and puts it back at the end, which is what lets the two steps hold
/// `&mut EditState` without the borrow checker having to reason about `state.view` at the same
/// time. The draft is one thing throughout the frame, and it is written back once.
fn show_form(
    ui: &mut egui::Ui,
    state: &mut AppState,
    mut edit: EditState,
    problem: Option<&Problem>,
    is_light: bool,
) {
    let t = state.t();
    let is_new = edit.editing_index.is_none();

    ui.label(controls::h2(if is_new {
        t.edit_title_new
    } else {
        t.edit_title_existing
    }));
    ui.add_space(controls::GAP_TIGHT);
    // Says what the screen is rather than admiring what the app does. The previous line here — "Unas
    // pocas teclas se convierten en algo más" — was a slogan, and a slogan at the top of a form is a
    // line of text that answers none of the questions somebody opening the form actually has.
    ui.label(controls::muted(
        studio::text(
            state,
            "Dos pasos: elige qué escribes y qué debe aparecer.",
            "Two steps: choose what you type and what should appear.",
            "Dalawang hakbang: piliin ang ita-type at ang dapat lumabas.",
            "दो चरण: क्या लिखना है और क्या दिखना चाहिए।",
        ),
        is_light,
    ));
    ui.add_space(controls::GAP_STACK);

    when_you_type(ui, state, &mut edit, problem, is_light);
    ui.add_space(controls::GAP_SECTION);
    appears_here(ui, state, &mut edit, is_light);
    ui.add_space(controls::GAP_SECTION);
    folder_card(ui, state, &mut edit, is_light);
    // The action strip below is a surface of its own; without this the last card butts into it.
    ui.add_space(controls::GAP);

    // The draft is its own state, and a change to it has to show up on the next frame rather than
    // whenever something else happens to ask for one.
    if matches!(&state.view, View::Edit(current) if current != &edit) {
        ui.ctx().request_repaint();
    }
    state.view = View::Edit(edit);
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

    fn typed(prefix: &str, word: &str) -> String {
        EditState {
            prefix: prefix.to_string(),
            word: word.to_string(),
            ..EditState::new_for_create("")
        }
        .trigger()
    }

    /// `:--` → `—` was impossible: a word box holding only symbols was read as a second prefix,
    /// which left no word, which the save check reported as an empty trigger.
    #[test]
    fn a_word_made_only_of_symbols_is_a_word() {
        assert_eq!(typed(":", "--"), ":--");
        assert_eq!(typed(":", "_"), ":_");
        assert_eq!(typed(":", "—"), ":—");
        assert_eq!(typed("::", "--"), "::--");
        // Whole thing typed into the word box: no prefix to find, so it is kept entire.
        assert_eq!(split_trigger(":--"), (String::new(), ":--".to_string()));
    }

    /// The two reasons the split exists in the first place, neither of which may move.
    #[test]
    fn the_prefix_box_still_owns_the_prefix_and_an_empty_word_is_still_empty() {
        assert_eq!(typed(":", ":hola"), ":hola");
        assert_eq!(typed(":", "hola"), ":hola");
        assert_eq!(split_trigger("::hola"), ("::".to_string(), "hola".to_string()));
        assert_eq!(typed(":", ""), "");
        assert_eq!(typed(":", "   "), "");
    }

    /// Opening a symbol-only trigger for editing and pressing Guardar has to give it back
    /// unchanged; otherwise the expansion could be created and then never edited again.
    #[test]
    fn a_symbol_only_trigger_survives_a_round_trip_through_the_editor() {
        for trigger in [":--", ":_", ":—", "//", ";;"] {
            let stored = SimpleMatch {
                trigger: trigger.to_string(),
                replace: "—".to_string(),
                vars: Vec::new(),
                label: None,
            };
            let edit = EditState::new_for_edit(0, &stored, None);
            assert_eq!(edit.trigger(), trigger);
            assert_eq!(edit.build_match().trigger, trigger);
        }
    }
}
