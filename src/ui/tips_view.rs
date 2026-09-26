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

//! Task-oriented quick guide. Short, scannable steps describe the current interface.
//! Read-only except for explicit navigation; all examples are illustrative, never saved.
//!
//! **Organised by what the reader came to do**, not by feature: one lead card for the only thing a
//! first-time reader needs (make an expansion), then one collapsed heading per task. Eight questions
//! that fit on a screen beat three open cards that do not, because the reader is here with a
//! question already in mind and is scanning for it.
//!
//! What goes in a body must be true of the interface *as it stands*. This screen is the easiest one
//! in the app to leave behind: nothing breaks when it lies, so nothing tells us it has started to.
//! Where a sentence names something the user will look for — a settings group, the pause button —
//! it is filled in from [`crate::i18n`] rather than retyped here, so renaming it in one place does
//! not leave four translations pointing at a name that no longer exists.

use crate::app::{AppState, View};
use super::{controls, studio};
use super::studio::text;
use super::keys;
use super::glyphs::{self, Glyph};

/// One task: a heading you can open, with the steps inside.
///
/// Closed by default and unindented, so the answer starts at the same left edge as the question.
/// The rule goes *between* topics, never above the first or below the last: a line with nothing
/// under it reads as an item that failed to draw.
fn topic(ui: &mut egui::Ui, first: bool, title: &str, contents: impl FnOnce(&mut egui::Ui)) {
    if !first {
        // The separator brings its own few pixels either side; adding a full gap on top made eight
        // closed headings take a screen and a half.
        ui.add_space(controls::GAP_TIGHT);
        ui.separator();
        ui.add_space(controls::GAP_TIGHT);
    }
    // Drawn here rather than by egui's CollapsingHeader, whose triangle is nobody's icon family: the
    // chevron is the one Windows puts on an expander, pointing where the answer will open.
    let id = ui.make_persistent_id(("topic", title));
    let mut open = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), controls::FIELD_HEIGHT),
        egui::Sense::click(),
    );
    controls::subtle_wash(ui, &response, true);
    if ui.is_rect_visible(rect) {
        let ink = ui.visuals().text_color();
        let icon = egui::Rect::from_min_size(
            egui::pos2(rect.left() + controls::GAP, rect.center().y - CHEVRON * 0.5),
            egui::Vec2::splat(CHEVRON),
        );
        let glyph = if open.is_open() { Glyph::ChevronDown } else { Glyph::ChevronRight };
        glyphs::paint(ui, icon, glyph, CHEVRON, ink);
        let words = controls::widget_line(ui, egui::RichText::new(title).strong(), egui::TextStyle::Body);
        let at = egui::pos2(icon.right() + controls::GAP_ROW, rect.center().y - words.size().y * 0.5);
        ui.painter()
            .with_clip_rect(rect.intersect(ui.clip_rect()))
            .galley(at, words, ink);
        controls::focus_ring(ui, &response, f32::from(controls::RADIUS_CONTROL));
    }
    if response.clicked() {
        open.toggle(ui);
    }
    open.show_body_unindented(ui, |ui| {
        ui.add_space(controls::GAP);
        contents(ui);
        ui.add_space(controls::GAP_TIGHT);
    });
}

/// The expander's chevron: smaller than a command's icon, because it marks a heading rather than
/// being a button of its own.
const CHEVRON: f32 = 12.0;

/// A paragraph of a topic, in the reading colour. The guide is the one screen made entirely of
/// prose; setting all of it in the supporting grey would make the whole page look like a footnote.
fn para(ui: &mut egui::Ui, body: &str) {
    ui.add(egui::Label::new(body).wrap());
}

/// A named part inside a topic that holds more than one answer.
fn part(ui: &mut egui::Ui, title: &str, body: &str) {
    ui.add_space(controls::GAP_ROW);
    ui.add(egui::Label::new(controls::field_label(title)).wrap());
    ui.add_space(controls::GAP_TIGHT);
    para(ui, body);
}

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let t = state.t();
    // The name the «…» menu gave it, so the way in and the page agree about what this is.
    let title = t.tips_button_tip;
    // Drawn above the scroll area on purpose: the way back must not be something you have to scroll
    // to the end of the guide to find.
    if controls::page_header(ui, t.back, title, state.settings.lang) {
        state.view = View::List;
    }
    controls::page_scroll(ui, "quick-guide", |ui| {
            // Prose sets badly in a very wide column; past this the line becomes hard to track back.
            // A cap, not a width: `set_max_width` takes the number as given.
            ui.set_max_width(ui.available_width().min(820.0));
            lead(ui, state);
            ui.add_space(28.0);
            tasks(ui, state);
            // Where the work underneath comes from, once, at the end of the page that explains it.
            ui.add_space(28.0);
            let is_light = !ui.visuals().dark_mode;
            ui.add(egui::Label::new(controls::small_muted(t.credits_espanso, is_light)).wrap());
            // The window's own gutter is drawn around the scroll area, not inside it.
            ui.add_space(controls::GAP_STACK);
        });
}

/// The one thing somebody opening this screen for the first time is here for, shown rather than
/// described: a trigger, an arrow, and the text it turns into.
fn lead(ui: &mut egui::Ui, state: &mut AppState) {
    let t = state.t();
    let is_light = !ui.visuals().dark_mode;
    let heading = text(
        state,
        "Escribe menos, di lo mismo",
        "Type less, say the same",
        "Mas kaunting tipa, parehong sinasabi",
        "कम लिखें, वही कहें",
    );
    let body = text(
        state,
        "Eliges un atajo corto, escribes el texto una sola vez y, a partir de ahí, Espanso lo completa por ti en cualquier programa.",
        "You pick a short shortcut, write the text once, and from then on Espanso completes it for you in any program.",
        "Pumili ka ng maikling shortcut, isulat ang teksto nang minsanan, at mula noon ay kukumpletuhin ito ng Espanso para sa iyo sa kahit anong programa.",
        "एक छोटा शॉर्टकट चुनें, टेक्स्ट एक बार लिखें, और उसके बाद Espanso उसे किसी भी प्रोग्राम में आपके लिए पूरा कर देगा।",
    );
    let trigger = text(state, ":hola", ":hi", ":kumusta", ":namaste");
    let result = text(
        state,
        "Hola, ¿en qué puedo ayudarte?",
        "Hi, how can I help you?",
        "Kumusta, paano kita matutulungan?",
        "नमस्ते, मैं आपकी कैसे मदद कर सकता हूँ?",
    );
    let note = crate::i18n::fill(
        text(
            state,
            "Los dos puntos son el prefijo: evitan que una palabra corriente dispare una expansión. Se cambia en {settings}.",
            "The colon is the prefix: it keeps an ordinary word from firing an expansion. You change it in {settings}.",
            "Ang tutuldok ang prefix: pinipigilan nitong ma-trigger ng karaniwang salita ang isang expansion. Mababago ito sa {settings}.",
            "कोलन उपसर्ग है: यह किसी आम शब्द को विस्तार चलाने से रोकता है। इसे {settings} में बदला जाता है।",
        ),
        &[("settings", t.settings_title)],
    );

    let mut creating = false;
    ui.vertical(|ui| {
        ui.add(egui::Label::new(controls::h3(heading)).wrap());
        ui.add_space(controls::GAP_TIGHT);
        ui.add(egui::Label::new(controls::muted(body, is_light)).wrap());
        ui.add_space(controls::GAP_ROW);
        controls::inset_frame(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let written = controls::code(trigger, ui.visuals());
            let arrow = controls::muted("→", is_light);
            ui.horizontal_wrapped(|ui| {
                ui.label(written);
                ui.label(arrow);
                ui.add(egui::Label::new(result).wrap());
            });
        });
        ui.add_space(controls::GAP_ROW);
        ui.add(egui::Label::new(controls::small_muted(note, is_light)).wrap());
        ui.add_space(controls::GAP_ROW);
        creating = controls::primary_button(ui, t.new_expansion).clicked();
    });
    // Acted on after the frame closes: [`studio::create_expansion`] moves the whole screen out from
    // under us, and doing that half way through painting this card is how a frame ends up drawn
    // against a view that no longer exists.
    if creating {
        studio::create_expansion(ui.ctx(), state);
    }
}

/// Everything else, one heading per task, in the order the plan lists them.
fn tasks(ui: &mut egui::Ui, state: &AppState) {
    let t = state.t();
    ui.vertical(|ui| {
        topic(
            ui,
            true,
            text(state, "Encontrar y editar", "Find and edit", "Hanapin at i-edit", "खोजें और संपादित करें"),
            |ui| {
                para(ui, text(
                    state,
                    "Busca por el atajo o por el texto que inserta; Ctrl+F lleva el cursor al buscador. Abre una fila para leer el texto completo: Editar y Eliminar están dentro de ese detalle, no en cada fila.",
                    "Search by the shortcut or by the text it inserts; Ctrl+F puts the cursor in the search box. Open a row to read the whole text: Edit and Delete live inside that detail, not on every row.",
                    "Maghanap gamit ang shortcut o ang tekstong ipinapasok nito; dinadala ng Ctrl+F ang cursor sa paghahanap. Buksan ang isang row para mabasa ang buong teksto: nasa loob ng detalye ang I-edit at Tanggalin, hindi sa bawat row.",
                    "शॉर्टकट से या उसके डाले जाने वाले टेक्स्ट से खोजें; Ctrl+F कर्सर को खोज बॉक्स में ले जाता है। पूरा टेक्स्ट पढ़ने के लिए पंक्ति खोलें: संपादित करें और हटाएँ उसी विवरण के अंदर हैं, हर पंक्ति पर नहीं।",
                ));
            },
        );

        let folders_body = crate::i18n::fill(
            text(
                state,
                "El selector {all}, junto al buscador, muestra solo una carpeta o {none}. Para mover varias expansiones de una vez, elige {select} en el menú «…»; para mover una sola, arrástrala hasta una carpeta o usa el menú de su detalle. En el mismo selector están {new} y {options}, donde puedes cambiarle el nombre o el prefijo, exportarla o eliminarla.",
                "The {all} picker, beside the search box, shows just one folder or {none}. To move several expansions at once, choose {select} in the «…» menu; to move one, drag it onto a folder or use the menu in its detail. The same picker holds {new} and {options}, where you can rename a folder, change its prefix, export it or delete it.",
                "Ang {all} na pagpipilian, katabi ng paghahanap, ay nagpapakita ng isang folder lang o {none}. Para maglipat ng ilang expansion nang sabay, piliin ang {select} sa menu na «…»; para sa isa, i-drag ito sa isang folder o gamitin ang menu sa detalye nito. Nasa parehong pagpipilian ang {new} at {options}, kung saan mapapalitan ang pangalan o prefix, mai-e-export o matatanggal ang folder.",
                "खोज के बगल वाला {all} चयनकर्ता केवल एक फ़ोल्डर या {none} दिखाता है। एक साथ कई विस्तार ले जाने के लिए «…» मेनू में {select} चुनें; एक को ले जाने के लिए उसे फ़ोल्डर पर खींचें या उसके विवरण का मेनू इस्तेमाल करें। उसी चयनकर्ता में {new} और {options} हैं, जहाँ फ़ोल्डर का नाम या उपसर्ग बदल सकते हैं, निर्यात या हटा सकते हैं।",
            ),
            &[
                ("all", text(state, "Todas las carpetas", "All folders", "Lahat ng folder", "सभी फ़ोल्डर")),
                ("none", t.no_folder),
                ("select", text(state, "Seleccionar varias", "Select several", "Pumili ng ilan", "कई चुनें")),
                ("new", t.add_new_folder.trim_start_matches(['+', ' ']).trim_end_matches('…')),
                ("options", text(state, "Opciones de carpeta", "Folder options", "Mga opsyon ng folder", "फ़ोल्डर विकल्प")),
            ],
        );
        topic(
            ui,
            false,
            text(state, "Organizar en carpetas", "Organize into folders", "Ayusin sa mga folder", "फ़ोल्डर में व्यवस्थित करें"),
            |ui| {
                para(ui, &folders_body);
                ui.add_space(controls::GAP);
                // Said plainly, because the confirmation is the only other place it is said and by
                // then the user is already deciding.
                para(ui, text(
                    state,
                    "Eliminar una carpeta elimina también las expansiones que contiene; la confirmación te dice cuántas son.",
                    "Deleting a folder also deletes the expansions inside it; the confirmation tells you how many.",
                    "Ang pagtanggal ng folder ay tinatanggal din ang mga expansion sa loob nito; sinasabi ng kumpirmasyon kung ilan.",
                    "फ़ोल्डर हटाने पर उसके अंदर के विस्तार भी हट जाते हैं; पुष्टि बताती है कि कितने।",
                ));
            },
        );

        topic(
            ui,
            false,
            text(state, "Usar fechas", "Use dates", "Gumamit ng petsa", "तारीख़ का उपयोग"),
            |ui| {
                para(ui, text(
                    state,
                    "Al crear una expansión elige Fecha en lugar de Texto. Hay cuatro formatos listos y uno que armas por bloques, con su idioma y su zona horaria. La vista previa es un ejemplo: la fecha se calcula cada vez que escribes el atajo, no cuando lo guardas.",
                    "When you create an expansion, choose Date instead of Text. There are four ready-made formats and one you build from blocks, with its own language and time zone. The preview is an example: the date is worked out each time you type the shortcut, not when you save it.",
                    "Kapag gumagawa ng expansion, piliin ang Petsa sa halip na Teksto. May apat na handang format at isa na binubuo mo sa mga block, may sariling wika at time zone. Halimbawa lang ang preview: kinakalkula ang petsa sa tuwing iti-type mo ang shortcut, hindi kapag nag-save ka.",
                    "विस्तार बनाते समय टेक्स्ट के बजाय तारीख़ चुनें। चार तैयार प्रारूप हैं और एक जिसे आप ब्लॉक से बनाते हैं, अपनी भाषा और समय क्षेत्र के साथ। पूर्वावलोकन एक उदाहरण है: तारीख़ हर बार शॉर्टकट लिखने पर बनती है, सहेजते समय नहीं।",
                ));
            },
        );

        let share_body = crate::i18n::fill(
            text(
                state,
                "En {settings} → {transfer} eliges qué carpetas exportar a un archivo; también puedes exportar una sola desde {options}. Quien lo reciba lo importa desde {settings}: se conservan las carpetas y los atajos que ya tenga se omiten, nunca se reemplazan.",
                "In {settings} → {transfer} you choose which folders to export to a file; you can also export just one from {options}. Whoever receives it imports it from {settings}: folders are kept, and shortcuts they already have are skipped, never replaced.",
                "Sa {settings} → {transfer}, pipiliin mo kung aling mga folder ang ie-export sa isang file; puwede ring isa lang mula sa {options}. Ini-import ito ng tatanggap mula sa {settings}: mananatili ang mga folder, at lalaktawan ang mga shortcut na mayroon na siya — hindi pinapalitan.",
                "{settings} → {transfer} में आप चुनते हैं कि कौन-से फ़ोल्डर फ़ाइल में निर्यात करने हैं; {options} से केवल एक भी निर्यात कर सकते हैं। पाने वाला उसे {settings} से आयात करता है: फ़ोल्डर बने रहते हैं, और जो शॉर्टकट उसके पास पहले से हैं वे छोड़ दिए जाते हैं, बदले नहीं जाते।",
            ),
            &[
                ("settings", t.settings_title),
                ("transfer", t.transfer_section),
                ("options", text(state, "Opciones de carpeta", "Folder options", "Mga opsyon ng folder", "फ़ोल्डर विकल्प")),
            ],
        );
        topic(
            ui,
            false,
            text(state, "Importar y exportar", "Import and export", "Mag-import at mag-export", "आयात और निर्यात"),
            |ui| para(ui, &share_body),
        );

        // What each setting does. The dialog itself names them and says nothing more; the sentence
        // that used to sit under each control is here, under the same name and in the same order,
        // for whoever asks.
        topic(ui, false, t.settings_title, |ui| {
            part(ui, t.autostart_checkbox, t.autostart_hint);
            part(ui, t.appearance_section, t.theme_hint);
            part(ui, t.language_section, t.language_hint);
            part(ui, t.prefix_section, t.prefix_hint);
            part(ui, t.apply_prefix, t.apply_prefix_hint);
        });

        let pause_body = crate::i18n::fill(
            text(
                state,
                "El botón {pause} de la biblioteca detiene las expansiones sin cerrar nada, y el mismo botón las vuelve a activar. El punto de estado, a la izquierda del título, dice si Espanso está activo, en pausa o sin responder.",
                "The {pause} button in the library stops expansions without closing anything, and the same button turns them back on. The status dot to the left of the title says whether Espanso is active, paused or not responding.",
                "Pinapatigil ng {pause} na button sa aklatan ang mga expansion nang walang isinasara, at ibinabalik din ito ng parehong button. Sinasabi ng status dot sa kaliwa ng pamagat kung aktibo, naka-pause o hindi tumutugon ang Espanso.",
                "लाइब्रेरी का {pause} बटन बिना कुछ बंद किए विस्तार रोक देता है, और वही बटन उन्हें फिर चालू करता है। शीर्षक के बाईं ओर का स्थिति बिंदु बताता है कि Espanso सक्रिय है, रुका हुआ है या जवाब नहीं दे रहा।",
            ),
            &[("pause", t.tray_pause)],
        );
        topic(
            ui,
            false,
            text(state, "Pausar y usar la bandeja", "Pause and use the tray", "Mag-pause at gamitin ang tray", "रोकें और ट्रे का उपयोग"),
            |ui| {
                para(ui, &pause_body);
                part(ui, t.tip_pause_title, t.tip_pause_body);
                part(ui, t.tip_where_title, t.tip_where_body);
                part(ui, t.tip_pin_title, t.tip_pin_body);
                ui.add_space(controls::GAP);
                tray_illustration(ui);
            },
        );

        topic(ui, false, t.tip_undo_title, |ui| para(ui, t.tip_undo_body));

        topic(
            ui,
            false,
            text(state, "Atajos de teclado", "Keyboard shortcuts", "Mga keyboard shortcut", "कीबोर्ड शॉर्टकट"),
            |ui| shortcut_table(ui, state),
        );

        let search_body = crate::i18n::fill(t.tip_search_body, &[("keys", state.search_shortcut)]);
        topic(
            ui,
            false,
            text(state, "Buscar desde cualquier aplicación", "Search from any application", "Maghanap mula sa kahit anong app", "किसी भी ऐप से खोजें"),
            |ui| {
                para(ui, t.tip_search_title);
                ui.add_space(controls::GAP_TIGHT);
                para(ui, &search_body);
            },
        );
    });
}

/// Every shortcut the app answers to, printed from [`keys`] so the guide cannot drift from what the
/// keys really do: rename a binding there and this row follows it.
fn shortcut_table(ui: &mut egui::Ui, state: &AppState) {
    let lang = state.settings.lang;
    let or = text(state, "o", "or", "o", "या");
    let rows: [(String, &str); 10] = [
        (
            keys::text(keys::NEW, lang),
            text(state, "Nueva expansión", "New expansion", "Bagong expansion", "नया विस्तार"),
        ),
        (
            format!("{} {or} {}", keys::text(keys::FIND, lang), keys::text(keys::FIND_ALT, lang)),
            text(state, "Ir al buscador", "Go to the search box", "Pumunta sa paghahanap", "खोज बॉक्स पर जाएँ"),
        ),
        (
            format!("{} {}", keys::text(keys::UP, lang), keys::text(keys::DOWN, lang)),
            text(state, "Moverse por la lista", "Move through the list", "Gumalaw sa listahan", "सूची में चलें"),
        ),
        (
            keys::text(keys::OPEN, lang),
            text(state, "Editar la expansión abierta", "Edit the open expansion", "I-edit ang bukas na expansion", "खुला विस्तार संपादित करें"),
        ),
        (
            keys::text(keys::DELETE, lang),
            text(state, "Eliminar la expansión abierta, tras confirmar", "Delete the open expansion, after confirming", "Tanggalin ang bukas na expansion, pagkatapos kumpirmahin", "पुष्टि के बाद खुला विस्तार हटाएँ"),
        ),
        (
            keys::text(keys::RENAME, lang),
            text(state, "Opciones de la carpeta que estás viendo", "Options for the folder you are viewing", "Mga opsyon ng folder na tinitingnan mo", "जो फ़ोल्डर देख रहे हैं उसके विकल्प"),
        ),
        (
            keys::text(keys::SAVE, lang),
            text(state, "Guardar en el editor", "Save in the editor", "I-save sa editor", "संपादक में सहेजें"),
        ),
        (
            keys::text(keys::BACK, lang),
            text(state, "Cerrar el menú, el diálogo o el detalle; salir del editor", "Close the menu, the dialog or the detail; leave the editor", "Isara ang menu, dialog o detalye; umalis sa editor", "मेनू, संवाद या विवरण बंद करें; संपादक से निकलें"),
        ),
        (
            keys::click_text(egui::Modifiers::CTRL, lang),
            text(state, "Añadir una expansión a la selección", "Add one expansion to the selection", "Magdagdag ng isang expansion sa pinili", "चयन में एक विस्तार जोड़ें"),
        ),
        (
            keys::click_text(egui::Modifiers::SHIFT, lang),
            text(state, "Seleccionar todo el intervalo", "Select the whole range", "Piliin ang buong hanay", "पूरी श्रेणी चुनें"),
        ),
    ];
    // The keys in one column, so the eye runs down them; the width of that column is the widest
    // combination, so no meaning starts further right than it has to.
    let key_width = rows
        .iter()
        .map(|(k, _)| controls::widget_line(ui, controls::code(k, ui.visuals()), egui::TextStyle::Body).size().x)
        .fold(0.0_f32, f32::max);
    for (combo, meaning) in &rows {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = controls::GAP_WIDE;
            let (slot, _) = ui.allocate_exact_size(
                egui::vec2(key_width, ui.text_style_height(&egui::TextStyle::Body)),
                egui::Sense::hover(),
            );
            let line = controls::widget_line(ui, controls::code(combo, ui.visuals()), egui::TextStyle::Body);
            ui.painter().galley(slot.left_top(), line, egui::Color32::PLACEHOLDER);
            ui.add(egui::Label::new(*meaning).wrap());
        });
        ui.add_space(controls::GAP_TIGHT);
    }
}

/// A drawing of the notification area: the overflow flyout above, the taskbar below, and the icon
/// being dragged from one to the other — the gesture the tip is asking for.
///
/// Painted rather than shipped as an image. A screenshot would be wrong the moment Windows changes
/// its taskbar, wrong on a different accent colour, wrong in the other theme, and blurry on a scaled
/// display. Forty lines of rectangles are right in all four cases and weigh nothing.
///
/// The clock is drawn too, and it earns its place: "next to the clock" is how everybody actually
/// describes where the tray is, and it is the one landmark that makes the strip unmistakably the
/// notification area rather than a generic bar. The flyout sits directly above the chevron it opens
/// from, and the destination is an empty outline — a place waiting to be filled.
pub fn tray_illustration(ui: &mut egui::Ui) {
    let is_light = !ui.visuals().dark_mode;
    let accent = crate::app::accent(ui.visuals());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(264.0, 112.0), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();

    let ink = crate::app::text_tertiary(is_light);
    let quiet = ink.gamma_multiply(0.55);
    let surface = crate::app::win_control_for(is_light);
    let edge = crate::app::line_strong(is_light);

    // --- the taskbar, along the bottom -----------------------------------------------------------
    let bar = egui::Rect::from_min_size(
        egui::pos2(rect.left(), rect.bottom() - 34.0),
        egui::vec2(rect.width(), 34.0),
    );
    painter.rect_filled(bar, egui::CornerRadius::same(5), surface);
    painter.rect_stroke(
        bar,
        egui::CornerRadius::same(5),
        egui::Stroke::new(1.0, edge),
        egui::StrokeKind::Inside,
    );

    // The clock: two stacked bars standing in for the time and the date, at the right-hand end.
    for (i, w) in [34.0_f32, 40.0].into_iter().enumerate() {
        let y = bar.center().y - 5.0 + i as f32 * 10.0;
        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(bar.right() - 12.0 - w, y), egui::vec2(w, 4.0)),
            egui::CornerRadius::same(2),
            quiet,
        );
    }

    // Two icons that are already pinned, then the empty slot ours is heading for, then the chevron.
    let chevron_x = bar.left() + 34.0;
    let landing_x = chevron_x + 34.0;
    for i in 0..2 {
        let slot = egui::Rect::from_center_size(
            egui::pos2(landing_x + 34.0 + i as f32 * 30.0, bar.center().y),
            egui::vec2(15.0, 15.0),
        );
        painter.rect_filled(slot, egui::CornerRadius::same(3), quiet);
    }
    let landing = egui::Rect::from_center_size(
        egui::pos2(landing_x, bar.center().y),
        egui::vec2(17.0, 17.0),
    );
    painter.rect_stroke(
        landing,
        egui::CornerRadius::same(3),
        egui::Stroke::new(1.5, accent),
        egui::StrokeKind::Inside,
    );

    // The chevron that opens the flyout, pointing up at it.
    let chevron = egui::pos2(chevron_x, bar.center().y + 2.0);
    painter.line_segment(
        [chevron + egui::vec2(-5.0, 2.0), chevron + egui::vec2(0.0, -3.0)],
        egui::Stroke::new(1.8, ink),
    );
    painter.line_segment(
        [chevron + egui::vec2(0.0, -3.0), chevron + egui::vec2(5.0, 2.0)],
        egui::Stroke::new(1.8, ink),
    );

    // --- the flyout, directly above the chevron it belongs to ------------------------------------
    let flyout = egui::Rect::from_min_size(
        egui::pos2(chevron_x - 20.0, rect.top()),
        egui::vec2(104.0, 36.0),
    );
    painter.rect_filled(flyout, egui::CornerRadius::same(7), surface);
    painter.rect_stroke(
        flyout,
        egui::CornerRadius::same(7),
        egui::Stroke::new(1.0, edge),
        egui::StrokeKind::Inside,
    );
    let ours_x = flyout.left() + 52.0;
    for i in 0..3 {
        let x = flyout.left() + 22.0 + i as f32 * 30.0;
        let slot = egui::Rect::from_center_size(egui::pos2(x, flyout.center().y), egui::vec2(16.0, 16.0));
        let mine = (x - ours_x).abs() < 1.0;
        painter.rect_filled(
            slot,
            egui::CornerRadius::same(3),
            if mine { accent } else { quiet },
        );
    }

    // --- the drag ---------------------------------------------------------------------------------
    let from = egui::pos2(ours_x, flyout.bottom() + 5.0);
    let to = egui::pos2(landing.center().x, landing.top() - 8.0);
    painter.line_segment([from, to], egui::Stroke::new(1.8, accent));
    let dir = (to - from).normalized();
    let side = egui::vec2(-dir.y, dir.x) * 4.0;
    painter.add(egui::Shape::convex_polygon(
        vec![to + dir * 4.0, to - dir * 3.0 + side, to - dir * 3.0 - side],
        accent,
        egui::Stroke::NONE,
    ));
}
