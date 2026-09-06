//! What Windows itself is currently wearing: light or dark, and the accent colour.
//!
//! Three registry reads and a preference. `ThemeMode` is the user's choice — follow the system, or
//! pin one appearance — and `follows_system` is what lets a pinned theme skip the watch in
//! [`crate::sysevents`] entirely, since a pinned theme cannot change underneath us.
//!
//! Every read falls back rather than failing: a missing `AppsUseLightTheme` means light, which is
//! Windows' own default on a fresh install, and a missing accent palette means Windows' default
//! blue. An app that refuses to draw because a registry value is absent would be a worse answer
//! than one drawn in the wrong blue.
//!
//! The colours themselves are not here — this module answers *which* palette, and
//! `app.rs` holds the palette.

use serde::{Deserialize, Serialize};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

/// Whether the interface follows Windows or is pinned to one appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub const ALL: [ThemeMode; 3] = [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark];

    /// Resolves to the appearance that should actually be drawn right now.
    pub fn is_light(self) -> bool {
        match self {
            ThemeMode::System => is_light_theme(),
            ThemeMode::Light => true,
            ThemeMode::Dark => false,
        }
    }

    /// Only the automatic mode has to keep watching the registry; a pinned theme never changes on
    /// its own, so the poll can be skipped entirely.
    pub fn follows_system(self) -> bool {
        matches!(self, ThemeMode::System)
    }
}

/// Reads whether Windows 11 is currently set to a light theme for apps. Defaults to light if the
/// registry value is missing (matches Windows' own default on a fresh install).
pub fn is_light_theme() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
        .and_then(|key| key.get_value::<u32, _>("AppsUseLightTheme"))
        .map(|v| v != 0)
        .unwrap_or(true)
}

/// Windows' default accent (`#0078D4`) and the two shades Fluent derives from it, used when the
/// real palette can't be read.
const FALLBACK_ACCENT_LIGHT2: (u8, u8, u8) = (0x4C, 0xC2, 0xFF);
const FALLBACK_ACCENT_DARK1: (u8, u8, u8) = (0x00, 0x67, 0xC0);

/// The user's own accent colour, in the shade Fluent specifies for the given theme.
///
/// Windows keeps eight shades of the chosen accent in `AccentPalette`, ordered lightest to darkest.
/// Fluent uses *Light2* (index 1) for accent text and fills on dark backgrounds and *Dark1*
/// (index 4) on light ones — picking the same shades is what makes the app read as part of the
/// system rather than as something with its own idea of blue.
pub fn system_accent(is_light: bool) -> (u8, u8, u8) {
    let index = if is_light { 4 } else { 1 };
    read_accent_palette()
        .and_then(|p| {
            let o = index * 4;
            (p.len() >= o + 3).then(|| (p[o], p[o + 1], p[o + 2]))
        })
        .unwrap_or(if is_light {
            FALLBACK_ACCENT_DARK1
        } else {
            FALLBACK_ACCENT_LIGHT2
        })
}

fn read_accent_palette() -> Option<Vec<u8>> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Accent")
        .ok()?;
    let value: winreg::RegValue = key.get_raw_value("AccentPalette").ok()?;
    Some(value.bytes.into_owned())
}
