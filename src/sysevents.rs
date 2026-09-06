//! Hearing about appearance changes from Windows instead of asking it repeatedly.
//!
//! Windows announces a light/dark switch and an accent-colour change by broadcasting messages to
//! every **top-level** window. Receiving them needs a window and a message loop, and eframe owns
//! both for the real window — so this creates its own: a top-level window that is zero-sized, never
//! shown, and marked as a tool window so it stays out of the taskbar and out of Alt-Tab. It sits on
//! its own thread blocked inside `GetMessageW`, which costs nothing at all until a message arrives.
//!
//! The obvious choice — a *message-only* window, parented to `HWND_MESSAGE` — is the wrong one
//! here, and silently so: those windows are excluded from broadcasts by design, so the listener
//! would run forever and simply never hear anything.
//!
//! When one does, the handler sets a flag and wakes the interface. The interface then re-reads the
//! registry **once**, in response to something that really happened, instead of every two seconds
//! on the chance that it might have.
//!
//! If any of this fails — a class that will not register, a window that will not create — the
//! caller gets `None` and falls back to the timer it used before. An appearance that updates a
//! couple of seconds late is a far better outcome than one that never updates at all.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
    TranslateMessage, MSG, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};

/// Sent when a system-wide setting changes. The one we care about identifies itself by name in
/// `lParam`; every other settings change is ignored so a mouse-speed tweak doesn't cost a registry
/// read.
const WM_SETTINGCHANGE: u32 = 0x001A;
/// Sent when the visual style changes.
const WM_THEMECHANGED: u32 = 0x031A;
/// Sent by the desktop window manager when the user picks a different accent colour.
const WM_DWMCOLORIZATIONCOLORCHANGED: u32 = 0x0320;

/// The name Windows uses for "the light/dark and accent colour set changed".
const IMMERSIVE_COLOR_SET: &str = "ImmersiveColorSet";

struct Shared {
    changed: Arc<AtomicBool>,
    ctx: egui::Context,
}

/// The window procedure is a bare function pointer with nowhere to hang state, so what it needs
/// lives here. Written once, before the window that could call it exists.
static SHARED: OnceLock<Shared> = OnceLock::new();

/// A live subscription to Windows' appearance notifications.
pub struct ThemeWatch {
    changed: Arc<AtomicBool>,
}

impl ThemeWatch {
    /// Starts listening. Returns `None` if the message-only window could not be created, in which
    /// case the caller should keep polling.
    ///
    /// `ctx` is woken whenever something arrives, so a change is picked up immediately even while
    /// the window is hidden in the tray and nothing else is asking for frames.
    pub fn start(ctx: egui::Context) -> Option<Self> {
        let changed = Arc::new(AtomicBool::new(false));

        // If this is somehow called twice, the second caller shares the first one's flag rather
        // than quietly listening to nothing.
        if SHARED
            .set(Shared {
                changed: Arc::clone(&changed),
                ctx,
            })
            .is_err()
        {
            return SHARED.get().map(|s| Self {
                changed: Arc::clone(&s.changed),
            });
        }

        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<bool>();

        // The window must be created on the thread that pumps its messages, so both happen here.
        // The thread is deliberately never joined: it lives as long as the process, and blocks in
        // `GetMessageW` the entire time.
        std::thread::Builder::new()
            .name("theme-watch".to_string())
            .spawn(move || {
                let created = unsafe { create_listener_window() };
                let _ = ready_tx.send(created.is_some());
                if created.is_none() {
                    return;
                }
                unsafe { pump_messages() };
            })
            .ok()?;

        // Wait for the window to exist before promising the caller it can stop polling.
        match ready_rx.recv_timeout(std::time::Duration::from_secs(2)) {
            Ok(true) => Some(Self { changed }),
            _ => None,
        }
    }

    /// Whether an appearance change arrived since the last time this was asked. Clears the flag.
    ///
    /// This is a single atomic swap — the whole point of the exercise is that the common answer,
    /// "nothing happened", costs nothing to obtain.
    pub fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::Relaxed)
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn create_listener_window() -> Option<HWND> {
    let hinstance = GetModuleHandleW(None).ok()?;
    let class_name = wide("EspansoManagerAppearanceWatch");

    let class = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: hinstance.into(),
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..Default::default()
    };
    // A zero atom means the class could not be registered.
    if RegisterClassW(&class) == 0 {
        return None;
    }

    CreateWindowExW(
        // A tool window never appears in the taskbar or in Alt-Tab, which is what keeps this
        // invisible to the user despite being a real top-level window.
        WS_EX_TOOLWINDOW,
        PCWSTR(class_name.as_ptr()),
        PCWSTR::null(),
        // Created without WS_VISIBLE and never shown, so it is never drawn anywhere.
        WS_POPUP,
        0,
        0,
        0,
        0,
        None,
        None,
        Some(hinstance.into()),
        None,
    )
    .ok()
}

unsafe fn pump_messages() {
    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

/// Reads the NUL-terminated wide string `lParam` points at, and says whether it names the colour
/// set. A null or unreadable pointer simply means "not the message we want".
unsafe fn is_immersive_color_set(lparam: LPARAM) -> bool {
    let ptr = lparam.0 as *const u16;
    if ptr.is_null() {
        return false;
    }
    // Bounded so a malformed broadcast can't walk off into unmapped memory.
    let mut len = 0usize;
    while len < 64 && *ptr.add(len) != 0 {
        len += 1;
    }
    let slice = std::slice::from_raw_parts(ptr, len);
    String::from_utf16_lossy(slice) == IMMERSIVE_COLOR_SET
}

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let appearance_changed = match message {
        WM_DWMCOLORIZATIONCOLORCHANGED | WM_THEMECHANGED => true,
        WM_SETTINGCHANGE => is_immersive_color_set(lparam),
        _ => false,
    };

    if appearance_changed {
        if let Some(shared) = SHARED.get() {
            shared.changed.store(true, Ordering::Relaxed);
            // Wakes the interface even when it is hidden in the tray, so the palette is already
            // correct the moment the window is shown again.
            shared.ctx.request_repaint();
        }
    }

    DefWindowProcW(hwnd, message, wparam, lparam)
}
