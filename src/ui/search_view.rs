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

//! The search window: Alt+Space anywhere in Windows, pick an expansion, and it lands where the
//! cursor was.
//!
//! Espanso's own version of this is what it replaces, and the division of labour is the point.
//! Everything visible here is ours — the type ramp, the accent, the row layout, the way a selected
//! row is lit — while the insertion itself is handed back to espanso through `match exec`. Putting
//! text into Word, Teams, a terminal and an Electron app, with the clipboard restored afterwards,
//! is the hard part of a text expander, and it is already solved.
//!
//! Keyboard first, because that is how this is used: it opens with the field focused, Up and Down
//! move, Enter picks, Esc closes. The mouse works too, but nobody reaching for Alt+Space wants it.
//!
//! ## Why the list scrolls instead of stopping at eight
//!
//! The window is a glance, not a page, so it is a fixed eight rows tall. That used to be done by
//! throwing away every result past the eighth, which quietly hid expansions from a search that had
//! genuinely found them. Now every match is kept and the list scrolls through the window — with the
//! arrows, with the wheel, and with the selection dragging the view along behind it, which is how
//! espanso's own bar behaves.

use crate::app::{accent, hairline, secondary_text, text_tertiary, AppState};

/// An upper bound on how many matches are held at once. High enough that no real search ever meets
/// it, low enough that one keystroke can never turn into thousands of rows.
const MAX_RESULTS: usize = 200;

/// How many rows the window is tall. Everything past this is reached by scrolling.
pub const VISIBLE_ROWS: usize = 8;

/// One row's height, and therefore the pitch of the list: rows are drawn with no spacing between
/// them so that this number alone decides how tall the window has to be.
pub const ROW_HEIGHT: f32 = 34.0;

/// Wide enough for a trigger and a useful slice of what it expands to.
pub const WINDOW_WIDTH: f32 = 560.0;

const PAD: f32 = 12.0;
const GAP_ABOVE_LIST: f32 = 8.0;
const GAP_BELOW_LIST: f32 = 6.0;
const GAP_ABOVE_EMPTY: f32 = 10.0;

/// Breathing room at both ends of a row. The right-hand one is what keeps a long preview from
/// running into the window's border instead of ending somewhere deliberate.
const ROW_INSET: f32 = 12.0;

/// Width reserved for the trigger, so every preview in the list starts on the same vertical line —
/// the same column rule the main window follows.
const TRIGGER_COLUMN: f32 = 132.0;

/// Kept clear between the trigger and the preview, so a long trigger ends with an ellipsis rather
/// than butting up against the text beside it.
const COLUMN_GAP: f32 = 8.0;

pub struct Hit {
    pub trigger: String,
    pub preview: String,
}

/// The expansions matching the query, best first.
///
/// Matches on the trigger *and* on the replacement text, because half the time you remember what
/// the expansion says rather than what you named it. A trigger match sorts first: if you typed
/// something that looks like a trigger, that is almost certainly what you meant.
pub fn hits(state: &AppState, query: &str) -> Vec<Hit> {
    let needle = query.trim().to_lowercase();
    let list = state.list_all();
    let mut by_trigger = Vec::new();
    let mut by_text = Vec::new();

    for (trigger, preview) in list {
        if needle.is_empty() {
            by_trigger.push(Hit { trigger, preview });
            continue;
        }
        if trigger.to_lowercase().contains(&needle) {
            by_trigger.push(Hit { trigger, preview });
        } else if preview.to_lowercase().contains(&needle) {
            by_text.push(Hit { trigger, preview });
        }
    }

    by_trigger.extend(by_text);
    by_trigger.truncate(MAX_RESULTS);
    by_trigger
}

/// How tall and wide the window has to be to show `rows` results without cutting anything off.
///
/// Derived from the constants above and from the fonts actually loaded, rather than guessed. The
/// guess it replaces was a fixed number that took no account of the spacing egui inserts between
/// widgets, so on a full list the window came up some seventy points too short and the footer line
/// underneath the results was sliced off the bottom edge — a line of text that was there, that the
/// window promised room for, and that could not be read.
pub fn window_size(ctx: &egui::Context, rows: usize) -> egui::Vec2 {
    let style = ctx.style_of(ctx.theme());
    let line = |text_style: egui::TextStyle| {
        let font = text_style.resolve(&style);
        ctx.fonts_mut(|f| f.row_height(&font))
    };
    let contents = if rows == 0 {
        GAP_ABOVE_EMPTY + line(egui::TextStyle::Body)
    } else {
        GAP_ABOVE_LIST
            + rows.min(VISIBLE_ROWS) as f32 * ROW_HEIGHT
            + GAP_BELOW_LIST
            + line(egui::TextStyle::Small)
    };
    let height = PAD * 2.0 + crate::ui::controls::FIELD_HEIGHT + contents;
    egui::vec2(WINDOW_WIDTH, height.ceil())
}

/// Draws the window. Returns the trigger to expand, if the user picked one.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) -> Option<String> {
    let t = state.t();
    let is_light = !ui.visuals().dark_mode;
    let accent_color = accent(ui.visuals());
    let mut chosen = None;

    let results = hits(state, &state.search_query);
    // Keep the highlight inside the list as it shrinks under a longer query.
    if state.search_selected >= results.len() {
        state.search_selected = results.len().saturating_sub(1);
    }

    let (up, down, enter, escape) = ui.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::Enter),
            i.key_pressed(egui::Key::Escape),
        )
    });
    if escape {
        state.close_search();
        return None;
    }
    if down && !results.is_empty() {
        state.search_selected = (state.search_selected + 1) % results.len();
        state.search_follow = true;
    }
    if up && !results.is_empty() {
        state.search_selected = state.search_selected.checked_sub(1).unwrap_or(results.len() - 1);
        state.search_follow = true;
    }
    if enter {
        if let Some(hit) = results.get(state.search_selected) {
            chosen = Some(hit.trigger.clone());
        }
    }

    egui::Frame::default()
        .fill(crate::app::win_background_for(is_light))
        .stroke(egui::Stroke::new(1.0, crate::app::line_strong(is_light)))
        .corner_radius(10u8)
        .inner_margin(egui::Margin::same(PAD as i8))
        .show(ui, |ui| {
            // Every gap in this window is written out below, so that what `window_size` computes
            // and what actually gets drawn cannot drift apart.
            ui.spacing_mut().item_spacing.y = 0.0;

            let field = ui.add(
                crate::ui::controls::text_field(&mut state.search_query)
                    .desired_width(ui.available_width())
                    .hint_text(t.search_window_hint),
            );
            // Focused every frame rather than once: this window is created fresh each time it
            // opens, and the one thing that must never happen is that you hit Alt+Space, start
            // typing, and the letters go nowhere.
            if !field.has_focus() {
                field.request_focus();
            }
            if field.changed() {
                // A narrower query means a different list; start it from the top.
                state.search_selected = 0;
                state.search_follow = true;
            }

            if results.is_empty() {
                ui.add_space(GAP_ABOVE_EMPTY);
                ui.label(egui::RichText::new(t.search_window_empty).color(text_tertiary(is_light)));
                return;
            }

            ui.add_space(GAP_ABOVE_LIST);
            let selected = state.search_selected;
            let follow = std::mem::take(&mut state.search_follow);
            egui::ScrollArea::vertical()
                .max_height(VISIBLE_ROWS as f32 * ROW_HEIGHT)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (i, hit) in results.iter().enumerate() {
                        if row(ui, hit, i == selected, follow && i == selected, accent_color, is_light)
                        {
                            chosen = Some(hit.trigger.clone());
                        }
                    }
                });

            ui.add_space(GAP_BELOW_LIST);
            ui.label(
                egui::RichText::new(t.search_window_footer)
                    .small()
                    .color(text_tertiary(is_light)),
            );
        });

    chosen
}

/// One result. The same shape as a row in the main list — trigger in the accent, preview beside it —
/// so the two windows are recognisably the same program.
///
/// `follow` drags the view to this row: set on the row the arrows just moved to, so walking past
/// the bottom of the window scrolls instead of losing the highlight off the edge.
fn row(
    ui: &mut egui::Ui,
    hit: &Hit,
    selected: bool,
    follow: bool,
    accent_color: egui::Color32,
    is_light: bool,
) -> bool {
    let mut clicked = false;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ROW_HEIGHT),
        egui::Sense::click(),
    );

    if follow {
        // `None` means "scroll the least that brings this into view", which keeps the list still
        // when the selection is already on screen.
        ui.scroll_to_rect(rect, None);
    }

    if ui.is_rect_visible(rect) {
        if selected {
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(6),
                crate::app::selection_tint(is_light, accent_color),
            );
            // The same accent bar the main list puts against a selected row.
            ui.painter().rect_filled(
                egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
                egui::CornerRadius::same(2),
                accent_color,
            );
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(6), crate::app::hover_tint(is_light));
        }

        let text_left = rect.left() + ROW_INSET;
        let preview_left = text_left + TRIGGER_COLUMN;
        let preview_color = if selected {
            ui.visuals().text_color()
        } else {
            secondary_text(is_light)
        };

        paint_line(
            ui,
            &hit.trigger,
            egui::TextStyle::Monospace,
            accent_color,
            egui::pos2(text_left, rect.center().y),
            TRIGGER_COLUMN - COLUMN_GAP,
        );
        paint_line(
            ui,
            &hit.preview,
            egui::TextStyle::Body,
            preview_color,
            egui::pos2(preview_left, rect.center().y),
            rect.right() - ROW_INSET - preview_left,
        );
        ui.painter().line_segment(
            [
                egui::pos2(rect.left() + 6.0, rect.bottom()),
                egui::pos2(rect.right() - 6.0, rect.bottom()),
            ],
            egui::Stroke::new(1.0, hairline(is_light)),
        );
    }

    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        clicked = true;
    }
    clicked
}

/// Paints one line of text that ends at `max_width` with an ellipsis, vertically centred on `at`.
///
/// The main window has always done this properly — a `Label` set to truncate, which asks the font
/// how wide the text really is. This window paints its rows itself and was instead shortening the
/// preview by *counting characters*, at a count far larger than the room available, and then
/// hard-clipping whatever was left against the edge of the row. So the text ran all the way to the
/// window's border and stopped in the middle of a letter, with nothing to say that more had been
/// cut off. Measuring in pixels is the only way to end a line where you meant to.
fn paint_line(
    ui: &egui::Ui,
    text: &str,
    style: egui::TextStyle,
    color: egui::Color32,
    at: egui::Pos2,
    max_width: f32,
) {
    let mut job = egui::text::LayoutJob::simple(
        text.to_owned(),
        style.resolve(ui.style()),
        color,
        max_width.max(0.0),
    );
    // One row, and break wherever the width runs out: a preview is a glance at the text, not a
    // paragraph, and a word that does not fit should still show the part of it that does.
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.painter().layout_job(job);
    let top_left = egui::pos2(at.x, at.y - galley.size().y * 0.5);
    ui.painter().galley(top_left, galley, color);
}
