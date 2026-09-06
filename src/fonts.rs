//! The two system faces egui does not bundle, loaded from `C:\Windows\Fonts` at runtime.
//!
//! Nothing is embedded in the executable — a portable app that carried 8 MB of fonts would be a
//! worse trade than reading them off the machine that is already holding them. egui's own bundled
//! faces stay in charge of Latin and emoji; these are appended as fallbacks, consulted only for
//! characters the bundled ones cannot draw.
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
/// Always loaded. It is 2.5 MB and it is reachable from anybody's text in any language.
const SYMBOLS: &[&str] = &[
    "C:\\Windows\\Fonts\\seguisym.ttf",
    "C:\\Windows\\Fonts\\segoeui.ttf",
];

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
    let mut status = FontStatus::default();

    if want_devanagari {
        status.devanagari_missing = !add_face(&mut fonts, DEVANAGARI);
    }
    status.symbols_missing = !add_face(&mut fonts, SYMBOLS);

    ctx.set_fonts(fonts);
    status
}

/// Adds the first of `candidates` that exists on this machine. `false` if none of them does.
fn add_face(fonts: &mut egui::FontDefinitions, candidates: &[&str]) -> bool {
    let Some((name, bytes)) = load_first_available(candidates) else {
        return false;
    };
    fonts.font_data.insert(
        name.clone(),
        std::sync::Arc::new(egui::FontData::from_owned(bytes)),
    );
    // Appended rather than prepended: the bundled fonts stay in charge of Latin and emoji, and
    // these are consulted only for characters they cannot draw. Both families, because a trigger is
    // monospaced and its replacement is not, and either may contain anything.
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push(name.clone());
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
