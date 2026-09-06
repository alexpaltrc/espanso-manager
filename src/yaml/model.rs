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

//! The shape of `base.yml`, and the promise printed at the top of it.
//!
//! That banner says EspansoManager preserves any match it cannot show in its interface, and this
//! module is where that is kept or broken. Two mechanisms hold it:
//!
//! - `MatchEntry::Advanced` carries anything the form cannot represent — shell commands, regex
//!   triggers, per-app rules — as its original `Value`, untouched, and writes it back out
//!   unchanged. `is_safely_editable` is what the interface asks before offering to edit a row.
//! - `MatchFile::extra_top_level` does the same for keys beside `matches:` that we know nothing
//!   about, such as `global_vars`.
//!
//! Which is why a `matches:` that is neither a list nor absent is a hard error rather than an empty
//! file: taking it for "no expansions" and saving over it would delete everything the user had,
//! under a banner promising the opposite. An absent or null `matches:` is genuinely empty and
//! loads fine.

use crate::i18n::Strings;
use serde::{Deserialize, Serialize};
use serde_norway::{Mapping, Value};

/// A `vars:` entry inside a match, e.g. the "current date" variable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VarEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub var_type: String,
    #[serde(default, skip_serializing_if = "Mapping::is_empty")]
    pub params: Mapping,
}

impl VarEntry {
    pub fn param_str(&self, key: &str) -> Option<&str> {
        self.params.get(key)?.as_str()
    }
}

/// A match the GUI fully understands and can edit with a simple form.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SimpleMatch {
    pub trigger: String,
    pub replace: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vars: Vec<VarEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl SimpleMatch {
    /// If this match is exactly a "insert current date/time" expansion created by one of our
    /// presets (a single `date`-typed var, referenced verbatim as the whole replacement), returns
    /// that var. Used so editing a previously-created date expansion re-opens the preset picker
    /// instead of the raw-text editor.
    pub fn as_date_var(&self) -> Option<&VarEntry> {
        if self.vars.len() != 1 {
            return None;
        }
        let var = &self.vars[0];
        if var.var_type != "date" {
            return None;
        }
        if self.replace.trim() == format!("{{{{{}}}}}", var.name) {
            Some(var)
        } else {
            None
        }
    }
}

/// One entry of `matches:`. Anything the GUI doesn't fully understand (plural `triggers:`,
/// `regex:`, per-app conditionals, unknown extra fields, ...) is kept as an opaque, untouched
/// value so editing OTHER matches can never corrupt it.
#[derive(Debug, Clone)]
pub enum MatchEntry {
    Simple(SimpleMatch),
    Advanced(Value),
}

impl MatchEntry {
    /// Identity used for bookkeeping (folder assignment, selection, lookups). Never translated, so
    /// switching languages can't silently detach an expansion from its folder.
    ///
    /// Borrowed rather than cloned: the list re-derives itself from these on every rebuild, and at
    /// a few thousand expansions an allocation per entry per pass is real work for no gain.
    pub fn trigger_str(&self) -> &str {
        match self {
            MatchEntry::Simple(m) => m.trigger.as_str(),
            MatchEntry::Advanced(v) => v.get("trigger").and_then(Value::as_str).unwrap_or(""),
        }
    }

    /// Owned form of [`trigger_str`], for the places that need to keep it past the borrow.
    pub fn trigger_label(&self) -> String {
        self.trigger_str().to_string()
    }

    pub fn preview(&self, t: &'static Strings) -> String {
        match self {
            MatchEntry::Simple(m) => {
                if let Some(var) = m.as_date_var() {
                    // Shown as a worked example ("e.g. August 29, 2026") rather than as the
                    // strftime pattern that produced it: the pattern is an implementation detail
                    // that means nothing to the people this list is for.
                    let example = crate::datefmt::sample(
                        var.param_str("format").unwrap_or(""),
                        var.param_str("tz"),
                        super::presets::month_lang_of(var),
                    );
                    crate::i18n::fill(t.preview_date, &[("example", &example)])
                } else if !m.vars.is_empty() {
                    t.preview_advanced_vars.to_string()
                } else {
                    m.replace
                        .chars()
                        .map(|c| if c == '\n' { ' ' } else { c })
                        .collect()
                }
            }
            MatchEntry::Advanced(_) => t.preview_advanced.to_string(),
        }
    }

    /// Whether the simple form can safely rewrite this match's `replace`/`vars` without losing
    /// anything. A plain text match or one of our own date presets qualifies; a match with other
    /// kinds of `vars` (shell commands, clipboard, ...) round-trips fine when left untouched, but
    /// our form doesn't model those, so it must not be allowed to silently drop them on save.
    pub fn is_safely_editable(&self) -> bool {
        match self {
            MatchEntry::Simple(m) => m.vars.is_empty() || m.as_date_var().is_some(),
            MatchEntry::Advanced(_) => false,
        }
    }
}

/// Internal shape used only to classify+round-trip a `matches:` entry. `extra` catches any field
/// the GUI doesn't model; if it's non-empty after parsing, the entry is treated as Advanced.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SimpleMatchRaw {
    trigger: String,
    replace: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    vars: Vec<VarEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(flatten)]
    extra: Mapping,
}

fn classify(value: &Value) -> MatchEntry {
    if let Ok(raw) = serde_norway::from_value::<SimpleMatchRaw>(value.clone()) {
        if raw.extra.is_empty() {
            return MatchEntry::Simple(SimpleMatch {
                trigger: raw.trigger,
                replace: raw.replace,
                vars: raw.vars,
                label: raw.label,
            });
        }
    }
    MatchEntry::Advanced(value.clone())
}

fn to_value(entry: &MatchEntry) -> Value {
    match entry {
        MatchEntry::Simple(m) => {
            let raw = SimpleMatchRaw {
                trigger: m.trigger.clone(),
                replace: m.replace.clone(),
                vars: m.vars.clone(),
                label: m.label.clone(),
                extra: Mapping::new(),
            };
            serde_norway::to_value(&raw).expect("SimpleMatchRaw always serializes")
        }
        MatchEntry::Advanced(v) => v.clone(),
    }
}

/// A whole `*.yml` match file: our editable list of matches, plus any top-level keys we don't
/// touch (e.g. `global_vars:`, `imports:`) preserved exactly as read.
#[derive(Debug, Clone, Default)]
pub struct MatchFile {
    pub entries: Vec<MatchEntry>,
    pub extra_top_level: Mapping,
}

impl MatchFile {
    pub fn from_str(contents: &str) -> Result<Self, serde_norway::Error> {
        let root: Value = serde_norway::from_str(contents)?;
        let mut top = match root {
            Value::Mapping(m) => m,
            Value::Null => Mapping::new(),
            other => {
                // Not a mapping at all: treat the whole thing as unparseable rather than guessing.
                use serde::de::Error as _;
                return Err(serde_norway::Error::custom(format!(
                    "unexpected top-level shape in match file: {other:?}"
                )));
            }
        };

        let matches_key = Value::String("matches".to_string());
        let entries = match top.remove(&matches_key) {
            Some(Value::Sequence(seq)) => seq.iter().map(classify).collect(),
            // `matches:` with nothing under it, or no key at all. An empty file is a fine file.
            Some(Value::Null) | None => Vec::new(),
            // Anything else is refused rather than read as "no matches", for the same reason the
            // root shape above is. `remove` has already taken the value out, so it would not
            // survive in `extra_top_level` either: the next save would write `matches: []` over
            // whatever was there. Forgetting the leading `-` on a hand-edited entry is all it takes
            //
            //     matches:
            //       trigger: ":hi"
            //       replace: hola
            //
            // to turn every expansion in the file into a mapping — and the banner this very module
            // writes promises that anything it cannot show is preserved.
            Some(other) => {
                use serde::de::Error as _;
                return Err(serde_norway::Error::custom(format!(
                    "`matches:` is not a list: {other:?}"
                )));
            }
        };

        Ok(MatchFile {
            entries,
            extra_top_level: top,
        })
    }

    pub fn to_string_with_banner(&self) -> Result<String, serde_norway::Error> {
        let mut root = Mapping::new();
        let seq: Vec<Value> = self.entries.iter().map(to_value).collect();
        root.insert(Value::String("matches".to_string()), Value::Sequence(seq));
        for (k, v) in &self.extra_top_level {
            root.insert(k.clone(), v.clone());
        }
        let body = serde_norway::to_string(&Value::Mapping(root))?;
        Ok(format!(
            // Kept in English regardless of interface language: this is a comment inside a shared
            // config file that a colleague or a support engineer may open on any machine.
            "# Managed by EspansoManager — you can still edit this file by hand;\n\
             # EspansoManager preserves any match it cannot show in its interface.\n\
             # https://espanso.org/docs/\n\n{body}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_matches_key_is_an_empty_file_and_nothing_worse() {
        for text in ["matches:\n", "matches: []\n", "# nothing here\n", ""] {
            let mf = MatchFile::from_str(text)
                .unwrap_or_else(|e| panic!("{text:?} should load: {e}"));
            assert!(mf.entries.is_empty());
        }
    }

    #[test]
    fn matches_that_is_not_a_list_is_refused_instead_of_emptied() {
        // The leading `-` forgotten while hand-editing. This used to load as zero matches without
        // a word, and the next save wrote `matches: []` over it.
        let text = "matches:\n  trigger: \":hi\"\n  replace: hola\n";
        assert!(MatchFile::from_str(text).is_err());
        // A scalar is refused the same way.
        assert!(MatchFile::from_str("matches: hola\n").is_err());
    }

    #[test]
    fn a_list_still_loads_and_keeps_the_other_top_level_keys() {
        let text = "imports:\n  - other.yml\nmatches:\n  - trigger: \":hi\"\n    replace: hola\n";
        let mf = MatchFile::from_str(text).unwrap();
        assert_eq!(mf.entries.len(), 1);
        assert_eq!(mf.entries[0].trigger_str(), ":hi");
        assert!(mf
            .extra_top_level
            .contains_key(Value::String("imports".to_string())));
    }
}
