# Changelog

All notable changes to EspansoManager. Dates are the day the work was finished, not the day it
was published.

## Unreleased — 0.0.2-dev

Everything below is built and installed locally as `0.0.2-dev`. Nothing here has been published.
When it is, this heading becomes `## 0.0.2 — <date>` and the `-dev` comes off the version.

### Added

- **Pick the folders when exporting and importing.** Export asks which folders to include;
  import reads the file first and offers the folders that file actually turned out to hold. The
  expansions that belong to no folder are a choice of their own, not a leftover. The question is
  skipped when there is only one folder, because a question with one answer is not a question.
- **Deleting a folder now asks first**, in the same style as the expansion warning.
- **Emoji are drawn with the font Windows itself uses** for them, instead of the small one
  bundled with the interface toolkit. `👍` and `💡` used to be an illegible tangle and a bare
  ring; now they read as what they are. They remain single-colour: the toolkit this app draws
  with paints every glyph in one flat colour, so colour emoji are not possible, and the Windows
  colour emoji font would have added 12 MB to the download without changing a single pixel.
  The app's own small icons — pencil, wastebasket, cross, plus, back arrow — are deliberately
  drawn from the bundled faces exactly as before, so no button changed shape.

### Changed

- **A quieter interface, with the detail beside the list** (2026-09-25). This supersedes parts of
  the 2026-09-22 interface described in the next entry:
  - The library title is centred, with the real state of Espanso (a dot and **Active**,
    **Paused** or **Not responding**) on its left, and three borderless commands on its right:
    pause/resume, new, and a `…` menu. The search and an **All folders** menu sit under it, short
    and centred. The folder rectangles are gone.
  - Rows are the trigger in a monospaced face and a one-line preview, split by faint lines.
    Choosing one opens an inspector on the right with the whole text (wrapped, scrollable, with
    a copy button), its folder, **Edit** and a `…` menu for move and delete. On a narrow window
    the inspector takes the place of the list, with **Back**.
  - The editor, settings, the folder page, the guide and the welcome screen lost their cards,
    numbers and subtitles: headings and space do the grouping. The editor asks before throwing
    away unsaved changes.
  - The move and export/import pickers are a list of folders with a check, the same lines as the
    folder menu. A new folder is made with **Create**, not **Save**.
  - Keyboard: Ctrl+N new, Ctrl+L or Ctrl+F search, arrows and Enter in the list, F2 folder
    options, Del delete (always asked), Ctrl+S save, Esc backs out one layer at a time. None of
    them act while text is being typed. Shortcuts are shown in tooltips, menus and the guide.
  - **Fluid hover in the list.** One soft highlight glides from row to row under the pointer
    instead of each row lighting up and going dark on its own. It fades in where it first lands
    and fades out in place when the pointer leaves; with Windows' animation effects off it only
    fades, it never travels. The same glide now runs through the rest of the app: the command
    bar's buttons, every menu, the switches in Settings and the topics of the guide. It never
    lands on a disabled line, and slides under the chosen segment without covering it. Outlined
    buttons and chips keep their own hover, because their fill is part of their shape.
  - The status dot sits at the middle of its word, not level with the top of the capitals. The
    start-up switch reads **Start EspansoManager with Windows**, without "(and Espanso)".
- **A new interface, from the library to the last dialog** (2026-09-22). One central library
  with nothing permanently docked beside it:
  - The title shows what Espanso really answered, not what the app assumes. The pill reads
    **Active**, **Paused** or **Not responding**. Pause / Resume sits right beside it.
  - Folders are rectangular filters with their counts. They run from **All** through each
    folder to **No folder**, and wrap onto more rows instead of scrolling sideways.
  - Each row reads **When you type → This text appears**. A row opens in place to show the whole
    trigger, the whole text, its folder and its actions. Closed rows carry no buttons.
  - Multiple selection is an explicit **Select** mode. **Select all** means the expansions on
    screen. Changing folder or search clears the selection and says so. The bulk actions stay
    disabled until something is selected.
  - The editor is two numbered steps, with a live preview and validation next to the field it
    concerns. **Save / Cancel** is pinned to the bottom so it never scrolls away. A text draft
    survives switching to a date and back.
  - Settings, the quick guide and the welcome screen were redone in the same style. All three
    themes (light, dark, follow Windows) were checked with equal care.

  Saving and coming back keeps the folder, the search, the scroll position and the open row.
  The saved row scrolls into view and is briefly tinted.

  This replaces the sidebar layout that reached the local build before it (unpublished). Its
  sidebar and permanent inspector are gone.
- **Every dialog is the same dialog.** Move, new folder, delete confirmations and the export and
  import folder pickers share one frame and margin, and one button order: action first, then
  Cancel, as Windows does it. Their width shrinks with the window. Moving to a folder shows every
  folder as the same rectangles the library filters with. It used to hide the sixth one behind a
  scroll bar.
- **Counts agree with their nouns** in all four languages ("1 expansion", not "1 expansions").
- **Keyboard focus is visible.** Tab now draws a ring around the focused button, folder, row,
  switch or segment in both themes. Before, only text boxes showed where the keyboard was.
- The smallest window is now 620×540 (it was 620×480). With 480 px, a second row of folders
  pushed the footer off the bottom. At 540, three rows of folders still fit. Anything taller
  scrolls the whole page instead of cutting it off.
- **The warning shown before deleting expansions is now drawn inside the app.** It used to be a
  system dialog, which meant it always looked light even in dark theme and matched nothing else
  on screen. It now follows the theme, lists the expansions **in the same order as the list
  behind it**, folds a long selection into a "Show N more" line instead of running off the edge
  of the window, and hands keyboard focus back where it came from when it closes. There is no
  warning triangle beside the heading, on purpose: Windows 11 does not use one in its own
  dialogs, and the red sentence and the red button already say what kind of question this is.
- The **Delete** button in that warning is legible in light theme. It used to be nearly
  invisible.
- Text being typed into a form survives cancelling that warning.
- **The accent colour is Windows' own again** (2026-09-25). The new interface had fixed it to a
  violet, and the code that read the system accent went unused and was removed. It is back: every
  accent-coloured button, link, chip and ring takes the colour chosen in Windows' settings, in the
  shade Windows itself uses for that theme.
- **Darker dark theme.** The background is near black (`#030303`) and cards and controls are
  `#242424`. The main text is `#E6E6E6` and supporting text is `#C8C8C8`. Hover, pressed and
  line shades were moved down with them so the steps between them stay the same.
- **The title bar uses Mica**, the Windows 11 material that takes a faint tint from the desktop
  wallpaper. Windows only draws it while the window is active, and a solid-colour desktop gives it
  nothing to tint with, so on such a desktop it looks the same as before. Versions of Windows
  older than 11 ignore it.
- **Folders take at most two rows.** Past that, the ones that do not fit wait behind a
  "**12 more**" chip that opens a list of them with their counts. The folder being viewed always
  keeps its chip. With thirty folders on the smallest window, the chips used to fill the screen and
  push the list out of sight. Only folders with a chip can take a dropped expansion; for the rest,
  there is **Move to folder**.

### Fixed

- **Lines of text of different sizes now share a baseline** instead of being centred on each
  other (2026-09-25). Folder counts no longer sit higher than the folder name. The
  Active/Paused word is level with the heading instead of 5 px above it. A closed row's trigger
  is level with its text. The footer note is level with its links.
- **"Quick guide", "Back" and the other link-style buttons that start a line** now have their
  first letter on the page's left edge, like every other line. Before, their invisible button
  padding put them 14 px further in.

- **Emptying a folder no longer deletes it.** A folder that existed only because of what was in
  it vanished when the last expansion was moved out. Folders like this came from an import or
  from typing a new name in the editor. The filter then jumped to All without a word. It now
  stays, empty, like one made with **New folder**.
- **Import no longer counts duplicates as additions.** The folder picker offered "2 will be
  added" when one of them already existed. It now counts only what will really be added. It
  hides folders that would add nothing, and reports what was skipped.
- **A failed restart no longer hides the import result.** The Espanso error used to replace
  "Added: N". Now the result comes first and the error below it.
- **Saving to a read-only `base.yml` keeps what you typed.** The change used to stay in memory
  though not on disk, and the draft was lost. Now the editor stays open with the draft intact,
  and the import is undone in the same way.
- **Hindi buttons were empty boxes.** The Devanagari face was missing from the font family that
  button labels use. Pause, New expansion, Select, Settings and the rest were illegible. This
  was already broken in 0.0.1.
- **The library footer fell off the bottom of the window.** The list height used a guessed header
  size. It now uses the measured one.
- **Text ran off the right edge** of the welcome screen and the quick guide in narrow windows.
- **A trigger made only of symbols can be created.** `:--`, `:_` and `:—` were refused with
  "the trigger cannot be empty", which was not what had happened: the word box was being read
  as a second prefix, which left no word at all. A word box holding nothing but symbols is now
  a word. The same rule reached two other places — "apply the current prefix to my existing
  expansions" would have rewritten `:--` to just `:`, and one such trigger in a folder used to
  refuse that whole folder's prefix change; both now leave it exactly as it is.
- **"Start with Windows" tells the truth about whether it will.** The switch only checked that
  a start-up entry with our name existed. An entry naming a copy of the app in a folder that
  has since been moved or renamed starts nothing, and an entry Windows itself has been switched
  off — in Task Manager's *Startup apps*, in Settings, or by any of the tidy-up utilities people
  run — starts nothing either, because Windows records that decision in a second place and does
  not delete the entry. Both read as on. The switch now checks that the registered command is
  *this* executable and that Windows has not vetoed it, and turning it on repairs both: it
  rewrites the path and withdraws the veto.
- **Text boxes losing focus on their own.** Clicking the search box, or a box in the
  new-expansion form, sometimes left it unable to accept a single character. Windows had left
  the thread with no focus window at all while the window itself stayed active, and the app now
  notices and puts focus back.

## 0.0.1 — 2026-09-06

First public release.
