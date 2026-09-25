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

//! Every keyboard shortcut the app answers to, in one table, and how each is written in each language.
//!
//! A screen never spells a key combination itself: it asks this file whether one was pressed and how
//! to print it. That is what keeps a tooltip, a menu and the guide from ever disagreeing about which
//! keys do what, and what lets the guide print the whole table without keeping a copy of it.
//!
//! ## What is never taken
//!
//! Nothing here is bound to a key Windows or the text fields already mean something by: no Alt+Tab,
//! Alt+F4, Win+anything, Ctrl+C/X/V/Z/Y, Ctrl+A (which on this machine's Spanish Windows is *Abrir*,
//! not *Seleccionar todo*), Home/End or the arrows while a field has the cursor. The shortcuts that
//! act on the list — F2, Supr, Entrar, the arrows — are only read while nothing is being typed
//! ([`typing`]), so a word typed into the search box can never delete an expansion.

use crate::i18n::Lang;

/// One key combination.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Shortcut {
    pub mods: egui::Modifiers,
    pub key: egui::Key,
}

const fn ctrl(key: egui::Key) -> Shortcut {
    Shortcut { mods: egui::Modifiers::CTRL, key }
}

const fn bare(key: egui::Key) -> Shortcut {
    Shortcut { mods: egui::Modifiers::NONE, key }
}

/// A new expansion — in the folder being looked at, when there is one.
pub const NEW: Shortcut = ctrl(egui::Key::N);
/// To the search box, with what is in it selected. Ctrl+L as in every Windows address bar; Ctrl+F
/// because it is the one people try first.
pub const FIND: Shortcut = ctrl(egui::Key::L);
pub const FIND_ALT: Shortcut = ctrl(egui::Key::F);
/// Rename the folder being looked at.
pub const RENAME: Shortcut = bare(egui::Key::F2);
/// Delete what is chosen — always through the confirmation.
pub const DELETE: Shortcut = bare(egui::Key::Delete);
/// Open the chosen expansion in the editor.
pub const OPEN: Shortcut = bare(egui::Key::Enter);
/// Save in the editor.
pub const SAVE: Shortcut = ctrl(egui::Key::S);
/// Back out of whatever is on top: a menu, then a dialog, then the detail.
pub const BACK: Shortcut = bare(egui::Key::Escape);
/// Move the choice through the list.
pub const UP: Shortcut = bare(egui::Key::ArrowUp);
pub const DOWN: Shortcut = bare(egui::Key::ArrowDown);

/// Whether a text field has the keyboard. While it does, only shortcuts with Ctrl are read.
pub fn typing(ctx: &egui::Context) -> bool {
    ctx.text_edit_focused()
}

/// Whether `shortcut` was pressed this frame; consumes it, so nothing else acts on it as well.
pub fn pressed(ctx: &egui::Context, shortcut: Shortcut) -> bool {
    ctx.input_mut(|i| i.consume_key(shortcut.mods, shortcut.key))
}

/// How a key is printed on a Windows keyboard in this language.
fn key_name(key: egui::Key, lang: Lang) -> String {
    use egui::Key;
    let es = matches!(lang, Lang::Es);
    match key {
        Key::Delete => if es { "Supr" } else { "Del" }.into(),
        Key::Enter => if es { "Entrar" } else { "Enter" }.into(),
        Key::Escape => "Esc".into(),
        Key::ArrowUp => "↑".into(),
        Key::ArrowDown => "↓".into(),
        other => other.name().into(),
    }
}

/// `shortcut` written out the way Windows writes it in menus: "Ctrl+N", "Supr".
pub fn text(shortcut: Shortcut, lang: Lang) -> String {
    let mut out = String::new();
    if shortcut.mods.ctrl || shortcut.mods.command {
        out.push_str("Ctrl+");
    }
    if shortcut.mods.shift {
        out.push_str(if matches!(lang, Lang::Es) { "Mayús+" } else { "Shift+" });
    }
    if shortcut.mods.alt {
        out.push_str("Alt+");
    }
    out.push_str(&key_name(shortcut.key, lang));
    out
}

/// A click with a key held, written like a shortcut: "Ctrl+clic", "Mayús+clic".
pub fn click_text(mods: egui::Modifiers, lang: Lang) -> String {
    let click = match lang {
        Lang::Es => "clic",
        _ => "click",
    };
    let held = text(Shortcut { mods, key: egui::Key::A }, lang);
    format!("{}{click}", held.trim_end_matches('A'))
}

/// A tooltip that names an action and, after it, its keys — "Nueva expansión (Ctrl+N)", the way
/// Windows' own command bars do. The keys are there when asked for and nowhere else.
pub fn tip(label: &str, shortcut: Shortcut, lang: Lang) -> String {
    format!("{label} ({})", text(shortcut, lang))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_written_the_windows_way() {
        assert_eq!(text(NEW, Lang::Es), "Ctrl+N");
        assert_eq!(text(DELETE, Lang::Es), "Supr");
        assert_eq!(text(DELETE, Lang::En), "Del");
        assert_eq!(text(RENAME, Lang::Hi), "F2");
        assert_eq!(tip("Buscar", FIND, Lang::Es), "Buscar (Ctrl+L)");
        assert_eq!(click_text(egui::Modifiers::SHIFT, Lang::Es), "Mayús+clic");
        assert_eq!(click_text(egui::Modifiers::CTRL, Lang::En), "Ctrl+click");
    }

    #[test]
    fn no_two_commands_share_keys() {
        let all = [NEW, FIND, FIND_ALT, RENAME, DELETE, OPEN, SAVE, BACK, UP, DOWN];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}
