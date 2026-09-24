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

//! The handful of controls used on more than one screen, shaped after the ones Windows draws itself.
//!
//! ## The segmented control
//!
//! Wherever a setting has a few choices and they all fit on one line, Windows 11 draws them as one
//! outlined track with the chosen option lit — not as loose radio buttons, and not as free-floating
//! toggles. This app was doing that job in three different shapes: painted icon segments for the
//! list density, `selectable_label`s for the theme and the language, and radio buttons for
//! text-or-date. They behave identically, so they now look identical too; the density switch keeps
//! its pictograms but shares this file's background painter, which is what guarantees the two stay
//! in step rather than merely starting out similar.
//!
//! ## Why the text field is a helper and not `add_sized`
//!
//! A `TextEdit` is as tall as its text plus its margin — about 22 points — while the buttons beside
//! it are 30, so the obvious fix was to hand `add_sized` the height we wanted. That stretches the
//! *frame* and leaves the text exactly where it was: at the top, with all the extra height dumped
//! underneath. It is why the trigger boxes looked like the words in them had floated upward.
//!
//! Growing the margin instead makes the field genuinely taller, the way a Windows field is, and the
//! text stays in the middle because the middle is where it was all along.
//!
//! ## The metrics live here, and only here
//!
//! Colour is decided in `app.rs`; *size* is decided here. Every corner radius, every gap and every
//! row's padding in the app is one of the constants below, so a screen cannot quietly invent its own
//! 11-point gap or 6-point radius. The six radii are a deliberate ladder — the bigger the surface,
//! the rounder its corner — and picking one is therefore a question about what kind of thing is being
//! drawn, not a free choice.
//!
//! What does *not* belong here: anything used on exactly one screen. A constant with one caller is
//! that screen's business, and putting it here only makes the shared vocabulary harder to read.

use crate::app::{
    accent, danger, hairline, hover_tint, line_strong, mix, readable_on, secondary_text,
    selection_tint, win_background_for, win_card_for, win_control_for,
};

/// Height of a text field and of one segment, before the container's own 2-point margin. Matches
/// `interact_size.y`, so a field, a button and a segmented control all line up in the same row.
pub const FIELD_HEIGHT: f32 = 34.0;

// --- Corners ----------------------------------------------------------------------------------
//
// A ladder, not a set of alternatives: a control is the smallest thing on screen and the least
// rounded, a window is the largest and the most.

/// A button, a field, a chip — anything you press or type in.
pub const RADIUS_CONTROL: u8 = 7;
/// A block sunk *into* a surface: a preview, the bulk-action bar.
pub const RADIUS_INSET: u8 = 8;
/// The list of expansions.
pub const RADIUS_LIST: u8 = 9;
/// One titled section of a screen.
pub const RADIUS_SECTION: u8 = 10;
/// A dialog standing in front of the window.
pub const RADIUS_DIALOG: u8 = 12;
/// A folder name shown as a label rather than as a control.
pub const RADIUS_TAG: u8 = 5;

// --- Spacing ----------------------------------------------------------------------------------
//
// Multiples of the same small step. Between two things that belong together, the small ones;
// between two things that merely follow one another, the large ones.

pub const GAP_TIGHT: f32 = 4.0;
pub const GAP: f32 = 8.0;
pub const GAP_ROW: f32 = 10.0;
pub const GAP_WIDE: f32 = 12.0;
pub const GAP_SECTION: f32 = 16.0;
pub const GAP_STACK: f32 = 18.0;

/// The gutter down each side of a screen, and the narrower one a small window gets instead.
///
/// Tightening it is the *first* thing that gives when the window shrinks, before any control is
/// allowed to get smaller — which is the difference between a layout that adapts and one that just
/// squeezes everything indiscriminately.
pub const PAGE_MARGIN: i8 = 24;
pub const PAGE_MARGIN_NARROW: i8 = 16;
/// Below this width the screen is treated as narrow.
pub const NARROW: f32 = 480.0;

pub fn page_margin(width: f32) -> i8 {
    if width < NARROW { PAGE_MARGIN_NARROW } else { PAGE_MARGIN }
}

/// How far a page's scroll area reaches into the gutter: enough on the left for a card's stroke,
/// and on the right for the stroke plus the scroll bar at its widest (10 px) and a gap before it.
const PAGE_OUTLINE_ROOM: i8 = 2;
const PAGE_BAR_ROOM: i8 = 14;

/// The vertical scroll area a whole screen scrolls in.
///
/// A scroll area clips to exactly its own rect, and a card that fills the width has its outer
/// stroke on that rect's edge — so half the stroke and the rounded corners were cut off on the
/// right. And egui's floating bar is drawn inside the same rect, over the cards' edge. Both are
/// fixed the same way: the area is widened into the gutter, and the same amount is handed back as
/// `content_margin`. The content is exactly as wide as before, its outlines are inside the clip,
/// and the bar floats in the gutter beside the cards instead of on them. Both gutters are wider
/// than the room taken (`PAGE_MARGIN_NARROW` is 16).
pub fn page_scroll<R>(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let mut rect = ui.available_rect_before_wrap();
    rect.min.x -= f32::from(PAGE_OUTLINE_ROOM);
    rect.max.x += f32::from(PAGE_BAR_ROOM);
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        egui::ScrollArea::vertical()
            .id_salt(id_salt)
            .auto_shrink([false, false])
            .content_margin(egui::Margin {
                left: PAGE_OUTLINE_ROOM,
                right: PAGE_BAR_ROOM,
                top: 0,
                bottom: 0,
            })
            .show(ui, contents)
            .inner
    })
    .inner
}

/// Padding inside one section of a screen.
pub const SECTION_PAD: i8 = 18;
/// Horizontal padding inside a list row, and the vertical padding in each density.
pub const ROW_PAD_X: f32 = 14.0;
pub const ROW_PAD_COMFORTABLE: f32 = 13.0;
pub const ROW_PAD_DENSE: f32 = 8.0;
/// The side of a checkbox's square.
pub const CHECKBOX: f32 = 18.0;

// --- Type -------------------------------------------------------------------------------------

/// The heading at the top of a screen. `TextStyle::Heading` is set to this size in
/// [`crate::app`]'s layout style; naming it here is what lets a screen ask for "the heading"
/// without also having to know how big that is.
pub fn h2(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).heading()
}

/// The heading of one section *within* a screen. Between body and heading, and set apart by weight
/// rather than by being merely bigger.
pub fn h3(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).size(17.0).strong()
}

/// Supporting copy: the sentence under a heading, a count, a hint.
pub fn muted(text: impl Into<String>, is_light: bool) -> egui::RichText {
    egui::RichText::new(text).color(secondary_text(is_light))
}

/// The same, one size down — for counts and captions, never for anything that names a thing.
pub fn small_muted(text: impl Into<String>, is_light: bool) -> egui::RichText {
    egui::RichText::new(text).small().color(secondary_text(is_light))
}

/// A trigger, written the way a trigger is written: monospaced and in the accent, so it reads as a
/// literal string to type rather than as a word in a sentence.
pub fn code(text: impl Into<String>, visuals: &egui::Visuals) -> egui::RichText {
    egui::RichText::new(text).monospace().color(accent(visuals))
}

/// The label above a field.
pub fn field_label(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).strong()
}

// --- Page chrome ------------------------------------------------------------------------------

/// The top of every screen that is not the library: one step back, the screen's name, and a line
/// saying what the screen is for.
///
/// Meant to be drawn *before* the screen's scroll area rather than inside it. Ajustes and the guide
/// are both taller than the window, and a way out that has to be scrolled to is not the one step
/// back the plan asks for. Returns whether the user asked to leave.
///
/// No chevron in front of the word. A glyph here would have to be one the icon family really draws
/// on every machine and in all four alphabets, and the word alone has never been ambiguous.
pub fn page_header(ui: &mut egui::Ui, back: &str, title: &str, subtitle: &str) -> bool {
    let is_light = !ui.visuals().dark_mode;
    let leaving = button(ui, back, Tone::Quiet, true).clicked();
    ui.add_space(GAP_WIDE);
    ui.add(egui::Label::new(h2(title)).wrap());
    if !subtitle.is_empty() {
        ui.add_space(GAP_TIGHT);
        ui.add(egui::Label::new(muted(subtitle, is_light)).wrap());
    }
    ui.add_space(GAP_STACK);
    leaving
}

// --- Surfaces ---------------------------------------------------------------------------------

/// One titled section of a screen: a raised card with the one line colour around it.
pub fn section_frame(is_light: bool) -> egui::Frame {
    egui::Frame::default()
        .fill(win_card_for(is_light))
        .stroke(egui::Stroke::new(1.0, hairline(is_light)))
        .corner_radius(RADIUS_SECTION)
        .inner_margin(egui::Margin::same(SECTION_PAD))
}

/// A block sunk into a surface — a preview of what an expansion inserts, the bulk-action bar.
/// Tinted rather than outlined: it is part of the thing it sits in, not a separate card.
pub fn inset_frame(ui: &egui::Ui) -> egui::Frame {
    let is_light = !ui.visuals().dark_mode;
    egui::Frame::default()
        .fill(selection_tint(is_light, accent(ui.visuals())))
        .corner_radius(RADIUS_INSET)
        .inner_margin(egui::Margin::same(14))
}

/// A folder's name shown as a label. Sits on the *window* colour, not the card's, so it reads as
/// stamped into the surface rather than raised off it.
pub fn tag_frame(is_light: bool) -> egui::Frame {
    egui::Frame::default()
        .fill(win_background_for(is_light))
        .stroke(egui::Stroke::new(1.0, hairline(is_light)))
        .corner_radius(RADIUS_TAG)
        .inner_margin(egui::Margin::symmetric(8, 3))
}

// --- Dialogs ----------------------------------------------------------------------------------

/// How much room a dialog keeps from the window's edges on each side, frame included.
const DIALOG_CLEARANCE: f32 = 36.0;

/// Every in-app dialog: the card surface and hairline the screens use, the page gutter as padding.
///
/// egui's own modal frame is `Frame::popup`, whose padding is the 6-point `menu_margin` meant for a
/// dropdown — the title of a question then sat almost on the dialog's edge, and each of the four
/// dialogs looked like a menu that had lost its way. Changing `menu_margin` itself would have fixed
/// the dialogs by bloating every real menu, so the dialogs get their own frame instead. The shadow is
/// still the popup's.
pub fn dialog(ctx: &egui::Context, id: &str) -> egui::Modal {
    let style = ctx.global_style();
    let is_light = !style.visuals.dark_mode;
    let frame = egui::Frame::popup(&style)
        .fill(win_card_for(is_light))
        .stroke(egui::Stroke::new(1.0, hairline(is_light)))
        .corner_radius(RADIUS_DIALOG)
        .inner_margin(egui::Margin::same(PAGE_MARGIN));
    egui::Modal::new(egui::Id::new(id)).frame(frame)
}

/// The width of a dialog's contents: `widest` where the window allows it, less on a small window,
/// so a dialog gives up width before it ever reaches the window's edge.
pub fn dialog_width(ctx: &egui::Context, widest: f32) -> f32 {
    let frame = 2.0 * f32::from(PAGE_MARGIN);
    (ctx.content_rect().width() - frame - 2.0 * DIALOG_CLEARANCE).min(widest)
}

/// The row of buttons at the foot of a dialog, laid out the way Windows lays out its own: against
/// the right edge, the action first and «Cancelar» after it. Right to left, so `buttons` adds them
/// in the opposite order — «Cancelar» first.
pub fn dialog_buttons(ui: &mut egui::Ui, buttons: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), buttons);
    });
}

// --- Buttons ----------------------------------------------------------------------------------

/// What a button is *for*, which is the only thing a caller should have to decide.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// The ordinary outlined button.
    Normal,
    /// The one filled button on a screen: the thing you came here to do. At most one.
    Primary,
    /// No fill and no border — a link that happens to be a button.
    Quiet,
    /// Ordinary in every way except that its words are red. Deliberately *not* a filled red
    /// button: a destructive action should be legible, not decorated.
    Danger,
}

/// The ring Windows draws round the control that Tab has reached: two points of ink, one point
/// clear of the control's edge.
///
/// Every control here sets its own fill and border, and in egui that replaces the visuals that
/// would otherwise have shown focus, so without this a keyboard user tabbed through the library
/// blind. A click does not give a button focus in egui — only Tab, or a text field being clicked —
/// so the ring appears for the keyboard and stays out of the way of the mouse, as it does in
/// Windows.
pub fn focus_ring(ui: &egui::Ui, response: &egui::Response, radius: f32) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.expand(3.0),
            radius + 3.0,
            egui::Stroke::new(2.0, ui.visuals().strong_text_color()),
            egui::StrokeKind::Inside,
        );
    }
}

/// Every button in the app that is not a segment or a chip.
///
/// `enabled` is a parameter rather than something the caller wraps this in, because a disabled
/// action has to stay *visible* — the plan's rule that bulk actions are present and greyed with
/// nothing selected, instead of vanishing and taking the explanation with them.
pub fn button(ui: &mut egui::Ui, label: &str, tone: Tone, enabled: bool) -> egui::Response {
    let is_light = !ui.visuals().dark_mode;
    let accent_color = accent(ui.visuals());

    let (fill, stroke, ink) = match tone {
        Tone::Primary => (
            accent_color,
            egui::Stroke::NONE,
            readable_on(accent_color),
        ),
        Tone::Quiet => (
            egui::Color32::TRANSPARENT,
            egui::Stroke::NONE,
            accent_color,
        ),
        Tone::Danger => (
            win_control_for(is_light),
            egui::Stroke::new(1.0, line_strong(is_light)),
            danger(is_light),
        ),
        Tone::Normal => (
            win_control_for(is_light),
            egui::Stroke::new(1.0, line_strong(is_light)),
            ui.visuals().text_color(),
        ),
    };

    // Pinned to the icon family for the same reason `primary_button` is: some of these labels
    // carry a glyph, and the proportional face does not have it. See [`crate::fonts::icon`].
    let text = egui::RichText::new(label)
        .family(crate::fonts::icons_family())
        .color(ink);
    let text = if tone == Tone::Primary { text.strong() } else { text };

    let widget = egui::Button::new(text).fill(fill).stroke(stroke);
    let response = ui.add_enabled(enabled, widget);
    if response.enabled() && response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    focus_ring(ui, &response, f32::from(RADIUS_CONTROL));
    response
}

// --- Chips and checkboxes ----------------------------------------------------------------------

/// One folder filter: a rectangle with the folder's name and, when there is one, its count.
///
/// Chosen state is carried three ways at once — fill, border and colour — because one of them alone
/// is a problem for somebody: fill alone disappears on a dim panel, colour alone disappears for a
/// colour-blind reader, and border alone is too quiet to find at a glance.
pub fn chip(
    ui: &mut egui::Ui,
    label: &str,
    count: Option<usize>,
    selected: bool,
) -> egui::Response {
    const PAD_X: f32 = 11.0;
    const COUNT_GAP: f32 = 5.0;

    let is_light = !ui.visuals().dark_mode;
    let accent_color = accent(ui.visuals());
    let ink = if selected { accent_color } else { ui.visuals().text_color() };

    let name_font = egui::TextStyle::Button.resolve(ui.style());
    let count_font = egui::TextStyle::Small.resolve(ui.style());

    // Laid out before anything is allocated, so the chip asks for exactly the width its words need.
    // Nothing here ever shrinks the text to make a row fit: the row wraps instead.
    let name = ui
        .painter()
        .layout_no_wrap(label.to_owned(), name_font, ink);
    let tally = count.map(|n| {
        ui.painter()
            .layout_no_wrap(n.to_string(), count_font, secondary_text(is_light))
    });

    let mut width = name.size().x + PAD_X * 2.0;
    if let Some(tally) = &tally {
        width += COUNT_GAP + tally.size().x;
    }
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, FIELD_HEIGHT), egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let fill = if selected {
            selection_tint(is_light, accent_color)
        } else if response.hovered() {
            hover_tint(is_light)
        } else {
            win_control_for(is_light)
        };
        let edge = if selected { accent_color } else { hairline(is_light) };
        let painter = ui.painter();
        painter.rect_filled(rect, RADIUS_CONTROL, fill);
        painter.rect_stroke(
            rect,
            RADIUS_CONTROL,
            egui::Stroke::new(1.0, edge),
            egui::StrokeKind::Inside,
        );

        let mut x = rect.left() + PAD_X;
        let name_size = name.size();
        painter.galley(
            egui::pos2(x, rect.center().y - name_size.y * 0.5),
            name,
            ink,
        );
        x += name_size.x + COUNT_GAP;
        if let Some(tally) = tally {
            let size = tally.size();
            painter.galley(
                egui::pos2(x, rect.center().y - size.y * 0.5),
                tally,
                secondary_text(is_light),
            );
        }
    }

    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    focus_ring(ui, &response, f32::from(RADIUS_CONTROL));
    response
}

/// The square that marks one item of a set. Painted rather than `egui::Checkbox` so it is the size
/// the design says ([`CHECKBOX`], 18 points) and carries the accent, instead of egui's own smaller
/// grey one.
///
/// Only painting, with no sense of its own, because in the list the whole row is the click target:
/// a box that took its own clicks would have a dead spot in the middle of every row where ticking
/// the box and opening the row disagreed about what had just been pressed.
pub fn paint_checkbox(ui: &egui::Ui, rect: egui::Rect, checked: bool) {
    let is_light = !ui.visuals().dark_mode;
    let accent_color = accent(ui.visuals());
    let radius = egui::CornerRadius::same(4);
    let painter = ui.painter();

    if checked {
        painter.rect_filled(rect, radius, accent_color);
        let tick = readable_on(accent_color);
        // Drawn from two strokes rather than set as a glyph: a check mark from a font lands at
        // whatever size and baseline that font decided, and this one has to sit in an 18-point box.
        let w = rect.width();
        let stroke = egui::Stroke::new((w * 0.115).max(1.6), tick);
        let p = |fx: f32, fy: f32| egui::pos2(rect.left() + w * fx, rect.top() + w * fy);
        painter.line_segment([p(0.24, 0.52), p(0.42, 0.70)], stroke);
        painter.line_segment([p(0.42, 0.70), p(0.76, 0.31)], stroke);
    } else {
        painter.rect_filled(rect, radius, win_control_for(is_light));
        painter.rect_stroke(
            rect,
            radius,
            egui::Stroke::new(1.0, line_strong(is_light)),
            egui::StrokeKind::Inside,
        );
    }
}

// --- Notices ----------------------------------------------------------------------------------

/// A sentence the screen wants read before the next action: what an import did, why saving is
/// refused, what deleting a folder will take with it.
///
/// A tinted block with a coloured bar down its left edge — the bar is what makes it findable at a
/// glance, and it changes colour while the text does not, so the message stays as readable as any
/// other sentence on the screen.
pub fn notice(ui: &mut egui::Ui, text: &str, tone: Tone) {
    let is_light = !ui.visuals().dark_mode;
    let accent_color = accent(ui.visuals());
    let bar = match tone {
        Tone::Danger => danger(is_light),
        _ => accent_color,
    };

    let fill = if matches!(tone, Tone::Danger) {
        mix(win_background_for(is_light), bar, if is_light { 0.09 } else { 0.16 })
    } else {
        selection_tint(is_light, accent_color)
    };

    let inner = egui::Frame::default()
        .fill(fill)
        .corner_radius(egui::CornerRadius {
            nw: 0,
            ne: RADIUS_CONTROL,
            sw: 0,
            se: RADIUS_CONTROL,
        })
        .inner_margin(egui::Margin {
            left: 17,
            right: 14,
            top: 12,
            bottom: 12,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(text);
        });

    // The bar goes on last and over the fill's left edge, which is square for exactly this reason.
    let rect = inner.response.rect;
    ui.painter().rect_filled(
        egui::Rect::from_min_max(rect.left_top(), egui::pos2(rect.left() + 3.0, rect.bottom())),
        0,
        bar,
    );
}

/// A single-line field with Windows' proportions.
///
/// `vertical_align` is belt and braces: the margin alone already centres the text, but a caller who
/// wraps this in `add_sized` would otherwise reintroduce the very bug this exists to fix.
pub fn text_field(text: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text)
        .margin(egui::Margin::symmetric(12, 9))
        .vertical_align(egui::Align::Center)
}

/// A multi-line field with the same horizontal padding. The text starts at the top here, which is
/// correct — a paragraph is read from its first line, not from its middle.
pub fn multiline_field(text: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::multiline(text).margin(egui::Margin::symmetric(10, 8))
}

/// The one filled button on a screen: the thing you came to that screen to do.
pub fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let accent_color = accent(ui.visuals());
    let response = ui.add(
        egui::Button::new(
            // Pinned to the icon family: every label that reaches here is chrome, and two of them
            // carry a glyph. See [`crate::fonts::icon`].
            egui::RichText::new(label)
                .family(crate::fonts::icons_family())
                .color(readable_on(accent_color)),
        )
            .fill(accent_color)
            .stroke(egui::Stroke::NONE),
    );
    focus_ring(ui, &response, f32::from(RADIUS_CONTROL));
    response
}

/// Paints one segment's background: the lit fill when it is the chosen one, a hover wash when the
/// pointer is over it, nothing otherwise.
///
/// The chosen side is *tinted* rather than filled with raw accent. A segmented control is a position
/// of a switch, not the primary action on the screen; giving it the same weight as the filled button
/// would leave two things shouting at once.
pub fn segment_background(ui: &egui::Ui, rect: egui::Rect, selected: bool, hovered: bool) {
    let is_light = !ui.visuals().dark_mode;
    let accent_color = accent(ui.visuals());
    let radius = egui::CornerRadius::same(3);
    let painter = ui.painter();

    let bg = if selected {
        selection_tint(is_light, accent_color)
    } else if hovered {
        hover_tint(is_light)
    } else {
        egui::Color32::TRANSPARENT
    };
    painter.rect_filled(rect, radius, bg);

    if selected {
        painter.rect_stroke(
            rect,
            radius,
            egui::Stroke::new(
                1.0,
                mix(
                    win_background_for(is_light),
                    accent_color,
                    if is_light { 0.28 } else { 0.32 },
                ),
            ),
            egui::StrokeKind::Inside,
        );
    }
}

/// The outlined track the segments sit in.
pub fn segment_track(is_light: bool) -> egui::Frame {
    egui::Frame::default()
        .fill(win_control_for(is_light))
        .stroke(egui::Stroke::new(1.0, line_strong(is_light)))
        .corner_radius(8u8)
        .inner_margin(egui::Margin::same(2))
}

/// A row of mutually exclusive options in one track. Returns the index the user just picked, or
/// `None` if they picked nothing or re-picked what was already chosen.
///
/// `selected` is an `Option` because a value can legitimately be none of the offered ones — a custom
/// prefix typed by hand, for instance. In that case no segment is lit, which says so honestly rather
/// than lighting whichever one happens to be first.
pub fn segmented<S: AsRef<str>>(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    labels: &[S],
    selected: Option<usize>,
) -> Option<usize> {
    if labels.is_empty() {
        return None;
    }

    const HEIGHT: f32 = FIELD_HEIGHT - 4.0;
    const GAP: f32 = 2.0;
    const PADDING: f32 = 12.0;

    let is_light = !ui.visuals().dark_mode;
    let font = egui::TextStyle::Button.resolve(ui.style());
    let galleys: Vec<_> = labels
        .iter()
        .map(|label| {
            ui.painter().layout_no_wrap(
                label.as_ref().to_owned(),
                font.clone(),
                egui::Color32::PLACEHOLDER,
            )
        })
        .collect();

    let mut widths: Vec<f32> = galleys
        .iter()
        .map(|g| (g.size().x + PADDING * 2.0).max(HEIGHT))
        .collect();
    let gaps = GAP * (labels.len() - 1) as f32;
    let natural: f32 = widths.iter().sum::<f32>() + gaps;

    // On a narrow window the track shrinks rather than running off the edge. Each segment then
    // clips its own text instead of writing over its neighbour.
    let room = ui.available_width() - 8.0;
    if natural > room && room > gaps {
        let factor = (room - gaps) / (natural - gaps);
        for w in &mut widths {
            *w *= factor;
        }
    }
    let total: f32 = widths.iter().sum::<f32>() + gaps;

    let mut picked = None;
    // Stable ids, so two segmented controls on one screen can never collide and steal each
    // other's clicks when the layout around them shifts.
    ui.push_id(id_salt, |ui| {
    segment_track(is_light).show(ui, |ui| {
        // An explicit allocation, not `ui.horizontal`: that claims the parent's whole width before
        // laying anything out, which would stretch the track across the window.
        ui.allocate_ui_with_layout(
            egui::vec2(total, HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                for (i, (galley, width)) in galleys.into_iter().zip(widths).enumerate() {
                    let is_selected = selected == Some(i);
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(width, HEIGHT),
                        egui::Sense::click(),
                    );

                    if ui.is_rect_visible(rect) {
                        segment_background(ui, rect, is_selected, response.hovered());
                        let ink = if is_selected {
                            ui.visuals().text_color()
                        } else {
                            secondary_text(is_light)
                        };
                        let at = rect.center() - galley.size() * 0.5;
                        ui.painter()
                            .with_clip_rect(rect.intersect(ui.clip_rect()))
                            .galley(at, galley, ink);
                    }

                    if response.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    focus_ring(ui, &response, 3.0);
                    if response.clicked() && !is_selected {
                        picked = Some(i);
                    }
                }
            },
        );
    });
    });

    picked
}


/// A Windows 11 toggle switch.
///
/// A checkbox and a switch say slightly different things: a checkbox marks an item in a list, a
/// switch turns a thing on. "Start with Windows" is the second kind, and it is the only setting in
/// this app that is, so it gets the control Windows itself uses for it.
///
/// The travel is animated through egui's own `animate_bool_with_time`, the same easing already
/// driving the selection bar — one animation system for the whole app, and it costs a repaint only
/// while the knob is actually moving.
pub fn toggle(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let size = egui::vec2(40.0, 20.0);
    let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    focus_ring(ui, &response, rect.height() * 0.5);

    if ui.is_rect_visible(rect) {
        let is_light = !ui.visuals().dark_mode;
        let accent_color = accent(ui.visuals());
        let t = ui.ctx().animate_bool_with_time(response.id, *on, 0.12);

        let off_fill = win_control_for(is_light);
        let fill = mix(off_fill, accent_color, t);
        let stroke_color = mix(line_strong(is_light), accent_color, t);
        let radius = egui::CornerRadius::same((rect.height() * 0.5) as u8);

        let painter = ui.painter();
        painter.rect_filled(rect, radius, fill);
        painter.rect_stroke(
            rect,
            radius,
            egui::Stroke::new(1.0, stroke_color),
            egui::StrokeKind::Inside,
        );

        // Off, the knob is the ink colour on the control surface; on, it is whatever reads against
        // the accent — which is how it stays visible on a pale accent as well as a dark one.
        let knob_x = egui::lerp((rect.left() + 10.0)..=(rect.right() - 10.0), t);
        let knob_r = egui::lerp(5.0..=6.0, t);
        let knob_color = if t > 0.5 {
            readable_on(accent_color)
        } else {
            secondary_text(is_light)
        };
        painter.circle_filled(egui::pos2(knob_x, rect.center().y), knob_r, knob_color);
    }

    response
}
