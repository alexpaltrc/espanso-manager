//! Reading and writing `base.yml` — the user's own file, which espanso is watching at the same
//! time.
//!
//! A save is: validate, serialize, re-parse the result as a self-check, back the old file up, write
//! to a scratch file beside it, flush, then rename over the top. The rename is what makes it safe
//! against espanso opening the file mid-write; the flush is what makes it safe against the power
//! going out, since NTFS journals the directory entry and not the contents.
//!
//! **Nothing in this module may write to the console.** There is none: the `windows_subsystem`
//! attribute at the top of `main.rs` builds this binary without one, so an `eprintln!` here is not
//! a quiet log, it is a failure nobody will ever be told about. Everything that degrades on the
//! way to a save that still went through is carried back in `SaveWarnings` for the caller to put
//! on screen. That rule has been broken three separate times; if you are about to report something
//! from here, add a field.

use super::model::{MatchEntry, MatchFile};
use crate::i18n::{fill, Strings};
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub enum ValidationIssue {
    EmptyTrigger,
    DuplicateTrigger(String),
}

impl ValidationIssue {
    pub fn friendly_message(&self, t: &'static Strings) -> String {
        match self {
            ValidationIssue::EmptyTrigger => t.err_validation_empty.to_string(),
            ValidationIssue::DuplicateTrigger(trigger) => {
                fill(t.err_validation_duplicate, &[("name", trigger)])
            }
        }
    }
}

pub fn validate(mf: &MatchFile) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for entry in &mf.entries {
        if let MatchEntry::Simple(m) = entry {
            if m.trigger.trim().is_empty() {
                issues.push(ValidationIssue::EmptyTrigger);
                continue;
            }
            if !seen.insert(m.trigger.as_str()) {
                issues.push(ValidationIssue::DuplicateTrigger(m.trigger.clone()));
            }
        }
    }
    issues
}

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Parse(serde_norway::Error),
}

impl LoadError {
    pub fn friendly_message(&self, t: &'static Strings) -> String {
        match self {
            LoadError::Io(e) => fill(t.err_read_file, &[("err", &e.to_string())]),
            LoadError::Parse(e) => fill(t.err_parse_yaml, &[("err", &e.to_string())]),
        }
    }
}

pub fn load(path: &Path) -> Result<MatchFile, LoadError> {
    let contents = std::fs::read_to_string(path).map_err(LoadError::Io)?;
    MatchFile::from_str(&contents).map_err(LoadError::Parse)
}

#[derive(Debug)]
pub enum SaveError {
    Validation(Vec<ValidationIssue>),
    /// Our own serialization didn't round-trip through the parser — a bug on our side, not the
    /// user's. Better to refuse the write than risk corrupting their file.
    SelfCheckFailed(serde_norway::Error),
    Io(std::io::Error),
}

impl SaveError {
    pub fn friendly_message(&self, t: &'static Strings) -> String {
        match self {
            SaveError::Validation(issues) => issues
                .iter()
                .map(|i| i.friendly_message(t))
                .collect::<Vec<_>>()
                .join("\n"),
            SaveError::SelfCheckFailed(e) => fill(t.err_self_check, &[("err", &e.to_string())]),
            SaveError::Io(e) => fill(t.err_write_file, &[("err", &e.to_string())]),
        }
    }
}

/// What went wrong on the way to a save that still went through.
///
/// Neither of these is a reason to throw the user's edit away, and neither ever was. But both used
/// to be announced with `eprintln!`, into a console this binary does not have — see the rule at the
/// top of this module — so those lines went to a handle nobody can read. The user was left
/// believing a backup exists, and the backup is precisely what they reach for when something has
/// gone wrong.
#[derive(Debug, Default)]
pub struct SaveWarnings {
    /// The previous file could not be copied into `backups/` before being replaced.
    pub backup: Option<std::io::Error>,
    /// The bytes were written but Windows would not confirm they reached the disk. Expected on a
    /// USB stick or a network share, which this app is carried on.
    pub flush: Option<std::io::Error>,
}

impl SaveWarnings {
    /// The warnings as one block of text, in the order they happened, or `None` if there are none.
    pub fn message(&self, t: &'static Strings) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();
        if let Some(e) = &self.backup {
            parts.push(fill(t.save_backup_failed, &[("err", &e.to_string())]));
        }
        if let Some(e) = &self.flush {
            parts.push(fill(t.save_flush_failed, &[("err", &e.to_string())]));
        }
        (!parts.is_empty()).then(|| parts.join("\n\n"))
    }
}

/// Validates, serializes, re-parses as a self-check, backs up the previous file, and atomically
/// replaces it. Never truncates the live file in place, and never leaves a half-written one:
/// the bytes are on the disk before the rename that makes them the expansions file.
///
/// Returns what degraded on the way, for the caller to put on screen.
pub fn save(path: &Path, mf: &MatchFile, backups_dir: &Path) -> Result<SaveWarnings, SaveError> {
    let issues = validate(mf);
    if !issues.is_empty() {
        return Err(SaveError::Validation(issues));
    }

    let rendered = mf
        .to_string_with_banner()
        .map_err(SaveError::SelfCheckFailed)?;
    MatchFile::from_str(&rendered).map_err(SaveError::SelfCheckFailed)?;

    let mut warnings = SaveWarnings::default();

    if path.exists() {
        if let Err(e) = backup(path, backups_dir, &mut warnings) {
            warnings.backup = Some(e);
        }
    }

    // The rename is what makes this safe against a reader — espanso is watching this folder and
    // may open the file at any instant — but it is not, on its own, safe against the power going
    // out. NTFS journals the directory entry, not the file's contents, so a rename can survive a
    // hard reset while the bytes it points at have not been written yet: the expansions file
    // comes back empty. The flush is what closes that window.
    //
    // A failed flush is not a reason to throw the edit away, though. On a USB stick or a network
    // share — and this app is carried on both — FlushFileBuffers can refuse where the write
    // itself was fine. Same policy as the backup above: say so and carry on.
    let tmp_path = path.with_extension("yml.tmp");
    {
        let mut file = std::fs::File::create(&tmp_path).map_err(SaveError::Io)?;
        file.write_all(rendered.as_bytes()).map_err(SaveError::Io)?;
        if let Err(e) = file.sync_all() {
            warnings.flush = Some(e);
        }
    } // The handle closes here: renaming a file still open can fail with a sharing violation.

    // And if the rename fails, the temporary file does not stay behind. It would be sitting in
    // espanso's own match folder, which espanso reads.
    if let Err(e) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(SaveError::Io(e));
    }
    Ok(warnings)
}

fn backup(
    path: &Path,
    backups_dir: &Path,
    warnings: &mut SaveWarnings,
) -> std::io::Result<()> {
    std::fs::create_dir_all(backups_dir)?;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "base.yml".to_string());
    let backup_path: PathBuf = backups_dir.join(format!("{file_name}.{ts}.bak"));
    std::fs::copy(path, &backup_path)?;
    // The backup is the only remaining copy of the previous file the moment the rename above
    // lands, so it gets the same flush — and reopened for writing, because FlushFileBuffers needs
    // GENERIC_WRITE and a read handle would be refused outright. Recorded rather than returned:
    // failing here must not skip the pruning below, or the 20-file cap quietly stops holding.
    //
    // It goes into `flush` and not into `backup`, because the backup exists — `copy` above
    // succeeded. What is unconfirmed is whether it reached the disk, which is word for word what
    // that warning says. `get_or_insert` so the live file's own flush, a few lines later on the
    // same drive and for the same reason, is the one the user is told about if both fail.
    if let Err(e) = std::fs::OpenOptions::new()
        .write(true)
        .open(&backup_path)
        .and_then(|f| f.sync_all())
    {
        warnings.flush.get_or_insert(e);
    }
    prune_old_backups(backups_dir, 20)
}

fn prune_old_backups(backups_dir: &Path, keep: usize) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(backups_dir)?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
    if entries.len() > keep {
        for e in &entries[..entries.len() - keep] {
            let _ = std::fs::remove_file(e.path());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yaml::model::SimpleMatch;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("espansomanager-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn one_match(trigger: &str) -> MatchFile {
        MatchFile {
            entries: vec![MatchEntry::Simple(SimpleMatch {
                trigger: trigger.to_string(),
                replace: "hola".to_string(),
                vars: vec![],
                label: None,
            })],
            extra_top_level: Default::default(),
        }
    }

    #[test]
    fn a_save_leaves_a_readable_file_and_no_scratch_file_beside_it() {
        let dir = scratch("save");
        let path = dir.join("base.yml");

        save(&path, &one_match(":uno"), &dir.join("backups")).unwrap();

        let reloaded = load(&path).unwrap();
        assert_eq!(reloaded.entries.len(), 1);
        assert_eq!(reloaded.entries[0].trigger_str(), ":uno");
        // The temporary file the atomic replace goes through must never be left in the folder
        // espanso watches.
        assert!(!path.with_extension("yml.tmp").exists());
    }

    #[test]
    fn saving_over_an_existing_file_replaces_it_and_keeps_a_backup() {
        let dir = scratch("backup");
        let path = dir.join("base.yml");
        let backups = dir.join("backups");

        save(&path, &one_match(":uno"), &backups).unwrap();
        save(&path, &one_match(":dos"), &backups).unwrap();

        assert_eq!(load(&path).unwrap().entries[0].trigger_str(), ":dos");
        assert!(!path.with_extension("yml.tmp").exists());

        let kept: Vec<_> = std::fs::read_dir(&backups).unwrap().flatten().collect();
        assert_eq!(kept.len(), 1, "the first version should have been kept");
        let previous = std::fs::read_to_string(kept[0].path()).unwrap();
        assert!(previous.contains(":uno"), "backup holds the previous file");
    }

    /// Validation runs before anything touches the disk, so a rejected save leaves the stored file
    /// exactly as it was.
    #[test]
    fn a_rejected_save_does_not_touch_the_stored_file() {
        let dir = scratch("validation");
        let path = dir.join("base.yml");
        let backups = dir.join("backups");
        save(&path, &one_match(":uno"), &backups).unwrap();
        let before = std::fs::read_to_string(&path).unwrap();

        let mut duplicated = one_match(":dup");
        duplicated.entries.push(duplicated.entries[0].clone());
        let result = save(&path, &duplicated, &backups);

        assert!(matches!(result, Err(SaveError::Validation(_))));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        assert!(!path.with_extension("yml.tmp").exists());
    }

    #[test]
    fn a_save_with_nothing_to_report_reports_nothing() {
        let dir = scratch("quiet");
        let path = dir.join("base.yml");
        let warnings = save(&path, &one_match(":uno"), &dir.join("backups")).unwrap();
        assert!(warnings.backup.is_none());
        assert!(warnings.flush.is_none());
    }

    #[test]
    fn a_backup_that_cannot_be_made_is_handed_back_rather_than_printed() {
        let dir = scratch("nobackup");
        let path = dir.join("base.yml");
        // There has to be a previous file for a backup to be attempted at all.
        save(&path, &one_match(":uno"), &dir.join("backups")).unwrap();

        // A backups folder that can never be created: its parent is a file.
        let blocker = dir.join("blocker");
        std::fs::write(&blocker, b"not a directory").unwrap();

        let warnings = save(&path, &one_match(":dos"), &blocker.join("backups")).unwrap();
        assert!(
            warnings.backup.is_some(),
            "the failure has to come back to the caller"
        );
        // And the edit still went through, which is the whole reason it is a warning and not an
        // error.
        let reloaded = load(&path).unwrap();
        assert_eq!(reloaded.entries[0].trigger_str(), ":dos");
    }
}
