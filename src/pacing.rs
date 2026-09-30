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

//! Waiting for the screen between frames without spending the processor on the wait.
//!
//! eframe's default is OpenGL's own vsync: `SwapBuffers` holds each frame back until the monitor
//! is ready for it. How a driver waits there is its own business, and NVIDIA's spins. Measured on
//! a 200 Hz monitor with the pointer sweeping the list, eframe spent 1.1 ms of real work per frame
//! (our views about 0.2 of it) and the process used 70–100 % of a core; scrolling, 111–114 %. The
//! difference was the driver, polling a clock inside the swap.
//!
//! So the swap no longer waits (`vsync: false` in main.rs) and the frame waits here instead, at its
//! start, in `DwmFlush` — which blocks in the kernel until the desktop compositor's next pass, the
//! same beat a windowed program's frame is shown on anyway. Same pointer sweep, same 200 frames a
//! second: 32–43 % of a core, and 33–42 % scrolling. Memory and start-up time did not move.
//!
//! What does **not** belong here is deciding *whether* to draw — that stays with egui's repaint
//! requests. This only sets how fast frames that were asked for may follow one another.
//!
//! The one way this could go wrong is a `DwmFlush` that does not block: composition unavailable,
//! as in some remote sessions. Unthrottled, a no-vsync loop draws thousands of frames a second
//! and pins a core. So a flush that failed, or came back at once, is followed by a sleep that
//! keeps frames at least one refresh apart — never faster than vsync would have allowed.

use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{DwmFlush, DwmGetCompositionTimingInfo, DWM_TIMING_INFO};

/// What counts as "came back at once". A real flush waits for the next pass, which at 240 Hz is
/// still up to 4 ms away; a frame that happens to finish just before a pass can see a short wait,
/// and the floor below then costs it nothing, because a refresh has already gone by.
const PROMPT_RETURN: Duration = Duration::from_millis(1);

/// Assumed when Windows will not say: 60 Hz, the slowest refresh anyone is likely to be on, so the
/// guess can only ever err towards drawing *less*.
const FALLBACK_PERIOD: Duration = Duration::from_micros(16_667);

#[derive(Default)]
pub struct Pacer {
    last_frame: Option<Instant>,
}

impl Pacer {
    /// Call once at the start of every frame the window is visible for.
    pub fn wait_for_compositor(&mut self) {
        let start = Instant::now();
        let flushed = unsafe { DwmFlush() }.is_ok();
        if !flushed || start.elapsed() < PROMPT_RETURN {
            if let Some(last) = self.last_frame {
                // A tenth short of a full period, so a frame on schedule is never pushed back into
                // the next one by the imprecision of the sleep itself.
                let floor = last + refresh_period().mul_f32(0.9);
                let now = Instant::now();
                if floor > now {
                    std::thread::sleep(floor - now);
                }
            }
        }
        self.last_frame = Some(Instant::now());
    }
}

/// The compositor's refresh period. Asked only on the fallback path, so it costs nothing while
/// `DwmFlush` is doing its job.
fn refresh_period() -> Duration {
    let mut info = DWM_TIMING_INFO {
        cbSize: std::mem::size_of::<DWM_TIMING_INFO>() as u32,
        ..Default::default()
    };
    if unsafe { DwmGetCompositionTimingInfo(HWND::default(), &mut info) }.is_err() {
        return FALLBACK_PERIOD;
    }
    let rate = info.rateRefresh;
    if rate.uiNumerator == 0 || rate.uiDenominator == 0 {
        return FALLBACK_PERIOD;
    }
    // Hz = numerator / denominator, so the period is denominator / numerator seconds.
    Duration::from_secs_f64(rate.uiDenominator as f64 / rate.uiNumerator as f64)
}
