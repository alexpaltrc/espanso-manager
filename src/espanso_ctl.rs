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

//! Everything this app says to espanso, and the only place it is allowed to say it.
//!
//! Two channels, and they are not interchangeable. Ordinary commands run `espansod.exe` as a child
//! process with `CREATE_NO_WINDOW` — without that flag every status poll flashes a console window
//! on the desktop. Expansions instead go down espanso's own named pipe,
//! `\.\pipe\espansoworkerv2`, which saves the ~50 ms of starting a process.
//!
//! **Every function here blocks the thread that calls it, and that thread is the one drawing the
//! interface.** So nothing runs without a deadline: `CTL_TIMEOUT` caps a single call, and the
//! waiting loops carve each poll out of the caller's remaining budget rather than starting a fresh
//! four seconds every time round. A six-second wait that actually spent 8.4 is the bug that shape
//! produces, and it was measured, not imagined.
//!
//! For the same reason, anything asked here at start-up is paid for before the first frame exists.
//! `version` is not called on the way up for that reason — it feeds one line in Ajustes, so it is
//! asked when that screen is opened.
//!
//! `match_exec` is the sharp one. It means "expand as though this trigger had just been typed", so
//! espanso sends one backspace per character of the trigger before writing the replacement. That
//! single fact is why our own launcher could never work; see `LAUNCHER-BACKUP.txt`.

use crate::i18n::{fill, Strings};
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Upper bound on how long we'll ever wait on an espansod subprocess before giving up on it and
/// killing it. Nothing in this app may block the GUI thread longer than this, no matter what
/// espansod (or whatever is intercepting it — an antivirus, EDR agent, etc.) does. Without this,
/// a single stuck subprocess call froze the entire window, tray icon included, forcing a trip to
/// Task Manager.
const CTL_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Debug, Clone)]
pub struct CtlResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

impl CtlResult {
    /// The technical detail worth showing (collapsed) alongside a friendly error banner.
    pub fn detail(&self) -> String {
        if !self.stderr.trim().is_empty() {
            self.stderr.trim().to_string()
        } else {
            self.stdout.trim().to_string()
        }
    }
}

/// The pipe espanso's worker listens on, named exactly as espanso names it itself.
const WORKER_PIPE: &str = r"\\.\pipe\espansoworkerv2";

#[derive(Clone)]
pub struct EspansoCtl {
    exe_path: PathBuf,
}

impl EspansoCtl {
    /// `exe_path` should point at `espansod.exe`, which is expected to sit next to our own exe.
    pub fn new(exe_path: PathBuf) -> Self {
        Self { exe_path }
    }

    pub fn exe_exists(&self) -> bool {
        self.exe_path.is_file()
    }

    fn working_dir(&self) -> &Path {
        self.exe_path.parent().unwrap_or_else(|| Path::new("."))
    }

    /// For short-lived, "answer and exit immediately" subcommands (`service status`, `cmd
    /// enable`/`disable`) — normally back in well under a second. `service start`/`restart` don't
    /// go through this at all: espansod's own process for those does not reliably exit on its own
    /// once launched without an inherited console (see `spawn_detached`).
    ///
    /// Bounded by [`CTL_TIMEOUT`] no matter what: espansod itself, or something intercepting it
    /// (antivirus/EDR agents are a known culprit), can occasionally stall a child process
    /// indefinitely, and this call happens on the GUI thread — an unbounded wait here previously
    /// froze the whole window, including the tray icon, until killed from Task Manager.
    fn run(&self, args: &[&str], t: &'static Strings) -> std::io::Result<CtlResult> {
        self.run_until(args, Instant::now() + CTL_TIMEOUT, t)
    }

    /// [`run`], but bounded by a deadline the caller already owns.
    ///
    /// A poll inside a timed loop must not be allowed to outlive the loop's own budget. It used to:
    /// `wait_until_running(6s)` checked the clock only *after* a status call, and a status call can
    /// take the full `CTL_TIMEOUT`, so six seconds meant "run 4 s, notice 4 < 6, sleep, run 4 s
    /// again" — about 8.3 s. Three stages of that is where `ensure_running`'s stated 18 seconds
    /// became roughly 25, all of it before the window or the tray icon exist.
    fn run_until(
        &self,
        args: &[&str],
        deadline: Instant,
        t: &'static Strings,
    ) -> std::io::Result<CtlResult> {
        let mut child = Command::new(&self.exe_path)
            .args(args)
            // The portable daemon locates its `.espanso`/`.espanso-runtime` folders relative to
            // its own working directory, so always run it from its own folder.
            .current_dir(self.working_dir())
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // The pipes are drained on a thread of their own, and this is not tidiness — it is the
        // difference between a slow command and a frozen program.
        //
        // Waiting for the child was already bounded. Reading its output was not, and that is the
        // half that could hang: `read_to_string` returns at end of file, and a Windows pipe only
        // reaches end of file once *every* handle to its writing end is shut. `Command::spawn`
        // hands those handles to the child, the child hands them to anything it starts, and a
        // grandchild that outlives its parent keeps the pipe open with nobody left to write to it.
        // Read on this thread, that is a wait with no end, taken with the graphical thread — and
        // the tray icon's window belongs to that same thread, which is why the icon would stay on
        // screen and stop answering. Quit, in particular, ran this before it had asked for
        // anything to close, so the request was never even made.
        //
        // On this side of the channel every path now ends, and a reader left holding a pipe that
        // nobody will ever close is one parked thread rather than an application that will not shut
        // down.
        let mut stdout_pipe = child.stdout.take();
        let mut stderr_pipe = child.stderr.take();
        let (output_tx, output_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut stdout = String::new();
            let mut stderr = String::new();
            if let Some(pipe) = stdout_pipe.as_mut() {
                let _ = pipe.read_to_string(&mut stdout);
            }
            if let Some(pipe) = stderr_pipe.as_mut() {
                let _ = pipe.read_to_string(&mut stderr);
            }
            let _ = output_tx.send((stdout, stderr));
        });

        // The number in the message is the budget this call actually had, not the constant — a poll
        // cut short by an outer deadline should not claim it waited four seconds.
        let budget = deadline.saturating_duration_since(Instant::now());
        let timed_out = |t: &'static Strings| CtlResult {
            success: false,
            stdout: String::new(),
            stderr: fill(
                t.err_espansod_timeout,
                &[("n", &budget.as_secs().max(1).to_string())],
            ),
        };

        loop {
            if let Some(status) = child.try_wait()? {
                // The command is done; the last of its output may still be in flight. Whatever
                // budget is left is what it gets, and an empty result is better than never
                // returning.
                let remaining = deadline.saturating_duration_since(Instant::now());
                let (stdout, stderr) = output_rx.recv_timeout(remaining).unwrap_or_default();
                return Ok(CtlResult {
                    success: status.success(),
                    stdout,
                    stderr,
                });
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(timed_out(t));
            }
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    /// Launches an espansod subcommand detached, without waiting for it to exit.
    fn spawn_detached(&self, args: &[&str]) -> std::io::Result<()> {
        Command::new(&self.exe_path)
            .args(args)
            .current_dir(self.working_dir())
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(())
    }

    pub fn is_running(&self, t: &'static Strings) -> bool {
        matches!(self.service_status(t), Ok(r) if r.success)
    }

    fn wait_until_running(&self, timeout: Duration, t: &'static Strings) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            // Whichever comes first: a single call's own ceiling, or what is left of the budget
            // this function was given. The loop is only ever re-entered while `now < deadline`, so
            // this is always a moment in the future and no poll is started that cannot finish.
            let poll_deadline = (Instant::now() + CTL_TIMEOUT).min(deadline);
            if matches!(self.run_until(&["service", "status"], poll_deadline, t), Ok(r) if r.success)
            {
                return true;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            // Capped as well, so the last sleep cannot walk past the deadline it is waiting for.
            std::thread::sleep(Duration::from_millis(300).min(left));
        }
    }

    /// Asks espanso to perform an expansion, wherever the keyboard focus happens to be.
    ///
    /// Fired and forgotten rather than waited on: the caller has just handed the focus back to the
    /// window the user was in, and blocking the interface for a second while a subprocess starts
    /// would be the one thing that made the search window feel slower than espanso's own.
    /// Asks espanso to expand a match.
    ///
    /// This is what `espansod match exec --trigger` does, minus the `espansod`. That command's
    /// only job is to write one line of JSON to the pipe espanso's worker is listening on, and
    /// starting an eighteen-megabyte copy of espanso to write fifty-six bytes for us costs about
    /// fifty milliseconds every time — which is most of the difference between an expansion that
    /// appears and one you watch arrive. The message and the protocol are espanso's own, taken
    /// from `espanso-ipc`: an externally tagged `IPCEvent`, newline terminated, no reply expected.
    ///
    /// The CLI stays as the fallback. If espanso is mid-restart there may be no pipe to write to
    /// for a moment, and the command can wait for it in a way a single file handle cannot.
    pub fn match_exec(&self, trigger: &str) -> std::io::Result<()> {
        match self.request_expansion(trigger) {
            Ok(()) => Ok(()),
            Err(_) => self.spawn_detached(&["match", "exec", "--trigger", trigger]),
        }
    }

    fn request_expansion(&self, trigger: &str) -> std::io::Result<()> {
        use std::io::Write;

        let event = serde_json::json!({
            "RequestMatchExpansion": { "trigger": trigger, "args": {} }
        });
        let mut line = serde_json::to_string(&event)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        line.push('\n');

        // A named pipe accepts one connection at a time, so a request that lands while espanso is
        // still finishing the last one gets a "busy" and is worth asking again a moment later.
        let mut attempt = 0;
        let mut pipe = loop {
            match std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(WORKER_PIPE)
            {
                Ok(pipe) => break pipe,
                Err(e) if attempt < 3 => {
                    attempt += 1;
                    std::thread::sleep(Duration::from_millis(8));
                    if attempt == 3 {
                        return Err(e);
                    }
                }
                Err(e) => return Err(e),
            }
        };
        pipe.write_all(line.as_bytes())?;
        pipe.flush()
    }

    /// The version of espanso in this folder, for display. `None` if it cannot be asked.
    ///
    /// Read once at startup and remembered; this spawns a process, which is not something to do
    /// while drawing.
    pub fn version(&self, t: &'static Strings) -> Option<String> {
        let result = self.run(&["--version"], t).ok()?;
        if !result.success {
            return None;
        }
        let version = result.stdout.split_whitespace().last()?.trim().to_string();
        (!version.is_empty()).then_some(version)
    }

    pub fn cmd_enable(&self, t: &'static Strings) -> std::io::Result<CtlResult> {
        self.run(&["cmd", "enable"], t)
    }

    pub fn cmd_disable(&self, t: &'static Strings) -> std::io::Result<CtlResult> {
        self.run(&["cmd", "disable"], t)
    }

    /// Stops the daemon. Used when the user quits from the tray: with espanso's own icon turned
    /// off, a manager that exited while espanso kept running would leave expansions firing with
    /// nothing on screen to pause or explain them.
    /// Stops the daemon, without waiting to see it happen.
    ///
    /// Detached on purpose, and it is the one command here that has to be. Every other call goes
    /// through [`run`], which is bounded — and which enforces that bound by killing the child. That
    /// is right for a command whose answer we are about to read, and exactly wrong for this one:
    /// the only caller is Quit, which is on its way out and never looks at the result.
    ///
    /// The case it protects against is a machine where starting a process is slow — an EDR agent
    /// inspecting every launch is enough. There, the four-second cap expires before espansod has
    /// really begun, and `run` then kills the very process that was about to do the job. Quit would
    /// exit having blocked for four seconds and stopped nothing, and since espanso's own tray icon
    /// is turned off on our behalf, the daemon would be left expanding text with no icon anywhere
    /// to pause it. Detached, that same slow start simply finishes late, on a child that outlives
    /// us.
    ///
    /// Measured here: the daemon is told to stop within 50 ms — killing the command at 50 ms still
    /// stopped espanso — while the command itself takes about 230 ms to tidy up and exit. So the
    /// 800 ms of [`QUIT_GRACE`](crate::tray) covers the whole thing comfortably on a machine that
    /// is behaving, and on one that is not, the work still happens after we are gone.
    pub fn service_stop_detached(&self) -> std::io::Result<()> {
        self.spawn_detached(&["service", "stop"])
    }

    pub fn service_status(&self, t: &'static Strings) -> std::io::Result<CtlResult> {
        self.run(&["service", "status"], t)
    }

    /// Where espanso itself says its runtime folder is.
    ///
    /// Asked, not guessed. The portable layout keeps it beside the executable and that is what
    /// this app assumes everywhere else, but espanso resolves the path by its own rules, and a
    /// folder that has been moved, upgraded in place, or handed down from an older install can
    /// answer differently. Anything written to the wrong runtime folder is ignored in silence,
    /// which is the worst way to be wrong: see `silence_espanso_wizard`.
    pub fn runtime_dir(&self, t: &'static Strings) -> Option<PathBuf> {
        let result = self.run(&["path", "runtime"], t).ok()?;
        if !result.success {
            return None;
        }
        let path = result.stdout.trim();
        (!path.is_empty()).then(|| PathBuf::from(path))
    }

    /// Best-effort startup sequence: make sure the daemon is actually running. Never panics;
    /// returns the last failing step's detail so the caller can show a friendly banner.
    pub fn ensure_running(&self, t: &'static Strings) -> Result<(), String> {
        if !self.exe_exists() {
            return Err(fill(
                t.err_espansod_missing,
                &[("path", &self.exe_path.display().to_string())],
            ));
        }
        if self.is_running(t) {
            return Ok(());
        }

        let _ = self.spawn_detached(&["service", "start"]);
        if self.wait_until_running(Duration::from_secs(6), t) {
            return Ok(());
        }

        // Two more ways to start it, both silent.
        //
        // What used to be here was `espansod launcher`, and that was a mistake with a face: the
        // launcher is espanso's *onboarding*, the "Espanso is running! try typing :espanso"
        // window — precisely the window this app exists to replace. On a machine where the
        // service takes more than six seconds to answer at boot (a corporate laptop with an EDR
        // agent inspecting every process start is enough) that fallback fired every single time
        // the computer was switched on, and espanso's welcome greeted the user every morning.
        //
        // `--unmanaged` starts the same service without needing it registered with Windows, which
        // was the real reason a fallback existed. `daemon` starts the daemon in the spawned
        // process itself. Neither draws anything.
        let _ = self.spawn_detached(&["service", "start", "--unmanaged"]);
        if self.wait_until_running(Duration::from_secs(6), t) {
            return Ok(());
        }

        self.spawn_detached(&["daemon"])
            .map_err(|e| fill(t.err_espanso_start, &[("err", &e.to_string())]))?;
        if self.wait_until_running(Duration::from_secs(6), t) {
            return Ok(());
        }

        Err(t.err_espanso_no_response.to_string())
    }

    /// Asks espansod to reload its configuration after a save, then confirms it actually came
    /// back up. `service restart`'s own process is not reliably waitable (see `run`'s doc
    /// comment), so this fires it off detached and polls `service status` instead.
    pub fn restart_and_confirm(
        &self,
        timeout: Duration,
        t: &'static Strings,
    ) -> Result<(), String> {
        let _ = self.spawn_detached(&["service", "restart"]);
        // Mid-restart, `service status` may briefly report "not running" — give it a beat before
        // polling.
        std::thread::sleep(Duration::from_millis(400));
        if self.wait_until_running(timeout, t) {
            Ok(())
        } else {
            Err(t.err_restart_unconfirmed.to_string())
        }
    }
}
