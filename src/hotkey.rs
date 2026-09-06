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

//! The global shortcut that opens the search window.
//!
//! Espanso has a search bar of its own on the same shortcut, and it is good — but it is espanso's
//! window, drawn by espanso's toolkit, and there is no seam to reach into. So this app takes the
//! shortcut over and draws its own, then hands the chosen expansion back to espanso to actually
//! insert (see [`crate::espanso_ctl::EspansoCtl::match_exec`]). We own the face; espanso keeps the
//! part that is genuinely hard.
//!
//! Espanso's own bar is switched off before this runs, unconditionally, so there is never more than
//! one search window on the machine. If none of the candidate shortcuts can be registered — some
//! other program owns them all — this returns `None` and the window is simply unreachable by
//! keyboard; the tips screen then has nothing to promise, which is honest.
//!
//! Same shape as the theme listener: a thread parked on the message queue, waking the frame loop
//! only when something has actually happened, and costing nothing in between.

use eframe::egui;
use std::sync::mpsc::{Receiver, TryRecvError};

/// Arbitrary; only has to be unique within this process.
const HOTKEY_ID: i32 = 0xE59A;

/// The shortcuts tried, in order, with the label each is called by.
///
/// `RegisterHotKey` is first come, first served, and Alt+Space is popular — PowerToys Run and most
/// launcher apps take it by default. Rather than losing the window to whoever booted first, the
/// second choice adds Shift, which is nearly always free. Whichever one is won gets *named* in the
/// tips screen, so the app never tells anybody to press something that does nothing.
pub const CANDIDATES: [Shortcut; 2] = [
    Shortcut { modifiers: 0x0001, label: "Alt+Space" },
    Shortcut { modifiers: 0x0001 | 0x0004, label: "Alt+Shift+Space" },
];

#[derive(Clone, Copy)]
pub struct Shortcut {
    /// `MOD_ALT`, optionally with `MOD_SHIFT`. Written out rather than imported so the table stays
    /// readable at a glance.
    modifiers: u32,
    pub label: &'static str,
}

pub struct HotkeyWatch {
    presses: Receiver<()>,
    /// Which of the candidates was actually won.
    pub shortcut: Shortcut,
}

impl HotkeyWatch {
    /// True if the shortcut has been pressed since the last time this was asked.
    pub fn take_pressed(&self) -> bool {
        let mut pressed = false;
        loop {
            match self.presses.try_recv() {
                Ok(()) => pressed = true,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return pressed,
            }
        }
    }
}

/// Stops the window underneath from reading our shortcut as a bare Alt tap.
///
/// Alt+Space is a chord, but the window that has the keyboard only ever sees half of it:
/// `RegisterHotKey` swallows the Space, so from that window's point of view Alt went down and, a
/// moment later, Alt came up with nothing at all in between. That is the definition of "Alt was
/// tapped", and a Windows app answers it by activating its menu bar. From then on everything
/// espanso types into that window is read as a menu shortcut instead of as text — in Notepad the
/// letter "e" is the settings gear, which is exactly why picking almost any expansion opened
/// Notepad's settings and typed nothing at all. Chromium-based apps have no menu bar to activate,
/// which is why the very same expansion landed perfectly in a browser and vanished in Notepad.
///
/// One keystroke is enough to fix it, sent while Alt is still held: the chord stops being "Alt on
/// its own", so the release means nothing and the menu bar stays shut. `VK_NONAME` is Microsoft's
/// own reserved no-op code — no keyboard can produce it, so nothing has a reason to be listening
/// for it.
fn break_alt_chord() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VK_NONAME,
    };

    let event = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VK_NONAME,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    let events = [event(KEYBD_EVENT_FLAGS(0)), event(KEYEVENTF_KEYUP)];
    unsafe { SendInput(&events, std::mem::size_of::<INPUT>() as i32) };
}

/// Claims the first of [`CANDIDATES`] that is free, or gives up cleanly if all of them are taken.
pub fn start(ctx: &egui::Context) -> Option<HotkeyWatch> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, HOT_KEY_MODIFIERS, MOD_NOREPEAT, VK_SPACE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

    let (tx, presses) = std::sync::mpsc::channel();
    let (ready_tx, ready) = std::sync::mpsc::channel();
    let ctx = ctx.clone();

    // Registered on the thread that pumps for it: Windows delivers WM_HOTKEY to the queue of the
    // thread that made the call, so doing it anywhere else would post the message somewhere nobody
    // is listening.
    std::thread::spawn(move || {
        let won = CANDIDATES.into_iter().find(|candidate| unsafe {
            RegisterHotKey(
                None,
                HOTKEY_ID,
                HOT_KEY_MODIFIERS(candidate.modifiers) | MOD_NOREPEAT,
                VK_SPACE.0 as u32,
            )
            .is_ok()
        });
        let _ = ready_tx.send(won);
        if won.is_none() {
            return;
        }

        let mut msg = MSG::default();
        // `GetMessageW` blocks until something arrives, so this thread costs nothing at rest.
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
            if msg.message == WM_HOTKEY && msg.wParam.0 as i32 == HOTKEY_ID {
                // First, and on this thread, because it has to reach the window underneath while
                // that window still has the keyboard — a moment later it will be ours.
                break_alt_chord();
                if tx.send(()).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        }
    });

    match ready.recv() {
        Ok(Some(shortcut)) => Some(HotkeyWatch { presses, shortcut }),
        _ => None,
    }
}
