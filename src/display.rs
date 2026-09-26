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

//! Fitting the window to whatever screen it happens to open on.
//!
//! This is a portable app: the same folder gets carried from a large desktop monitor to a laptop
//! and back. Two things have to survive that trip — the window must not open bigger than the screen
//! it lands on (including a size remembered from a *different* machine), and the interface must not
//! be scaled for a 27" panel when it is being read on a 13" one.
//!
//! Sizes here are always in egui *points*, never raw pixels: Windows' own display scaling already
//! turns points into pixels, so a 150 %-scaled laptop reports a smaller point-space than its pixel
//! count suggests — which is exactly the number that should decide how roomy the layout can be.

use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::{
    SystemParametersInfoW, SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

/// The usable part of the primary screen — the desktop minus the taskbar — in points.
#[derive(Clone, Copy, Debug)]
pub struct WorkArea {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl WorkArea {
    /// A window this size is guaranteed to fit, with a margin so it never looks wedged against the
    /// screen edges.
    pub fn fits(&self, desired: [f32; 2]) -> [f32; 2] {
        [
            desired[0].min(self.width * 0.94),
            desired[1].min(self.height * 0.94),
        ]
    }

    /// Turns a work-area rectangle in physical pixels into one in points, or `None` if the reading
    /// is nonsense — a screen under 200 points on either side is not a screen, and fixed defaults
    /// are a better answer than a window sized from a bad number.
    ///
    /// Both readings below arrive as a `RECT` and need exactly this, which is why it lives here
    /// rather than twice: the two of them disagreeing about what counts as usable is precisely the
    /// kind of drift that only shows up on somebody else's monitor.
    fn from_rect(rect: RECT, scale: f32) -> Option<Self> {
        let scale = if scale > 0.1 { scale } else { 1.0 };
        let width = (rect.right - rect.left) as f32 / scale;
        let height = (rect.bottom - rect.top) as f32 / scale;
        if width < 200.0 || height < 200.0 {
            return None;
        }
        Some(WorkArea {
            left: rect.left as f32 / scale,
            top: rect.top as f32 / scale,
            width,
            height,
        })
    }
}

/// Reads the primary screen's work area and converts it to points using `scale`.
///
/// `scale` is the display scaling factor (1.0 at 100 %, 1.5 at 150 %). Callers that already have a
/// window should pass its own `scale_factor()`; before there is a window, [`system_scale`] is the
/// best available answer.
pub fn work_area(scale: f32) -> Option<WorkArea> {
    let mut rect = RECT::default();
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut rect as *mut RECT as *mut std::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if ok.is_err() {
        return None;
    }
    WorkArea::from_rect(rect, scale)
}

/// The system display scaling factor, for use before a window exists.
pub fn system_scale() -> f32 {
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForSystem() };
    if dpi == 0 {
        1.0
    } else {
        dpi as f32 / 96.0
    }
}

/// How much to scale the type ramp, given the height of the screen in points.
///
/// The sizes themselves are Windows 11's own (see `apply_layout_style`), so on any ordinary screen
/// this is exactly 1.0 and the text matches what Settings and File Explorer use. Only a short
/// laptop panel pulls it in, and only slightly: a tenth smaller buys back a row or two without
/// making anything hard to read.
pub fn text_scale(work_height_points: f32) -> f32 {
    const SMALL: f32 = 700.0;
    const ROOMY: f32 = 900.0;
    let t = ((work_height_points - SMALL) / (ROOMY - SMALL)).clamp(0.0, 1.0);
    0.9 + 0.1 * t
}

/// The size the window opens at, and the smallest it is ever allowed to be.
///
/// Shared with `main.rs` so the builder and the on-show repair agree; a minimum enforced in one
/// place and ignored in the other is how a window ends up too small to use.
///
/// The minimum height is the editor's. Measured on the built window with the minimum lowered: at
/// 480 the library still shows its title, search and seven rows, but the editor's text box shrinks
/// to about two lines above Guardar; at 540 it has room for about five. (540 was first chosen for
/// a second row of folder chips that pushed a footer off the bottom. Both are gone; the number
/// stays for the editor.)
pub const DEFAULT_SIZE: [f32; 2] = [1180.0, 780.0];
pub const MIN_SIZE: [f32; 2] = [620.0, 540.0];

/// The work area of the monitor a given window is actually on.
///
/// [`work_area`] asks `SPI_GETWORKAREA`, which only ever answers for the *primary* monitor. On a
/// laptop docked to two external screens that is very often not the monitor the window is on, so
/// every decision made from it — how big the window may be, whether its position is on-screen — was
/// being taken against the wrong rectangle.
pub fn work_area_of_window(hwnd: windows::Win32::Foundation::HWND, scale: f32) -> Option<WorkArea> {
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };

    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let ok = unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        GetMonitorInfoW(monitor, &mut info)
    };
    if !ok.as_bool() {
        return None;
    }
    WorkArea::from_rect(info.rcWork, scale)
}

/// Puts a window in front of whatever the person is looking at, and gives it the keyboard.
///
/// `SetForegroundWindow` on its own is not enough and never was. Windows only grants the foreground
/// to a process that owns the last input event, and a click on a tray icon is input to the shell,
/// not to us — so the call is silently ignored and the window comes back *behind* everything, which
/// is exactly what happens on a busy desktop with a dozen windows open. Briefly attaching to the
/// input queue of whatever is currently in front makes Windows treat the request as coming from the
/// foreground application, which is the long-standing way to do this honestly.
/// Blocks until `hwnd` has been the foreground window continuously for `hold`, or until `timeout`.
///
/// Handing the keyboard back is a request, not an instruction that has already taken effect:
/// `SetForegroundWindow` returns long before Windows has actually moved the focus, and our own
/// search window is not even destroyed until the frame it was closed on has finished drawing. So
/// anything typed in that gap goes to the wrong window — which is exactly what happened, and why
/// the trigger this exists to protect never arrived where it was aimed.
///
/// Takes the handle as an `isize` so it can be carried to another thread; `HWND` is not `Send`.
pub fn wait_until_foreground(
    hwnd: isize,
    hold: std::time::Duration,
    timeout: std::time::Duration,
) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    const STEP: std::time::Duration = std::time::Duration::from_millis(10);
    let deadline = std::time::Instant::now() + timeout;
    let mut steady_since = None;

    loop {
        let now = std::time::Instant::now();
        if unsafe { GetForegroundWindow() }.0 as isize == hwnd {
            // Reaching the front once is not enough. Our own window is destroyed a frame or two
            // after the keyboard is handed back, and that destruction flickers the focus away and
            // straight back again. Typing into the flicker is what scattered the trigger: one
            // character landed in the document and the other seven went nowhere, so espanso.s
            // backspaces ate the document instead. The handover has to hold still first.
            let since = *steady_since.get_or_insert(now);
            if now.duration_since(since) >= hold {
                return true;
            }
        } else {
            steady_since = None;
        }
        if now >= deadline {
            return false;
        }
        std::thread::sleep(STEP);
    }
}

/// Types `text` into whatever window currently has the keyboard, character by character.
///
/// Sent as Unicode rather than as virtual key codes, so it does not care what keyboard layout is
/// active and a trigger containing an accent, a symbol or an emoji arrives exactly as written.
/// UTF-16 code units are what Windows wants here, which is also why surrogate pairs need no
/// special handling — they are simply two units in a row.
///
/// Injected input is inserted into the same system input queue as everything else, so whatever is
/// sent afterwards by another process is guaranteed to arrive after this, not interleaved with it.
/// That ordering is what makes [`crate::espanso_ctl::EspansoCtl::match_exec`] safe to call
/// immediately after.
///
/// Returns whether every keystroke was actually accepted. Windows refuses the injection outright —
/// `SendInput` inserts nothing and reports zero — when the focused window belongs to a
/// higher-integrity process: Task Manager, an elevated console, an admin tool. The caller has to
/// know, because the expansion request that follows makes espanso send one backspace per character
/// of the trigger, and if the trigger was never typed those backspaces eat the user's own text.
#[must_use]
pub fn type_text(text: &str) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, INPUT_KEYBOARD,
        VIRTUAL_KEY,
    };

    let mut events = Vec::new();
    for unit in text.encode_utf16() {
        for flags in [KEYEVENTF_UNICODE, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP] {
            events.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: unit,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
    }
    if events.is_empty() {
        return false;
    }
    let sent = unsafe { SendInput(&events, std::mem::size_of::<INPUT>() as i32) };
    sent as usize == events.len()
}

pub fn bring_to_front(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::SetActiveWindow;
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsIconic, SetForegroundWindow,
        ShowWindow, SW_RESTORE, SW_SHOW,
    };

    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }

        let foreground = GetForegroundWindow();
        let ours = GetCurrentThreadId();
        let theirs = GetWindowThreadProcessId(foreground, None);
        let borrowed = theirs != 0 && theirs != ours;
        if borrowed {
            let _ = AttachThreadInput(theirs, ours, true);
        }
        let _ = SetForegroundWindow(hwnd);
        let _ = BringWindowToTop(hwnd);
        let _ = SetActiveWindow(hwnd);
        if borrowed {
            let _ = AttachThreadInput(theirs, ours, false);
        }
    }
}

/// Hands the keyboard back to `hwnd` when Windows has left this thread holding no focus at all.
///
/// This is the repair for a bug that was reported as "the text box deselects itself and then I
/// cannot type anything". Measured on the running app, the broken state is exactly this: the window
/// is visible, foreground *and* active, and `GetFocus()` for its thread returns nothing. egui keeps
/// its own idea of which widget is focused right through it, so the box is still the one that would
/// receive the text — but egui only paints the caret while the viewport has the focus, so the box
/// stops looking selected, and there is no keyboard focus for the typing to arrive on anyway.
///
/// The part that makes it maddening rather than merely odd is that clicking does not undo it.
/// Windows assigns the focus when a window is *activated*, and ours is already the active window,
/// so no amount of clicking inside it ever produces another activation. Hiding the window to the
/// tray and opening it again does produce one, which is how the workaround was found before the
/// cause was.
///
/// So this asks Windows once per frame what its own view of the focus is, and puts it back when it
/// has gone missing. `GetGUIThreadInfo` is the only call that answers honestly here — `GetFocus`
/// alone cannot tell "nobody has it" apart from "a menu has it", and those two must not be treated
/// the same.
///
/// Returns whether it had to step in, which is nothing the app acts on: it is there so the fix can
/// be measured rather than assumed.
pub fn restore_keyboard_focus(hwnd: windows::Win32::Foundation::HWND) -> bool {
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetGUIThreadInfo, GUITHREADINFO, GUI_INMENUMODE, GUI_POPUPMENUMODE, GUI_SYSTEMMENUMODE,
    };

    if hwnd.is_invalid() {
        return false;
    }

    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    // Only ever about our own thread. Asking about somebody else's would be both rude and useless:
    // the focus is per-thread, and the one that is broken is ours.
    if unsafe { GetGUIThreadInfo(GetCurrentThreadId(), &mut info) }.is_err() {
        return false;
    }

    // Three states where Windows has parked the focus deliberately and taking it back would break
    // something the person is in the middle of: a menu is open (the window's own system menu counts,
    // and it is on this thread), the window is being dragged or resized, or the mouse is captured —
    // which it is for the whole time a button is held down, so this also keeps the repair out of
    // the middle of a drag in the list.
    if info.flags.contains(GUI_INMENUMODE)
        || info.flags.contains(GUI_SYSTEMMENUMODE)
        || info.flags.contains(GUI_POPUPMENUMODE)
        || !info.hwndMenuOwner.is_invalid()
        || !info.hwndMoveSize.is_invalid()
        || !info.hwndCapture.is_invalid()
    {
        return false;
    }

    // Nothing but the broken shape is touched. `hwndFocus` already pointing somewhere means the
    // keyboard has an owner and it is not our business which; `hwndActive` not being our window
    // means we are not the one being typed into, so putting the focus here would be taking it.
    if info.hwndActive != hwnd || !info.hwndFocus.is_invalid() {
        return false;
    }

    unsafe {
        // `SetFocus` returns whichever window *used* to hold the focus, and windows-rs turns a null
        // one into `Err` — which is precisely the case every single time this runs, since having
        // nothing focused is the reason we are here. So the result says nothing about whether it
        // worked; the only honest check is to ask again.
        let _ = SetFocus(Some(hwnd));
        GetFocus() == hwnd
    }
}
