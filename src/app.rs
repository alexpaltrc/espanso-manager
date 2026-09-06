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

//! The application itself: its state, its Windows 11 palette, and the eframe loop that drives both.
//!
//! The longest-lived file here, and four neighbourhoods live in it. It is worth knowing which one
//! you are in before editing, because the rules differ.
//!
//! **1. `AppState`** — everything the interface reads, and every mutation it can perform. The
//! screens under [`crate::ui`] never open `base.yml` or `settings.json` themselves: they call a
//! method here, and that method is what validates, saves, reloads and sets the banner. If a change
//! has to reach the disk, it belongs on this side of the file, not in a view.
//!
//! **2. The palette** — `win_background` through `caution`, plus `tuned_visuals` and
//! `apply_layout_style`. These are Windows 11's own colours and type sizes rather than invented
//! ones, which is why they are written as literals with the Fluent name beside them. Every one of
//! them takes `is_light` as an argument instead of reading the theme itself, so a single frame can
//! never come out half in one theme and half in the other.
//!
//! **3. Window geometry** — `fit_window_to` and its helpers: the on-show repair that stops a size
//! or position remembered from a different machine opening off-screen or too small to use. The
//! rules come from [`crate::display`]; only the winit plumbing is here.
//!
//! **4. `EspansoManagerApp`** — the eframe `App`. It owns the tray, the hotkey and the theme
//! watch, drains their queues once a frame, and hands the frame to a view.
//!
//! The split that matters in neighbourhood 4: `logic` runs every frame *including while the window
//! is hidden in the tray*, and eframe forbids drawing anything from it. `ui` runs only when there
//! is a window. Work that must happen whether or not anybody is looking goes in `logic`; anything
//! that draws goes in `ui`. `OWN_LAUNCHER` sits on the wrong side of that line today and is
//! switched off, which is the only reason it is harmless — see `LAUNCHER-BACKUP.txt`.

use crate::autostart;
use crate::espanso_ctl::EspansoCtl;
use crate::fonts;
use crate::i18n::{fill, Lang, Strings};
use crate::settings::{Settings, SettingsStore};
use crate::theme::{self, ThemeMode};
use crate::tray::{Tray, TrayEvents, VISIBLE_POLL_INTERVAL};
use crate::ui;
use crate::ui::edit_form::EditState;
use crate::yaml::io as yaml_io;
use crate::yaml::model::{MatchEntry, MatchFile};
use eframe::egui;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};
use std::sync::mpsc::TryRecvError;

/// Drag-and-drop payload: the triggers of the expansions being dragged (identifying entries by
/// trigger, not index, so nothing gets confused if the list re-filters mid-drag). A single-item
/// drag of something not currently selected, or the whole selection if you drag a selected row.
pub type DragPayload = Vec<String>;

/// Whether this app provides its own Alt+Space search window.
///
/// Off. Espanso's own search bar is the one in use, and it is the better one: it picks a match by
/// identity and so never sends the backspaces that made ours eat the words in front of the cursor
/// — a door espanso keeps to itself and does not open to anything outside it.
///
/// Ours is still here, whole, and this constant is the entire switch. Off, nothing of ours is
/// registered, so the window cannot open and cannot get in espanso's way. `LAUNCHER-BACKUP.txt`
/// beside the source says what was built and what was never solved.
///
/// It is no longer the entire switch for turning it back *on*, though. `show_search_window` is
/// called from `logic`, and eframe 0.36.1 forbids drawing there — and runs no egui pass at all
/// while the window is hidden, which is precisely the state Alt+Space exists to work from. Nothing
/// is wrong today, because with this off `self.hotkey` is `None` and the window can never open.
/// Whoever flips this has to move that call out of `logic` first.
const OWN_LAUNCHER: bool = false;

/// Far more characters than any row can display, so the width-aware trim the label does afterwards
/// still has plenty to work with.
const PREVIEW_MAX_CHARS: usize = 200;

/// Shortens `s` to `max_chars` characters, adding an ellipsis if anything was cut. Counts
/// characters rather than bytes, so it can never split one in half.
pub fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars).collect();
        out.push('…');
        out
    }
}

/// Deliberately not boxed. `EditState` makes this enum about 240 bytes, and clippy asks for a
/// `Box` on that basis — but the reasoning behind that lint is arrays and queues of the enum, and
/// there is exactly one `View` in the whole program (`AppState::view`). Boxing it would buy back a
/// quarter of a kilobyte, once, in exchange for an allocation every time the editor opens and a
/// dereference at every place the editor is drawn.
#[allow(clippy::large_enum_variant)]
pub enum View {
    List,
    Edit(EditState),
    Settings,
    Tips,
    Onboarding,
}

pub enum PendingConfirmKind {
    DeleteFolder { folder: String },
    DeleteSelection,
}

/// A destructive action (deleting a folder or a multi-selection) awaiting confirmation in an
/// in-app modal, rather than a native message box — needed because the list of affected
/// expansions can be long, and a native dialog can't scroll or collapse to stay compact.
pub struct PendingConfirm {
    pub kind: PendingConfirmKind,
    /// Snapshot of affected entries at the moment the action was requested, as (trigger, preview)
    /// pairs — owned, so the confirmation stays stable even if the user somehow changes something
    /// else first.
    pub items: Vec<(String, String)>,
    pub expanded: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BannerKind {
    Info,
    Error,
}

/// A change that the message announcing it can also take back.
///
/// Dragging expansions into a folder is the one action here that happens without a confirmation —
/// it has to, or the gesture would be unusable — and it can move a whole selection at once. Rather
/// than putting a dialog in front of every drop, the result is simply reversible: the banner that
/// says what happened carries the way to undo it.
pub enum UndoAction {
    /// Folder assignments exactly as they were before the move, as (trigger, previous folder).
    FolderMove(Vec<(String, Option<String>)>),
}

pub struct Banner {
    pub kind: BannerKind,
    pub message: String,
    pub undo: Option<UndoAction>,
}

/// One expansion as the list draws it.
pub struct ListRow {
    pub trigger: String,
    /// The replacement, already shortened. Hard-capped here rather than at the widget:
    /// `Label::truncate` does the final width-aware trim, but it has to lay the text out to know
    /// where to cut, so handing it a multi-kilobyte replacement would make the row pay for glyphs
    /// nobody can see.
    pub preview: String,
}

/// Everything the list screen needs, derived once from the expansions and the folder assignments.
///
/// Rebuilt only when something it depends on actually changes (see [`AppState::list`]). Deriving it
/// per frame instead meant re-reading every expansion, re-rendering every preview and re-sorting
/// every folder sixty times a second just to draw a list that had not moved — invisible at a dozen
/// expansions, and several milliseconds of pure waste at a few thousand.
pub struct ListCache {
    pub rows: Vec<ListRow>,
    /// Folder name → indices into `rows`, in the A→Z order folders are shown in.
    pub grouped: Vec<(String, Vec<usize>)>,
    /// Indices into `rows` for the expansions that belong to no folder.
    pub ungrouped: Vec<usize>,
    /// Expansions in the file, including the ones this list never shows — so the screen can tell
    /// "you have none yet" apart from "your search matched none".
    pub total_entries: usize,
}

/// What the cached list was built from. Anything that can change the list changes this, so a stale
/// cache cannot survive.
#[derive(PartialEq)]
struct ListCacheKey {
    revision: u64,
    search: String,
    lang: Lang,
    /// Belt and braces alongside `revision`: even if some future edit path forgot to bump the
    /// revision, adding or removing an expansion or a folder assignment still invalidates the cache.
    entry_count: usize,
    folder_assignment_count: usize,
}

/// All application state that isn't specific to rendering one particular screen. Owned by
/// [`EspansoManagerApp`] and passed down to the `ui::*` view functions.
pub struct AppState {
    pub match_file: MatchFile,
    pub match_file_path: PathBuf,
    pub backups_dir: PathBuf,
    pub settings: Settings,
    pub settings_store: SettingsStore,
    pub ctl: EspansoCtl,
    pub view: View,
    pub search: String,
    pub banner: Option<Banner>,
    /// Why the expansions file could not be read, if it could not. Saving is refused while this
    /// is set: the model in memory is empty, and it did not come from disk.
    pub load_error: Option<String>,
    pub autostart_enabled: bool,
    pub exe_path: PathBuf,
    /// Triggers of the expansions currently checked in the list, for bulk actions.
    pub selected: BTreeSet<String>,
    /// Where a Shift-click range starts, following the same rule as a file list: the last row that
    /// was clicked on its own.
    pub selection_anchor: Option<String>,
    /// (original name, current edit buffer) while a folder's name is being edited inline.
    pub renaming_folder: Option<(String, String)>,
    pub pending_confirm: Option<PendingConfirm>,
    /// Set when the language changes; the owning app reloads fonts and tray labels next frame,
    /// since those need the egui context and the tray handle that `AppState` doesn't hold.
    pub pending_language_refresh: bool,
    /// Set when the theme mode changes, so the palette is re-applied immediately rather than at
    /// the next scheduled poll.
    pub pending_theme_refresh: bool,
    /// What has been typed into the first-run screen's trial box, and whether the expansion has
    /// fired in it yet.
    pub onboarding_probe: String,
    pub onboarding_expanded: bool,
    /// The search window: whether it is up, what has been typed into it, which result is lit, and
    /// the window that had the keyboard before it opened — which is where the expansion has to go
    /// back to.
    pub search_open: bool,
    pub search_query: String,
    pub search_selected: usize,
    pub search_return_to: isize,
    /// Set when the window opens, cleared once the keyboard has been asked for. Without this the
    /// window appears but the typing goes to whatever was behind it — which is worse than not
    /// opening at all, because it puts stray characters in the user's document.
    pub search_needs_focus: bool,
    /// Set once the window has actually held the keyboard. Losing it after that means the person
    /// clicked somewhere else, which is a dismissal — but the frames *before* it arrives are not.
    pub search_had_focus: bool,
    /// An expansion that has been picked and is waiting for the search window to be *gone* before
    /// the keyboard is handed back. See [`App::show_search_window`].
    pub search_handoff: Option<String>,
    /// Asks the list to scroll the lit row back into view on the next frame. Set by the arrows and
    /// by a change to the query, so the selection can never walk off the edge of the window.
    pub search_follow: bool,
    /// Espanso's own version. Shown in Settings, never acted on.
    ///
    /// Read the first time that screen is opened rather than at startup. Asking for it means
    /// spawning `espanso --version` and waiting on it for up to `CTL_TIMEOUT`, and at startup that
    /// wait happened with the window already created and unresponsive — on every launch, including
    /// the hidden autostart one, for a line most people never look at. See
    /// [`AppState::ensure_espanso_version`].
    pub espanso_version: Option<String>,
    /// Whether the question above has been asked. Kept apart from the answer because "espanso would
    /// not say" is also an answer, and asking again every time Settings is opened would mean
    /// another four-second wait on exactly the machine that could least afford the first one.
    pub espanso_version_asked: bool,
    /// The shortcut that actually opens the search window, as a person would say it. Whatever the
    /// tips screen promises has to be this.
    pub search_shortcut: &'static str,
    /// Bumped by every change to the expansions or their folders; see [`AppState::touch_list`].
    list_revision: u64,
    list_cache: Option<Rc<ListCache>>,
    list_cache_key: Option<ListCacheKey>,
}

impl AppState {
    pub fn set_info_banner(&mut self, message: impl Into<String>) {
        self.banner = Some(Banner {
            kind: BannerKind::Info,
            message: message.into(),
            undo: None,
        });
    }

    pub fn set_error_banner(&mut self, message: impl Into<String>) {
        self.banner = Some(Banner {
            kind: BannerKind::Error,
            message: message.into(),
            undo: None,
        });
    }

    /// The scratch file holding the one expansion the first-run screen demonstrates.
    ///
    /// Deliberately its own file rather than a line in `base.yml`. The app only ever reads and
    /// writes `base.yml`, so a trial expansion living there would be a real entry in the user's own
    /// list — and one added behind their back. Here it is separate, espanso picks it up because it
    /// loads every file in the match folder, and it is deleted the moment the screen is finished
    /// with. Anything left behind by a crash is cleared at the next start.
    fn onboarding_match_path(&self) -> PathBuf {
        self.match_file_path
            .parent()
            .map(|dir| dir.join("onboarding.yml"))
            .unwrap_or_else(|| PathBuf::from("onboarding.yml"))
    }

    /// Puts the trial expansion in place. Espanso watches the match folder, so it becomes live on
    /// its own within a moment — no restart, and nothing touched that the user owns.
    pub fn begin_onboarding(&mut self) {
        let path = self.onboarding_match_path();
        let body = format!(
            "# Temporary — created by EspansoManager for its first-run screen and deleted as soon\n\
             # as that screen is finished with. Nothing here is yours; edit base.yml instead.\n\
             matches:\n- trigger: '{}'\n  replace: {}\n",
            crate::ui::onboarding_view::PROBE_TRIGGER,
            crate::ui::onboarding_view::PROBE_REPLACE,
        );
        let _ = std::fs::write(path, body);
    }

    /// Takes the trial expansion away again and remembers that the screen has been seen.
    pub fn finish_onboarding(&mut self) {
        self.clear_onboarding_match();
        self.settings.onboarding_done = true;
        let _ = self.settings_store.save(&self.settings);
    }

    pub fn clear_onboarding_match(&self) {
        let _ = std::fs::remove_file(self.onboarding_match_path());
    }

    /// Every expansion this app manages, as (trigger, one-line preview), for the search window.
    pub fn list_all(&self) -> Vec<(String, String)> {
        let t = self.t();
        self.match_file
            .entries
            .iter()
            .filter_map(|entry| match entry {
                MatchEntry::Simple(m) => Some((m.trigger.clone(), entry.preview(t))),
                MatchEntry::Advanced(_) => None,
            })
            .collect()
    }

    pub fn close_search(&mut self) {
        self.search_open = false;
        self.search_query.clear();
        self.search_selected = 0;
        self.search_follow = false;
    }

    /// Asks espanso its version, once, on the way into the screen that shows it.
    ///
    /// Blocking here is fine in a way it was not at startup: the user has just clicked Ajustes and
    /// the window is up and drawn. Called from the one place `View::Settings` is entered.
    pub fn ensure_espanso_version(&mut self) {
        if self.espanso_version_asked {
            return;
        }
        self.espanso_version_asked = true;
        self.espanso_version = self.ctl.version(self.t());
    }

    pub fn refresh_autostart_cache(&mut self) {
        self.autostart_enabled = autostart::is_enabled();
    }

    /// Shorthand for the active language's string table.
    pub fn t(&self) -> &'static Strings {
        self.settings.t()
    }

    /// Marks the list as out of date.
    ///
    /// Call this from anywhere that changes an expansion or a folder assignment. The search box and
    /// the interface language are watched by the cache key itself and need no call.
    fn touch_list(&mut self) {
        self.list_revision = self.list_revision.wrapping_add(1);
    }

    /// The list as it should be drawn, rebuilding it only if something it depends on has changed.
    ///
    /// Handed out behind an [`Rc`] so a caller can hold on to it while still mutating the rest of
    /// the state — selecting a row, opening the editor, deleting something — without the borrow
    /// checker forcing the whole list to be re-derived just to satisfy it.
    pub fn list(&mut self) -> Rc<ListCache> {
        let key = ListCacheKey {
            revision: self.list_revision,
            search: self.search.clone(),
            lang: self.settings.lang,
            entry_count: self.match_file.entries.len(),
            folder_assignment_count: self.settings.folder_by_trigger.len(),
        };
        if self.list_cache_key.as_ref() != Some(&key) || self.list_cache.is_none() {
            self.list_cache = Some(Rc::new(self.build_list()));
            self.list_cache_key = Some(key);
        }
        self.list_cache.clone().expect("just populated above")
    }

    fn build_list(&self) -> ListCache {
        let t = self.t();
        let query = self.search.to_lowercase();

        // Only text/date expansions are ever surfaced — anything else (shell commands, regex
        // matches, per-app conditionals, ...) stays untouched in the YAML file but out of this
        // list, by design, so coworkers only ever see the two kinds they're meant to use.
        let mut rows: Vec<ListRow> = Vec::new();
        for entry in self.match_file.entries.iter().filter(|e| e.is_safely_editable()) {
            let preview = entry.preview(t);
            if !query.is_empty() {
                let matches = entry.trigger_str().to_lowercase().contains(&query)
                    || preview.to_lowercase().contains(&query);
                if !matches {
                    continue;
                }
            }
            rows.push(ListRow {
                trigger: entry.trigger_label(),
                preview: truncate(&preview, PREVIEW_MAX_CHARS),
            });
        }

        // Grouped in one pass (O(n)) rather than re-scanning the rows once per folder name
        // (O(folders × n)). `BTreeMap` also gives the A→Z folder ordering for free.
        let mut grouped: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        let mut ungrouped: Vec<usize> = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            match self.settings.folder_of(&row.trigger) {
                // The folder name is only copied the first time it's seen, not once per row.
                Some(folder) => match grouped.get_mut(folder) {
                    Some(bucket) => bucket.push(index),
                    None => {
                        grouped.insert(folder.to_string(), vec![index]);
                    }
                },
                None => ungrouped.push(index),
            }
        }

        // No flattened draw order is kept here. It would have to say which rows are on screen, and
        // that turns on which folders are folded open — egui's business, changing without anything
        // in this model changing with it. The one place that needs it builds it from these two
        // fields at the moment it is asked for: `ui::list_view::visible_order`.
        ListCache {
            rows,
            grouped: grouped.into_iter().collect(),
            ungrouped,
            total_entries: self.match_file.entries.len(),
        }
    }

    pub fn set_autostart(&mut self, enabled: bool) {
        let t = self.t();
        match autostart::set_enabled(enabled, &self.exe_path) {
            Ok(()) => {
                self.autostart_enabled = enabled;
                if enabled {
                    self.set_info_banner(t.autostart_on);
                } else {
                    self.set_info_banner(t.autostart_off);
                }
            }
            Err(e) => {
                self.refresh_autostart_cache();
                self.set_error_banner(fill(t.autostart_error, &[("err", &e.to_string())]));
            }
        }
    }

    pub fn set_prefix(&mut self, prefix: String) {
        if prefix.trim().is_empty() {
            return;
        }
        self.settings.prefix = prefix;
        if let Err(e) = self.settings_store.save(&self.settings) {
            let msg = fill(self.t().prefix_save_error, &[("err", &e.to_string())]);
            self.set_error_banner(msg);
        }
    }

    pub fn set_theme_mode(&mut self, mode: ThemeMode) {
        if self.settings.theme_mode == mode {
            return;
        }
        self.settings.theme_mode = mode;
        if let Err(e) = self.settings_store.save(&self.settings) {
            let msg = fill(self.t().theme_save_error, &[("err", &e.to_string())]);
            self.set_error_banner(msg);
        }
        self.pending_theme_refresh = true;
    }

    pub fn set_language(&mut self, lang: Lang) {
        if self.settings.lang == lang {
            return;
        }
        self.settings.lang = lang;
        if let Err(e) = self.settings_store.save(&self.settings) {
            let msg = fill(self.t().language_save_error, &[("err", &e.to_string())]);
            self.set_error_banner(msg);
        }
        // Fonts and tray labels can't be swapped from here (they need the egui context and the
        // tray handle), so the owning app picks this up on the next frame.
        self.pending_language_refresh = true;
    }

    /// Strips a leading run of non-alphanumeric characters (`:`, `;`, `?`, `//`, ...), treating
    /// it as "whatever prefix this trigger currently uses".
    fn strip_leading_prefix(trigger: &str) -> &str {
        trigger.trim_start_matches(|c: char| !c.is_alphanumeric())
    }

    pub fn apply_prefix_to_existing(&mut self) {
        let new_prefix = self.settings.prefix.clone();
        let mut renamed = Vec::new();
        let mut resulting: Vec<String> = Vec::new();

        for entry in &self.match_file.entries {
            if let MatchEntry::Simple(m) = entry {
                let rest = Self::strip_leading_prefix(&m.trigger);
                let new_trigger = format!("{new_prefix}{rest}");
                resulting.push(new_trigger.clone());
                if new_trigger != m.trigger {
                    renamed.push((m.trigger.clone(), new_trigger));
                }
            } else {
                resulting.push(entry.trigger_label());
            }
        }

        let t = self.t();
        if renamed.is_empty() {
            self.set_info_banner(t.prefix_already_applied);
            return;
        }

        let mut seen = std::collections::HashSet::new();
        let mut collisions = Vec::new();
        for r in &resulting {
            if !seen.insert(r.as_str()) {
                collisions.push(r.clone());
            }
        }
        if !collisions.is_empty() {
            self.set_error_banner(fill(
                t.prefix_collision,
                &[("list", &collisions.join(", "))],
            ));
            return;
        }

        let summary: String = renamed
            .iter()
            .take(8)
            .map(|(old, new)| format!("  {old}  →  {new}"))
            .collect::<Vec<_>>()
            .join("\n");
        let more = if renamed.len() > 8 {
            fill(t.prefix_confirm_more, &[("n", &(renamed.len() - 8).to_string())])
        } else {
            String::new()
        };

        let confirmed = rfd::MessageDialog::new()
            .set_title(t.prefix_confirm_title)
            .set_description(fill(
                t.prefix_confirm_body,
                &[
                    ("n", &renamed.len().to_string()),
                    ("list", &summary),
                    ("more", &more),
                ],
            ))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        if confirmed != rfd::MessageDialogResult::Yes {
            return;
        }

        for entry in &mut self.match_file.entries {
            if let MatchEntry::Simple(m) = entry {
                let rest = Self::strip_leading_prefix(&m.trigger).to_string();
                m.trigger = format!("{new_prefix}{rest}");
            }
        }
        for (old, new) in &renamed {
            self.settings.rename_trigger(old, new);
            self.selection_rename(old, new);
        }
        let _ = self.settings_store.save(&self.settings);
        self.touch_list();

        self.save_and_reload(fill(
            t.prefix_applied,
            &[("n", &renamed.len().to_string())],
        ));
    }

    pub fn request_delete(&mut self, index: usize) {
        let Some(entry) = self.match_file.entries.get(index) else {
            return;
        };
        let trigger = entry.trigger_label();
        let t = self.t();
        let confirmed = rfd::MessageDialog::new()
            .set_title(t.delete_one_title)
            .set_description(fill(t.delete_one_body, &[("name", &trigger)]))
            .set_level(rfd::MessageLevel::Warning)
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        if confirmed == rfd::MessageDialogResult::Yes {
            self.match_file.entries.remove(index);
            self.settings.remove_trigger(&trigger);
            self.selection_forget(&trigger);
            let _ = self.settings_store.save(&self.settings);
            self.touch_list();
            self.view = View::List;
            self.save_and_reload(t.expansion_deleted);
        }
    }

    /// Removes every entry whose trigger is in `triggers`, cleaning up their folder assignments
    /// too. Matches by trigger rather than index so it's safe to call with a snapshot taken
    /// earlier (e.g. from a confirmation modal).
    fn remove_by_triggers(&mut self, triggers: &[String]) {
        // A `HashSet` membership check keeps this O(n) even for a huge bulk delete — `.retain`
        // with `.any(...)` over `triggers` for every entry would be O(entries × triggers), which
        // gets slow fast once both sides are in the thousands.
        let to_remove: std::collections::HashSet<&str> = triggers.iter().map(String::as_str).collect();
        self.match_file
            .entries
            .retain(|e| !to_remove.contains(e.trigger_str()));
        for t in triggers {
            self.settings.remove_trigger(t);
            self.selection_forget(t);
        }
        let _ = self.settings_store.save(&self.settings);
        self.touch_list();
    }

    /// Follows a renamed expansion through the selection.
    ///
    /// `selected` and `selection_anchor` hold trigger strings, and a trigger is not an identity —
    /// it is the one thing about an expansion the user is most likely to change. Renaming without
    /// this leaves the old name behind: the bar counts a row that is no longer there, and the next
    /// drag carries the dead name into [`assign_folder_to_triggers`].
    fn selection_rename(&mut self, old: &str, new: &str) {
        if self.selected.remove(old) {
            self.selected.insert(new.to_string());
        }
        if self.selection_anchor.as_deref() == Some(old) {
            self.selection_anchor = Some(new.to_string());
        }
    }

    /// Drops a deleted expansion from the selection. The anchor goes with it: a Shift-click
    /// extending from a row that no longer exists finds nothing to measure from and silently
    /// degrades into a plain click, which is a worse answer than starting a fresh range.
    fn selection_forget(&mut self, trigger: &str) {
        self.selected.remove(trigger);
        if self.selection_anchor.as_deref() == Some(trigger) {
            self.selection_anchor = None;
        }
    }

    pub fn toggle_selected(&mut self, trigger: &str) {
        if !self.selected.remove(trigger) {
            self.selected.insert(trigger.to_string());
        }
        self.selection_anchor = Some(trigger.to_string());
    }

    /// Handles a click on a row using the conventions of a Windows file list: a plain click selects
    /// just that row, Ctrl adds or removes one, and Shift extends from the last plainly-clicked row
    /// to this one.
    ///
    /// `visible_order` is the list exactly as drawn, so a Shift range covers what the user actually
    /// sees between the two rows rather than some hidden underlying order.
    pub fn click_select(
        &mut self,
        trigger: &str,
        ctrl: bool,
        shift: bool,
        visible_order: &[String],
    ) {
        if shift {
            if let Some(anchor) = self.selection_anchor.clone() {
                let from = visible_order.iter().position(|t| t == &anchor);
                let to = visible_order.iter().position(|t| t == trigger);
                if let (Some(a), Some(b)) = (from, to) {
                    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                    if !ctrl {
                        self.selected.clear();
                    }
                    for t in &visible_order[lo..=hi] {
                        self.selected.insert(t.clone());
                    }
                    // The anchor deliberately stays put, so dragging the shift-click around keeps
                    // growing and shrinking the same range.
                    return;
                }
            }
        }

        if ctrl {
            self.toggle_selected(trigger);
            return;
        }

        // A plain click on an already-multi-selected row keeps the selection, so it can be dragged
        // as a group — clicking a single row still collapses down to just that one.
        let only_this = self.selected.len() == 1 && self.selected.contains(trigger);
        if only_this {
            self.selected.clear();
            self.selection_anchor = None;
        } else {
            self.selected.clear();
            self.selected.insert(trigger.to_string());
            self.selection_anchor = Some(trigger.to_string());
        }
    }

    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.selection_anchor = None;
    }

    /// Whether the current selection contains at least one expansion that actually sits in a
    /// folder — the "Quitar de su carpeta" action is meaningless (and so stays hidden) otherwise.
    /// Cheap: at most one map lookup per selected item, and the selection is small by nature.
    pub fn selection_has_foldered(&self) -> bool {
        self.selected
            .iter()
            .any(|t| self.settings.folder_of(t).is_some())
    }

    pub fn set_compact_view(&mut self, compact: bool) {
        if self.settings.compact_view == compact {
            return;
        }
        self.settings.compact_view = compact;
        if let Err(e) = self.settings_store.save(&self.settings) {
            let msg = fill(self.t().view_save_error, &[("err", &e.to_string())]);
            self.set_error_banner(msg);
        }
    }

    pub fn request_delete_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        let t = self.t();
        // One pass building a lookup table, rather than re-scanning all entries for every
        // selected trigger — matters once both the selection and the list are large.
        let previews: std::collections::HashMap<String, String> = self
            .match_file
            .entries
            .iter()
            .map(|e| (e.trigger_label(), e.preview(t)))
            .collect();
        let items = self
            .selected
            .iter()
            .map(|t| (t.clone(), previews.get(t).cloned().unwrap_or_default()))
            .collect();
        self.pending_confirm = Some(PendingConfirm {
            kind: PendingConfirmKind::DeleteSelection,
            items,
            expanded: false,
        });
    }

    pub fn request_delete_folder(&mut self, folder: &str) {
        let t = self.t();
        let items: Vec<(String, String)> = self
            .match_file
            .entries
            .iter()
            .filter(|e| self.settings.folder_of(&e.trigger_label()) == Some(folder))
            .map(|e| (e.trigger_label(), e.preview(t)))
            .collect();
        if items.is_empty() {
            return;
        }
        self.pending_confirm = Some(PendingConfirm {
            kind: PendingConfirmKind::DeleteFolder {
                folder: folder.to_string(),
            },
            items,
            expanded: false,
        });
    }

    pub fn cancel_pending_confirm(&mut self) {
        self.pending_confirm = None;
    }

    pub fn confirm_pending_delete(&mut self) {
        let Some(pending) = self.pending_confirm.take() else {
            return;
        };
        let triggers: Vec<String> = pending.items.iter().map(|(t, _)| t.clone()).collect();
        let count = triggers.len();
        self.remove_by_triggers(&triggers);
        self.clear_selection();
        let t = self.t();
        let message = match pending.kind {
            PendingConfirmKind::DeleteFolder { folder } => fill(
                t.folder_deleted,
                &[("name", &folder), ("n", &count.to_string())],
            ),
            PendingConfirmKind::DeleteSelection => {
                fill(t.selection_deleted, &[("n", &count.to_string())])
            }
        };
        self.save_and_reload(message);
    }

    pub fn start_rename_folder(&mut self, folder: &str) {
        self.renaming_folder = Some((folder.to_string(), folder.to_string()));
    }

    pub fn cancel_rename_folder(&mut self) {
        self.renaming_folder = None;
    }

    pub fn commit_rename_folder(&mut self) {
        let Some((old, new)) = self.renaming_folder.take() else {
            return;
        };
        let new = new.trim().to_string();
        if new.is_empty() || new == old {
            return;
        }
        let t = self.t();
        if self.settings.all_folder_names().iter().any(|f| f == &new) {
            self.set_error_banner(fill(t.folder_name_taken, &[("name", &new)]));
            return;
        }
        for folder in self.settings.folder_by_trigger.values_mut() {
            if *folder == old {
                *folder = new.clone();
            }
        }
        let _ = self.settings_store.save(&self.settings);
        self.touch_list();
        self.set_info_banner(fill(t.folder_renamed, &[("name", &new)]));
    }

    /// Reconciles the folder map with the file, once, at startup.
    ///
    /// See [`Settings::drop_folders_not_in`] for what an orphan assignment costs. Every entry is
    /// checked, not just the ones the list shows: an advanced match is invisible here but is very
    /// much still in the file, and its folder is not stale just because this app declines to draw
    /// a row for it.
    ///
    /// Skipped when the file could not be read. A failed load leaves an empty model behind, and
    /// pruning against that would delete every folder the user has for the one reason that is
    /// certainly not their doing — the same reasoning that stops [`Self::save_and_reload`] from
    /// writing.
    fn drop_orphan_folders(&mut self) {
        if self.load_error.is_some() {
            return;
        }
        let live: std::collections::HashSet<String> = self
            .match_file
            .entries
            .iter()
            .map(MatchEntry::trigger_label)
            .collect();
        // Written back only when something actually went, so an ordinary start still touches
        // nothing on disk.
        if self.settings.drop_folders_not_in(&live) > 0 {
            let _ = self.settings_store.save(&self.settings);
        }
    }

    /// Moves a group of expansions into `folder`, or clears their folder if `None` — used by the
    /// "sin carpeta" drop zone as well as regular assignment from the edit form.
    pub fn assign_folder_to_triggers(&mut self, triggers: &[String], folder: Option<String>) {
        // Where each of these lived a moment ago, so the message can offer to put them back — and,
        // first, so anything already where it is being sent can be dropped from the operation.
        //
        // Without this, dragging an expansion that belongs to no folder onto the empty strip that
        // means "no folder" still announced a move and offered to undo one. Nothing had changed.
        // A gesture that does nothing should say nothing.
        //
        // A trigger that no longer names anything is dropped here rather than filed. The folder map
        // is keyed by trigger, and a key with no expansion behind it is unreachable: the list groups
        // only rows that exist, so no folder header is ever drawn for it and there is nothing to
        // click to rename or delete it — while it goes on offering its folder in the edit form's
        // picker and goes on claiming that name against a real folder being renamed.
        let previous: Vec<(String, Option<String>)> = {
            let live: std::collections::HashSet<&str> = self
                .match_file
                .entries
                .iter()
                .map(MatchEntry::trigger_str)
                .collect();
            triggers
                .iter()
                .filter(|trigger| live.contains(trigger.as_str()))
                .map(|trigger| {
                    (
                        trigger.clone(),
                        self.settings.folder_of(trigger).map(str::to_string),
                    )
                })
                .filter(|(_, was)| was.as_deref() != folder.as_deref())
                .collect()
        };

        if previous.is_empty() {
            return;
        }
        let triggers: Vec<String> = previous.iter().map(|(t, _)| t.clone()).collect();
        let triggers = &triggers[..];

        for trigger in triggers {
            self.settings.set_folder(trigger, folder.clone());
        }
        let _ = self.settings_store.save(&self.settings);
        self.touch_list();
        self.clear_selection();
        let t = self.t();
        let n = triggers.len().to_string();
        match &folder {
            Some(f) => {
                self.set_info_banner(fill(t.moved_to_folder, &[("n", &n), ("name", f)]))
            }
            None => self.set_info_banner(fill(t.removed_from_folder, &[("n", &n)])),
        }
        if let Some(banner) = &mut self.banner {
            banner.undo = Some(UndoAction::FolderMove(previous));
        }
    }

    /// Writes the expansions the list shows out to a file the user picks.
    pub fn export_expansions(&mut self) {
        let t = self.t();
        let export = crate::transfer::build_export(&self.match_file.entries, |trigger| {
            self.settings.folder_of(trigger).map(str::to_string)
        });
        let count = export.expansions.len();

        let Some(path) = rfd::FileDialog::new()
            .set_title(t.export_button)
            .set_file_name(crate::transfer::suggested_filename())
            .add_filter(t.transfer_file_kind, &["yml", "yaml"])
            .save_file()
        else {
            return; // The user closed the dialog; that is not an error worth a banner.
        };

        match crate::transfer::write(&path, &export) {
            Ok(()) => self.set_info_banner(fill(t.export_done, &[("n", &count.to_string())])),
            Err(crate::transfer::TransferError::Io(e)) => {
                self.set_error_banner(fill(t.export_error, &[("err", &e)]))
            }
            Err(_) => self.set_error_banner(t.export_error),
        }
    }

    /// Adds the expansions from a file the user picks. Never replaces anything: see
    /// [`crate::transfer::merge`].
    pub fn import_expansions(&mut self) {
        let t = self.t();
        let Some(path) = rfd::FileDialog::new()
            .set_title(t.import_button)
            .add_filter(t.transfer_file_kind, &["yml", "yaml"])
            .pick_file()
        else {
            return;
        };

        let export = match crate::transfer::read(&path) {
            Ok(export) => export,
            Err(crate::transfer::TransferError::Io(e)) => {
                self.set_error_banner(fill(t.import_error, &[("err", &e)]));
                return;
            }
            Err(_) => {
                // "Not one of ours" and "valid but empty" land in the same place: either way there
                // is nothing to add, and the file is the thing to look at.
                self.set_error_banner(t.import_not_ours);
                return;
            }
        };

        let merged = crate::transfer::merge(&self.match_file.entries, export);
        let added = merged.added.len();
        let skipped = merged.skipped;

        if added == 0 {
            self.set_info_banner(fill(t.import_none_added, &[("k", &skipped.to_string())]));
            return;
        }

        self.match_file.entries.extend(merged.added);
        for (trigger, folder) in &merged.folders {
            self.settings.set_folder(trigger, folder.clone());
        }
        let _ = self.settings_store.save(&self.settings);
        self.touch_list();

        let message = if skipped == 0 {
            fill(t.import_done, &[("n", &added.to_string())])
        } else {
            fill(
                t.import_done_skipped,
                &[("n", &added.to_string()), ("k", &skipped.to_string())],
            )
        };
        self.save_and_reload(message);
    }

    /// Puts back whatever the banner currently on screen is offering to undo.
    pub fn undo_from_banner(&mut self) {
        let Some(banner) = self.banner.take() else {
            return;
        };
        let Some(UndoAction::FolderMove(previous)) = banner.undo else {
            return;
        };
        for (trigger, folder) in &previous {
            self.settings.set_folder(trigger, folder.clone());
        }
        let _ = self.settings_store.save(&self.settings);
        self.touch_list();
        let t = self.t();
        self.set_info_banner(t.move_undone);
    }

    pub fn save_edit(&mut self, edit: &EditState) {
        let t = self.t();
        let new_match = edit.build_match();
        if new_match.trigger.trim().is_empty() {
            self.set_error_banner(t.trigger_empty);
            return;
        }

        // Advanced matches count too. They are not shown in the list, so a trigger that collides
        // with one would look free, save without complaint, and then never fire — espanso keeps
        // the first match for a trigger, and the invisible one is usually the first.
        let duplicate = self.match_file.entries.iter().enumerate().any(|(i, e)| {
            Some(i) != edit.editing_index && e.trigger_str() == new_match.trigger
        });
        if duplicate {
            self.set_error_banner(fill(t.trigger_duplicate, &[("name", &new_match.trigger)]));
            return;
        }

        let previous_trigger = edit.editing_index.and_then(|i| match &self.match_file.entries[i] {
            MatchEntry::Simple(m) => Some(m.trigger.clone()),
            MatchEntry::Advanced(_) => None,
        });

        match edit.editing_index {
            Some(index) => self.match_file.entries[index] = MatchEntry::Simple(new_match.clone()),
            None => self.match_file.entries.push(MatchEntry::Simple(new_match.clone())),
        }

        if let Some(previous_trigger) = &previous_trigger {
            self.settings.rename_trigger(previous_trigger, &new_match.trigger);
            self.selection_rename(previous_trigger, &new_match.trigger);
        }
        self.settings.set_folder(&new_match.trigger, edit.resolved_folder());
        let _ = self.settings_store.save(&self.settings);
        self.touch_list();

        self.view = View::List;
        self.save_and_reload(t.expansion_saved);
    }

    fn save_and_reload(&mut self, success_message: impl Into<String>) {
        // The model never came from disk. Writing it back would put an empty file where one we
        // could not read used to be, and every expansion in it would be gone from espanso. Say so
        // instead, in the same words the dialog at startup used.
        if let Some(message) = self.load_error.clone() {
            self.set_error_banner(message);
            return;
        }
        let t = self.t();
        match yaml_io::save(&self.match_file_path, &self.match_file, &self.backups_dir) {
            Ok(warnings) => {
                let outcome = match self.ctl.restart_and_confirm(Duration::from_secs(6), t) {
                    Ok(()) => Ok(success_message.into()),
                    Err(e) => Err(e),
                };
                // The edit is on disk either way, so the first line still says so. What follows is
                // that the safety net under it is not there — which is worth the red banner,
                // because the moment it matters is the moment nobody is looking.
                match (outcome, warnings.message(t)) {
                    (Ok(message), None) => self.set_info_banner(message),
                    (Ok(message), Some(warning)) => {
                        self.set_error_banner(format!("{message}\n\n{warning}"))
                    }
                    (Err(e), None) => self.set_error_banner(e),
                    (Err(e), Some(warning)) => self.set_error_banner(format!("{e}\n\n{warning}")),
                }
            }
            Err(e) => self.set_error_banner(e.friendly_message(t)),
        }
    }
}

// --- Windows 11 (Fluent) palette ---------------------------------------------------------------
//
// Values below are the documented WinUI colour tokens rather than invented ones, so the window sits
// next to Settings or File Explorer without looking like a different toolkit. The accent itself is
// read from the user's own Windows accent palette, in the shade Fluent specifies per theme.

/// `SolidBackgroundFillColorBase` — the window itself.
pub fn win_background_for(is_light: bool) -> egui::Color32 {
    win_background(is_light)
}

fn win_background(is_light: bool) -> egui::Color32 {
    if is_light {
        egui::Color32::from_rgb(0xF3, 0xF3, 0xF3)
    } else {
        egui::Color32::from_rgb(0x20, 0x20, 0x20)
    }
}

/// `CardBackgroundFillColorDefault` — raised surfaces such as a folder or a settings group.
fn win_card(is_light: bool) -> egui::Color32 {
    if is_light {
        egui::Color32::from_rgb(0xFB, 0xFB, 0xFB)
    } else {
        egui::Color32::from_rgb(0x2B, 0x2B, 0x2B)
    }
}

/// `ControlFillColorDefault` — the body of a button or other control.
pub fn win_control_for(is_light: bool) -> egui::Color32 {
    win_control(is_light)
}

fn win_control(is_light: bool) -> egui::Color32 {
    if is_light {
        egui::Color32::from_rgb(0xFD, 0xFD, 0xFD)
    } else {
        egui::Color32::from_rgb(0x2D, 0x2D, 0x2D)
    }
}

/// `ControlFillColorSecondary` — the same control while the pointer is over it.
fn win_control_hover(is_light: bool) -> egui::Color32 {
    if is_light {
        egui::Color32::from_rgb(0xF6, 0xF6, 0xF6)
    } else {
        egui::Color32::from_rgb(0x32, 0x32, 0x32)
    }
}

/// `ControlFillColorTertiary` — pressed.
fn win_control_active(is_light: bool) -> egui::Color32 {
    if is_light {
        egui::Color32::from_rgb(0xF5, 0xF5, 0xF5)
    } else {
        egui::Color32::from_rgb(0x27, 0x27, 0x27)
    }
}

/// `ControlStrokeColorDefault` — the hairline around controls and cards.
fn win_stroke(is_light: bool) -> egui::Color32 {
    if is_light {
        egui::Color32::from_rgb(0xE5, 0xE5, 0xE5)
    } else {
        egui::Color32::from_rgb(0x38, 0x38, 0x38)
    }
}

/// `TextFillColorPrimary`.
fn win_text(is_light: bool) -> egui::Color32 {
    if is_light {
        egui::Color32::from_rgb(0x1B, 0x1B, 0x1B)
    } else {
        egui::Color32::from_rgb(0xFF, 0xFF, 0xFF)
    }
}

/// `TextFillColorSecondary` — hints and supporting copy.
fn win_text_secondary(is_light: bool) -> egui::Color32 {
    if is_light {
        egui::Color32::from_rgb(0x5D, 0x5D, 0x5D)
    } else {
        egui::Color32::from_rgb(0xC5, 0xC5, 0xC5)
    }
}

/// The user's own Windows accent, in the shade Fluent uses for this theme.
///
/// This reads the registry, so it is called only when the palette is (re)built — never from
/// drawing code. Widgets that need the accent read it back out of the palette with [`accent`],
/// which is a field access.
fn read_system_accent(is_light: bool) -> egui::Color32 {
    let (r, g, b) = theme::system_accent(is_light);
    egui::Color32::from_rgb(r, g, b)
}

/// The accent colour currently in use.
///
/// [`tuned_visuals`] stores it in the palette, so every widget can read the same value for free
/// instead of each one going back to the registry. That distinction matters: this is called once
/// per folder and once per row, on every frame, and a registry read in that position turns a list
/// of eighty folders into eighty registry reads sixty times a second.
pub fn accent(visuals: &egui::Visuals) -> egui::Color32 {
    visuals.hyperlink_color
}

/// Bumps up the default egui text sizes and widget spacing so the interface reads comfortably at
/// this window's size instead of the tiny defaults egui ships with.
///
/// Applied once at startup and never again: it only touches `text_styles` and `spacing`, which
/// `Context::set_visuals` does not overwrite — unlike the palette in [`tuned_visuals`], which has to
/// be re-applied on every theme switch.
fn apply_layout_style(ctx: &egui::Context, text_scale: f32) {
    // Windows 11's own type ramp, in the same units egui measures in, rather than egui's defaults
    // multiplied by some factor. Scaling the defaults is how the interface ended up with 11-point
    // labels: egui's "small" starts at 9, and no amount of multiplying makes a 9 into a caption.
    //
    //   Body / Button  14   the size nearly everything is set in
    //   Caption        12   hints and counts, and nothing that names a thing
    //   Subtitle       20   the heading at the top of a screen
    //   Monospace      13   triggers; a hair under body so it doesn't out-weigh it
    //
    // Anything that names something — a folder, a section, a field — is Body. Weight and colour do
    // the separating, not size; that is what keeps a hierarchy readable instead of merely small.
    let size = |points: f32| (points * text_scale).round();

    ctx.all_styles_mut(|style| {
        use egui::{FontFamily, FontId, TextStyle};
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(size(20.0), FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Body,
            FontId::new(size(14.0), FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(size(14.0), FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Small,
            FontId::new(size(12.0), FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(size(13.0), FontFamily::Monospace),
        );

        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 7.0);
        style.spacing.interact_size.y = 30.0;
        style.spacing.indent = 22.0;
    });
}

/// Repaints egui's stock palette in Windows 11's own colours.
///
/// Everything here is a plain colour/number swap the renderer already had to read anyway, so it
/// costs exactly nothing per frame — no extra textures, shadows or passes.
pub fn tuned_visuals(is_light: bool) -> egui::Visuals {
    let mut v = if is_light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    let accent = read_system_accent(is_light);
    let stroke = win_stroke(is_light);
    let text = win_text(is_light);
    let text_2 = win_text_secondary(is_light);

    // Fluent's control radius.
    let radius = egui::CornerRadius::same(4);

    v.widgets.noninteractive.bg_fill = win_card(is_light);
    v.widgets.noninteractive.weak_bg_fill = win_card(is_light);
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, stroke);
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, text);

    v.widgets.inactive.bg_fill = win_control(is_light);
    v.widgets.inactive.weak_bg_fill = win_control(is_light);
    // Anything you can press or type in gets the stronger border: it marks the edge of a target,
    // where a hairline only divides two things that are already separate.
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, line_strong(is_light));
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, text);

    v.widgets.hovered.bg_fill = win_control_hover(is_light);
    v.widgets.hovered.weak_bg_fill = win_control_hover(is_light);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, stroke);
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, text);

    v.widgets.active.bg_fill = win_control_active(is_light);
    v.widgets.active.weak_bg_fill = win_control_active(is_light);
    v.widgets.active.bg_stroke = egui::Stroke::new(1.0, accent);
    v.widgets.active.fg_stroke = egui::Stroke::new(1.0, text);

    v.widgets.open.bg_fill = win_control_hover(is_light);
    v.widgets.open.weak_bg_fill = win_control_hover(is_light);
    v.widgets.open.bg_stroke = egui::Stroke::new(1.0, stroke);
    v.widgets.open.fg_stroke = egui::Stroke::new(1.0, text);

    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = radius;
        w.expansion = 0.0;
    }

    // egui draws a vertical rule down the left of anything indented. In a design whose whole point
    // is removing strokes, that one puts a line beside every folder body. It lives in the visuals,
    // so it has to be set here — anywhere else and the next theme change wipes it.
    v.indent_has_left_vline = false;

    v.panel_fill = win_background(is_light);
    v.window_fill = win_card(is_light);
    v.window_stroke = egui::Stroke::new(1.0, stroke);
    // Text fields sit on the control colour rather than on pure white or near-black. With the boxes
    // gone from the list, a search box that glowed brighter than everything around it was the
    // loudest thing on the screen.
    v.extreme_bg_color = win_control(is_light);
    v.faint_bg_color = win_card(is_light);
    v.override_text_color = Some(text);
    v.weak_text_color = Some(text_2);

    v.selection.bg_fill = accent.gamma_multiply(if is_light { 0.28 } else { 0.38 });
    v.selection.stroke = egui::Stroke::new(1.0, accent);
    v.hyperlink_color = accent;

    v
}

/// `CardBackgroundFillColorDefault` — used for a folder's card.
pub fn folder_fill(is_light: bool) -> egui::Color32 {
    win_card(is_light)
}

/// The line between two rows. Slightly stronger than the stroke around a control: with the boxes
/// gone it is the only thing separating one expansion from the next, so it has to survive a cheap
/// panel at low brightness.
pub fn hairline(is_light: bool) -> egui::Color32 {
    if is_light {
        egui::Color32::from_rgb(0xDE, 0xDE, 0xDE)
    } else {
        egui::Color32::from_rgb(0x3B, 0x3B, 0x3B)
    }
}

/// The border around something you can type in or press — deliberately more visible than a
/// hairline, because it marks the edge of a target rather than a division between two.
pub fn line_strong(is_light: bool) -> egui::Color32 {
    if is_light {
        egui::Color32::from_rgb(0xD6, 0xD6, 0xD6)
    } else {
        egui::Color32::from_rgb(0x45, 0x45, 0x45)
    }
}

/// Supporting text: counts, hints, the quieter half of a pair. Darker on light than the mock had
/// it, which measured about 3.2:1 against the window and would have failed anyone reading on a
/// dim panel.
pub fn text_tertiary(is_light: bool) -> egui::Color32 {
    if is_light {
        egui::Color32::from_rgb(0x6E, 0x6E, 0x6E)
    } else {
        egui::Color32::from_rgb(0x94, 0x94, 0x94)
    }
}

pub fn secondary_text(is_light: bool) -> egui::Color32 {
    win_text_secondary(is_light)
}

/// Applies a theme to *everything*, including the window's own title bar.
///
/// The title bar is drawn by Windows, not by us, so it follows the system setting unless the window
/// is explicitly told otherwise — which is how a dark window could end up wearing a light title bar
/// after the theme was pinned or switched. Handing winit the same theme we just gave egui keeps the
/// two halves of the window in step (it forwards this to DWM's immersive-dark-mode attribute).
pub fn apply_theme(
    ctx: &egui::Context,
    window: Option<&winit::window::Window>,
    is_light: bool,
) {
    // egui does not keep *a* palette — it keeps two, one for light and one for dark, and swaps
    // between them by itself whenever the host reports that the system theme changed. Writing only
    // the currently active one therefore left the other as egui's stock palette, so a Windows theme
    // switch quietly replaced every Fluent colour with egui's defaults, and re-showing the window
    // from the tray brought that stale palette back with it.
    //
    // Both are written on every apply, so whichever one egui reaches for is ours.
    ctx.set_visuals_of(egui::Theme::Light, tuned_visuals(true));
    ctx.set_visuals_of(egui::Theme::Dark, tuned_visuals(false));

    // And the choice between them is made here, not by egui: someone who pinned "Tema claro" has
    // to stay on light even while Windows is dark, which egui's own following would override.
    ctx.set_theme(if is_light {
        egui::ThemePreference::Light
    } else {
        egui::ThemePreference::Dark
    });

    if let Some(window) = window {
        window.set_theme(Some(if is_light {
            winit::window::Theme::Light
        } else {
            winit::window::Theme::Dark
        }));
    }
}

/// Shrinks and nudges the window so it sits entirely inside `area`.
///
/// Only ever makes the window smaller or moves it back on-screen; a window that already fits is
/// left exactly where the user put it.
fn fit_window_to(window: &winit::window::Window, area: crate::display::WorkArea) {
    let scale = window.scale_factor();
    // Measured and set in the same units. This used to read the *outer* size and then set the
    // *inner* one, so every correction quietly shaved off the title bar as well.
    let inner = window.inner_size().to_logical::<f32>(scale);

    // A ceiling *and* a floor. Too big is a nuisance you can drag back; too small is not — below a
    // certain size the window has no edges left to grab and no buttons left to press, which is
    // exactly the state it came back in from the tray. On a screen too small for even the minimum,
    // the screen wins, because a window larger than its monitor is still recoverable.
    let ceiling = area.fits([inner.width.max(1.0), inner.height.max(1.0)]);
    let wanted = [
        ceiling[0].max(crate::display::MIN_SIZE[0].min(area.width)),
        ceiling[1].max(crate::display::MIN_SIZE[1].min(area.height)),
    ];
    if (wanted[0] - inner.width).abs() > 1.0 || (wanted[1] - inner.height).abs() > 1.0 {
        let _ = window.request_inner_size(winit::dpi::LogicalSize::new(wanted[0], wanted[1]));
    }

    let Ok(pos) = window.outer_position() else {
        return;
    };
    let pos = pos.to_logical::<f32>(scale);
    let x = pos.x.clamp(area.left, (area.left + area.width - wanted[0]).max(area.left));
    let y = pos.y.clamp(area.top, (area.top + area.height - wanted[1]).max(area.top));
    if (x - pos.x).abs() > 1.0 || (y - pos.y).abs() > 1.0 {
        window.set_outer_position(winit::dpi::LogicalPosition::new(x, y));
    }
}

/// The native handle behind the winit window, for the few things winit cannot do itself.
fn hwnd_of(window: &winit::window::Window) -> Option<windows::Win32::Foundation::HWND> {
    use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(windows::Win32::Foundation::HWND(
            handle.hwnd.get() as *mut std::ffi::c_void,
        )),
        _ => None,
    }
}

/// The work area the window should be measured against: the monitor it is on, falling back to the
/// primary one only when that cannot be determined.
fn work_area_for(window: &winit::window::Window) -> Option<crate::display::WorkArea> {
    let scale = window.scale_factor() as f32;
    hwnd_of(window)
        .and_then(|hwnd| crate::display::work_area_of_window(hwnd, scale))
        .or_else(|| crate::display::work_area(scale))
}

/// Blends two colours, `t` of the way from `a` to `b`.
pub fn mix(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgb(
        lerp(a.r(), b.r()),
        lerp(a.g(), b.g()),
        lerp(a.b(), b.b()),
    )
}

/// Black or white, whichever is readable on `background`.
///
/// The accent is the user's, not ours — it can be pale yellow or near-black, and a primary button
/// with hard-coded white text becomes unreadable the moment somebody picks the wrong one. Uses the
/// standard luminance weights rather than a plain average, because the eye is far more sensitive to
/// green than to blue.
pub fn readable_on(background: egui::Color32) -> egui::Color32 {
    let luminance = 0.2126 * background.r() as f32
        + 0.7152 * background.g() as f32
        + 0.0722 * background.b() as f32;
    if luminance > 140.0 {
        egui::Color32::from_rgb(0x10, 0x10, 0x14)
    } else {
        egui::Color32::WHITE
    }
}

/// The tint a selected row sits on: the surface pulled towards the accent, far enough to be
/// unmistakable and not so far that the trigger written in that same accent stops standing out.
pub fn selection_tint(is_light: bool, accent: egui::Color32) -> egui::Color32 {
    mix(
        win_background(is_light),
        accent,
        if is_light { 0.11 } else { 0.14 },
    )
}

/// The faint wash under the row the pointer is over. Deliberately much weaker than the selection,
/// so "where my mouse is" never competes with "what I have chosen".
pub fn hover_tint(is_light: bool) -> egui::Color32 {
    if is_light {
        egui::Color32::from_rgb(0xE9, 0xE9, 0xEC)
    } else {
        egui::Color32::from_rgb(0x2A, 0x2A, 0x2E)
    }
}

/// A red that stays legible on either theme — Windows' own error red on light, its lighter
/// counterpart on dark, rather than one colour that is too dark on one and too harsh on the other.
pub fn danger(is_light: bool) -> egui::Color32 {
    if is_light {
        egui::Color32::from_rgb(0xC4, 0x2B, 0x1C)
    } else {
        egui::Color32::from_rgb(0xFF, 0x99, 0xA4)
    }
}

/// An amber for "this will probably not do what you want" — louder than a hint, quieter than an
/// error, because nothing here is wrong yet. Windows' own caution colour on light, and a lighter
/// version of it on dark, for the same reason `danger` has two.
pub fn caution(is_light: bool) -> egui::Color32 {
    if is_light {
        egui::Color32::from_rgb(0x8A, 0x5B, 0x00)
    } else {
        egui::Color32::from_rgb(0xF2, 0xC0, 0x5C)
    }
}

pub struct EspansoManagerApp {
    state: AppState,
    tray: Tray,
    /// Tray and menu events, delivered by the threads that wait on them rather than fetched by a
    /// timer. See [`crate::tray::spawn_event_pumps`].
    tray_events: TrayEvents,
    window: Option<Arc<winit::window::Window>>,
    last_theme_check: Instant,
    /// Listens for Windows appearance broadcasts. `None` if it could not be started, in which
    /// case `poll_theme` falls back to its timer.
    theme_watch: Option<crate::sysevents::ThemeWatch>,
    /// Alt+Space, when this app managed to claim it. `None` means something else already owns the
    /// shortcut, and espanso's own search bar has been left switched on instead.
    hotkey: Option<crate::hotkey::HotkeyWatch>,
    /// While `now` is before this, the appearance is re-read every frame: Windows announces a
    /// change slightly before all the values behind it have settled.
    theme_recheck_until: Instant,
    is_light: bool,
    /// The accent the current palette was built from, so a change to it in Windows can be noticed
    /// without re-reading the registry from drawing code.
    last_accent: egui::Color32,
    start_hidden_pending: bool,
    /// Set by the tray's Quit item. The window's close handler normally cancels the close and hides
    /// to the tray instead; this is what tells it that this particular close is meant.
    quitting: bool,
}

pub struct StartupContext {
    pub match_file: MatchFile,
    pub match_file_path: PathBuf,
    pub backups_dir: PathBuf,
    pub settings: Settings,
    pub settings_store: SettingsStore,
    pub ctl: EspansoCtl,
    pub exe_path: PathBuf,
    pub config_path: PathBuf,
    pub tray: Tray,
    pub start_hidden: bool,
    /// Set if something already went wrong before the window even opened (e.g. the daemon
    /// failed to start), so we can surface it as soon as the UI is visible instead of it being
    /// silently lost.
    pub startup_warning: Option<String>,
    /// Set if the expansions file exists but could not be read. The model is the empty fallback,
    /// so saving stays refused until the app is restarted against a file it can parse.
    pub load_error: Option<String>,
}

impl EspansoManagerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, mut ctx: StartupContext) -> Self {
        let is_light = ctx.settings.theme_mode.is_light();
        let window = cc.winit_window().cloned();
        // A size remembered on one machine travels with the folder to the next one, so a window
        // saved on a large monitor can arrive on a laptop taller than the screen. Correct it before
        // anything is drawn, and size the text to the screen that is actually here.
        let work_area = window
            .as_deref()
            .and_then(|w| crate::display::work_area(w.scale_factor() as f32))
            .or_else(|| crate::display::work_area(crate::display::system_scale()));
        if let Some(window) = window.as_deref() {
            if let Some(area) = work_area_for(window).or(work_area) {
                fit_window_to(window, area);
            }
        }
        // 1.0 when the screen cannot be measured, never more: the sizes in `apply_layout_style` are
        // Windows 11's own, so unscaled is already correct and a larger fallback would blow the
        // type up on precisely the machine we know least about.
        let text_scale = work_area.map_or(1.0, |a| crate::display::text_scale(a.height));
        apply_layout_style(&cc.egui_ctx, text_scale);
        apply_theme(&cc.egui_ctx, window.as_deref(), is_light);
        let font_status = fonts::install(&cc.egui_ctx, ctx.settings.lang == Lang::Hi);

        // Whoever owns Alt+Space also owns the search window, and only one of us can.
        //
        // With [`OWN_LAUNCHER`] off, espanso keeps the shortcut and its own bar, and this app
        // registers nothing at all — which is the one place ours could possibly get in the way.
        // With it on, espanso's bar is switched off *first* and only then is the shortcut claimed:
        // espanso holds the key while it is running, so asking for it first could only ever fail,
        // and the restart is what makes it let go. Either way the setting is written on every
        // start, so the two can never both be on.
        let wanted = if OWN_LAUNCHER { "OFF" } else { "ALT+SPACE" };
        match crate::config_patch::set_search_shortcut(&ctx.config_path, wanted) {
            Ok(true) => {
                let _ = ctx.ctl.restart_and_confirm(Duration::from_secs(6), ctx.settings.t());
            }
            Ok(false) => {}
            // Joins whatever main.rs already had to say, since the banner that carries it has not
            // been raised yet at this point. Silence here would mean the search bar quietly
            // belonging to nobody.
            //
            // Not added twice, though. This is the same file main.rs failed on a moment ago, so
            // when it is unreadable both attempts produce the identical paragraph, and a banner
            // that says the same thing twice reads as a bug in the banner.
            Err(e) => {
                let text = crate::i18n::fill(
                    ctx.settings.t().config_patch_warning,
                    &[("err", &e.to_string())],
                );
                ctx.startup_warning = Some(match ctx.startup_warning.take() {
                    Some(existing) if existing.contains(&text) => existing,
                    Some(existing) => format!("{existing}\n\n{text}"),
                    None => text,
                });
            }
        }
        let hotkey = if OWN_LAUNCHER {
            crate::hotkey::start(&cc.egui_ctx)
        } else {
            None
        };

        // Built before the tray is handed over to the app: the pump has to recognise the Quit item
        // by its own id, so that it can end the program without waiting for a frame that, hidden in
        // the tray, may never come.
        let tray_events = crate::tray::spawn_event_pumps(
            &cc.egui_ctx,
            crate::tray::QuitPlan {
                id: ctx.tray.quit_id(),
                ctl: ctx.ctl.clone(),
            },
        );

        let mut state = AppState {
            match_file: ctx.match_file,
            match_file_path: ctx.match_file_path,
            backups_dir: ctx.backups_dir,
            settings: ctx.settings,
            settings_store: ctx.settings_store,
            ctl: ctx.ctl,
            view: View::List,
            search: String::new(),
            banner: None,
            load_error: ctx.load_error,
            autostart_enabled: autostart::is_enabled(),
            exe_path: ctx.exe_path,
            selected: BTreeSet::new(),
            selection_anchor: None,
            renaming_folder: None,
            pending_confirm: None,
            pending_language_refresh: false,
            pending_theme_refresh: false,
            onboarding_probe: String::new(),
            onboarding_expanded: false,
            search_open: false,
            search_query: String::new(),
            search_selected: 0,
            search_return_to: 0,
            search_needs_focus: false,
            search_had_focus: false,
            search_handoff: None,
            search_follow: false,
            espanso_version: None,
            espanso_version_asked: false,
            search_shortcut: "Alt+Space",
            list_revision: 0,
            list_cache: None,
            list_cache_key: None,
        };
        state.drop_orphan_folders();
        if let Some(watch) = &hotkey {
            state.search_shortcut = watch.shortcut.label;
        }

        if let Some(warning) = ctx.startup_warning {
            state.set_error_banner(warning);
        }

        // First run gets the one screen that stands in for espanso's three-window wizard. Every
        // other run clears whatever an interrupted first run may have left behind, so its trial
        // expansion can never quietly become a permanent match nobody can see.
        if state.settings.onboarding_done {
            state.clear_onboarding_match();
        } else {
            state.begin_onboarding();
            state.view = View::Onboarding;
        }
        // Only ever true for somebody who has chosen Hindi: it is the one case where the face is
        // asked for at all. It used to be raised by a missing *symbols* font too, and raised
        // regardless of language — a red error about Devanagari at every start, on machines where
        // nobody had asked for it and nothing was actually wrong with what they could see.
        if font_status.devanagari_missing {
            state.set_error_banner(state.t().hindi_font_missing);
        }

        Self {
            state,
            tray: ctx.tray,
            tray_events,
            window,
            last_theme_check: Instant::now(),
            theme_watch: crate::sysevents::ThemeWatch::start(cc.egui_ctx.clone()),
            hotkey,
            theme_recheck_until: Instant::now(),
            is_light,
            last_accent: read_system_accent(is_light),
            start_hidden_pending: ctx.start_hidden,
            quitting: false,
        }
    }

    /// Applies a language change: swaps the font set (some scripts need a system font egui doesn't
    /// bundle) and re-labels the tray, neither of which `AppState` can reach on its own.
    fn apply_language_change(&mut self, ctx: &egui::Context) {
        // The font set is rebuilt, because one of its faces belongs to one language: the 5.3 MB
        // Devanagari collection is picked up when Hindi is chosen and let go again when it is not.
        // Everything else in the set is the same either way, so this is the only moment it can
        // change and the only moment the missing-font warning can become true or stop being true.
        let status = fonts::install(ctx, self.state.settings.lang == Lang::Hi);
        if status.devanagari_missing {
            self.state.set_error_banner(self.state.t().hindi_font_missing);
        }
        self.tray.relabel(self.state.t());
    }

    fn hide_to_tray(&self) {
        if let Some(window) = &self.window {
            window.set_visible(false);
        }
    }

    /// Shows the window from the tray — and makes sure it comes back somewhere it can be used.
    ///
    /// Three things can be wrong by the time this runs, none of them the user's doing: the size
    /// restored from `ui_state.ron` may have been recorded on a different machine, the position may
    /// be on a monitor that has since been unplugged, and the scale factor may have changed when
    /// the laptop was docked. Each of those on its own produces a window that is tiny, off-screen,
    /// or both — and a window with no taskbar button, in those states, is genuinely hard to get
    /// back. So the geometry is checked against the monitor it is actually on, every time, and
    /// corrected before anyone sees it.
    fn show_and_focus(&self) {
        let Some(window) = &self.window else {
            return;
        };
        window.set_visible(true);
        if let Some(area) = work_area_for(window) {
            fit_window_to(window, area);
        }
        window.focus_window();
        // winit's own focus call is `SetForegroundWindow`, which Windows ignores for a process that
        // does not own the last input event — and a click on a tray icon is input to the shell, not
        // to us. Without this the window reappears behind everything else on a busy desktop.
        if let Some(hwnd) = hwnd_of(window) {
            crate::display::bring_to_front(hwnd);
        }
    }

    /// Opens the search window when Alt+Space is pressed, or closes it if it is already up.
    ///
    /// The window that had the keyboard is remembered here, before ours takes it — that is where
    /// the expansion has to end up, and by the time anything is chosen it is far too late to ask.
    fn poll_hotkey(&mut self) {
        let Some(hotkey) = &self.hotkey else {
            return;
        };
        if !hotkey.take_pressed() {
            return;
        }
        if self.state.search_open {
            self.state.close_search();
            return;
        }
        self.state.search_return_to = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow().0 as isize
        };
        self.state.search_query.clear();
        self.state.search_selected = 0;
        self.state.search_open = true;
        self.state.search_needs_focus = true;
        self.state.search_had_focus = false;
        self.state.search_follow = true;
    }

    /// Draws the search window, and performs the expansion once one is picked.
    ///
    /// The order at the end matters: the window goes away, the keyboard goes back to where it came
    /// from, and only then is espanso asked to type. Espanso injects into whatever has focus at the
    /// moment it runs, so handing the focus back first is the whole trick.
    fn show_search_window(&mut self, ctx: &egui::Context) {
        if !self.state.search_open {
            self.hand_over_expansion();
            return;
        }
        // Sized to what it is actually showing, the way every launcher does: the box grows and
        // shrinks as the query narrows, instead of leaving a slab of empty window under the last
        // result. Measured from the real font metrics rather than guessed — the guess used to come
        // up short and slice the footer line off the bottom edge.
        let rows = crate::ui::search_view::hits(&self.state, &self.state.search_query).len();
        let size = crate::ui::search_view::window_size(ctx, rows);
        let mut builder = egui::ViewportBuilder::default()
            .with_title("EspansoManager")
            .with_inner_size(size)
            .with_decorations(false)
            .with_resizable(false)
            .with_always_on_top()
            .with_active(true)
            .with_taskbar(false);
        // Centred on the screen the person is actually working on, not on the primary one.
        let previous = windows::Win32::Foundation::HWND(self.state.search_return_to as *mut _);
        let area = crate::display::work_area_of_window(previous, 1.0)
            .or_else(|| crate::display::work_area(1.0));
        if let Some(area) = area {
            builder = builder.with_position(egui::pos2(
                area.left + (area.width - size.x) * 0.5,
                area.top + (area.height - size.y) * 0.35,
            ));
        }

        let mut chosen = None;
        let state = &mut self.state;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("espanso_manager_search"),
            builder,
            |ctx, _class| {
                // Asked for explicitly rather than left to the window manager. A hotkey press gives
                // this process the right to take the foreground, but only if it actually asks —
                // and a search box that does not have the keyboard is worse than none at all,
                // because the keystrokes land in whatever document was underneath.
                if state.search_needs_focus {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    state.search_needs_focus = false;
                }

                // Clicking anywhere else dismisses it, the way every launcher on every platform
                // behaves — including espanso's own. Gated on having held the keyboard first,
                // because the frame between asking for focus and getting it also reports "not
                // focused", and closing there would make the window flash and vanish.
                let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
                if focused {
                    state.search_had_focus = true;
                } else if state.search_had_focus {
                    state.close_search();
                }
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        chosen = crate::ui::search_view::show(ui, state);
                    });
                if ctx.input(|i| i.viewport().close_requested()) {
                    state.close_search();
                }
            },
        );

        if let Some(trigger) = chosen {
            // Nothing is handed back on this frame. The window is only *asked* to close here; egui
            // does not destroy it until the frame has finished drawing, and asking for the
            // keyboard back while it is still alive makes the focus flicker away and straight
            // back again. Deferring by one frame removes that flicker rather than waiting it out,
            // which is worth about a fifth of a second of the delay you could feel.
            self.state.close_search();
            self.state.search_handoff = Some(trigger);
            ctx.request_repaint();
        }
    }

    /// Gives the keyboard back to the window the search was opened over, and asks espanso to
    /// expand there.
    ///
    /// Runs on the frame *after* the search window closed, so by now it is genuinely gone and the
    /// focus can settle in one move.
    ///
    /// The trigger is typed out first, and it has to be. `RequestMatchExpansion` — the message
    /// behind `espanso match exec`, and the only expansion entry point espanso offers to anything
    /// outside itself — does not mean "type this expansion". It means "expand as though this
    /// trigger had just been typed", so espanso sends one backspace per character of the trigger
    /// to rub out what it assumes the person typed. Measured, not guessed: `:fe` eats exactly
    /// three characters and `:testing` exactly eight. Espanso's own search bar never did this
    /// because it picks a match by identity and tells its engine there was no trigger at all, but
    /// that door is internal to espanso; from out here the only way to be right is to make its
    /// assumption true. So the trigger goes in, espanso's backspaces take it out again, and what
    /// the document is left with is the replacement and nothing else — exactly what it would hold
    /// had the trigger been typed by hand. Espanso ignores injected keystrokes for its own
    /// detection, so this cannot set off a second expansion of its own.
    ///
    /// If the keyboard never actually arrives, nothing is typed and the expansion is still
    /// requested: a stray trigger left behind in someone's document is the worse outcome.
    ///
    /// The keyboard arriving and the typing being *refused* is a third case, and it goes the other
    /// way. Windows blocks injected input into a window owned by a higher-integrity process, and
    /// there the focus check passes while nothing is typed at all — so an expansion request would
    /// have espanso backspace over as many characters of the user's own document as the trigger is
    /// long. Nothing happening is the better failure.
    fn hand_over_expansion(&mut self) {
        let Some(trigger) = self.state.search_handoff.take() else {
            return;
        };

        let target = self.state.search_return_to;
        let previous = windows::Win32::Foundation::HWND(target as *mut _);
        if !previous.is_invalid() {
            crate::display::bring_to_front(previous);
        }

        let ctl = self.state.ctl.clone();
        std::thread::spawn(move || {
            let focused = target != 0
                && crate::display::wait_until_foreground(
                    target,
                    Duration::from_millis(40),
                    Duration::from_millis(600),
                );
            if focused && !crate::display::type_text(&trigger) {
                return;
            }
            let _ = ctl.match_exec(&trigger);
        });
    }

    fn drain_tray_events(&self) {
        loop {
            match self.tray_events.tray.try_recv() {
                Ok(TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }) => self.show_and_focus(),
                Ok(_) => {}
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return,
            }
        }
    }

    fn drain_menu_events(&mut self, ctx: &egui::Context) {
        let t = self.state.t();
        while let Ok(event) = self.tray_events.menu.try_recv() {
            if self.tray.is_quit(&event.id) {
                // Only the polite half of quitting happens here: let the close through, so eframe
                // shuts the window down in order and writes its saved size and position. Stopping
                // espanso and ending the process belong to the menu pump — see `tray::finish_quit`
                // — because those must happen whether or not this frame ever runs, and on a
                // machine hiding in the tray with nothing to repaint, it may not.
                self.quitting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            if let Some(Err(message)) = self.tray.handle_menu_id(&event.id, &self.state.ctl, t) {
                self.state.set_error_banner(message);
            }
        }
    }

    /// Keeps the window's title bar — which Windows draws, not us — matching the theme the rest of
    /// the window is using.
    ///
    /// Setting it once when the theme changes isn't enough: at startup the window is still being
    /// set up and winit applies the *system* theme to it after our call, which is how a light
    /// window could end up under a dark title bar. Comparing against what the window actually
    /// reports and correcting any mismatch fixes that case and any other, and costs one integer
    /// comparison per frame.
    fn sync_title_bar(&self) {
        let Some(window) = &self.window else {
            return;
        };
        let wanted = if self.is_light {
            winit::window::Theme::Light
        } else {
            winit::window::Theme::Dark
        };
        if window.theme() != Some(wanted) {
            window.set_theme(Some(wanted));
        }
    }

    /// Keeps the palette in step with Windows' own appearance settings: light/dark, and the user's
    /// accent colour.
    ///
    /// Normally nothing here touches the registry at all. Windows announces both changes by
    /// broadcasting a message, and [`crate::sysevents::ThemeWatch`] is listening for it on a thread
    /// of its own; the check below is then a single atomic read, and the registry is only consulted
    /// on the frame where something actually changed. That also makes the change *immediate*
    /// instead of arriving up to two seconds late.
    ///
    /// The timer survives as a fallback for the case where that listener could not be created. It
    /// is better to update a couple of seconds late than to sit on a stale palette forever.
    fn poll_theme(&mut self, ctx: &egui::Context) {
        let now = Instant::now();

        if let Some(watch) = &self.theme_watch {
            if watch.take_changed() {
                // Windows announces the change before everything behind it has settled — the
                // accent palette in particular is written by a different component than the
                // light/dark flag. Reading once, immediately, can therefore pick up the old value
                // and then not look again for half a minute. So an announcement opens a short
                // window during which we keep re-reading.
                self.theme_recheck_until = now + Duration::from_millis(1500);
            }
        }

        if now < self.theme_recheck_until {
            // Keep frames coming while the window is open, so the re-reads actually happen.
            ctx.request_repaint_after(Duration::from_millis(150));
        } else {
            // How often to look when nothing has been announced. Two seconds if there is no
            // listener at all; a lazy half-minute when there is one, purely so a notification we
            // never received — a tool that edits the registry without telling anyone, a broadcast
            // lost while the machine was busy — repairs itself instead of leaving the palette wrong
            // until the app restarts. One registry read every 30 seconds is not a cost worth
            // optimising away, and it stops the event path being a single point of failure.
            let backstop = if self.theme_watch.is_some() {
                Duration::from_secs(30)
            } else {
                Duration::from_secs(2)
            };
            if self.last_theme_check.elapsed() < backstop {
                return;
            }
        }
        self.last_theme_check = now;

        let mode = self.state.settings.theme_mode;
        let is_light = if mode.follows_system() {
            mode.is_light()
        } else {
            self.is_light
        };
        let accent_now = read_system_accent(is_light);
        if is_light != self.is_light || accent_now != self.last_accent {
            self.is_light = is_light;
            self.last_accent = accent_now;
            apply_theme(ctx, self.window.as_deref(), is_light);
        }
    }

    fn show_banner(&mut self, ui: &mut egui::Ui) {
        let t = self.state.t();
        let Some(banner) = &mut self.state.banner else {
            return;
        };
        let (bg, fg) = match banner.kind {
            BannerKind::Info => (egui::Color32::from_rgb(40, 90, 60), egui::Color32::WHITE),
            BannerKind::Error => (egui::Color32::from_rgb(120, 40, 40), egui::Color32::WHITE),
        };
        let mut close = false;
        let mut undo = false;
        egui::Frame::default()
            .fill(bg)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                // The buttons take their place first, and the message fills what is left of the
                // row, wrapping into as many lines as it needs.
                //
                // The obvious order was what stood here -- message, then buttons pushed to the
                // right -- and it cut long messages off at the window edge without a mark: a
                // label laid out along a horizontal row is offered unlimited width, so it never
                // wraps, and the frame just clips whatever overruns. These messages carry the
                // reason something failed, usually with Windows' own words at the end of the
                // sentence, which is exactly the part that went over the edge.
                ui.horizontal_top(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                        if ui.small_button("❌").on_hover_text(t.banner_close_tip).clicked() {
                            close = true;
                        }
                        if banner.undo.is_some() && ui.small_button(t.undo).clicked() {
                            undo = true;
                        }
                        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new(&banner.message).color(fg))
                                    .wrap(),
                            );
                        });
                    });
                });
            });
        if undo {
            self.state.undo_from_banner();
        } else if close {
            self.state.banner = None;
        }
    }
}

impl eframe::App for EspansoManagerApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.start_hidden_pending {
            self.start_hidden_pending = false;
            self.hide_to_tray();
        }

        self.drain_tray_events();
        self.drain_menu_events(ctx);
        self.poll_hotkey();
        self.show_search_window(ctx);

        if let Some(Err(message)) = self.tray.tick(&self.state.ctl, self.state.t()) {
            self.state.set_error_banner(message);
        }

        // Settings changes that need more than `AppState` to take effect are applied here, once,
        // rather than from inside the settings screen's own rendering.
        if std::mem::take(&mut self.state.pending_language_refresh) {
            self.apply_language_change(ctx);
        }
        if std::mem::take(&mut self.state.pending_theme_refresh) {
            self.is_light = self.state.settings.theme_mode.is_light();
            self.last_accent = read_system_accent(self.is_light);
            apply_theme(ctx, self.window.as_deref(), self.is_light);
        }

        self.poll_theme(ctx);
        self.sync_title_bar();

        // Esc backs out of whatever is on top: a pending confirmation first, then a selection.
        // egui reports it as a physical key, so it works the same on a keyboard that prints "Esc",
        // "Escape" or nothing at all. Deliberately limited to the list — in the editor it would
        // throw away whatever the user had just typed.
        if matches!(self.state.view, View::List) && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.state.pending_confirm.is_some() {
                self.state.cancel_pending_confirm();
            } else {
                self.state.clear_selection();
            }
        }

        // The X hides to the tray rather than quitting — except when the tray's own Quit item is
        // what asked for the close, which is the one way out of the program.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_to_tray();
        }

        // Hidden in the tray, the app now schedules nothing at all. Everything that used to need
        // finding out about arrives on its own: a tray click and a menu pick wake it through
        // `spawn_event_pumps`, a Windows appearance change through `sysevents`. The one exception is
        // a ten-minute pause, which ends at a time rather than on an event, so that alone books a
        // frame — and books it for the moment it is actually due.
        //
        // With the window open the slow heartbeat stays. A forced repaint there re-lays-out the
        // whole current view, so it is the expensive case, and two seconds is already lazy.
        let is_visible = self.window.as_ref().and_then(|w| w.is_visible()).unwrap_or(true);
        if is_visible {
            ctx.request_repaint_after(VISIBLE_POLL_INTERVAL);
        } else if let Some(until) = self.tray.timed_pause_deadline() {
            ctx.request_repaint_after(until.saturating_duration_since(Instant::now()));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // `show_collapsible` slides the panel in/out and correctly shrinks the space given to
        // everything below it as it animates, so the banner appearing or disappearing never
        // overlaps the rest of the interface — it always pushes it, never covers it.
        let mut banner_expanded = self.state.banner.is_some();
        egui::Panel::top("banner")
            .show_separator_line(false)
            .show_collapsible(ui, &mut banner_expanded, |ui| {
                self.show_banner(ui);
            });

        egui::CentralPanel::default().show(ui, |ui| match &self.state.view {
            View::List => ui::list_view::show(ui, &mut self.state),
            View::Edit(_) => ui::edit_form::show(ui, &mut self.state),
            View::Settings => ui::settings_view::show(ui, &mut self.state),
            View::Tips => ui::tips_view::show(ui, &mut self.state),
            View::Onboarding => ui::onboarding_view::show(ui, &mut self.state),
        });

        if matches!(self.state.view, View::List) {
            ui::list_view::show_selection_bar(ui.ctx(), &mut self.state);
        }
        ui::list_view::show_pending_confirm(ui.ctx(), &mut self.state);
    }
}
