//! Every user-visible string in the application, in each supported language.
//!
//! Translations live in one `Strings` struct per language rather than a runtime lookup table on
//! purpose: adding a new string is a compile error until *every* language provides it, so a
//! half-translated release is impossible. Lookups are plain field accesses on a `&'static` struct,
//! so translation costs nothing at runtime.
//!
//! Strings that embed a value use named placeholders (`{n}`, `{name}`, ...) filled by [`fill`],
//! never positional ones, so a translator is free to reorder them to suit their grammar.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Lang {
    #[default]
    En,
    Es,
    Fil,
    Hi,
}

impl Lang {
    pub const ALL: [Lang; 4] = [Lang::En, Lang::Es, Lang::Fil, Lang::Hi];

    /// The language's own name for itself — what a speaker of it expects to see in a picker.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Es => "Español",
            Lang::Fil => "Filipino",
            Lang::Hi => "हिन्दी",
        }
    }

    /// A plain-Latin name for the language, used wherever its own script cannot be drawn yet.
    pub fn latin_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Es => "Español",
            Lang::Fil => "Filipino",
            Lang::Hi => "Hindi",
        }
    }

    /// A representative character of the language's own script, for checking whether the currently
    /// loaded fonts can actually draw its name.
    fn script_sample(self) -> char {
        match self {
            Lang::Hi => 'ह',
            _ => 'A',
        }
    }

    /// What to show in the language picker.
    ///
    /// A script-specific font is only loaded once its language is selected — the Devanagari one is
    /// 5.3 MB and would otherwise be carried all day by everyone who will never draw a single
    /// character of it — and it may not be on the machine at all. Either way a name like "हिन्दी"
    /// would render as a row of empty boxes, leaving the very people who need that option unable to
    /// find it. When the glyphs are not there, the Latin name is shown instead, which is at least
    /// readable and recognisable. This is what makes the loading conditional affordable.
    pub fn picker_label(self, ctx: &egui::Context) -> &'static str {
        let sample = self.script_sample();
        let font_id = egui::FontId::proportional(14.0);
        if ctx.fonts_mut(|f| f.has_glyph(&font_id, sample)) {
            self.native_name()
        } else {
            self.latin_name()
        }
    }

    pub fn strings(self) -> &'static Strings {
        match self {
            Lang::En => &EN,
            Lang::Es => &ES,
            Lang::Fil => &FIL,
            Lang::Hi => &HI,
        }
    }
}

/// Substitutes `{placeholder}` tokens in a template.
/// Substitutes `{name}` placeholders in a translated string.
///
/// One left-to-right pass over the template, which is what makes it safe. The earlier version ran
/// `str::replace` once per argument, each pass over the result of the last — so a value substituted
/// early was itself searched for later keys. Every value here that the user typed is exposed to
/// that: a folder named `{n}` came out of "Deleted the folder \"{name}\" and its {n} expansions" as
/// the count, and a trigger containing `{more}` did the same to the prefix confirmation. Nothing
/// substituted in this version is ever looked at again.
///
/// A key with no argument for it is left on screen as `{key}` rather than quietly dropped. The two
/// are the same size of mistake — a translator loses a placeholder, or renames one — but only one
/// of them can be seen and reported. Silently vanishing is how an OS error message disappears from
/// a failure banner in one language and nobody finds out. `placeholders_agree_across_languages`
/// below is meant to catch it first; this is what happens if it ever does not.
pub fn fill(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        // `{` is one byte, so this stays on a character boundary in any UTF-8 string.
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let key = &after[..close];
                match args.iter().find(|(k, _)| *k == key) {
                    Some((_, value)) => out.push_str(value),
                    None => {
                        out.push('{');
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            // An opening brace with nothing closing it is just a brace.
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

pub struct Strings {
    // --- main window ---------------------------------------------------------------------
    pub app_title: &'static str,
    pub settings_button: &'static str,
    pub tips_title: &'static str,
    pub tips_button_tip: &'static str,
    pub tips_footer: &'static str,
    pub tip_search_title: &'static str,
    pub tip_search_body: &'static str,
    pub onboarding_title: &'static str,
    pub onboarding_intro: &'static str,
    pub onboarding_try_title: &'static str,
    pub onboarding_try_body: &'static str,
    pub onboarding_try_placeholder: &'static str,
    pub onboarding_try_ok: &'static str,
    pub onboarding_autostart_title: &'static str,
    pub onboarding_autostart_note: &'static str,
    pub onboarding_where_title: &'static str,
    pub onboarding_where_body: &'static str,
    pub onboarding_start: &'static str,
    pub search_window_hint: &'static str,
    pub search_window_empty: &'static str,
    pub search_window_footer: &'static str,
    pub tip_where_title: &'static str,
    pub tip_where_body: &'static str,
    pub tip_pin_title: &'static str,
    pub tip_pin_body: &'static str,
    pub tip_undo_title: &'static str,
    pub tip_undo_body: &'static str,
    pub tip_pause_title: &'static str,
    pub tip_pause_body: &'static str,
    pub tip_select_title: &'static str,
    pub tip_select_body: &'static str,
    pub new_expansion: &'static str,
    pub search_label: &'static str,
    pub empty_title: &'static str,
    pub empty_hint: &'static str,
    pub no_matches: &'static str,
    pub view_comfortable_tip: &'static str,
    pub view_compact_tip: &'static str,
    pub prefix_field_hint: &'static str,
    pub word_field_hint: &'static str,
    pub trigger_preview: &'static str,
    pub made_by: &'static str,
    pub credits_espanso: &'static str,
    pub edit_button: &'static str,
    pub edit_tip: &'static str,
    pub delete: &'static str,
    pub delete_tip: &'static str,
    pub rename_folder_tip: &'static str,
    pub delete_folder_tip: &'static str,
    pub save_name_tip: &'static str,
    pub cancel: &'static str,
    pub cancel_tip: &'static str,
    pub selected_count: &'static str,
    pub remove_from_folder: &'static str,
    pub delete_selected: &'static str,
    pub clear_selection_tip: &'static str,
    pub moving_n: &'static str,

    // --- confirmation modal --------------------------------------------------------------
    pub confirm_delete_folder_title: &'static str,
    pub confirm_delete_folder_body: &'static str,
    pub confirm_delete_selection_title: &'static str,
    pub confirm_delete_selection_body: &'static str,
    pub see_more: &'static str,

    // --- edit form -----------------------------------------------------------------------
    pub edit_title_new: &'static str,
    pub edit_title_existing: &'static str,
    pub when_you_type: &'static str,
    pub will_be_replaced_by: &'static str,
    pub kind_text: &'static str,
    pub kind_date: &'static str,
    pub preset_date_only: &'static str,
    pub preset_datetime_utc: &'static str,
    pub preset_month_day_year: &'static str,
    pub preset_day_month_year: &'static str,
    pub month_language_label: &'static str,
    pub month_language_hint: &'static str,
    pub preset_custom: &'static str,
    pub blocks_hint: &'static str,
    pub blocks_empty: &'static str,
    pub blocks_fields: &'static str,
    pub blocks_separators: &'static str,
    pub blocks_remove_tip: &'static str,
    pub blocks_advanced: &'static str,
    pub block_year: &'static str,
    pub block_month_name: &'static str,
    pub block_month_number: &'static str,
    pub block_day: &'static str,
    pub block_hour: &'static str,
    pub block_hour_utc: &'static str,
    pub blocks_utc_note: &'static str,
    pub block_minute: &'static str,
    pub block_space: &'static str,
    pub format_label: &'static str,
    pub folder_optional: &'static str,
    pub no_folder: &'static str,
    pub new_folder_placeholder: &'static str,
    pub add_new_folder: &'static str,
    pub name_label: &'static str,
    pub save: &'static str,

    // --- settings ------------------------------------------------------------------------
    pub back: &'static str,
    pub settings_title: &'static str,
    pub autostart_section: &'static str,
    pub autostart_checkbox: &'static str,
    pub autostart_hint: &'static str,
    pub appearance_section: &'static str,
    pub theme_system: &'static str,
    pub theme_light: &'static str,
    pub theme_dark: &'static str,
    pub theme_hint: &'static str,
    pub language_section: &'static str,
    pub language_hint: &'static str,
    pub transfer_section: &'static str,
    pub transfer_hint: &'static str,
    pub export_button: &'static str,
    pub import_button: &'static str,
    pub transfer_file_kind: &'static str,
    pub export_done: &'static str,
    pub export_error: &'static str,
    pub import_done: &'static str,
    pub import_done_skipped: &'static str,
    pub import_none_added: &'static str,
    pub import_not_ours: &'static str,
    pub import_error: &'static str,
    pub prefix_section: &'static str,
    pub prefix_hint: &'static str,
    pub custom_label: &'static str,
    pub apply_prefix: &'static str,
    pub apply_prefix_hint: &'static str,

    // --- banners and dialogs -------------------------------------------------------------
    pub banner_close_tip: &'static str,
    pub undo: &'static str,
    pub move_undone: &'static str,
    pub autostart_on: &'static str,
    pub autostart_off: &'static str,
    pub autostart_error: &'static str,
    pub prefix_save_error: &'static str,
    pub view_save_error: &'static str,
    pub language_save_error: &'static str,
    pub theme_save_error: &'static str,
    pub prefix_already_applied: &'static str,
    pub prefix_collision: &'static str,
    pub prefix_confirm_title: &'static str,
    pub prefix_confirm_body: &'static str,
    pub prefix_confirm_more: &'static str,
    pub prefix_applied: &'static str,
    pub delete_one_title: &'static str,
    pub delete_one_body: &'static str,
    pub expansion_deleted: &'static str,
    pub trigger_empty: &'static str,
    pub trigger_duplicate: &'static str,
    pub trigger_shadow: &'static str,
    pub expansion_saved: &'static str,
    pub folder_deleted: &'static str,
    pub selection_deleted: &'static str,
    pub folder_name_taken: &'static str,
    pub folder_renamed: &'static str,
    pub moved_to_folder: &'static str,
    pub removed_from_folder: &'static str,

    // --- match previews ------------------------------------------------------------------
    pub preview_date: &'static str,
    pub preview_advanced_vars: &'static str,
    pub preview_advanced: &'static str,

    // --- file / process errors -----------------------------------------------------------
    pub err_read_file: &'static str,
    pub err_parse_yaml: &'static str,
    pub err_self_check: &'static str,
    pub err_write_file: &'static str,
    /// The save went through but the copy that would have let it be undone did not.
    pub save_backup_failed: &'static str,
    /// The save went through but Windows would not confirm the bytes reached the disk.
    pub save_flush_failed: &'static str,
    pub err_validation_empty: &'static str,
    pub err_validation_duplicate: &'static str,
    pub err_espansod_missing: &'static str,
    pub err_espansod_timeout: &'static str,
    pub err_espanso_start: &'static str,
    pub err_espanso_no_response: &'static str,
    pub err_restart_unconfirmed: &'static str,
    pub err_espanso_action: &'static str,
    pub err_espanso_comm: &'static str,
    pub verb_pause_timed: &'static str,
    pub verb_pause: &'static str,
    pub verb_resume: &'static str,
    pub verb_auto_resume: &'static str,

    // --- tray ----------------------------------------------------------------------------
    pub tray_pause: &'static str,
    pub tray_resume: &'static str,
    pub tray_pause_10: &'static str,
    pub tray_quit: &'static str,
    pub tray_tooltip: &'static str,

    // --- startup dialogs -----------------------------------------------------------------
    pub already_running: &'static str,
    /// Shown when the single-instance lock could not be taken at all, so this copy
    /// started without knowing whether another one is already running.
    pub instance_guard_warning: &'static str,
    /// The settings file would not parse. Its contents were moved to `{name}`, which is the only
    /// place the folder assignments still exist.
    pub settings_damaged: &'static str,
    /// The same, but the damaged file could not even be moved out of the way.
    pub settings_damaged_lost: &'static str,
    /// The settings file could not be read at all. Nothing was touched.
    pub settings_unreadable: &'static str,
    pub panic_title: &'static str,
    pub panic_body: &'static str,
    pub tray_error: &'static str,
    pub startup_espanso_warning: &'static str,
    /// Shown when espanso's own `default.yml` could not be read or written, which is the one
    /// failure that lets espanso's tray icon and its Windows toasts come back unannounced.
    pub config_patch_warning: &'static str,
    pub hindi_font_missing: &'static str,
}

pub static EN: Strings = Strings {
    app_title: "My text expansions",
    settings_button: "Settings",
    tips_title: "Tips",
    tips_button_tip: "Tips",
    tips_footer: "Nothing here is required reading. It is the handful of things people ask about most.",
    tip_search_title: "Find an expansion without leaving what you are doing",
    tip_search_body: "Press {keys} anywhere in Windows and a search box appears. Type part of a trigger or part of its text, pick one, and it is inserted where your cursor was.",
    onboarding_title: "Welcome",
    onboarding_intro: "You type a short trigger and it turns into the full text you saved. It works in any program: email, chat, a document, the browser.",
    onboarding_try_title: "See it work",
    onboarding_try_body: "Type  :espanso  in the box below — the whole thing, without spaces.",
    onboarding_try_placeholder: "Try it here",
    onboarding_try_ok: "That is all there is to it. It behaves the same way in every other program.",
    onboarding_autostart_title: "Start with Windows",
    onboarding_autostart_note: "You can change this whenever you like, in Settings.",
    onboarding_where_title: "Where it lives",
    onboarding_where_body: "In the system tray, next to the clock — one click brings the window back. Windows hides new icons behind the small arrow, so drag this one onto the taskbar to keep it in sight.",
    onboarding_start: "Get started",
    search_window_hint: "Search your expansions",
    search_window_empty: "Nothing matches that.",
    search_window_footer: "↑ ↓ to move · Enter to insert · Esc to close",
    tip_where_title: "Where did the window go?",
    tip_where_body: "It lives in the system tray, next to the clock. One click brings it back. Closing it with the X does not shut it down — it puts it away, so it never sits in front of the work you were doing.",
    tip_pin_title: "Keep the icon where you can see it",
    tip_pin_body: "Windows hides new tray icons behind the small arrow. Open it and drag the EspansoManager icon down onto the taskbar — it stays there from then on.",
    tip_undo_title: "An expansion fired when you did not want it to",
    tip_undo_body: "Press Esc, once, right after it happens. The expansion is undone and your text is left as you typed it.",
    tip_pause_title: "Pause it when it is in the way",
    tip_pause_body: "Right-click the tray icon: pause for ten minutes, or pause until you say otherwise. Useful when you are typing something your triggers would interfere with — a password field, a piece of code.",
    tip_select_title: "Work on several at once",
    tip_select_body: "In the list, hold Ctrl to pick expansions one by one, or Shift to take a whole run of them. You can then delete them, or take them out of their folder, in one go.",
    new_expansion: "New expansion",
    search_label: "Search expansions",
    empty_title: "You don't have any text expansions yet.",
    empty_hint: "Use the \"New expansion\" button above to create your first one.",
    no_matches: "No expansion matches your search.",
    view_comfortable_tip: "Comfortable view",
    view_compact_tip: "Compact view",
    prefix_field_hint: "prefix",
    word_field_hint: "word",
    trigger_preview: "You will type: {trigger}",
    made_by: "EspansoManager — made with AI by Alex Palacios",
    credits_espanso: "Built on Espanso, created by Federico Terzi and its contributors.",
    edit_button: "✏ Edit",
    edit_tip: "Edit",
    delete: "Delete",
    delete_tip: "Delete",
    rename_folder_tip: "Rename folder",
    delete_folder_tip: "Delete folder",
    save_name_tip: "Save name",
    cancel: "Cancel",
    cancel_tip: "Cancel",
    selected_count: "{n} selected",
    remove_from_folder: "Take out of folder",
    delete_selected: "Delete selected",
    clear_selection_tip: "Clear selection",
    moving_n: "Moving {n} expansions",

    confirm_delete_folder_title: "Delete folder \"{name}\"",
    confirm_delete_folder_body: "You are about to delete the folder \"{name}\", which contains {n} expansion(s). Continuing will delete ALL expansions in this folder. This cannot be undone.",
    confirm_delete_selection_title: "Delete {n} expansion(s)",
    confirm_delete_selection_body: "You are about to delete {n} selected expansion(s). This cannot be undone.",
    see_more: "See more ({n} more)",

    edit_title_new: "New text expansion",
    edit_title_existing: "Edit text expansion",
    when_you_type: "When you type this:",
    will_be_replaced_by: "It will be replaced by:",
    kind_text: "Text",
    kind_date: "Current date or time",
    preset_date_only: "Date only (ISO 8601)",
    preset_datetime_utc: "Date and time in UTC (ISO 8601)",
    preset_month_day_year: "Month, day and year",
    preset_day_month_year: "Day, month and year",
    month_language_label: "Month language",
    month_language_hint: "This wins over your Windows language and over the app's: the month is written this way on any computer.",
    preset_custom: "Custom",
    blocks_hint: "Drag the blocks below to build your format, or click one to add it at the end.",
    blocks_empty: "Drop blocks here",
    blocks_fields: "Pieces of the date",
    blocks_separators: "Separators (reusable)",
    blocks_remove_tip: "Remove",
    blocks_advanced: "This format uses codes the blocks cannot show. Edit it as text below.",
    block_year: "Year",
    block_month_name: "Month (name)",
    block_month_number: "Month (number)",
    block_day: "Day",
    block_hour: "Hour (local)",
    block_hour_utc: "Hour (UTC)",
    blocks_utc_note: "The whole date will be in UTC, not only the hour — that is how a timestamp stays coherent.",
    block_minute: "Minutes",
    block_space: "space",
    format_label: "Format (strftime code):",
    folder_optional: "Folder (optional):",
    no_folder: "No folder",
    new_folder_placeholder: "New folder…",
    add_new_folder: "+ New folder…",
    name_label: "Name:",
    save: "Save",

    back: "Back",
    settings_title: "Settings",
    autostart_section: "Start automatically",
    autostart_checkbox: "Start EspansoManager (and Espanso) with Windows",
    autostart_hint: "It will run in the background, in the system tray.",
    appearance_section: "Appearance",
    theme_system: "System",
    theme_light: "Light theme",
    theme_dark: "Dark theme",
    theme_hint: "\"System\" follows your Windows light/dark setting.",
    language_section: "Language",
    language_hint: "Changes the language of this window only, not your expansions.",
    transfer_section: "Export and import",
    transfer_hint: "Send your expansions to a colleague, or add theirs to yours. Importing only adds — nothing you already have is replaced.",
    export_button: "Export my expansions…",
    import_button: "Import expansions…",
    transfer_file_kind: "EspansoManager export",
    export_done: "Exported: {n}.",
    export_error: "The file could not be written: {err}",
    import_done: "Added: {n}.",
    import_done_skipped: "Added: {n}. Skipped: {k} — that trigger was already in use.",
    import_none_added: "Nothing was added: every trigger in that file is already in use.",
    import_not_ours: "That file does not look like an EspansoManager export.",
    import_error: "The file could not be read: {err}",
    prefix_section: "Prefix for new expansions",
    prefix_hint: "The prefix is what you type before each trigger, for example \":\" in \":hello\". Pick the one you prefer.",
    custom_label: "Custom:",
    apply_prefix: "Apply the current prefix to my existing expansions",
    apply_prefix_hint: "Changes the leading prefix of every expansion you already have to the one selected above (for example, from \"::hello\" to \":hello\").",

    banner_close_tip: "Close",
    undo: "Undo",
    move_undone: "The move was undone.",
    autostart_on: "EspansoManager will now start with Windows.",
    autostart_off: "It will no longer start automatically with Windows.",
    autostart_error: "Could not change the Windows start-up setting: {err}",
    prefix_save_error: "Could not save the prefix setting: {err}",
    view_save_error: "Could not save the view setting: {err}",
    language_save_error: "Could not save the language setting: {err}",
    theme_save_error: "Could not save the theme setting: {err}",
    prefix_already_applied: "All your expansions already use this prefix.",
    prefix_collision: "The change was not applied: at least two expansions would end up with the same trigger ({list}). Please review them before changing the prefix.",
    prefix_confirm_title: "Change the prefix of existing expansions",
    prefix_confirm_body: "{n} trigger(s) will be renamed:\n\n{list}{more}\n\nContinue?",
    prefix_confirm_more: "\n  … and {n} more",
    prefix_applied: "Updated the prefix of {n} expansion(s).",
    delete_one_title: "Delete expansion",
    delete_one_body: "Delete the expansion \"{name}\"? This cannot be undone.",
    expansion_deleted: "Expansion deleted.",
    trigger_empty: "The trigger cannot be empty.",
    trigger_duplicate: "Another expansion already uses the trigger \"{name}\".",
    trigger_shadow: "\"{short}\" fires the moment you type it, so \"{long}\" would never work.",
    expansion_saved: "Expansion saved.",
    folder_deleted: "Deleted the folder \"{name}\" and its {n} expansion(s).",
    selection_deleted: "Deleted {n} expansion(s).",
    folder_name_taken: "A folder named \"{name}\" already exists.",
    folder_renamed: "Folder renamed to \"{name}\".",
    moved_to_folder: "{n} expansion(s) moved to \"{name}\".",
    removed_from_folder: "{n} expansion(s) taken out of their folder.",

    preview_date: "Date/time — e.g. {example}",
    preview_advanced_vars: "Advanced (uses special variables) — edit it in the .yml file",
    preview_advanced: "Advanced — edit it in the .yml file",

    err_read_file: "Could not read the expansions file: {err}",
    err_parse_yaml: "The expansions file (base.yml) has a YAML formatting error and could not be read:\n{err}\n\nEspanso will not be able to use it either until it is fixed. You can open it with a text editor to correct it by hand.",
    err_self_check: "EspansoManager produced a file it could not read back, so nothing was saved in order to protect your existing expansions. Technical detail: {err}",
    err_write_file: "Could not save the expansions file: {err}",
    save_backup_failed: "No backup copy of the previous file could be made first: {err}\n\nThe change is saved. What is missing is the copy in the backups folder that would let you go back.",
    save_flush_failed: "Windows would not confirm that the file reached the disk: {err}\n\nThe change is saved and Espanso can read it. On a USB stick or a network drive this is common; if the machine loses power in the next few seconds, the change could be lost.",
    err_validation_empty: "The trigger cannot be empty.",
    err_validation_duplicate: "There is already an expansion using exactly the trigger \"{name}\".",
    err_espansod_missing: "espansod.exe was not found next to EspansoManager.exe (expected at {path}).",
    err_espansod_timeout: "espansod.exe did not respond within {n} seconds and had to be closed.",
    err_espanso_start: "Could not start Espanso: {err}",
    err_espanso_no_response: "Espanso did not respond after trying to start it.",
    err_restart_unconfirmed: "The change was saved, but Espanso did not confirm that it restarted correctly. You may need to restart it manually.",
    err_espanso_action: "Espanso could not {verb}.\n\n{detail}",
    err_espanso_comm: "Could not communicate with Espanso to {verb}: {err}",
    verb_pause_timed: "pause temporarily",
    verb_pause: "pause",
    verb_resume: "resume",
    verb_auto_resume: "resume automatically",

    tray_pause: "Pause",
    tray_resume: "Resume",
    tray_pause_10: "Pause for 10 minutes",
    tray_quit: "Quit",
    tray_tooltip: "Espanso — click to open, right-click to pause",

    already_running: "EspansoManager is already running. Look for it in the system tray, next to the clock (it may be hidden behind the arrow for hidden icons).",
    instance_guard_warning: "Windows would not grant the lock that stops two copies of EspansoManager running at once: {err}\n\nThe app has started anyway. The only consequence is that a second copy could now open alongside this one — if you see two icons in the system tray, close one of them.",
    settings_damaged: "Your settings file could not be read as settings, so EspansoManager has started with the defaults — which means your folders are not there. The old file was kept as {name}; nothing has been deleted.",
    settings_damaged_lost: "Your settings file could not be read as settings, so EspansoManager has started with the defaults — which means your folders are not there. The old file could not be moved out of the way either, so the next change you save will replace it.",
    settings_unreadable: "Your settings file could not be opened: {err}\n\nEspansoManager has started with the defaults, so your folders are not showing. The file itself has not been touched — close whatever may be holding it and open the app again.",
    panic_title: "EspansoManager hit an unexpected error",
    panic_body: "EspansoManager had to close because of an unexpected internal error.\n\nYour saved expansions are not affected.\n\nTechnical detail ({location}):\n{message}",
    tray_error: "Could not create the system tray icon: {err}\n\nEspanso is still running, and its own icon has been given back to it — you can pause or quit it from there. If this keeps happening, check that the .espanso-runtime folder still has its .ico files.",
    startup_espanso_warning: "Could not confirm that Espanso is running: {err}\n\nEspansoManager will stay open so you can review the configuration, but text expansions may not be active yet.",
    config_patch_warning: "Could not adjust Espanso's own settings: {err}\n\nEspanso may go on showing its tray icon and its Windows notifications next to EspansoManager's. Check that default.yml can be read and is saved as UTF-8.",
    hindi_font_missing: "Hindi was selected, but no font supporting Devanagari was found on this computer, so the text may not display correctly. Choose another language if you see empty boxes.",
};

pub static ES: Strings = Strings {
    app_title: "Mis expansiones de texto",
    settings_button: "Ajustes",
    tips_title: "Consejos",
    tips_button_tip: "Consejos",
    tips_footer: "Nada de esto es obligatorio. Son las cosas que más se preguntan.",
    tip_search_title: "Busca una expansión sin salir de lo que estás haciendo",
    tip_search_body: "Presiona {keys} en cualquier parte de Windows y aparece un buscador. Escribe parte del activador o parte de su texto, elige una, y se inserta donde estaba tu cursor.",
    onboarding_title: "Te damos la bienvenida",
    onboarding_intro: "Escribes un atajo corto y se convierte en el texto completo que guardaste. Funciona en cualquier programa: correo, chat, un documento, el navegador.",
    onboarding_try_title: "Míralo funcionar",
    onboarding_try_body: "Escribe  :espanso  en la caja de abajo — completo y sin espacios.",
    onboarding_try_placeholder: "Pruébalo aquí",
    onboarding_try_ok: "Eso es todo. Se comporta igual en cualquier otro programa.",
    onboarding_autostart_title: "Iniciar con Windows",
    onboarding_autostart_note: "Puedes cambiarlo cuando quieras, en Ajustes.",
    onboarding_where_title: "Dónde vive",
    onboarding_where_body: "En la bandeja del sistema, junto al reloj — un clic y la ventana vuelve. Windows esconde los iconos nuevos detrás de la flechita, así que arrastra este a la barra de tareas para tenerlo siempre a la vista.",
    onboarding_start: "Empezar",
    search_window_hint: "Busca entre tus expansiones",
    search_window_empty: "Nada coincide.",
    search_window_footer: "↑ ↓ para moverte · Enter para insertar · Esc para cerrar",
    tip_where_title: "¿Dónde quedó la ventana?",
    tip_where_body: "Vive en la bandeja del sistema, junto al reloj. Un clic y vuelve. Cerrarla con la X no la apaga: la guarda, para que nunca se te atraviese mientras trabajas.",
    tip_pin_title: "Deja el icono siempre a la vista",
    tip_pin_body: "Windows esconde los iconos nuevos detrás de la flechita. Ábrela y arrastra el de EspansoManager hacia la barra de tareas: de ahí ya no se mueve.",
    tip_undo_title: "Se activó una expansión sin querer",
    tip_undo_body: "Presiona Esc una vez, justo después. La expansión se deshace y tu texto queda tal como lo escribiste.",
    tip_pause_title: "Pausa cuando estorbe",
    tip_pause_body: "Clic derecho en el icono de la bandeja: pausa diez minutos, o pausa hasta que tú digas. Sirve cuando escribes algo donde tus atajos estorbarían — una contraseña, un pedazo de código.",
    tip_select_title: "Trabaja con varias a la vez",
    tip_select_body: "En la lista, mantén Ctrl para elegirlas una por una, o Shift para tomar un tramo entero. Luego puedes borrarlas, o sacarlas de su carpeta, de un solo golpe.",
    new_expansion: "Nueva expansión",
    search_label: "Buscar expansiones",
    empty_title: "Aún no tienes ninguna expansión de texto.",
    empty_hint: "Usa el botón \"Nueva expansión\" de arriba para crear la primera.",
    no_matches: "Ninguna expansión coincide con tu búsqueda.",
    view_comfortable_tip: "Vista predeterminada",
    view_compact_tip: "Vista compacta",
    prefix_field_hint: "prefijo",
    word_field_hint: "palabra",
    trigger_preview: "Escribirás: {trigger}",
    made_by: "EspansoManager — hecho con IA por Alex Palacios",
    credits_espanso: "Construido sobre Espanso, creado por Federico Terzi y sus colaboradores.",
    edit_button: "✏ Editar",
    edit_tip: "Editar",
    delete: "Eliminar",
    delete_tip: "Eliminar",
    rename_folder_tip: "Renombrar carpeta",
    delete_folder_tip: "Eliminar carpeta",
    save_name_tip: "Guardar nombre",
    cancel: "Cancelar",
    cancel_tip: "Cancelar",
    selected_count: "{n} seleccionada(s)",
    remove_from_folder: "Quitar de su carpeta",
    delete_selected: "Eliminar seleccionadas",
    clear_selection_tip: "Cancelar selección",
    moving_n: "Moviendo {n} expansiones",

    confirm_delete_folder_title: "Eliminar carpeta \"{name}\"",
    confirm_delete_folder_body: "Vas a eliminar la carpeta \"{name}\", que incluye {n} expansión(es). Continuar eliminará TODAS las expansiones de esta carpeta. Esta acción no se puede deshacer.",
    confirm_delete_selection_title: "Eliminar {n} expansión(es)",
    confirm_delete_selection_body: "Vas a eliminar {n} expansión(es) seleccionada(s). Esta acción no se puede deshacer.",
    see_more: "Ver más ({n} más)",

    edit_title_new: "Nueva expansión de texto",
    edit_title_existing: "Editar expansión de texto",
    when_you_type: "Cuando escribas esto:",
    will_be_replaced_by: "Se reemplazará por:",
    kind_text: "Texto",
    kind_date: "Fecha u hora actual",
    preset_date_only: "Solo la fecha (ISO 8601)",
    preset_datetime_utc: "Fecha y hora en UTC (ISO 8601)",
    preset_month_day_year: "Mes, día y año",
    preset_day_month_year: "Día, mes y año",
    month_language_label: "Idioma del mes",
    month_language_hint: "Manda sobre el idioma de Windows y sobre el de la aplicación: el mes se escribirá así en cualquier computadora.",
    preset_custom: "Personalizado",
    blocks_hint: "Arrastra los bloques de abajo para armar tu formato, o haz clic en uno para añadirlo al final.",
    blocks_empty: "Suelta aquí los bloques",
    blocks_fields: "Piezas de la fecha",
    blocks_separators: "Separadores (se pueden repetir)",
    blocks_remove_tip: "Quitar",
    blocks_advanced: "Este formato usa códigos que los bloques no pueden mostrar. Edítalo como texto abajo.",
    block_year: "Año",
    block_month_name: "Mes (nombre)",
    block_month_number: "Mes (número)",
    block_day: "Día",
    block_hour: "Hora (local)",
    block_hour_utc: "Hora (UTC)",
    blocks_utc_note: "Toda la fecha irá en UTC, no solo la hora — así la marca de tiempo es coherente.",
    block_minute: "Minutos",
    block_space: "espacio",
    format_label: "Formato (código strftime):",
    folder_optional: "Carpeta (opcional):",
    no_folder: "Sin carpeta",
    new_folder_placeholder: "Nueva carpeta…",
    add_new_folder: "+ Nueva carpeta…",
    name_label: "Nombre:",
    save: "Guardar",

    back: "Volver",
    settings_title: "Ajustes",
    autostart_section: "Inicio automático",
    autostart_checkbox: "Iniciar EspansoManager (y Espanso) con Windows",
    autostart_hint: "Se ejecutará en segundo plano, en la bandeja del sistema.",
    appearance_section: "Apariencia",
    theme_system: "Sistema",
    theme_light: "Tema claro",
    theme_dark: "Tema oscuro",
    theme_hint: "\"Sistema\" sigue la configuración de claro/oscuro de Windows.",
    language_section: "Idioma",
    language_hint: "Cambia el idioma de esta ventana solamente, no el de tus expansiones.",
    transfer_section: "Exportar e importar",
    transfer_hint: "Envía tus expansiones a un compañero, o añade las suyas a las tuyas. Importar solo añade: nada de lo que ya tienes se reemplaza.",
    export_button: "Exportar mis expansiones…",
    import_button: "Importar expansiones…",
    transfer_file_kind: "Exportación de EspansoManager",
    export_done: "Exportadas: {n}.",
    export_error: "No se pudo escribir el archivo: {err}",
    import_done: "Añadidas: {n}.",
    import_done_skipped: "Añadidas: {n}. Omitidas: {k} — ese activador ya estaba en uso.",
    import_none_added: "No se añadió nada: todos los activadores de ese archivo ya están en uso.",
    import_not_ours: "Ese archivo no parece una exportación de EspansoManager.",
    import_error: "No se pudo leer el archivo: {err}",
    prefix_section: "Prefijo para nuevas expansiones",
    prefix_hint: "El prefijo es el texto que escribes antes de cada activador, por ejemplo \":\" en \":hola\". Elige el que prefieras.",
    custom_label: "Personalizado:",
    apply_prefix: "Aplicar el prefijo actual a mis expansiones existentes",
    apply_prefix_hint: "Cambia el prefijo inicial de cada expansión que ya tienes por el que elegiste arriba (por ejemplo, de \"::hola\" a \":hola\").",

    banner_close_tip: "Cerrar",
    undo: "Deshacer",
    move_undone: "Se deshizo el movimiento.",
    autostart_on: "EspansoManager se iniciará con Windows a partir de ahora.",
    autostart_off: "Ya no se iniciará automáticamente con Windows.",
    autostart_error: "No se pudo cambiar el inicio automático con Windows: {err}",
    prefix_save_error: "No se pudo guardar el ajuste de prefijo: {err}",
    view_save_error: "No se pudo guardar el ajuste de vista: {err}",
    language_save_error: "No se pudo guardar el ajuste de idioma: {err}",
    theme_save_error: "No se pudo guardar el ajuste de tema: {err}",
    prefix_already_applied: "Todas tus expansiones ya usan este prefijo.",
    prefix_collision: "No se aplicó el cambio: al menos dos expansiones terminarían con el mismo activador ({list}). Revísalas manualmente antes de cambiar el prefijo.",
    prefix_confirm_title: "Cambiar prefijo de expansiones existentes",
    prefix_confirm_body: "Se van a renombrar {n} activador(es):\n\n{list}{more}\n\n¿Continuar?",
    prefix_confirm_more: "\n  … y {n} más",
    prefix_applied: "Se actualizó el prefijo de {n} expansión(es).",
    delete_one_title: "Eliminar expansión",
    delete_one_body: "¿Eliminar la expansión \"{name}\"? Esta acción no se puede deshacer.",
    expansion_deleted: "Expansión eliminada.",
    trigger_empty: "El activador no puede estar vacío.",
    trigger_duplicate: "Ya existe otra expansión con el activador \"{name}\".",
    trigger_shadow: "\"{short}\" se activa apenas lo escribes, así que \"{long}\" nunca funcionaría.",
    expansion_saved: "Expansión guardada.",
    folder_deleted: "Se eliminó la carpeta \"{name}\" y sus {n} expansión(es).",
    selection_deleted: "Se eliminaron {n} expansión(es).",
    folder_name_taken: "Ya existe una carpeta llamada \"{name}\".",
    folder_renamed: "Carpeta renombrada a \"{name}\".",
    moved_to_folder: "{n} expansión(es) movida(s) a \"{name}\".",
    removed_from_folder: "{n} expansión(es) quitada(s) de su carpeta.",

    preview_date: "Fecha/hora — ej. {example}",
    preview_advanced_vars: "Avanzado (usa variables especiales) — edítalo en el archivo .yml",
    preview_advanced: "Avanzado — edítalo en el archivo .yml",

    err_read_file: "No se pudo leer el archivo de expansiones: {err}",
    err_parse_yaml: "El archivo de expansiones (base.yml) tiene un error de formato YAML y no se pudo leer:\n{err}\n\nEspanso tampoco podrá usarlo hasta que se corrija. Puedes abrirlo con un editor de texto para corregirlo a mano.",
    err_self_check: "EspansoManager generó un archivo que no pudo releer correctamente, así que no se guardó nada para proteger tus expansiones existentes. Detalle técnico: {err}",
    err_write_file: "No se pudo guardar el archivo de expansiones: {err}",
    save_backup_failed: "No se pudo hacer antes una copia de seguridad del archivo anterior: {err}\n\nEl cambio sí quedó guardado. Lo que falta es la copia en la carpeta de copias de seguridad que te permitiría volver atrás.",
    save_flush_failed: "Windows no confirmó que el archivo llegara al disco: {err}\n\nEl cambio está guardado y Espanso puede leerlo. En una memoria USB o una unidad de red esto es habitual; si el equipo se apaga en los próximos segundos, el cambio podría perderse.",
    err_validation_empty: "El activador (trigger) no puede estar vacío.",
    err_validation_duplicate: "Ya existe una expansión que usa exactamente el activador \"{name}\".",
    err_espansod_missing: "No se encontró espansod.exe junto a EspansoManager.exe (se esperaba en {path}).",
    err_espansod_timeout: "espansod.exe no respondió en {n} segundos y tuvo que forzarse su cierre.",
    err_espanso_start: "No se pudo iniciar Espanso: {err}",
    err_espanso_no_response: "Espanso no respondió después de intentar iniciarlo.",
    err_restart_unconfirmed: "Se guardó el cambio, pero Espanso no confirmó que se haya reiniciado correctamente. Es posible que necesites reiniciarlo manualmente.",
    err_espanso_action: "Espanso no pudo {verb}.\n\n{detail}",
    err_espanso_comm: "No se pudo comunicar con Espanso para {verb}: {err}",
    verb_pause_timed: "pausar temporalmente",
    verb_pause: "pausar",
    verb_resume: "activar",
    verb_auto_resume: "reactivar automáticamente",

    tray_pause: "Pausar",
    tray_resume: "Activar",
    tray_pause_10: "Pausar 10 minutos",
    tray_quit: "Salir",
    tray_tooltip: "Espanso — clic para abrir, clic derecho para pausar",

    already_running: "EspansoManager ya se está ejecutando. Búscalo en la bandeja del sistema, junto al reloj (puede estar oculto en la flecha de íconos ocultos).",
    instance_guard_warning: "Windows no concedió el bloqueo que impide que se ejecuten dos copias de EspansoManager a la vez: {err}\n\nLa aplicación se abrió de todas formas. La única consecuencia es que ahora podría abrirse una segunda copia junto a esta: si ves dos íconos en la bandeja del sistema, cierra uno.",
    settings_damaged: "El archivo de ajustes no se pudo leer como tal, así que EspansoManager arrancó con los valores por defecto: tus carpetas no están. El archivo anterior se guardó como {name}; no se ha borrado nada.",
    settings_damaged_lost: "El archivo de ajustes no se pudo leer como tal, así que EspansoManager arrancó con los valores por defecto: tus carpetas no están. Tampoco se pudo apartar el archivo anterior, así que el próximo cambio que guardes lo reemplazará.",
    settings_unreadable: "No se pudo abrir el archivo de ajustes: {err}\n\nEspansoManager arrancó con los valores por defecto, así que tus carpetas no aparecen. El archivo no se ha tocado: cierra lo que pueda estar reteniéndolo y vuelve a abrir la aplicación.",
    panic_title: "EspansoManager encontró un error inesperado",
    panic_body: "EspansoManager tuvo que cerrarse por un error interno inesperado.\n\nTus expansiones guardadas no se ven afectadas.\n\nDetalle técnico ({location}):\n{message}",
    tray_error: "No se pudo crear el ícono en la bandeja del sistema: {err}\n\nEspanso sigue funcionando y se le devolvió su propio ícono: desde ahí puedes pausarlo o cerrarlo. Si vuelve a ocurrir, revisa que la carpeta .espanso-runtime conserve sus archivos .ico.",
    startup_espanso_warning: "No se pudo confirmar que Espanso esté funcionando: {err}\n\nEspansoManager seguirá abierto para que puedas revisar la configuración, pero las expansiones de texto podrían no estar activas todavía.",
    config_patch_warning: "No se pudo ajustar la configuración de Espanso: {err}\n\nEspanso podría seguir mostrando su icono en la bandeja y sus avisos de Windows junto a los de EspansoManager. Comprueba que default.yml se pueda leer y esté guardado como UTF-8.",
    hindi_font_missing: "Se seleccionó hindi, pero no se encontró en esta computadora ninguna fuente compatible con devanagari, así que el texto podría no verse bien. Elige otro idioma si ves cuadros vacíos.",
};

pub static FIL: Strings = Strings {
    app_title: "Aking mga text expansion",
    settings_button: "Mga setting",
    tips_title: "Mga tip",
    tips_button_tip: "Mga tip",
    tips_footer: "Walang kailangang basahin dito. Ito ang mga bagay na madalas itanong.",
    tip_search_title: "Maghanap ng expansion nang hindi umaalis sa ginagawa mo",
    tip_search_body: "Pindutin ang {keys} kahit saan sa Windows at lilitaw ang isang search box. I-type ang bahagi ng trigger o ng teksto nito, pumili, at ipapasok ito kung nasaan ang cursor mo.",
    onboarding_title: "Maligayang pagdating",
    onboarding_intro: "Nagta-type ka ng maikling trigger at nagiging buong teksto ito na na-save mo. Gumagana sa kahit anong programa: email, chat, dokumento, browser.",
    onboarding_try_title: "Tingnan mo kung paano gumagana",
    onboarding_try_body: "I-type ang  :espanso  sa kahon sa ibaba — buo at walang espasyo.",
    onboarding_try_placeholder: "Subukan dito",
    onboarding_try_ok: "Ganoon lang iyon. Ganito rin ang gagawin nito sa iba pang programa.",
    onboarding_autostart_title: "Simulan kasama ng Windows",
    onboarding_autostart_note: "Mababago mo ito kahit kailan, sa Mga setting.",
    onboarding_where_title: "Saan ito nakatira",
    onboarding_where_body: "Sa system tray, katabi ng orasan — isang click at babalik ang window. Itinatago ng Windows ang mga bagong icon sa likod ng maliit na arrow, kaya i-drag ito sa taskbar para laging nakikita.",
    onboarding_start: "Magsimula",
    search_window_hint: "Maghanap sa mga expansion mo",
    search_window_empty: "Walang tumutugma.",
    search_window_footer: "↑ ↓ para gumalaw · Enter para ipasok · Esc para isara",
    tip_where_title: "Saan napunta ang window?",
    tip_where_body: "Nasa system tray ito, katabi ng orasan. Isang click at babalik. Ang pagsasara gamit ang X ay hindi nagpapatay nito — itinatabi lang, para hindi humarang sa trabaho mo.",
    tip_pin_title: "Panatilihing nakikita ang icon",
    tip_pin_body: "Itinatago ng Windows ang mga bagong tray icon sa likod ng maliit na arrow. Buksan ito at i-drag ang icon ng EspansoManager pababa sa taskbar — doon na ito mananatili.",
    tip_undo_title: "Tumakbo ang expansion nang hindi mo sinasadya",
    tip_undo_body: "Pindutin ang Esc, isang beses, agad pagkatapos. Mababawi ang expansion at mananatili ang teksto mo gaya ng pagkakasulat mo.",
    tip_pause_title: "I-pause kapag nakakaabala",
    tip_pause_body: "I-right-click ang tray icon: i-pause nang sampung minuto, o i-pause hanggang sabihin mo. Kapaki-pakinabang kapag may tinitipa kang makakasagabal sa mga trigger mo — isang password, isang piraso ng code.",
    tip_select_title: "Maraming expansion nang sabay",
    tip_select_body: "Sa listahan, pindutin nang matagal ang Ctrl para pumili isa-isa, o Shift para kunin ang buong hanay. Pagkatapos ay maaari mong burahin ang mga ito, o alisin sa folder nila, nang sabay-sabay.",
    new_expansion: "Bagong expansion",
    search_label: "Maghanap ng expansion",
    empty_title: "Wala ka pang anumang text expansion.",
    empty_hint: "Gamitin ang butones na \"Bagong expansion\" sa itaas para gumawa ng una mo.",
    no_matches: "Walang expansion na tumutugma sa iyong paghahanap.",
    view_comfortable_tip: "Karaniwang view",
    view_compact_tip: "Compact na view",
    prefix_field_hint: "prefix",
    word_field_hint: "salita",
    trigger_preview: "Ita-type mo: {trigger}",
    made_by: "EspansoManager — ginawa gamit ang AI ni Alex Palacios",
    credits_espanso: "Nakabatay sa Espanso, na nilikha ni Federico Terzi at ng mga kontribyutor nito.",
    edit_button: "✏ I-edit",
    edit_tip: "I-edit",
    delete: "Burahin",
    delete_tip: "Burahin",
    rename_folder_tip: "Palitan ang pangalan ng folder",
    delete_folder_tip: "Burahin ang folder",
    save_name_tip: "I-save ang pangalan",
    cancel: "Kanselahin",
    cancel_tip: "Kanselahin",
    selected_count: "{n} ang napili",
    remove_from_folder: "Ilabas sa folder",
    delete_selected: "Burahin ang mga napili",
    clear_selection_tip: "Alisin ang pagpili",
    moving_n: "Inililipat ang {n} na expansion",

    confirm_delete_folder_title: "Burahin ang folder na \"{name}\"",
    confirm_delete_folder_body: "Buburahin mo ang folder na \"{name}\", na naglalaman ng {n} expansion. Kapag nagpatuloy ka, mabubura ANG LAHAT ng expansion sa folder na ito. Hindi ito maaaring bawiin.",
    confirm_delete_selection_title: "Burahin ang {n} expansion",
    confirm_delete_selection_body: "Buburahin mo ang {n} napiling expansion. Hindi ito maaaring bawiin.",
    see_more: "Tingnan pa ({n} pa)",

    edit_title_new: "Bagong text expansion",
    edit_title_existing: "I-edit ang text expansion",
    when_you_type: "Kapag i-type mo ito:",
    will_be_replaced_by: "Papalitan ito ng:",
    kind_text: "Teksto",
    kind_date: "Kasalukuyang petsa o oras",
    preset_date_only: "Petsa lamang (ISO 8601)",
    preset_datetime_utc: "Petsa at oras sa UTC (ISO 8601)",
    preset_month_day_year: "Buwan, araw at taon",
    preset_day_month_year: "Araw, buwan at taon",
    month_language_label: "Wika ng buwan",
    month_language_hint: "Nangingibabaw ito sa wika ng Windows at sa wika ng app: ganito isusulat ang buwan sa kahit anong kompyuter.",
    preset_custom: "Pasadya",
    blocks_hint: "I-drag ang mga bloke sa ibaba para buuin ang iyong format, o i-click ang isa para idagdag sa dulo.",
    blocks_empty: "Ihulog dito ang mga bloke",
    blocks_fields: "Mga bahagi ng petsa",
    blocks_separators: "Mga separator (paulit-ulit na magagamit)",
    blocks_remove_tip: "Alisin",
    blocks_advanced: "Gumagamit ang format na ito ng mga code na hindi maipapakita ng mga bloke. I-edit ito bilang teksto sa ibaba.",
    block_year: "Taon",
    block_month_name: "Buwan (pangalan)",
    block_month_number: "Buwan (numero)",
    block_day: "Araw",
    block_hour: "Oras (lokal)",
    block_hour_utc: "Oras (UTC)",
    blocks_utc_note: "Ang buong petsa ay nasa UTC, hindi lang ang oras — ganoon nagiging magkatugma ang timestamp.",
    block_minute: "Minuto",
    block_space: "espasyo",
    format_label: "Format (strftime code):",
    folder_optional: "Folder (opsyonal):",
    no_folder: "Walang folder",
    new_folder_placeholder: "Bagong folder…",
    add_new_folder: "+ Bagong folder…",
    name_label: "Pangalan:",
    save: "I-save",

    back: "Bumalik",
    settings_title: "Mga setting",
    autostart_section: "Awtomatikong pagsisimula",
    autostart_checkbox: "Simulan ang EspansoManager (at Espanso) kasama ng Windows",
    autostart_hint: "Tatakbo ito sa background, sa system tray.",
    appearance_section: "Hitsura",
    theme_system: "Sistema",
    theme_light: "Maliwanag na tema",
    theme_dark: "Madilim na tema",
    theme_hint: "Ang \"Sistema\" ay sumusunod sa setting ng Windows na maliwanag/madilim.",
    language_section: "Wika",
    language_hint: "Binabago lamang nito ang wika ng window na ito, hindi ang iyong mga expansion.",
    transfer_section: "I-export at i-import",
    transfer_hint: "Ipadala ang iyong mga expansion sa isang katrabaho, o idagdag ang sa kanila sa iyo. Ang pag-import ay nagdaragdag lamang: walang pinapalitan sa mayroon ka na.",
    export_button: "I-export ang aking mga expansion…",
    import_button: "Mag-import ng mga expansion…",
    transfer_file_kind: "Export ng EspansoManager",
    export_done: "Na-export: {n}.",
    export_error: "Hindi maisulat ang file: {err}",
    import_done: "Naidagdag: {n}.",
    import_done_skipped: "Naidagdag: {n}. Nilaktawan: {k} — ginagamit na ang trigger na iyon.",
    import_none_added: "Walang naidagdag: ginagamit na ang lahat ng trigger sa file na iyon.",
    import_not_ours: "Mukhang hindi ito isang export ng EspansoManager.",
    import_error: "Hindi mabasa ang file: {err}",
    prefix_section: "Prefix para sa mga bagong expansion",
    prefix_hint: "Ang prefix ang tine-type mo bago ang bawat trigger, halimbawa \":\" sa \":kumusta\". Piliin ang gusto mo.",
    custom_label: "Pasadya:",
    apply_prefix: "Ilapat ang kasalukuyang prefix sa mga umiiral kong expansion",
    apply_prefix_hint: "Binabago nito ang paunang prefix ng bawat expansion na mayroon ka na tungo sa napili mo sa itaas (halimbawa, mula \"::kumusta\" tungong \":kumusta\").",

    banner_close_tip: "Isara",
    undo: "I-undo",
    move_undone: "Naibalik ang paglipat.",
    autostart_on: "Magsisimula na ang EspansoManager kasama ng Windows.",
    autostart_off: "Hindi na ito awtomatikong magsisimula kasama ng Windows.",
    autostart_error: "Hindi mabago ang setting ng pagsisimula sa Windows: {err}",
    prefix_save_error: "Hindi ma-save ang setting ng prefix: {err}",
    view_save_error: "Hindi ma-save ang setting ng view: {err}",
    language_save_error: "Hindi ma-save ang setting ng wika: {err}",
    theme_save_error: "Hindi ma-save ang setting ng tema: {err}",
    prefix_already_applied: "Ginagamit na ng lahat ng iyong expansion ang prefix na ito.",
    prefix_collision: "Hindi inilapat ang pagbabago: hindi bababa sa dalawang expansion ang magkakaroon ng parehong trigger ({list}). Pakisuri muna ang mga ito bago palitan ang prefix.",
    prefix_confirm_title: "Palitan ang prefix ng mga umiiral na expansion",
    prefix_confirm_body: "Papalitan ang pangalan ng {n} trigger:\n\n{list}{more}\n\nMagpatuloy?",
    prefix_confirm_more: "\n  … at {n} pa",
    prefix_applied: "Na-update ang prefix ng {n} expansion.",
    delete_one_title: "Burahin ang expansion",
    delete_one_body: "Burahin ang expansion na \"{name}\"? Hindi ito maaaring bawiin.",
    expansion_deleted: "Nabura ang expansion.",
    trigger_empty: "Hindi maaaring walang laman ang trigger.",
    trigger_duplicate: "May ibang expansion na gumagamit na ng trigger na \"{name}\".",
    trigger_shadow: "Agad tumatakbo ang \"{short}\" pagkatapos mong i-type, kaya hindi kailanman gagana ang \"{long}\".",
    expansion_saved: "Na-save ang expansion.",
    folder_deleted: "Nabura ang folder na \"{name}\" at ang {n} expansion nito.",
    selection_deleted: "Nabura ang {n} expansion.",
    folder_name_taken: "May folder nang pinangalanang \"{name}\".",
    folder_renamed: "Pinalitan ang pangalan ng folder tungong \"{name}\".",
    moved_to_folder: "{n} expansion ang inilipat sa \"{name}\".",
    removed_from_folder: "{n} expansion ang inilabas sa kanilang folder.",

    preview_date: "Petsa/oras — hal. {example}",
    preview_advanced_vars: "Advanced (gumagamit ng espesyal na variable) — i-edit ito sa .yml file",
    preview_advanced: "Advanced — i-edit ito sa .yml file",

    err_read_file: "Hindi mabasa ang file ng mga expansion: {err}",
    err_parse_yaml: "May maling YAML format ang file ng mga expansion (base.yml) kaya hindi ito mabasa:\n{err}\n\nHindi rin ito magagamit ng Espanso hangga't hindi naaayos. Maaari mo itong buksan sa isang text editor para ayusin nang manu-mano.",
    err_self_check: "Gumawa ang EspansoManager ng file na hindi nito mabasang muli, kaya walang na-save upang maprotektahan ang iyong mga umiiral na expansion. Teknikal na detalye: {err}",
    err_write_file: "Hindi ma-save ang file ng mga expansion: {err}",
    save_backup_failed: "Hindi muna nakagawa ng backup na kopya ng dating file: {err}\n\nNa-save ang pagbabago. Ang kulang ay ang kopya sa folder ng mga backup na sana'y magpapabalik sa iyo.",
    save_flush_failed: "Hindi kinumpirma ng Windows na nakarating sa disk ang file: {err}\n\nNa-save ang pagbabago at kayang basahin ito ng Espanso. Karaniwan ito sa USB stick o network drive; kung mawalan ng kuryente ang makina sa susunod na ilang segundo, maaaring mawala ang pagbabago.",
    err_validation_empty: "Hindi maaaring walang laman ang trigger.",
    err_validation_duplicate: "May expansion nang gumagamit ng eksaktong trigger na \"{name}\".",
    err_espansod_missing: "Hindi natagpuan ang espansod.exe katabi ng EspansoManager.exe (inaasahan sa {path}).",
    err_espansod_timeout: "Hindi tumugon ang espansod.exe sa loob ng {n} segundo kaya kinailangang isara.",
    err_espanso_start: "Hindi masimulan ang Espanso: {err}",
    err_espanso_no_response: "Hindi tumugon ang Espanso pagkatapos subukang simulan ito.",
    err_restart_unconfirmed: "Na-save ang pagbabago, ngunit hindi kinumpirma ng Espanso na nag-restart ito nang maayos. Maaaring kailanganin mong i-restart ito nang manu-mano.",
    err_espanso_action: "Hindi nagawa ng Espanso na {verb}.\n\n{detail}",
    err_espanso_comm: "Hindi makipag-ugnayan sa Espanso para {verb}: {err}",
    verb_pause_timed: "pansamantalang mag-pause",
    verb_pause: "mag-pause",
    verb_resume: "magpatuloy",
    verb_auto_resume: "awtomatikong magpatuloy",

    tray_pause: "I-pause",
    tray_resume: "Ipagpatuloy",
    tray_pause_10: "I-pause nang 10 minuto",
    tray_quit: "Lumabas",
    tray_tooltip: "Espanso — i-click para buksan, i-right-click para i-pause",

    already_running: "Tumatakbo na ang EspansoManager. Hanapin ito sa system tray, katabi ng orasan (maaaring nakatago ito sa likod ng arrow para sa mga nakatagong icon).",
    instance_guard_warning: "Hindi ibinigay ng Windows ang lock na pumipigil na tumakbo nang sabay ang dalawang kopya ng EspansoManager: {err}\n\nNagbukas pa rin ang app. Ang tanging epekto ay maaari nang magbukas ang pangalawang kopya sa tabi nito — kung may nakita kang dalawang icon sa system tray, isara mo ang isa.",
    settings_damaged: "Hindi mabasa bilang settings ang settings file mo, kaya nagsimula ang EspansoManager sa mga default — ibig sabihin, wala ang mga folder mo. Itinabi ang lumang file bilang {name}; walang binura.",
    settings_damaged_lost: "Hindi mabasa bilang settings ang settings file mo, kaya nagsimula ang EspansoManager sa mga default — ibig sabihin, wala ang mga folder mo. Hindi rin ito naitabi, kaya papalitan ito ng susunod mong i-save.",
    settings_unreadable: "Hindi mabuksan ang settings file mo: {err}\n\nNagsimula ang EspansoManager sa mga default, kaya hindi lumalabas ang mga folder mo. Hindi ginalaw ang file — isara ang anumang maaaring humahawak dito at buksang muli ang app.",
    panic_title: "Nakaranas ang EspansoManager ng hindi inaasahang error",
    panic_body: "Kinailangang isara ang EspansoManager dahil sa hindi inaasahang internal na error.\n\nHindi apektado ang iyong mga na-save na expansion.\n\nTeknikal na detalye ({location}):\n{message}",
    tray_error: "Hindi magawa ang icon sa system tray: {err}\n\nTumatakbo pa rin ang Espanso at naibalik na sa kanya ang sarili niyang icon — mula roon ay maaari mo siyang i-pause o isara. Kung paulit-ulit itong nangyayari, tingnan kung nasa .espanso-runtime folder pa ang mga .ico file nito.",
    startup_espanso_warning: "Hindi makumpirma na tumatakbo ang Espanso: {err}\n\nMananatiling bukas ang EspansoManager para masuri mo ang configuration, ngunit maaaring hindi pa aktibo ang mga text expansion.",
    config_patch_warning: "Hindi naiayos ang sariling mga setting ng Espanso: {err}\n\nMaaaring ipagpatuloy ng Espanso ang pagpapakita ng tray icon nito at ng mga notification ng Windows katabi ng sa EspansoManager. Tiyaking nababasa ang default.yml at naka-save ito bilang UTF-8.",
    hindi_font_missing: "Napili ang Hindi, ngunit walang natagpuang font na sumusuporta sa Devanagari sa computer na ito, kaya maaaring hindi maipakita nang tama ang teksto. Pumili ng ibang wika kung may nakikita kang mga blangkong kahon.",
};

pub static HI: Strings = Strings {
    app_title: "मेरे टेक्स्ट विस्तार",
    settings_button: "सेटिंग्स",
    tips_title: "सुझाव",
    tips_button_tip: "सुझाव",
    tips_footer: "यहाँ कुछ भी पढ़ना ज़रूरी नहीं है। ये वही बातें हैं जो लोग सबसे ज़्यादा पूछते हैं।",
    tip_search_title: "जो कर रहे हैं उसे छोड़े बिना कोई एक्सपैंशन ढूँढें",
    tip_search_body: "Windows में कहीं भी {keys} दबाएँ और एक खोज बॉक्स आ जाएगा। ट्रिगर का या उसके टेक्स्ट का कुछ हिस्सा लिखें, एक चुनें, और वह वहीं जुड़ जाएगा जहाँ आपका कर्सर था।",
    onboarding_title: "आपका स्वागत है",
    onboarding_intro: "आप एक छोटा ट्रिगर लिखते हैं और वह आपके सहेजे हुए पूरे टेक्स्ट में बदल जाता है। यह किसी भी प्रोग्राम में काम करता है: ईमेल, चैट, दस्तावेज़, ब्राउज़र।",
    onboarding_try_title: "इसे काम करते देखें",
    onboarding_try_body: "नीचे के बॉक्स में  :espanso  लिखें — पूरा, बिना जगह छोड़े।",
    onboarding_try_placeholder: "यहाँ आज़माएँ",
    onboarding_try_ok: "बस इतना ही। हर दूसरे प्रोग्राम में भी यह ऐसे ही काम करेगा।",
    onboarding_autostart_title: "Windows के साथ शुरू करें",
    onboarding_autostart_note: "इसे आप जब चाहें सेटिंग्स में बदल सकते हैं।",
    onboarding_where_title: "यह कहाँ रहता है",
    onboarding_where_body: "सिस्टम ट्रे में, घड़ी के पास — एक क्लिक और विंडो वापस आ जाती है। Windows नए आइकन को छोटे तीर के पीछे छिपा देता है, इसलिए इसे टास्कबार पर खींच लाएँ ताकि हमेशा दिखता रहे।",
    onboarding_start: "शुरू करें",
    search_window_hint: "अपने एक्सपैंशन खोजें",
    search_window_empty: "कुछ भी मेल नहीं खाता।",
    search_window_footer: "↑ ↓ चलने के लिए · Enter डालने के लिए · Esc बंद करने के लिए",
    tip_where_title: "विंडो कहाँ चली गई?",
    tip_where_body: "यह सिस्टम ट्रे में रहती है, घड़ी के पास। एक क्लिक और वापस आ जाती है। X से बंद करने पर यह बंद नहीं होती — बस रख दी जाती है, ताकि काम करते समय बीच में न आए।",
    tip_pin_title: "आइकन को हमेशा दिखने दें",
    tip_pin_body: "Windows नए ट्रे आइकन को छोटे तीर के पीछे छिपा देता है। उसे खोलें और EspansoManager के आइकन को टास्कबार पर खींच लाएँ — फिर वह वहीं रहेगा।",
    tip_undo_title: "एक्सपैंशन बिना चाहे चल गया",
    tip_undo_body: "तुरंत बाद एक बार Esc दबाएँ। एक्सपैंशन पलट जाएगा और आपका टेक्स्ट वैसा ही रहेगा जैसा आपने लिखा था।",
    tip_pause_title: "जब बाधा बने तो रोक दें",
    tip_pause_body: "ट्रे आइकन पर दायाँ क्लिक करें: दस मिनट के लिए रोकें, या जब तक आप न कहें तब तक रोकें। तब काम आता है जब आप कुछ ऐसा टाइप कर रहे हों जिसमें आपके ट्रिगर बाधा डालें — कोई पासवर्ड, कोई कोड।",
    tip_select_title: "एक साथ कई पर काम करें",
    tip_select_body: "सूची में, एक-एक चुनने के लिए Ctrl दबाए रखें, या पूरी श्रृंखला लेने के लिए Shift। फिर आप उन्हें एक ही बार में मिटा सकते हैं, या उनके फ़ोल्डर से बाहर निकाल सकते हैं।",
    new_expansion: "नया विस्तार",
    search_label: "एक्सपैंशन खोजें",
    empty_title: "आपके पास अभी कोई टेक्स्ट विस्तार नहीं है।",
    empty_hint: "पहला बनाने के लिए ऊपर दिए \"नया विस्तार\" बटन का उपयोग करें।",
    no_matches: "आपकी खोज से कोई विस्तार मेल नहीं खाता।",
    view_comfortable_tip: "सामान्य दृश्य",
    view_compact_tip: "संक्षिप्त दृश्य",
    prefix_field_hint: "उपसर्ग",
    word_field_hint: "शब्द",
    trigger_preview: "आप टाइप करेंगे: {trigger}",
    made_by: "EspansoManager — Alex Palacios द्वारा AI से बनाया गया।",
    credits_espanso: "Espanso पर आधारित, जिसे Federico Terzi और उनके योगदानकर्ताओं ने बनाया है।",
    edit_button: "✏ संपादित करें",
    edit_tip: "संपादित करें",
    delete: "हटाएँ",
    delete_tip: "हटाएँ",
    rename_folder_tip: "फ़ोल्डर का नाम बदलें",
    delete_folder_tip: "फ़ोल्डर हटाएँ",
    save_name_tip: "नाम सहेजें",
    cancel: "रद्द करें",
    cancel_tip: "रद्द करें",
    selected_count: "{n} चयनित",
    remove_from_folder: "फ़ोल्डर से बाहर निकालें",
    delete_selected: "चयनित हटाएँ",
    clear_selection_tip: "चयन रद्द करें",
    moving_n: "{n} विस्तार ले जाए जा रहे हैं",

    confirm_delete_folder_title: "फ़ोल्डर \"{name}\" हटाएँ",
    confirm_delete_folder_body: "आप फ़ोल्डर \"{name}\" हटाने जा रहे हैं, जिसमें {n} विस्तार हैं। आगे बढ़ने पर इस फ़ोल्डर के सभी विस्तार हट जाएँगे। यह क्रिया पूर्ववत नहीं की जा सकती।",
    confirm_delete_selection_title: "{n} विस्तार हटाएँ",
    confirm_delete_selection_body: "आप {n} चयनित विस्तार हटाने जा रहे हैं। यह क्रिया पूर्ववत नहीं की जा सकती।",
    see_more: "और देखें ({n} और)",

    edit_title_new: "नया टेक्स्ट विस्तार",
    edit_title_existing: "टेक्स्ट विस्तार संपादित करें",
    when_you_type: "जब आप यह टाइप करें:",
    will_be_replaced_by: "इसे इससे बदला जाएगा:",
    kind_text: "टेक्स्ट",
    kind_date: "वर्तमान दिनांक या समय",
    preset_date_only: "केवल दिनांक (ISO 8601)",
    preset_datetime_utc: "UTC में दिनांक और समय (ISO 8601)",
    preset_month_day_year: "महीना, दिन और वर्ष",
    preset_day_month_year: "दिन, महीना और वर्ष",
    month_language_label: "महीने की भाषा",
    month_language_hint: "यह Windows की भाषा और ऐप की भाषा, दोनों पर भारी पड़ती है: महीना किसी भी कंप्यूटर पर इसी भाषा में लिखा जाएगा।",
    preset_custom: "कस्टम",
    blocks_hint: "अपना प्रारूप बनाने के लिए नीचे के ब्लॉक खींचें, या किसी पर क्लिक करके उसे अंत में जोड़ें।",
    blocks_empty: "ब्लॉक यहाँ छोड़ें",
    blocks_fields: "दिनांक के हिस्से",
    blocks_separators: "विभाजक (बार-बार उपयोग करें)",
    blocks_remove_tip: "हटाएँ",
    blocks_advanced: "यह प्रारूप ऐसे कोड उपयोग करता है जो ब्लॉक नहीं दिखा सकते। इसे नीचे पाठ के रूप में संपादित करें।",
    block_year: "वर्ष",
    block_month_name: "महीना (नाम)",
    block_month_number: "महीना (संख्या)",
    block_day: "दिन",
    block_hour: "घंटा (स्थानीय)",
    block_hour_utc: "घंटा (UTC)",
    blocks_utc_note: "पूरी तारीख़ UTC में होगी, सिर्फ़ घंटा नहीं — इसी तरह टाइमस्टैम्प एक जैसा रहता है।",
    block_minute: "मिनट",
    block_space: "स्पेस",
    format_label: "प्रारूप (strftime कोड):",
    folder_optional: "फ़ोल्डर (वैकल्पिक):",
    no_folder: "कोई फ़ोल्डर नहीं",
    new_folder_placeholder: "नया फ़ोल्डर…",
    add_new_folder: "+ नया फ़ोल्डर…",
    name_label: "नाम:",
    save: "सहेजें",

    back: "वापस",
    settings_title: "सेटिंग्स",
    autostart_section: "स्वचालित शुरुआत",
    autostart_checkbox: "Windows के साथ EspansoManager (और Espanso) शुरू करें",
    autostart_hint: "यह पृष्ठभूमि में, सिस्टम ट्रे में चलेगा।",
    appearance_section: "रूप",
    theme_system: "सिस्टम",
    theme_light: "हल्की थीम",
    theme_dark: "गहरी थीम",
    theme_hint: "\"सिस्टम\" आपकी Windows की हल्की/गहरी सेटिंग का अनुसरण करता है।",
    language_section: "भाषा",
    language_hint: "यह केवल इस विंडो की भाषा बदलता है, आपके विस्तारों की नहीं।",
    transfer_section: "निर्यात और आयात",
    transfer_hint: "अपने विस्तार किसी सहकर्मी को भेजें, या उनके विस्तार अपने में जोड़ें। आयात केवल जोड़ता है: आपके पास जो पहले से है, वह नहीं बदला जाता।",
    export_button: "मेरे विस्तार निर्यात करें…",
    import_button: "विस्तार आयात करें…",
    transfer_file_kind: "EspansoManager निर्यात",
    export_done: "निर्यात किए गए: {n}।",
    export_error: "फ़ाइल नहीं लिखी जा सकी: {err}",
    import_done: "जोड़े गए: {n}।",
    import_done_skipped: "जोड़े गए: {n}। छोड़े गए: {k} — वह ट्रिगर पहले से उपयोग में था।",
    import_none_added: "कुछ नहीं जोड़ा गया: उस फ़ाइल के सभी ट्रिगर पहले से उपयोग में हैं।",
    import_not_ours: "यह फ़ाइल EspansoManager का निर्यात नहीं लगती।",
    import_error: "फ़ाइल नहीं पढ़ी जा सकी: {err}",
    prefix_section: "नए विस्तारों के लिए उपसर्ग",
    prefix_hint: "उपसर्ग वह है जो आप हर ट्रिगर से पहले टाइप करते हैं, जैसे \":नमस्ते\" में \":\"। अपनी पसंद का चुनें।",
    custom_label: "कस्टम:",
    apply_prefix: "वर्तमान उपसर्ग को मेरे मौजूदा विस्तारों पर लागू करें",
    apply_prefix_hint: "यह आपके पास पहले से मौजूद हर विस्तार के शुरुआती उपसर्ग को ऊपर चुने गए उपसर्ग में बदल देता है (उदाहरण के लिए, \"::नमस्ते\" से \":नमस्ते\")।",

    banner_close_tip: "बंद करें",
    undo: "पूर्ववत करें",
    move_undone: "स्थानांतरण पूर्ववत कर दिया गया।",
    autostart_on: "अब EspansoManager Windows के साथ शुरू होगा।",
    autostart_off: "अब यह Windows के साथ स्वचालित रूप से शुरू नहीं होगा।",
    autostart_error: "Windows स्टार्ट-अप सेटिंग नहीं बदली जा सकी: {err}",
    prefix_save_error: "उपसर्ग सेटिंग सहेजी नहीं जा सकी: {err}",
    view_save_error: "दृश्य सेटिंग सहेजी नहीं जा सकी: {err}",
    language_save_error: "भाषा सेटिंग सहेजी नहीं जा सकी: {err}",
    theme_save_error: "थीम सेटिंग सहेजी नहीं जा सकी: {err}",
    prefix_already_applied: "आपके सभी विस्तार पहले से ही इस उपसर्ग का उपयोग करते हैं।",
    prefix_collision: "परिवर्तन लागू नहीं किया गया: कम से कम दो विस्तारों का ट्रिगर एक जैसा हो जाएगा ({list})। उपसर्ग बदलने से पहले कृपया उनकी समीक्षा करें।",
    prefix_confirm_title: "मौजूदा विस्तारों का उपसर्ग बदलें",
    prefix_confirm_body: "{n} ट्रिगर का नाम बदला जाएगा:\n\n{list}{more}\n\nजारी रखें?",
    prefix_confirm_more: "\n  … और {n} अधिक",
    prefix_applied: "{n} विस्तार का उपसर्ग अपडेट किया गया।",
    delete_one_title: "विस्तार हटाएँ",
    delete_one_body: "क्या विस्तार \"{name}\" हटाना है? यह क्रिया पूर्ववत नहीं की जा सकती।",
    expansion_deleted: "विस्तार हटा दिया गया।",
    trigger_empty: "ट्रिगर खाली नहीं हो सकता।",
    trigger_duplicate: "ट्रिगर \"{name}\" का उपयोग पहले से ही कोई अन्य विस्तार कर रहा है।",
    trigger_shadow: "\"{short}\" टाइप करते ही चल जाता है, इसलिए \"{long}\" कभी काम नहीं करेगा।",
    expansion_saved: "विस्तार सहेजा गया।",
    folder_deleted: "फ़ोल्डर \"{name}\" और उसके {n} विस्तार हटा दिए गए।",
    selection_deleted: "{n} विस्तार हटा दिए गए।",
    folder_name_taken: "\"{name}\" नाम का फ़ोल्डर पहले से मौजूद है।",
    folder_renamed: "फ़ोल्डर का नाम बदलकर \"{name}\" कर दिया गया।",
    moved_to_folder: "{n} विस्तार \"{name}\" में ले जाए गए।",
    removed_from_folder: "{n} विस्तार उनके फ़ोल्डर से बाहर निकाले गए।",

    preview_date: "दिनांक/समय — जैसे {example}",
    preview_advanced_vars: "उन्नत (विशेष वेरिएबल का उपयोग करता है) — इसे .yml फ़ाइल में संपादित करें",
    preview_advanced: "उन्नत — इसे .yml फ़ाइल में संपादित करें",

    err_read_file: "विस्तार फ़ाइल पढ़ी नहीं जा सकी: {err}",
    err_parse_yaml: "विस्तार फ़ाइल (base.yml) में YAML प्रारूप त्रुटि है और इसे पढ़ा नहीं जा सका:\n{err}\n\nजब तक इसे ठीक नहीं किया जाता, Espanso भी इसका उपयोग नहीं कर पाएगा। आप इसे टेक्स्ट एडिटर में खोलकर स्वयं ठीक कर सकते हैं।",
    err_self_check: "EspansoManager ने एक ऐसी फ़ाइल बनाई जिसे वह दोबारा नहीं पढ़ सका, इसलिए आपके मौजूदा विस्तारों की सुरक्षा के लिए कुछ भी सहेजा नहीं गया। तकनीकी विवरण: {err}",
    err_write_file: "विस्तार फ़ाइल सहेजी नहीं जा सकी: {err}",
    save_backup_failed: "पहले पिछली फ़ाइल की बैकअप प्रति नहीं बनाई जा सकी: {err}\n\nबदलाव सहेजा जा चुका है। जो नहीं है वह बैकअप फ़ोल्डर की वह प्रति है जिससे आप पीछे लौट सकते थे।",
    save_flush_failed: "Windows ने पुष्टि नहीं की कि फ़ाइल डिस्क तक पहुँची: {err}\n\nबदलाव सहेजा जा चुका है और Espanso उसे पढ़ सकता है। USB ड्राइव या नेटवर्क ड्राइव पर यह आम है; अगर अगले कुछ सेकंड में मशीन बंद हो जाए तो बदलाव खो सकता है।",
    err_validation_empty: "ट्रिगर खाली नहीं हो सकता।",
    err_validation_duplicate: "ठीक \"{name}\" ट्रिगर का उपयोग करने वाला एक विस्तार पहले से मौजूद है।",
    err_espansod_missing: "EspansoManager.exe के साथ espansod.exe नहीं मिला ({path} पर अपेक्षित था)।",
    err_espansod_timeout: "espansod.exe ने {n} सेकंड में उत्तर नहीं दिया और उसे बंद करना पड़ा।",
    err_espanso_start: "Espanso शुरू नहीं किया जा सका: {err}",
    err_espanso_no_response: "शुरू करने की कोशिश के बाद भी Espanso ने उत्तर नहीं दिया।",
    err_restart_unconfirmed: "परिवर्तन सहेजा गया, लेकिन Espanso ने पुष्टि नहीं की कि वह सही ढंग से पुनः आरंभ हुआ। आपको इसे स्वयं पुनः आरंभ करना पड़ सकता है।",
    err_espanso_action: "Espanso {verb} नहीं कर सका।\n\n{detail}",
    err_espanso_comm: "{verb} के लिए Espanso से संपर्क नहीं हो सका: {err}",
    verb_pause_timed: "अस्थायी रूप से रोकना",
    verb_pause: "रोकना",
    verb_resume: "सक्रिय करना",
    verb_auto_resume: "स्वचालित रूप से फिर से सक्रिय करना",

    tray_pause: "रोकें",
    tray_resume: "सक्रिय करें",
    tray_pause_10: "10 मिनट के लिए रोकें",
    tray_quit: "बाहर निकलें",
    tray_tooltip: "Espanso — खोलने के लिए क्लिक करें, रोकने के लिए दायाँ क्लिक करें",

    already_running: "EspansoManager पहले से चल रहा है। इसे सिस्टम ट्रे में, घड़ी के पास देखें (यह छिपे हुए आइकनों वाले तीर के पीछे हो सकता है)।",
    instance_guard_warning: "Windows ने वह लॉक नहीं दिया जो EspansoManager की दो प्रतियों को एक साथ चलने से रोकता है: {err}\n\nऐप फिर भी खुल गया। इसका एकमात्र असर यह है कि अब इसके साथ दूसरी प्रति भी खुल सकती है — अगर सिस्टम ट्रे में दो आइकन दिखें, तो एक बंद कर दें।",
    settings_damaged: "आपकी सेटिंग्स फ़ाइल सेटिंग्स के रूप में नहीं पढ़ी जा सकी, इसलिए EspansoManager डिफ़ॉल्ट मानों के साथ शुरू हुआ — यानी आपके फ़ोल्डर मौजूद नहीं हैं। पुरानी फ़ाइल {name} के नाम से रख दी गई है; कुछ भी मिटाया नहीं गया।",
    settings_damaged_lost: "आपकी सेटिंग्स फ़ाइल सेटिंग्स के रूप में नहीं पढ़ी जा सकी, इसलिए EspansoManager डिफ़ॉल्ट मानों के साथ शुरू हुआ — यानी आपके फ़ोल्डर मौजूद नहीं हैं। पुरानी फ़ाइल हटाकर सुरक्षित भी नहीं की जा सकी, इसलिए अगली बार सहेजने पर वह बदल जाएगी।",
    settings_unreadable: "आपकी सेटिंग्स फ़ाइल खोली नहीं जा सकी: {err}\n\nEspansoManager डिफ़ॉल्ट मानों के साथ शुरू हुआ, इसलिए आपके फ़ोल्डर नहीं दिख रहे। फ़ाइल को छुआ नहीं गया है — जो कुछ भी उसे रोक रहा हो उसे बंद करें और ऐप दोबारा खोलें।",
    panic_title: "EspansoManager में एक अप्रत्याशित त्रुटि हुई",
    panic_body: "एक अप्रत्याशित आंतरिक त्रुटि के कारण EspansoManager को बंद करना पड़ा।\n\nआपके सहेजे गए विस्तार प्रभावित नहीं हुए हैं।\n\nतकनीकी विवरण ({location}):\n{message}",
    tray_error: "सिस्टम ट्रे आइकन नहीं बनाया जा सका: {err}\n\nEspanso अब भी चल रहा है और उसका अपना आइकन उसे लौटा दिया गया है — वहीं से आप उसे रोक या बंद कर सकते हैं। अगर यह बार-बार हो, तो देखें कि .espanso-runtime फ़ोल्डर में उसकी .ico फ़ाइलें मौजूद हैं या नहीं।",
    startup_espanso_warning: "पुष्टि नहीं हो सकी कि Espanso चल रहा है: {err}\n\nEspansoManager खुला रहेगा ताकि आप कॉन्फ़िगरेशन देख सकें, लेकिन हो सकता है कि टेक्स्ट विस्तार अभी सक्रिय न हों।",
    config_patch_warning: "Espanso की अपनी सेटिंग्स नहीं बदली जा सकीं: {err}\n\nEspansoManager के साथ-साथ Espanso अपना ट्रे आइकॉन और Windows सूचनाएँ दिखाता रह सकता है। जाँचें कि default.yml पढ़ा जा सकता है और UTF-8 में सहेजा गया है।",
    hindi_font_missing: "हिन्दी चुनी गई, लेकिन इस कंप्यूटर पर देवनागरी का समर्थन करने वाला कोई फ़ॉन्ट नहीं मिला, इसलिए टेक्स्ट सही ढंग से नहीं दिख सकता। यदि आपको खाली बॉक्स दिखें तो कोई दूसरी भाषा चुनें।",
};

#[cfg(test)]
mod tests {
    use super::*;

    /// This very file, read at compile time. Only in the test build, so the release binary does
    /// not carry a copy of its own source.
    const SOURCE: &str = include_str!("i18n.rs");

    #[test]
    fn a_substituted_value_is_never_scanned_again() {
        // The real shape of the bug: the folder name goes in first, and the old implementation
        // then went looking for `{n}` in the string it had just built.
        assert_eq!(
            fill(
                "Deleted \"{name}\" and its {n} expansions",
                &[("name", "{n}"), ("n", "3")]
            ),
            "Deleted \"{n}\" and its 3 expansions"
        );
    }

    /// Argument order must not matter any more. Same arguments, opposite order, same answer.
    #[test]
    fn the_order_of_the_arguments_does_not_change_the_result() {
        let template = "{a} then {b}";
        let forwards = fill(template, &[("a", "{b}"), ("b", "second")]);
        let backwards = fill(template, &[("b", "second"), ("a", "{b}")]);
        assert_eq!(forwards, backwards);
        assert_eq!(forwards, "{b} then second");
    }

    /// A placeholder nobody supplied stays visible. It used to disappear, taking the value with
    /// it — an error message with no error in it, in one language only.
    #[test]
    fn a_placeholder_with_no_argument_is_left_where_it_is() {
        assert_eq!(fill("failed: {err}", &[]), "failed: {err}");
        assert_eq!(fill("{a} and {b}", &[("a", "one")]), "one and {b}");
    }

    /// Braces that are not placeholders are text like any other, including in scripts where a
    /// byte index is not a character index.
    #[test]
    fn stray_braces_pass_through_untouched() {
        assert_eq!(fill("100% {sure", &[("sure", "x")]), "100% {sure");
        assert_eq!(fill("saldo }{ raro", &[]), "saldo }{ raro");
        assert_eq!(fill("ñandú {n} पूर्ण", &[("n", "2")]), "ñandú 2 पूर्ण");
    }

    /// The `{token}`s in one line, sorted, ignoring anything that is not shaped like a placeholder.
    fn placeholders(line: &'static str) -> Vec<&'static str> {
        let mut found = Vec::new();
        let mut rest = line;
        while let Some(open) = rest.find('{') {
            let after = &rest[open + 1..];
            match after.find('}') {
                Some(close) => {
                    let key = &after[..close];
                    if !key.is_empty() && key.chars().all(|c| c.is_ascii_alphabetic() || c == '_') {
                        found.push(key);
                    }
                    rest = &after[close + 1..];
                }
                None => rest = after,
            }
        }
        found.sort_unstable();
        found
    }

    /// The `key: "value",` of a table line, or `None` for anything else.
    fn field_key(line: &'static str) -> Option<&'static str> {
        let (key, rest) = line.strip_prefix("    ")?.split_once(':')?;
        let looks_like_a_field = !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        (looks_like_a_field && rest.starts_with(" \"")).then_some(key)
    }

    /// Every field of one language table, paired with the placeholders in its value, by key.
    fn table(lang: &str) -> Vec<(&'static str, Vec<&'static str>)> {
        let opening = format!("pub static {lang}: Strings = Strings {{");
        let mut rows = Vec::new();
        let mut inside = false;
        for line in SOURCE.lines() {
            if line.starts_with(&opening) {
                inside = true;
            } else if inside {
                if line.starts_with("};") {
                    break;
                }
                if let Some(key) = field_key(line) {
                    rows.push((key, placeholders(line)));
                }
            }
        }
        rows.sort_by_key(|(key, _)| *key);
        rows
    }

    fn struct_field_count() -> usize {
        let mut count = 0;
        let mut inside = false;
        for line in SOURCE.lines() {
            if line.starts_with("pub struct Strings {") {
                inside = true;
            } else if inside {
                if line.starts_with('}') {
                    break;
                }
                if line.starts_with("    pub ") {
                    count += 1;
                }
            }
        }
        count
    }

    /// The struct guarantees that every language *has* every string. It cannot say anything about
    /// what is inside them, and that is where the damage lives: `{nombre}` written where English
    /// has `{name}`, or `{err}` dropped from a failure message, breaks one language only and no
    /// compiler will ever mention it.
    ///
    /// Read out of this file's own source rather than off the structs, because 190 named fields
    /// cannot be walked without a macro rewrite of a thousand lines of working translation.
    ///
    /// The count is checked against the struct first, and that check is the important one: it is
    /// what stops this test from quietly becoming a test of nothing if the tables are ever
    /// reformatted so the parser stops recognising them.
    #[test]
    fn placeholders_agree_across_languages() {
        let fields = struct_field_count();
        assert!(
            fields > 100,
            "the parser found only {fields} fields in `Strings` — it has stopped understanding \
             this file, and everything below it is checking nothing"
        );

        let en = table("EN");
        assert_eq!(en.len(), fields, "EN does not have one line per struct field");

        for lang in ["ES", "FIL", "HI"] {
            let other = table(lang);
            assert_eq!(
                other.len(),
                fields,
                "{lang} does not have one line per struct field"
            );
            for ((en_key, en_ph), (key, ph)) in en.iter().zip(other.iter()) {
                assert_eq!(en_key, key, "{lang} is missing `{en_key}`");
                assert_eq!(
                    en_ph, ph,
                    "{lang}'s `{key}` uses {ph:?} where English uses {en_ph:?}"
                );
            }
        }
    }
}
