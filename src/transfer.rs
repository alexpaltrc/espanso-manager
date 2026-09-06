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
use std::collections::HashSet;
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
}
