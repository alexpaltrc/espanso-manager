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
    egui::CollapsingHeader::new(egui::RichText::new(title).strong())
        .id_salt(title)
        .show_unindented(ui, |ui| {
            ui.add_space(controls::GAP);
            contents(ui);
            ui.add_space(controls::GAP_TIGHT);
        });
}

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
    let title = text(
        state,
        "¿Cómo te ayudamos?",
        "How can we help?",
        "Paano ka namin matutulungan?",
        "हम आपकी कैसे मदद करें?",
    );
    let subtitle = text(
        state,
        "Elige lo que quieres hacer.",
        "Choose what you want to do.",
        "Piliin ang gusto mong gawin.",
        "चुनें कि आप क्या करना चाहते हैं।",
    );
    // Drawn above the scroll area on purpose: the way back must not be something you have to scroll
    // to the end of the guide to find.
    if controls::page_header(ui, t.back, title, subtitle) {
        state.view = View::List;
    }
    controls::page_scroll(ui, "quick-guide", |ui| {
            // Prose sets badly in a very wide column; past this the line becomes hard to track back.
            // A cap, not a width: `set_max_width` takes the number as given.
            ui.set_max_width(ui.available_width().min(820.0));
            lead(ui, state);
            ui.add_space(controls::GAP_SECTION);
            tasks(ui, state);
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
    controls::section_frame(is_light).show(ui, |ui| {
        ui.set_width(ui.available_width());
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
    let is_light = !ui.visuals().dark_mode;
    controls::section_frame(is_light).show(ui, |ui| {
        ui.set_width(ui.available_width());

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
                "Los rectángulos de arriba filtran la biblioteca: {all}, {none} y cada una de tus carpetas. Usa {select} para marcar varias expansiones y moverlas de una vez, o arrastra una fila hasta el rectángulo de una carpeta. Con una carpeta filtrada aparece {options}, donde puedes cambiarle el nombre o el prefijo, exportarla o eliminarla.",
                "The rectangles at the top filter the library: {all}, {none} and each of your folders. Use {select} to tick several expansions and move them at once, or drag a row onto a folder's rectangle. With a folder filtered you get {options}, where you can rename it, change its prefix, export it or delete it.",
                "Sinasala ng mga parisukat sa itaas ang aklatan: {all}, {none} at bawat folder mo. Gamitin ang {select} para markahan ang ilang expansion at ilipat nang sabay, o i-drag ang isang row papunta sa parisukat ng folder. Kapag may na-filter na folder, lilitaw ang {options}, kung saan mababago ang pangalan o prefix, mai-e-export o matatanggal ito.",
                "ऊपर के आयत लाइब्रेरी को छाँटते हैं: {all}, {none} और आपका हर फ़ोल्डर। एक साथ कई विस्तार ले जाने के लिए {select} से उन्हें चिह्नित करें, या किसी पंक्ति को फ़ोल्डर के आयत पर खींचें। कोई फ़ोल्डर छाँटा हो तो {options} दिखता है, जहाँ नाम या उपसर्ग बदल सकते हैं, निर्यात या हटा सकते हैं।",
            ),
            &[
                ("all", text(state, "Todos", "All", "Lahat", "सभी")),
                ("none", t.no_folder),
                ("select", text(state, "Seleccionar", "Select", "Pumili", "चुनें")),
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

        let pause_body = crate::i18n::fill(
            text(
                state,
                "El botón {pause} de la biblioteca detiene las expansiones sin cerrar nada, y el mismo botón las vuelve a activar. El punto de estado, junto al título, dice si Espanso está activo, en pausa o sin responder.",
                "The {pause} button in the library stops expansions without closing anything, and the same button turns them back on. The status dot beside the title says whether Espanso is active, paused or not responding.",
                "Pinapatigil ng {pause} na button sa aklatan ang mga expansion nang walang isinasara, at ibinabalik din ito ng parehong button. Sinasabi ng status dot sa tabi ng pamagat kung aktibo, naka-pause o hindi tumutugon ang Espanso.",
                "लाइब्रेरी का {pause} बटन बिना कुछ बंद किए विस्तार रोक देता है, और वही बटन उन्हें फिर चालू करता है। शीर्षक के बगल का स्थिति बिंदु बताता है कि Espanso सक्रिय है, रुका हुआ है या जवाब नहीं दे रहा।",
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
            |ui| {
                let rows = [
                    ("Ctrl+N", text(state, "Nueva expansión", "New expansion", "Bagong expansion", "नया विस्तार")),
                    ("Ctrl+F", text(state, "Buscar en la biblioteca", "Search the library", "Maghanap sa aklatan", "लाइब्रेरी खोजें")),
                    ("Ctrl+S", text(state, "Guardar en el editor", "Save in the editor", "I-save sa editor", "संपादक में सहेजें")),
                    (
                        "Ctrl",
                        text(state, "Con clic, añade una expansión a la selección", "With a click, adds one expansion to the selection", "Kasama ang click, nagdaragdag ng isang expansion sa pinili", "क्लिक के साथ, चयन में एक विस्तार जोड़ता है"),
                    ),
                    (
                        "Shift",
                        text(state, "Con clic, selecciona todo el intervalo", "With a click, selects the whole range", "Kasama ang click, pinipili ang buong hanay", "क्लिक के साथ, पूरी श्रेणी चुनता है"),
                    ),
                ];
                for (keys, meaning) in rows {
                    let key = controls::code(keys, ui.visuals());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(key);
                        ui.add(egui::Label::new(meaning).wrap());
                    });
                    ui.add_space(controls::GAP_TIGHT);
                }
            },
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
