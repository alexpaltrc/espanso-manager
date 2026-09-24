//! Folder metadata and prefix changes, derived from the real triggers rather than a second setting.
//! Plans never change a file. AppState validates and saves them; the UI only previews them.
//! Matches the editor cannot safely represent are preserved and still reserve their triggers.

use crate::{settings::Settings, yaml::model::MatchEntry};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

#[derive(Default, Clone)]
pub struct FolderInfo {
    pub count: usize,
    pub prefixes: BTreeSet<String>,
}

pub fn prefix(trigger: &str) -> &str {
    let end = trigger.char_indices().find(|(_, c)| c.is_alphanumeric())
        .map_or(trigger.len(), |(i, _)| i);
    &trigger[..end]
}

/// Whether a trigger has a word a prefix could sit in front of. `":hola"` has one; `":--"`, `":_"`
/// and `":—"` do not.
///
/// A trigger made only of symbols has no separable prefix: nothing in it says where one would end.
/// Every prefix operation therefore skips it rather than guessing — replacing "its prefix" would
/// either leave nothing at all or staple a second prefix onto the whole trigger. Same rule as
/// [`crate::ui::edit_form::split_trigger`], which is what lets such a trigger be created.
pub fn has_word(trigger: &str) -> bool {
    trigger.chars().any(char::is_alphanumeric)
}

pub fn summarize(entries: &[MatchEntry], settings: &Settings) -> BTreeMap<String, FolderInfo> {
    let mut folders = BTreeMap::<String, FolderInfo>::new();
    for entry in entries.iter().filter(|e| e.is_safely_editable()) {
        if let Some(name) = settings.folder_of(entry.trigger_str()) {
            let info = folders.entry(name.to_owned()).or_default();
            info.count += 1;
            // Counted like any other expansion, but a symbol-only trigger contributes no prefix:
            // listing `:--` as a prefix would offer to change something the change then skips.
            if has_word(entry.trigger_str()) {
                info.prefixes.insert(prefix(entry.trigger_str()).to_owned());
            }
        }
    }
    folders
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rename {
    pub index: usize,
    pub old: String,
    pub new: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PrefixError {
    Invalid,
    Collision(String),
}

/// Checks the entire resulting namespace, including plural triggers on advanced matches.
/// Unrelated duplicate triggers in hand-edited YAML do not block a change elsewhere.
pub fn plan(entries: &[MatchEntry], settings: &Settings, folder: &str, new_prefix: &str)
    -> Result<Vec<Rename>, PrefixError>
{
    if new_prefix.is_empty() || new_prefix.chars().count() > 8
        || new_prefix.chars().any(|c| c.is_alphanumeric() || c.is_whitespace() || c.is_control()) {
        return Err(PrefixError::Invalid);
    }
    let mut renames = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        if !entry.is_safely_editable() || settings.folder_of(entry.trigger_str()) != Some(folder) {
            continue;
        }
        let old = entry.trigger_str();
        // Left exactly as it is, and deliberately not an error: one `:--` in a folder used to
        // refuse the whole folder's prefix change, in a message that said the trigger was empty.
        if !has_word(old) { continue; }
        let word = &old[prefix(old).len()..];
        let new = format!("{new_prefix}{word}");
        if new != old { renames.push(Rename { index, old: old.to_owned(), new }); }
    }
    let changed: HashSet<usize> = renames.iter().map(|r| r.index).collect();
    let mut reserved = HashSet::<String>::new();
    for (index, entry) in entries.iter().enumerate() {
        if changed.contains(&index) { continue; }
        if !entry.trigger_str().is_empty() { reserved.insert(entry.trigger_str().to_owned()); }
        if let MatchEntry::Advanced(value) = entry {
            if let Some(triggers) = value.get("triggers").and_then(|v| v.as_sequence()) {
                reserved.extend(triggers.iter().filter_map(|v| v.as_str()).map(str::to_owned));
            }
        }
    }
    for rename in &renames {
        if !reserved.insert(rename.new.clone()) { return Err(PrefixError::Collision(rename.new.clone())); }
    }
    Ok(renames)
}

/// Rebuilds assignments from a snapshot: chained renames must not overwrite another assignment.
pub fn apply_assignments(settings: &mut Settings, renames: &[Rename]) {
    let map: HashMap<&str, &str> = renames.iter().map(|r| (r.old.as_str(), r.new.as_str())).collect();
    settings.folder_by_trigger = settings.folder_by_trigger.iter().map(|(trigger, folder)| {
        (map.get(trigger.as_str()).copied().unwrap_or(trigger).to_owned(), folder.clone())
    }).collect();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yaml::model::SimpleMatch;
    fn entry(trigger: &str) -> MatchEntry {
        MatchEntry::Simple(SimpleMatch { trigger: trigger.into(), replace: "Texto 🥳".into(), vars: vec![], label: None })
    }
    fn settings() -> Settings {
        let mut settings = Settings::default();
        settings.set_folder(":hola", Some("A".into()));
        settings.set_folder("::adiós", Some("A".into()));
        settings.set_folder(":fuera", Some("B".into()));
        settings
    }
    #[test]
    fn scope_unicode_and_mixed_prefixes() {
        let entries = vec![entry(":hola"), entry("::adiós"), entry(":fuera")];
        let mut settings = settings();
        let info = summarize(&entries, &settings);
        assert_eq!(info["A"].count, 2);
        assert_eq!(info["A"].prefixes.len(), 2);
        let changes = plan(&entries, &settings, "A", "//").unwrap();
        assert_eq!(changes.iter().map(|r| r.new.as_str()).collect::<Vec<_>>(), ["//hola", "//adiós"]);
        apply_assignments(&mut settings, &changes);
        assert_eq!(settings.folder_of("//adiós"), Some("A"));
        assert_eq!(settings.folder_of(":fuera"), Some("B"));
        assert_eq!(settings.folder_of("::adiós"), None);
    }
    #[test]
    fn collision_outside_folder_and_advanced_aliases() {
        for extra in [entry(";hola"), MatchEntry::Advanced(serde_norway::from_str("triggers: [';hola', ':alias']\nreplace: advanced\n").unwrap())] {
            assert_eq!(plan(&[entry(":hola"), extra], &settings(), "A", ";"), Err(PrefixError::Collision(";hola".into())));
        }
    }
    #[test]
    fn collapsing_prefixes_cannot_merge_two_expansions() {
        let mut s = settings();
        s.set_folder("::hola", Some("A".into()));
        assert!(matches!(plan(&[entry(":hola"), entry("::hola")], &s, "A", ";"), Err(PrefixError::Collision(_))));
    }
    /// `:--` has no word, so there is nothing to re-prefix. It used to refuse the whole folder's
    /// change with a message saying the trigger was empty; now it simply keeps what it has, and
    /// the expansions around it are renamed as asked.
    #[test]
    fn a_symbol_only_trigger_is_skipped_rather_than_refusing_the_whole_folder() {
        let entries = vec![entry(":hola"), entry(":--"), entry(":_")];
        let mut settings = settings();
        settings.set_folder(":--", Some("A".into()));
        settings.set_folder(":_", Some("A".into()));
        let changes = plan(&entries, &settings, "A", ";").unwrap();
        assert_eq!(changes.iter().map(|r| r.new.as_str()).collect::<Vec<_>>(), [";hola"]);
        // Counted in the folder, but contributing no prefix to offer changing.
        let info = summarize(&entries, &settings);
        assert_eq!(info["A"].count, 3);
        assert_eq!(info["A"].prefixes.iter().map(String::as_str).collect::<Vec<_>>(), [":"]);
    }

    #[test]
    fn rejects_invalid_prefix_and_preserves_advanced_entries() {
        for p in ["", "texto", ": ", "\n", "123", ":::::::::"] {
            assert_eq!(plan(&[entry(":hola")], &settings(), "A", p), Err(PrefixError::Invalid));
        }
        let advanced = MatchEntry::Advanced(serde_norway::from_str("trigger: ':hola'\nreplace: text\nword: true\n").unwrap());
        assert!(plan(&[advanced], &settings(), "A", ";").unwrap().is_empty());
        assert!(plan(&[entry(":hola")], &settings(), "A", ":").unwrap().is_empty());
    }
}
