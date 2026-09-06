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

//! The main screen: the list of expansions, its folders, and everything you can do by dragging.
//!
//! The longest file in the project, and four separate mechanisms account for most of it. Knowing
//! which one you are in is the difference between a five-line change and a puzzling one.
//!
//! **Virtualization.** A row far outside `ui.clip_rect()` is not drawn; it is replaced by a gap of
//! exactly its own height, measured once from a real row and cached per density. So anything asked
//! per row must be cheap *and* must be asked after that check — `Context::is_being_dragged` reads
//! like a cheap lookup and is an exclusive lock on the whole context, which is why the pass-wide
//! answers are hoisted into `RowPass` before the loop. Folders default to closed past
//! `FOLDERS_OPEN_BY_DEFAULT` for the same reason: laying out every row to show a list of names is
//! the expensive part in practice.
//!
//! **Hover.** Two slots, one frame apart. A row must know whether it is highlighted *before* it
//! draws, which is before it can learn whether the pointer is over it, so it reads last frame's
//! answer; the pending slot is what stops the highlight sticking to a row that has since been
//! scrolled away, filtered out or deleted.
//!
//! **Drag and drop.** Folders are assignments, not containers — see [`crate::settings`] — so a drop
//! is a call to `AppState::assign_folder_to_triggers` and nothing moves in `base.yml`.
//! `DRAG_START_DISTANCE` keeps an ordinary click from becoming a drag, and `drag_autoscroll` must
//! be called from *inside* the `ScrollArea` closure.
//!
//! **The two overlays**, `show_selection_bar` and `show_pending_confirm`, are drawn over the list
//! rather than in it, and take `&egui::Context` rather than a `Ui` for that reason.

use crate::app::{
    accent, folder_fill, hairline, truncate, AppState, DragPayload, ListCache, ListRow,
    PendingConfirmKind, View,
};
use crate::i18n::{fill, Strings};
use crate::ui::controls;
use crate::ui::edit_form::EditState;

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
// Glyph safety
// ---------------------------------------------------------------------------------------------

/// Picks the first glyph the *current* font actually has, falling back to plain text.
///
/// egui's proportional family is Ubuntu-Light + NotoEmoji + emoji-icon-font; `Hack` is only in the
/// monospace family. Symbols that live solely in Hack (U+25BE `▾`, for one) therefore render as an
/// empty box in an ordinary button — which is exactly the bug this exists to make impossible.
/// Rather than hand-picking glyphs and hoping, we ask the font at runtime.
///
/// The answer is cached in egui's own memory, keyed by the candidate list, so the font is queried
/// once per candidate set for the whole lifetime of the process rather than once per widget per
/// frame.
fn glyph(ctx: &egui::Context, candidates: &[char], fallback: &str) -> String {
    let key = egui::Id::new("glyph_cache").with(candidates);
    if let Some(cached) = ctx.data(|d| d.get_temp::<String>(key)) {
        return cached;
    }
    // Glyph coverage is a property of the font *family*, not the size, so any proportional FontId
    // answers the question for every proportional widget in the app.
    let font_id = egui::FontId::proportional(14.0);
    let found = candidates
        .iter()
        .copied()
        .find(|c| ctx.fonts_mut(|f| f.has_glyph(&font_id, *c)))
        .map(|c| c.to_string())
        .unwrap_or_else(|| fallback.to_string());
    ctx.data_mut(|d| d.insert_temp(key, found.clone()));
    found
}

/// "Show more" chevron. `⏷` lives in emoji-icon-font and `⬇` in NotoEmoji, both of which *are* in
/// the proportional family; the plain-text fallback keeps this legible even if neither resolves.
fn expand_glyph(ctx: &egui::Context) -> String {
    glyph(ctx, &['⏷', '⬇'], "+")
}

// ---------------------------------------------------------------------------------------------
// Drag auto-scroll
// ---------------------------------------------------------------------------------------------

/// Height of the band at each edge of the list that starts auto-scrolling, in points.
/// The gap between the two groups of buttons in the header: the primary action on one side, and
/// somewhere else to go on the other.
///
/// Wider than the app's ordinary 10-point gap, and deliberately. With the tips button welded flat
/// against "Ajustes" the space *inside* that group is zero, so the space *around* it has to be
/// unmistakably larger or the whole row reads as one undifferentiated lump. 16 is the next step up
/// in the same 4-point scale Windows lays its own interfaces out on.
const GROUP_GAP: f32 = 16.0;

/// Width reserved for the trigger in the compact view, so every preview starts on the same line.
const TRIGGER_COLUMN: f32 = 132.0;

/// A gap kept below the very last expansion. It is a drop target as much as breathing room: with
/// every row filed into folders there would otherwise be nowhere left to drop one to take it back
/// out. About the height of one compact row — enough to aim at, not enough to look like an
/// accident.
const TRAILING_DROP_HEIGHT: f32 = 38.0;

/// Above this many folders they all start closed. See `show_list` for why.
const FOLDERS_OPEN_BY_DEFAULT: usize = 5;

/// Where a folder's open/closed state lives.
///
/// Built from the folder name alone rather than with `Ui::make_persistent_id`, which mixes in the
/// id of whichever `Ui` happens to be drawing — here a frame's inner `Ui`, created inside a
/// closure and not addressable from anywhere else. [`visible_order`] has to read this state without
/// being inside that closure, and a folder name is already unique within the list.
fn folder_header_id(folder: &str) -> egui::Id {
    egui::Id::new(("folder_header", folder))
}

/// The clickable name and the count beside it that head a section — a folder's, or the ungrouped
/// one's — and the single place their appearance is decided.
///
/// `show_ungrouped` says in its own comment that it is "given the same header a folder has", and
/// that this is what lines its name up with the folder names above it. That was a hand copy, which
/// is the one thing that cannot keep such a promise: the two had to be edited together and nothing
/// said so. Now there is one of them.
///
/// Deliberately not in the accent, and with no folder pictogram. The accent belongs to the
/// triggers; using it here too made a folder name look like one more expansion. Quiet, small and
/// set apart by weight instead, a heading reads as a heading — and the icon turned out to be saying
/// what the indentation already says.
///
/// Sets `want_toggle` when the name itself is clicked, so the caller can open or close its own
/// `CollapsingState` afterwards; toggling from in here would be doing it mid-header.
fn section_title(
    ui: &mut egui::Ui,
    title: &str,
    count: usize,
    is_light: bool,
    want_toggle: &mut bool,
) {
    // Clicking the name itself opens/closes the section, matching what people expect from a file
    // explorer. Anything drawn after it is a separate widget and keeps its own clicks.
    let label = ui.add(
        egui::Label::new(
            egui::RichText::new(title)
                .strong()
                .color(crate::app::secondary_text(is_light)),
        )
        .sense(egui::Sense::click())
        .selectable(false),
    );
    if label.clicked() {
        *want_toggle = true;
    }
    if label.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    ui.label(
        egui::RichText::new(count.to_string())
            .small()
            .color(crate::app::text_tertiary(is_light)),
    );
}

/// Whether a folder's rows are on screen at this moment.
fn folder_is_open(ctx: &egui::Context, folder: &str, default_open: bool) -> bool {
    egui::collapsing_header::CollapsingState::load(ctx, folder_header_id(folder))
        .map_or(default_open, |state| state.is_open())
}

fn folders_default_open(list: &ListCache) -> bool {
    list.grouped.len() <= FOLDERS_OPEN_BY_DEFAULT
}

/// The two things every row would otherwise ask egui for on its own, read once for the whole pass.
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
}

fn row_pass(ui: &egui::Ui, compact: bool) -> RowPass {
    RowPass {
        dragged_id: ui.ctx().dragged_id(),
        known_height: ui
            .ctx()
            .data(|d| d.get_temp::<f32>(row_height_key(compact))),
    }
}

fn row_height_key(compact: bool) -> egui::Id {
    egui::Id::new("row_advance").with(compact)
}

/// The triggers a Shift-click range is allowed to run over: what is drawn, in the order it is
/// drawn.
///
/// A collapsed folder draws no rows at all, and a range that ran straight through them would tick
/// expansions nobody can see. That is not a cosmetic miscount: the bulk delete at least lists what
/// it is about to remove, but dragging the selection onto a folder re-files every one of them in a
/// single gesture and reports nothing but a total.
///
/// Built at the moment of the click rather than kept beside the list, because whether a folder is
/// open is egui's business rather than the model's — the list is rebuilt only when its *contents*
/// change, and folding one open changes none of them.
///
/// An anchor that has since been folded away is simply not in here, and a range with no anchor
/// falls back to selecting the one row that was clicked. Extending a selection from a row that is
/// no longer on screen has no meaning to offer.
fn visible_order(ctx: &egui::Context, list: &ListCache) -> Vec<String> {
    let default_open = folders_default_open(list);
    let mut order = Vec::with_capacity(list.rows.len());
    for (folder, indices) in &list.grouped {
        if folder_is_open(ctx, folder, default_open) {
            order.extend(indices.iter().map(|&i| list.rows[i].trigger.clone()));
        }
    }
    order.extend(list.ungrouped.iter().map(|&i| list.rows[i].trigger.clone()));
    order
}

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
    // over the buttons above the list never scrolls it.
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
    controls::segment_track(is_light)
        .show(ui, |ui| {
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
            crate::app::text_tertiary(is_light)
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
// List
// ---------------------------------------------------------------------------------------------

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let t = state.t();
    let mut open_new = false;
    let mut open_settings = false;
    let mut open_tips = false;
    let mut set_compact: Option<bool> = None;

    begin_hover_frame(ui.ctx());

    // The buttons are placed first and the title takes whatever is left, rather than the other way
    // round. Laid out title-first, the heading claims the width its text wants and the buttons are
    // then drawn on top of it — on a narrow window, or in a language with a longer title, the two
    // overlap and the title reads as a word cut in half. This way the title shortens instead, which
    // is the one of the two that can afford to.
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // "Ajustes" with a second, smaller control welded to its right end — the same shape the
            // date blocks use for their remove button, and for the same reason: the two belong
            // together, so they should read as one object with a control on its end rather than as
            // two destinations of equal weight. The small one carries no word, because a word next
            // to "Ajustes" would claim that weight back.
            //
            // Two real buttons rather than one hand-painted control: each keeps egui's own hover
            // and press feedback, and their adjoining flat edges make the divider by themselves.
            // Added right-to-left, so the tips half is pushed first to end up on the right.
            // egui charges the gap *after* placing each widget, so the space between this pair and
            // the primary button is decided by the spacing in force when "Ajustes" goes in — not by
            // anything set once both are already down. Setting it back at the end therefore did
            // nothing at all, and left the two groups a couple of pixels apart.
            let r = 4u8;
            let bulb = glyph(ui.ctx(), &['💡', '☀', 'ⓘ'], "?");
            ui.spacing_mut().item_spacing.x = 0.0;
            if ui
                .add(
                    egui::Button::new(&bulb)
                        .corner_radius(egui::CornerRadius { nw: 0, sw: 0, ne: r, se: r })
                        .min_size(egui::vec2(34.0, 0.0)),
                )
                .on_hover_text(t.tips_button_tip)
                .clicked()
            {
                open_tips = true;
            }
            ui.spacing_mut().item_spacing.x = GROUP_GAP;
            if ui
                .add(
                    egui::Button::new(t.settings_button)
                        .corner_radius(egui::CornerRadius { nw: r, sw: r, ne: 0, se: 0 }),
                )
                .clicked()
            {
                open_settings = true;
            }
            // The one filled button on the screen. With the boxes gone from the rows, the accent
            // has room to mean something again: this is the thing you came here to do.
            if controls::primary_button(ui, &format!("➕ {}", t.new_expansion)).clicked() {
                open_new = true;
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(t.app_title).heading())
                        .truncate()
                        .selectable(false),
                );
            });
        });
    });

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        // The label lives inside the box. A caption outside it named what was already obvious
        // from the box being empty, and cost a line of width on every screen size.
        ui.add(
            controls::text_field(&mut state.search)
                .desired_width(280.0)
                .hint_text(t.search_label),
        );
        if !state.search.is_empty() && ui.small_button("❌").clicked() {
            state.search.clear();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            set_compact = view_switch(ui, state.settings.compact_view, t);
        });
    });
    ui.add_space(2.0);
    ui.separator();

    if let Some(compact) = set_compact {
        state.set_compact_view(compact);
    }

    // Filtering, grouping and preview rendering all happen inside this call, and only when
    // something has actually changed since the last frame — see `AppState::list`. The handle is an
    // `Rc`, so holding it here costs nothing and leaves `state` free to be mutated by the rows
    // below (selecting, opening the editor, deleting, ...).
    let list = state.list();

    // A folder's contents cost nothing to render while collapsed (egui skips the body entirely),
    // so once there are more than a handful of folders we default them closed — otherwise a
    // large, fully-foldered collection would lay out every single row on every repaint just to
    // show a list of folder names, which is the expensive part in practice, not the row count
    // itself.
    let default_open = folders_default_open(&list);
    let pass = row_pass(ui, state.settings.compact_view);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            drag_autoscroll(ui);

            for (folder, indices) in &list.grouped {
                show_folder(ui, state, &list, folder, indices, default_open, pass);
                ui.add_space(4.0);
            }

            if !list.ungrouped.is_empty() {
                if !list.grouped.is_empty() {
                    ui.add_space(2.0);
                }
                // Titled only when there is at least one folder above it. With no folders at all
                // every expansion is unfiled, and a heading saying so would name the obvious.
                show_ungrouped(ui, state, &list, &list.ungrouped, !list.grouped.is_empty(), pass);
            }

            // Breathing room below the last row that doubles as somewhere to drop. Without it, a
            // collection where every expansion is filed away leaves no bare list left to drag one
            // back onto.
            if !list.rows.is_empty() {
                let is_light = !ui.visuals().dark_mode;
                let accent_color = accent(ui.visuals());
                if let Some(payload) = trailing_drop_zone(ui, accent_color, is_light) {
                    state.assign_folder_to_triggers(&payload, None);
                }
            }

            if list.total_entries == 0 {
                ui.add_space(24.0);
                ui.vertical_centered(|ui| {
                    ui.weak(t.empty_title);
                    ui.weak(t.empty_hint);
                });
            } else if list.rows.is_empty() {
                ui.add_space(24.0);
                ui.vertical_centered(|ui| {
                    ui.weak(t.no_matches);
                });
            }
        });

    end_hover_frame(ui.ctx());

    if open_new {
        state.view = View::Edit(EditState::new_for_create(&state.settings.prefix));
    }
    if open_settings {
        state.view = View::Settings;
        state.refresh_autostart_cache();
        state.ensure_espanso_version();
    }
    if open_tips {
        state.view = View::Tips;
    }
}

fn open_edit_for(state: &mut AppState, trigger: &str) {
    let Some(index) = state
        .match_file
        .entries
        .iter()
        .position(|e| e.trigger_str() == trigger)
    else {
        return;
    };
    if let crate::yaml::model::MatchEntry::Simple(m) = &state.match_file.entries[index] {
        let folder = state.settings.folder_of(&m.trigger).map(str::to_string);
        state.view = View::Edit(EditState::new_for_edit(index, m, folder));
    }
}

fn delete_by_trigger(state: &mut AppState, trigger: &str) {
    if let Some(index) = state
        .match_file
        .entries
        .iter()
        .position(|e| e.trigger_str() == trigger)
    {
        state.request_delete(index);
    }
}

const DROP_ZONE_RADIUS: u8 = 9;

/// What a place-a-row-can-go looks like: nothing at all until a drag starts, a faint outline while
/// one is in flight so every possible destination is visible at once, and the accent for the one
/// under the pointer.
///
/// There are two drop zones and they are the same object seen twice — one wraps a folder, the other
/// is the bare strip under the last row — but they were painted from two separate copies of the
/// same four numbers. The interface is finished and signed off, so the only thing those copies
/// could still do was drift apart the first time one of them was touched.
fn drop_zone_look(
    accent: egui::Color32,
    is_light: bool,
    can_accept: bool,
    over_it: bool,
) -> (egui::Color32, egui::Stroke) {
    let fill = if over_it {
        crate::app::mix(crate::app::win_background_for(is_light), accent, 0.12)
    } else {
        egui::Color32::TRANSPARENT
    };
    let stroke = if over_it {
        egui::Stroke::new(1.5, accent)
    } else if can_accept {
        egui::Stroke::new(1.0, hairline(is_light))
    } else {
        egui::Stroke::NONE
    };
    (fill, stroke)
}

/// The empty strip below the last expansion. Dropping there takes a row out of its folder.
fn trailing_drop_zone(
    ui: &mut egui::Ui,
    accent: egui::Color32,
    is_light: bool,
) -> Option<std::sync::Arc<DragPayload>> {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), TRAILING_DROP_HEIGHT),
        egui::Sense::hover(),
    );

    let can_accept = egui::DragAndDrop::has_payload_of_type::<DragPayload>(ui.ctx());
    if can_accept && ui.is_rect_visible(rect) {
        // `can_accept` is true for the whole of this branch, which is what makes the strip appear
        // at all: it is never drawn in its no-drag state.
        let (fill, stroke) = drop_zone_look(accent, is_light, true, response.contains_pointer());
        let inner = rect.shrink2(egui::vec2(6.0, 4.0));
        let radius = egui::CornerRadius::same(DROP_ZONE_RADIUS);
        ui.painter().rect_filled(inner, radius, fill);
        ui.painter()
            .rect_stroke(inner, radius, stroke, egui::StrokeKind::Inside);
    }

    response.dnd_release_payload::<DragPayload>()
}

/// A drop target that is only visible while there is something to drop on it.
///
/// This is `Ui::dnd_drop_zone` with one behaviour removed: that method discards the fill and stroke
/// of the frame it is given and substitutes the stock widget colours, which in a design built on
/// removing boxes puts one back around every folder, permanently.
fn folder_drop_zone<R>(
    ui: &mut egui::Ui,
    accent: egui::Color32,
    is_light: bool,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<std::sync::Arc<DragPayload>> {
    let mut frame = egui::Frame::default()
        .corner_radius(DROP_ZONE_RADIUS)
        .inner_margin(egui::Margin::symmetric(6, 6))
        .begin(ui);

    add_contents(&mut frame.content_ui);
    let response = frame.allocate_space(ui);

    let can_accept = egui::DragAndDrop::has_payload_of_type::<DragPayload>(ui.ctx());
    let over_it = can_accept && response.contains_pointer();
    let (fill, stroke) = drop_zone_look(accent, is_light, can_accept, over_it);
    frame.frame.fill = fill;
    frame.frame.stroke = stroke;
    frame.paint(ui);

    response.dnd_release_payload::<DragPayload>()
}

fn show_folder(
    ui: &mut egui::Ui,
    state: &mut AppState,
    list: &ListCache,
    folder: &str,
    indices: &[usize],
    default_open: bool,
    pass: RowPass,
) {
    let t = state.t();
    let is_light = !ui.visuals().dark_mode;
    let accent = accent(ui.visuals());

    // A folder is no longer a card. It is a heading with a count, and the rows belong to it because
    // they sit under it — which is how a folder reads on paper and in every file list.
    //
    // `Ui::dnd_drop_zone` cannot be used for that: it takes the frame you hand it and then
    // overwrites its fill and stroke with the stock widget colours, so a drop zone is always a
    // visible box whether you asked for one or not. The few lines it wraps are reproduced here so
    // the box can appear only while something is actually being dragged — obvious at the moment it
    // matters, invisible the rest of the time.
    let dropped = folder_drop_zone(ui, accent, is_light, |ui| {
        let is_renaming = state
            .renaming_folder
            .as_ref()
            .is_some_and(|(orig, _)| orig == folder);

        let collapsing_id = folder_header_id(folder);
        let mut want_toggle = false;
        let mut header = egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            collapsing_id,
            default_open,
        )
        .show_header(ui, |ui| {
            if is_renaming {
                let buf = &mut state.renaming_folder.as_mut().unwrap().1;
                let resp = ui.add(controls::text_field(buf).desired_width(200.0));
                let commit = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.small_button("✔").on_hover_text(t.save_name_tip).clicked() || commit {
                    state.commit_rename_folder();
                }
                if ui.small_button("❌").on_hover_text(t.cancel_tip).clicked() {
                    state.cancel_rename_folder();
                }
            } else {
                section_title(ui, folder, indices.len(), is_light, &mut want_toggle);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("🗑").on_hover_text(t.delete_folder_tip).clicked() {
                        state.request_delete_folder(folder);
                    }
                    if ui
                        .small_button("✏")
                        .on_hover_text(t.rename_folder_tip)
                        .clicked()
                    {
                        state.start_rename_folder(folder);
                    }
                });
            }
        });

        if want_toggle {
            header.toggle();
        }

        header.body(|ui| {
            ui.add_space(2.0);
            for &index in indices {
                show_row(ui, state, list, &list.rows[index], pass);
            }
        });
    });

    if let Some(payload) = dropped {
        state.assign_folder_to_triggers(&payload, Some(folder.to_string()));
    }
}

/// The rows that belong to no folder, and the target for dropping one back out of a folder.
///
/// The zone keeps a minimum height even when empty, so there is always somewhere to drop even when
/// every expansion happens to be filed away.
fn show_ungrouped(
    ui: &mut egui::Ui,
    state: &mut AppState,
    list: &ListCache,
    indices: &[usize],
    titled: bool,
    pass: RowPass,
) {
    let t = state.t();
    let is_light = !ui.visuals().dark_mode;
    let accent_color = accent(ui.visuals());
    // Same hand-rolled drop zone as a folder. egui's own paints the stock control fill behind
    // whatever it wraps, which is why the expansions outside every folder used to sit on a lighter
    // grey slab than the rest of the window.
    let dropped = folder_drop_zone(ui, accent_color, is_light, |ui| {
        ui.set_min_width(ui.available_width());
        if !titled {
            for &index in indices {
                show_row(ui, state, list, &list.rows[index], pass);
            }
            return;
        }

        // Given the same header a folder has - arrow, name, count - rather than a bare line of
        // text. Under a folder's contents these rows used to start with no announcement of any
        // kind, so the eye carried on reading them as more of that folder. Reusing the folder
        // header is also what lines the name up with the folder names above it: the arrow is what
        // sets that indent, so a heading without one would sit slightly to the left of every
        // folder and look like a mistake.
        let id = ui.make_persistent_id("ungrouped_header");
        let mut want_toggle = false;
        let mut header = egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            id,
            true,
        )
        .show_header(ui, |ui| {
            section_title(ui, t.no_folder, indices.len(), is_light, &mut want_toggle);
        });
        if want_toggle {
            header.toggle();
        }
        header.body(|ui| {
            ui.add_space(2.0);
            for &index in indices {
                show_row(ui, state, list, &list.rows[index], pass);
            }
        });
    });
    if let Some(payload) = dropped {
        state.assign_folder_to_triggers(&payload, None);
    }
}

/// The drag handle's id for a row, used both to drive the drag and to tint the hovered row.
fn row_id(trigger: &str) -> egui::Id {
    egui::Id::new("expansion_row").with(trigger)
}

fn show_row(
    ui: &mut egui::Ui,
    state: &mut AppState,
    list: &ListCache,
    row: &ListRow,
    pass: RowPass,
) {
    let compact = state.settings.compact_view;
    let id = row_id(&row.trigger);
    let dragging = pass.dragged_id == Some(id);

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
    // scrolls into view, and a row being dragged is always drawn wherever it is.
    let known_height = pass.known_height;
    if !dragging {
        if let Some(height) = known_height {
            let top = ui.cursor().top();
            let view = ui.clip_rect();
            let band = height * 3.0;
            if top + height < view.top() - band || top > view.bottom() + band {
                ui.add_space(height);
                return;
            }
        }
    }

    let before = ui.cursor().top();
    show_row_inner(ui, state, list, row, compact);
    if !dragging {
        let height = ui.cursor().top() - before;
        // `known_height` is this frame's snapshot, so on the one frame where the height actually
        // changes — a switch between the two densities — every drawn row writes it once instead of
        // just the first. Fifteen writes, once, against the few thousand reads a frame this
        // replaced.
        if height > 1.0 && known_height != Some(height) {
            ui.ctx()
                .data_mut(|d| d.insert_temp(row_height_key(compact), height));
        }
    }
}

fn show_row_inner(
    ui: &mut egui::Ui,
    state: &mut AppState,
    list: &ListCache,
    row: &ListRow,
    compact: bool,
) {
    let t = state.t();
    let trigger = row.trigger.as_str();
    let is_selected = state.selected.contains(trigger);
    // How many expansions a drag starting here would carry. The payload itself is only assembled
    // if a drag actually begins: building it eagerly meant copying the whole selection once for
    // every selected row, on every frame — quadratic work for something nobody had asked for yet.
    let drag_count = if is_selected {
        state.selected.len()
    } else {
        1
    };
    let drag_id = row_id(trigger);

    let is_light = !ui.visuals().dark_mode;
    let accent_color = accent(ui.visuals());

    let mut delete_clicked = false;
    let mut edit_clicked = false;

    let hovered_row =
        ui.ctx().data(|d| d.get_temp::<egui::Id>(hovered_row_key())) == Some(drag_id);

    let make_payload = || -> DragPayload {
        if is_selected {
            state.selected.iter().cloned().collect()
        } else {
            vec![trigger.to_string()]
        }
    };

    // The horizontal wrapper is what keeps a row as tall as its content: without it the frame is
    // handed the whole remaining height of the list and stretches to fill it.
    let outer = ui
        .horizontal(|ui| {
            drag_source(
            ui,
            drag_id,
            t,
            drag_count,
            make_payload,
            |ui| {
                // No box around the row any more. A list where every item wears its own border,
                // inside a folder that wears another one, spends four strokes saying what a little
                // space and one hairline say better. Unselected rows now sit straight on the window
                // background; only the row you are pointing at, or the ones you have chosen, are
                // painted at all.
                let bg = if is_selected {
                    crate::app::selection_tint(is_light, accent_color)
                } else if hovered_row {
                    crate::app::hover_tint(is_light)
                } else {
                    egui::Color32::TRANSPARENT
                };

                egui::Frame::default()
                    .fill(bg)
                    .corner_radius(if is_selected || hovered_row { 6u8 } else { 0u8 })
                    .inner_margin(if compact {
                        egui::Margin::symmetric(12, 6)
                    } else {
                        egui::Margin::symmetric(14, 9)
                    })
                    .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    // The buttons are placed first inside a right-to-left layout so they pin to the
                    // right edge; the label column then goes in a nested left-to-right layout,
                    // which receives exactly the width that's left over. That's what lets the
                    // preview truncate to the real available width instead of a guessed character
                    // count — and it keeps the row a fixed height no matter how long the text is.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mut buttons = egui::Rect::NOTHING;

                        let (del, edit) = if compact {
                            let d = ui.small_button("🗑").on_hover_text(t.delete_tip);
                            let e = ui.small_button("✏").on_hover_text(t.edit_tip);
                            (d, e)
                        } else {
                            let d = ui.button("🗑").on_hover_text(t.delete_tip);
                            let e = ui.button(t.edit_button);
                            (d, e)
                        };
                        if del.clicked() {
                            delete_clicked = true;
                        }
                        if edit.clicked() {
                            edit_clicked = true;
                        }
                        buttons = buttons.union(del.rect).union(edit.rect);

                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let trigger_text = egui::RichText::new(&row.trigger)
                                .strong()
                                .monospace()
                                .color(accent_color);
                            if compact {
                                // The trigger gets a fixed column rather than only the width it
                                // happens to need, so every preview in the list starts at the same
                                // x. Ragged starts are what made the compact view feel unsettled;
                                // one invisible line down the page is most of what makes it look
                                // composed. A longer trigger truncates rather than pushing its own
                                // preview out of line with everyone else's.
                                // A *minimum* column, not a fixed one: the trigger is padded out to
                                // the column width so the previews line up, but a long trigger is
                                // allowed to push past it rather than being cut short. The name of
                                // the thing is what you are scanning for; the preview can move.
                                //
                                // Both labels stay direct siblings in the same row layout, which is
                                // what keeps them on one baseline — nesting the trigger inside its
                                // own sub-region centred it separately and left it sitting a few
                                // pixels below its own preview.
                                let placed = ui.add(
                                    egui::Label::new(trigger_text).truncate().selectable(false),
                                );
                                let padding =
                                    TRIGGER_COLUMN - placed.rect.width() - ui.spacing().item_spacing.x;
                                if padding > 0.0 {
                                    ui.add_space(padding);
                                }
                                ui.add(
                                    egui::Label::new(egui::RichText::new(&row.preview).weak())
                                        .truncate()
                                        .selectable(false),
                                );
                            } else {
                                ui.vertical(|ui| {
                                    ui.add(
                                        egui::Label::new(trigger_text)
                                            .truncate()
                                            .selectable(false),
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&row.preview).weak(),
                                        )
                                        .truncate()
                                        .selectable(false),
                                    );
                                });
                            }
                        });

                        buttons
                    })
                    .inner
                    })
                    .inner
                },
            )
        });
    let row_result = outer.inner;
    let row_rect = outer.response.rect;

    // The one stroke that survived: a hairline between rows, drawn rather than implied by a box.
    // It stops at the row the pointer is on and at anything selected, so a highlighted row reads as
    // a single continuous shape instead of a shape with a line through its bottom edge.
    if !is_selected && !hovered_row {
        let y = row_rect.bottom() + (if compact { 2.0 } else { 3.0 });
        ui.painter().hline(
            row_rect.x_range(),
            y,
            egui::Stroke::new(1.0, hairline(is_light)),
        );
    }

    // A bar down the left edge of anything selected. The tint alone reads as "hovered" at a glance;
    // the bar is what makes a selection unmistakable even in a list of tinted rows.
    if is_selected {
        ui.painter().rect_filled(
            egui::Rect::from_min_max(
                row_rect.min,
                egui::pos2(row_rect.left() + 3.0, row_rect.bottom()),
            ),
            egui::CornerRadius::same(2),
            accent_color,
        );
    }

    ui.add_space(if compact { 4.0 } else { 7.0 });

    // Track which row the pointer is over so the next frame can tint it, without giving the row a
    // hover sense of its own that would compete with the buttons inside it.
    if let Some(resp) = &row_result {
        if resp.hovered() {
            ui.ctx()
                .data_mut(|d| d.insert_temp(hovered_row_pending_key(), drag_id));
            ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
        }
        // `clicked()` on its own is not enough. egui stops calling a press a click once the pointer
        // has drifted more than six points, or once the button has been held past 0.8 s — and
        // neither of those makes it a drag *here*, where a drag has to clear eighteen. Between the
        // two thresholds sits a band that used to do nothing whatsoever: a click with an unsteady
        // hand, and a click held still while reading the row it is aimed at. Both are clicks.
        //
        // The second arm has to be qualified, because a genuine drag ends here too: egui forgets
        // the drag on the release frame, so the row is drawn as an ordinary row again and reports
        // `drag_stopped` exactly like a wobble does. The latch is what still knows the difference,
        // and without it dropping a whole selection into a folder would collapse it to the one row
        // that happened to be grabbed.
        if resp.clicked() || (resp.drag_stopped() && !drag_is_deliberate(ui)) {
            let (ctrl, shift) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));
            // Assembled here rather than once a frame: a range is only ever needed on the frame a
            // row is actually clicked, which is at most one row in the whole list.
            let order = visible_order(ui.ctx(), list);
            state.click_select(trigger, ctrl, shift, &order);
        }
    }

    if delete_clicked {
        delete_by_trigger(state, trigger);
    }
    if edit_clicked {
        open_edit_for(state, trigger);
    }
}

/// Draws a row and makes its body both clickable (to select) and draggable (to re-file).
///
/// `add_contents` returns the screen rect of the row's own buttons; that region is excluded from
/// the row's interactive area. egui resolves overlapping widgets by taking the topmost, and this
/// interaction is registered *after* the buttons, so without the exclusion the row would swallow
/// their clicks entirely.
///
/// While a drag is in progress the row is redrawn on a floating layer that follows the pointer —
/// as a compact badge when several expansions are moving together, so the preview reflects the
/// whole selection rather than just the row that happened to be grabbed.
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

/// The "remove from folder" button, revealed by opening up the space it needs rather than by
/// appearing at full size.
///
/// The space reserved for it grows from nothing to the button's natural width over the same eased
/// fraction that fades it in, and the button is drawn inside that space clipped to it — so it looks
/// like it slides out from behind its neighbour while the bar stretches to make room. Because the
/// row is sized from what it measures, animating this one width is what animates the whole bar; no
/// separate animation is needed, and none of it costs more than one extra rectangle.
fn show_unfile_button(
    ui: &mut egui::Ui,
    t: &'static Strings,
    shown: f32,
    unfile_t: f32,
    row_height: f32,
    remove_from_folder: &mut bool,
) {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let natural = ui.ctx().fonts_mut(|f| {
        f.layout_no_wrap(t.remove_from_folder.to_owned(), font, egui::Color32::WHITE)
            .rect
            .width()
    }) + ui.spacing().button_padding.x * 2.0;

    let (slot, _) = ui.allocate_exact_size(
        egui::vec2(natural * unfile_t, row_height),
        egui::Sense::hover(),
    );
    let clip = slot.intersect(ui.clip_rect());
    let mut button_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_size(
                slot.min,
                egui::vec2(natural, slot.height()),
            ))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    button_ui.set_clip_rect(clip);
    button_ui.set_opacity(shown * unfile_t);
    // Only accepted once it is essentially fully out, so a click aimed at the button beside it can
    // never land on one that is still sliding into place.
    if button_ui.button(t.remove_from_folder).clicked() && unfile_t > 0.9 {
        *remove_from_folder = true;
    }
}

/// Floating bar shown at the bottom-center of the window while one or more expansions are
/// checked, offering bulk actions. Deliberately styled to stand out (bright border, dark fill)
/// so it reads as a temporary, contextual control rather than a permanent part of the interface.
pub fn show_selection_bar(ctx: &egui::Context, state: &mut AppState) {
    let t = state.t();
    let has_selection = !state.selected.is_empty();
    let shown = ctx.animate_bool_with_time(egui::Id::new("selection_bar_vis"), has_selection, 0.12);
    if shown <= 0.0 {
        return;
    }

    let mut remove_from_folder = false;
    let mut delete_selected = false;
    let mut clear = false;
    let count = state.selected.len();

    // The "quitar de su carpeta" action only means something when the selection actually contains
    // filed-away expansions, so it fades in and out with that condition instead of sitting there
    // permanently doing nothing.
    let has_foldered = state.selection_has_foldered();
    let unfile_t =
        ctx.animate_bool_with_time(egui::Id::new("selection_bar_unfile"), has_foldered, 0.15);

    egui::Area::new(egui::Id::new("selection_bar"))
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -18.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_opacity(shown);
            // Built from the user's own Windows accent rather than a fixed blue, so it belongs to
            // the same window as the expansion names above it. The surface is the ordinary card
            // colour pulled a little way towards the accent — enough to read as a distinct,
            // temporary control, not so much that it turns into a coloured slab — and the accent
            // itself is kept for the outline, which is what actually makes it stand off the list.
            let is_light = !ui.visuals().dark_mode;
            let accent_color = accent(ui.visuals());
            let surface = crate::app::mix(
                folder_fill(is_light),
                accent_color,
                if is_light { 0.14 } else { 0.22 },
            );
            let text_color = ui.visuals().text_color();
            egui::Frame::popup(ui.style())
                .fill(surface)
                .stroke(egui::Stroke::new(1.5, accent_color))
                .corner_radius(12u8)
                .inner_margin(egui::Margin::symmetric(16, 10))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                    // `Ui::horizontal` starts by claiming the *whole* width the parent can offer —
                    // inside a floating area that is most of the screen — so a row built with it
                    // made the bar far wider than anything in it. The row is therefore given an
                    // explicit width instead: what it measured last frame. That single number is
                    // what makes the bar exactly as wide as its contents, and it is also what makes
                    // the bar stretch smoothly, because the width it follows changes smoothly.
                    let row_width_id = egui::Id::new("selection_bar_row_width");
                    let row_width = ui
                        .ctx()
                        .data(|d| d.get_temp::<f32>(row_width_id))
                        .unwrap_or(0.0);
                    let row_height = ui.spacing().interact_size.y;
                    let gap = ui.spacing().item_spacing.x;

                    let measured = ui
                        .allocate_ui_with_layout(
                            egui::vec2(row_width, row_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                let row_start = ui.cursor().left();
                                ui.colored_label(
                                    text_color,
                                    egui::RichText::new(fill(
                                        t.selected_count,
                                        &[("n", &count.to_string())],
                                    ))
                                    .strong(),
                                );
                                ui.separator();

                                if unfile_t > 0.0 {
                                    show_unfile_button(
                                        ui,
                                        t,
                                        shown,
                                        unfile_t,
                                        row_height,
                                        &mut remove_from_folder,
                                    );
                                }

                                if ui
                                    .button(
                                        egui::RichText::new(t.delete_selected)
                                            .color(crate::app::danger(is_light)),
                                    )
                                    .clicked()
                                {
                                    delete_selected = true;
                                }
                                if ui
                                    .small_button("❌")
                                    .on_hover_text(t.clear_selection_tip)
                                    .clicked()
                                {
                                    clear = true;
                                }
                                // The cursor sits one gap past the last widget, so take that back.
                                ui.cursor().left() - row_start - gap
                            },
                        )
                        .inner;
                    if measured > 1.0 && (measured - row_width).abs() > 0.5 {
                        ui.ctx().data_mut(|d| d.insert_temp(row_width_id, measured));
                    }

                    // The Ctrl/Shift hint used to live here. It reappeared every single time a row
                    // was selected, long after anyone had learned it, and it was the only reason
                    // this bar was two lines tall. It is a tip, so it moved to the tips window.
                    });
                });
        });

    if remove_from_folder {
        let triggers: Vec<String> = state
            .selected
            .iter()
            .filter(|t| state.settings.folder_of(t).is_some())
            .cloned()
            .collect();
        state.assign_folder_to_triggers(&triggers, None);
    }
    if delete_selected {
        state.request_delete_selected();
    }
    if clear {
        state.clear_selection();
    }
}

fn item_line(ui: &mut egui::Ui, trigger: &str, preview: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(trigger).strong().monospace());
        ui.label(
            egui::RichText::new(truncate(preview, 28))
                .color(egui::Color32::from_rgb(120, 175, 235))
                .italics(),
        );
    });
}

fn item_chip(ui: &mut egui::Ui, trigger: &str, preview: &str) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_width(150.0);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(trigger).strong().monospace().small());
                ui.label(
                    egui::RichText::new(truncate(preview, 20))
                        .color(egui::Color32::from_rgb(120, 175, 235))
                        .small(),
                );
            });
        });
}


/// In-app modal confirming a destructive bulk action (deleting a folder or a multi-selection).
/// Uses a native message box for a single item elsewhere in the app, but this one needs to show a
/// variable-length list of affected expansions without ever growing into a giant window — capped
/// at a fixed width, with a "ver más" toggle for medium-sized lists and a horizontally scrolling
/// strip of compact chips for large ones.
pub fn show_pending_confirm(ctx: &egui::Context, state: &mut AppState) {
    let Some(pending) = &state.pending_confirm else {
        return;
    };

    let t = state.settings.t();
    let count = pending.items.len();
    let n = count.to_string();
    let title = match &pending.kind {
        PendingConfirmKind::DeleteFolder { folder } => {
            fill(t.confirm_delete_folder_title, &[("name", folder)])
        }
        PendingConfirmKind::DeleteSelection => {
            fill(t.confirm_delete_selection_title, &[("n", &n)])
        }
    };
    let intro = match &pending.kind {
        PendingConfirmKind::DeleteFolder { folder } => fill(
            t.confirm_delete_folder_body,
            &[("name", folder), ("n", &n)],
        ),
        PendingConfirmKind::DeleteSelection => {
            fill(t.confirm_delete_selection_body, &[("n", &n)])
        }
    };
    let expanded = pending.expanded;

    let mut confirm = false;
    let mut cancel = false;
    let mut expand = false;

    let modal = egui::Modal::new(egui::Id::new("pending_confirm")).show(ctx, |ui| {
        ui.set_max_width(460.0);
        ui.heading(title);
        ui.add_space(6.0);
        ui.label(intro);
        ui.add_space(10.0);

        if count <= 5 {
            for (trigger, preview) in &pending.items {
                item_line(ui, trigger, preview);
            }
        } else if count < 10 {
            let shown = if expanded { count } else { 5 };
            for (trigger, preview) in pending.items.iter().take(shown) {
                item_line(ui, trigger, preview);
            }
            if !expanded {
                let label = format!(
                    "{} {}",
                    expand_glyph(ui.ctx()),
                    fill(t.see_more, &[("n", &(count - 5).to_string())])
                );
                if ui.small_button(label).clicked() {
                    expand = true;
                }
            }
        } else {
            egui::ScrollArea::horizontal()
                .max_height(76.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (trigger, preview) in &pending.items {
                            item_chip(ui, trigger, preview);
                        }
                    });
                });
        }

        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if ui
                .button(
                    egui::RichText::new(t.delete)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .clicked()
            {
                confirm = true;
            }
            if ui.button(t.cancel).clicked() {
                cancel = true;
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
