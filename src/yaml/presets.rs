//! The four "insert today's date" shapes the form offers, and the round trip between one of them
//! and the espanso variable that expresses it.
//!
//! The point of this module is that nobody using the app ever has to learn that espanso has a
//! variable syntax at all. `build_var` turns a choice into the YAML; `detect` reads the YAML back
//! and recognises the choice, so reopening an expansion shows the same radio button that made it.
//! Anything `detect` does not recognise is a custom format, and the form says so rather than
//! silently rounding it to the nearest preset.
//!
//! The formats are chrono's, because that is what espanso passes them to — and the ones that spell
//! a month out carry a language with them. See [`crate::datefmt`] for why that language has to be
//! stored in the file rather than read from the machine.

use super::model::VarEntry;
use crate::datefmt;
use crate::i18n::Lang;
use serde_norway::{Mapping, Value};

/// A built-in "insert current date/time" preset offered to the user, so they never have to know
/// espanso's variable syntax exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatePreset {
    IsoDateOnly,
    IsoDateTimeUtc,
    MonthDayYear,
    DayMonthYear,
}

impl DatePreset {
    pub const ALL: [DatePreset; 4] = [
        DatePreset::IsoDateOnly,
        DatePreset::IsoDateTimeUtc,
        DatePreset::MonthDayYear,
        DatePreset::DayMonthYear,
    ];

    pub fn label(self, t: &'static crate::i18n::Strings) -> &'static str {
        match self {
            DatePreset::IsoDateOnly => t.preset_date_only,
            DatePreset::IsoDateTimeUtc => t.preset_datetime_utc,
            DatePreset::MonthDayYear => t.preset_month_day_year,
            DatePreset::DayMonthYear => t.preset_day_month_year,
        }
    }

    pub fn format(self) -> &'static str {
        match self {
            DatePreset::IsoDateOnly => "%Y-%m-%d",
            DatePreset::IsoDateTimeUtc => "%Y-%m-%dT%H:%M:%SZ",
            // `%-d` drops the leading zero, so the ninth reads "August 9" rather than "August 09" —
            // written-out dates aren't zero-padded in any of the languages offered here.
            DatePreset::MonthDayYear => "%B %-d, %Y",
            DatePreset::DayMonthYear => "%-d %B %Y",
        }
    }

    pub fn tz(self) -> Option<&'static str> {
        match self {
            DatePreset::IsoDateTimeUtc => Some("UTC"),
            _ => None,
        }
    }

    /// What this preset produces right now, in `lang` — shown next to the option so the choice is
    /// made by looking at the result rather than by decoding a format string.
    pub fn sample(self, lang: Lang) -> String {
        datefmt::sample(self.format(), self.tz(), lang)
    }

    /// Detects which preset (if any) a given date var matches exactly, so re-editing a match
    /// created from a preset shows that preset selected instead of "personalizado".
    ///
    /// The month language is deliberately *not* part of the comparison: it's a separate choice on
    /// top of the preset, recovered by [`month_lang_of`].
    pub fn detect(var: &VarEntry) -> Option<DatePreset> {
        if var.var_type != "date" {
            return None;
        }
        Self::ALL.into_iter().find(|preset| {
            var.param_str("format") == Some(preset.format()) && var.param_str("tz") == preset.tz()
        })
    }

    pub fn build_var(self, var_name: &str, month_lang: Lang) -> VarEntry {
        build_date_var(var_name, self.format(), self.tz(), month_lang)
    }
}

/// The month language a stored date var was created with. Falls back to English, which is both this
/// app's default and what espanso itself does with a tag it doesn't recognise.
pub fn month_lang_of(var: &VarEntry) -> Lang {
    datefmt::lang_from_locale_tag(var.param_str("locale"))
}

/// Builds a custom (non-preset) date var from a user-chosen strftime format and optional IANA tz,
/// for the "avanzado" escape hatch inside the date picker.
pub fn build_custom_date_var(
    var_name: &str,
    format: &str,
    tz: Option<&str>,
    month_lang: Lang,
) -> VarEntry {
    build_date_var(
        var_name,
        format,
        tz.filter(|t| !t.trim().is_empty()),
        month_lang,
    )
}

/// The one place a `date` var is assembled, so every path writes the same shape.
///
/// `locale` is written only when the format actually spells something out in words. Espanso falls
/// back to the *machine's* locale when the parameter is missing, so a written-out month would come
/// out in a different language on a colleague's computer — pinning it here is what makes an
/// expansion mean the same thing everywhere it's shared. For a purely numeric format there is
/// nothing to pin, and the parameter is left out rather than adding noise to the file.
fn build_date_var(var_name: &str, format: &str, tz: Option<&str>, month_lang: Lang) -> VarEntry {
    let mut params = Mapping::new();
    params.insert(
        Value::String("format".to_string()),
        Value::String(format.to_string()),
    );
    if let Some(tz) = tz {
        params.insert(
            Value::String("tz".to_string()),
            Value::String(tz.to_string()),
        );
    }
    if datefmt::needs_locale(format) {
        params.insert(
            Value::String("locale".to_string()),
            Value::String(datefmt::locale_tag(month_lang).to_string()),
        );
    }
    VarEntry {
        name: var_name.to_string(),
        var_type: "date".to_string(),
        params,
    }
}
