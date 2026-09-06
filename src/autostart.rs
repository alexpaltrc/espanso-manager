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

//! The "start with Windows" checkbox, which is one value under `HKCU\...\CurrentVersion\Run`.
//!
//! Registered with `--tray`, so the copy Windows starts at login comes up hidden beside the clock
//! instead of throwing a window at somebody who was trying to log in.
//!
//! The state is read live from the registry every time it is asked for, never cached in
//! `settings.json`. Two things own this bit — us and whatever else can edit that key, including the
//! person themselves — and a cached copy would let the checkbox and the machine disagree with no
//! way to tell which one is lying.

use std::path::Path;
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "EspansoManager";

/// Reads live from the registry rather than a cached setting, so the checkbox never drifts from
/// reality if someone edits the Run key by hand.
pub fn is_enabled() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(RUN_KEY)
        .and_then(|key| key.get_value::<String, _>(VALUE_NAME))
        .is_ok()
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
    Ok(())
}
