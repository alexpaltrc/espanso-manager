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

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Start-up, in the order the pieces have to come up. The order is most of what this file says.
//!
//! The saved language is read first, because the two dialogs that can appear before there is any
//! window — "already running" and the panic dialog — have to speak it. Then the panic hook, so
//! that anything failing after this point is visible: the `windows_subsystem` attribute above
//! builds this without a console, so a panic without the hook is a program that simply vanishes
//! off the screen.
//!
//! Then the single-instance mutex, then espanso's wizard flags, then the window. Note the mutex is
//! named per *session*, not per folder: a second copy started from a different folder is still a
//! second copy, and two tray icons for one program is the thing being prevented.
//!
//! Warnings gathered along the way are not shown as they happen. Nothing here is worth refusing to
//! start over, so they are carried in `StartupContext` and put on screen together once there is
//! somewhere to put them.


mod app;
mod autostart;
mod config_patch;
mod datefmt;
mod display;
mod espanso_ctl;
mod fonts;
mod hotkey;
mod i18n;
mod settings;
mod sysevents;
mod theme;
mod transfer;
mod tray;
mod ui;
mod yaml;

use app::{EspansoManagerApp, StartupContext};
use eframe::egui;
use espanso_ctl::EspansoCtl;
use settings::SettingsStore;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::CreateMutexW;

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// What came of asking Windows for the only-one-copy mutex.
///
/// Three outcomes, and there used to be two: "another copy is running" and "the mutex could not be
/// created" both arrived as the same `None`, and the caller left on either. The second one is a
/// door with no sign on it -- double-clicking the program produced no window, no tray icon, no
/// message, nothing whatsoever -- and the failure behind it is not exotic on an office machine:
/// run one copy as administrator and its mutex carries a high integrity label, so the next
/// ordinary launch is denied access to it rather than told that it already exists.
enum InstanceGuard {
    /// We hold it. The handle stays alive for as long as the program runs.
    Held(windows::Win32::Foundation::HANDLE),
    /// Another copy holds it. The user has been shown where that copy is; this one should go.
    AlreadyRunning,
    /// Windows would not hand it over at all, so whether another copy is running is unknowable.
    /// Carries the reason, to be shown once there is a window to show it in.
    Unavailable(String),
}

/// Prevents two copies running at once (e.g. the autostart entry firing while the user also
/// double-clicks the exe), which would otherwise create two tray icons. We deliberately don't try
/// to "wake up" the other instance's window — just tell the user where it already is.
fn acquire_single_instance_guard(t: &'static i18n::Strings) -> InstanceGuard {
    let name = wide("Local\\EspansoManager-SingleInstance-9f3d2b7a");
    // Reading the thread's last error straight afterwards is sound: on the success path nothing
    // in between touches it. `CreateMutexW` builds its error lazily, with
    // `.ok_or_else(Error::from_thread)`, so ERROR_ALREADY_EXISTS is still there to be read.
    let handle = match unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) } {
        Ok(handle) => handle,
        Err(e) => return InstanceGuard::Unavailable(e.message()),
    };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        rfd::MessageDialog::new()
            .set_title("EspansoManager")
            .set_description(t.already_running)
            .set_level(rfd::MessageLevel::Info)
            .show();
        return InstanceGuard::AlreadyRunning;
    }
    InstanceGuard::Held(handle)
}

fn install_panic_hook(t: &'static i18n::Strings) {
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown error".to_string());
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        rfd::MessageDialog::new()
            .set_title(t.panic_title)
            .set_description(i18n::fill(
                t.panic_body,
                &[("location", &location), ("message", &message)],
            ))
            .set_level(rfd::MessageLevel::Error)
            .show();
    }));
}

fn main() {
    let exe_path = std::env::current_exe().expect("could not determine the executable's path");
    let base_dir = exe_path
        .parent()
        .expect("the executable should have a containing folder")
        .to_path_buf();

    // The saved language has to be read before anything can be shown, since even the earliest
    // dialogs (already-running, panic) need to speak it.
    let manager_dir = base_dir.join(".espanso-manager");
    let settings_store = SettingsStore::new(&manager_dir);
    // The language this message will be written in is the default one, not the user's: their
    // choice was in the file that could not be read. Nothing can be done about that, and it is
    // still far better than the silence this replaced.
    let (settings, settings_issue) = settings_store.load();
    let t = settings.t();

    install_panic_hook(t);

    // Warnings gathered before the window exists, shown together once it does. Nothing here is
    // worth refusing to start over, and nothing here should be lost either.
    let mut startup_warnings: Vec<String> = Vec::new();

    // The folder assignments are gone in every one of these cases. Which one it was decides
    // whether there is still a copy to go and fetch them from, so each says something different.
    match settings_issue {
        Some(settings::SettingsIssue::Damaged { kept_at: Some(path) }) => {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());
            startup_warnings.push(i18n::fill(t.settings_damaged, &[("name", &name)]));
        }
        Some(settings::SettingsIssue::Damaged { kept_at: None }) => {
            startup_warnings.push(t.settings_damaged_lost.to_string());
        }
        Some(settings::SettingsIssue::Unreadable(err)) => {
            startup_warnings.push(i18n::fill(t.settings_unreadable, &[("err", &err)]));
        }
        None => {}
    }

    let _instance_guard = match acquire_single_instance_guard(t) {
        InstanceGuard::Held(handle) => handle,
        InstanceGuard::AlreadyRunning => return,
        InstanceGuard::Unavailable(err) => {
            // Starting without the guard means a second copy could appear alongside a first. That
            // is a nuisance, and a visible, self-explanatory one — two icons in the tray. Refusing
            // to start, with nothing on screen to say why, is the worse of the two by a distance.
            // `HANDLE` is a bare pointer with no destructor, so a null one costs nothing to hold.
            startup_warnings.push(i18n::fill(t.instance_guard_warning, &[("err", &err)]));
            windows::Win32::Foundation::HANDLE::default()
        }
    };

    let start_hidden = std::env::args().any(|a| a == "--tray");

    let espansod_path = base_dir.join("espansod.exe");
    let ctl = EspansoCtl::new(espansod_path);

    silence_espanso_wizard(&ctl, t, &base_dir.join(".espanso-runtime"));

    if let Err(e) = ctl.ensure_running(t) {
        startup_warnings.push(i18n::fill(t.startup_espanso_warning, &[("err", &e)]));
    }

    let config_path = base_dir.join(".espanso").join("config").join("default.yml");

    let match_file_path = base_dir.join(".espanso").join("match").join("base.yml");
    // Set when there are bytes on disk that could not be turned into a model. While it is set the
    // app refuses to save, because the model below is empty and writing an empty model back would
    // replace every expansion the user has — plus any `imports:` or `global_vars:` — with nothing.
    // The dialog says the file could not be read; it must not then be the thing that destroys it.
    let mut load_error: Option<String> = None;
    let match_file = match yaml::io::load(&match_file_path) {
        Ok(mf) => mf,
        Err(e) => {
            let message = e.friendly_message(t);
            rfd::MessageDialog::new()
                .set_title("EspansoManager")
                .set_description(message.as_str())
                .set_level(rfd::MessageLevel::Error)
                .show();
            // "There is no file yet" is the one failure where writing a fresh one is exactly
            // right, and saving already does that. Latching on it would leave a folder without a
            // base.yml permanently unable to gain one.
            let missing = matches!(
                &e,
                yaml::io::LoadError::Io(io) if io.kind() == std::io::ErrorKind::NotFound
            );
            if !missing {
                load_error = Some(message);
            }
            yaml::model::MatchFile::default()
        }
    };

    let backups_dir = manager_dir.join("backups");

    let runtime_dir = base_dir.join(".espanso-runtime");
    let tray = match tray::Tray::new(&runtime_dir, t) {
        Ok(tray) => tray,
        Err(e) => {
            // Leaving is fine. Leaving *quietly* is not: espanso is already running by now, and on
            // any folder this app has opened before, its icon was turned off on a previous run and
            // is off again the moment the daemon reads the config. Walking out at this point would
            // hand the user a program typing into their documents with nothing anywhere to stop
            // it. So the icon goes back to espanso first, and the daemon is told to re-read the
            // file, before anything is said or this process ends.
            let restored = config_patch::restore_espanso_icon(&config_path);
            if restored.as_ref().copied().unwrap_or(false) {
                let _ = ctl.restart_and_confirm(std::time::Duration::from_secs(6), t);
            }
            // This is the one place where failing to write that file is not an inconvenience but
            // the whole disaster the block above exists to prevent: no tray icon from us, and now
            // no way to give espanso its own back either. It goes in the same dialog rather than a
            // second one, because there is only one thing to say and one moment left to say it.
            let mut message = i18n::fill(t.tray_error, &[("err", &e)]);
            if let Err(err) = &restored {
                message.push_str("\n\n");
                message.push_str(&i18n::fill(
                    t.config_patch_warning,
                    &[("err", &err.to_string())],
                ));
            }
            rfd::MessageDialog::new()
                .set_title("EspansoManager")
                .set_description(message)
                .set_level(rfd::MessageLevel::Error)
                .show();
            std::process::exit(1);
        }
    };

    // Espanso has its own tray icon and its own Windows toasts; both are turned off, since this
    // app already shows an icon and reports every result in its own window. If anything actually
    // changed, the running daemon needs a nudge to pick it up (it won't re-read the config on its
    // own just because the file changed on disk).
    //
    // Deliberately after the tray exists, not before: on a folder that has never run this app, the
    // old order turned espanso's icon off and only then found out whether ours could be shown at
    // all. This is the half of that problem that ordering can fix; the other half — a folder where
    // the setting is already in the file from last time — is what the restore above is for.
    match config_patch::ensure_managed_settings(&config_path) {
        Ok(true) => {
            let _ = ctl.restart_and_confirm(std::time::Duration::from_secs(6), t);
        }
        Ok(false) => {}
        // Not fatal — the app runs fine, espanso just keeps an icon and a set of toasts we meant
        // to turn off. But it has to be said, because from the outside it is indistinguishable
        // from us having decided to leave them on.
        Err(e) => startup_warnings.push(i18n::fill(t.config_patch_warning, &[("err", &e.to_string())])),
    }

    // Collected up to here rather than earlier, so the config failure above can join the others in
    // the one banner instead of needing a second place to appear.
    let startup_warning = (!startup_warnings.is_empty()).then(|| startup_warnings.join("\n\n"));

    let icon = std::fs::read(runtime_dir.join("icon_no_backgroundv2.png"))
        .ok()
        .and_then(|bytes| eframe::icon_data::from_png_bytes(&bytes).ok());

    // Never larger than the screen it is opening on. The generous defaults suit a desktop monitor;
    // on a laptop panel they would put the buttons below the bottom edge, which on a window with no
    // taskbar button is genuinely hard to recover from.
    let work_area = display::work_area(display::system_scale());
    let fit = |desired: [f32; 2]| match work_area {
        Some(area) => area.fits(desired),
        None => desired,
    };

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("EspansoManager")
        .with_inner_size(fit(display::DEFAULT_SIZE))
        // Not passed through `fit`. This is the floor, and clamping a floor to the screen is how
        // a small primary monitor ends up licensing a window too small to use on a large one.
        .with_min_inner_size(display::MIN_SIZE)
        .with_visible(!start_hidden)
        .with_taskbar(false);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        // Without this, eframe's "persistence" feature (window position, which folders are
        // expanded, ...) defaults to a system-wide per-user config directory — breaking the
        // "everything lives in this one portable folder" promise. Keep it inside our own state
        // folder instead.
        persistence_path: Some(manager_dir.join("ui_state.ron")),
        ..Default::default()
    };

    let startup_ctx = StartupContext {
        match_file,
        match_file_path,
        backups_dir,
        settings,
        settings_store,
        ctl,
        exe_path,
        config_path,
        tray,
        start_hidden,
        startup_warning,
        load_error,
    };

    let _ = eframe::run_native(
        "EspansoManager",
        native_options,
        Box::new(move |cc| Ok(Box::new(EspansoManagerApp::new(cc, startup_ctx)))),
    );
}

/// Tells espanso its own setup wizard has already been done, before the daemon is ever started.
///
/// Espanso greets a folder with no record of a completed wizard with three windows of its own — a
/// welcome page, a start-with-Windows page, and a "here is your tray icon" page. Those questions are
/// asked by this app's own first-run screen instead, in this app's own design, so espanso's copy is
/// redundant and confusing: the same two decisions, twice, in two visual languages.
///
/// The flags are also shipped inside the package, but shipping them is not enough. A folder that
/// came from an older hand-off, or one where espanso ran before this app existed, has its own state
/// — and that is exactly the case that produced the wizard on a machine that had been working
/// fine for weeks. Writing them here covers every folder this app is ever pointed at.
///
/// Two details, both learned the hard way on a machine that showed espanso's welcome window at
/// every single boot:
///
/// The values are written **unconditionally**. The earlier version wrote only the flags that were
/// missing, on the reasoning that a stored `false` meant espanso had something genuine to say. It
/// does not: this app asks espanso's two questions itself, on its own first-run screen, so there is
/// nothing espanso can add. A folder carrying a `false` — and espanso writes one — was condemned to
/// the welcome window forever, because the one piece of code that could have corrected it had been
/// told to leave it alone.
///
/// And the folder is the one **espanso names**, not the one this app assumes. They are the same in
/// a healthy portable folder; when they are not, every flag written goes somewhere espanso will
/// never look, and nothing says so. The guess stays as the fallback for the case where espanso
/// cannot be asked at all.
fn silence_espanso_wizard(
    ctl: &EspansoCtl,
    t: &'static i18n::Strings,
    assumed_runtime_dir: &std::path::Path,
) {
    let runtime_dir = ctl
        .runtime_dir(t)
        .unwrap_or_else(|| assumed_runtime_dir.to_path_buf());
    let kvs = runtime_dir.join("kvs");
    if std::fs::create_dir_all(&kvs).is_err() {
        return;
    }
    for flag in [
        "has_completed_wizard",
        "has_displayed_welcome",
        "has_selected_auto_start_option",
    ] {
        // espanso's key-value store is one JSON document per key, so the whole file is the word
        // `true`.
        let _ = std::fs::write(kvs.join(flag), b"true");
    }
}
