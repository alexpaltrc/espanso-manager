# EspansoManager

A small desktop GUI (Rust + egui) for managing [espanso](https://espanso.org) text
expansions on Windows, built to sit next to `espansod.exe` inside an espanso **portable**
folder.

Version **0.0.1**. Not yet distributed to anyone.

| Light | Dark |
|---|---|
| ![Light theme](docs/screenshot-light.png) | ![Dark theme](docs/screenshot-dark.png) |

## What it does

espanso replaces short triggers you type with longer text — `:sig` becomes your whole
signature. Out of the box, changing those expansions means editing a YAML file by hand.
This app is the window that stands in for that file.

Everything is edited through the app, but nothing is locked away: the file it writes is
espanso's own `.espanso/match/base.yml`, still readable and still editable by hand.

## Features

**Managing your expansions**

- Create, edit and delete expansions without seeing a line of YAML.
- Reorder them by dragging. Drag one onto a folder to file it there.
- Group them into **folders**. Folders are this app's own idea — espanso never sees them,
  so they cost nothing in the file and break nothing if you stop using the app.
- Select several at once with Ctrl or Shift, then delete them or take them out of their
  folder in one go.
- Search box, and two list densities — comfortable cards or compact single lines.
- The trigger field shows what you will actually type as you type it, refuses an empty or
  duplicate trigger, and warns when a short trigger would swallow a longer one (`:hi` fires
  before `:hint` can ever finish).

**Pausing**

- Right-click the tray icon to **pause for ten minutes** — it resumes on its own — or to
  **pause indefinitely** until you say otherwise. For password fields, or for typing code
  your triggers would interfere with.
- Resume from the same menu. Espanso is started for you when the app opens and stopped when
  you quit, and it is reloaded after every save — you never run a command to make a change
  take effect.

**Appearance**

- **Light and dark themes**, or **follow Windows** and switch when Windows does.
- The interface is drawn in **your own Windows accent colour**, in the shade Fluent
  specifies for each theme.
- Both are picked up **the moment you change them in Windows** — the app is told, it does
  not poll — so it never sits there looking like the old theme.

**Dates and times**

- Insert the current date without learning that espanso has a variable syntax at all.
- Four ready-made formats, each shown as the date it would produce right now.
- For anything else, build the format by **dragging blocks** — Year, Month (name), Day,
  Hour — with a live preview of the result.
- The month's language is **stored with the expansion**, so "5 de septiembre" stays Spanish
  on a colleague's English computer instead of quietly becoming "5 de September".

**Living on your computer**

- Sits in the **system tray**. Closing the window with the X puts it away rather than
  shutting it down, so it never lands in front of what you were doing.
- Optionally **starts with Windows**, in the background.
- **One icon, not two**: espanso's own tray icon and its Windows notifications are turned
  off on your behalf, with a comment left in its config saying who did it and why.
- Replaces espanso's three-window setup wizard with a **single first-run screen** that lets
  you try an expansion on the spot.
- Portable. Everything lives in the folder; only the "start with Windows" checkbox writes
  anything outside it.

**Sharing and moving**

- **Export** your expansions to a file and **import** someone else's. Importing only adds —
  a trigger you already use is skipped, never overwritten, and the app tells you how many.
- Choose the **prefix** for new expansions (`:`, `::`, `;`, `?`, `//`, or your own), and
  re-prefix everything you already have in one step, after a confirmation that lists exactly
  what will be renamed.

**Four languages**

- English, Spanish, Filipino and Hindi, switchable at any time. It changes this window only
  — your expansions are never translated.

**Not losing your work**

- Every save keeps a **backup of the previous file** (the last 20), and every save is read
  back before it is trusted.
- Anything the interface cannot display — `imports:`, `global_vars:`, an expansion using
  espanso features this app has no screen for — is **carried through untouched** instead of
  being dropped.
- If `base.yml` cannot be read, the app **refuses to save over it** and says so, rather than
  replacing your expansions with an empty file.

## Credits

None of this would exist without two things:

- **[espanso](https://espanso.org)** and the extraordinary work of its developer,
  [Federico Terzi](https://federicoterzi.com). espanso does all the real work — the
  keyboard hooks, the matching, the text injection. This app only gives it a window.
- **[Claude Code](https://claude.com/claude-code)**, which wrote this program.

The author, Alex Palacios, has **no programming knowledge whatsoever**. Every line here was
written by Claude Code, at his direction and against his testing: he decided what the app
should do, used it every day, and said what was wrong until it was right. He cannot review
this code, and does not claim to.

That is worth stating plainly rather than leaving to be discovered. If you are considering
running this, read it, or build it yourself from source — do not take its correctness on
anyone's word here.

## What this is not

This is **not a fork of espanso** and contains none of its code. EspansoManager is a
separate program that talks to espanso from the outside, by launching `espansod.exe` as a
child process and reading what it prints (`src/espanso_ctl.rs`). The two are shipped in the
same folder; they are not the same work.

## Repository scope

This repository is the **source only** — under 1 MB. It deliberately does not contain:

| Not here | Why |
|---|---|
| `EspansoManager.exe`, the distribution ZIP | Build outputs. GitHub Releases is the place for these, when there is something to release. |
| `espansod.exe`, `msvcp140*.dll`, … | Not our code. |
| `.espanso/`, `.espanso-manager/`, `.espanso-runtime/` | The user's real expansions and settings. |
| `target/` | 1.3 GB build cache; regenerates in ~3 minutes. |

All of those live one level above this folder in a working copy, outside the repository —
see the note in `.gitignore` for why the root is drawn here and not higher.

The screenshots above were taken from a throwaway copy holding invented expansions, never
from a real one.

## Building

Requires a stable Rust toolchain (built with 1.98, edition 2021) on Windows.

```
cargo build --release
```

The binary lands at `target/release/EspansoManager.exe` and is meant to be copied one level
up, next to `espansod.exe`.

```
cargo test      # 49 tests
cargo clippy --release --all-targets
```

Both are expected to be clean. `profile.release` uses `panic = "abort"`; the panic hook in
`main.rs` still runs and still shows its dialog, which is verified on the built binary — see
the comment in `Cargo.toml` before changing it.

## Finding your way around

**Every file in `src/` opens with a `//!` header** stating what that file decides, what
invariant it holds, and what does not belong in it. Read that header before editing the
file — it is the real documentation, and it is kept true.

Roughly: `main.rs` owns startup order; `app.rs` owns all state and every write to disk;
`yaml/` owns the shape of `base.yml` and the promise that anything the interface cannot
display is preserved untouched; `espanso_ctl.rs` owns everything said to the daemon (all of
it blocking, all of it on a deadline); and nothing under `ui/` ever touches the disk.

## Status

In daily use by the author. Interface is finished and approved; changes that make it uglier
are not wanted. The Filipino and Hindi translations have not been reviewed by a native
speaker.

## License

Not yet chosen. Until one is added, all rights are reserved.
