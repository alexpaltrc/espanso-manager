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

//! Building a custom date format by arranging blocks, instead of by knowing strftime.
//!
//! The four presets cover almost everything, but "Personalizado" used to drop the user in front of
//! an empty box expecting `%B %-d, %Y` — a notation that explains nothing to anyone who has not
//! already learned it. Here the same thing is assembled from named pieces: *Año*, *Mes (nombre)*,
//! *Día*, and separators. You drag what you want into a row, and the result underneath updates as
//! you go.
//!
//! ## The format string stays the single source of truth
//!
//! There is deliberately no separate list-of-blocks in the app's state. The blocks are *parsed out
//! of* the format string every frame and written straight back into it when they change. Keeping a
//! parallel structure would mean keeping two things in step, and the moment they disagreed — a
//! hand-edited field, an expansion imported from a colleague — one of them would be silently wrong.
//! This way the text field and the blocks cannot drift apart, because there is only one of them.
//!
//! A format the blocks cannot express (anything with a token outside the set below) is not a
//! failure: the builder simply steps aside and says so, and the text field still works.

use crate::i18n::{Lang, Strings};

/// One piece of a date format: either a value Windows fills in, or a literal character between two.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Block {
    Year,
    MonthName,
    MonthNumber,
    Day,
    /// The hour on this machine's clock.
    Hour,
    /// The same instant expressed in UTC. Espanso applies a time zone to the whole variable, not to
    /// one field of it, so choosing this puts the entire date into UTC — see `timezone_for`.
    HourUtc,
    Minute,
    Separator(char),
}

/// The value blocks, in the order they appear in the palette — roughly largest unit to smallest,
/// which is how people say dates out loud.
pub const FIELDS: [Block; 7] = [
    Block::Year,
    Block::MonthName,
    Block::MonthNumber,
    Block::Day,
    Block::Hour,
    Block::HourUtc,
    Block::Minute,
];

/// The separators. Unlike the value blocks these are meant to be used over and over, so the palette
/// never greys them out.
pub const SEPARATORS: [Block; 6] = [
    Block::Separator('-'),
    Block::Separator('/'),
    // A colon is how a time is written, so building "14:30" out of blocks needs one.
    Block::Separator(':'),
    Block::Separator(','),
    Block::Separator(' '),
    Block::Separator('_'),
];

impl Block {
    /// The strftime this block stands for.
    ///
    /// Numbers are all zero-padded: `09/08/2026` rather than `9/08/2026`. Predictability matters
    /// more here than typographic polish — the presets already cover the written-out forms, and a
    /// builder whose output changes width depending on the day would be a strange thing to hand
    /// somebody who came here to avoid surprises.
    pub fn token(self) -> String {
        match self {
            Block::Year => "%Y".to_string(),
            Block::MonthName => "%B".to_string(),
            Block::MonthNumber => "%m".to_string(),
            Block::Day => "%d".to_string(),
            Block::Hour | Block::HourUtc => "%H".to_string(),
            Block::Minute => "%M".to_string(),
            Block::Separator(c) => c.to_string(),
        }
    }

    pub fn label(self, t: &'static Strings) -> String {
        match self {
            Block::Year => t.block_year.to_string(),
            Block::MonthName => t.block_month_name.to_string(),
            Block::MonthNumber => t.block_month_number.to_string(),
            Block::Day => t.block_day.to_string(),
            Block::Hour => t.block_hour.to_string(),
            Block::HourUtc => t.block_hour_utc.to_string(),
            Block::Minute => t.block_minute.to_string(),
            // A space has nothing to show, so it is named instead of drawn.
            Block::Separator(' ') => t.block_space.to_string(),
            Block::Separator(c) => c.to_string(),
        }
    }

    pub fn is_separator(self) -> bool {
        matches!(self, Block::Separator(_))
    }
}

/// Turns a sequence of blocks back into the strftime string that gets saved.
pub fn to_format(blocks: &[Block]) -> String {
    blocks.iter().map(|b| b.token()).collect()
}

/// Reads a format string as blocks, or returns `None` if it contains anything the builder cannot
/// represent — in which case the caller shows the text field alone rather than a builder that would
/// quietly discard part of the format.
pub fn parse(format: &str, tz_is_utc: bool) -> Option<Vec<Block>> {
    let mut blocks = Vec::new();
    let mut chars = format.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            let spec = chars.next()?;
            blocks.push(match spec {
                'Y' => Block::Year,
                'B' => Block::MonthName,
                'm' => Block::MonthNumber,
                'd' => Block::Day,
                // Local and UTC produce the same code; which one it was is recorded in the
                // expansion's time zone, not in the format string.
                'H' => {
                    if tz_is_utc {
                        Block::HourUtc
                    } else {
                        Block::Hour
                    }
                }
                'M' => Block::Minute,
                _ => return None,
            });
        } else if SEPARATORS.contains(&Block::Separator(c)) {
            blocks.push(Block::Separator(c));
        } else {
            return None;
        }
    }

    Some(blocks)
}

/// The time zone the assembled expansion should carry, if any.
///
/// Espanso applies one time zone to the whole date variable, so a single UTC hour makes the entire
/// expansion UTC — the year, month and day come from the UTC clock too. That is almost always what
/// somebody asking for a UTC time wants (a timestamp is only coherent if all of it agrees), but it
/// is not obvious, so the builder says so on screen whenever this returns `Some`.
pub fn timezone_for(blocks: &[Block]) -> Option<&'static str> {
    blocks
        .iter()
        .any(|b| matches!(b, Block::HourUtc))
        .then_some("UTC")
}

/// Which placed block is being dragged, when one is being reordered.
#[derive(Clone, Copy)]
struct MovedBlock(usize);

/// Draws the builder and returns a new format string if the user rearranged anything.
///
/// Returns `None` when nothing changed, so the caller only writes on a real edit.
pub fn show(
    ui: &mut egui::Ui,
    t: &'static Strings,
    blocks: &[Block],
    month_lang: &mut Lang,
) -> Option<Vec<Block>> {
    let mut next: Option<Vec<Block>> = None;
    let accent = crate::app::accent(ui.visuals());
    let is_light = !ui.visuals().dark_mode;

    ui.label(
        egui::RichText::new(t.blocks_hint)
            .small()
            .color(crate::app::text_tertiary(is_light)),
    );
    ui.add_space(6.0);

    // --- the row being built -------------------------------------------------------------------
    //
    // Hand-rolled rather than `Ui::dnd_drop_zone`, for two reasons: that method paints its own fill
    // over the frame it is given, and it can only hand back one kind of payload — this row has to
    // accept both a new block from the palette and a block already in the row being moved.
    let mut slots: Vec<egui::Rect> = Vec::new();

    let mut frame = egui::Frame::default()
        .fill(crate::app::win_control_for(is_light))
        .stroke(egui::Stroke::new(1.0, crate::app::line_strong(is_light)))
        .corner_radius(8u8)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .begin(ui);

    {
        let ui = &mut frame.content_ui;
        ui.set_min_width(ui.available_width());
        ui.set_min_height(34.0);
        if blocks.is_empty() {
            ui.label(
                egui::RichText::new(t.blocks_empty).color(crate::app::text_tertiary(is_light)),
            );
        } else {
            ui.horizontal_wrapped(|ui| {
                for (i, block) in blocks.iter().enumerate() {
                    // The row is broken by hand. `horizontal_wrapped` only wraps widgets that say
                    // how big they are *before* being laid out, and a chip does not: it is a nested
                    // frame, which takes whatever is left of the current line and draws itself in
                    // it. Left to egui, a format of nine or ten blocks therefore ran straight off
                    // the right-hand edge — the last chip cut in half by the window border, its ×
                    // outside the window and impossible to click, so a block added by accident
                    // could not be taken out again.
                    //
                    // `i > 0` is "there is already something on this line": every pass places a
                    // chip, so ending the line is only ever wrong on the very first one.
                    if i > 0 && ui.available_rect_before_wrap().width() < chip_width(ui, t, *block) {
                        ui.end_row();
                    }
                    let (removed, rect) = placed_chip(ui, t, *block, accent, is_light, i);
                    slots.push(rect);
                    if removed {
                        let mut without = blocks.to_vec();
                        without.remove(i);
                        next = Some(without);
                    }
                    ui.add_space(6.0);
                }
            });
        }
    }

    let response = frame.allocate_space(ui);
    let carrying = egui::DragAndDrop::has_payload_of_type::<Block>(ui.ctx())
        || egui::DragAndDrop::has_payload_of_type::<MovedBlock>(ui.ctx());
    if carrying && response.contains_pointer() {
        frame.frame.stroke = egui::Stroke::new(1.5, accent);
    }
    frame.paint(ui);

    // Where in the row the pointer is: everything that comes before it in reading order comes
    // first. That is what lets a block be dropped *between* two others rather than only at the end.
    let insert_at = ui
        .ctx()
        .pointer_interact_pos()
        .map(|p| insert_index(&slots, p))
        .unwrap_or(blocks.len());

    // `Response::dnd_release_payload` cannot be used here. It gates on `hovered()`, and a widget is
    // never "hovered" while something is being dragged over it — for a drop zone that contains its
    // own drag sources, as this row does, the pointer on release is over the chip rather than over
    // the row, so the drop was silently never seen. `contains_pointer` asks the question that was
    // actually meant: is the pointer inside this rectangle.
    let released = ui.input(|i| i.pointer.any_released());
    let dropped_here = released && response.contains_pointer();

    // The type has to be checked *before* taking. `DragAndDrop::take_payload` removes whatever is
    // being carried and only then tries to downcast it, so asking "is it a Block?" while a moved
    // block was in flight threw the moved block away and reported nothing — which is exactly why
    // reordering appeared to do nothing at all.
    let carrying_move = egui::DragAndDrop::has_payload_of_type::<MovedBlock>(ui.ctx());

    if dropped_here && !carrying_move {
        if let Some(dropped) = egui::DragAndDrop::take_payload::<Block>(ui.ctx()) {
            let mut with = blocks.to_vec();
            with.insert(insert_at.min(with.len()), *dropped);
            next = Some(with);
        }
    }
    if let Some(moved) = (dropped_here && carrying_move)
        .then(|| egui::DragAndDrop::take_payload::<MovedBlock>(ui.ctx()))
        .flatten()
    {
        let from = moved.0;
        if from < blocks.len() {
            let mut reordered = blocks.to_vec();
            let block = reordered.remove(from);
            // Removing the block first shifts everything after it down by one, so a target past it
            // has to come back by one too — otherwise every move to the right lands one slot late.
            let to = if insert_at > from { insert_at - 1 } else { insert_at };
            reordered.insert(to.min(reordered.len()), block);
            next = Some(reordered);
        }
    }

    // Espanso puts one time zone on the whole expansion, so this has to be said out loud.
    if timezone_for(blocks).is_some() {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(t.blocks_utc_note)
                .small()
                .color(crate::app::text_tertiary(is_light)),
        );
    }

    // Only asked once there is a month spelled out in words to ask about, and asked here rather
    // than at the foot of the form: beside the block it governs, where the answer can be seen in
    // the sample above.
    if blocks.iter().any(|b| matches!(b, Block::MonthName)) {
        ui.add_space(12.0);
        crate::ui::edit_form::month_language(ui, t, month_lang);
    }

    ui.add_space(10.0);

    // --- the palette ---------------------------------------------------------------------------
    ui.label(
        egui::RichText::new(t.blocks_fields)
            .strong()
            .color(crate::app::secondary_text(is_light)),
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for (i, block) in FIELDS.iter().enumerate() {
            // Broken by hand for the same reason the row above is: a palette chip is a nested frame
            // too. Here it did not run off the edge, it was squeezed against it — the label inside
            // wrapped instead, turning "Minutos" into a column one letter wide. Visible in Spanish
            // at the smallest window the app allows.
            if i > 0 && ui.available_rect_before_wrap().width() < palette_chip_width(ui, t, *block) {
                ui.end_row();
            }
            if let Some(list) = palette_block(ui, t, *block, accent, blocks, i) {
                next = Some(list);
            }
        }
    });

    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(t.blocks_separators)
            .strong()
            .color(crate::app::secondary_text(is_light)),
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for (i, block) in SEPARATORS.iter().enumerate() {
            if i > 0 && ui.available_rect_before_wrap().width() < palette_chip_width(ui, t, *block) {
                ui.end_row();
            }
            if let Some(list) = palette_block(ui, t, *block, accent, blocks, 100 + i) {
                next = Some(list);
            }
        }
    });

    next
}

/// The width of the × built into every placed chip.
const REMOVE_WIDTH: f32 = 26.0;

/// Everything `placed_chip` puts around its label: the padding before it, the gap after it, the
/// divider, the room kept for the ×, and the pixel the frame's stroke adds on each side.
const CHIP_AROUND_LABEL: f32 = 10.0 + 8.0 + 1.0 + REMOVE_WIDTH + 2.0;

/// The same for a palette chip, which is only `chip`'s frame: its horizontal inner margin on each
/// side, plus the pixel of stroke on each side.
const PALETTE_AROUND_LABEL: f32 = 10.0 + 10.0 + 2.0;

/// How wide the label alone comes out, in the font it will actually be drawn in — `chip_text` puts
/// separators in the monospace family, which is wider per character than the proportional one.
fn label_width(ui: &egui::Ui, t: &'static Strings, block: Block) -> f32 {
    egui::WidgetText::from(chip_text(t, block))
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Body,
        )
        .size()
        .x
}

/// How wide a placed chip will be, worked out before it is drawn.
///
/// The row needs this to decide where to break, and it cannot ask afterwards — by then the chip is
/// already over the edge. Rounded up, because a break one pixel early costs nothing and a break one
/// pixel late is a chip cut by the window border.
fn chip_width(ui: &egui::Ui, t: &'static Strings, block: Block) -> f32 {
    (label_width(ui, t, block) + CHIP_AROUND_LABEL).ceil()
}

/// The same for a chip in the palette.
fn palette_chip_width(ui: &egui::Ui, t: &'static Strings, block: Block) -> f32 {
    (label_width(ui, t, block) + PALETTE_AROUND_LABEL).ceil()
}

/// How many chips come before the pointer, which is the slot a dropped block goes into.
///
/// Reading order, not x order. The row is two lines high as soon as the format is long, and an
/// x-only test counts every chip on the *second* line whose centre happens to sit left of the
/// pointer as coming before it — even while the pointer is still on the first. A block dropped
/// halfway along line one then landed one slot too far for each of them. Lines are compared first:
/// a chip that ends above the pointer is behind it whatever its x, and only the chips the pointer
/// is level with are split into left and right.
fn insert_index(slots: &[egui::Rect], pointer: egui::Pos2) -> usize {
    slots
        .iter()
        .filter(|r| {
            r.bottom() <= pointer.y || (r.top() <= pointer.y && r.center().x < pointer.x)
        })
        .count()
}

/// A block already in the row, with its remove control built into it.
///
/// The two sit inside one outline, split by a divider, so the pair reads as a single object with a
/// button on its end — `[ Año │ × ]`. The × is drawn in the destructive red rather than in muted
/// grey: a control the eye has to hunt for is a control nobody finds, and the previous version made
/// people wonder how a block could be taken out at all.
///
/// Returns `true` on the frame the × is clicked.
fn placed_chip(
    ui: &mut egui::Ui,
    t: &'static Strings,
    block: Block,
    accent: egui::Color32,
    is_light: bool,
    index: usize,
) -> (bool, egui::Rect) {
    let mut removed = false;
    let danger = crate::app::danger(is_light);

    // egui's own drag source handles the payload, the threshold and the floating preview. Hand-
    // rolling that with `Ui::interact` looked equivalent and was not: once a drag begins egui routes
    // the interaction to the widget it thinks is being dragged, and a second interaction claiming
    // the same rectangle simply never reported one.
    let id = egui::Id::new("move_block").with(index);
    let dragged = ui
        .dnd_drag_source(id, MovedBlock(index), |ui| {
            egui::Frame::default()
                .fill(chip_fill(ui, block, accent, is_light))
                .stroke(egui::Stroke::new(1.0, chip_stroke(block, accent, is_light)))
                .corner_radius(6u8)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.horizontal(|ui| {
                        ui.add_space(10.0);
                        ui.label(chip_text(t, block));
                        ui.add_space(8.0);

                        // The divider is what stops the × reading as part of the format.
                        let (line, _) =
                            ui.allocate_exact_size(egui::vec2(1.0, 18.0), egui::Sense::hover());
                        ui.painter().rect_filled(
                            line,
                            0,
                            chip_stroke(block, accent, is_light).gamma_multiply(0.7),
                        );

                        // Room for the remove control, which is drawn over this space afterwards:
                        // a real button here would be inside the drag source and would lose its
                        // clicks to it.
                        let (slot, _) = ui.allocate_exact_size(
                            egui::vec2(REMOVE_WIDTH, 24.0),
                            egui::Sense::hover(),
                        );
                        ui.painter().text(
                            slot.center(),
                            egui::Align2::CENTER_CENTER,
                            "×",
                            egui::TextStyle::Body.resolve(ui.style()),
                            danger,
                        );
                    });
                });
        })
        .response;

    let rect = dragged.rect;

    // Registered last, so it sits on top of the drag source and takes the clicks that land on it.
    let remove_rect = egui::Rect::from_min_max(
        egui::pos2(rect.right() - REMOVE_WIDTH, rect.top()),
        rect.max,
    );
    let remove = ui.interact(remove_rect, id.with("remove"), egui::Sense::click());
    if remove.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if remove.on_hover_text(t.blocks_remove_tip).clicked() {
        removed = true;
    }

    (removed, rect)
}

/// One block in the palette: draggable onto the row, and clickable as the simpler way to do the
/// same thing. Dragging is what was asked for; clicking is what most people try first.
///
/// `dnd_drag_source` is not used here even though it looks like the obvious fit: it senses dragging
/// only, so the block would never report a click and the simpler gesture would silently do nothing.
/// One interaction that senses both is registered instead — the same approach the expansion rows
/// use, and for the same reason.
fn palette_block(
    ui: &mut egui::Ui,
    t: &'static Strings,
    block: Block,
    accent: egui::Color32,
    current: &[Block],
    salt: usize,
) -> Option<Vec<Block>> {
    let id = egui::Id::new("palette_block").with(salt);

    // Always drawn in place, so picking one up never makes the palette reflow under the pointer.
    let placed = ui.scope(|ui| chip(ui, t, block, accent)).response.rect;
    let response = ui.interact(placed, id, egui::Sense::click_and_drag());

    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    if response.dragged() {
        egui::DragAndDrop::set_payload(ui.ctx(), block);
        // A copy follows the pointer, so it is obvious something is being carried.
        if let Some(pointer) = ui.ctx().pointer_interact_pos() {
            egui::Area::new(id.with("carried"))
                .order(egui::Order::Tooltip)
                .fixed_pos(pointer + egui::vec2(10.0, 10.0))
                .show(ui.ctx(), |ui| {
                    chip(ui, t, block, accent);
                });
        }
    }

    if response.clicked() {
        let mut with = current.to_vec();
        with.push(block);
        return Some(with);
    }
    None
}

/// Draws one block. Value blocks carry the accent so they read as "something gets filled in here";
/// separators stay neutral, because they are exactly the character shown and nothing more.
/// Value blocks carry a wash of the accent so they read as "something gets filled in here";
/// separators stay neutral, because a separator is exactly the character shown and nothing more.
fn chip_fill(
    ui: &egui::Ui,
    block: Block,
    accent: egui::Color32,
    is_light: bool,
) -> egui::Color32 {
    let _ = ui;
    if block.is_separator() {
        crate::app::win_control_for(is_light)
    } else {
        crate::app::mix(
            crate::app::win_control_for(is_light),
            accent,
            if is_light { 0.16 } else { 0.26 },
        )
    }
}

fn chip_stroke(block: Block, accent: egui::Color32, is_light: bool) -> egui::Color32 {
    if block.is_separator() {
        crate::app::line_strong(is_light)
    } else {
        accent
    }
}

fn chip_text(t: &'static Strings, block: Block) -> egui::RichText {
    let label = block.label(t);
    if block.is_separator() {
        egui::RichText::new(label).monospace()
    } else {
        egui::RichText::new(label)
    }
}

fn chip(ui: &mut egui::Ui, t: &'static Strings, block: Block, accent: egui::Color32) {
    let is_light = !ui.visuals().dark_mode;
    egui::Frame::default()
        .fill(chip_fill(ui, block, accent, is_light))
        .stroke(egui::Stroke::new(1.0, chip_stroke(block, accent, is_light)))
        .corner_radius(6u8)
        .inner_margin(egui::Margin::symmetric(10, 5))
        .show(ui, |ui| {
            ui.label(chip_text(t, block));
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, Rect};

    /// Two lines of three chips each, the shape a long format takes once the row breaks.
    /// Line one runs y 100..126, line two y 132..158; chips are 100 wide with a 20 gap.
    fn two_lines() -> Vec<Rect> {
        let line = |top: f32| {
            (0..3).map(move |i| {
                let left = 10.0 + i as f32 * 120.0;
                Rect::from_min_max(pos2(left, top), pos2(left + 100.0, top + 26.0))
            })
        };
        line(100.0).chain(line(132.0)).collect()
    }

    #[test]
    fn before_everything_inserts_at_the_front() {
        assert_eq!(insert_index(&two_lines(), pos2(5.0, 112.0)), 0);
    }

    #[test]
    fn the_end_of_the_first_line_does_not_reach_into_the_second() {
        // The x-only test this replaced counted the three chips of line two as well, since all
        // three centres sit left of x=400 — a block dropped here landed at slot 6, not 3.
        assert_eq!(insert_index(&two_lines(), pos2(400.0, 112.0)), 3);
    }

    #[test]
    fn a_pointer_on_the_second_line_counts_the_whole_first_one() {
        assert_eq!(insert_index(&two_lines(), pos2(5.0, 145.0)), 3);
        // In the gap between the first and second chip of line two, so before the second: the
        // three of line one, plus the one whose centre the pointer has passed.
        assert_eq!(insert_index(&two_lines(), pos2(120.0, 145.0)), 4);
        assert_eq!(insert_index(&two_lines(), pos2(400.0, 145.0)), 6);
    }

    #[test]
    fn the_gap_between_lines_belongs_to_the_line_above() {
        assert_eq!(insert_index(&two_lines(), pos2(5.0, 129.0)), 3);
    }

    #[test]
    fn below_the_last_line_inserts_at_the_end() {
        assert_eq!(insert_index(&two_lines(), pos2(5.0, 200.0)), 6);
    }

    #[test]
    fn a_single_line_still_splits_on_x_alone() {
        let one: Vec<Rect> = two_lines().into_iter().take(3).collect();
        assert_eq!(insert_index(&one, pos2(5.0, 112.0)), 0);
        assert_eq!(insert_index(&one, pos2(70.0, 112.0)), 1);
        assert_eq!(insert_index(&one, pos2(190.0, 112.0)), 2);
        assert_eq!(insert_index(&one, pos2(400.0, 112.0)), 3);
    }

    #[test]
    fn an_empty_row_always_inserts_at_the_front() {
        assert_eq!(insert_index(&[], pos2(400.0, 112.0)), 0);
    }
}
