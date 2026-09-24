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

//! Carrying expansions between machines: writing them out to a file, and reading somebody else's
//! file back in.
//!
//! The portable folder already travels as a whole, so this is not a backup mechanism — it is for
//! the narrower, more common thing: a colleague says "send me your signatures" and you want to hand
//! over some expansions without handing over your entire configuration.
//!
//! ## Why a separate format instead of espanso's own match file
//!
//! Two reasons. Folders are this app's own bookkeeping and never appear in espanso's YAML, so an
//! espanso match file could not carry them and the structure would arrive flattened. And a file
//! that *looks* like a config file invites being dropped into the config folder, where a
//! half-understood copy could shadow or conflict with the real one. This file announces itself with
//! its own top-level key, so it can never be mistaken for something espanso should load — and if it
//! ever does end up in the match folder, espanso finds no `matches:` key and quietly ignores it.
//!
//! It is still YAML, and still readable and hand-editable, which is the part that mattered.

use crate::yaml::model::{MatchEntry, SimpleMatch, VarEntry};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// Bumped only if the shape changes in a way an older build could not read.
const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct Export {
    /// Doubles as the marker that this is one of our files at all: a YAML document without this key
    /// is rejected before anything is imported, so pointing the importer at an arbitrary file gives
    /// a clear "this isn't one of ours" rather than a confusing parse error.
    pub espanso_manager_export: u32,
    #[serde(default)]
    pub expansions: Vec<ExportEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportEntry {
    pub trigger: String,
    pub replace: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vars: Vec<VarEntry>,
    /// The folder it was filed under, if any. Recreated on import, so a shared set arrives
    /// organised the way its author left it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}

/// What went wrong, in terms the caller can turn into a sentence for the user.
pub enum TransferError {
    /// The file could not be read or written at all.
    Io(String),
    /// The file is not YAML, or not our shape.
    NotOurs,
    /// Valid, but there was nothing in it to import.
    Empty,
}

/// Builds the export document from the expansions the app actually manages.
///
/// Only the ones the list shows are included. Anything the interface deliberately hides — shell
/// commands, regex triggers, per-app rules — stays behind: exporting something the user cannot see
/// would mean handing a colleague expansions neither of them has read, and a shell command is not a
/// thing to pass around by accident.
pub fn build_export(
    entries: &[MatchEntry],
    folder_of: impl Fn(&str) -> Option<String>,
) -> Export {
    let expansions = entries
        .iter()
        .filter(|e| e.is_safely_editable())
        .filter_map(|e| match e {
            MatchEntry::Simple(m) => Some(ExportEntry {
                trigger: m.trigger.clone(),
                replace: m.replace.clone(),
                vars: m.vars.clone(),
                folder: folder_of(&m.trigger),
            }),
            MatchEntry::Advanced(_) => None,
        })
        .collect();

    Export {
        espanso_manager_export: FORMAT_VERSION,
        expansions,
    }
}

/// The folders an export document covers, each with the number of expansions filed under it.
///
/// Ordered the way the rest of the app already orders folders — named ones as
/// [`crate::settings::Settings::all_folder_names`] returns them, which is a `BTreeSet` — with the
/// expansions that were never filed anywhere last. A `None` name is not "there are no folders": it
/// is that group, and choosing to send it or leave it behind is as real a choice as any folder's.
pub fn groups(export: &Export) -> Vec<(Option<String>, usize)> {
    let mut named: BTreeMap<&str, usize> = BTreeMap::new();
    let mut unfiled = 0usize;
    for e in &export.expansions {
        match e.folder.as_deref() {
            Some(name) => *named.entry(name).or_insert(0) += 1,
            None => unfiled += 1,
        }
    }

    let mut out: Vec<(Option<String>, usize)> = named
        .into_iter()
        .map(|(name, count)| (Some(name.to_string()), count))
        .collect();
    if unfiled > 0 {
        out.push((None, unfiled));
    }
    out
}

/// What an import would really bring, folder by folder, and how many it would leave out.
///
/// [`groups`] counts what is in the file, which for an export is the same thing as what travels.
/// For an import it is not: an expansion whose trigger is already in use is skipped by [`merge`],
/// and a picker that offered "Prompts · 3" and then added two was announcing an outcome that was
/// never going to happen. These counts are [`merge`]'s own rules applied ahead of time. A folder
/// with nothing left to add is not offered at all; its expansions are the second number, so the
/// report at the end can still say they were left out.
///
/// One case is counted by file order rather than by choice: a trigger that appears twice in the
/// file, in two folders, is counted in the first. Leaving that folder out would let the second copy
/// in after all — one more than the picker said, never one less.
pub fn import_groups(existing: &[MatchEntry], export: &Export) -> (Vec<(Option<String>, usize)>, usize) {
    let mut taken: HashSet<String> = existing.iter().map(|e| e.trigger_label()).collect();
    let mut named: BTreeMap<&str, usize> = BTreeMap::new();
    let mut unfiled = 0usize;
    let mut in_file: BTreeMap<Option<&str>, usize> = BTreeMap::new();

    for e in &export.expansions {
        *in_file.entry(e.folder.as_deref()).or_insert(0) += 1;
        let trigger = e.trigger.trim().to_string();
        if trigger.is_empty() || taken.contains(&trigger) || !accepted(&trigger, e) {
            continue;
        }
        taken.insert(trigger);
        match e.folder.as_deref() {
            Some(name) => *named.entry(name).or_insert(0) += 1,
            None => unfiled += 1,
        }
    }

    // Whatever sits in a folder that is not going to be offered is already known to be left out.
    let left_out = in_file
        .iter()
        .filter(|(folder, _)| match folder {
            Some(name) => !named.contains_key(name),
            None => unfiled == 0,
        })
        .map(|(_, count)| count)
        .sum();

    let mut out: Vec<(Option<String>, usize)> = named
        .into_iter()
        .map(|(name, count)| (Some(name.to_string()), count))
        .collect();
    if unfiled > 0 {
        out.push((None, unfiled));
    }
    (out, left_out)
}

/// Whether [`merge`] would take this entry, trigger aside.
fn accepted(trigger: &str, entry: &ExportEntry) -> bool {
    MatchEntry::Simple(SimpleMatch {
        trigger: trigger.to_string(),
        replace: entry.replace.clone(),
        vars: entry.vars.clone(),
        label: None,
    })
    .is_safely_editable()
}

/// Drops every expansion whose folder is not in `keep`.
///
/// `keep` holds the same values [`groups`] hands out, `None` included, so an empty `keep` keeps
/// nothing and a `keep` without `None` in it leaves the unfiled expansions behind. That is the
/// point: "everything except the ones I never sorted" has to be expressible.
pub fn retain_folders(mut export: Export, keep: &[Option<String>]) -> Export {
    export
        .expansions
        .retain(|e| keep.iter().any(|k| k.as_deref() == e.folder.as_deref()));
    export
}

pub fn write(path: &Path, export: &Export) -> Result<(), TransferError> {
    let body = serde_norway::to_string(export).map_err(|e| TransferError::Io(e.to_string()))?;
    let text = format!(
        // English on purpose: this file is made to be sent to someone else, whose copy of the app
        // may well be running in another language.
        "# Expansions exported from EspansoManager.\n\
         # Import them from Settings -> Export and import. They are added to whatever is\n\
         # already there; nothing you have is replaced.\n\n{body}"
    );
    std::fs::write(path, text).map_err(|e| TransferError::Io(e.to_string()))
}

pub fn read(path: &Path) -> Result<Export, TransferError> {
    let text = std::fs::read_to_string(path).map_err(|e| TransferError::Io(e.to_string()))?;
    let export: Export = serde_norway::from_str(&text).map_err(|_| TransferError::NotOurs)?;
    if export.espanso_manager_export == 0 {
        return Err(TransferError::NotOurs);
    }
    if export.expansions.is_empty() {
        return Err(TransferError::Empty);
    }
    Ok(export)
}

/// The result of merging an import into what is already there.
pub struct Merged {
    pub added: Vec<MatchEntry>,
    /// Folder to file each added expansion under, paired by trigger.
    pub folders: Vec<(String, Option<String>)>,
    /// Triggers that were left alone because that trigger was already in use.
    pub skipped: usize,
}

/// Merges an import into the existing expansions, **only ever adding**.
///
/// A trigger that is already in use is skipped rather than overwritten or renamed. Overwriting
/// would destroy something the user did not ask to lose; renaming would invent triggers nobody
/// chose and that nobody would remember to type. Skipping leaves the file in a state the user can
/// reason about, and the count of what was skipped is reported so they can go and rename by hand if
/// they actually wanted those.
///
/// Duplicates *within* the imported file are handled by the same rule, since each accepted trigger
/// joins the set as it goes.
///
/// Everything that arrives is held to the same standard as everything that leaves. The export side
/// refuses entries this app cannot display or edit; the import side had no such check, and it is
/// the side that reads files this app did not write. An entry that got in that way would go into
/// espanso's match file, where espanso would honour it, while the list here never showed it — a
/// trigger firing from a row nobody can see or delete.
pub fn merge(existing: &[MatchEntry], export: Export) -> Merged {
    let mut taken: HashSet<String> = existing.iter().map(|e| e.trigger_label()).collect();

    let mut added = Vec::new();
    let mut folders = Vec::new();
    let mut skipped = 0usize;

    for entry in export.expansions {
        let trigger = entry.trigger.trim().to_string();
        if trigger.is_empty() || taken.contains(&trigger) {
            skipped += 1;
            continue;
        }
        let candidate = MatchEntry::Simple(SimpleMatch {
            trigger: trigger.clone(),
            replace: entry.replace,
            vars: entry.vars,
            label: None,
        });
        if !candidate.is_safely_editable() {
            skipped += 1;
            continue;
        }
        // The trigger is claimed only once the entry is actually accepted. Claiming it earlier
        // would let a rejected entry shadow a good one further down the same file.
        taken.insert(trigger.clone());
        folders.push((trigger, entry.folder));
        added.push(candidate);
    }

    Merged {
        added,
        folders,
        skipped,
    }
}

/// A filename that says what the file is without the user having to type anything.
pub fn suggested_filename() -> &'static str {
    "expansiones-espansomanager.yml"
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_norway::{Mapping, Value};

    fn entry(trigger: &str, vars: Vec<VarEntry>) -> ExportEntry {
        ExportEntry {
            trigger: trigger.to_string(),
            replace: "texto".to_string(),
            vars,
            folder: None,
        }
    }

    /// A `shell` var is the canonical thing the form cannot model: the export side refuses it, so
    /// the import side must too.
    fn shell_var() -> VarEntry {
        let mut params = Mapping::new();
        params.insert(
            Value::String("cmd".to_string()),
            Value::String("echo hola".to_string()),
        );
        VarEntry {
            name: "salida".to_string(),
            var_type: "shell".to_string(),
            params,
        }
    }

    #[test]
    fn import_rejects_what_export_would_never_have_written() {
        let merged = merge(
            &[],
            Export {
                espanso_manager_export: FORMAT_VERSION,
                expansions: vec![entry(":hola", vec![]), entry(":shell", vec![shell_var()])],
            },
        );

        let triggers: Vec<&str> = merged.added.iter().map(|e| e.trigger_str()).collect();
        assert_eq!(triggers, [":hola"]);
        assert_eq!(merged.skipped, 1);
    }

    /// The rejected entry must not reserve its trigger on the way out: a good entry further down
    /// the same file has as much right to it as if the bad one had never been there. Checked on
    /// the contents, not just the trigger — the two entries here are deliberately named the same,
    /// so a test that only compared triggers would pass whichever of them survived.
    #[test]
    fn a_rejected_entry_does_not_shadow_a_later_good_one() {
        let merged = merge(
            &[],
            Export {
                espanso_manager_export: FORMAT_VERSION,
                expansions: vec![entry(":firma", vec![shell_var()]), entry(":firma", vec![])],
            },
        );

        assert_eq!(merged.added.len(), 1);
        assert_eq!(merged.added[0].trigger_str(), ":firma");
        assert!(
            merged.added[0].is_safely_editable(),
            "the one that got in must be the one the app can show"
        );
        assert_eq!(merged.skipped, 1);
    }

    /// The original rule, unchanged by the filter: a trigger already in the file is never
    /// overwritten.
    #[test]
    fn an_existing_trigger_is_still_left_alone() {
        let existing = vec![MatchEntry::Simple(SimpleMatch {
            trigger: ":firma".to_string(),
            replace: "la mía".to_string(),
            vars: vec![],
            label: None,
        })];
        let merged = merge(
            &existing,
            Export {
                espanso_manager_export: FORMAT_VERSION,
                expansions: vec![entry(":firma", vec![])],
            },
        );
        assert!(merged.added.is_empty());
        assert_eq!(merged.skipped, 1);
    }

    fn filed(trigger: &str, folder: Option<&str>) -> ExportEntry {
        ExportEntry {
            folder: folder.map(str::to_string),
            ..entry(trigger, Vec::new())
        }
    }

    fn doc(expansions: Vec<ExportEntry>) -> Export {
        Export {
            espanso_manager_export: FORMAT_VERSION,
            expansions,
        }
    }

    /// The picker offers what the import will add, not what the file holds: a trigger already in
    /// use is not counted, and a folder with nothing left to add is not offered but still counted
    /// as left out, so the report at the end can say so.
    #[test]
    fn an_import_offers_only_what_it_will_add() {
        let existing: Vec<MatchEntry> = [":a", ":c"]
            .iter()
            .map(|t| {
                MatchEntry::Simple(SimpleMatch {
                    trigger: t.to_string(),
                    replace: "ya está".to_string(),
                    vars: vec![],
                    label: None,
                })
            })
            .collect();
        let file = doc(vec![
            filed(":a", Some("Uno")),
            filed(":b", Some("Uno")),
            filed(":c", Some("Dos")),
            filed(":d", None),
        ]);
        let (groups, left_out) = import_groups(&existing, &file);
        assert_eq!(groups, vec![(Some("Uno".to_string()), 1), (None, 1)]);
        assert_eq!(left_out, 1);
    }

    /// Named folders in name order, the unfiled ones last however many there are, and the counts
    /// per folder rather than a bare list of names — the picker shows those counts.
    #[test]
    fn groups_are_counted_and_ordered_with_the_unfiled_last() {
        let export = doc(vec![
            filed("a", Some("Trabajo")),
            filed("b", None),
            filed("c", Some("Firmas")),
            filed("d", Some("Trabajo")),
            filed("e", None),
        ]);

        assert_eq!(
            groups(&export),
            vec![
                (Some("Firmas".to_string()), 1),
                (Some("Trabajo".to_string()), 2),
                (None, 2),
            ]
        );
    }

    /// No unfiled expansions means no unfiled row at all, rather than one reading zero.
    #[test]
    fn no_unfiled_group_when_everything_has_a_folder() {
        let export = doc(vec![filed("a", Some("Trabajo"))]);
        assert_eq!(groups(&export), vec![(Some("Trabajo".to_string()), 1)]);
    }

    #[test]
    fn retain_folders_keeps_only_what_was_chosen() {
        let export = doc(vec![
            filed("a", Some("Trabajo")),
            filed("b", None),
            filed("c", Some("Firmas")),
        ]);

        let kept = retain_folders(export, &[Some("Firmas".to_string())]);
        let triggers: Vec<&str> = kept.expansions.iter().map(|e| e.trigger.as_str()).collect();
        assert_eq!(triggers, vec!["c"]);
    }

    /// The unfiled group is selectable on its own, and picking the named folders leaves it out —
    /// the case a naive "no folder means don't filter" implementation gets backwards.
    #[test]
    fn the_unfiled_group_is_a_choice_of_its_own() {
        let export = doc(vec![filed("a", Some("Trabajo")), filed("b", None)]);

        let only_unfiled = retain_folders(export, &[None]);
        let triggers: Vec<&str> = only_unfiled
            .expansions
            .iter()
            .map(|e| e.trigger.as_str())
            .collect();
        assert_eq!(triggers, vec!["b"]);

        let export = doc(vec![filed("a", Some("Trabajo")), filed("b", None)]);
        let only_named = retain_folders(export, &[Some("Trabajo".to_string())]);
        let triggers: Vec<&str> = only_named
            .expansions
            .iter()
            .map(|e| e.trigger.as_str())
            .collect();
        assert_eq!(triggers, vec!["a"]);
    }
}
