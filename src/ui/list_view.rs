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

//! The library: everything you have, in one flat list, with the folders as filters above it.
//!
//! The screen reads top to bottom in the order the plan sets out — title and the real state of
//! espanso, then search and the one primary action, then the folder chips, then how many rows the
//! filter left and the way into picking several, then the list itself, then a quiet way out to help
//! and settings. Nothing here writes to disk; every action is a call into `AppState`.
//!
//! ## A folder filters, it does not contain
//!
//! Folders are assignments — see [`crate::settings`] — so the list is *one* list and a chip decides
//! which part of it you are looking at. There are no collapsible folder sections any more, and with
//! them went the three things that only existed to serve them: the per-folder drop zone, the strip
//! under the last row that took an expansion back out, and the rule that folded folders shut past a
//! handful. The chips do that work now, and they do it without laying out a single row.
//!
//! ## Four mechanisms account for most of this file
//!
//! **Virtualization.** A row far outside `ui.clip_rect()` is not drawn; it is replaced by a gap of
//! exactly its own height, measured once from a real row and cached per density. So anything asked
//! per row must be cheap *and* must be asked after that check — `Context::is_being_dragged` reads
//! like a cheap lookup and is an exclusive lock on the whole context, which is why the pass-wide
//! answers are hoisted into [`RowPass`] before the loop. The one open row is exempt: it is always
//! drawn in full, and it never writes to the height cache, because its height is not a row's height.
//!
//! **Hover.** Two slots, one frame apart. A row must know whether it is highlighted *before* it
//! draws, which is before it can learn whether the pointer is over it, so it reads last frame's
//! answer; the pending slot is what stops the highlight sticking to a row that has since been
//! scrolled away, filtered out or deleted.
//!
//! **Drag and drop.** A drop is a call to `AppState::assign_folder_to_triggers` and nothing moves in
//! `base.yml`. The targets are the folder chips themselves. `DRAG_START_DISTANCE` keeps an ordinary
//! click from becoming a drag, and `drag_autoscroll` must be called from *inside* the `ScrollArea`
//! closure.
//!
//! **Choosing several.** An explicit mode, entered from a button and never by accident — except by
//! Ctrl- or Shift-clicking a row, which is what someone who already knows Windows will try first.
//! Changing folder or search empties the selection and says so, because a selection that survives a
//! filter change is a selection nobody can see.

use crate::app::{
    accent, hairline, truncate, AppState, DragPayload, ListCache, ListRow, PendingConfirmKind, View,
};
use crate::i18n::{fill, Strings};
use crate::ui::controls::{self, Tone};
use crate::ui::edit_form::EditState;
use crate::ui::studio;

/// Where the pointer currently is, remembered between frames so a row can tint itself without
/// taking a hover sense of its own that would compete with the buttons inside it. Stored as an
/// `Id` rather than the trigger text: it is read once per row per frame, and an `Id` is a plain
/// integer where a `String` would be a copy.
fn hovered_row_key() -> egui::Id {
    egui::Id::new("hovered_row")
}

/// Where a row reports, as it is drawn, that the pointer is over it — read back on the *next* frame.
///
/// Two slots, not one, and the reason is the order things happen in. A row has to know whether it is
/// the highlighted one *before* it draws itself, which is before it can find out whether the pointer
/// is over it; so what it reads has to be the previous frame's answer. Clearing the single slot at
/// the top of a frame would therefore mean no row ever reads a highlight at all.
///
/// With a pending slot the highlight also stops being sticky, which is what this is really for. The
/// old single slot was only ever written to, never cleared, so the last row the pointer touched
/// stayed lit — after the pointer moved to a different part of the window, after it left the window
/// entirely, indefinitely. Now nothing writes to the pending slot unless a row is genuinely under
/// the pointer, and the swap at the start of the next frame drops whatever is stale: a row that was
/// left behind, scrolled out of the list, filtered away by the search, or deleted.
fn hovered_row_pending_key() -> egui::Id {
    egui::Id::new("hovered_row_pending")
}

/// Promotes what the rows reported last frame into the answer they will read this frame.
fn begin_hover_frame(ctx: &egui::Context) {
    ctx.data_mut(|d| {
        match d.get_temp::<egui::Id>(hovered_row_pending_key()) {
            Some(id) => {
                d.insert_temp(hovered_row_key(), id);
            }
            None => d.remove::<egui::Id>(hovered_row_key()),
        }
        d.remove::<egui::Id>(hovered_row_pending_key());
    });
}

/// Asks for one more frame when what the rows just reported differs from what they were shown.
///
/// Without this the highlight would linger on the way out. The pointer leaving the window is a
/// single event: the frame it arrives in still draws the old highlight (that frame read the
/// previous answer), and the corrected frame only happens if something asks for one. Nothing else
/// would — there is no more input to react to.
fn end_hover_frame(ctx: &egui::Context) {
    let (shown, reported) = ctx.data(|d| {
        (
            d.get_temp::<egui::Id>(hovered_row_key()),
            d.get_temp::<egui::Id>(hovered_row_pending_key()),
        )
    });
    if shown != reported {
        ctx.request_repaint();
    }
}

// ---------------------------------------------------------------------------------------------
// Pointing at what just happened
// ---------------------------------------------------------------------------------------------

/// How long a just-saved expansion stays lit, and how much of that it spends fading out.
///
/// Long enough to find with the eye once the editor has closed, short enough to be gone before
/// anyone could mistake it for a selection.
const FLASH_SECONDS: f64 = 2.0;
const FLASH_FADE: f64 = 0.8;

fn flash_key() -> egui::Id {
    egui::Id::new("library-flash")
}

/// When the countdown actually began, written by the first frame that draws the flash rather than
/// by the editor. See [`flash_now`] for why those are not the same moment.
fn flash_started_key() -> egui::Id {
    egui::Id::new("library-flash-started")
}

/// Set with the flash and cleared by the row that used it, so the list is scrolled *once* rather
/// than pinned to that row for the whole two seconds.
fn flash_scroll_key() -> egui::Id {
    egui::Id::new("library-flash-scroll")
}

/// Names the expansion the library should point at as soon as it is on screen again.
///
/// The plan asks that saving return you to the search, folder and position you left — and then say
/// which row you just changed, because a list that comes back looking exactly as it did is a list
/// that says nothing happened. Called by the editor on its way out; nothing else writes here.
///
/// Kept in egui's temporary memory, like the open row and the folder filter: it is something the
/// screen is doing for two seconds, not a fact about the user's expansions.
pub(super) fn mark_saved(ctx: &egui::Context, trigger: &str) {
    ctx.data_mut(|d| {
        d.insert_temp(flash_key(), trigger.to_owned());
        // A flash left over from an earlier save has no claim on this one's two seconds.
        d.remove::<f64>(flash_started_key());
        d.insert_temp(flash_scroll_key(), true);
    });
}

/// Which row is lit right now and how brightly, or `None` once its two seconds are up.
///
/// The clock starts here, on the first frame that can actually show the row, and **not** when
/// [`mark_saved`] was called. Between those two moments sits the whole of saving: the file is
/// written and then `espansod` is restarted with a six-second deadline, all of it blocking this
/// thread. egui's `time` is wall-clock, so the frame after a save can arrive seconds later than the
/// one that asked for the flash — stamping the start time in `mark_saved` meant the countdown was
/// already spent before a single frame was drawn, and the row was never lit at all. This was not
/// visible in the code: it looked like two lines of the same function.
fn flash_now(ctx: &egui::Context) -> Option<(egui::Id, f32)> {
    let trigger = ctx.data(|d| d.get_temp::<String>(flash_key()))?;
    // The clock is read *before* the data store is locked. Both of them live on the context, and
    // asking for one from inside the other deadlocks — it is not a borrow the compiler can catch.
    let now = ctx.input(|i| i.time);
    let started = match ctx.data(|d| d.get_temp::<f64>(flash_started_key())) {
        Some(started) => started,
        None => {
            ctx.data_mut(|d| d.insert_temp(flash_started_key(), now));
            now
        }
    };
    let left = FLASH_SECONDS - (now - started);
    if left <= 0.0 {
        ctx.data_mut(|d| {
            d.remove::<String>(flash_key());
            d.remove::<f64>(flash_started_key());
            d.remove::<bool>(flash_scroll_key());
        });
        return None;
    }
    // Nothing else is asking for frames while the pointer sits still, so without this the row
    // would stay lit until the user next moved the mouse.
    ctx.request_repaint();
    Some((row_id(&trigger), (left / FLASH_FADE).min(1.0) as f32))
}

// ---------------------------------------------------------------------------------------------
// Glyph safety
// ---------------------------------------------------------------------------------------------

/// Picks the first glyph the installed fonts actually draw, falling back to plain text.
///
/// Asked of `family`, which has to be the family the caller then draws in, or the answer is about
/// the wrong fonts. For the buttons that is the icon family — Ubuntu-Light + NotoEmoji +
/// emoji-icon-font, and no face read off the machine. `Hack` is in none of them, so a symbol that
/// lives only there (U+25BE `▾`, for one) would come out as an empty box in an ordinary button,
/// which is exactly the bug this exists to make impossible. Rather than hand-picking glyphs and
/// hoping, we ask the font at runtime.
///
/// The asking is [`crate::fonts::can_draw`], and not egui's own `has_glyph`, which is wrong here
/// in a way that costs real glyphs: it reports every emoji as missing. That is why this used to
/// answer `?` for a light bulb and nothing at all for a warning sign.
///
/// The answer is cached in egui's own memory, keyed by the candidate list, so the fonts are asked
/// once per candidate set for the whole lifetime of the process rather than once per widget per
/// frame.
fn glyph(
    ctx: &egui::Context,
    family: egui::FontFamily,
    candidates: &[char],
    fallback: &str,
) -> String {
    let key = egui::Id::new("glyph_cache").with(candidates).with(&family);
    if let Some(cached) = ctx.data(|d| d.get_temp::<String>(key)) {
        return cached;
    }
    let found = candidates
        .iter()
        .copied()
        .find(|c| crate::fonts::can_draw(ctx, family.clone(), *c))
        .map(|c| c.to_string())
        .unwrap_or_else(|| fallback.to_string());
    ctx.data_mut(|d| d.insert_temp(key, found.clone()));
    found
}

/// "Show more" chevron. `⏷` lives in emoji-icon-font and `⬇` in NotoEmoji, both of which egui
/// bundles, so both are in the icon family; the plain-text fallback keeps this legible even if
/// neither resolves.
fn expand_glyph(ctx: &egui::Context) -> String {
    glyph(ctx, crate::fonts::icons_family(), &['⏷', '⬇'], "+")
}

// ---------------------------------------------------------------------------------------------
// Where the screen's own state lives
// ---------------------------------------------------------------------------------------------

/// Whether the library is in "pick several" mode. Kept in egui's temporary memory rather than in
/// `AppState`: it is a way of looking at the list, not a fact about the user's expansions, and it
/// should not survive a restart.
fn select_mode(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(egui::Id::new("library-select-mode")).unwrap_or(false))
}

fn set_select_mode(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| {
        if on {
            d.insert_temp(egui::Id::new("library-select-mode"), true);
        } else {
            d.remove::<bool>(egui::Id::new("library-select-mode"));
        }
    });
}

/// The one row whose detail is open, if any. One at a time, so the list never turns into a page of
/// unfolded cards you have to scroll past to reach the next name.
fn open_row(ctx: &egui::Context) -> Option<String> {
    ctx.data(|d| d.get_temp::<String>(egui::Id::new("library-open-row")))
}

fn set_open_row(ctx: &egui::Context, trigger: Option<&str>) {
    ctx.data_mut(|d| match trigger {
        Some(t) => {
            d.insert_temp(egui::Id::new("library-open-row"), t.to_owned());
        }
        None => d.remove::<String>(egui::Id::new("library-open-row")),
    });
}

fn move_targets_key() -> egui::Id {
    egui::Id::new("library-move-targets")
}

fn create_folder_key() -> egui::Id {
    egui::Id::new("library-create-folder")
}

/// Opens the "move to a folder" question for a set of expansions.
fn ask_to_move(ctx: &egui::Context, triggers: Vec<String>) {
    if triggers.is_empty() {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(move_targets_key(), triggers));
}

// ---------------------------------------------------------------------------------------------
// Rows: measurement and virtualization
// ---------------------------------------------------------------------------------------------

/// The things every row would otherwise ask egui for on its own, read once for the whole pass.
///
/// Both questions were being asked per row, and both were being asked *before* the virtualization
/// check that throws most rows away — so a list of a few thousand expansions paid for them a few
/// thousand times a frame to draw fifteen. `Context::is_being_dragged` is the expensive one: it
/// resolves to `interaction_snapshot`, which takes an exclusive lock on the whole context
/// (`egui-0.36.1/src/context.rs:4189-4191`), not the cheap read it reads like.
///
/// Hoisting is exact rather than an approximation: `dragged_id` is a snapshot fixed for the length
/// of a pass, and the row height is measured from rows drawn in this same density.
#[derive(Clone, Copy)]
struct RowPass {
    dragged_id: Option<egui::Id>,
    /// `None` until the first row has ever been measured, which draws the whole list in full for
    /// exactly one frame — the list can be slow once, never wrong.
    known_height: Option<f32>,
    compact: bool,
    select_mode: bool,
    /// Width of the "when you type" column, so every replacement in the list starts on one line.
    trigger_w: f32,
    /// The row saved a moment ago and how lit it still is — see [`flash_now`]. Hoisted like the
    /// rest: it is one answer for the whole pass, and it reads the clock to get it.
    flash: Option<(egui::Id, f32)>,
}

fn row_pass(ui: &egui::Ui, compact: bool, select_mode: bool, trigger_w: f32) -> RowPass {
    RowPass {
        dragged_id: ui.ctx().dragged_id(),
        known_height: ui
            .ctx()
            .data(|d| d.get_temp::<f32>(row_height_key(compact, select_mode))),
        compact,
        select_mode,
        trigger_w,
        flash: flash_now(ui.ctx()),
    }
}

fn row_height_key(compact: bool, select_mode: bool) -> egui::Id {
    egui::Id::new("row_advance").with(compact).with(select_mode)
}

/// Width of the trigger column: a share of what there is, never below the plan's minimum and never
/// so wide that the replacement loses its own column.
///
/// The reference lays the two out as `.65fr` against `1.7fr`; this is that ratio, bounded. It is a
/// *minimum* for the label inside it — a longer trigger is allowed to push past rather than being
/// cut short, because the name of the thing is what you are scanning for.
fn trigger_column(available: f32) -> f32 {
    (available * 0.28).clamp(92.0, 210.0)
}

/// The triggers a Shift-click range is allowed to run over: what the filter leaves visible, in the
/// order it is drawn.
///
/// Built at the moment of the click rather than kept beside the list: a range is only ever needed on
/// the frame a row is actually clicked, which is at most one row in the whole list.
fn visible_order(list: &ListCache, visible: &[usize]) -> Vec<String> {
    visible.iter().map(|&i| list.rows[i].trigger.clone()).collect()
}

// ---------------------------------------------------------------------------------------------
// Drag auto-scroll
// ---------------------------------------------------------------------------------------------

/// Height of the band at each edge of the list that starts auto-scrolling, in points.
const AUTOSCROLL_ZONE: f32 = 46.0;
/// The much thinner band right at the very edge that always scrolls at full speed.
const AUTOSCROLL_TURBO_ZONE: f32 = 12.0;
const AUTOSCROLL_MIN_SPEED: f32 = 90.0;
const AUTOSCROLL_MAX_SPEED: f32 = 900.0;

/// While a row drag is in flight, scrolls the list when the pointer nears its top or bottom edge,
/// so items can be dropped somewhere far away without letting go.
///
/// Must be called from *inside* the `ScrollArea`'s closure: `Ui::scroll_with_delta` writes into the
/// per-pass state that the enclosing scroll area consumes, so calling it elsewhere would silently
/// do nothing.
///
/// Speed is expressed in points per second and multiplied by the frame's own delta time, so the
/// scroll feels identical regardless of frame rate; and it only requests continuous repaints while
/// it is actually scrolling, leaving the app's normal idle throttling untouched the rest of the time.
fn drag_autoscroll(ui: &egui::Ui) {
    if !egui::DragAndDrop::has_payload_of_type::<DragPayload>(ui.ctx()) {
        return;
    }
    let Some(pointer) = ui.ctx().pointer_latest_pos() else {
        return;
    };

    // The visible window into the list, in screen coordinates.
    let view = ui.clip_rect();
    if view.height() <= AUTOSCROLL_ZONE * 2.0 {
        return; // Too short to have meaningful edges.
    }

    let from_top = pointer.y - view.top();
    let from_bottom = view.bottom() - pointer.y;

    // Ignore the pointer entirely once it has left the list horizontally or vertically, so hovering
    // over the chips above the list never scrolls it.
    if pointer.x < view.left() || pointer.x > view.right() {
        return;
    }
    if from_top < -AUTOSCROLL_TURBO_ZONE || from_bottom < -AUTOSCROLL_TURBO_ZONE {
        return;
    }

    // `depth` is 0 at the inner edge of the band and 1 at the outer edge (or beyond).
    let (direction, depth) = if from_top < AUTOSCROLL_ZONE {
        (-1.0, (AUTOSCROLL_ZONE - from_top) / AUTOSCROLL_ZONE)
    } else if from_bottom < AUTOSCROLL_ZONE {
        (1.0, (AUTOSCROLL_ZONE - from_bottom) / AUTOSCROLL_ZONE)
    } else {
        return;
    };

    let in_turbo = from_top < AUTOSCROLL_TURBO_ZONE || from_bottom < AUTOSCROLL_TURBO_ZONE;
    let speed = if in_turbo {
        AUTOSCROLL_MAX_SPEED
    } else {
        // Squared easing: gentle for most of the band, ramping up as you push further into it.
        let t = depth.clamp(0.0, 1.0);
        AUTOSCROLL_MIN_SPEED + (AUTOSCROLL_MAX_SPEED - AUTOSCROLL_MIN_SPEED) * t * t
    };

    let dt = ui.input(|i| i.stable_dt).clamp(0.0, 1.0 / 20.0);
    let distance = speed * dt * direction;

    // `PassState::scroll_delta` is negated by the scroll area before being applied, so a *negative*
    // y here moves the view further down the list.
    ui.scroll_with_delta_animation(
        egui::vec2(0.0, -distance),
        egui::style::ScrollAnimation::none(),
    );
    ui.ctx().request_repaint();
}

// ---------------------------------------------------------------------------------------------
// View switch
// ---------------------------------------------------------------------------------------------

/// Two-position segmented control for list density, drawn as a pair of icons rather than words.
///
/// The icons are painted directly instead of using font glyphs: they always render, in every
/// language and on every machine, and they carry their meaning at a glance the way a text label
/// can't. The selected side is filled with the accent colour and the other is left muted, so the
/// pair reads as a light switch — one on, one off.
///
/// Returns `Some(compact)` on the frame the user picks the *other* side.
fn view_switch(ui: &mut egui::Ui, compact: bool, t: &'static crate::i18n::Strings) -> Option<bool> {
    let mut result = None;
    let is_light = !ui.visuals().dark_mode;

    // The two halves share one outlined container, the way a segmented control does everywhere
    // else in Windows. Drawn as two loose buttons they read as two unrelated toggles; inside one
    // track it is obvious they are two positions of the same switch.
    controls::segment_track(is_light).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        // Right-to-left parent, so push the compact (denser) option first to end up on the right.
        if segment(ui, compact, true, t.view_compact_tip) {
            result = Some(true);
        }
        if segment(ui, !compact, false, t.view_comfortable_tip) {
            result = Some(false);
        }
    });
    result
}

/// One half of the view switch. `dense` picks which pictogram is drawn: four tight lines for the
/// compact view, two tall stacked cards for the comfortable one.
fn segment(ui: &mut egui::Ui, selected: bool, dense: bool, tip: &str) -> bool {
    let size = egui::vec2(30.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let is_light = !ui.visuals().dark_mode;
    controls::focus_ring(ui, &response, 3.0);

    if ui.is_rect_visible(rect) {
        // Shared with every other segmented control in the app, so the density switch and the
        // theme, language and prefix pickers cannot drift apart over time.
        controls::segment_background(ui, rect, selected, response.hovered());
        let painter = ui.painter();

        // Filled and bright when this side is active, thin and muted when it is not — the same
        // "lit / unlit" cue a physical switch gives.
        let ink = if selected {
            ui.visuals().text_color()
        } else {
            crate::app::secondary_text(is_light)
        };
        let inner = rect.shrink2(egui::vec2(8.0, 6.0));
        if dense {
            let rows = 4;
            let gap = inner.height() / rows as f32;
            for i in 0..rows {
                let y = inner.top() + gap * (i as f32 + 0.5);
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(inner.left(), y - 1.0),
                        egui::pos2(inner.right(), y + 1.0),
                    ),
                    egui::CornerRadius::same(1),
                    ink,
                );
            }
        } else {
            let cards = 2;
            let gap = inner.height() / cards as f32;
            for i in 0..cards {
                let top = inner.top() + gap * i as f32 + 1.0;
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(inner.left(), top),
                        egui::pos2(inner.right(), top + gap - 3.0),
                    ),
                    egui::CornerRadius::same(2),
                    ink,
                );
            }
        }
    }

    let response = response.on_hover_text(tip);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.clicked() && !selected
}

// ---------------------------------------------------------------------------------------------
// The screen
// ---------------------------------------------------------------------------------------------

/// Height set aside for the strip of quiet links at the foot of the screen, until the first frame
/// has measured the real one.
const FOOTER_HEIGHT: f32 = 46.0;
/// The list never gets less than this, however small the window: below it the list stops being a
/// list. One full row: at the window's minimum height (see `display::MIN_SIZE`) this fits with up
/// to three rows of folder chips above it; with more than that the page scrolls (see [`show`]).
const LIST_MIN_HEIGHT: f32 = 56.0;
/// How tall an open row's replacement box may grow before it starts scrolling instead. Rounded down
/// to whole lines at draw time, so the real cap is this or a little less, never a sliced line.
const DETAIL_MAX_HEIGHT: f32 = 180.0;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    begin_hover_frame(ui.ctx());
    // The screen is sized to the window, list included, so normally this never scrolls. It exists
    // for the one case no fixed size can cover: folder chips wrap onto as many rows as there are
    // folders, and on a short window the rows above the list can leave less than the list's
    // minimum. Then the page scrolls rather than pushing the footer out through the bottom edge.
    let page_height = ui.available_height();
    controls::page_scroll(ui, "library-page", |ui| page(ui, state, page_height));

    keyboard(ui, state);
    end_hover_frame(ui.ctx());
}

fn page(ui: &mut egui::Ui, state: &mut AppState, page_height: f32) {
    let is_light = !ui.visuals().dark_mode;
    let page_top = ui.cursor().top();

    // A chip for a folder that has since been renamed or deleted would filter the list down to
    // nothing and give no way back. Falling back to "all" is the only honest answer.
    let mut active = studio::filter(ui.ctx());
    if let Some(folder) = active.clone().filter(|f| !f.is_empty()) {
        if !state.list().folder_names.contains(&folder) {
            studio::set_filter(ui.ctx(), state, None);
            active = None;
        }
    }

    // 1 — What this is, and whether espanso is actually listening.
    title_row(ui, state, is_light);
    ui.add_space(controls::GAP_SECTION);

    // 2 — Finding something, and making something.
    search_row(ui, state, is_light);
    ui.add_space(controls::GAP_WIDE);

    let list = state.list();

    // 3 — The folders, as filters.
    folder_chips(ui, state, &list, &active, is_light);
    ui.add_space(controls::GAP_WIDE);

    // Which rows the filter leaves, in the order the file has them. Reusing the cache's own
    // grouping means this costs a clone of a list of indices rather than a scan of the expansions.
    let visible: Vec<usize> = match active.as_deref() {
        None => (0..list.rows.len()).collect(),
        Some("") => list.ungrouped.clone(),
        Some(folder) => list
            .grouped
            .iter()
            .find(|(name, _)| name == folder)
            .map(|(_, indices)| indices.clone())
            .unwrap_or_default(),
    };

    // 4 — How many, and the way into choosing several.
    count_row(ui, state, visible.len(), active.as_deref(), is_light);

    let picking = select_mode(ui.ctx());
    if picking {
        ui.add_space(controls::GAP_ROW);
        bulk_bar(ui, state, &list, &visible, is_light);
    }
    ui.add_space(controls::GAP_WIDE);

    // 5 — The list. Given the line its card must end on rather than a height, because the
    // headings inside the card are measured, not estimated: an estimate that ran 16 px short
    // was pushing the footer into the bottom margin at every size, and off the window at the
    // smallest one.
    //
    // The footer is measured too, a frame late: its buttons are as tall as the script in them, and
    // Devanagari stands taller than Latin, so a fixed allowance that fit Spanish left Hindi a few
    // pixels over — just enough to put a scroll bar on a page that has nothing to scroll.
    let footer_id = egui::Id::new("library-footer-height");
    let footer_height = ui.ctx().data(|d| d.get_temp::<f32>(footer_id)).unwrap_or(FOOTER_HEIGHT);
    // egui also puts its item spacing after the card, which is part of the gap to the footer.
    let card_bottom = page_top + page_height
        - footer_height
        - controls::GAP_WIDE
        - ui.spacing().item_spacing.y;
    show_list(ui, state, &list, &visible, card_bottom, picking, is_light);

    // 6 — Quietly, the way to everything else.
    ui.add_space(controls::GAP_WIDE);
    let footer_top = ui.cursor().top();
    footer(ui, state, is_light);
    let measured = ui.min_rect().bottom() - footer_top;
    if (measured - footer_height).abs() > 0.5 {
        ui.ctx().data_mut(|d| d.insert_temp(footer_id, measured));
        ui.ctx().request_repaint();
    }
}

/// Title, the real state of espanso, and the one button that changes it.
fn title_row(ui: &mut egui::Ui, state: &mut AppState, is_light: bool) {
    let t = state.t();
    let paused = state.paused;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = if paused { t.tray_resume } else { t.tray_pause };
            if controls::button(ui, label, Tone::Normal, true).clicked() {
                // The tray owns the daemon. The view can only ask. See [`AppState::paused`].
                state.pause_toggle_requested = true;
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(
                    egui::Label::new(controls::h2(studio::text(
                        state,
                        "Tus expansiones",
                        "Your expansions",
                        "Mga expansion mo",
                        "आपके विस्तार",
                    )))
                    .truncate(),
                )
                .on_hover_text(t.app_title);
                ui.add_space(controls::GAP_ROW);
                status_pill(ui, state, paused, is_light);
            });
        });
    });
}

/// A dot and a word: running, paused, or not answering. The dot carries the state in position and
/// colour, the word carries it in language — neither alone is enough for everybody.
///
/// "Activo" is only said once espanso has actually replied (see [`AppState::espanso_confirmed`]);
/// a daemon that never answered is not described as running because nobody paused it.
fn status_pill(ui: &mut egui::Ui, state: &AppState, paused: bool, is_light: bool) {
    let (colour, label) = if !state.espanso_confirmed {
        (
            crate::app::danger(is_light),
            studio::text(state, "Sin respuesta", "Not responding", "Hindi tumutugon", "जवाब नहीं"),
        )
    } else if paused {
        (
            crate::app::caution(is_light),
            studio::text(state, "Pausado", "Paused", "Naka-pause", "रुका हुआ"),
        )
    } else {
        (
            crate::app::success(is_light),
            studio::text(state, "Activo", "Active", "Aktibo", "सक्रिय"),
        )
    };
    let response = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = controls::GAP_TIGHT + 2.0;
            let (dot, _) = ui.allocate_exact_size(egui::Vec2::splat(9.0), egui::Sense::hover());
            ui.painter().circle_filled(dot.center(), 4.0, colour);
            ui.label(egui::RichText::new(label).small().color(colour));
        })
        .response;
    if !state.espanso_confirmed {
        let t = state.t();
        let why = studio::text(
            state,
            "Espanso no contestó la última vez que se le habló. «{pause}» o «{resume}» vuelve a intentarlo.",
            "Espanso did not answer the last time it was asked. “{pause}” or “{resume}” tries again.",
            "Hindi sumagot ang Espanso noong huling tanungin. Susubok muli ang “{pause}” o “{resume}”.",
            "पिछली बार पूछने पर Espanso ने जवाब नहीं दिया। “{pause}” या “{resume}” फिर से कोशिश करता है।",
        );
        response.on_hover_text(crate::i18n::fill(
            why,
            &[("pause", t.tray_pause), ("resume", t.tray_resume)],
        ));
    }
}

/// The search field, and the one filled button on the screen.
fn search_row(ui: &mut egui::Ui, state: &mut AppState, is_light: bool) {
    let t = state.t();
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if controls::button(
                ui,
                &format!("+  {}", t.new_expansion),
                Tone::Primary,
                true,
            )
            .clicked()
            {
                studio::create_expansion(ui.ctx(), state);
            }
            ui.add_space(controls::GAP_ROW);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(18.0, 20.0), egui::Sense::hover());
                studio::icon(
                    ui,
                    icon_rect,
                    studio::Icon::Search,
                    crate::app::secondary_text(is_light),
                );
                let room = ui.available_width() - controls::GAP_ROW;
                let field = ui.add(
                    controls::text_field(&mut state.search)
                        .id(egui::Id::new("library-search"))
                        .desired_width(room.max(80.0))
                        .hint_text(t.search_label),
                );
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::F)) {
                    field.request_focus();
                }
                // A selection made under one search cannot be seen under the next one. Rather than
                // carry it invisibly, it goes — and the banner says so, because a selection that
                // vanishes without a word reads as a bug.
                if field.changed() && !state.selected.is_empty() {
                    forget_selection(ui.ctx(), state);
                }
            });
        });
    });
}

/// Empties the selection and says out loud that it happened.
fn forget_selection(ctx: &egui::Context, state: &mut AppState) {
    state.clear_selection();
    set_select_mode(ctx, false);
    let message = studio::text(
        state,
        "Se quitó la selección al cambiar de carpeta o de búsqueda.",
        "The selection was cleared when the folder or search changed.",
        "Inalis ang pinili nang magbago ang folder o paghahanap.",
        "फ़ोल्डर या खोज बदलने पर चयन हटा दिया गया।",
    );
    state.set_info_banner(message);
}

/// The folder filters: "Todos", every folder the user has, "Sin carpeta", and a way to make one.
///
/// Each chip is also where you drop an expansion to file it. That is the whole of drag and drop
/// now — there is no folder section to aim at any more, and a chip is a bigger, steadier target
/// than a heading halfway down a scrolling list ever was.
fn folder_chips(
    ui: &mut egui::Ui,
    state: &mut AppState,
    list: &ListCache,
    active: &Option<String>,
    is_light: bool,
) {
    let t = state.t();
    let mut pick: Option<Option<String>> = None;
    let mut drop_on: Option<(Option<String>, std::sync::Arc<DragPayload>)> = None;

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(controls::GAP_TIGHT + 3.0, controls::GAP);

        let all = studio::text(state, "Todos", "All", "Lahat", "सभी");
        if controls::chip(ui, all, Some(list.rows.len()), active.is_none()).clicked() {
            pick = Some(None);
        }

        for folder in &list.folder_names {
            let count = list
                .grouped
                .iter()
                .find(|(name, _)| name == folder)
                .map_or(0, |(_, indices)| indices.len());
            let chip = controls::chip(ui, folder, Some(count), active.as_deref() == Some(folder));
            drop_target(ui, &chip, is_light);
            if chip.clicked() {
                pick = Some(Some(folder.clone()));
            }
            if let Some(payload) = chip.dnd_release_payload::<DragPayload>() {
                drop_on = Some((Some(folder.clone()), payload));
            }
        }

        let chip = controls::chip(
            ui,
            t.no_folder,
            Some(list.ungrouped.len()),
            active.as_deref() == Some(""),
        );
        drop_target(ui, &chip, is_light);
        if chip.clicked() {
            pick = Some(Some(String::new()));
        }
        if let Some(payload) = chip.dnd_release_payload::<DragPayload>() {
            drop_on = Some((None, payload));
        }

        if controls::button(ui, t.add_new_folder, Tone::Quiet, true).clicked() {
            ui.ctx()
                .data_mut(|d| d.insert_temp(create_folder_key(), String::new()));
        }
    });

    if let Some((folder, payload)) = drop_on {
        state.assign_folder_to_triggers(&payload, folder);
    }
    if let Some(folder) = pick {
        let had_selection = !state.selected.is_empty();
        studio::set_filter(ui.ctx(), state, folder.as_deref());
        set_select_mode(ui.ctx(), false);
        if had_selection {
            let message = studio::text(
                state,
                "Se quitó la selección al cambiar de carpeta o de búsqueda.",
                "The selection was cleared when the folder or search changed.",
                "Inalis ang pinili nang magbago ang folder o paghahanap.",
                "फ़ोल्डर या खोज बदलने पर चयन हटा दिया गया।",
            );
            state.set_info_banner(message);
        }
    }
}

/// Outlines a chip while something is being dragged, and lights it up when the pointer is over it,
/// so every place a row can go is visible at the moment it matters and invisible the rest of the
/// time.
fn drop_target(ui: &egui::Ui, chip: &egui::Response, is_light: bool) {
    if !egui::DragAndDrop::has_payload_of_type::<DragPayload>(ui.ctx()) {
        return;
    }
    let over = chip.contains_pointer();
    let colour = if over {
        accent(ui.visuals())
    } else {
        crate::app::line_strong(is_light)
    };
    ui.painter().rect_stroke(
        chip.rect,
        controls::RADIUS_CONTROL,
        egui::Stroke::new(if over { 2.0 } else { 1.0 }, colour),
        egui::StrokeKind::Inside,
    );
}

/// How many rows the filter left, the way into picking several, and the density switch.
fn count_row(
    ui: &mut egui::Ui,
    state: &mut AppState,
    visible: usize,
    active: Option<&str>,
    is_light: bool,
) {
    let t = state.t();
    let picking = select_mode(ui.ctx());
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(compact) = view_switch(ui, state.settings.compact_view, t) {
                state.set_compact_view(compact);
            }
            ui.add_space(controls::GAP);
            let label = if picking {
                studio::text(state, "Listo", "Done", "Tapos na", "हो गया")
            } else {
                studio::text(state, "Seleccionar", "Select", "Pumili", "चुनें")
            };
            if controls::button(ui, label, Tone::Normal, visible > 0 || picking).clicked() {
                set_select_mode(ui.ctx(), !picking);
                if picking {
                    state.clear_selection();
                }
                set_open_row(ui.ctx(), None);
            }

            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let count = crate::i18n::fill(
                    studio::plural(
                        visible,
                        studio::text(
                            state,
                            "{n} expansión",
                            "{n} expansion",
                            "{n} expansion",
                            "{n} विस्तार",
                        ),
                        studio::text(
                            state,
                            "{n} expansiones",
                            "{n} expansions",
                            "{n} expansion",
                            "{n} विस्तार",
                        ),
                    ),
                    &[("n", &visible.to_string())],
                );
                ui.label(controls::small_muted(count, is_light));

                // Only when a real folder is being looked at: there is nothing to rename, re-prefix,
                // export or delete about "all" or about "no folder".
                if let Some(folder) = active.filter(|f| !f.is_empty()) {
                    ui.add_space(controls::GAP_ROW);
                    let label = studio::text(
                        state,
                        "Opciones de carpeta",
                        "Folder options",
                        "Mga opsyon ng folder",
                        "फ़ोल्डर विकल्प",
                    );
                    if controls::button(ui, label, Tone::Quiet, true).clicked() {
                        state.view = View::FolderOptions(folder.to_owned());
                    }
                }
            });
        });
    });
}

/// What you can do to several expansions at once. Present and greyed with nothing ticked, rather
/// than absent: an action that disappears takes the explanation of itself with it.
fn bulk_bar(
    ui: &mut egui::Ui,
    state: &mut AppState,
    list: &ListCache,
    visible: &[usize],
    is_light: bool,
) {
    let t = state.t();
    let count = state.selected.len();
    let any = count > 0;
    let all_ticked = !visible.is_empty()
        && visible
            .iter()
            .all(|&i| state.selected.contains(&list.rows[i].trigger));

    let mut select_all = false;
    let mut clear = false;
    let mut move_them = false;
    let mut unfile = false;
    let mut delete = false;

    controls::inset_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(controls::GAP, controls::GAP);
            ui.label(
                egui::RichText::new(fill(t.selected_count, &[("n", &count.to_string())])).strong(),
            );

            let label = if all_ticked {
                studio::text(
                    state,
                    "Quitar selección",
                    "Clear selection",
                    "Alisin ang pinili",
                    "चयन हटाएँ",
                )
            } else {
                studio::text(
                    state,
                    "Seleccionar todas",
                    "Select all",
                    "Piliin lahat",
                    "सभी चुनें",
                )
            };
            if controls::button(ui, label, Tone::Quiet, !visible.is_empty()).clicked() {
                if all_ticked {
                    clear = true;
                } else {
                    select_all = true;
                }
            }

            let move_label = studio::text(
                state,
                "Mover a carpeta",
                "Move to folder",
                "Ilipat sa folder",
                "फ़ोल्डर में ले जाएँ",
            );
            if controls::button(ui, move_label, Tone::Normal, any).clicked() {
                move_them = true;
            }
            if controls::button(ui, t.remove_from_folder, Tone::Normal, any && state.selection_has_foldered())
                .clicked()
            {
                unfile = true;
            }
            if controls::button(ui, t.delete_selected, Tone::Danger, any).clicked() {
                delete = true;
            }
        });
    });
    let _ = is_light;

    if select_all {
        // "Todas" means every expansion the filter is showing, and nothing else. Anything hidden by
        // the folder chip or by the search is not on this screen and cannot be acted on from it.
        for &i in visible {
            state.selected.insert(list.rows[i].trigger.clone());
        }
        state.selection_anchor = visible.first().map(|&i| list.rows[i].trigger.clone());
    }
    if clear {
        state.clear_selection();
    }
    if move_them {
        ask_to_move(ui.ctx(), state.selected.iter().cloned().collect());
    }
    if unfile {
        let triggers: Vec<String> = state
            .selected
            .iter()
            .filter(|t| state.settings.folder_of(t).is_some())
            .cloned()
            .collect();
        state.assign_folder_to_triggers(&triggers, None);
    }
    if delete {
        state.request_delete_selected();
    }
}

/// A quiet strip at the foot: help, settings, and what this program is.
fn footer(ui: &mut egui::Ui, state: &mut AppState, is_light: bool) {
    let t = state.t();
    let top = ui.cursor().top();
    ui.painter().hline(
        ui.max_rect().x_range(),
        top,
        egui::Stroke::new(1.0, hairline(is_light)),
    );
    ui.add_space(controls::GAP_ROW);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(controls::small_muted(
                if crate::EXPERIMENTAL {
                    studio::text(
                        state,
                        "Vista de prueba · datos independientes",
                        "Preview · separate data",
                        "Preview · hiwalay na data",
                        "पूर्वावलोकन · अलग डेटा",
                    )
                } else {
                    concat!("Espanso Manager · ", env!("CARGO_PKG_VERSION"))
                },
                is_light,
            ));
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let help = studio::text(
                    state,
                    "Guía rápida",
                    "Quick guide",
                    "Mabilis na gabay",
                    "त्वरित गाइड",
                );
                if controls::button(ui, help, Tone::Quiet, true)
                    .on_hover_text(t.tips_button_tip)
                    .clicked()
                {
                    state.view = View::Tips;
                }
                if controls::button(ui, t.settings_button, Tone::Quiet, true).clicked() {
                    state.view = View::Settings;
                    state.refresh_autostart_cache();
                    state.ensure_espanso_version();
                }
            });
        });
    });
}

/// Ctrl+N, and Escape as a way back out of whatever the list is currently doing.
fn keyboard(ui: &mut egui::Ui, state: &mut AppState) {
    if state.pending_confirm.is_some() || state.pending_transfer.is_some() {
        return;
    }
    if ui.ctx().data(|d| d.get_temp::<Vec<String>>(move_targets_key()).is_some())
        || ui.ctx().data(|d| d.get_temp::<String>(create_folder_key()).is_some())
    {
        return;
    }
    if ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::N)) {
        studio::create_expansion(ui.ctx(), state);
        return;
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        if open_row(ui.ctx()).is_some() {
            set_open_row(ui.ctx(), None);
        } else if select_mode(ui.ctx()) {
            set_select_mode(ui.ctx(), false);
            state.clear_selection();
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The list itself
// ---------------------------------------------------------------------------------------------

fn list_card(is_light: bool) -> egui::Frame {
    egui::Frame::default()
        .fill(crate::app::win_card_for(is_light))
        .stroke(egui::Stroke::new(1.0, hairline(is_light)))
        .corner_radius(controls::RADIUS_LIST)
}

fn show_list(
    ui: &mut egui::Ui,
    state: &mut AppState,
    list: &ListCache,
    visible: &[usize],
    card_bottom: f32,
    picking: bool,
    is_light: bool,
) {
    let t = state.t();
    let trigger_w = trigger_column(ui.available_width());
    let pass = row_pass(ui, state.settings.compact_view, picking, trigger_w);
    let open = open_row(ui.ctx());
    let active = studio::filter(ui.ctx());

    list_card(is_light).show(ui, |ui| {
        ui.set_width(ui.available_width());
        column_headings(ui, state, trigger_w, picking, is_light);
        // Less the card's own 1 px outline, which sits outside its contents.
        let height = (card_bottom - ui.cursor().top() - 1.0).max(LIST_MIN_HEIGHT);

        egui::ScrollArea::vertical()
            .id_salt(("library", &active))
            .max_height(height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                drag_autoscroll(ui);

                if list.total_entries == 0 {
                    empty_state(ui, t.empty_title, t.empty_hint, is_light);
                    return;
                }
                if visible.is_empty() {
                    // Three different nothings, and telling them apart matters: a brand new folder
                    // that reported "nothing matches your search" when nothing had been searched
                    // for was the first thing this screen got wrong out loud.
                    let (title, hint) = if !state.search.trim().is_empty() {
                        (t.no_matches, "")
                    } else if matches!(active.as_deref(), Some("")) {
                        (
                            studio::text(
                                state,
                                "Todas tus expansiones están en alguna carpeta.",
                                "Every expansion is filed in a folder.",
                                "Nasa folder na ang lahat ng expansion mo.",
                                "आपके सभी विस्तार किसी फ़ोल्डर में हैं।",
                            ),
                            "",
                        )
                    } else if active.is_some() {
                        (
                            studio::text(
                                state,
                                "Esta carpeta está vacía.",
                                "This folder is empty.",
                                "Walang laman ang folder na ito.",
                                "यह फ़ोल्डर खाली है।",
                            ),
                            studio::text(
                                state,
                                "Arrastra expansiones hasta su pestaña, o ábrelas y usa «Mover a carpeta».",
                                "Drag expansions onto its tab, or open one and use \"Move to folder\".",
                                "I-drag ang mga expansion sa tab nito, o buksan ang isa at gamitin ang \"Ilipat sa folder\".",
                                "विस्तार को इसके टैब पर खींचें, या कोई खोलकर «फ़ोल्डर में ले जाएँ» चुनें।",
                            ),
                        )
                    } else {
                        (t.no_matches, "")
                    };
                    empty_state(ui, title, hint, is_light);
                    return;
                }

                egui::Frame::default()
                    .inner_margin(egui::Margin::symmetric(4, 4))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let mut previous_lit = true; // No rule above the first row.
                        for &index in visible {
                            let row = &list.rows[index];
                            let is_open = open.as_deref() == Some(row.trigger.as_str());
                            previous_lit = show_row(
                                ui,
                                state,
                                list,
                                row,
                                visible,
                                pass,
                                is_open,
                                previous_lit,
                            );
                        }
                    });
            });
    });
}

fn empty_state(ui: &mut egui::Ui, title: &str, hint: &str, is_light: bool) {
    ui.add_space(32.0);
    ui.vertical_centered(|ui| {
        ui.label(controls::muted(title, is_light));
        if !hint.is_empty() {
            ui.add_space(controls::GAP);
            ui.label(controls::small_muted(hint, is_light));
        }
    });
    ui.add_space(32.0);
}

/// "Cuando escribes → Aparece este texto", standing over the two columns it names.
fn column_headings(
    ui: &mut egui::Ui,
    state: &AppState,
    trigger_w: f32,
    picking: bool,
    is_light: bool,
) {
    let head = egui::Frame::default()
        .inner_margin(egui::Margin {
            left: 18,
            right: 18,
            top: 11,
            bottom: 11,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = controls::GAP_WIDE;
                if picking {
                    ui.add_space(controls::CHECKBOX + controls::GAP_ROW - controls::GAP_WIDE);
                }
                // Padded out to the column width exactly the way a row pads its trigger, rather
                // than allocated a box of that width: `allocate_ui_with_layout` gives the content
                // the space it asks for but then claims only the space the content used, so the
                // heading would sit over the first column while the rows below it lined up with
                // something 90 points to the right.
                let placed = ui.label(controls::small_muted(
                    studio::text(
                        state,
                        "Cuando escribes",
                        "When you type",
                        "Kapag tina-type mo",
                        "जब आप लिखते हैं",
                    ),
                    is_light,
                ));
                let padding = trigger_w - placed.rect.width() - ui.spacing().item_spacing.x;
                if padding > 0.0 {
                    ui.add_space(padding);
                }
                ui.label(controls::small_muted(
                    studio::text(
                        state,
                        "Aparece este texto",
                        "This text appears",
                        "Lalabas ang tekstong ito",
                        "यह टेक्स्ट दिखता है",
                    ),
                    is_light,
                ));
            });
        });
    let rect = head.response.rect;
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(1.0, hairline(is_light)),
    );
}

fn open_edit_for(state: &mut AppState, trigger: &str) {
    let Some(index) = entry_index(state, trigger) else {
        return;
    };
    if let crate::yaml::model::MatchEntry::Simple(m) = &state.match_file.entries[index] {
        let folder = state.settings.folder_of(&m.trigger).map(str::to_string);
        state.view = View::Edit(EditState::new_for_edit(index, m, folder));
    }
}

fn entry_index(state: &AppState, trigger: &str) -> Option<usize> {
    state
        .match_file
        .entries
        .iter()
        .position(|e| e.trigger_str() == trigger)
}

/// The drag handle's id for a row, used both to drive the drag and to tint the hovered row.
fn row_id(trigger: &str) -> egui::Id {
    egui::Id::new("expansion_row").with(trigger)
}

/// Draws one row, or a gap of exactly its height when it is far enough off screen not to matter.
///
/// Returns whether the row ended up highlighted, which is what the next row needs in order to know
/// whether to draw a rule above itself: a hairline through a tinted block reads as damage.
#[allow(clippy::too_many_arguments)]
fn show_row(
    ui: &mut egui::Ui,
    state: &mut AppState,
    list: &ListCache,
    row: &ListRow,
    visible: &[usize],
    pass: RowPass,
    is_open: bool,
    previous_lit: bool,
) -> bool {
    let id = row_id(&row.trigger);
    let dragging = pass.dragged_id == Some(id);
    let flashed = pass.flash.is_some_and(|(lit, _)| lit == id);

    // Rows far outside the visible window are replaced by a gap of exactly their own height.
    //
    // Nothing off-screen can be seen, hovered or clicked, so laying one out in full is work with no
    // observable effect — and with a few thousand expansions in a single flat list, that work was
    // the entire cost of a scroll. The height is not guessed: it is measured from rows that *were*
    // drawn, in this same density, so the gap is the exact size of the thing it stands in for and
    // the scrollbar stays truthful. Before the first row has ever been measured, everything is
    // drawn normally, so the list can never be wrong — only, for one frame, slower.
    //
    // The band kept either side of the viewport means a row is already laid out by the time it
    // scrolls into view. A row being dragged is always drawn wherever it is, and so is the one open
    // row: its height is not a row's height, and standing in for it with a row-sized gap would make
    // the scrollbar lie by however tall its detail is.
    //
    // The row that was just saved is drawn too, wherever in the list it has landed. It is about to
    // ask to be scrolled into view, and a gap standing in for it cannot ask for anything.
    if !dragging && !is_open && !flashed {
        if let Some(height) = pass.known_height {
            let top = ui.cursor().top();
            let view = ui.clip_rect();
            let band = height * 3.0;
            if top + height < view.top() - band || top > view.bottom() + band {
                ui.add_space(height);
                return false;
            }
        }
    }

    let before = ui.cursor().top();
    let lit = show_row_inner(ui, state, list, row, visible, pass, is_open, previous_lit);
    if !dragging && !is_open {
        let height = ui.cursor().top() - before;
        // `known_height` is this frame's snapshot, so on the one frame where the height actually
        // changes — a switch between the two densities, or entering the picking mode — every drawn
        // row writes it once instead of just the first. Fifteen writes, once, against the few
        // thousand reads a frame this replaced.
        if height > 1.0 && pass.known_height != Some(height) {
            ui.ctx().data_mut(|d| {
                d.insert_temp(row_height_key(pass.compact, pass.select_mode), height)
            });
        }
    }
    lit
}

/// A chevron, painted rather than set. A glyph from a font lands at whatever size and baseline that
/// font decided; this one has to sit in an 18-point box next to text, on four alphabets.
fn chevron(ui: &egui::Ui, rect: egui::Rect, open: bool, ink: egui::Color32) {
    let c = rect.center();
    let (w, h) = (4.5, 2.7);
    let points = if open {
        vec![
            egui::pos2(c.x - w, c.y + h),
            egui::pos2(c.x, c.y - h),
            egui::pos2(c.x + w, c.y + h),
        ]
    } else {
        vec![
            egui::pos2(c.x - w, c.y - h),
            egui::pos2(c.x, c.y + h),
            egui::pos2(c.x + w, c.y - h),
        ]
    };
    ui.painter()
        .add(egui::Shape::line(points, egui::Stroke::new(1.5, ink)));
}

#[allow(clippy::too_many_arguments)]
fn show_row_inner(
    ui: &mut egui::Ui,
    state: &mut AppState,
    list: &ListCache,
    row: &ListRow,
    visible: &[usize],
    pass: RowPass,
    is_open: bool,
    previous_lit: bool,
) -> bool {
    let t = state.t();
    let trigger = row.trigger.clone();
    let is_selected = state.selected.contains(&trigger);
    let drag_id = row_id(&trigger);
    let is_light = !ui.visuals().dark_mode;
    let accent_color = accent(ui.visuals());
    let hovered_row = ui.ctx().data(|d| d.get_temp::<egui::Id>(hovered_row_key())) == Some(drag_id);
    let flash = pass
        .flash
        .and_then(|(lit, amount)| (lit == drag_id).then_some(amount));
    let lit = is_selected || is_open || hovered_row || flash.is_some();

    // The rule between two rows, drawn rather than implied by a box around each. It stops at
    // anything tinted, so a highlighted row reads as one continuous shape.
    if !previous_lit && !lit {
        ui.painter().hline(
            ui.max_rect().x_range(),
            ui.cursor().top(),
            egui::Stroke::new(1.0, hairline(is_light)),
        );
    }

    // How many expansions a drag starting here would carry. The payload itself is only assembled
    // if a drag actually begins: building it eagerly meant copying the whole selection once for
    // every selected row, on every frame — quadratic work for something nobody had asked for yet.
    let drag_count = if is_selected { state.selected.len() } else { 1 };
    let make_payload = || -> DragPayload {
        if is_selected {
            state.selected.iter().cloned().collect()
        } else {
            vec![trigger.clone()]
        }
    };

    let mut bg = if is_selected {
        crate::app::selection_tint(is_light, accent_color)
    } else if is_open || hovered_row {
        crate::app::hover_tint(is_light)
    } else {
        egui::Color32::TRANSPARENT
    };
    // A row saved a moment ago is lit over whatever else it would have been, and fades from there
    // back to the list's own colour. Mixed into an opaque colour rather than laid over the row as a
    // translucent wash, so the fade happens behind the words instead of across them.
    if let Some(amount) = flash {
        bg = crate::app::mix(
            crate::app::win_card_for(is_light),
            accent_color,
            if is_light { 0.11 } else { 0.20 } * amount,
        );
    }

    let mut edit_clicked = false;
    let mut delete_clicked = false;
    let mut move_clicked = false;
    let mut toggle_open = false;
    let mut clicked_with: Option<(bool, bool)> = None;

    // One frame around the closed row *and* its detail, so the tint covers both. `Frame` reserves
    // its background shape before the content is laid out and fills it in afterwards, which is what
    // lets it paint behind something whose height it could not have known in advance.
    let framed = egui::Frame::default()
        .fill(bg)
        .corner_radius(controls::RADIUS_CONTROL)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            // The horizontal wrapper is what keeps a row as tall as its content: without it the
            // frame is handed the whole remaining height of the list and stretches to fill it.
            let outer = ui.horizontal(|ui| {
                drag_source(ui, drag_id, t, drag_count, make_payload, |ui| {
                    closed_row(ui, state, row, pass, is_selected, is_open, is_light);
                    egui::Rect::NOTHING
                })
            });

            if let Some(resp) = &outer.inner {
                if resp.hovered() {
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(hovered_row_pending_key(), drag_id));
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                }
                // `clicked()` on its own is not enough. egui stops calling a press a click once the
                // pointer has drifted more than six points, or once the button has been held past
                // 0.8 s — and neither of those makes it a drag *here*, where a drag has to clear
                // eighteen. Between the two thresholds sits a band that used to do nothing
                // whatsoever: a click with an unsteady hand, and a click held still while reading
                // the row it is aimed at. Both are clicks.
                //
                // The second arm has to be qualified, because a genuine drag ends here too: egui
                // forgets the drag on the release frame, so the row is drawn as an ordinary row
                // again and reports `drag_stopped` exactly like a wobble does. The latch is what
                // still knows the difference, and without it dropping a whole selection into a
                // folder would collapse it to the one row that happened to be grabbed.
                if resp.double_clicked() {
                    edit_clicked = true;
                }
                if resp.clicked() || (resp.drag_stopped() && !drag_is_deliberate(ui)) {
                    clicked_with = Some(ui.input(|i| (i.modifiers.command, i.modifiers.shift)));
                }
            }

            if is_open {
                detail(
                    ui,
                    state,
                    &trigger,
                    is_light,
                    &mut edit_clicked,
                    &mut move_clicked,
                    &mut delete_clicked,
                );
            }
            let _ = &mut toggle_open;
        });

    // Brought into view once, on the first frame after the save. `None` for the alignment means
    // "only as far as it takes to see it", so a row that was on screen anyway does not jump, and
    // the search and folder the user came back to keep the position they had.
    if flash.is_some() && ui.ctx().data(|d| d.get_temp::<bool>(flash_scroll_key())) == Some(true) {
        ui.scroll_to_rect(framed.response.rect, None);
        ui.ctx()
            .data_mut(|d| d.insert_temp(flash_scroll_key(), false));
    }

    if let Some((ctrl, shift)) = clicked_with {
        let picking = pass.select_mode;
        if picking || ctrl || shift {
            // Ctrl- or Shift-clicking a row is how someone who already knows Windows asks for
            // several, so it turns the mode on rather than being ignored.
            set_select_mode(ui.ctx(), true);
            set_open_row(ui.ctx(), None);
            let order = visible_order(list, visible);
            // In picking mode a plain click is a tick, which is what Ctrl already means here.
            state.click_select(&trigger, ctrl || picking, shift, &order);
        } else {
            toggle_open = true;
        }
        ui.ctx().request_repaint();
    }
    if toggle_open {
        set_open_row(ui.ctx(), if is_open { None } else { Some(&trigger) });
    }
    if move_clicked {
        ask_to_move(ui.ctx(), vec![trigger.clone()]);
    }
    if delete_clicked {
        if let Some(index) = entry_index(state, &trigger) {
            state.request_delete(index);
        }
    }
    if edit_clicked {
        open_edit_for(state, &trigger);
    }

    lit
}

/// The row as it looks closed: a tick box when picking, the trigger, the replacement, a chevron.
fn closed_row(
    ui: &mut egui::Ui,
    state: &AppState,
    row: &ListRow,
    pass: RowPass,
    is_selected: bool,
    is_open: bool,
    is_light: bool,
) {
    let pad_y = if pass.compact {
        controls::ROW_PAD_DENSE
    } else {
        controls::ROW_PAD_COMFORTABLE
    };
    let accent_color = accent(ui.visuals());

    egui::Frame::default()
        .inner_margin(egui::Margin {
            left: controls::ROW_PAD_X as i8,
            right: controls::ROW_PAD_X as i8,
            top: pad_y as i8,
            bottom: pad_y as i8,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = controls::GAP_WIDE;

                if pass.select_mode {
                    let (rect, _) = ui.allocate_exact_size(
                        egui::Vec2::splat(controls::CHECKBOX),
                        egui::Sense::hover(),
                    );
                    controls::paint_checkbox(ui, rect, is_selected);
                }

                // The chevron is placed first inside a right-to-left layout so it pins to the right
                // edge; the two text columns then go in a nested left-to-right layout and receive
                // exactly the width that is left. That is what lets the replacement truncate to the
                // real available width instead of to a guessed character count.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::Vec2::splat(18.0), egui::Sense::hover());
                    chevron(
                        ui,
                        rect,
                        is_open,
                        crate::app::secondary_text(is_light),
                    );

                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let trigger_text = egui::RichText::new(&row.trigger)
                            .strong()
                            .monospace()
                            .color(accent_color);
                        // A hard column, not a minimum: a trigger wider than the column is cut
                        // short here and shown whole in the open detail. Letting it push past
                        // instead would move the start of the replacement from row to row, and a
                        // ragged second column is exactly what the two headings promise it isn't.
                        let spacing = ui.spacing().item_spacing.x;
                        let used = ui
                            .scope(|ui| {
                                ui.set_max_width((pass.trigger_w - spacing).max(1.0));
                                ui.add(
                                    egui::Label::new(trigger_text).truncate().selectable(false),
                                )
                                .rect
                                .width()
                            })
                            .inner;
                        let padding = pass.trigger_w - used - spacing;
                        if padding > 0.0 {
                            ui.add_space(padding);
                        }
                        ui.add(
                            egui::Label::new(controls::muted(&row.preview, is_light))
                                .truncate()
                                .selectable(false),
                        );
                    });
                });
            });
        });
    let _ = state;
}

/// What an open row shows: the whole replacement, where it is filed, and the three things you can
/// do to it. The actions live here and only here — a list where every closed row carries Edit and
/// Delete is a list you cannot read.
#[allow(clippy::too_many_arguments)]
fn detail(
    ui: &mut egui::Ui,
    state: &AppState,
    trigger: &str,
    is_light: bool,
    edit: &mut bool,
    move_it: &mut bool,
    delete: &mut bool,
) {
    let t = state.t();
    egui::Frame::default()
        .inner_margin(egui::Margin {
            left: controls::ROW_PAD_X as i8,
            right: controls::ROW_PAD_X as i8,
            top: 0,
            bottom: controls::ROW_PAD_X as i8,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            // The list's own preview is capped so a multi-kilobyte replacement never costs the list
            // anything. Here there is exactly one row to look up, so it can show the whole thing.
            let full = state
                .match_file
                .entries
                .iter()
                .find(|e| e.trigger_label() == trigger)
                .map(|e| e.preview(t));

            // The closed row cuts a long trigger to keep the column straight, so this is the one
            // place the whole thing is readable. It wraps rather than truncates for that reason.
            ui.label(controls::small_muted(t.when_you_type, is_light));
            ui.add_space(controls::GAP_TIGHT);
            controls::inset_frame(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(trigger)
                            .strong()
                            .monospace()
                            .color(accent(ui.visuals())),
                    )
                    .wrap()
                    .selectable(true),
                );
            });

            ui.add_space(controls::GAP_ROW);
            ui.label(controls::small_muted(t.will_be_replaced_by, is_light));
            ui.add_space(controls::GAP_TIGHT);
            controls::inset_frame(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                // Capped at a whole number of lines rather than at a round number of points: a box
                // whose height falls mid-glyph shows a sliced line at every scroll position except
                // the last one, which reads as damage rather than as "there is more below".
                let line = ui.text_style_height(&egui::TextStyle::Body);
                let cap = (DETAIL_MAX_HEIGHT / line).floor().max(3.0) * line;
                egui::ScrollArea::vertical()
                    .id_salt(("row-detail", trigger))
                    .max_height(cap)
                    .auto_shrink([false, true])
                    .show(ui, |ui| match &full {
                        Some(text) => {
                            ui.add(egui::Label::new(text).wrap().selectable(true));
                        }
                        None => {
                            ui.label(controls::muted(t.no_matches, is_light));
                        }
                    });
            });

            ui.add_space(controls::GAP_ROW);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(controls::GAP, controls::GAP);
                controls::tag_frame(is_light).show(ui, |ui| {
                    let folder = state.settings.folder_of(trigger).unwrap_or(t.no_folder);
                    ui.label(controls::small_muted(folder, is_light));
                });
                ui.add_space(controls::GAP);
                if controls::button(ui, t.edit_button, Tone::Normal, true)
                    .on_hover_text(t.edit_tip)
                    .clicked()
                {
                    *edit = true;
                }
                let move_label = studio::text(
                    state,
                    "Mover a carpeta",
                    "Move to folder",
                    "Ilipat sa folder",
                    "फ़ोल्डर में ले जाएँ",
                );
                if controls::button(ui, move_label, Tone::Normal, true).clicked() {
                    *move_it = true;
                }
                if controls::button(ui, t.delete, Tone::Danger, true)
                    .on_hover_text(t.delete_tip)
                    .clicked()
                {
                    *delete = true;
                }
            });
        });
}

// ---------------------------------------------------------------------------------------------
// Dragging
// ---------------------------------------------------------------------------------------------

/// How far the pointer must travel, still held down, before a press on a row counts as a drag
/// rather than as a click that wobbled.
///
/// egui's own threshold is a few pixels, which is fine for a slider but far too eager here: letting
/// go over a folder re-files every selected expansion at once, so the gesture has to be clearly
/// meant. Roughly a fingertip's worth of movement — deliberate to perform, essentially impossible
/// to do by accident while clicking.
const DRAG_START_DISTANCE: f32 = 18.0;

/// Whether the gesture in flight has travelled far enough to count as a drag rather than a click.
///
/// Latched for the length of one press rather than measured afresh each frame. egui clears
/// `press_origin` the instant the button comes up, so on the release frame — the one frame where
/// the answer decides what the whole gesture *was* — a live measurement always reads zero, and a
/// drag that had plainly crossed the threshold would look like a press that never moved at all.
/// This mirrors how egui settles the same question for itself: `has_moved_too_much_for_a_click` is
/// a latch too, not a running comparison.
///
/// A consequence worth having on its own: wandering back towards the starting point part-way
/// through no longer cancels a drag that was already under way.
fn drag_is_deliberate(ui: &egui::Ui) -> bool {
    let (pressed, distance) = ui.input(|i| {
        (
            i.pointer.any_pressed(),
            match (i.pointer.press_origin(), i.pointer.latest_pos()) {
                (Some(origin), Some(now)) => origin.distance(now),
                _ => 0.0,
            },
        )
    });
    let key = egui::Id::new("row_drag_is_deliberate");
    ui.ctx().data_mut(|d| {
        let latched = d.get_temp_mut_or_default::<bool>(key);
        if pressed {
            *latched = false;
        }
        *latched |= distance >= DRAG_START_DISTANCE;
        *latched
    })
}

/// Draws a row and makes its body both clickable (to select or unfold) and draggable (to re-file).
///
/// `add_contents` returns the screen rect of anything inside the row that takes its own clicks;
/// that region is excluded from the row's interactive area. egui resolves overlapping widgets by
/// taking the topmost, and this interaction is registered *after* the contents, so without the
/// exclusion the row would swallow their clicks entirely. The closed row has nothing of the sort
/// and returns `Rect::NOTHING`.
///
/// While a drag is in progress the row is redrawn on a floating layer that follows the pointer —
/// as a compact badge when several expansions are moving together, so the preview reflects the
/// whole selection rather than just the row that happened to be grabbed.
fn drag_source(
    ui: &mut egui::Ui,
    id: egui::Id,
    t: &'static Strings,
    count: usize,
    payload: impl Fn() -> DragPayload,
    add_contents: impl FnOnce(&mut egui::Ui) -> egui::Rect,
) -> Option<egui::Response> {
    let deliberate = drag_is_deliberate(ui);
    if deliberate && ui.ctx().is_being_dragged(id) {
        let layer_id = egui::LayerId::new(egui::Order::Tooltip, id);
        let response = ui
            .scope_builder(egui::UiBuilder::new().layer_id(layer_id), |ui| {
                if count > 1 {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(fill(t.moving_n, &[("n", &count.to_string())]));
                    });
                } else {
                    add_contents(ui);
                }
            })
            .response;
        if let Some(pointer_pos) = ui.ctx().pointer_interact_pos() {
            let delta = pointer_pos - response.rect.center();
            ui.ctx()
                .transform_layer_shapes(layer_id, egui::emath::TSTransform::from_translation(delta));
        }
        // Keep the payload alive for the whole drag, so every drop zone stays armed. This branch
        // never runs on the frame the button comes up — egui forgets the drag as it processes the
        // release — so it cannot hand a drop zone back a payload another one has just taken.
        egui::DragAndDrop::set_payload(ui.ctx(), payload());
        None
    } else {
        let inner = ui.scope(add_contents);
        let row_rect = inner.response.rect;
        let buttons_rect = inner.inner;

        let right_edge = if buttons_rect.is_positive() {
            buttons_rect.left() - 4.0
        } else {
            row_rect.right()
        };
        if right_edge <= row_rect.left() {
            return None;
        }
        let interact_rect =
            egui::Rect::from_min_max(row_rect.min, egui::pos2(right_edge, row_rect.max.y));
        let response = ui.interact(interact_rect, id, egui::Sense::click_and_drag());
        // Tab reaches rows too. The ring goes *inside* the row: outside, the list card would clip
        // it on the first and last rows and against the card's sides.
        if response.has_focus() {
            ui.painter().rect_stroke(
                row_rect.shrink(1.0),
                controls::RADIUS_CONTROL,
                egui::Stroke::new(2.0, ui.visuals().strong_text_color()),
                egui::StrokeKind::Inside,
            );
        }
        // A press held perfectly still generates no input events, so the interface never wakes and
        // egui never gets a frame in which to notice that the press has outlasted the 0.8 s it
        // allows a click. It notices on the release instead — by which point it has given up on
        // reading the gesture as a click and discarded the pending drag in the same breath, so the
        // row comes out neither clicked nor dragged. Asking for the next frame while the button is
        // down gives egui somewhere to make that decision, and costs frames only for as long as a
        // button is actually held on this row.
        if response.is_pointer_button_down_on() {
            ui.ctx().request_repaint();
        }
        // egui only reports `is_being_dragged` once the pointer clears its drag threshold, so
        // publishing the payload on the first `dragged()` frame is what arms the drop zones.
        if deliberate && response.dragged() {
            egui::DragAndDrop::set_payload(ui.ctx(), payload());
        }
        Some(response)
    }
}

// ---------------------------------------------------------------------------------------------
// Modals
// ---------------------------------------------------------------------------------------------

/// "Where should these go?" — asked for one expansion from its open row, or for a whole selection
/// from the bulk bar. The same question either way, so the same dialog.
pub fn show_move_modal(ctx: &egui::Context, state: &mut AppState) {
    let Some(targets) = ctx.data(|d| d.get_temp::<Vec<String>>(move_targets_key())) else {
        return;
    };
    if targets.is_empty() {
        ctx.data_mut(|d| d.remove::<Vec<String>>(move_targets_key()));
        return;
    }

    let t = state.t();
    let folders = state.settings.all_folder_names();
    let mut chosen: Option<Option<String>> = None;
    let mut close = false;

    let modal = controls::dialog(ctx, "library-move").show(ctx, |ui| {
        let is_light = !ui.visuals().dark_mode;
        ui.set_width(controls::dialog_width(ctx, 400.0));
        ui.label(controls::h3(studio::text(
            state,
            "Mover a una carpeta",
            "Move to a folder",
            "Ilipat sa isang folder",
            "किसी फ़ोल्डर में ले जाएँ",
        )));
        ui.add_space(controls::GAP);
        let n = targets.len();
        ui.label(controls::muted(
            crate::i18n::fill(
                studio::plural(
                    n,
                    studio::text(
                        state,
                        "Se moverá {n} expansión.",
                        "{n} expansion will move.",
                        "{n} expansion ang ililipat.",
                        "{n} विस्तार ले जाया जाएगा।",
                    ),
                    studio::text(
                        state,
                        "Se moverán {n} expansiones.",
                        "{n} expansions will move.",
                        "{n} expansion ang ililipat.",
                        "{n} विस्तार ले जाए जाएँगे।",
                    ),
                ),
                &[("n", &n.to_string())],
            ),
            is_light,
        ));
        ui.add_space(controls::GAP_STACK);

        egui::ScrollArea::vertical()
            .id_salt("library-move-list")
            .max_height(260.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                // The same rectangles the library filters with, wrapped: a column of buttons hid
                // the sixth folder behind the fold even in a tall window. Names are cut short
                // because a chip never wraps inside itself, and a whole sentence would run it off
                // the edge of the dialog.
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(controls::GAP, controls::GAP);
                    for folder in &folders {
                        if controls::chip(ui, &truncate(folder, 28), None, false).clicked() {
                            chosen = Some(Some(folder.clone()));
                        }
                    }
                    if controls::chip(ui, t.no_folder, None, false).clicked() {
                        chosen = Some(None);
                    }
                });
            });

        ui.add_space(controls::GAP_STACK);
        ui.horizontal(|ui| {
            if controls::button(ui, t.add_new_folder, Tone::Quiet, true).clicked() {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(create_folder_key(), String::new()));
                close = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if controls::button(ui, t.cancel, Tone::Normal, true).clicked() {
                    close = true;
                }
            });
        });
    });

    if modal.should_close() {
        close = true;
    }
    if let Some(folder) = chosen {
        state.assign_folder_to_triggers(&targets, folder);
        close = true;
    }
    if close {
        ctx.data_mut(|d| d.remove::<Vec<String>>(move_targets_key()));
    }
}

/// Making a folder that holds nothing yet. It exists the moment it is named — see
/// [`crate::settings::Settings::create_folder`] — so the chip appears straight away and the user can
/// drop things into it.
pub fn show_create_folder_modal(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut name) = ctx.data(|d| d.get_temp::<String>(create_folder_key())) else {
        return;
    };
    let t = state.t();
    let mut close = false;
    let mut create = false;

    let modal = controls::dialog(ctx, "library-create-folder").show(ctx, |ui| {
        let is_light = !ui.visuals().dark_mode;
        ui.set_width(controls::dialog_width(ctx, 380.0));
        ui.label(controls::h3(studio::text(
            state,
            "Nueva carpeta",
            "New folder",
            "Bagong folder",
            "नया फ़ोल्डर",
        )));
        ui.add_space(controls::GAP);
        ui.label(controls::muted(
            studio::text(
                state,
                "Las carpetas solo ordenan tu lista. No cambian los atajos ni el texto.",
                "Folders only tidy your list. They change neither the shortcuts nor the text.",
                "Inaayos lang ng folder ang listahan mo. Walang binabago sa shortcut o teksto.",
                "फ़ोल्डर केवल सूची व्यवस्थित करते हैं। शॉर्टकट या टेक्स्ट नहीं बदलते।",
            ),
            is_light,
        ));
        ui.add_space(controls::GAP_STACK);

        let field = ui.add(
            controls::text_field(&mut name)
                .id(egui::Id::new("library-create-folder-field"))
                .desired_width(ui.available_width())
                .hint_text(t.new_folder_placeholder),
        );
        if !field.has_focus() && !field.lost_focus() {
            field.request_focus();
        }
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

        ui.add_space(controls::GAP_STACK);
        controls::dialog_buttons(ui, |ui| {
            if controls::button(ui, t.cancel, Tone::Normal, true).clicked() {
                close = true;
            }
            let ready = !name.trim().is_empty();
            if controls::button(ui, t.save, Tone::Primary, ready).clicked() || (enter && ready) {
                create = true;
            }
        });
    });

    if modal.should_close() {
        close = true;
    }
    if create {
        let trimmed = name.trim().to_owned();
        if state.create_folder(&trimmed) {
            studio::set_filter(ctx, state, Some(&trimmed));
            close = true;
        }
    }
    if close {
        ctx.data_mut(|d| d.remove::<String>(create_folder_key()));
    } else {
        ctx.data_mut(|d| d.insert_temp(create_folder_key(), name));
    }
}

// ---------------------------------------------------------------------------------------------
// The confirmation
// ---------------------------------------------------------------------------------------------

/// Fixed width of the confirmation modal, in points. Fixed rather than fitted, so that the same
/// question is always the same shape whether it is about one expansion or forty.
const MODAL_WIDTH: f32 = 460.0;

/// Horizontal padding inside the block that holds the affected expansions.
const ITEM_BLOCK_PAD: i8 = 10;
/// How many affected expansions the modal shows before it offers "ver más".
const ITEMS_SHOWN: usize = 5;
/// The tallest the list of affected expansions may get. Past this it scrolls in place, so a
/// folder holding forty medicines still asks its question in a dialog rather than in a window
/// taller than the screen. Eight rows: it has to be comfortably more than [`ITEMS_SHOWN`], or
/// pressing "ver más" removes the button and shows nothing new, which is what 190.0 did when it
/// turned out to be exactly five rows tall.
const ITEM_LIST_MAX_HEIGHT: f32 = 300.0;

/// One affected expansion in the confirmation modal, drawn the way the list itself draws a row:
/// the trigger in the accent, the text after it as quiet supporting detail.
///
/// Both colours are passed in rather than fixed here. They used to be a single hard-coded blue
/// (`120, 175, 235`), picked against the dark theme and then used on the light one too, where it
/// sat pale on near-white.
fn item_line(
    ui: &mut egui::Ui,
    trigger: &str,
    preview: &str,
    trigger_width: f32,
    accent: egui::Color32,
    quiet: egui::Color32,
) {
    ui.horizontal(|ui| {
        // The trigger gets a column of its own, so the previews all start on one vertical line
        // instead of stepping in and out with the length of the trigger beside them.
        ui.scope(|ui| {
            ui.set_min_width(trigger_width);
            ui.label(egui::RichText::new(trigger).monospace().color(accent));
        });
        // Still capped at 34 characters, and also elided to whatever width is left: on a small
        // window a long trigger beside a 34-character preview no longer fits the dialog.
        ui.add(egui::Label::new(egui::RichText::new(truncate(preview, 34)).color(quiet)).truncate());
    });
}

/// Width of that column: the widest trigger in the set, capped so that one absurdly long trigger
/// cannot push every preview past the right edge of the modal.
fn trigger_column_width(ui: &egui::Ui, items: &[(String, String)]) -> f32 {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    items
        .iter()
        .map(|(trigger, _)| {
            ui.painter()
                .layout_no_wrap(trigger.clone(), font.clone(), egui::Color32::PLACEHOLDER)
                .size()
                .x
        })
        .fold(0.0_f32, f32::max)
        .min(150.0)
        + 10.0
}

/// In-app modal confirming a destructive action: one expansion, a whole folder, or a
/// multi-selection.
///
/// All three come through here — see [`crate::app::PendingConfirm`] for why the single one stopped
/// raising a native dialog. The shape has to survive a list of any length without ever growing
/// into a giant window, so the width is fixed, the list shows five with a "ver más" toggle for
/// the rest, and past a certain height it scrolls in place.
pub fn show_pending_confirm(ctx: &egui::Context, state: &mut AppState) {
    let Some(pending) = &state.pending_confirm else {
        return;
    };

    let t = state.settings.t();
    let count = pending.items.len();
    let n = count.to_string();
    let title = match &pending.kind {
        PendingConfirmKind::DeleteOne { .. } => t.delete_one_title.to_string(),
        PendingConfirmKind::DeleteFolder { folder } => {
            fill(t.confirm_delete_folder_title, &[("name", folder)])
        }
        // One ticked row asks exactly the same question as one clicked row — and "Eliminar 1
        // expansiones" is not a sentence in any of the four languages.
        PendingConfirmKind::DeleteSelection if count == 1 => t.delete_one_title.to_string(),
        PendingConfirmKind::DeleteSelection => {
            fill(t.confirm_delete_selection_title, &[("n", &n)])
        }
    };
    // Only the folder needs a sentence, because it is the one case where what disappears is more
    // than what is listed below: the folder itself goes too. For the other two the title and the
    // list already say everything, and a sentence repeating a count the title carries was this
    // modal's own filler.
    let intro = match &pending.kind {
        PendingConfirmKind::DeleteFolder { folder } => {
            Some(fill(t.confirm_delete_folder_body, &[("name", folder)]))
        }
        _ => None,
    };
    let expanded = pending.expanded;

    let mut confirm = false;
    let mut cancel = false;
    let mut expand = false;

    let modal = controls::dialog(ctx, "pending_confirm").show(ctx, |ui| {
        ui.set_width(controls::dialog_width(ctx, MODAL_WIDTH));
        let is_light = !ui.visuals().dark_mode;
        let danger = crate::app::danger(is_light);
        let accent_color = accent(ui.visuals());
        let quiet = crate::app::secondary_text(is_light);
        let trigger_width = trigger_column_width(ui, &pending.items);
        // Taken before anything is drawn, so the block below spans the modal instead of hugging
        // its own longest line.
        let inner_width = ui.available_width().min(MODAL_WIDTH) - 2.0 * ITEM_BLOCK_PAD as f32;

        // No mark beside the heading, by choice and not for want of a glyph. The first time this
        // was decided the reason given was that `⚠` did not resolve — that was wrong, and it was
        // egui's `has_glyph` that said so; see [`crate::fonts::can_draw`] for why it lies about
        // every emoji. Both `⚠` and `❗` draw perfectly well. They are still left out: Windows 11
        // does not put a warning glyph in its own dialogs, the red sentence below and the red
        // button already say what kind of question this is, and a triangle on top of them is one
        // alarm too many.
        ui.add(egui::Label::new(controls::h3(title)).wrap());
        ui.add_space(controls::GAP_ROW);
        if let Some(intro) = intro {
            ui.add(egui::Label::new(intro).wrap());
            ui.add_space(controls::GAP_ROW);
        }

        // The affected expansions in one contained block, so they read as a list of things rather
        // than as more of the sentence above them.
        egui::Frame::default()
            .fill(crate::app::win_control_for(is_light))
            .stroke(egui::Stroke::new(1.0, hairline(is_light)))
            .corner_radius(controls::RADIUS_CONTROL)
            .inner_margin(egui::Margin::symmetric(ITEM_BLOCK_PAD, 8))
            .show(ui, |ui| {
                ui.set_min_width(inner_width);
                let shown = if expanded { count } else { count.min(ITEMS_SHOWN) };
                // A solid scroll bar that takes up room, not egui's default floating one. The
                // floating bar is painted only while the pointer is inside the area, so a list
                // with seven more rows below the fold looked exactly like a list that ended —
                // measured on the real window, where the capture showed no bar at all. Solid, it
                // appears only when the rows really do overflow, and then it stays put.
                ui.spacing_mut().scroll.floating = false;
                // Every length reads the same way: the same aligned lines, five of them, and a
                // toggle for the rest. A folder of twelve used to draw a sideways-scrolling strip
                // of little cards instead, which asked the user to scroll a confirmation dialog
                // horizontally to find out what it was about to delete — and the strip was cut
                // off mid-word at the modal's own edge, measured on the real window.
                egui::ScrollArea::vertical()
                    .max_height(ITEM_LIST_MAX_HEIGHT)
                    // Full width, not the width of its own longest line: left to shrink, the
                    // scroll bar parks itself immediately after the longest preview instead of
                    // at the edge of the block, and reads as if it were slicing the text.
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (trigger, preview) in pending.items.iter().take(shown) {
                            item_line(ui, trigger, preview, trigger_width, accent_color, quiet);
                        }
                    });
                if count > shown {
                    ui.add_space(2.0);
                    let label = format!(
                        "{} {}",
                        expand_glyph(ui.ctx()),
                        fill(t.see_more, &[("n", &(count - shown).to_string())])
                    );
                    if ui.small_button(crate::fonts::icon(label)).clicked() {
                        expand = true;
                    }
                }
            });

        ui.add_space(controls::GAP_WIDE);
        // The one sentence that matters, in the same red as the button that carries the action, so
        // the modal never spends a second colour saying the same thing twice.
        ui.add(egui::Label::new(egui::RichText::new(t.confirm_delete_undo).color(danger)).wrap());
        ui.add_space(controls::GAP_STACK);
        controls::dialog_buttons(ui, |ui| {
            if controls::button(ui, t.cancel, Tone::Normal, true).clicked() {
                cancel = true;
            }
            // Red text on an ordinary button, which is how this app has always marked a
            // destructive action. It used to be white text here, and on the light theme the light
            // button swallowed it whole: what the user saw was a blank button next to "Cancelar".
            if controls::button(ui, t.delete, Tone::Danger, true).clicked() {
                confirm = true;
            }
        });
    });

    if modal.backdrop_response.clicked() {
        cancel = true;
    }
    if expand {
        if let Some(p) = &mut state.pending_confirm {
            p.expanded = true;
        }
    }
    if confirm {
        state.confirm_pending_delete();
    } else if cancel {
        state.cancel_pending_confirm();
    }
}
