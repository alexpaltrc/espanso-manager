//! `settings.json` — everything that is this app's own preference rather than espanso's.
//!
//! The one piece here that is not a plain preference is `folder_by_trigger`. Folders exist only in
//! this app; espanso has no idea they exist, and `base.yml` has nowhere to put them. So a folder is
//! an assignment *keyed by trigger*, which means every operation that changes a trigger has to move
//! its assignment with it — `rename_trigger`, `remove_trigger` — or the folder quietly detaches.
//!
//! `drop_folders_not_in` clears assignments whose trigger is gone. Its caller,
//! `AppState::drop_orphan_folders`, returns early when `load_error` is set, and that guard is the
//! whole safety of it: a momentarily unreadable `base.yml` looks exactly like "the user deleted
//! everything", and pruning against that would wipe every folder assignment at once.
//!
//! A file that cannot be parsed is copied aside and reported, never overwritten with defaults. It
//! is the only record of work the user did in this app.

use crate::i18n::{Lang, Strings};
use crate::theme::ThemeMode;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const PREFIX_SUGGESTIONS: [&str; 5] = [":", "::", ";", "?", "//"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub prefix: String,
    pub window_pos: Option<(f32, f32)>,
    pub window_size: Option<(f32, f32)>,
    /// Optional folder grouping, entirely our own bookkeeping — never written into espanso's own
    /// YAML — keyed by the match's current trigger.
    pub folder_by_trigger: BTreeMap<String, String>,
    /// Renders the list as short single-line rows instead of two-line cards.
    pub compact_view: bool,
    /// Interface language. Independent of the expansions themselves, which are never translated.
    pub lang: Lang,
    /// Light/dark appearance, or follow Windows.
    pub theme_mode: ThemeMode,
    /// Whether the first-run screen has been seen. Defaults to `false`, which is also what an
    /// older settings file without the field deserialises to — so an existing install shows the
    /// screen once and then never again, and a fresh one shows it on its very first start.
    pub onboarding_done: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            prefix: ":".to_string(),
            window_pos: None,
            window_size: None,
            folder_by_trigger: BTreeMap::new(),
            compact_view: false,
            lang: Lang::default(),
            theme_mode: ThemeMode::default(),
            onboarding_done: false,
        }
    }
}

impl Settings {
    pub fn t(&self) -> &'static Strings {
        self.lang.strings()
    }

    pub fn folder_of(&self, trigger: &str) -> Option<&str> {
        self.folder_by_trigger.get(trigger).map(|s| s.as_str())
    }

    pub fn set_folder(&mut self, trigger: &str, folder: Option<String>) {
        match folder.filter(|f| !f.trim().is_empty()) {
            Some(f) => {
                self.folder_by_trigger.insert(trigger.to_string(), f.trim().to_string());
            }
            None => {
                self.folder_by_trigger.remove(trigger);
            }
        }
    }

    /// Keeps a folder assignment attached to its match when the trigger text itself changes
    /// (editing a trigger, or a bulk prefix rename).
    pub fn rename_trigger(&mut self, old: &str, new: &str) {
        if old == new {
            return;
        }
        if let Some(folder) = self.folder_by_trigger.remove(old) {
            self.folder_by_trigger.insert(new.to_string(), folder);
        }
    }

    pub fn remove_trigger(&mut self, trigger: &str) {
        self.folder_by_trigger.remove(trigger);
    }

    /// Drops every folder assignment whose trigger is not in `live`, and says how many went.
    ///
    /// The interface keeps this map in step as expansions are added, renamed and deleted through
    /// it. Nothing keeps it in step with the file, and the file is meant to be edited by hand —
    /// that is the whole point of the portable folder. Delete a match in `base.yml` and its
    /// assignment stays behind, keyed to a trigger that no longer exists.
    ///
    /// Such a key cannot be reached from anywhere: the list groups only rows that exist, so no
    /// folder header is drawn for it and there is nothing to click to rename or delete it. It is
    /// not inert, though. [`all_folder_names`](Self::all_folder_names) reads the map's *values*,
    /// so the orphan keeps its folder in the edit form's picker, offering a folder that holds
    /// nothing, and keeps that name reserved against a real folder being renamed onto it.
    pub fn drop_folders_not_in(&mut self, live: &std::collections::HashSet<String>) -> usize {
        let before = self.folder_by_trigger.len();
        self.folder_by_trigger
            .retain(|trigger, _| live.contains(trigger));
        before - self.folder_by_trigger.len()
    }

    pub fn all_folder_names(&self) -> Vec<String> {
        let set: BTreeSet<&String> = self.folder_by_trigger.values().collect();
        set.into_iter().cloned().collect()
    }
}

/// Why the settings on disk were not the ones the app started with — kept until there is a window
/// to say it in, since [`SettingsStore::load`] runs long before one exists.
pub enum SettingsIssue {
    /// The file is there but could not be read. Left exactly where it is.
    Unreadable(String),
    /// The file is there and readable but is not settings any more. `kept_at` is where its
    /// contents were put, or `None` if even that could not be done.
    Damaged { kept_at: Option<PathBuf> },
}

pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(manager_dir: &Path) -> Self {
        Self {
            path: manager_dir.join("settings.json"),
        }
    }

    /// Reads the settings, and says what went wrong if anything did.
    ///
    /// Every failure used to collapse into `unwrap_or_default()`. That is the right *state* to
    /// carry on with, and the wrong thing to say nothing about: `folder_by_trigger` is the only
    /// copy of the folder structure there is, and the next of the app's dozen-odd saves writes the
    /// defaults straight over the file it came from. Losing all of it is survivable. Losing all of
    /// it without a word, and with the first-run screen suppressed so there is not even that hint,
    /// is not.
    pub fn load(&self) -> (Settings, Option<SettingsIssue>) {
        if !self.path.exists() {
            // No file at all is the genuinely new folder, and the only one that gets a first run.
            return (Settings::default(), None);
        }

        let (mut settings, issue) = match std::fs::read_to_string(&self.path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(settings) => (settings, None),
                Err(_) => (
                    Settings::default(),
                    Some(SettingsIssue::Damaged {
                        kept_at: self.keep_damaged_copy(),
                    }),
                ),
            },
            // Read errors are kept apart from parse errors on purpose. A file that cannot be
            // opened is very probably still perfectly good — a scanner holding it, a share that
            // dropped — and moving it aside for that would be the one action that could actually
            // lose something.
            Err(e) => (
                Settings::default(),
                Some(SettingsIssue::Unreadable(e.to_string())),
            ),
        };
        // A settings file means this app has run here before, so whoever is at the keyboard has
        // already been past a first run — even if their file predates this field and therefore
        // deserialises it as `false`, and even if it is one of the broken ones above. Only a
        // folder with no settings at all is genuinely new.
        settings.onboarding_done = true;
        (settings, issue)
    }

    /// Moves a settings file that will not parse out of the way, under a timestamped name.
    ///
    /// Not left where it is, because the next save would write over the only copy of the folder
    /// assignments; not deleted, for the same reason. Timestamped exactly as the match file's
    /// backups are — a second bad file must not erase the first, which is the one more likely to
    /// still hold something worth reading.
    fn keep_damaged_copy(&self) -> Option<PathBuf> {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let target = self.path.with_extension(format!("json.{ts}.bad"));
        std::fs::rename(&self.path, &target).ok().map(|()| target)
    }

    pub fn save(&self, settings: &Settings) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // A serialization failure used to be swallowed and `{}` written in its place: a valid,
        // empty settings file, every folder assignment gone, reported as a success. Nothing in
        // `Settings` can actually fail to serialize — serde_json writes a non-finite float as
        // `null` rather than erroring, and there is nothing else in here that could — but "cannot
        // happen" and "erases everything in silence if it does" is not a pair worth keeping.
        let json = serde_json::to_string_pretty(settings)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Written beside the real file and renamed over it, the same shape the match file's save
        // uses and for the same reason: `fs::write` truncates the live file first and fills it
        // afterwards, so anything that interrupts it — a crash, the power going — leaves an empty
        // file behind. An empty settings file parses as nothing and loads as defaults, which is to
        // say every folder gone. After a rename there is only ever the old file or the new one.
        let tmp_path = self.path.with_extension("json.tmp");
        {
            let mut file = std::fs::File::create(&tmp_path)?;
            file.write_all(json.as_bytes())?;
            // A refused flush is not a reason to throw the change away: this folder is carried on
            // USB sticks and network shares, where FlushFileBuffers can fail on a write that was
            // perfectly fine. Same policy the match file's save settled on.
            let _ = file.sync_all();
        } // Closed before the rename: renaming a file that is still open can fail.

        // A rename that fails must not leave the temporary file sitting in the state folder.
        if let Err(e) = std::fs::rename(&tmp_path, &self.path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(e);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn live(triggers: &[&str]) -> HashSet<String> {
        triggers.iter().map(|t| t.to_string()).collect()
    }

    fn settings_with(pairs: &[(&str, &str)]) -> Settings {
        let mut s = Settings::default();
        for (trigger, folder) in pairs {
            s.folder_by_trigger
                .insert(trigger.to_string(), folder.to_string());
        }
        s
    }

    /// The case that put this here: a match deleted by hand in `base.yml` leaves an assignment
    /// keyed to a trigger nothing can reach, and its folder goes on appearing in the picker.
    #[test]
    fn an_assignment_whose_trigger_is_gone_is_dropped() {
        let mut s = settings_with(&[(":firma", "Trabajo"), (":borrado", "Fantasma")]);
        assert_eq!(s.drop_folders_not_in(&live(&[":firma"])), 1);
        assert_eq!(s.all_folder_names(), vec!["Trabajo".to_string()]);
    }

    /// A folder that still has a real expansion in it must survive, even when another expansion
    /// filed under the same name has gone: the name is only dead once nothing is left in it.
    #[test]
    fn a_folder_survives_while_one_of_its_expansions_does() {
        let mut s = settings_with(&[(":uno", "Trabajo"), (":dos", "Trabajo")]);
        assert_eq!(s.drop_folders_not_in(&live(&[":uno"])), 1);
        assert_eq!(s.folder_of(":uno"), Some("Trabajo"));
        assert_eq!(s.all_folder_names(), vec!["Trabajo".to_string()]);
    }

    /// Nothing to do is reported as nothing to do — the caller only writes the file back when the
    /// count is non-zero, so a wrong answer here would mean a disk write on every single start.
    #[test]
    fn a_map_with_nothing_stale_in_it_is_left_alone() {
        let mut s = settings_with(&[(":uno", "Trabajo")]);
        assert_eq!(s.drop_folders_not_in(&live(&[":uno", ":otro"])), 0);
        assert_eq!(s.folder_of(":uno"), Some("Trabajo"));
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("espansomanager-settings-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A folder with no settings in it is the only genuinely new one, and the only one that should
    /// still be offered the first-run screen.
    #[test]
    fn a_folder_with_no_settings_yet_is_a_first_run() {
        let store = SettingsStore::new(&scratch("absent"));
        let (settings, issue) = store.load();
        assert!(issue.is_none());
        assert!(!settings.onboarding_done);
    }

    /// The round trip, and the proof that the temporary file the save writes through does not stay
    /// behind in the state folder.
    #[test]
    fn a_saved_file_reads_back_and_leaves_no_scratch_file() {
        let dir = scratch("roundtrip");
        let store = SettingsStore::new(&dir);
        store.save(&settings_with(&[(":firma", "Trabajo")])).unwrap();

        let (settings, issue) = store.load();
        assert!(issue.is_none());
        assert_eq!(settings.folder_of(":firma"), Some("Trabajo"));
        assert!(settings.onboarding_done, "the file was there, so this is not a first run");
        assert!(
            !dir.join("settings.json.tmp").exists(),
            "the file the save writes through must not survive it"
        );
    }

    /// The case this was written for: the file is unreadable as settings, so the app has to start
    /// with none — but the folder assignments must still exist somewhere afterwards, and the app
    /// must be told so it can say so.
    #[test]
    fn a_damaged_file_is_kept_and_reported_rather_than_overwritten() {
        let dir = scratch("damaged");
        let store = SettingsStore::new(&dir);
        let damaged = r#"{"folder_by_trigger": {":firma": "Trab"#;
        std::fs::write(dir.join("settings.json"), damaged).unwrap();

        let (settings, issue) = store.load();
        assert!(settings.folder_by_trigger.is_empty());
        let Some(SettingsIssue::Damaged { kept_at: Some(path) }) = issue else {
            panic!("a file that will not parse has to be reported as damaged, and kept");
        };
        assert_eq!(std::fs::read_to_string(&path).unwrap(), damaged);

        // And the save that follows — there is one within seconds of every start — must land on
        // the real file without touching what was put aside.
        store.save(&settings).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), damaged);
    }
}
