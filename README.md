# EspansoManager

A small desktop GUI (Rust + egui) for managing [espanso](https://espanso.org) text
expansions on Windows, built to sit next to `espansod.exe` inside an espanso **portable**
folder. It edits `.espanso/match/base.yml`, drives the daemon, and lives in the tray.

Version **0.0.1**. Not yet distributed to anyone.

## What this is not

This is **not a fork of espanso** and contains none of its code. EspansoManager is a
separate program that talks to espanso from the outside, by launching `espansod.exe` as a
child process and reading what it prints (`src/espanso_ctl.rs`). The two are shipped in the
same folder; they are not the same work.

## Repository scope

This repository is the **source only** — 32 files, under 700 KB. It deliberately does not
contain:

| Not here | Why |
|---|---|
| `EspansoManager.exe`, the distribution ZIP | Build outputs. GitHub Releases is the place for these, when there is something to release. |
| `espansod.exe`, `msvcp140*.dll`, … | Not our code. |
| `.espanso/`, `.espanso-manager/`, `.espanso-runtime/` | The user's real expansions and settings. |
| `target/` | 1.3 GB build cache; regenerates in ~3 minutes. |

All of those live one level above this folder in a working copy, outside the repository —
see the note in `.gitignore` for why the root is drawn here and not higher.

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
