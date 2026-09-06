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

//! A miniature date formatter, used *only* to show the user a live example of what a date
//! expansion will produce.
//!
//! Espanso does the real formatting at typing time (chrono, with the locale tables it embeds); this
//! module exists so the edit form and the list can show "August 29, 2026" instead of the
//! `%B %-d, %Y` incantation that produced it. Nobody should have to learn strftime to know what
//! they just created.
//!
//! Everything here mirrors what espanso actually emits: the month and weekday names below were read
//! back out of a running espanso, one locale and one name at a time, rather than transcribed from a
//! reference — so the preview and the expansion agree letter for letter, accents and all.
//!
//! Only the specifiers espanso's own presets use, plus the handful anyone is likely to reach for in
//! a custom format, are understood. Anything else is left in the text exactly as written, so an
//! unknown code shows up as itself instead of being quietly guessed at.

use crate::i18n::Lang;

/// The BCP-47 tag written into the expansion's `locale` parameter.
///
/// Espanso accepts the hyphenated form (`es-ES`) and silently falls back to English for the
/// underscored one (`es_ES`), which is the sort of thing that looks like it works right up until
/// somebody's date comes out in the wrong language. Verified against espanso 2.4.0 directly.
pub fn locale_tag(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "en-US",
        Lang::Es => "es-ES",
        Lang::Fil => "fil-PH",
        Lang::Hi => "hi-IN",
    }
}

/// Reverse of [`locale_tag`], for re-opening an expansion that was created earlier. An unknown or
/// missing tag means English, which is both our default and espanso's fallback.
pub fn lang_from_locale_tag(tag: Option<&str>) -> Lang {
    match tag {
        Some("es-ES") => Lang::Es,
        Some("fil-PH") => Lang::Fil,
        Some("hi-IN") => Lang::Hi,
        _ => Lang::En,
    }
}

/// True if `format` contains anything whose spelling depends on the language, which is exactly when
/// the "month language" choice is worth showing (and worth writing into the file).
pub fn needs_locale(format: &str) -> bool {
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            continue;
        }
        // Step over a padding modifier so `%-d`-style codes are read correctly.
        let mut next = chars.next();
        if matches!(next, Some('-') | Some('_') | Some('0')) {
            next = chars.next();
        }
        // The first seven spell something out: month, weekday, meridiem.
        //
        // The last five are the composites, and they are the ones that were missing. chrono does
        // not build them from fixed pieces — it looks the whole pattern up in the locale
        // (`d_t_fmt`, `d_fmt`, `t_fmt`, `t_fmt_ampm`), so what they mean changes with the language
        // and not merely how it is spelled. `%x` is the sharp one: `%m/%d/%Y` in en-US and
        // `%d/%m/%y` in es-ES, so the same expansion reads as a different *date* on a colleague's
        // machine. `%X` is a 12-hour clock in en-US and a 24-hour one in es-ES, and `%r` — which
        // has no 12-hour form in es-ES at all — falls back to 24-hour there. `%c` and `%v` also
        // carry a written month or weekday inside them.
        //
        // Anything else is left alone, `%%` included: a doubled percent is a literal one, and the
        // letter after it is ordinary text that no locale touches.
        if matches!(
            next,
            Some('B' | 'b' | 'h' | 'A' | 'a' | 'p' | 'P' | 'c' | 'v' | 'r' | 'x' | 'X')
        ) {
            return true;
        }
    }
    false
}

const MONTHS_EN: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September",
    "October", "November", "December",
];
const MONTHS_ES: [&str; 12] = [
    "enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre",
    "octubre", "noviembre", "diciembre",
];
const MONTHS_FIL: [&str; 12] = [
    "Enero", "Pebrero", "Marso", "Abril", "Mayo", "Hunyo", "Hulyo", "Agosto", "Setyembre",
    "Oktubre", "Nobyembre", "Disyembre",
];
const MONTHS_HI: [&str; 12] = [
    "जनवरी", "फ़रवरी", "मार्च", "अप्रैल", "मई", "जून", "जुलाई", "अगस्त", "सितंबर", "अक्तूबर",
    "नवंबर", "दिसंबर",
];

const MONTHS_SHORT_EN: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MONTHS_SHORT_ES: [&str; 12] = [
    "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
];
const MONTHS_SHORT_FIL: [&str; 12] = [
    "Ene", "Peb", "Mar", "Abr", "May", "Hun", "Hul", "Ago", "Set", "Okt", "Nob", "Dis",
];
const MONTHS_SHORT_HI: [&str; 12] = [
    "जन॰", "फ़र॰", "मार्च", "अप्रैल", "मई", "जून", "जुल॰", "अग॰", "सित॰", "अक्तू॰", "नव॰", "दिस॰",
];

/// Monday first, matching the order [`Now::weekday_index`] produces.
const DAYS_EN: [&str; 7] = [
    "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday",
];
const DAYS_ES: [&str; 7] = [
    "lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo",
];
const DAYS_FIL: [&str; 7] = [
    "Lunes", "Martes", "Miyerkoles", "Huwebes", "Biyernes", "Sabado", "Linggo",
];
const DAYS_HI: [&str; 7] = [
    "सोमवार", "मंगलवार", "बुधवार", "गुरुवार", "शुक्रवार", "शनिवार", "रविवार",
];

const DAYS_SHORT_EN: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const DAYS_SHORT_ES: [&str; 7] = ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"];
const DAYS_SHORT_FIL: [&str; 7] = ["Lun", "Mar", "Miy", "Huw", "Biy", "Sab", "Lin"];
const DAYS_SHORT_HI: [&str; 7] = ["सोम", "मंगल", "बुध", "गुरु", "शुक्र", "शनि", "रवि"];

/// AM and PM, which are neither of those words outside English and in Spanish are no word at all:
/// a language that tells the time on a 24-hour clock has nothing to put there, so espanso prints
/// an empty string for `%p`. Showing "PM" instead would be the preview inventing a value the
/// expansion will never produce.
///
/// Unlike the names above these were not read back out of a running espanso. They are
/// `pure-rust-locales` 0.8.2 `LC_TIME::AM_PM` — the table chrono's `format_localized` reads, and so
/// the one espanso's own output comes from. What says it is the right source is that every month
/// and weekday table above matches that same crate letter for letter, accents and Devanagari and
/// all.
const AM_PM_EN: [&str; 2] = ["AM", "PM"];
const AM_PM_ES: [&str; 2] = ["", ""];
const AM_PM_FIL: [&str; 2] = ["N.U.", "N.H."];
const AM_PM_HI: [&str; 2] = ["पूर्वाह्न", "अपराह्न"];

fn months(lang: Lang, short: bool) -> &'static [&'static str; 12] {
    match (lang, short) {
        (Lang::En, false) => &MONTHS_EN,
        (Lang::Es, false) => &MONTHS_ES,
        (Lang::Fil, false) => &MONTHS_FIL,
        (Lang::Hi, false) => &MONTHS_HI,
        (Lang::En, true) => &MONTHS_SHORT_EN,
        (Lang::Es, true) => &MONTHS_SHORT_ES,
        (Lang::Fil, true) => &MONTHS_SHORT_FIL,
        (Lang::Hi, true) => &MONTHS_SHORT_HI,
    }
}

fn days(lang: Lang, short: bool) -> &'static [&'static str; 7] {
    match (lang, short) {
        (Lang::En, false) => &DAYS_EN,
        (Lang::Es, false) => &DAYS_ES,
        (Lang::Fil, false) => &DAYS_FIL,
        (Lang::Hi, false) => &DAYS_HI,
        (Lang::En, true) => &DAYS_SHORT_EN,
        (Lang::Es, true) => &DAYS_SHORT_ES,
        (Lang::Fil, true) => &DAYS_SHORT_FIL,
        (Lang::Hi, true) => &DAYS_SHORT_HI,
    }
}

/// The meridiem for one language, in the case `%p` uses. `%P` lowercases whatever this returns,
/// exactly as chrono does — which for Devanagari, having no case, changes nothing.
fn meridiem(lang: Lang, is_pm: bool) -> &'static str {
    let table = match lang {
        Lang::En => &AM_PM_EN,
        Lang::Es => &AM_PM_ES,
        Lang::Fil => &AM_PM_FIL,
        Lang::Hi => &AM_PM_HI,
    };
    table[usize::from(is_pm)]
}

/// A moment in time, broken into the fields a format string can ask for.
#[derive(Clone, Copy)]
pub struct Now {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    /// Sunday = 0, as Windows reports it.
    pub day_of_week: u16,
}

impl Now {
    pub fn local() -> Self {
        Self::from_systemtime(unsafe {
            windows::Win32::System::SystemInformation::GetLocalTime()
        })
    }

    pub fn utc() -> Self {
        Self::from_systemtime(unsafe {
            windows::Win32::System::SystemInformation::GetSystemTime()
        })
    }

    fn from_systemtime(st: windows::Win32::Foundation::SYSTEMTIME) -> Self {
        Self {
            year: st.wYear,
            month: st.wMonth,
            day: st.wDay,
            hour: st.wHour,
            minute: st.wMinute,
            second: st.wSecond,
            day_of_week: st.wDayOfWeek,
        }
    }

    /// Monday = 0, to index the name tables above.
    fn weekday_index(self) -> usize {
        ((self.day_of_week + 6) % 7) as usize
    }

    fn month_index(self) -> usize {
        (self.month.clamp(1, 12) - 1) as usize
    }
}

/// How a number should be padded, from the flag between `%` and the letter.
#[derive(Clone, Copy, PartialEq)]
enum Pad {
    Zero,
    None,
    Space,
}

fn pad2(out: &mut String, value: u16, pad: Pad) {
    match pad {
        Pad::None => out.push_str(&value.to_string()),
        Pad::Space if value < 10 => {
            out.push(' ');
            out.push_str(&value.to_string());
        }
        _ if value < 10 => {
            out.push('0');
            out.push_str(&value.to_string());
        }
        _ => out.push_str(&value.to_string()),
    }
}

/// Renders `format` for the given moment, in the given language.
///
/// Unknown specifiers are copied through untouched (`%Q` stays `%Q`) rather than dropped or
/// guessed: a preview that quietly invents a value would be worse than one that visibly admits it
/// doesn't know this code.
pub fn format(format: &str, now: Now, lang: Lang) -> String {
    let mut out = String::with_capacity(format.len() + 16);
    let mut chars = format.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let mut pad = Pad::Zero;
        match chars.peek() {
            Some('-') => {
                pad = Pad::None;
                chars.next();
            }
            Some('_') => {
                pad = Pad::Space;
                chars.next();
            }
            Some('0') => {
                chars.next();
            }
            _ => {}
        }
        let Some(spec) = chars.next() else {
            out.push('%');
            break;
        };
        match spec {
            'Y' => out.push_str(&now.year.to_string()),
            'y' => pad2(&mut out, now.year % 100, pad),
            'm' => pad2(&mut out, now.month, pad),
            'd' => pad2(&mut out, now.day, pad),
            'e' => pad2(&mut out, now.day, Pad::Space),
            'H' => pad2(&mut out, now.hour, pad),
            'I' => {
                // A 12-hour clock has no zero: midnight and noon are both "12".
                let on_the_dial = now.hour % 12;
                pad2(&mut out, if on_the_dial == 0 { 12 } else { on_the_dial }, pad);
            }
            'M' => pad2(&mut out, now.minute, pad),
            'S' => pad2(&mut out, now.second, pad),
            'B' => out.push_str(months(lang, false)[now.month_index()]),
            'b' | 'h' => out.push_str(months(lang, true)[now.month_index()]),
            'A' => out.push_str(days(lang, false)[now.weekday_index()]),
            'a' => out.push_str(days(lang, true)[now.weekday_index()]),
            'p' => out.push_str(meridiem(lang, now.hour >= 12)),
            'P' => out.extend(meridiem(lang, now.hour >= 12).chars().flat_map(char::to_lowercase)),
            'F' => {
                out.push_str(&now.year.to_string());
                out.push('-');
                pad2(&mut out, now.month, Pad::Zero);
                out.push('-');
                pad2(&mut out, now.day, Pad::Zero);
            }
            'T' => {
                pad2(&mut out, now.hour, Pad::Zero);
                out.push(':');
                pad2(&mut out, now.minute, Pad::Zero);
                out.push(':');
                pad2(&mut out, now.second, Pad::Zero);
            }
            '%' => out.push('%'),
            other => {
                // Not a code we model — put it back exactly as it was written.
                out.push('%');
                if pad == Pad::None {
                    out.push('-');
                } else if pad == Pad::Space {
                    out.push('_');
                }
                out.push(other);
            }
        }
    }
    out
}

/// Convenience wrapper matching how a date expansion is actually configured: a format, an optional
/// time zone (only `UTC` is offered by the presets), and the language its month names use.
pub fn sample(fmt: &str, tz: Option<&str>, lang: Lang) -> String {
    let now = if tz.is_some_and(|tz| tz.eq_ignore_ascii_case("UTC")) {
        Now::utc()
    } else {
        Now::local()
    };
    format(fmt, now, lang)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-09-05, a Saturday. The hour is what each test varies.
    fn at(hour: u16) -> Now {
        Now {
            year: 2026,
            month: 9,
            day: 5,
            hour,
            minute: 20,
            second: 0,
            day_of_week: 6,
        }
    }

    #[test]
    fn the_meridiem_is_the_one_espanso_will_print() {
        // pure-rust-locales LC_TIME::AM_PM, which is where chrono reads it and so where espanso's
        // own output comes from.
        assert_eq!(format("%p", at(15), Lang::En), "PM");
        assert_eq!(format("%p", at(15), Lang::Fil), "N.H.");
        assert_eq!(format("%p", at(15), Lang::Hi), "अपराह्न");
        assert_eq!(format("%p", at(9), Lang::En), "AM");
        assert_eq!(format("%p", at(9), Lang::Fil), "N.U.");
        assert_eq!(format("%p", at(9), Lang::Hi), "पूर्वाह्न");
    }

    #[test]
    fn spanish_has_no_meridiem_at_all() {
        // Not an oversight: es_ES leaves AM_PM empty, so espanso writes nothing there. A preview
        // showing "PM" would be promising a word that never appears.
        assert_eq!(format("%I:%M %p", at(15), Lang::Es), "03:20 ");
        assert_eq!(format("%p", at(9), Lang::Es), "");
    }

    #[test]
    fn the_lowercase_form_lowercases_the_language_own_word() {
        assert_eq!(format("%P", at(15), Lang::En), "pm");
        assert_eq!(format("%P", at(15), Lang::Fil), "n.h.");
        // Devanagari has no case, so this one comes out unchanged.
        assert_eq!(format("%P", at(15), Lang::Hi), "अपराह्न");
    }

    #[test]
    fn noon_and_midnight_fall_on_the_right_side() {
        assert_eq!(format("%p", at(12), Lang::En), "PM");
        assert_eq!(format("%p", at(0), Lang::En), "AM");
        assert_eq!(format("%I", at(12), Lang::En), "12");
        assert_eq!(format("%I", at(0), Lang::En), "12");
    }

    #[test]
    fn a_meridiem_still_asks_for_a_locale() {
        // It is language-dependent after all, so the `locale:` key has to be written — dropping it
        // would let the reader's own machine decide, which is what that key exists to prevent.
        assert!(needs_locale("%I:%M %p"));
        assert!(needs_locale("%P"));
    }
}

#[cfg(test)]
mod locale_tests {
    use super::*;

    #[test]
    fn the_composites_ask_for_a_locale_too() {
        // chrono looks each of these up whole in the locale tables, so the language decides what
        // they mean and not just how they are spelled.
        for fmt in ["%c", "%v", "%r", "%x", "%X"] {
            assert!(needs_locale(fmt), "{fmt} should need a locale");
        }
    }

    #[test]
    fn a_purely_numeric_format_still_asks_for_none() {
        for fmt in ["%Y-%m-%d", "%d/%m/%Y %H:%M", "%F %T", "%H:%M:%S", "%y%j"] {
            assert!(!needs_locale(fmt), "{fmt} should not need a locale");
        }
    }

    #[test]
    fn a_doubled_percent_is_a_literal_and_hides_nothing() {
        // `%%B` is a per-cent sign followed by the letter B, not a month name.
        assert!(!needs_locale("%%B"));
        assert!(!needs_locale("100%% %H:%M"));
        assert!(needs_locale("%%%B"));
    }

    #[test]
    fn a_padding_flag_does_not_hide_the_letter_behind_it() {
        assert!(needs_locale("%-d %B %Y"));
        assert!(needs_locale("%_A"));
        assert!(!needs_locale("%-d/%-m/%Y"));
    }
}
