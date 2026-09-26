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

//! The library: everything you have, in one flat list, with the chosen one open beside it.
//!
//! The screen reads top to bottom — the real state of espanso on the left of the title, the three
//! borderless commands on its right; then the search and the folder picker, short and centred;
//! then the list, trigger and a one-line preview per row. Choosing a row opens the inspector at
//! the right, or, when the window is too narrow for two columns, in place of the list with a way
//! back. Nothing here writes to disk; every action is a call into `AppState`.
//!
//! ## A folder filters, it does not contain
//!
//! Folders are assignments — see [`crate::settings`] — so the list is *one* list and the folder
//! picker decides which part of it you are looking at. There are no folder sections, drop zones or
//! per-folder headers in the list; the picker's menu does that work without laying out a row.
//!
//! ## Four mechanisms account for most of this file
//!
//! **Virtualization.** A row far outside `ui.clip_rect()` is not drawn; it is replaced by a gap of
//! exactly its own height, measured once from a real row and cached per density. So anything asked
//! per row must be cheap *and* must be asked after that check — `Context::is_being_dragged` reads
//! like a cheap lookup and is an exclusive lock on the whole context, which is why the pass-wide
//! answers are hoisted into [`RowPass`] before the loop. The chosen row, the focused one, a row
//! being dragged and one just saved are exempt: each of them may have to scroll itself into view.
//!
//! **Hover.** Two slots, one frame apart. A row must know whether it is highlighted *before* it
//! draws, which is before it can learn whether the pointer is over it, so it reads last frame's
//! answer; the pending slot is what stops the highlight sticking to a row that has since been
//! scrolled away, filtered out or deleted. Those slots now only decide the chosen row's own
//! hover shade; the wash under an ordinary row is one shape for the whole list that glides from
//! row to row — see [`Glide`].
//!
//! **Drag and drop.** A drop is a call to `AppState::assign_folder_to_triggers` and nothing moves in
//! `base.yml`. The targets are a strip of folder names laid over the top of the list only while
//! something is being dragged. `DRAG_START_DISTANCE` keeps an ordinary click from becoming a drag,
//! and `drag_autoscroll` must be called from *inside* the `ScrollArea` closure.
//!
//! **Choosing several.** An explicit mode, entered from the `…` menu and never by accident — except by
//! Ctrl- or Shift-clicking a row, which is what someone who already knows Windows will try first.
//! Changing folder or search empties the selection and says so, because a selection that survives a
//! filter change is a selection nobody can see.

use crate::app::{
    accent, hairline, truncate, AppState, DragPayload, ListCache, ListRow, PendingConfirmKind, View,
};
use crate::i18n::{fill, Strings};
use crate::ui::controls::{self, Tone};
use crate::ui::edit_form::EditState;
use crate::ui::glyphs::{self, Glyph};
use crate::ui::keys;
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
// Fluid hover
// ---------------------------------------------------------------------------------------------

/// The one wash that follows the pointer down the list. Rows do not tint themselves on hover: a
/// single shape, painted under all of them, glides from the row it was on to the row the pointer
/// is on now, so moving down the list reads as one movement instead of a row going dark and
/// another lighting up. It fades in where it first lands — it never flies in from wherever the
/// pointer last left the list — and fades out, in place, when the pointer leaves.
///
/// Positions are kept relative to the top of the list's content, not the screen, so scrolling
/// carries the wash with its row instead of leaving it to catch up. That is also why the list does
/// not use [`crate::ui::glide`]'s groups, which measure from their first control: here the first
/// row drawn changes with every scroll, because rows off screen are not drawn at all.
#[derive(Clone, Copy, Default)]
struct Glide {
    top: f32,
    height: f32,
    top_speed: f32,
    height_speed: f32,
    /// 0 is gone, 1 is fully lit.
    alpha: f32,
    left: f32,
    right: f32,
}

fn glide_key() -> egui::Id {
    egui::Id::new("library-glide")
}

/// Where the row under the pointer reports its rectangle, this frame.
fn glide_target_key() -> egui::Id {
    egui::Id::new("library-glide-target")
}

/// Moves the wash one frame towards the row the pointer is over, and paints it into `slot`.
fn glide(ui: &egui::Ui, slot: egui::layers::ShapeIdx, origin: f32, is_light: bool) {
    let ctx = ui.ctx();
    let target = ctx.data_mut(|d| {
        let rect = d.get_temp::<egui::Rect>(glide_target_key());
        d.remove::<egui::Rect>(glide_target_key());
        rect
    });
    let mut g = ctx.data(|d| d.get_temp::<Glide>(glide_key())).unwrap_or_default();
    let dt = ui.input(|i| i.stable_dt).clamp(0.0, 1.0 / 20.0);
    let animate = crate::theme::animations_enabled();

    match target {
        Some(rect) => {
            let top = rect.top() - origin;
            // A fresh arrival, or Windows asked for no travelling: land in place.
            if g.alpha < 0.02 || !animate {
                g.top = top;
                g.height = rect.height();
                g.top_speed = 0.0;
                g.height_speed = 0.0;
            } else {
                let (x, v) = crate::ui::glide::spring_step(g.top - top, g.top_speed, dt);
                g.top = top + x;
                g.top_speed = v;
                let (x, v) = crate::ui::glide::spring_step(g.height - rect.height(), g.height_speed, dt);
                g.height = rect.height() + x;
                g.height_speed = v;
            }
            g.left = rect.left();
            g.right = rect.right();
            g.alpha = if animate { (g.alpha + dt / crate::ui::glide::FADE_IN).min(1.0) } else { 1.0 };
        }
        None => {
            g.alpha = if animate { (g.alpha - dt / crate::ui::glide::FADE_OUT).max(0.0) } else { 0.0 }
        }
    }

    let moving = target.is_some_and(|rect| {
        (g.top - (rect.top() - origin)).abs() > 0.1
            || (g.height - rect.height()).abs() > 0.1
            || g.top_speed.abs() > 0.5
    });
    if moving || (g.alpha > 0.0 && g.alpha < 1.0) {
        ctx.request_repaint();
    }

    if g.alpha > 0.0 {
        let rect = egui::Rect::from_min_max(
            egui::pos2(g.left, origin + g.top),
            egui::pos2(g.right, origin + g.top + g.height),
        );
        ui.painter().set(
            slot,
            egui::epaint::RectShape::filled(
                rect,
                controls::RADIUS_CONTROL,
                crate::app::hover_tint(is_light).gamma_multiply(g.alpha),
            ),
        );
    }
    ctx.data_mut(|d| d.insert_temp(glide_key(), g));
}

/// The stretch of the list the wash covers right now, in screen coordinates — where the rules
/// between rows stand aside, since a hairline through a tinted block reads as damage. Only while
/// the wash is solid enough to be the thing the eye sees there.
fn glide_band(ctx: &egui::Context, origin: f32) -> Option<egui::Rangef> {
    let g = ctx.data(|d| d.get_temp::<Glide>(glide_key()))?;
    (g.alpha > 0.35).then(|| {
        egui::Rangef::new(origin + g.top - 0.5, origin + g.top + g.height + 0.5)
    })
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
    // With Windows' animation effects off the tint does not fade: it is there, and then it is not.
    let amount = if crate::theme::animations_enabled() { (left / FLASH_FADE).min(1.0) } else { 1.0 };
    Some((row_id(&trigger), amount as f32))
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
    /// Where the hover wash stands — see [`glide_band`]. Filled in inside the scroll area, the
    /// only place that knows where the list's content starts.
    glide: Option<egui::Rangef>,
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
        glide: None,
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
// The screen
// ---------------------------------------------------------------------------------------------

/// The list never gets less than this, however small the window: below it the list stops being a
/// list. One full row.
const LIST_MIN_HEIGHT: f32 = 56.0;
/// From this width on, the chosen expansion opens in a pane beside the list; below it, the detail
/// takes the whole window and «Volver» leads back.
const INSPECTOR_BREAK: f32 = 720.0;
/// The inspector's share of the window, and the widths it is kept between.
const INSPECTOR_SHARE: f32 = 0.42;
const INSPECTOR_MIN: f32 = 300.0;
const INSPECTOR_MAX: f32 = 420.0;
/// The widest the search box gets. A search box across the whole window is a search box nobody's
/// eye goes to.
const SEARCH_MAX: f32 = 340.0;
const SEARCH_MIN: f32 = 180.0;

fn search_id() -> egui::Id {
    egui::Id::new("library-search")
}

/// Whether the search box had the keyboard at the end of the last frame. egui takes focus away on
/// Esc before this screen runs, so without it the Esc that should empty the search could not be
/// told apart from the Esc that should close the inspector.
fn search_had_focus_key() -> egui::Id {
    egui::Id::new("library-search-had-focus")
}

/// Set when the choice moved by keyboard, so the row that is now chosen scrolls itself into view
/// once, the way the row saved a moment ago does.
fn scroll_to_open_key() -> egui::Id {
    egui::Id::new("library-scroll-to-open")
}

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let ctx = ui.ctx().clone();
    begin_hover_frame(&ctx);
    // Read before anything on this screen draws a popup: a popup closes itself on the Esc of this
    // frame, and that Esc is then spent — it must not also close the inspector behind it.
    let popup_was_open = egui::Popup::is_any_open(&ctx);
    let is_light = !ui.visuals().dark_mode;

    // A folder that has since been renamed or deleted would filter the list down to nothing and
    // give no way back. Falling back to "all" is the only honest answer.
    let mut active = studio::filter(&ctx);
    if let Some(folder) = active.clone().filter(|f| !f.is_empty()) {
        if !state.list().folder_names.contains(&folder) {
            studio::set_filter(&ctx, state, None);
            active = None;
        }
    }

    let list = state.list();
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

    // The open row has to be one the list is showing: an inspector about an expansion the search
    // has just filtered away describes something that is not on the screen.
    let open = open_row(&ctx).filter(|t| visible.iter().any(|&i| list.rows[i].trigger == *t));
    if open.is_none() && open_row(&ctx).is_some() {
        set_open_row(&ctx, None);
    }
    let wide = ui.available_width() >= INSPECTOR_BREAK;

    match (&open, wide) {
        (Some(trigger), false) => {
            let trigger = trigger.clone();
            inspector(ui, state, &trigger, false, is_light);
        }
        _ => {
            header(ui, state, is_light);
            ui.add_space(controls::GAP_WIDE);
            search_row(ui, state, &list, active.as_deref(), is_light);
            ui.add_space(controls::GAP_WIDE);

            let full = ui.available_rect_before_wrap();
            let (list_rect, side) = match &open {
                Some(trigger) => {
                    let w = (full.width() * INSPECTOR_SHARE).clamp(INSPECTOR_MIN, INSPECTOR_MAX);
                    let left = egui::Rect::from_min_max(
                        full.min,
                        egui::pos2(full.right() - w - controls::GAP_SECTION, full.bottom()),
                    );
                    let right =
                        egui::Rect::from_min_max(egui::pos2(full.right() - w, full.top()), full.max);
                    (left, Some((right, trigger.clone())))
                }
                None => (full, None),
            };
            ui.scope_builder(egui::UiBuilder::new().max_rect(list_rect), |ui| {
                library(ui, state, &list, &visible, active.as_deref(), is_light);
            });
            if let Some((rect, trigger)) = side {
                ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    inspector(ui, state, &trigger, true, is_light);
                });
            }
            ui.advance_cursor_after_rect(full);
        }
    }

    keyboard(ui, state, &list, &visible, active.as_deref(), popup_was_open);
    end_hover_frame(&ctx);
}

// --- The top of the screen --------------------------------------------------------------------

/// The size of the dot beside «Activo».
const STATUS_DOT: f32 = 8.0;

/// What espanso is doing, as a word and a dot. The word says it for everyone; the dot's colour
/// says it at a glance, and its shape changes too — full while running, a ring while paused — so
/// no one needs to tell green from amber to read it.
///
/// «Activo» is only said once espanso has actually replied (see [`AppState::espanso_confirmed`]);
/// a daemon that never answered is not described as running because nobody paused it.
fn status_parts(state: &AppState, is_light: bool) -> (egui::Color32, &'static str, bool) {
    if !state.espanso_confirmed {
        (
            crate::app::danger(is_light),
            studio::text(state, "Sin respuesta", "Not responding", "Hindi tumutugon", "जवाब नहीं"),
            true,
        )
    } else if state.paused {
        (
            crate::app::caution(is_light),
            studio::text(state, "Pausado", "Paused", "Naka-pause", "रुका हुआ"),
            false,
        )
    } else {
        (
            crate::app::success(is_light),
            studio::text(state, "Activo", "Active", "Aktibo", "सक्रिय"),
            true,
        )
    }
}

fn status_width(ui: &egui::Ui, state: &AppState, is_light: bool) -> f32 {
    let (_, word, _) = status_parts(state, is_light);
    let galley = ui.painter().layout_no_wrap(
        word.to_owned(),
        egui::TextStyle::Body.resolve(ui.style()),
        egui::Color32::PLACEHOLDER,
    );
    STATUS_DOT + controls::GAP + galley.size().x
}

/// Segoe UI's capitals stand this fraction of the font size above the baseline.
const CAP_HEIGHT: f32 = 0.7;

fn status(ui: &mut egui::Ui, state: &AppState, is_light: bool) {
    let (colour, word, filled) = status_parts(state, is_light);
    let galley = ui.painter().layout_no_wrap(
        word.to_owned(),
        egui::TextStyle::Body.resolve(ui.style()),
        crate::app::secondary_text(is_light),
    );
    let size = egui::vec2(STATUS_DOT + controls::GAP + galley.size().x, controls::FIELD_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let top = rect.center().y - galley.size().y * 0.5;
    // Centred on the letters, not on the line box: the box keeps room for descenders and line
    // spacing, so its middle sits above the word and a dot there looks pinned to the top. The
    // letters' own middle is halfway between the baseline and the top of the capitals.
    let font_size = egui::TextStyle::Body.resolve(ui.style()).size;
    let letters_middle = top + controls::baseline(&galley) - font_size * CAP_HEIGHT * 0.5;
    let centre = egui::pos2(rect.left() + STATUS_DOT * 0.5, letters_middle);
    if filled {
        ui.painter().circle_filled(centre, STATUS_DOT * 0.5, colour);
    } else {
        ui.painter()
            .circle_stroke(centre, STATUS_DOT * 0.5 - 0.75, egui::Stroke::new(1.5, colour));
    }
    ui.painter().galley(
        egui::pos2(rect.left() + STATUS_DOT + controls::GAP, top),
        galley,
        egui::Color32::PLACEHOLDER,
    );
    if !state.espanso_confirmed {
        let t = state.t();
        let why = studio::text(
            state,
            "Espanso no contestó la última vez que se le habló. «{pause}» o «{resume}» vuelve a intentarlo.",
            "Espanso did not answer the last time it was asked. “{pause}” or “{resume}” tries again.",
            "Hindi sumagot ang Espanso noong huling tanungin. Susubok muli ang “{pause}” o “{resume}”.",
            "पिछली बार पूछने पर Espanso ने जवाब नहीं दिया। “{pause}” या “{resume}” फिर से कोशिश करता है।",
        );
        response.on_hover_text(fill(why, &[("pause", t.tray_pause), ("resume", t.tray_resume)]));
    }
}

/// How wide the three command buttons are together.
fn commands_width() -> f32 {
    3.0 * controls::FIELD_HEIGHT + 2.0 * controls::GAP_TIGHT
}

/// The state of espanso on the left, the title in the middle, the three commands on the right. On a
/// window too narrow for the three on one line, the title moves to a line of its own below them —
/// it is never cut short.
fn header(ui: &mut egui::Ui, state: &mut AppState, is_light: bool) {
    let title = controls::h1(studio::text(
        state,
        "Expansiones",
        "Expansions",
        "Mga expansion",
        "विस्तार",
    ));
    let title_line = controls::widget_line(ui, title.clone(), egui::TextStyle::Body);
    let width = ui.available_width();
    let side = status_width(ui, state, is_light).max(commands_width()) + controls::GAP_WIDE;
    let one_line = title_line.size().x + 2.0 * side <= width;

    let row_height = title_line.size().y.max(controls::FIELD_HEIGHT);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, row_height), egui::Sense::hover());
    let mut left = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    status(&mut left, state, is_light);
    let commands_rect = egui::Rect::from_min_max(
        egui::pos2(rect.right() - commands_width(), rect.top()),
        rect.max,
    );
    let mut right = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(commands_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    right.spacing_mut().item_spacing.x = controls::GAP_TIGHT;
    commands(&mut right, state);

    if one_line {
        let at = rect.center() - title_line.size() * 0.5;
        ui.painter().galley(at, title_line, egui::Color32::PLACEHOLDER);
    } else {
        ui.add_space(controls::GAP_TIGHT);
        ui.vertical_centered(|ui| {
            ui.add(egui::Label::new(title).wrap());
        });
    }
}

/// Pause or resume, a new expansion, and the rest behind «…».
fn commands(ui: &mut egui::Ui, state: &mut AppState) {
    let t = state.t();
    let lang = state.settings.lang;
    let (glyph, tip) = if state.paused {
        (Glyph::Play, t.tray_resume)
    } else {
        (Glyph::Pause, t.tray_pause)
    };
    if controls::icon_button(ui, glyph, tip, true).clicked() {
        // The tray owns the daemon. The view can only ask. See [`AppState::paused`].
        state.pause_toggle_requested = true;
    }
    let new_tip = keys::tip(t.new_expansion, keys::NEW, lang);
    if controls::icon_button(ui, Glyph::Add, &new_tip, true).clicked() {
        studio::create_expansion(ui.ctx(), state);
    }
    let more = controls::icon_button(
        ui,
        Glyph::More,
        studio::text(state, "Más opciones", "More options", "Iba pang opsyon", "और विकल्प"),
        true,
    );
    let picking = select_mode(ui.ctx());
    let compact = state.settings.compact_view;
    let mut toggle_picking = false;
    let mut toggle_compact = false;
    let mut go: Option<View> = None;
    menu(&more, MENU_WIDTH, |ui| {
        let label = if picking {
            studio::text(state, "Terminar selección", "Stop selecting", "Tapusin ang pagpili", "चयन समाप्त करें")
        } else {
            studio::text(state, "Seleccionar varias", "Select several", "Pumili ng ilan", "कई चुनें")
        };
        if controls::menu_item(ui, Some(Glyph::MultiSelect), label, None, Tone::Normal, true).clicked() {
            toggle_picking = true;
        }
        let dense = t.view_compact_tip;
        if controls::menu_check_item(ui, dense, None, compact).clicked() {
            toggle_compact = true;
        }
        controls::menu_separator(ui);
        let guide = t.tips_button_tip;
        if controls::menu_item(ui, Some(Glyph::Help), guide, None, Tone::Normal, true).clicked() {
            go = Some(View::Tips);
        }
        if controls::menu_item(ui, Some(Glyph::Settings), t.settings_button, None, Tone::Normal, true)
            .clicked()
        {
            go = Some(View::Settings);
        }
    });
    if toggle_picking {
        set_select_mode(ui.ctx(), !picking);
        if picking {
            state.clear_selection();
        }
        set_open_row(ui.ctx(), None);
    }
    if toggle_compact {
        state.set_compact_view(!compact);
    }
    if let Some(view) = go {
        if matches!(view, View::Settings) {
            state.refresh_autostart_cache();
            state.ensure_espanso_version();
        }
        state.view = view;
    }
}

/// How wide a flyout is, unless its words need more.
const MENU_WIDTH: f32 = 240.0;
/// How tall the list of folders in the folder menu grows before it scrolls.
const MENU_FOLDERS_MAX_HEIGHT: f32 = 320.0;

/// A flyout under `button`, opened and closed by it, laid out the way every flyout here is.
///
/// Opened from the keyboard, the first line takes the focus, so the arrows can walk the menu at
/// once; opened with the mouse, nothing is focused and no ring appears.
fn menu(button: &egui::Response, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    let by_keyboard = button.clicked() && button.has_focus();
    egui::Popup::menu(button)
        .align(egui::RectAlign::BOTTOM_END)
        .gap(controls::GAP_TIGHT)
        .width(width)
        .show(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let first = ui.next_auto_id();
            add(ui);
            if by_keyboard {
                ui.memory_mut(|m| m.request_focus(first));
            }
        });
}

/// The search box and, beside it, which folder the list is showing. Centred together as one group
/// while they fit side by side; on a narrow window, one above the other.
fn search_row(
    ui: &mut egui::Ui,
    state: &mut AppState,
    list: &ListCache,
    active: Option<&str>,
    is_light: bool,
) {
    let t = state.t();
    let lang = state.settings.lang;
    let all = studio::text(state, "Todas las carpetas", "All folders", "Lahat ng folder", "सभी फ़ोल्डर");
    let folder_label: String = match active {
        None => all.to_owned(),
        Some("") => t.no_folder.to_owned(),
        Some(folder) => truncate(folder, 28),
    };
    let picker_w = controls::subtle_button_width(ui, true, &folder_label, true);
    let width = ui.available_width();
    let side_by_side = SEARCH_MIN + controls::GAP + picker_w <= width;
    let search_w = if side_by_side {
        (width - controls::GAP - picker_w).min(SEARCH_MAX)
    } else {
        width.min(SEARCH_MAX)
    };

    let hint = studio::text(state, "Buscar", "Search", "Maghanap", "खोजें");
    let find_tip = keys::tip(t.search_label, keys::FIND, lang);
    let mut picker = None;
    let mut field_changed = false;
    let mut field_focused = false;
    let mut search = |ui: &mut egui::Ui, state: &mut AppState| {
        let field = controls::search_field(ui, search_id(), &mut state.search, hint, search_w)
            .on_hover_text(&find_tip);
        field_changed = field.changed();
        field_focused = field.has_focus();
    };
    if side_by_side {
        let group = search_w + controls::GAP + picker_w;
        ui.horizontal(|ui| {
            ui.add_space(((width - group) * 0.5).max(0.0));
            ui.spacing_mut().item_spacing.x = controls::GAP;
            search(ui, state);
            picker = Some(controls::subtle_button(
                ui,
                Some(Glyph::Folder),
                &folder_label,
                Some(Glyph::ChevronDown),
            ));
        });
    } else {
        ui.vertical_centered(|ui| search(ui, state));
        ui.add_space(controls::GAP_TIGHT);
        controls::quiet_row(ui, |ui| {
            picker = Some(controls::subtle_button(
                ui,
                Some(Glyph::Folder),
                &folder_label,
                Some(Glyph::ChevronDown),
            ));
        });
    }
    ui.ctx().data_mut(|d| d.insert_temp(search_had_focus_key(), field_focused));
    // A selection made under one search cannot be seen under the next one. Rather than carry it
    // invisibly, it goes — and the banner says so, because a selection that vanishes without a
    // word reads as a bug.
    if field_changed && !state.selected.is_empty() {
        forget_selection(ui.ctx(), state);
    }
    if let Some(picker) = picker {
        folder_menu(ui, state, list, active, &picker, all, is_light);
    }
}

/// The folders, as a flyout: every one with its count, «Sin carpeta», and at the foot the two
/// things done *to* folders.
fn folder_menu(
    ui: &mut egui::Ui,
    state: &mut AppState,
    list: &ListCache,
    active: Option<&str>,
    picker: &egui::Response,
    all: &str,
    is_light: bool,
) {
    let _ = is_light;
    let t = state.t();
    let lang = state.settings.lang;
    let mut pick: Option<Option<String>> = None;
    let mut create = false;
    let mut options = false;
    let counts: Vec<usize> = list
        .folder_names
        .iter()
        .map(|folder| {
            list.grouped
                .iter()
                .find(|(name, _)| name == folder)
                .map_or(0, |(_, indices)| indices.len())
        })
        .collect();
    let room = ui.ctx().content_rect().bottom() - picker.rect.bottom() - 160.0;
    let real_folder = active.filter(|f| !f.is_empty());
    let options_label = studio::text(
        state,
        "Opciones de carpeta…",
        "Folder options…",
        "Mga opsyon ng folder…",
        "फ़ोल्डर विकल्प…",
    );
    // The menu line draws its own plus; the string carries one for the places that have no icon.
    let new_folder = format!(
        "{}…",
        t.add_new_folder.trim_start_matches(['+', ' ']).trim_end_matches('…')
    );
    let rename_keys = keys::text(keys::RENAME, lang);

    let by_keyboard = picker.clicked() && picker.has_focus();
    egui::Popup::menu(picker)
        .align(egui::RectAlign::BOTTOM_START)
        .gap(controls::GAP_TIGHT)
        .width(MENU_WIDTH)
        .show(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let first = ui.next_auto_id();
            let total = list.rows.len().to_string();
            if controls::menu_check_item(ui, all, Some(&total), active.is_none()).clicked() {
                pick = Some(None);
            }
            if by_keyboard {
                ui.memory_mut(|m| m.request_focus(first));
            }
            egui::ScrollArea::vertical()
                .id_salt("library-folder-menu")
                .max_height(room.clamp(controls::FIELD_HEIGHT * 3.0, MENU_FOLDERS_MAX_HEIGHT))
                .show(ui, |ui| {
                    for (folder, count) in list.folder_names.iter().zip(&counts) {
                        let count = count.to_string();
                        let chosen = active == Some(folder.as_str());
                        if controls::menu_check_item(ui, &truncate(folder, 32), Some(&count), chosen)
                            .clicked()
                        {
                            pick = Some(Some(folder.clone()));
                        }
                    }
                });
            let loose = list.ungrouped.len().to_string();
            if controls::menu_check_item(ui, t.no_folder, Some(&loose), active == Some("")).clicked() {
                pick = Some(Some(String::new()));
            }
            controls::menu_separator(ui);
            if controls::menu_item(ui, Some(Glyph::Add), &new_folder, None, Tone::Normal, true).clicked() {
                create = true;
            }
            if controls::menu_item(
                ui,
                Some(Glyph::Rename),
                options_label,
                Some(&rename_keys),
                Tone::Normal,
                real_folder.is_some(),
            )
            .clicked()
            {
                options = true;
            }
        });

    if create {
        ui.ctx().data_mut(|d| d.insert_temp(create_folder_key(), String::new()));
    }
    if let (true, Some(folder)) = (options, real_folder) {
        state.view = View::FolderOptions(folder.to_owned());
    }
    if let Some(folder) = pick {
        pick_folder(ui.ctx(), state, folder.as_deref());
    }
}

/// Shows one folder, or all of them — and says so if that threw away a selection.
fn pick_folder(ctx: &egui::Context, state: &mut AppState, folder: Option<&str>) {
    let had_selection = !state.selected.is_empty();
    studio::set_filter(ctx, state, folder);
    set_select_mode(ctx, false);
    set_open_row(ctx, None);
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

// --- Dropping onto a folder ---------------------------------------------------------------------

/// While a row is being dragged, the folders it can be dropped on, floating over the top of the
/// list. They exist only for the length of the drag: the rest of the time, the folders are one
/// quiet menu and take no room at all.
fn drop_targets(ui: &mut egui::Ui, state: &mut AppState, list: &ListCache, at: egui::Rect) {
    if !egui::DragAndDrop::has_payload_of_type::<DragPayload>(ui.ctx()) {
        return;
    }
    let t = state.t();
    let is_light = !ui.visuals().dark_mode;
    let mut drop_on: Option<(Option<String>, std::sync::Arc<DragPayload>)> = None;
    egui::Area::new(egui::Id::new("library-drop-targets"))
        .order(egui::Order::Foreground)
        .fixed_pos(at.left_top())
        .interactable(true)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style())
                .fill(crate::app::win_card_for(is_light))
                .corner_radius(controls::RADIUS_DIALOG)
                .inner_margin(egui::Margin::same(controls::GAP as i8))
                .show(ui, |ui| {
                    ui.set_max_width(at.width() - 2.0 * controls::GAP);
                    ui.label(controls::small_muted(
                        studio::text(
                            state,
                            "Suelta sobre una carpeta",
                            "Drop on a folder",
                            "Ibagsak sa isang folder",
                            "किसी फ़ोल्डर पर छोड़ें",
                        ),
                        is_light,
                    ));
                    ui.add_space(controls::GAP_TIGHT);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(controls::GAP, controls::GAP);
                        let names = list
                            .folder_names
                            .iter()
                            .map(|f| (Some(f.clone()), truncate(f, 28)))
                            .chain(std::iter::once((None, t.no_folder.to_owned())));
                        for (folder, label) in names {
                            let chip = controls::chip(ui, &label, None, false);
                            let over = chip.contains_pointer();
                            ui.painter().rect_stroke(
                                chip.rect,
                                controls::RADIUS_CONTROL,
                                egui::Stroke::new(
                                    if over { 2.0 } else { 1.0 },
                                    if over { accent(ui.visuals()) } else { crate::app::line_strong(is_light) },
                                ),
                                egui::StrokeKind::Inside,
                            );
                            if let Some(payload) = chip.dnd_release_payload::<DragPayload>() {
                                drop_on = Some((folder, payload));
                            }
                        }
                    });
                });
        });
    if let Some((folder, payload)) = drop_on {
        state.assign_folder_to_triggers(&payload, folder);
    }
}

// --- Choosing several ---------------------------------------------------------------------------

/// What you can do to several expansions at once. Present and greyed with nothing ticked, rather
/// than absent: an action that disappears takes the explanation of itself with it.
fn bulk_bar(ui: &mut egui::Ui, state: &mut AppState, list: &ListCache, visible: &[usize]) {
    let t = state.t();
    let lang = state.settings.lang;
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
    let mut done = false;

    controls::inset_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(controls::GAP, controls::GAP);
            ui.label(controls::h3(fill(t.selected_count, &[("n", &count.to_string())])));

            let label = if all_ticked {
                studio::text(state, "Quitar selección", "Clear selection", "Alisin ang pinili", "चयन हटाएँ")
            } else {
                studio::text(state, "Seleccionar todas", "Select all", "Piliin lahat", "सभी चुनें")
            };
            if controls::button(ui, label, Tone::Quiet, !visible.is_empty()).clicked() {
                if all_ticked {
                    clear = true;
                } else {
                    select_all = true;
                }
            }
            let move_label = studio::text(state, "Mover a carpeta", "Move to folder", "Ilipat sa folder", "फ़ोल्डर में ले जाएँ");
            if controls::button(ui, move_label, Tone::Normal, any).clicked() {
                move_them = true;
            }
            if controls::button(ui, t.remove_from_folder, Tone::Normal, any && state.selection_has_foldered())
                .clicked()
            {
                unfile = true;
            }
            let delete_tip = keys::tip(t.delete_selected, keys::DELETE, lang);
            if controls::button(ui, t.delete_selected, Tone::Danger, any)
                .on_hover_text(delete_tip)
                .clicked()
            {
                delete = true;
            }
            let stop = keys::tip(
                studio::text(state, "Terminar selección", "Stop selecting", "Tapusin ang pagpili", "चयन समाप्त करें"),
                keys::BACK,
                lang,
            );
            if controls::icon_button(ui, Glyph::Close, &stop, true).clicked() {
                done = true;
            }
        });
    });

    if select_all {
        // "Todas" means every expansion the filter is showing, and nothing else. Anything hidden by
        // the folder or by the search is not on this screen and cannot be acted on from it.
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
    if done {
        set_select_mode(ui.ctx(), false);
        state.clear_selection();
    }
}

// --- The keyboard -------------------------------------------------------------------------------

/// Every key the library answers to, read once at the end of the frame. The table of which keys
/// those are lives in [`keys`]; this decides only what each means here.
fn keyboard(
    ui: &mut egui::Ui,
    state: &mut AppState,
    list: &ListCache,
    visible: &[usize],
    active: Option<&str>,
    popup_was_open: bool,
) {
    let ctx = ui.ctx().clone();
    if state.pending_confirm.is_some() || state.pending_transfer.is_some() {
        return;
    }
    if ctx.data(|d| d.get_temp::<Vec<String>>(move_targets_key()).is_some())
        || ctx.data(|d| d.get_temp::<String>(create_folder_key()).is_some())
    {
        return;
    }
    // A menu that is open has the keyboard: its lines take the arrows and Enter, and Esc closes it.
    if popup_was_open || egui::Popup::is_any_open(&ctx) {
        return;
    }

    if keys::pressed(&ctx, keys::NEW) {
        studio::create_expansion(&ctx, state);
        return;
    }
    if keys::pressed(&ctx, keys::FIND) || keys::pressed(&ctx, keys::FIND_ALT) {
        focus_search(&ctx, state);
        return;
    }

    let open = open_row(&ctx);
    let picking = select_mode(&ctx);

    // Esc steps back one layer at a time: out of the search, then the detail, then the picking.
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        let in_search = ctx.data(|d| d.get_temp::<bool>(search_had_focus_key())).unwrap_or(false);
        if in_search {
            if !state.search.is_empty() {
                state.search.clear();
                ctx.memory_mut(|m| m.request_focus(search_id()));
            }
        } else if open.is_some() {
            set_open_row(&ctx, None);
        } else if picking {
            set_select_mode(&ctx, false);
            state.clear_selection();
        }
        return;
    }

    if keys::typing(&ctx) {
        return;
    }
    // A button or a menu line that has the keyboard owns Enter and the arrows; only a row, or
    // nothing, lets the list have them.
    let focused = ctx.memory(|m| m.focused());
    let on_row = focused.is_none_or(|id| visible.iter().any(|&i| row_id(&list.rows[i].trigger) == id));
    if !on_row {
        return;
    }

    let order: Vec<&str> = visible.iter().map(|&i| list.rows[i].trigger.as_str()).collect();
    let here = open.as_deref().and_then(|t| order.iter().position(|o| *o == t));
    let step = if keys::pressed(&ctx, keys::DOWN) {
        Some(1isize)
    } else if keys::pressed(&ctx, keys::UP) {
        Some(-1)
    } else {
        None
    };
    if let Some(step) = step {
        if picking || order.is_empty() {
            return;
        }
        let next = match here {
            Some(i) => (i as isize + step).clamp(0, order.len() as isize - 1) as usize,
            None if step > 0 => 0,
            None => order.len() - 1,
        };
        set_open_row(&ctx, Some(order[next]));
        ctx.data_mut(|d| d.insert_temp(scroll_to_open_key(), true));
        if focused.is_some() {
            ctx.memory_mut(|m| m.request_focus(row_id(order[next])));
        }
        return;
    }

    if keys::pressed(&ctx, keys::DELETE) {
        if picking && !state.selected.is_empty() {
            state.request_delete_selected();
        } else if let Some(index) = open.as_deref().and_then(|t| entry_index(state, t)) {
            state.request_delete(index);
        }
        return;
    }
    if keys::pressed(&ctx, keys::RENAME) {
        if let Some(folder) = active.filter(|f| !f.is_empty()) {
            state.view = View::FolderOptions(folder.to_owned());
        }
        return;
    }
    // Enter on a focused row arrives as that row's click, and the row opens the editor itself.
    if focused.is_none() && keys::pressed(&ctx, keys::OPEN) {
        if let Some(trigger) = open {
            open_edit_for(state, &trigger);
        }
    }
}

/// The search box takes the keyboard, with what is already in it selected, so typing replaces it.
fn focus_search(ctx: &egui::Context, state: &AppState) {
    let id = search_id();
    ctx.memory_mut(|m| m.request_focus(id));
    if let Some(mut edit) = egui::TextEdit::load_state(ctx, id) {
        let end = state.search.chars().count();
        edit.cursor.set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(0),
            egui::text::CCursor::new(end),
        )));
        edit.store(ctx, id);
    }
}

// ---------------------------------------------------------------------------------------------
// The list itself
// ---------------------------------------------------------------------------------------------

/// Everything under the search: the picking bar when there is one, then the rows, down to the
/// bottom of the window.
fn library(
    ui: &mut egui::Ui,
    state: &mut AppState,
    list: &ListCache,
    visible: &[usize],
    active: Option<&str>,
    is_light: bool,
) {
    let picking = select_mode(ui.ctx());
    if picking {
        bulk_bar(ui, state, list, visible);
        ui.add_space(controls::GAP);
    }
    let list_top = ui.available_rect_before_wrap();
    drop_targets(ui, state, list, list_top);
    show_list(ui, state, list, visible, active, picking, is_light);
}

fn show_list(
    ui: &mut egui::Ui,
    state: &mut AppState,
    list: &ListCache,
    visible: &[usize],
    active: Option<&str>,
    picking: bool,
    is_light: bool,
) {
    let t = state.t();
    let trigger_w = trigger_column(ui.available_width());
    let pass = row_pass(ui, state.settings.compact_view, picking, trigger_w);
    let open = open_row(ui.ctx());
    let height = ui.available_height().max(LIST_MIN_HEIGHT);
    // The one row Tab stops on: the chosen one, or the first. The arrows do the rest, as in every
    // Windows list — Tab does not walk through a few hundred rows to reach the pane beside them.
    let tab_stop = open
        .clone()
        .or_else(|| visible.first().map(|&i| list.rows[i].trigger.clone()));

    egui::ScrollArea::vertical()
        .id_salt(("library", active))
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
                // that reported "nothing matches your search" when nothing had been searched for
                // was the first thing this screen got wrong out loud.
                let (title, hint) = if !state.search.trim().is_empty() {
                    (t.no_matches, "")
                } else if matches!(active, Some("")) {
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
                            "Abre una expansión y usa «Mover a carpeta», o arrástrala hasta aquí.",
                            "Open an expansion and use \"Move to folder\", or drag it here.",
                            "Buksan ang isang expansion at gamitin ang \"Ilipat sa folder\", o i-drag ito rito.",
                            "कोई विस्तार खोलकर «फ़ोल्डर में ले जाएँ» चुनें, या उसे यहाँ खींचें।",
                        ),
                    )
                } else {
                    (t.no_matches, "")
                };
                empty_state(ui, title, hint, is_light);
                return;
            }

            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            // The hover wash goes under every row, so its place in the paint order is taken now
            // and filled in once the rows have said which of them the pointer is over.
            let origin = ui.cursor().top();
            let slot = ui.painter().add(egui::Shape::Noop);
            let pass = RowPass { glide: glide_band(ui.ctx(), origin), ..pass };
            let mut previous_lit = true; // No rule above the first row.
            for &index in visible {
                let row = &list.rows[index];
                let is_open = open.as_deref() == Some(row.trigger.as_str());
                let focusable = tab_stop.as_deref() == Some(row.trigger.as_str());
                previous_lit = show_row(
                    ui,
                    state,
                    list,
                    row,
                    visible,
                    pass,
                    is_open,
                    focusable,
                    previous_lit,
                );
            }
            glide(ui, slot, origin, is_light);
        });
}

fn empty_state(ui: &mut egui::Ui, title: &str, hint: &str, is_light: bool) {
    ui.add_space(48.0);
    ui.vertical_centered(|ui| {
        ui.add(egui::Label::new(controls::muted(title, is_light)).wrap());
        if !hint.is_empty() {
            ui.add_space(controls::GAP);
            ui.add(egui::Label::new(controls::small_muted(hint, is_light)).wrap());
        }
    });
    ui.add_space(48.0);
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
    focusable: bool,
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
    // scrolls into view. A row being dragged is always drawn wherever it is, and so are the chosen
    // row — the arrows may have just moved the choice off screen, and it has to scroll itself back
    // — the one Tab stops on, and the one saved a moment ago, which asks to be scrolled into view.
    if !dragging && !is_open && !flashed && !focusable {
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
    let lit = show_row_inner(ui, state, list, row, visible, pass, is_open, focusable, previous_lit);
    if !dragging {
        let height = ui.cursor().top() - before;
        // `known_height` is this frame's snapshot, so on the one frame where the height actually
        // changes — a switch between the two densities, or entering the picking mode — every drawn
        // row writes it once instead of just the first.
        if height > 1.0 && pass.known_height != Some(height) {
            ui.ctx().data_mut(|d| {
                d.insert_temp(row_height_key(pass.compact, pass.select_mode), height)
            });
        }
    }
    lit
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
    focusable: bool,
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
    // The hover is not the row's to paint: see [`Glide`].
    let lit = is_selected || is_open || flash.is_some();

    // The rule between two rows, drawn rather than implied by a box around each. It stops at
    // anything tinted, so a highlighted row reads as one continuous shape.
    let under_glide = pass.glide.is_some_and(|band| band.contains(ui.cursor().top()));
    if !previous_lit && !lit && !under_glide {
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

    let mut bg = if is_open || is_selected {
        if hovered_row {
            crate::app::selection_hover_tint(is_light)
        } else {
            crate::app::selection_tint(is_light, accent_color)
        }
    } else {
        egui::Color32::TRANSPARENT
    };
    // A row saved a moment ago is lit over whatever else it would have been, and fades from there
    // back to the list's own colour. Mixed into an opaque colour rather than laid over the row as a
    // translucent wash, so the fade happens behind the words instead of across them.
    if let Some(amount) = flash {
        bg = crate::app::mix(
            crate::app::tint_base(is_light),
            accent_color,
            if is_light { 0.11 } else { 0.20 } * amount,
        );
    }

    let mut edit_clicked = false;
    let mut clicked_with: Option<(bool, bool)> = None;

    let framed = egui::Frame::default()
        .fill(bg)
        .corner_radius(controls::RADIUS_CONTROL)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // The horizontal wrapper is what keeps a row as tall as its content: without it the
            // frame is handed the whole remaining height of the list and stretches to fill it.
            let outer = ui.horizontal(|ui| {
                drag_source(ui, drag_id, t, drag_count, focusable, make_payload, |ui| {
                    closed_row(ui, row, pass, is_selected, is_light);
                    egui::Rect::NOTHING
                })
            });

            if let Some(resp) = &outer.inner {
                if resp.hovered() {
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(hovered_row_pending_key(), drag_id));
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                }
                // The chosen row keeps the arrows for the list: without the lock, egui would move
                // the keyboard to whatever widget lies above or below it.
                if resp.has_focus() {
                    ui.memory_mut(|m| {
                        m.set_focus_lock_filter(
                            drag_id,
                            egui::EventFilter { vertical_arrows: true, ..Default::default() },
                        )
                    });
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
                let by_enter = resp.clicked()
                    && resp.has_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if resp.double_clicked() || (by_enter && !pass.select_mode) {
                    edit_clicked = true;
                } else if resp.clicked() || (resp.drag_stopped() && !drag_is_deliberate(ui)) {
                    clicked_with = Some(ui.input(|i| (i.modifiers.command, i.modifiers.shift)));
                }
            }
        });

    // Under the pointer: tell the wash where to go. Not while something is being dragged — the
    // wash following a drag across the list would compete with the drop targets.
    if pass.dragged_id.is_none() && ui.rect_contains_pointer(framed.response.rect) {
        let rect = framed.response.rect;
        ui.ctx().data_mut(|d| d.insert_temp(glide_target_key(), rect));
    }

    // The accent's one appearance in the list: a short bar beside the chosen row.
    if is_open {
        controls::accent_bar(ui, framed.response.rect);
    }

    // Brought into view once, on the first frame after the save, or after the arrows moved the
    // choice. `None` for the alignment means "only as far as it takes to see it", so a row that
    // was on screen anyway does not jump.
    let scroll_flash =
        flash.is_some() && ui.ctx().data(|d| d.get_temp::<bool>(flash_scroll_key())) == Some(true);
    if scroll_flash {
        ui.scroll_to_rect(framed.response.rect, None);
        ui.ctx().data_mut(|d| d.insert_temp(flash_scroll_key(), false));
    }
    if is_open && ui.ctx().data_mut(|d| d.remove_temp::<bool>(scroll_to_open_key())).is_some() {
        ui.scroll_to_rect(framed.response.rect, None);
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
            set_open_row(ui.ctx(), if is_open { None } else { Some(&trigger) });
        }
        ui.ctx().request_repaint();
    }
    if edit_clicked {
        open_edit_for(state, &trigger);
    }

    lit
}

/// The row: a tick box when picking, the trigger in its column, and what it writes, quietly.
fn closed_row(ui: &mut egui::Ui, row: &ListRow, pass: RowPass, is_selected: bool, is_light: bool) {
    let pad_y = if pass.compact {
        controls::ROW_PAD_DENSE
    } else {
        controls::ROW_PAD_COMFORTABLE
    };

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

                let trigger_text = egui::RichText::new(&row.trigger)
                    .monospace()
                    .color(ui.visuals().strong_text_color());
                // A hard column, not a minimum: a trigger wider than the column is cut short here
                // and shown whole in the detail. Letting it push past instead would move the start
                // of the replacement from row to row.
                let spacing = ui.spacing().item_spacing.x;
                // Laid out and truncated the way a label would be, but painted by hand, a hair
                // below centre: centred, the smaller monospace stood a point above the
                // replacement. See [`controls::baseline_drop`].
                let trigger = egui::WidgetText::from(trigger_text.clone()).into_galley(
                    ui,
                    Some(egui::TextWrapMode::Truncate),
                    (pass.trigger_w - spacing).max(1.0),
                    egui::TextStyle::Body,
                );
                let (slot, hover) = ui.allocate_exact_size(trigger.size(), egui::Sense::hover());
                let drop = controls::baseline_drop(
                    ui,
                    egui::RichText::new("x"),
                    egui::RichText::new("x").monospace(),
                );
                let elided = trigger.elided;
                ui.painter().galley(
                    slot.left_top() + egui::vec2(0.0, drop),
                    trigger,
                    egui::Color32::PLACEHOLDER,
                );
                if elided {
                    hover.on_hover_text(trigger_text);
                }
                let padding = pass.trigger_w - slot.width() - spacing;
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
}

// ---------------------------------------------------------------------------------------------
// The inspector
// ---------------------------------------------------------------------------------------------

/// Room kept under the text box for what follows it: the gap, the folder's caption and line, the
/// gap, the row of actions, and the card's own bottom margin and edge.
const INSPECTOR_FOOT: f32 = controls::GAP_SECTION
    + 18.0
    + controls::GAP_TIGHT
    + 24.0
    + controls::GAP_SECTION
    + controls::FIELD_HEIGHT
    + controls::GAP_SECTION
    + 2.0;
/// What the sunken text box adds around its text: its margin above and below, and its edge.
const INSET_CHROME: f32 = 2.0 * 10.0 + 2.0;

/// Everything about one expansion: its whole trigger, its whole text, its folder, and what can be
/// done to it. Beside the list on a wide window (`side`), or as the whole screen on a narrow one,
/// with «Volver» at the top instead of the close button.
fn inspector(ui: &mut egui::Ui, state: &mut AppState, trigger: &str, side: bool, is_light: bool) {
    let t = state.t();
    let lang = state.settings.lang;
    let mut close = false;
    let mut edit = false;
    let mut move_it = false;
    let mut unfile = false;
    let mut delete = false;

    // The list's own preview is capped so a multi-kilobyte replacement never costs the list
    // anything. Here there is exactly one row to look up, so it can show the whole thing.
    let full = state
        .match_file
        .entries
        .iter()
        .find(|e| e.trigger_str() == trigger)
        .map(|e| e.preview(t));
    let folder = state.settings.folder_of(trigger).map(str::to_owned);
    let close_tip = keys::tip(
        studio::text(state, "Cerrar", "Close", "Isara", "बंद करें"),
        keys::BACK,
        lang,
    );

    let mut body = |ui: &mut egui::Ui| {
        ui.set_width(ui.available_width());
        if !side {
            let back = studio::text(state, "Volver", "Back", "Bumalik", "वापस");
            controls::quiet_row(ui, |ui| {
                let response = controls::subtle_button(ui, Some(Glyph::Back), back, None)
                    .on_hover_text(keys::tip(back, keys::BACK, lang));
                if response.clicked() {
                    close = true;
                }
            });
            ui.add_space(controls::GAP_WIDE);
        }

        // The trigger, whole: the list cuts a long one to keep its column straight, so this is the
        // one place it can be read in full. It wraps rather than truncates for that reason.
        ui.horizontal_top(|ui| {
            if side {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if controls::icon_button(ui, Glyph::Close, &close_tip, true).clicked() {
                        close = true;
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                        trigger_heading(ui, trigger);
                    });
                });
            } else {
                trigger_heading(ui, trigger);
            }
        });
        ui.add_space(controls::GAP_SECTION);

        let text = full.clone().unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(controls::small_muted(
                studio::text(state, "Texto", "Text", "Teksto", "टेक्स्ट"),
                is_light,
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let copy = studio::text(state, "Copiar texto", "Copy text", "Kopyahin ang teksto", "टेक्स्ट कॉपी करें");
                let copied = studio::text(state, "Copiado", "Copied", "Nakopya", "कॉपी हो गया");
                controls::copy_button(ui, egui::Id::new("inspector-copy").with(trigger), &text, copy, copied);
            });
        });
        ui.add_space(controls::GAP_TIGHT);
        let room = (ui.available_height() - INSPECTOR_FOOT - INSET_CHROME).max(3.0 * controls::FIELD_HEIGHT);
        controls::inset_frame(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            // Capped at a whole number of lines rather than at a round number of points: a box
            // whose height falls mid-glyph shows a sliced line at every scroll position except the
            // last one, which reads as damage rather than as "there is more below".
            let line = ui.text_style_height(&egui::TextStyle::Body);
            let cap = (room / line).floor().max(3.0) * line;
            egui::ScrollArea::vertical()
                .id_salt(("inspector-text", trigger))
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

        ui.add_space(controls::GAP_SECTION);
        ui.label(controls::small_muted(
            studio::text(state, "Carpeta", "Folder", "Folder", "फ़ोल्डर"),
            is_light,
        ));
        ui.add_space(controls::GAP_TIGHT);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = controls::GAP;
            glyphs::show(ui, Glyph::Folder, glyphs::SIZE, crate::app::secondary_text(is_light));
            let name = folder.as_deref().unwrap_or(t.no_folder);
            ui.add(egui::Label::new(truncate(name, 40)).truncate());
        });

        ui.add_space(controls::GAP_SECTION);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = controls::GAP;
            if controls::button(ui, t.edit_button, Tone::Primary, true)
                .on_hover_text(keys::tip(t.edit_button, keys::OPEN, lang))
                .clicked()
            {
                edit = true;
            }
            let more = controls::icon_button(
                ui,
                Glyph::More,
                studio::text(state, "Más acciones", "More actions", "Iba pang aksyon", "और क्रियाएँ"),
                true,
            );
            let delete_keys = keys::text(keys::DELETE, lang);
            let move_label = studio::text(
                state,
                "Mover a carpeta…",
                "Move to folder…",
                "Ilipat sa folder…",
                "फ़ोल्डर में ले जाएँ…",
            );
            let foldered = folder.is_some();
            let remove_label = t.remove_from_folder;
            let delete_label = t.delete;
            let by_keyboard = more.clicked() && more.has_focus();
            egui::Popup::menu(&more)
                .align(egui::RectAlign::BOTTOM_START)
                .gap(controls::GAP_TIGHT)
                .width(MENU_WIDTH)
                .show(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let first = ui.next_auto_id();
                    if controls::menu_item(ui, Some(Glyph::MoveToFolder), move_label, None, Tone::Normal, true)
                        .clicked()
                    {
                        move_it = true;
                    }
                    if by_keyboard {
                        ui.memory_mut(|m| m.request_focus(first));
                    }
                    if controls::menu_item(ui, None, remove_label, None, Tone::Normal, foldered).clicked() {
                        unfile = true;
                    }
                    controls::menu_separator(ui);
                    if controls::menu_item(ui, Some(Glyph::Delete), delete_label, Some(&delete_keys), Tone::Danger, true)
                        .clicked()
                    {
                        delete = true;
                    }
                });
        });
    };

    if side {
        let height = ui.available_height();
        controls::section_frame(is_light)
            .inner_margin(egui::Margin::same(controls::GAP_SECTION as i8))
            .show(ui, |ui| {
                ui.set_min_height(height - 2.0 * controls::GAP_SECTION - 2.0);
                body(ui);
            });
    } else {
        controls::page_scroll(ui, ("inspector-page", trigger), body);
    }

    if close {
        set_open_row(ui.ctx(), None);
    }
    if edit {
        open_edit_for(state, trigger);
    }
    if move_it {
        ask_to_move(ui.ctx(), vec![trigger.to_owned()]);
    }
    if unfile {
        state.assign_folder_to_triggers(&[trigger.to_owned()], None);
    }
    if delete {
        if let Some(index) = entry_index(state, trigger) {
            state.request_delete(index);
        }
    }
}

/// An expansion's trigger as the inspector's heading: monospaced, a step up from the list, and
/// wrapped whole however long it is.
fn trigger_heading(ui: &mut egui::Ui, trigger: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(trigger)
                .monospace()
                .size(18.0)
                .color(ui.visuals().strong_text_color()),
        )
        .wrap()
        .selectable(true),
    );
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
    focusable: bool,
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
        // Only the one row Tab stops on is focusable; the arrows walk the rest (see `show_list`).
        let sense = if focusable {
            egui::Sense::click_and_drag()
        } else {
            egui::Sense::CLICK | egui::Sense::DRAG
        };
        let response = ui.interact(interact_rect, id, sense);
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
    let new_folder = format!(
        "{}…",
        t.add_new_folder.trim_start_matches(['+', ' ']).trim_end_matches('…')
    );
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
        // One expansion is the one in the detail beside it; only a selection needs counting.
        let n = targets.len();
        if n > 1 {
            ui.add_space(controls::GAP);
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
        }
        ui.add_space(controls::GAP_STACK);

        // The same lines as the folder menu in the header, so a folder is picked the same way
        // wherever it is picked; the check marks where these already are, when they all agree.
        let here = {
            let mut places = targets.iter().map(|tr| state.settings.folder_of(tr));
            let first = places.next().flatten();
            places.all(|p| p == first).then_some(first)
        };
        egui::ScrollArea::vertical()
            .id_salt("library-move-list")
            .max_height(260.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                for folder in &folders {
                    let checked = here == Some(Some(folder.as_str()));
                    if controls::menu_check_item(ui, &truncate(folder, 32), None, checked).clicked() {
                        chosen = Some(Some(folder.clone()));
                    }
                }
                if controls::menu_check_item(ui, t.no_folder, None, here == Some(None)).clicked() {
                    chosen = Some(None);
                }
            });
        controls::menu_separator(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        if controls::menu_item(ui, Some(Glyph::Add), &new_folder, None, Tone::Normal, true).clicked() {
            ui.ctx()
                .data_mut(|d| d.insert_temp(create_folder_key(), String::new()));
            close = true;
        }

        ui.add_space(controls::GAP_STACK);
        controls::dialog_buttons(ui, |ui| {
            if controls::button(ui, t.cancel, Tone::Normal, true).clicked() {
                close = true;
            }
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

    // The button says what it does: nothing is being saved, a folder is being made.
    let create_label = studio::text(state, "Crear", "Create", "Gumawa", "बनाएँ");
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
            if controls::button(ui, create_label, Tone::Primary, ready).clicked() || (enter && ready) {
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

