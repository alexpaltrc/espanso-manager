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

//! The interface's pictograms, drawn from Windows' own icon font.
//!
//! Segoe Fluent Icons on Windows 11, Segoe MDL2 Assets on Windows 10: the same codepoints, so one
//! table serves both, and the icons are the ones the user already sees in Explorer and Settings.
//! Every icon is centred on its visible ink rather than on the font's line box — icon fonts put
//! their em square wherever they like relative to the baseline, and centring the box left the pause
//! bars a point and a half off the middle of their button.
//!
//! A machine with neither font gets a few line segments instead, drawn here, for the icons the main
//! screen cannot do without. It is a fallback for a machine that cannot draw its own Start menu,
//! not a second icon set, and it is kept that small on purpose.
//!
//! **Not here:** the tray and window icons, which are bitmaps built in [`crate::icons`].

use egui::{Color32, Rect, Stroke};

/// The icons the screens use, and nothing else. Add one when a screen needs it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Glyph {
    Add,
    More,
    Search,
    Play,
    Pause,
    Close,
    Delete,
    Folder,
    MoveToFolder,
    Copy,
    Back,
    Settings,
    Help,
    ChevronDown,
    ChevronRight,
    Check,
    MultiSelect,
    Rename,
    Info,
    Error,
    Undo,
}

impl Glyph {
    /// The codepoint in Segoe Fluent Icons — and in Segoe MDL2 Assets, which uses the same ones.
    pub fn codepoint(self) -> char {
        match self {
            Glyph::Add => '\u{E710}',
            Glyph::More => '\u{E712}',
            Glyph::Search => '\u{E721}',
            Glyph::Play => '\u{E768}',
            Glyph::Pause => '\u{E769}',
            Glyph::Close => '\u{E711}',
            Glyph::Delete => '\u{E74D}',
            Glyph::Folder => '\u{E8B7}',
            Glyph::MoveToFolder => '\u{E8DE}',
            Glyph::Copy => '\u{E8C8}',
            Glyph::Back => '\u{E72B}',
            Glyph::Settings => '\u{E713}',
            Glyph::Help => '\u{E897}',
            Glyph::ChevronDown => '\u{E70D}',
            Glyph::ChevronRight => '\u{E76C}',
            Glyph::Check => '\u{E73E}',
            Glyph::MultiSelect => '\u{E762}',
            Glyph::Rename => '\u{E8AC}',
            Glyph::Info => '\u{E946}',
            Glyph::Error => '\u{EA39}',
            Glyph::Undo => '\u{E7A7}',
        }
    }
}

/// The size Fluent draws an icon at inside a standard control.
pub const SIZE: f32 = 16.0;

/// Whether this machine has an icon font at all. Asked once and remembered: the answer cannot
/// change while the program runs, and asking lays out text.
pub fn available(ctx: &egui::Context) -> bool {
    let key = egui::Id::new("fluent-icons-available");
    if let Some(known) = ctx.data(|d| d.get_temp::<bool>(key)) {
        return known;
    }
    let found = crate::fonts::can_draw(ctx, crate::fonts::fluent_family(), Glyph::Add.codepoint());
    ctx.data_mut(|d| d.insert_temp(key, found));
    found
}

/// Draws `glyph` centred in `rect`, `size` points tall.
pub fn paint(ui: &egui::Ui, rect: Rect, glyph: Glyph, size: f32, ink: Color32) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    if available(ui.ctx()) {
        let galley = ui.painter().layout_no_wrap(
            glyph.codepoint().to_string(),
            egui::FontId::new(size, crate::fonts::fluent_family()),
            ink,
        );
        // Centred on the ink, then snapped to the pixel grid: an icon one half-pixel off is an icon
        // drawn with soft edges.
        let ink_box = galley.mesh_bounds;
        let offset = rect.center() - ink_box.center();
        let ppp = ui.ctx().pixels_per_point();
        let at = egui::pos2(
            (offset.x * ppp).round() / ppp,
            (offset.y * ppp).round() / ppp,
        );
        ui.painter().galley(at, galley, ink);
    } else {
        fallback(ui, rect, glyph, size, ink);
    }
}

/// Allocates a `size`-point square for an icon that is only decoration beside text.
pub fn show(ui: &mut egui::Ui, glyph: Glyph, size: f32, ink: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::splat(size), egui::Sense::hover());
    paint(ui, rect, glyph, size, ink);
    response
}

/// A handful of icons in line segments, for a machine with no icon font. See the module notes.
fn fallback(ui: &egui::Ui, rect: Rect, glyph: Glyph, size: f32, ink: Color32) {
    let r = Rect::from_center_size(rect.center(), egui::Vec2::splat(size));
    let p = |x: f32, y: f32| r.min + egui::vec2(x * size, y * size);
    let s = Stroke::new(1.3, ink);
    let painter = ui.painter();
    match glyph {
        Glyph::Add => {
            painter.line_segment([p(0.5, 0.15), p(0.5, 0.85)], s);
            painter.line_segment([p(0.15, 0.5), p(0.85, 0.5)], s);
        }
        Glyph::Close => {
            painter.line_segment([p(0.2, 0.2), p(0.8, 0.8)], s);
            painter.line_segment([p(0.8, 0.2), p(0.2, 0.8)], s);
        }
        Glyph::More => {
            for x in [0.2, 0.5, 0.8] {
                painter.circle_filled(p(x, 0.5), size * 0.06, ink);
            }
        }
        Glyph::Search => {
            painter.circle_stroke(p(0.42, 0.42), size * 0.28, s);
            painter.line_segment([p(0.63, 0.63), p(0.88, 0.88)], s);
        }
        Glyph::Pause => {
            painter.line_segment([p(0.36, 0.2), p(0.36, 0.8)], Stroke::new(1.8, ink));
            painter.line_segment([p(0.64, 0.2), p(0.64, 0.8)], Stroke::new(1.8, ink));
        }
        Glyph::Play => {
            painter.add(egui::Shape::convex_polygon(
                vec![p(0.3, 0.18), p(0.82, 0.5), p(0.3, 0.82)],
                Color32::TRANSPARENT,
                s,
            ));
        }
        Glyph::ChevronDown => {
            painter.add(egui::Shape::line(vec![p(0.2, 0.36), p(0.5, 0.66), p(0.8, 0.36)], s));
        }
        Glyph::ChevronRight => {
            painter.add(egui::Shape::line(vec![p(0.36, 0.2), p(0.66, 0.5), p(0.36, 0.8)], s));
        }
        Glyph::Back => {
            painter.add(egui::Shape::line(vec![p(0.45, 0.2), p(0.15, 0.5), p(0.45, 0.8)], s));
            painter.line_segment([p(0.15, 0.5), p(0.88, 0.5)], s);
        }
        Glyph::Check => {
            painter.add(egui::Shape::line(vec![p(0.15, 0.52), p(0.4, 0.76), p(0.86, 0.26)], s));
        }
        // The rest always sit beside words that already say what they do.
        _ => {}
    }
}
