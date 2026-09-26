//! Espanso Manager's identity, at the size and contrast of the surface displaying it.
//!
//! Explorer reads the multi-size resource in the executable. The window selects its small and
//! large resources separately; the notification area gets the approved simplified trigger/paused
//! geometry, rasterized at its actual physical size. No icon depends on espansod's runtime files.
//!
//! Tray colours follow the WINDOWS shell, never the app's independently selectable theme. The
//! caller caches Appearance and only rebuilds on a setting, DPI or state change. This module does
//! no polling, starts no threads and never changes settings. Status is shape, not just colour.

use eframe::egui::IconData;
use windows::core::w;
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SM_CXICON, SM_CXSMICON};
use winit::platform::windows::{IconExtWindows, WindowExtWindows};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub face: [u8; 3],
    pub mark: [u8; 3],
    pub edge: [u8; 3],
}

impl Palette {
    pub const LIGHT: Self = Self {
        face: [0x30, 0x26, 0xB8],
        mark: [255, 255, 255],
        edge: [0xAC, 0xA6, 0xFF],
    };
    pub const DARK: Self = Self {
        face: [0xC2, 0xBC, 0xFF],
        mark: [0x28, 0x20, 0x5C],
        edge: [0x8B, 0x82, 0xE6],
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Appearance {
    pub size: u32,
    pub palette: Palette,
}

impl Appearance {
    pub fn current() -> Self {
        // The manager can be open on a different monitor from the notification area. Measuring
        // the manager would choose the wrong resource on mixed-DPI desktops.
        let dpi = unsafe {
            FindWindowW(w!("Shell_TrayWnd"), None)
                .ok()
                .map(|hwnd| GetDpiForWindow(hwnd))
                .filter(|&dpi| dpi != 0)
                .unwrap_or_else(|| GetDpiForSystem().max(96))
        };
        let size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, dpi) }.clamp(8, 128) as u32;
        let palette = if let Some((face, mark)) = crate::theme::contrast_colors() {
            Palette { face, mark, edge: mark }
        } else if crate::theme::is_light_taskbar() {
            Palette::LIGHT
        } else {
            Palette::DARK
        };
        Self { size, palette }
    }

    pub fn tray_icon(self, paused: bool) -> Result<tray_icon::Icon, String> {
        let data = render(self.size, self.palette, paused);
        tray_icon::Icon::from_rgba(data.rgba, data.width, data.height).map_err(|e| e.to_string())
    }
}

pub fn window_fallback() -> Option<IconData> {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/window-fallback.png")).ok()
}

/// winit exposes separate small/title and large/taskbar icons on Windows. Let the resource loader
/// choose the exact supplied size, rather than shrink a 256px keycap into the 16px title bar.
pub fn apply_window(window: &winit::window::Window) -> u32 {
    let dpi = (window.scale_factor() * 96.0).round() as u32;
    let load = |metric| {
        let size = unsafe { GetSystemMetricsForDpi(metric, dpi) }.clamp(8, 256) as u32;
        winit::window::Icon::from_resource(1, Some(winit::dpi::PhysicalSize::new(size, size)))
            .or_else(|_| {
                // Preserve the existing build-without-Windows-SDK behaviour: resources are
                // optional on that developer machine, but the running app still has an icon.
                let data = render(size, Palette::LIGHT, false);
                winit::window::Icon::from_rgba(data.rgba, data.width, data.height)
            })
            .ok()
    };
    window.set_window_icon(load(SM_CXSMICON));
    window.set_taskbar_icon(load(SM_CXICON));
    dpi
}

fn round_rect(x: f32, y: f32, bounds: [f32; 4], radius: f32) -> bool {
    let [left, top, right, bottom] = bounds;
    if x < left || x > right || y < top || y > bottom { return false; }
    let dx = x - x.clamp(left + radius, right - radius);
    let dy = y - y.clamp(top + radius, bottom - radius);
    dx * dx + dy * dy <= radius * radius
}

fn segment_distance_squared(x: f32, y: f32, a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let t = (((x - a[0]) * dx + (y - a[1]) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    (x - a[0] - t * dx).powi(2) + (y - a[1] - t * dy).powi(2)
}

/// The 16-unit geometry of the approved vector companion. Eight samples per axis preserve round
/// corners/diagonals at fractional DPI sizes; accumulating unpremultiplied colour only over covered
/// samples avoids black fringes on light backgrounds. No rescaling of an oversized bitmap occurs.
pub fn render(size: u32, palette: Palette, paused: bool) -> IconData {
    let size = size.clamp(8, 256);
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    let step = 16.0 / (size * 8) as f32;
    for py in 0..size {
        for px in 0..size {
            let mut color = [0u32; 3];
            let mut coverage = 0u32;
            for sy in 0..8 {
                for sx in 0..8 {
                    let x = (px * 8 + sx) as f32 * step + step * 0.5;
                    let y = (py * 8 + sy) as f32 * step + step * 0.5;
                    if !round_rect(x, y, [0.25, 0.25, 15.75, 15.75], 3.25) { continue; }
                    let inside = round_rect(x, y, [0.75, 0.75, 15.25, 15.25], 2.75);
                    let mark = if paused {
                        round_rect(x, y, [4.0, 4.0, 6.5, 12.0], 1.25)
                            || round_rect(x, y, [9.5, 4.0, 12.0, 12.0], 1.25)
                    } else {
                        (x - 5.0).powi(2) + (y - 5.25).powi(2) <= 1.3f32.powi(2)
                            || (x - 5.0).powi(2) + (y - 10.75).powi(2) <= 1.3f32.powi(2)
                            || segment_distance_squared(x, y, [8.6, 4.6], [12.0, 8.0]) <= 0.95f32.powi(2)
                            || segment_distance_squared(x, y, [12.0, 8.0], [8.6, 11.4]) <= 0.95f32.powi(2)
                    };
                    let sample = if mark { palette.mark } else if inside { palette.face } else { palette.edge };
                    for i in 0..3 { color[i] += sample[i] as u32; }
                    coverage += 1;
                }
            }
            match coverage {
                0 => rgba.extend_from_slice(&[0, 0, 0, 0]),
                _ => {
                    for channel in color { rgba.push(((channel + coverage / 2) / coverage) as u8); }
                    rgba.push(((coverage * 255 + 32) / 64) as u8);
                }
            }
        }
    }
    IconData { rgba, width: size, height: size }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_icons_keep_status_and_alpha_across_themes_and_dpi() {
        for size in [16, 20, 24, 28, 32, 40, 48, 64] {
            for palette in [Palette::LIGHT, Palette::DARK, Palette { face: [0; 3], mark: [255, 255, 0], edge: [255, 255, 0] }] {
                let active = render(size, palette, false);
                let paused = render(size, palette, true);
                assert_eq!(active.rgba.len(), (size * size * 4) as usize);
                assert_eq!(&active.rgba[..4], &[0, 0, 0, 0]);
                assert_eq!(&paused.rgba[..4], &[0, 0, 0, 0]);
                assert_ne!(active.rgba, paused.rgba);
                // State must never change the silhouette/transparency of the shared key.
                assert!(active.rgba.as_chunks::<4>().0.iter().zip(paused.rgba.as_chunks::<4>().0).all(|(a, b)| a[3] == b[3]));
                for (x, y) in [(5.0, 5.25), (5.0, 10.75), (11.5, 8.0)] {
                    let x = (x * size as f32 / 16.0) as usize;
                    let y = (y * size as f32 / 16.0) as usize;
                    let at = (y * size as usize + x) * 4;
                    assert_eq!(&active.rgba[at..at + 3], &palette.mark);
                }
            }
        }
    }
}
