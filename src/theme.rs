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

//! What Windows itself is currently wearing: light or dark, the accent colour, and whether
//! contrast is on.
//!
//! A few registry reads and a preference. `ThemeMode` is the user's choice — follow the system, or
//! pin one appearance — and `follows_system` is what lets a pinned theme skip the watch in
//! [`crate::sysevents`] entirely, since a pinned theme cannot change underneath us.
//!
//! Every read falls back rather than failing: a missing `AppsUseLightTheme` means light, which is
//! Windows' own default on a fresh install, and a missing accent palette means Windows' default
//! blue. An app that refuses to draw because a registry value is absent would be a worse answer
//! than one drawn in the wrong appearance.
//!
//! The ordinary colours are not here: app.rs and icons.rs hold their own palettes. This module
//! only answers *which* accent the user chose. Windows' contrast colours are returned verbatim
//! because those are the user's accessibility choices.

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

/// The notification area follows Windows mode, even when app mode is pinned to the opposite theme.
pub fn is_light_taskbar() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
        .and_then(|key| key.get_value::<u32, _>("SystemUsesLightTheme"))
        .map(|v| v != 0)
        .unwrap_or(false)
}

/// Native contrast themes override branded icon colours. Return background/foreground in RGB.
pub fn contrast_colors() -> Option<([u8; 3], [u8; 3])> {
    use windows::Win32::UI::Accessibility::{HIGHCONTRASTW, HCF_HIGHCONTRASTON};
    use windows::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS};
    use windows::Win32::Graphics::Gdi::{GetSysColor, COLOR_WINDOW, COLOR_WINDOWTEXT};
    let mut contrast = HIGHCONTRASTW { cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32, ..Default::default() };
    let enabled = unsafe {
        SystemParametersInfoW(SPI_GETHIGHCONTRAST, contrast.cbSize, Some(&mut contrast as *mut _ as *mut std::ffi::c_void), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0))
    }.is_ok() && (contrast.dwFlags.0 & HCF_HIGHCONTRASTON.0) != 0;
    if !enabled { return None; }
    let rgb = |c: u32| [c as u8, (c >> 8) as u8, (c >> 16) as u8];
    Some(unsafe { (rgb(GetSysColor(COLOR_WINDOW)), rgb(GetSysColor(COLOR_WINDOWTEXT))) })
}

/// The colours of the contrast theme the user picked, when one is on — each one a role Windows
/// names, so the app can hand every role to the colour the user chose for it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ContrastPalette {
    /// Behind text.
    pub window: [u8; 3],
    /// Text.
    pub text: [u8; 3],
    /// A chosen item, and the text on it.
    pub highlight: [u8; 3],
    pub highlight_text: [u8; 3],
    /// Links — here, anything written in the accent.
    pub hotlight: [u8; 3],
    /// Disabled text.
    pub gray: [u8; 3],
    /// A button's face and its words.
    pub button_face: [u8; 3],
    pub button_text: [u8; 3],
}

/// The whole contrast palette, or `None` when no contrast theme is on.
pub fn contrast_palette() -> Option<ContrastPalette> {
    use windows::Win32::Graphics::Gdi::{
        GetSysColor, COLOR_BTNFACE, COLOR_BTNTEXT, COLOR_GRAYTEXT, COLOR_HIGHLIGHT,
        COLOR_HIGHLIGHTTEXT, COLOR_HOTLIGHT, COLOR_WINDOW, COLOR_WINDOWTEXT, SYS_COLOR_INDEX,
    };
    // The preview build can be shown a contrast theme without switching the whole machine to one:
    // Windows 11's «Acuático», the palette its own contrast settings open on.
    if cfg!(feature = "preview") && std::env::var_os("EM_PREVIEW_CONTRAST").is_some() {
        return Some(ContrastPalette {
            window: [0x20, 0x20, 0x20],
            text: [0xFF, 0xFF, 0xFF],
            highlight: [0x8E, 0xE3, 0xF0],
            highlight_text: [0x26, 0x3B, 0x50],
            hotlight: [0x75, 0xE9, 0xFC],
            gray: [0xA6, 0xA6, 0xA6],
            button_face: [0x20, 0x20, 0x20],
            button_text: [0xFF, 0xFF, 0xFF],
        });
    }
    contrast_colors()?;
    let rgb = |index: SYS_COLOR_INDEX| {
        let c = unsafe { GetSysColor(index) };
        [c as u8, (c >> 8) as u8, (c >> 16) as u8]
    };
    Some(ContrastPalette {
        window: rgb(COLOR_WINDOW),
        text: rgb(COLOR_WINDOWTEXT),
        highlight: rgb(COLOR_HIGHLIGHT),
        highlight_text: rgb(COLOR_HIGHLIGHTTEXT),
        hotlight: rgb(COLOR_HOTLIGHT),
        gray: rgb(COLOR_GRAYTEXT),
        button_face: rgb(COLOR_BTNFACE),
        button_text: rgb(COLOR_BTNTEXT),
    })
}
