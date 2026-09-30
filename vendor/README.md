# vendor/

## eframe 0.36.1, with one function changed

`eframe/` is eframe 0.36.1 exactly as published on crates.io, with a single change in
`src/native/glow_integration.rs`, function `change_gl_context`. `Cargo.toml` points at it through
`[patch.crates-io]`. It is here only for that change. Nothing else in the copy was changed, not
even formatting.

eframe is by Emil Ernerfeldt and contributors, licensed MIT OR Apache-2.0
(Copyright (c) 2018-2021 Emil Ernerfeldt <emil.ernerfeldt@gmail.com>). Both licences allow
changing and redistributing it, as long as that notice is kept.

### What changed and why

Twice per frame, eframe makes the window's OpenGL context current before it uses it: once
before clearing and once before painting. If the context already is current, it can skip that
work, and on every system except Windows it does. On Windows it never skips, because of
[egui#4289](https://github.com/emilk/egui/issues/4289). WGL's `is_current` only checks that the
thread's current context is ours. It does not check which window that context is current on. With
two viewports sharing one context, the answer could be "yes" for the wrong window.

So on Windows, every frame released the context and took it again twice. On NVIDIA that is a
driver round trip each time. Measured over a 2,000-row list at 200 Hz while the pointer moves:

- about 1.2 M of the main thread's 3.2 M cycles per frame;
- the process drops from 19–24 % to 13–18 % of a core with the change, and private memory
  from 88 MB to 80 MB.

The change keeps the early-out on Windows too. It adds `is_current_window`, which asks WGL for the
thread's current device context (`wglGetCurrentDC`) and Windows for the window it belongs to
(`WindowFromDC`), and compares that with the surface's own window. The context is skipped only
when it is ours *and* current on this window. That is exactly the check #4289 was missing, so a
second viewport (the search window, when it is on) still gets its context made current.

### Upgrading eframe

1. Copy the new version from `%USERPROFILE%\.cargo\registry\src\index.crates.io-*\eframe-<ver>\`
   over `vendor/eframe/`, all of it, after `cargo fetch` has downloaded it. Before copying, point
   the `eframe` line in `Cargo.toml` at the new version and temporarily remove the patch.
2. If upstream has fixed #4289 (the `if !cfg!(target_os = "windows")` around the early-out in
   `change_gl_context` is gone), delete `vendor/eframe/` and the `[patch.crates-io]` section, and
   stop there.
3. Otherwise, redo the change: replace that `if !cfg!(...)` block with the early-out that also
   calls `is_current_window`, and add the two `is_current_window` functions below
   `change_gl_context`. The current copy is the reference: compare it with the original with
   `diff -r` to see the whole change.
4. `cargo build --release` with no warnings. Measure hover over a big list before and after.
