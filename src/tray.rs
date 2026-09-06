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

//! The tray icon, its menu, and the two threads that keep them alive while the window is not.
//!
//! The app spends most of the day hidden beside the clock, and while it is hidden eframe runs no
//! egui pass at all. Nothing here may therefore depend on a frame happening. Icon and menu events
//! arrive on threads that *block* waiting for them (`spawn_event_pumps`) rather than on a poll,
//! which is what took the hidden cost from 375 ms/min to about 50 ms across ten minutes;
//! `VISIBLE_POLL_INTERVAL` is a slow heartbeat kept only while there is a window to repaint.
//!
//! Quitting is finished here too, in `finish_quit`, and not in the eframe loop: stopping espanso
//! and ending the process have to happen whether or not another frame ever runs.
//!
//! The timed pause is the only state here with a deadline, and `tick` is what ends it. If the
//! re-enable fails it drops to `PausedIndefinite` rather than leaving a deadline sitting in the
//! past — retrying a failing resume once per frame, with nothing on screen to slow it down, is how
//! a hidden app becomes an invisible loop spawning `espansod` as fast as the machine allows.

use crate::espanso_ctl::{CtlResult, EspansoCtl};
use crate::i18n::{fill, Strings};
use std::path::Path;
use std::time::{Duration, Instant};
use eframe::egui;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub const PAUSE_DURATION: Duration = Duration::from_secs(10 * 60);
/// A slow heartbeat kept only while the window is *visible*, so a tray action taken with the window
/// open is still noticed promptly even if a wake-up were ever missed. Real interaction repaints
/// immediately through egui's own input handling; this never governs anything a person is waiting
/// for, so it can afford to be lazy.
pub const VISIBLE_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Tray and menu events, after threads have taken them off the library's own global channels.
pub struct TrayEvents {
    pub tray: std::sync::mpsc::Receiver<TrayIconEvent>,
    pub menu: std::sync::mpsc::Receiver<MenuEvent>,
}

/// Moves the two event streams onto threads that wake the app when something actually happens,
/// instead of the app asking five times a second whether anything has.
///
/// `tray-icon` hands its events to global channels that somebody has to drain. Draining them from
/// the frame loop means the frame loop has to keep running while the window is hidden in the tray —
/// which is the state this app spends nearly all of its life in, and the one where it should be
/// doing nothing at all.
///
/// It was not free. A forced frame every 200 ms cost **6.25 ms of CPU per second** with the window
/// hidden: twelve times what the app used sitting open and idle, all of it spent finding out that
/// nothing had happened. A thread parked on `recv()` costs nothing until an event arrives, and then
/// the click is answered at once rather than up to 200 ms later — cheaper *and* more responsive,
/// which is the only kind of optimisation worth making to something someone is looking at.
///
/// The same shape as the theme listener in `sysevents`: block where the event is, wake the frame
/// loop, let it do the work.
/// How long Quit waits for the window to bow out politely before the program is ended outright.
///
/// Long enough for eframe to close its window and write the saved size and position, short enough
/// that nobody watching decides the menu item did nothing.
const QUIT_GRACE: Duration = Duration::from_millis(800);

/// Everything the menu pump needs to end the program without the window's help.
pub struct QuitPlan {
    pub id: MenuId,
    pub ctl: EspansoCtl,
}

/// Ends the program. Runs on the pump thread, so it does not depend on a frame ever being drawn.
///
/// Quit used to be handled only inside the frame loop: the menu event was queued, a repaint was
/// requested, and the next frame stopped espanso and asked the window to close. That is fine while
/// frames are being drawn — and every part of it is silently skipped when they are not. Hidden in
/// the tray, with no window on screen to repaint, one machine simply never ran the frame: Quit did
/// nothing at all, the icon stayed where it was, and the only way out was Task Manager.
///
/// So the frame loop still gets its chance — it is what saves the window geometry, and it is asked
/// first — but it is no longer the only thing standing between the user and the end of the
/// program. Espanso is stopped here, where nothing can skip it, and the process ends here too.
fn finish_quit(quit: &QuitPlan) {
    let started = Instant::now();

    // Espanso goes down with the manager. Its own tray icon is switched off on our behalf, so a
    // daemon left running would keep expanding text with nothing on screen to pause it or explain
    // itself.
    //
    // Sent and not waited for: see `service_stop_detached` for why waiting was worse than useless
    // here. The child outlives this process either way, so nothing is lost by leaving before it
    // reports back — and the grace below is longer than the stop takes on a machine that is well.
    let _ = quit.ctl.service_stop_detached();

    let spent = started.elapsed();
    if spent < QUIT_GRACE {
        std::thread::sleep(QUIT_GRACE - spent);
    }
    std::process::exit(0);
}

pub fn spawn_event_pumps(ctx: &egui::Context, quit: QuitPlan) -> TrayEvents {
    let (tray_tx, tray) = std::sync::mpsc::channel();
    let tray_ctx = ctx.clone();
    std::thread::spawn(move || {
        // `recv` ends only when the library's channel is dropped, which happens at shutdown; the
        // send failing means the app is gone, and there is nothing left to wake.
        while let Ok(event) = TrayIconEvent::receiver().recv() {
            if tray_tx.send(event).is_err() {
                break;
            }
            tray_ctx.request_repaint();
        }
    });

    let (menu_tx, menu) = std::sync::mpsc::channel();
    let menu_ctx = ctx.clone();
    std::thread::spawn(move || {
        while let Ok(event) = MenuEvent::receiver().recv() {
            let is_quit = event.id == quit.id;
            // Forwarded first either way, so the window can close itself properly if it is in a
            // position to. `finish_quit` is what guarantees the program ends if it is not.
            if menu_tx.send(event).is_err() {
                break;
            }
            menu_ctx.request_repaint();
            if is_quit {
                finish_quit(&quit);
            }
        }
    });

    TrayEvents { tray, menu }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PauseState {
    Active,
    PausedIndefinite,
    PausedTimed(Instant),
}

pub struct Tray {
    pub icon: TrayIcon,
    icon_active: Icon,
    icon_paused: Icon,
    toggle_item: MenuItem,
    pause10_item: MenuItem,
    quit_item: MenuItem,
    toggle_id: MenuId,
    pause10_id: MenuId,
    quit_id: MenuId,
    pub state: PauseState,
}

fn ctl_result_to_message(
    res: std::io::Result<CtlResult>,
    verb: &str,
    t: &'static Strings,
) -> Result<(), String> {
    match res {
        Ok(r) if r.success => Ok(()),
        Ok(r) => Err(fill(
            t.err_espanso_action,
            &[("verb", verb), ("detail", &r.detail())],
        )),
        Err(e) => Err(fill(
            t.err_espanso_comm,
            &[("verb", verb), ("err", &e.to_string())],
        )),
    }
}

impl Tray {
    /// `runtime_dir` is Espanso's own `.espanso-runtime` folder, which already contains
    /// `normalv2.ico` / `disabledv2.ico` after the daemon's first run — reusing Espanso's own
    /// icons rather than shipping our own.
    pub fn new(runtime_dir: &Path, t: &'static Strings) -> Result<Self, String> {
        let icon_active =
            Icon::from_path(runtime_dir.join("normalv2.ico"), None).map_err(|e| e.to_string())?;
        let icon_paused =
            Icon::from_path(runtime_dir.join("disabledv2.ico"), None).map_err(|e| e.to_string())?;

        let toggle_item = MenuItem::new(t.tray_pause, true, None);
        let pause10_item = MenuItem::new(t.tray_pause_10, true, None);
        let quit_item = MenuItem::new(t.tray_quit, true, None);
        let toggle_id = toggle_item.id().clone();
        let pause10_id = pause10_item.id().clone();
        let quit_id = quit_item.id().clone();

        let menu = Menu::new();
        menu.append(&pause10_item).map_err(|e| e.to_string())?;
        menu.append(&toggle_item).map_err(|e| e.to_string())?;
        // A rule before Quit. The two above it are reversible in a click; this one ends the
        // session, and putting it flush against them invites the wrong one.
        menu.append(&PredefinedMenuItem::separator())
            .map_err(|e| e.to_string())?;
        menu.append(&quit_item).map_err(|e| e.to_string())?;

        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon(icon_active.clone())
            .with_tooltip(t.tray_tooltip)
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| e.to_string())?;

        Ok(Self {
            icon,
            icon_active,
            icon_paused,
            toggle_item,
            pause10_item,
            quit_item,
            toggle_id,
            pause10_id,
            quit_id,
            state: PauseState::Active,
        })
    }

    /// Re-labels the menu and tooltip after the user picks a different interface language, so the
    /// tray doesn't keep speaking the previous one until the app is restarted.
    pub fn relabel(&self, t: &'static Strings) {
        self.pause10_item.set_text(t.tray_pause_10);
        self.quit_item.set_text(t.tray_quit);
        let _ = self.icon.set_tooltip(Some(t.tray_tooltip));
        self.sync_visuals(t);
    }

    fn sync_visuals(&self, t: &'static Strings) {
        let (label, icon) = match self.state {
            PauseState::Active => (t.tray_pause, &self.icon_active),
            PauseState::PausedIndefinite | PauseState::PausedTimed(_) => {
                (t.tray_resume, &self.icon_paused)
            }
        };
        self.toggle_item.set_text(label);
        let _ = self.icon.set_icon(Some(icon.clone()));
    }

    pub fn pause_timed(&mut self, ctl: &EspansoCtl, t: &'static Strings) -> Result<(), String> {
        ctl_result_to_message(ctl.cmd_disable(t), t.verb_pause_timed, t)?;
        self.state = PauseState::PausedTimed(Instant::now() + PAUSE_DURATION);
        self.sync_visuals(t);
        Ok(())
    }

    pub fn toggle(&mut self, ctl: &EspansoCtl, t: &'static Strings) -> Result<(), String> {
        match self.state {
            PauseState::Active => {
                ctl_result_to_message(ctl.cmd_disable(t), t.verb_pause, t)?;
                self.state = PauseState::PausedIndefinite;
            }
            PauseState::PausedIndefinite | PauseState::PausedTimed(_) => {
                ctl_result_to_message(ctl.cmd_enable(t), t.verb_resume, t)?;
                self.state = PauseState::Active;
            }
        }
        self.sync_visuals(t);
        Ok(())
    }

    /// When a running timed pause is due to end, if one is running at all.
    ///
    /// The only thing left in this app that needs a clock rather than an event, and therefore the
    /// only reason to schedule a frame while the window is hidden.
    pub fn timed_pause_deadline(&self) -> Option<Instant> {
        match self.state {
            PauseState::PausedTimed(until) => Some(until),
            _ => None,
        }
    }

    /// Call every frame. Auto re-enables once a timed pause elapses, resyncing the toggle label.
    pub fn tick(&mut self, ctl: &EspansoCtl, t: &'static Strings) -> Option<Result<(), String>> {
        if let PauseState::PausedTimed(until) = self.state {
            if Instant::now() >= until {
                let result = ctl_result_to_message(ctl.cmd_enable(t), t.verb_auto_resume, t);
                // The pause is over either way, and that is the point: leaving it armed with its
                // deadline already in the past makes a failed re-enable retry on the next frame,
                // and the next, forever. With the window hidden there is nothing to slow that
                // down — `timed_pause_deadline` books the following frame with no delay at all —
                // so a daemon that will not come back turns the app into a loop spawning espansod
                // as fast as the machine can manage, invisibly.
                //
                // `PausedIndefinite` is also the honest answer: espanso really was not re-enabled.
                // Nothing on screen changes, because a timed pause and an open-ended one already
                // draw the same icon and the same "Reanudar" label; the only difference is that
                // this one stops asking.
                self.state = if result.is_ok() {
                    PauseState::Active
                } else {
                    PauseState::PausedIndefinite
                };
                self.sync_visuals(t);
                return Some(result);
            }
        }
        None
    }

    /// Returns Some(result) if `id` matched one of our two menu items.
    /// Whether `id` is the Quit item. Asked separately from [`handle_menu_id`] because quitting is
    /// not a state change on the tray — it is the end of the program, and the app has to own that.
    pub fn is_quit(&self, id: &MenuId) -> bool {
        *id == self.quit_id
    }

    /// The Quit item's id, for [`spawn_event_pumps`] — which recognises it without the app's help
    /// so that Quit works even when no frame is running.
    pub fn quit_id(&self) -> MenuId {
        self.quit_id.clone()
    }

    pub fn handle_menu_id(
        &mut self,
        id: &MenuId,
        ctl: &EspansoCtl,
        t: &'static Strings,
    ) -> Option<Result<(), String>> {
        if *id == self.pause10_id {
            Some(self.pause_timed(ctl, t))
        } else if *id == self.toggle_id {
            Some(self.toggle(ctl, t))
        } else {
            None
        }
    }
}
