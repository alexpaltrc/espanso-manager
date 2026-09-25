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

//! The system faces the interface is set in, loaded from `C:\Windows\Fonts` at runtime.
//!
//! Nothing is embedded in the executable — a portable app that carried 8 MB of fonts would be a
//! worse trade than reading them off the machine that is already holding them. Text is Segoe UI
//! Variable, the face Windows 11 draws its own interface in, with plain Segoe UI behind it where
//! it is missing; triggers are Cascadia Mono, or Consolas. The Devanagari face is a last resort,
//! and the symbols one is asked *before* the bundled emoji face, because Windows draws emoji
//! better than the little face egui ships. The interface's own icons sit out of all of it in their
//! own family — see [`install`].
//!
//! Called again on every language change, which is the moment the Devanagari face is picked up or
//! let go: it is 5.3 MB held for the life of the process, on a program that sits in the tray all
//! day, so it is loaded only when Hindi is actually the chosen language.

use eframe::egui;

/// **Devanagari**, because the language picker has to be able to *spell* हिन्दी — showing it as a
/// row of empty boxes would leave exactly the people who need that option unable to recognise it.
/// Windows ships Nirmala UI as a font *collection*; face 0 covers Devanagari and Latin both.
///
/// Loaded only when Hindi is the chosen language: Nirmala.ttc is 5.3 MB, it is held for the life of
/// the process, and this app sits in the tray all day on machines that will never draw a single
/// Devanagari character. [`crate::i18n::Lang::picker_label`] is what makes that affordable — it
/// spells the language in Latin whenever the glyphs are not there to spell it properly.
const DEVANAGARI: &[&str] = &[
    "C:\\Windows\\Fonts\\Nirmala.ttc",
    "C:\\Windows\\Fonts\\mangal.ttf",
];

/// **Symbols**, because expansions are people's own text and arrows turn up in it constantly —
/// `→` in a note about a workflow, `←` in a keyboard shortcut. egui bundles Latin and emoji and
/// nothing in between, so an arrow came out as an empty box in the list and in the editor: the app
/// appeared to have mangled text it had in fact stored perfectly. Segoe UI Symbol is the broad one;
/// plain Segoe UI carries the common arrows if it is missing.
///
/// It also turned out to be the answer to emoji. egui 0.36.1 cannot draw a colour emoji at all —
/// its rasteriser reads outlines and paints them white, so `seguiemj.ttf` and its 7 MB of colour
/// layers are 12 MB of nothing (measured: identical layout, identical pixels). Segoe UI Symbol
/// draws the same characters as monochrome line art, at the size Windows meant them to be read at,
/// and against egui's own bundled emoji face it wins clearly — measured side by side at 14 px, the
/// bundled `👍` is an illegible tangle and its `💡` a bare ring, because that face is shrunk to
/// 0.81 and its artwork is thin outlines. Hence [`Place::BeforeEmoji`].
///
/// Always loaded. It is 2.5 MB and it is reachable from anybody's text in any language.
const SYMBOLS: &[&str] = &[
    "C:\\Windows\\Fonts\\seguisym.ttf",
    "C:\\Windows\\Fonts\\segoeui.ttf",
];

/// The family the interface draws its own icons with: the pencil, the wastebasket, the tick, the
/// cross, the plus, the back arrow. It is egui's bundled chain and nothing else, which is the whole
/// point — see [`install`] for why a face read off the machine must not reach it.
const ICONS: &str = "icons";

/// Segoe UI Variable: one file, both weights. See [`install`].
const TEXT_VARIABLE: &str = "C:\\Windows\\Fonts\\SegUIVar.ttf";
/// Windows 10's pair, when the variable face is not there.
const TEXT_REGULAR: &str = "C:\\Windows\\Fonts\\segoeui.ttf";
const TEXT_SEMIBOLD: &str = "C:\\Windows\\Fonts\\seguisb.ttf";
/// Windows 11's icon font, then Windows 10's.
const FLUENT_ICONS: &[&str] = &[
    "C:\\Windows\\Fonts\\SegoeIcons.ttf",
    "C:\\Windows\\Fonts\\segmdl2.ttf",
];
const MONO: &[&str] = &[
    "C:\\Windows\\Fonts\\CascadiaMono.ttf",
    "C:\\Windows\\Fonts\\consola.ttf",
];

/// Titles and the few words that name a thing: Segoe UI at Semibold.
const SEMIBOLD: &str = "semibold";
/// Segoe Fluent Icons. See [`crate::ui::glyphs`].
const FLUENT: &str = "fluent-icons";

pub fn semibold_family() -> egui::FontFamily {
    egui::FontFamily::Name(SEMIBOLD.into())
}

pub fn fluent_family() -> egui::FontFamily {
    egui::FontFamily::Name(FLUENT.into())
}

fn weight(wght: f32) -> egui::FontTweak {
    egui::FontTweak {
        coords: egui::epaint::text::VariationCoords::new([(b"wght", wght)]),
        ..Default::default()
    }
}

/// A system font's bytes, read once for the life of the process.
///
/// [`install`] runs again on every language change, and every face it installs used to be read off
/// the disk and copied into a fresh buffer each time. These files never change under a running
/// program, so each is read once and kept; the weights of the variable face then share one copy.
fn cached_bytes(path: &'static str) -> Option<&'static [u8]> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static READ: OnceLock<Mutex<HashMap<&'static str, Option<&'static [u8]>>>> = OnceLock::new();
    let mut read = READ.get_or_init(Default::default).lock().ok()?;
    *read.entry(path).or_insert_with(|| {
        std::fs::read(path)
            .ok()
            .map(|bytes| &*Box::leak(bytes.into_boxed_slice()))
    })
}

/// The family for the interface's own icons. See [`ICONS`].
pub fn icons_family() -> egui::FontFamily {
    egui::FontFamily::Name(ICONS.into())
}

/// An interface icon: the same text, drawn from the faces the app ships rather than from whatever
/// the machine happens to have.
///
/// Safe to wrap a whole label in — `"+ New expansion"` and the like — because the bundled Latin
/// face comes first in this chain exactly as it does in the proportional one, so the words are
/// drawn by the same face either way and only the icon is pinned.
pub fn icon(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).family(icons_family())
}

/// Which of the two jobs was asked for, and whether it found a face on this machine.
///
/// One `bool` for both is what made the startup banner wrong. A machine with no Devanagari font and
/// a machine with no symbols font gave the same answer, and the caller turned either into "Hindi
/// was selected, but…" — shown, in red, at every single start, to people who had not selected it
/// and were never going to.
#[derive(Default, Clone, Copy)]
pub struct FontStatus {
    /// True only when the Devanagari face was wanted and none of its candidates was on the machine,
    /// which is to say: only ever for somebody actually reading Hindi.
    pub devanagari_missing: bool,
    /// True when neither symbols candidate was there. Nothing warns about this, deliberately: both
    /// are core Windows UI faces — `segoeui.ttf` is what the shell draws its own menus with — so a
    /// machine without either cannot render its own Start menu, and a banner from this app would
    /// not be the news. Recorded rather than dropped, so it is already here the day somebody
    /// decides it deserves a message.
    pub symbols_missing: bool,
}

/// Installs the font set the interface needs. Called once at startup, and again whenever the
/// language changes — which is when the Devanagari face is picked up or let go.
///
/// Nothing is bundled into the executable, so the portable build stays small; and egui only
/// rasterises the glyphs actually drawn, so a fallback face nobody's text reaches costs no atlas
/// space. The file bytes themselves are held for as long as the face is installed, which is the
/// whole reason the Devanagari one is conditional.
pub fn install(ctx: &egui::Context, want_devanagari: bool) -> FontStatus {
    let mut fonts = egui::FontDefinitions::default();
    let bundled_chain = fonts
        .families
        .get(&egui::FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    // The text face: Segoe UI Variable, which is what Windows 11 sets its own interface in, at
    // Regular for text and Semibold for titles — Fluent uses no other weights. It is one variable
    // file, so both weights are the same bytes read once and pinned to two points on its `wght` axis.
    // Windows 10 does not have it; there the static Segoe UI pair stands in, which is what that
    // system sets its own interface in.
    let (regular, semibold) = match cached_bytes(TEXT_VARIABLE) {
        Some(bytes) => (
            Some(egui::FontData::from_static(bytes).tweak(weight(400.0))),
            Some(egui::FontData::from_static(bytes).tweak(weight(600.0))),
        ),
        None => (
            cached_bytes(TEXT_REGULAR).map(egui::FontData::from_static),
            cached_bytes(TEXT_SEMIBOLD).map(egui::FontData::from_static),
        ),
    };
    if let Some(face) = regular {
        fonts.font_data.insert("studio-text".into(), std::sync::Arc::new(face));
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "studio-text".into());
    }
    let mut strong_chain = bundled_chain.clone();
    if let Some(face) = semibold {
        fonts.font_data.insert("studio-text-semibold".into(), std::sync::Arc::new(face));
        strong_chain.insert(0, "studio-text-semibold".into());
    } else if fonts.font_data.contains_key("studio-text") {
        strong_chain.insert(0, "studio-text".into());
    }
    fonts
        .families
        .insert(egui::FontFamily::Name(SEMIBOLD.into()), strong_chain);

    // The interface's pictograms: Segoe Fluent Icons, or on Windows 10 the MDL2 set it grew out of —
    // the same codepoints, drawn by the same hand. The bundled chain follows only so that
    // [`can_draw`] has a replacement box to compare against; see [`crate::ui::glyphs`].
    let mut fluent_chain = bundled_chain;
    if let Some((path, bytes)) = FLUENT_ICONS.iter().find_map(|p| cached_bytes(p).map(|b| (p, b))) {
        let name = format!("fluent-{}", path.rsplit('\\').next().unwrap_or("icons"));
        fonts.font_data.insert(name.clone(), std::sync::Arc::new(egui::FontData::from_static(bytes)));
        fluent_chain.insert(0, name);
    }
    fonts
        .families
        .insert(egui::FontFamily::Name(FLUENT.into()), fluent_chain);

    // Triggers are code-like, and Windows' own code face is Cascadia Mono (Consolas before it).
    // Both sit beside Segoe UI far better than the bundled Hack, which is drawn to another measure.
    if let Some((path, bytes)) = MONO.iter().find_map(|p| cached_bytes(p).map(|b| (p, b))) {
        let face = egui::FontData::from_static(bytes);
        let face = if path.contains("Cascadia") { face.tweak(weight(400.0)) } else { face };
        fonts.font_data.insert("studio-mono".into(), std::sync::Arc::new(face));
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "studio-mono".into());
    }

    let mut status = FontStatus::default();

    // The interface's own icons get their own family, taken now, while `fonts` still holds nothing
    // but what egui bundles. What comes after adds the symbol face to the two ordinary families only
    // and leaves this one alone, so the pencil and the wastebasket look the same on every machine.
    // A script face still reaches this family — see [`add_face`] — because the buttons draw their
    // labels here and a label has to be readable before its glyph can be pretty.
    //
    // That is not tidiness, it is the price of the line below: Segoe UI Symbol has to go *ahead* of
    // the bundled emoji face for people's own emoji to be legible, and it draws a pencil too — lying
    // flat, reading as a grey capsule at 14 px, measured side by side against the bundled one. The
    // reordering is worth it for `👍` and `💡` and unacceptable for the buttons, so the buttons opt
    // out. Latin is unaffected either way: the bundled text face is first in both chains.
    let bundled = fonts
        .families
        .get(&egui::FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    fonts
        .families
        .insert(egui::FontFamily::Name(ICONS.into()), bundled);

    if want_devanagari {
        status.devanagari_missing = !add_face(&mut fonts, DEVANAGARI, Place::Last);
    }
    status.symbols_missing = !add_face(&mut fonts, SYMBOLS, Place::BeforeEmoji);

    ctx.set_fonts(fonts);
    status
}

/// The little box egui substitutes when no installed face owns a character. Named here because
/// [`can_draw`] has to recognise it, and because `◻` in a source file is easy to mistake for a
/// character that failed to load — it is `U+25FB WHITE MEDIUM SQUARE`, and epaint picks it in
/// `CachedFamily::new`.
const REPLACEMENT_GLYPH: char = '\u{25FB}';

/// Whether the installed fonts can really draw `c` — as opposed to standing in for it with the
/// little box, or drawing nothing at all.
///
/// **`egui::Fonts::has_glyph` cannot be used for this, and the reason is not obvious.** It answers
/// by resolving which face in the family owns `c` and comparing that against the face the family
/// happened to fall back on for its *own* replacement glyph. Every character owned by that one
/// face is therefore reported missing. In the proportional family the face that owns `◻` is egui's
/// bundled NotoEmoji — which is also the only face in the chain that owns any emoji at all. So
/// `has_glyph` denies `⚠`, `❗`, `❤`, `✅`, `💡`, `😀` and every other emoji, all of which this app
/// draws perfectly well: measured against the fonts' own character tables, and against the atlas
/// rectangle each one actually gets. The monospace family answers correctly only by accident, Hack
/// being the face that owns `◻` there, and asking *it* instead would be worse — it would start
/// claiming glyphs that live only in Hack and come out as empty boxes in an ordinary button, which
/// is the exact bug the caller exists to prevent.
///
/// So this asks the one question that cannot lie: lay the character out, and look at which glyph
/// egui reached for. Two rejections, not one — the box means "no face has it", and an empty
/// rectangle means a face claimed it and then drew nothing, which on screen is a silent gap rather
/// than a visible box, and is the worse of the two.
///
/// Costs two text layouts, both of which land in egui's galley cache; callers that ask every frame
/// should still cache the answer, since the point of asking is usually to build a label.
pub fn can_draw(ctx: &egui::Context, family: egui::FontFamily, c: char) -> bool {
    if c == REPLACEMENT_GLYPH {
        return true;
    }
    // The family matters: the same character can be drawn by one and missing from another, and the
    // answer is only worth anything if it is about the family the caller is going to draw in.
    let font_id = egui::FontId::new(14.0, family);

    let first_glyph_uv = |ch: char| {
        ctx.fonts_mut(|f| {
            let galley = f.layout_no_wrap(ch.to_string(), font_id.clone(), egui::Color32::WHITE);
            let glyph = galley.rows.iter().flat_map(|row| row.glyphs.iter()).next()?;
            let uv = glyph.uv_rect;
            // Zero-sized means no atlas allocation: the face claimed the character and drew
            // nothing.
            (uv.size.x > 0.0 && uv.size.y > 0.0).then_some(uv.min)
        })
    };
    match (first_glyph_uv(c), first_glyph_uv(REPLACEMENT_GLYPH)) {
        (Some(theirs), Some(box_glyph)) => theirs != box_glyph,
        // No box to compare against means the family could not even find its own fallback, so
        // there is nothing here worth trusting; the caller's plain-text fallback is safer.
        _ => false,
    }
}

/// The face in egui's own bundle that owns every emoji, and [`Place::BeforeEmoji`]'s landmark.
/// Named by string because that is how `FontDefinitions` names it; if egui ever renames it, the
/// `position` below finds nothing and the face is appended, which is the old behaviour rather than
/// a crash.
const BUNDLED_EMOJI_FACE: &str = "NotoEmoji-Regular";

/// Where a system face goes in a family's fallback chain.
enum Place {
    /// Last, consulted only for characters nothing else can draw.
    Last,
    /// Ahead of egui's bundled emoji face, so this face's drawing of a character wins — while Latin
    /// still resolves in the bundled text face, which comes first and is left where it is.
    BeforeEmoji,
}

/// Adds the first of `candidates` that exists on this machine. `false` if none of them does.
///
/// Both families every time, because a trigger is monospaced and its replacement is not, and
/// either may contain anything at all.
fn add_face(fonts: &mut egui::FontDefinitions, candidates: &[&str], place: Place) -> bool {
    let Some((name, bytes)) = load_first_available(candidates) else {
        return false;
    };
    fonts.font_data.insert(
        name.clone(),
        std::sync::Arc::new(egui::FontData::from_owned(bytes)),
    );
    // A `Place::Last` face goes into the icon family as well. That family exists to keep Segoe UI
    // Symbol from winning the pencil — see [`install`] — not to keep whole *scripts* out, and a face
    // consulted only for characters nothing else can draw cannot change how the pencil looks.
    // Leaving it out is what turned every button label into empty boxes the moment the language was
    // Hindi: the buttons pin their text to this family, and Devanagari was only ever added to the
    // other two. A `Place::BeforeEmoji` face still stays out, which is the whole point of the family.
    let mut families = vec![
        egui::FontFamily::Proportional,
        egui::FontFamily::Monospace,
        egui::FontFamily::Name(SEMIBOLD.into()),
    ];
    if matches!(place, Place::Last) {
        families.push(egui::FontFamily::Name(ICONS.into()));
    }
    for family in families {
        let chain = fonts.families.entry(family).or_default();
        let at = match place {
            Place::Last => chain.len(),
            Place::BeforeEmoji => chain
                .iter()
                .position(|face| face == BUNDLED_EMOJI_FACE)
                .unwrap_or(chain.len()),
        };
        chain.insert(at, name.clone());
    }
    true
}

fn load_first_available(paths: &[&str]) -> Option<(String, Vec<u8>)> {
    for path in paths {
        if let Ok(bytes) = std::fs::read(path) {
            let name = std::path::Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "system-fallback-font".to_string());
            return Some((name, bytes));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs one egui pass, which is what makes the font atlas exist at all, and hands the atlas
    /// delta back so dropping it does not panic.
    fn pass(ctx: &egui::Context, mut body: impl FnMut(&egui::Context)) {
        let mut out = ctx.run_ui(Default::default(), |ui| body(ui.ctx()));
        out.textures_delta.clear();
    }

    /// The glyphs the interface reaches for, and the trap that used to hide them.
    ///
    /// Deliberately does **not** call [`install`]: every character below lives in a face egui
    /// bundles itself, so this test does not care which fonts the machine has, and it fails only
    /// if egui's own font set or its fallback logic changes.
    ///
    /// `has_glyph` is asserted *wrong* on purpose. The day it starts telling the truth is the day
    /// [`can_draw`] can be deleted, and this is the thing that will say so.
    #[test]
    fn can_draw_sees_the_emoji_has_glyph_denies() {
        let ctx = egui::Context::default();
        pass(&ctx, |_| {});
        pass(&ctx, |ctx| {
            let proportional = egui::FontId::proportional(14.0);

            for c in ['\u{26A0}', '\u{2757}', '\u{1F4A1}', '\u{1F600}', '\u{2705}'] {
                assert!(can_draw(ctx, egui::FontFamily::Proportional, c), "{c:?} is in NotoEmoji and does draw");
                assert!(
                    !ctx.fonts_mut(|f| f.has_glyph(&proportional, c)),
                    "{c:?}: has_glyph has started telling the truth — see `can_draw`"
                );
            }

            assert!(can_draw(ctx, egui::FontFamily::Proportional, 'a'));
            assert!(can_draw(ctx, egui::FontFamily::Proportional, '\u{00F1}'), "ñ");
            // No face has it: egui substitutes the box.
            assert!(!can_draw(ctx, egui::FontFamily::Proportional, '\u{1FB00}'));
            // A face claims it and draws nothing, which is the worse of the two failures.
            assert!(!can_draw(ctx, egui::FontFamily::Proportional, '\u{E0000}'));
        });
    }
}

