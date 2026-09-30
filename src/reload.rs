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

//! Restarting espanso after a save, on a thread of its own, so the window keeps drawing.
//!
//! Every save ends in `restart_and_confirm`, and that is a wait by design: 400 ms for espanso to
//! go down, then `service status` until it is back — never less than about 430 ms, and up to six
//! seconds when it is slow. Taken on the interface thread, that was a frozen window after every
//! single save, at exactly the moment the list is scrolling the saved row into view and tinting
//! it. The file is on disk before any of this starts, so there is nothing for the window to wait
//! *for*: it says what was done at once, and only a restart that fails comes back to say so.
//!
//! Three rules keep this from being a new way to be wrong:
//!
//! - **One restart at a time.** A save made while one is running asks for one more, not a second
//!   in parallel; saves in quick succession collapse into it, and only the last answer is kept.
//! - **Nothing else talks to espanso meanwhile.** Pause and resume wait for it (`busy`), so a
//!   pause cannot be sent to a daemon that is on its way out, or to none at all.
//! - **Quit waits for it.** A stop sent while a restart is in flight can land before the new
//!   daemon comes up, which would leave espanso running with no icon to show for it. `close`
//!   holds Quit back until the restart is done — half a second, normally.
//!
//! What does **not** belong here is the wording: `AppState` composes the banners.

use crate::i18n::Strings;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

type Job = dyn Fn(&'static Strings) -> Result<(), String> + Send + Sync;
type Wake = dyn Fn() + Send + Sync;

#[derive(Clone)]
pub struct Reloader {
    shared: Arc<Shared>,
}

struct Shared {
    job: Box<Job>,
    /// Called once an answer is waiting, so a frame comes to collect it.
    wake: Box<Wake>,
    state: Mutex<State>,
    idle: Condvar,
}

#[derive(Default)]
struct State {
    busy: bool,
    /// Asked for again while one was running, with the strings of the latest ask.
    again: Option<&'static Strings>,
    /// Quit has begun: nothing new starts.
    closing: bool,
    /// How the last restart went, until the frame loop collects it.
    outcome: Option<Result<(), String>>,
}

impl Reloader {
    pub fn new(
        job: impl Fn(&'static Strings) -> Result<(), String> + Send + Sync + 'static,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                job: Box::new(job),
                wake: Box::new(wake),
                state: Mutex::default(),
                idle: Condvar::new(),
            }),
        }
    }

    /// Asks for a restart and returns at once.
    pub fn request(&self, t: &'static Strings) {
        let mut state = self.shared.lock();
        if state.closing {
            return;
        }
        if state.busy {
            state.again = Some(t);
            return;
        }
        state.busy = true;
        // An answer nobody collected yet is about a file that has just been written over.
        state.outcome = None;
        drop(state);
        let shared = self.shared.clone();
        std::thread::spawn(move || shared.run(t));
    }

    pub fn busy(&self) -> bool {
        self.shared.lock().busy
    }

    pub fn take_outcome(&self) -> Option<Result<(), String>> {
        self.shared.lock().outcome.take()
    }

    /// Lets no new restart start, and waits up to `limit` for the one in flight to finish.
    pub fn close(&self, limit: Duration) {
        let mut state = self.shared.lock();
        state.closing = true;
        let _ = self.shared.idle.wait_timeout_while(state, limit, |s| s.busy);
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn run(&self, mut t: &'static Strings) {
        loop {
            // A panic here must still clear `busy`: left set, pause would wait for ever.
            let outcome = catch_unwind(AssertUnwindSafe(|| (self.job)(t)))
                .unwrap_or_else(|_| Err(t.err_restart_unconfirmed.to_string()));
            let mut state = self.lock();
            match state.again.take() {
                Some(next) if !state.closing => t = next,
                _ => {
                    state.busy = false;
                    state.outcome = Some(outcome);
                    self.idle.notify_all();
                    break;
                }
            }
        }
        (self.wake)();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    fn strings() -> &'static Strings {
        crate::settings::Settings::default().t()
    }

    /// A stand-in for espanso that takes `ms` to restart and counts how often it was asked.
    fn slow(ms: u64, answer: Result<(), String>) -> (Reloader, Arc<AtomicUsize>) {
        let runs = Arc::new(AtomicUsize::new(0));
        let counted = runs.clone();
        let reloader = Reloader::new(
            move |_| {
                counted.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(ms));
                answer.clone()
            },
            || {},
        );
        (reloader, runs)
    }

    fn settle(reloader: &Reloader) -> Option<Result<(), String>> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while reloader.busy() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        reloader.take_outcome()
    }

    #[test]
    fn asking_returns_before_the_restart_is_done() {
        let (reloader, _) = slow(300, Ok(()));
        let started = Instant::now();
        reloader.request(strings());
        assert!(started.elapsed() < Duration::from_millis(50));
        assert!(reloader.busy());
        assert_eq!(settle(&reloader), Some(Ok(())));
        assert!(!reloader.busy());
    }

    #[test]
    fn saves_during_a_restart_collapse_into_one_more() {
        let (reloader, runs) = slow(150, Err("no".into()));
        for _ in 0..4 {
            reloader.request(strings());
        }
        assert_eq!(settle(&reloader), Some(Err("no".into())));
        assert_eq!(runs.load(Ordering::SeqCst), 2);
        assert_eq!(reloader.take_outcome(), None);
    }

    #[test]
    fn quit_waits_for_the_restart_and_starts_no_other() {
        let (reloader, runs) = slow(200, Ok(()));
        reloader.request(strings());
        reloader.request(strings());
        let started = Instant::now();
        reloader.close(Duration::from_secs(5));
        let waited = started.elapsed();
        assert!(!reloader.busy());
        assert!(waited >= Duration::from_millis(150) && waited < Duration::from_secs(1));
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        reloader.request(strings());
        assert!(!reloader.busy());
    }

    #[test]
    fn quit_does_not_wait_past_its_limit() {
        let (reloader, _) = slow(2_000, Ok(()));
        reloader.request(strings());
        let started = Instant::now();
        reloader.close(Duration::from_millis(100));
        assert!(started.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn a_panicking_restart_still_frees_pause() {
        let reloader = Reloader::new(|_| panic!("espanso exploded"), || {});
        reloader.request(strings());
        assert!(matches!(settle(&reloader), Some(Err(_))));
        assert!(!reloader.busy());
    }
}
