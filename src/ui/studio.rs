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

//! The pieces the screens share but that are not plain controls: the drawn icon set, the folder
//! filter and the four-language literal.
//!
//! This file used to own the window's whole frame as well — a left-hand navigation column, a right-
//! hand inspector and a status strip along the bottom — and the three of them are gone. The
//! renovation puts the folders above the list as filters and the actions inside the row they act
//! on, so a permanent side panel had nothing left to show and was spending a fifth of a narrow
//! window to show it. What remains here is only what more than one screen genuinely shares.
//!
//! The live preview of an expansion went the same way. It was the inspector's content, kept on as a
//! column beside the editor, and it spent that column repeating the box the user was typing into.
//! The editor now shows the trigger and its result as a pair, in step 2 where the result is decided;
//! see [`crate::ui::edit_form`].
//!
//! **Not here:** anything that writes to disk, and anything that belongs to one screen alone. The
//! folder options moved out to [`crate::ui::folder_view`] when they became a screen of their own.
//!
//! The icons are painted from line segments rather than set from a font. Every one of them has to
//! land in an 18-point box next to text, on four alphabets and on whatever fonts the machine turns
//! out to have; a glyph arrives at the size and baseline its font chose, and an absent one arrives
//! as an empty box. See [`crate::fonts::can_draw`] for why asking the font first is not enough on
//! its own.

use crate::app::{AppState, View};
use crate::i18n::Lang;
use crate::ui::edit_form::EditState;
use egui::{Color32, Stroke, Vec2};

pub fn create_expansion(ctx: &egui::Context, state: &mut AppState) {
    let folder = filter(ctx).filter(|f| !f.is_empty());
    let list = state.list();
    let prefix = folder.as_ref().and_then(|f| list.folders.get(f))
        .filter(|info| info.prefixes.len() == 1).and_then(|info| info.prefixes.first())
        .unwrap_or(&state.settings.prefix);
    let mut edit = EditState::new_for_create(prefix);
    edit.folder = folder;
    state.view = View::Edit(edit);
}

/// Why a prefix cannot be applied, in the user's language. Lives here rather than in
/// [`crate::ui::folder_view`] because [`crate::app`] raises the same sentence as a banner when a
/// prefix typed in the settings collides with an existing trigger.
pub fn prefix_error(state: &AppState, error: &crate::folders::PrefixError) -> String {
    match error {
        crate::folders::PrefixError::Invalid => text(state,
            "Usa de 1 a 8 símbolos, sin letras, números ni espacios. Por ejemplo: : o //.",
            "Use 1–8 symbols without letters, numbers or spaces. For example: : or //.",
            "Gumamit ng 1–8 simbolo, walang letra, numero o espasyo. Halimbawa: : o //.",
            "1–8 चिह्न लिखें, अक्षर, अंक या रिक्त स्थान नहीं। जैसे : या //।").into(),
        crate::folders::PrefixError::Collision(trigger) => crate::i18n::fill(state.t().prefix_collision, &[("list", trigger)]),
    }
}

pub fn text<'a>(state: &AppState, es: &'a str, en: &'a str, fil: &'a str, hi: &'a str) -> &'a str {
    match state.settings.lang {
        Lang::Es => es,
        Lang::En => en,
        Lang::Fil => fil,
        Lang::Hi => hi,
    }
}

/// Picks between two already-translated wordings by count.
///
/// [`text`] settles the language; this settles the number, which Spanish and Hindi will not let a
/// single template get away with — "1 expansiones" is not a typo the reader forgives. Both forms
/// keep their `{n}` placeholder so the caller can go on filling them the same way.
pub fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 {
        one
    } else {
        many
    }
}

pub fn filter(ctx: &egui::Context) -> Option<String> {
    ctx.data(|d| d.get_temp::<String>(egui::Id::new("studio-folder")))
}

pub(super) fn set_filter(ctx: &egui::Context, state: &mut AppState, folder: Option<&str>) {
    ctx.data_mut(|d| {
        let key = egui::Id::new("studio-folder");
        if let Some(folder) = folder {
            d.insert_temp(key, folder.to_owned());
        } else {
            d.remove::<String>(key);
        }
    });
    state.clear_selection();
    state.cancel_rename_folder();
    state.search.clear();
    state.view = View::List;
    ctx.request_repaint();
}

/// The icons this app draws. A short list on purpose: each one is a few line segments written by
/// hand, and a set that grows past what the screens actually ask for is a set nobody keeps
/// consistent. Add a variant when a screen needs it, not in anticipation.
#[derive(Clone, Copy)]
pub enum Icon {
    Search,
}

/// Draws `kind` centred in `rect`, at a fixed 18 points whatever the rect is, so an icon beside
/// text is the same size on every screen.
pub fn icon(ui: &egui::Ui, rect: egui::Rect, kind: Icon, ink: Color32) {
    let r = egui::Rect::from_center_size(rect.center(), Vec2::splat(18.0));
    let p = |x: f32, y: f32| r.min + egui::vec2(x, y);
    let s = Stroke::new(1.45, ink);
    let painter = ui.painter();
    match kind {
        Icon::Search => {
            painter.circle_stroke(p(7., 7.), 5., s);
            painter.line_segment([p(11., 11.), p(16., 16.)], s);
        }
    }
}
