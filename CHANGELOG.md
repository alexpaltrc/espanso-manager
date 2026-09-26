# Changelog

All notable changes to EspansoManager. Dates are the day the work was finished, not the day it
was published.

## Unreleased (0.0.3-dev)

### Fixed

- **The detail beside the list shows the text with its line breaks**, and its copy button copies
  them. Both used the list's one-line preview, so a three-line signature read, and was copied, as
  one line.
## 0.0.2 — 2026-09-25

What changed since 0.0.1. Designs that were tried along the way and replaced before publishing
are not listed.

### Added

- **Pick the folders when exporting and importing.** Export asks which folders to include;
  import reads the file first and offers the folders that file actually turned out to hold. The
  expansions that belong to no folder are a choice of their own, not a leftover. The question is
  skipped when there is only one folder, because a question with one answer is not a question.
- **Deleting a folder now asks first**, in the same style as the expansion warning, and the
  folder's page says beside the button that its expansions go with it.
- **An inspector beside the list.** Choosing a row shows its whole trigger, its text (wrapped,
  scrollable, with a copy button), its folder, **Edit** and a `…` menu to move or delete it. On
  a narrow window it takes the place of the list, with **Back**.
- **Choosing several expansions** is an explicit mode, entered from the `…` menu, or with Ctrl-
  or Shift-click on a row. Changing folder or search clears the selection and says so.
- **Keyboard shortcuts.** Ctrl+N new, Ctrl+L or Ctrl+F search, arrows and Enter in the list, F2
  folder options, Del delete (always asked), Ctrl+S save, Esc backs out one layer at a time.
  None of them act while text is being typed. They are shown in tooltips, menus and the guide.
- **The editor asks before throwing away unsaved changes**, and only when there is something to
  lose.
- **EspansoManager has its own icon**, in Explorer, on the window and in the notification area.
  The executable used to have none, and the tray borrowed Espanso's. The tray icon follows the light or dark colour of the taskbar,
  and paused is a different shape, not only a different colour.
- **Contrast themes are followed.** With a Windows contrast theme on, the app is drawn in the
  colours chosen for it in Windows' settings.
- **Emoji are drawn with the font Windows itself uses** for them, instead of the small one
  bundled with the interface toolkit. `👍` and `💡` used to be an illegible tangle and a bare
  ring; now they read as what they are. They remain single-colour: the toolkit this app draws
  with paints every glyph in one flat colour, so colour emoji are not possible, and the Windows
  colour emoji font would have added 12 MB to the download without changing a single pixel.
  The app's own small icons are deliberately not drawn from it, so no button changed shape.

### Changed

- **A new, quieter interface.** One library with nothing permanently docked beside it:
  - A centred title, with the real state of Espanso on its left — a dot and **Active**,
    **Paused** or **Not responding**, which is what Espanso answered, not what the app assumes —
    and three borderless commands on its right: pause/resume, new, and a `…` menu.
  - Under it, a short centred search and an **All folders** menu that picks which folder is
    shown.
  - Rows are the trigger in a monospaced face and a one-line preview, split by faint lines.
  - The editor, the folder page, the guide and the welcome screen have no cards, numbers or
    subtitles: headings and space do the grouping. The editor's **Save / Cancel** is always on
    screen, and a text draft survives switching to a date and back.
  - Saving and coming back keeps the folder, the search and the scroll position. The saved row
    scrolls into view and is briefly tinted.
- **Settings is a dialog.** It opens in the middle of the window over the dimmed list and cannot
  be moved; Esc, the cross or a click outside it closes it. It names its five settings and
  nothing else: the explanations moved to the guide, under a new **Settings** topic, and so did
  the credit to Espanso's author. What export and import report is shown inside the dialog.
  Turning start-with-Windows on or off no longer posts a banner (the switch already says so); a
  failure still does. On a short window the dialog scrolls under a fixed title.
- **The colours are Windows 11's own**, in light and in dark, and the accent is the one chosen in
  Windows' settings, in the shade Windows itself uses for that theme.
- **The title bar uses Mica**, the Windows 11 material that takes a faint tint from the desktop
  wallpaper. Windows only draws it while the window is active, and a solid-colour desktop gives it
  nothing to tint with. Versions of Windows older than 11 ignore it.
- **Fluid hover.** One soft highlight glides from row to row under the pointer instead of each
  row lighting up and going dark on its own. The same glide runs through the command bar, every
  menu, the switches in Settings and the topics of the guide. Outlined buttons keep their own
  hover, because their fill is part of their shape.
- **Windows' animation effects switch applies to the whole app, live.** With it off, nothing
  travels, slides or fades, and the change takes effect without restarting. The mouse wheel
  keeps its short smoothing: Windows keeps that under a separate setting of its own.
- **Every dialog is the same dialog.** Move, new folder, delete confirmations and the export and
  import folder pickers share one frame, one margin and one button order: action first, then
  Cancel, as Windows does it. Their width shrinks with the window. The pickers are a list of
  folders with a check; a new folder is made with **Create**, not **Save**.
- **The warning shown before deleting expansions is drawn inside the app.** It used to be a
  system dialog, which meant it always looked light even in dark theme and matched nothing else
  on screen. It now follows the theme, lists the expansions **in the same order as the list
  behind it**, folds a long selection into a "Show N more" line instead of running off the edge
  of the window, and hands keyboard focus back where it came from when it closes. There is no
  warning triangle beside the heading, on purpose: Windows 11 does not use one in its own
  dialogs, and the red sentence and the red button already say what kind of question this is.
  Its **Delete** button is legible in light theme, and text being typed into a form survives
  cancelling it.
- **Keyboard focus is visible.** Tab draws a ring around the focused control in both themes.
  Before, only text boxes showed where the keyboard was.
- **Counts agree with their nouns** in all four languages ("1 expansion", not "1 expansions").
- The smallest window is now 620×540 (it was 620×480).
- The start-up switch reads **Start EspansoManager with Windows**, without "(and Espanso)".

### Fixed

- **Emptying a folder no longer deletes it.** A folder that existed only because of what was in
  it vanished when the last expansion was moved out. Folders like this came from an import or
  from typing a new name in the editor. It now stays, empty, like one made with **New folder**.
- **Import no longer counts duplicates as additions.** The folder picker offered "2 will be
  added" when one of them already existed. It now counts only what will really be added. It
  hides folders that would add nothing, and reports what was skipped.
- **A failed restart no longer hides the import result.** The Espanso error used to replace
  "Added: N". Now the result comes first and the error below it.
- **Saving to a read-only `base.yml` keeps what you typed.** The change used to stay in memory
  though not on disk, and the draft was lost. Now the editor stays open with the draft intact,
  and the import is undone in the same way.
- **Hindi buttons were empty boxes.** The Devanagari face was missing from the font family that
  button labels use. Pause, New expansion, Settings and the rest were illegible.
- **Text ran off the right edge** of the welcome screen and the quick guide in narrow windows.
- **"Back" and the other link-style buttons that start a line** have their first letter on the
  page's left edge, like every other line. Their invisible padding put them 14 px further in.
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
