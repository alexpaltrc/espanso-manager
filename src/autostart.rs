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

//! The "start with Windows" checkbox, which is one value under `HKCU\...\CurrentVersion\Run`
//! and one veto over it under `...\Explorer\StartupApproved\Run`.
//!
//! Registered with `--tray`, so the copy Windows starts at login comes up hidden beside the clock
//! instead of throwing a window at somebody who was trying to log in.
//!
//! The state is read live from the registry every time it is asked for, never cached in
//! `settings.json`. Two things own this bit — us and whatever else can edit that key, including the
//! person themselves — and a cached copy would let the checkbox and the machine disagree with no
//! way to tell which one is lying.
//!
//! "Enabled" here means *this* executable actually starts at login, which is a stricter question
//! than "is there a value with our name on it". A value naming a copy of the app in a folder that
//! has since been moved or renamed starts nothing; so does a value Windows has been told to ignore.
//! Both used to read as enabled, which turned a switch into a decoration — and neither can be
//! detected by looking at the Run value alone.

use std::path::Path;
use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
use winreg::RegKey;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
/// Where Task Manager's *Startup apps* tab, the Settings app and every tidy-up utility record that
/// an entry has been switched off. They do not delete the Run value when they do it.
const APPROVED_KEY: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run";
const VALUE_NAME: &str = "EspansoManager";

/// The executable named by a Run command, as written by us (`"C:\...\x.exe" --tray`) or by hand
/// (`C:\...\x.exe --tray`).
///
/// An unquoted path containing spaces cannot be told apart from a path plus arguments, and Windows
/// itself guesses at that. We always write the quoted form, so the unquoted branch only ever reads
/// somebody else's writing; guessing wrong there costs a switch that reads off until it is pressed,
/// not a broken entry.
fn registered_exe(command: &str) -> &str {
    let command = command.trim();
    match command.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or(""),
        None => command.split_whitespace().next().unwrap_or(""),
    }
}

/// Whether two paths name the same file on disk, resolving `..`, short 8.3 names and the casing
/// Windows does not care about. A registered path that no longer exists cannot resolve, and falls
/// back to a plain comparison it will fail — which is exactly the answer wanted: it starts nothing.
fn same_exe(registered: &Path, ours: &Path) -> bool {
    match (registered.canonicalize(), ours.canonicalize()) {
        (Ok(registered), Ok(ours)) => registered == ours,
        _ => registered
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(&ours.as_os_str().to_string_lossy()),
    }
}

/// Reads the flag Windows keeps in `StartupApproved`: the low bit of the first byte is the veto.
/// `02`/`06` mean enabled, `03`/`07` disabled. No record at all is the ordinary case and means
/// enabled — nobody has ever turned this entry off.
fn approved(record: Option<&[u8]>) -> bool {
    record.is_none_or(|bytes| bytes.first().is_none_or(|flags| flags & 1 == 0))
}

fn is_approved() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.open_subkey(APPROVED_KEY).and_then(|key| key.get_raw_value(VALUE_NAME)) {
        Ok(value) => approved(Some(&value.bytes)),
        Err(_) => approved(None),
    }
}

/// Reads live from the registry rather than a cached setting, so the checkbox never drifts from
/// reality if someone edits the Run key by hand.
pub fn is_enabled(exe_path: &Path) -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.open_subkey(RUN_KEY).and_then(|key| key.get_value::<String, _>(VALUE_NAME)) {
        Ok(command) => {
            same_exe(Path::new(registered_exe(&command)), exe_path) && is_approved()
        }
        Err(_) => false,
    }
}

pub fn set_enabled(enabled: bool, exe_path: &Path) -> std::io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(RUN_KEY)?;
    if enabled {
        let command = format!("\"{}\" --tray", exe_path.display());
        key.set_value(VALUE_NAME, &command)?;
    } else {
        // Not being registered at all is not an error.
        let _ = key.delete_value(VALUE_NAME);
    }
    // The veto goes either way, and its absence is what Windows reads as approval. Turning the
    // switch on has to clear a previous "off" or it would write a value that never runs and then
    // report success; turning it off leaves no note about a value that no longer exists. Failing to
    // reach the key is not worth a banner: the Run value, which is the part that starts anything,
    // is already written.
    if let Ok(approved) = hkcu.open_subkey_with_flags(APPROVED_KEY, KEY_SET_VALUE) {
        let _ = approved.delete_value(VALUE_NAME);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both spellings of a Run command, plus the ones that name nothing at all.
    #[test]
    fn reads_the_executable_out_of_a_run_command() {
        assert_eq!(registered_exe("\"C:\\Espanso\\EspansoManager.exe\" --tray"), "C:\\Espanso\\EspansoManager.exe");
        assert_eq!(registered_exe("\"C:\\Mis Cosas\\EspansoManager.exe\" --tray"), "C:\\Mis Cosas\\EspansoManager.exe");
        assert_eq!(registered_exe("C:\\Espanso\\EspansoManager.exe --tray"), "C:\\Espanso\\EspansoManager.exe");
        assert_eq!(registered_exe("  C:\\Espanso\\EspansoManager.exe  "), "C:\\Espanso\\EspansoManager.exe");
        assert_eq!(registered_exe(""), "");
        assert_eq!(registered_exe("\""), "");
    }

    /// A folder that moved leaves the old path behind. The switch has to say off, because off is
    /// what it is — and pressing it then writes the path that works.
    #[test]
    fn a_command_pointing_somewhere_else_is_not_enabled() {
        let ours = Path::new("C:\\Espanso\\EspansoManager.exe");
        assert!(same_exe(Path::new("c:\\espanso\\espansomanager.exe"), ours));
        assert!(!same_exe(Path::new("D:\\Viejo\\EspansoManager.exe"), ours));
        assert!(!same_exe(Path::new(""), ours));
    }

    /// The byte Task Manager writes when somebody switches the entry off there.
    #[test]
    fn the_startup_approved_flag_is_the_low_bit() {
        assert!(approved(None));
        assert!(approved(Some(&[0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])));
        assert!(approved(Some(&[0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])));
        assert!(!approved(Some(&[0x03, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])));
        assert!(!approved(Some(&[0x07, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])));
        // A value with no bytes says nothing; nothing means nobody turned it off.
        assert!(approved(Some(&[])));
    }
}
