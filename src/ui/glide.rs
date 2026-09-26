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

//! Fluid hover: one wash per group of controls, gliding to whichever of them the pointer is on.
//!
//! Controls that stand side by side — the lines of a flyout, the segments of a switch, the
//! borderless buttons of the command bar — do not light up and go dark one by one. The group owns
//! a single wash that fades in where the pointer first lands, glides from control to control while
//! the pointer stays among them, and fades out in place when it leaves. With Windows' animation
//! effects off it still fades; it never travels.
//!
//! **A group is the `Ui` its controls are placed in**, so nothing has to be declared: the lines of
//! one popup, the segments of one track, the buttons of one row share one. The first control of a
//! group drawn each frame moves the wash and paints it *before* painting itself, which is what puts
//! the wash under every control of the group. It moves towards where the pointer was last frame —
//! the group cannot know this frame's answer until its last control has been drawn — so the wash
//! is one frame behind the pointer, which at 60 frames a second nobody sees.
//!
//! Positions are kept relative to the group's first control, so a group that moves as a whole — a
//! popup opening, a dialog sliding, a scroll — carries its wash with it instead of leaving it to
//! catch up.
//!
//! The library's list has its own wash, for its own reason — see `list_view`'s `Glide` — and
//! borrows only the spring from here.
//!
//! What does **not** glide: outlined buttons and chips. Their fill is part of their shape, and a
//! wash sliding between two outlined boxes reads as something coming loose.

use std::collections::HashMap;

/// How quickly a wash reaches its target: a critically damped spring, so it arrives without
/// bouncing. At 34 it covers nine tenths of the way in about 0.11 s.
const OMEGA: f32 = 34.0;
/// How long a wash takes to appear, and to go.
pub const FADE_IN: f32 = 0.10;
pub const FADE_OUT: f32 = 0.16;

/// One step of a critically damped spring, exact for any `dt`: `x` is the distance still to go,
/// `v` the speed. Returns both after `dt` seconds.
pub fn spring_step(x: f32, v: f32, dt: f32) -> (f32, f32) {
    let decay = (-OMEGA * dt).exp();
    let b = v + OMEGA * x;
    ((x + b * dt) * decay, (v - OMEGA * b * dt) * decay)
}

/// What a control said about itself last frame: where it is, relative to its group's first
/// control, and how to wash it. Pointer beats keyboard: the control under the pointer wins over
/// the one Tab is on.
#[derive(Clone, Copy)]
struct Target {
    rect: egui::Rect,
    fill: egui::Color32,
    radius: u8,
    by_pointer: bool,
}

#[derive(Clone, Copy)]
struct Group {
    /// The wash, relative to the group's first control.
    rect: egui::Rect,
    speed: [f32; 4],
    alpha: f32,
    fill: egui::Color32,
    radius: u8,
    /// Where this frame's controls are measured from, and which frame that is.
    origin: egui::Pos2,
    frame: u64,
    /// What the controls reported last frame, which this frame moves towards, and what they are
    /// reporting now, for the next.
    target: Option<Target>,
    reporting: Option<Target>,
}

impl Default for Group {
    fn default() -> Self {
        Self {
            rect: egui::Rect::NOTHING,
            speed: [0.0; 4],
            alpha: 0.0,
            fill: egui::Color32::TRANSPARENT,
            radius: 0,
            origin: egui::Pos2::ZERO,
            frame: 0,
            target: None,
            reporting: None,
        }
    }
}

#[derive(Clone, Default)]
struct Groups(HashMap<egui::Id, Group>);

fn groups_key() -> egui::Id {
    egui::Id::new("glide-groups")
}

/// The call a control makes where it would have painted its own hover fill: `fill` is what it
/// would have painted, `lit` whether it would have painted it — under the pointer, pressed, or
/// with the keyboard on it.
pub fn item(ui: &egui::Ui, response: &egui::Response, radius: u8, lit: bool, fill: egui::Color32) {
    let ctx = ui.ctx();
    let id = egui::Id::new("glide").with(ui.id());
    let frame = ctx.cumulative_pass_nr();
    let rect = response.rect;
    let by_pointer = response.hovered() || response.is_pointer_button_down_on();
    // Everything that asks the context something is asked here, outside `data_mut`: the context
    // is one lock, and asking it anything from inside that closure waits on itself forever.
    let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 1.0 / 20.0);

    let (paint, busy) = ctx.data_mut(|d| {
        let groups = d.get_temp_mut_or_default::<Groups>(groups_key());
        let g = groups.0.entry(id).or_default();
        let mut paint = (None, false);
        if g.frame != frame {
            // The first control of the group this frame: last frame's reports become the target.
            if g.frame + 1 != frame {
                // Not drawn last frame — a menu that was closed. Whatever it held is stale, and a
                // wash must never fly in from where it was left.
                g.alpha = 0.0;
                g.reporting = None;
            }
            g.target = g.reporting.take();
            g.origin = rect.min;
            g.frame = frame;
            paint = step(g, dt);
        }
        if lit {
            let offered = Target {
                rect: rect.translate(-g.origin.to_vec2()),
                fill,
                radius,
                by_pointer,
            };
            if g.reporting.is_none_or(|t| offered.by_pointer || !t.by_pointer) {
                g.reporting = Some(offered);
            }
        }
        paint
    });

    if busy {
        ctx.request_repaint();
    }
    if let Some((rect, fill, radius)) = paint {
        ui.painter().rect_filled(rect, radius, fill);
    }
}

/// Moves one group's wash a frame towards its target. Returns what to paint, in screen space, and
/// whether it is still on its way and needs another frame.
fn step(g: &mut Group, dt: f32) -> (Option<(egui::Rect, egui::Color32, u8)>, bool) {
    match g.target {
        Some(t) => {
            if g.alpha < 0.02 || !crate::theme::animations_enabled() {
                g.rect = t.rect;
                g.speed = [0.0; 4];
            } else {
                let now = [g.rect.left(), g.rect.top(), g.rect.width(), g.rect.height()];
                let to = [t.rect.left(), t.rect.top(), t.rect.width(), t.rect.height()];
                let mut next = [0.0; 4];
                for i in 0..4 {
                    let (x, v) = spring_step(now[i] - to[i], g.speed[i], dt);
                    next[i] = to[i] + x;
                    g.speed[i] = v;
                }
                g.rect = egui::Rect::from_min_size(
                    egui::pos2(next[0], next[1]),
                    egui::vec2(next[2], next[3]),
                );
            }
            g.fill = t.fill;
            g.radius = t.radius;
            g.alpha = (g.alpha + dt / FADE_IN).min(1.0);
        }
        None => g.alpha = (g.alpha - dt / FADE_OUT).max(0.0),
    }

    let moving = g.target.is_some_and(|t| {
        (g.rect.min - t.rect.min).length() > 0.1
            || (g.rect.size() - t.rect.size()).length() > 0.1
            || g.speed.iter().any(|s| s.abs() > 0.5)
    });
    let busy = moving || (g.alpha > 0.0 && g.alpha < 1.0);
    let paint = (g.alpha > 0.0).then(|| {
        (
            g.rect.translate(g.origin.to_vec2()),
            g.fill.gamma_multiply(g.alpha),
            g.radius,
        )
    });
    (paint, busy)
}

/// Called once, after everything has been drawn. A group whose controls just reported something
/// other than what its wash is heading for needs one more frame to act on it — the pointer
/// leaving the window is a single event, and without this the wash would stay lit until the next
/// one. Groups not drawn for a while are forgotten.
pub fn end_frame(ctx: &egui::Context) {
    let frame = ctx.cumulative_pass_nr();
    let changed = ctx.data_mut(|d| {
        let groups = d.get_temp_mut_or_default::<Groups>(groups_key());
        groups.0.retain(|_, g| g.frame + 120 > frame);
        groups.0.values().any(|g| {
            g.frame == frame
                && g.target.map(|t| (t.rect, t.fill)) != g.reporting.map(|t| (t.rect, t.fill))
        })
    });
    if changed {
        ctx.request_repaint();
    }
}
