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

//! Small, surgical edits to `.espanso/config/default.yml` — espanso's file, not ours.
//!
//! Three settings are ours to hold: espanso's tray icon is hidden so there are not two icons for
//! one program, its notifications are silenced, and the search shortcut is handed to whoever should
//! have it (see `OWN_LAUNCHER` in [`crate::app`]). Everything else in that file belongs to the user
//! and must survive untouched.
//!
//! Hence the rules, which are what the tests here are actually testing: a key counts only at column
//! zero, because the same word indented is a value inside somebody else's block; a commented-out
//! key is not set; the file keeps the line endings it arrived with; and a file that is not there at
//! all is not an error, it is espanso's first run, so there is nothing to patch yet.
//!
//! Returns whether anything actually changed, because the caller restarts espanso on `true` and
//! restarting it on every start would be a visible cost for no reason.

use std::path::Path;

/// Espanso settings that EspansoManager turns off on its behalf, with the note left in the file
/// explaining why — the config is meant to be readable, so a change made by us shouldn't look
/// anonymous to whoever opens it next.
const MANAGED: [(&str, &str, &str); 2] = [
    (
        "show_icon",
        "false",
        "# Disabled by EspansoManager: avoids a second Espanso icon in the system tray, since \
         EspansoManager already shows one.",
    ),
    (
        "show_notifications",
        "false",
        "# Disabled by EspansoManager: the app reports what happened in its own window, so the \
         Windows toasts would only repeat it.",
    ),
];

/// Ensures the settings in [`MANAGED`] are set as we need them in espanso's own config.
pub fn ensure_managed_settings(config_path: &Path) -> std::io::Result<bool> {
    apply(config_path, &MANAGED)
}

/// Gives espanso its own tray icon back.
///
/// The one thing this app must never do is take espanso's icon away and then fail to show its
/// own: espanso keeps expanding text either way, and with no icon from either program there is
/// nothing on screen to pause it, stop it, or even explain where the typing is coming from — the
/// Task Manager is the only way out, and only for someone who already knows what to look for.
///
/// So the failure to build our icon is not just reported, it is undone. Left as a plain `true`
/// rather than removing the line, because a value with a note beside it says more to whoever
/// opens the file next than a key that quietly disappeared.
pub fn restore_espanso_icon(config_path: &Path) -> std::io::Result<bool> {
    apply(
        config_path,
        &[(
            "show_icon",
            "true",
            "# Restored by EspansoManager: it could not show its own tray icon, so Espanso keeps \
             its one.",
        )],
    )
}

/// Decides who owns the search shortcut.
///
/// `OFF` when this app has successfully claimed Alt+Space for its own search window; the espanso
/// default when it has not, so the feature survives even where the shortcut was already taken by
/// something else. Called once the answer is actually known, which is after the hotkey has been
/// registered — hence its own entry point rather than a line in [`MANAGED`].
pub fn set_search_shortcut(config_path: &Path, value: &str) -> std::io::Result<bool> {
    apply(
        config_path,
        &[(
            "search_shortcut",
            value,
            "# Set by EspansoManager: OFF while the app provides its own search window on the same \
             shortcut,\n# and espanso's default when that shortcut could not be claimed.",
        )],
    )
}

/// Edits `default.yml` at the text/line level rather than re-parsing and re-emitting the whole
/// YAML, so every existing comment in that file survives untouched — unlike the match file, this
/// one is meant to be read by users as inline documentation.
///
/// A key that is already present is rewritten in place; one that is missing is appended with its
/// explanation. A key the user has deliberately commented out is left alone, since a leading `#`
/// means it isn't active anyway.
///
/// Returns `Ok(true)` if the file was modified (meaning the daemon should be restarted to pick up
/// the change), `Ok(false)` if everything already matched, and does nothing (returning `Ok(false)`)
/// if the file doesn't exist yet — espanso creates it on its own first run.
///
/// Every other read failure is an `Err`, and the callers say so out loud. "Not there yet" is the
/// only one that means "nothing to do": a file we are not allowed to open, or one that is no longer
/// valid UTF-8 — which is all it takes for a Windows editor to save this file as ANSI over the
/// accented comment we ourselves put in it — used to look exactly like "everything already matched".
/// Espanso's tray icon and its Windows toasts would come back and stay back, with nothing anywhere
/// saying why.
fn apply(config_path: &Path, entries: &[(&str, &str, &str)]) -> std::io::Result<bool> {
    let original = match std::fs::read_to_string(config_path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };

    // Whatever the file already uses. `str::lines` drops the `\r`, so without this a single edit
    // would quietly rewrite every line of a CRLF file — the sort of change that turns a two-line
    // difference into a whole-file one for anyone who keeps this folder under version control.
    let newline = if original.contains("\r\n") { "\r\n" } else { "\n" };

    let mut lines: Vec<String> = original.lines().map(str::to_string).collect();
    let mut changed = false;

    for (key, value, comment) in entries {
        let mut found_active = false;

        for line in &mut lines {
            // Column zero, which is where a top-level espanso setting always is. Matching at any
            // indentation meant a `show_icon:` sitting inside a block scalar — someone's own text,
            // not configuration — was rewritten as though it were the setting, and counted as
            // found, so the real key was never added and espanso kept its default.
            let Some(rest) = line.strip_prefix(*key) else {
                continue;
            };
            if !rest.trim_start().starts_with(':') {
                continue;
            }
            found_active = true;
            let desired = format!("{key}: {value}");
            if line.trim_end() != desired {
                *line = desired;
                changed = true;
            }
        }

        if !found_active {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push(comment.to_string());
            lines.push(format!("{key}: {value}"));
            changed = true;
        }
    }

    if changed {
        let mut new_contents = lines.join(newline);
        if original.ends_with('\n') {
            new_contents.push_str(newline);
        }
        let tmp_path = config_path.with_extension("yml.tmp");
        std::fs::write(&tmp_path, new_contents)?;
        std::fs::rename(&tmp_path, config_path)?;
    }

    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("espansomanager-cfg-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const ONE: [(&str, &str, &str); 1] = [("show_icon", "false", "# note")];

    #[test]
    fn a_missing_file_is_the_one_read_failure_that_means_nothing_to_do() {
        let dir = scratch("missing");
        assert!(!apply(&dir.join("default.yml"), &ONE).unwrap());
    }

    #[test]
    fn a_file_that_is_not_utf8_is_reported_instead_of_being_taken_for_success() {
        let dir = scratch("badbytes");
        let path = dir.join("default.yml");
        // What a Windows editor saves when it writes an accented comment as ANSI. The file this
        // app installs really does carry one.
        std::fs::write(&path, b"# segundo \xed cono\nshow_icon: true\n").unwrap();
        let err = apply(&path, &ONE).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        // And the file it could not read is left exactly as it was.
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"# segundo \xed cono\nshow_icon: true\n"
        );
    }

    #[test]
    fn a_top_level_key_is_rewritten_where_it_stands() {
        let dir = scratch("toplevel");
        let path = dir.join("default.yml");
        std::fs::write(&path, "backend: Auto\nshow_icon: true\n").unwrap();
        assert!(apply(&path, &ONE).unwrap());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "backend: Auto\nshow_icon: false\n"
        );
        // Running it again has nothing left to do.
        assert!(!apply(&path, &ONE).unwrap());
    }

    #[test]
    fn an_indented_line_is_somebody_elses_text_and_is_left_alone() {
        let dir = scratch("indented");
        let path = dir.join("default.yml");
        std::fs::write(&path, "note: |\n  show_icon: true\n").unwrap();
        assert!(apply(&path, &ONE).unwrap());
        let out = std::fs::read_to_string(&path).unwrap();
        // The user's own line survives...
        assert!(out.contains("  show_icon: true"));
        // ...and the setting we actually manage is added at the top level, where espanso reads it.
        assert!(out.contains("\nshow_icon: false"));
    }

    #[test]
    fn a_commented_out_key_is_not_active_and_does_not_count() {
        let dir = scratch("commented");
        let path = dir.join("default.yml");
        std::fs::write(&path, "# show_icon: true\n").unwrap();
        assert!(apply(&path, &ONE).unwrap());
        let out = std::fs::read_to_string(&path).unwrap();
        assert!(out.contains("# show_icon: true"));
        assert!(out.contains("\nshow_icon: false"));
    }

    #[test]
    fn the_file_keeps_the_line_endings_it_arrived_with() {
        let dir = scratch("crlf");
        let path = dir.join("default.yml");
        std::fs::write(&path, "backend: Auto\r\nshow_icon: true\r\n").unwrap();
        assert!(apply(&path, &ONE).unwrap());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "backend: Auto\r\nshow_icon: false\r\n"
        );
    }

    #[test]
    fn an_appended_key_on_a_crlf_file_uses_crlf_too() {
        let dir = scratch("crlfadd");
        let path = dir.join("default.yml");
        std::fs::write(&path, "backend: Auto\r\n").unwrap();
        assert!(apply(&path, &ONE).unwrap());
        let out = std::fs::read_to_string(&path).unwrap();
        assert!(!out.contains('\n') || out.matches('\n').count() == out.matches("\r\n").count());
        assert!(out.contains("\r\nshow_icon: false"));
    }
}
