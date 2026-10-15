# GridCraft control protocol

Start the desktop app with a control port:

```sh
gridcraft --control 7979            # or GRIDCRAFT_CONTROL_PORT=7979
```

Then send newline-delimited JSON to `127.0.0.1:7979`. Every request line gets one reply line:

```json
{"id": 1, "method": "engine.execute", "params": {"command": "cell.set", "params": {"cell": "B2", "input": "=SUM(A1:A9)"}}}
{"id": 1, "ok": true, "result": {"cell": "B2", "value": "45"}}
```

Errors come back as `{"id": …, "ok": false, "error": "…"}`.

## Methods

| Method | Params | What it does |
|---|---|---|
| `engine.execute` | `{command, params}` | Runs any engine command (see `engine.commands` or `gridcraft-cli commands`) |
| `engine.commands` | `{search?}` | Every command: id, label, ribbon path, shortcut, params doc, enabled; `search` keeps those whose id, label or ribbon path contains it (case-insensitive) |
| `engine.journal` | | Commands run so far (replayable). Passwords (`password`, `…Password` params) are left out, so a replayed Protect Sheet has no password |
| `document.inspect` | | Workbook summary: sheets, used ranges, tables, charts, names, selection, undo labels, whether it is open read-only (`readOnly`, `lockReadOnly`) |
| `sheet.read` | `{range?, sheet?, formulas?, formatted?}` | Values (or formulas / displayed text) of a range |
| `cell.get` | `{cell?}` | One cell: value, formula, displayed text, style, merge, comment, link |
| `ui.inspect` | | UI state: ribbon tab, editor, open dialog, Backstage page, message, grid rect, scroll, perf |
| `ui.click` / `ui.drag` / `ui.move` | `{x, y, toX?, toY?, button?, count?, shift?, cmd?, alt?}` | Real pointer input in window points |
| `ui.cell.click` / `ui.cell.drag` | `{cell}` / `{from, to}` | Pointer input at a cell's centre (works in point mode while editing) |
| `ui.key` / `ui.text` | `{key, shift?, cmd?, alt?, ctrl?}` / `{text}` | Keyboard input (typing in the grid starts editing) |
| `ui.edit.begin` / `ui.edit.text` / `ui.edit.commit` / `ui.edit.cancel` | `{text?}` / `{move?: down\|right\|up\|left\|none}` | Drive the cell editor (`ui.edit.begin` on a read-only workbook opens none and answers an error) |
| `ui.ribbon` | `{tab}` | Switch ribbon tab |
| `ui.dialog` / `ui.dialog.set` / `ui.dialog.confirm` / `ui.dialog.cancel` | `{name}` / `{field, value}` | Open, fill in and confirm dialogs |
| `ui.backstage` (via `engine.execute`) | `{page?: home\|new\|open\|info\|saveAs\|print\|export\|options\|close}` | Open the File tab's Backstage view on a page, or close it (`ui.inspect` reports the open page as `backstage`) |
| `ui.screenshot` | `{path?}` | PNG of the window (base64 when no path). Animations are settled first: the picture shows the final state. A path that is a read-only workbook's file is refused |
| `ui.set` | `{dark?, formulaBar?, ribbonCollapsed?, animations?}` | UI preferences (`formulaBar` runs `view.formulaBar`) |
| `ui.resize` / `ui.focus` | `{width, height}` | Window control |
| `app.open` / `app.save` / `app.quit` | `{path}` or `{name?, base64}`, with `password?`, `recalculate?`, `readOnly?`, `lockReadOnly?` / `{path?}` | Files and lifetime. `app.open` opens a file, or a workbook's bytes (base64; `name`, default `Book.xlsx`, gives its title and format), passing `password`, `recalculate`, `readOnly` and `lockReadOnly` to `file.open` as given, and answers `{title, readOnly, lockReadOnly, recalculated}` (for a file already open, `alreadyOpen` and what was ignored instead of `recalculated`); a value `file.open` refuses is an error and opens nothing. `password` opens a password-protected workbook; without it, such a workbook returns an error (never a dialog) |

Any other method name that is a command id (e.g. `home.bold`) runs that command.

Use `engine.execute` with `{command: "view.theme", params: {mode: "system"}}`
to choose the display theme; `mode` accepts `"system"`, `"light"`, or `"dark"`.
System follows live OS appearance changes and falls back to Light when the platform supplies no preference.
The saved choice remains System; new users still start in Light. The legacy
`view.darkMode` command and `ui.set {dark}` select a fixed Light or Dark theme.

`{command: "view.animations", params: {on: false}}` turns motion in the UI off (the selection
outline then jumps to a new selection instead of gliding there); without `on` it toggles. It is
the View ▸ Animations checkbox, saved with the other UI preferences and on by default.

`{command: "app.language.set", params: {language: "ja"}}` switches the interface language
(`code` is accepted as an alias for `language`); `app.language.english` and
`app.language.japanese` are shortcuts. Only interface chrome is translated, never command ids or
document text. The choice is saved in `ui.json`; with nothing saved the desktop app follows the
system language.

`{command: "file.open", params: {path: "Secret.xlsx", password: "…"}}` opens a password-protected
workbook (Agile or Standard encryption). Without `password` it returns an error containing
"is password-protected. Enter its password to open it." and never opens a dialog; a wrong
password returns "The password is incorrect." The password is left out of the journal and never
quoted in an error. The opened workbook has no save path, so Save asks where to save it, without
the password, and never replaces the encrypted original.

`{command: "file.open", params: {path: "Report.xlsx", recalculate: false}}` opens a workbook with
the results stored in it instead of calculating its formulas again, so a `TODAY()` keeps the date
it was saved with. A formula stored without a result is blank until it is calculated. Without
`recalculate`, formulas are calculated again unless the workbook was saved in manual calculation
mode and doesn't ask to be calculated on load; an `.xlsx` with `fullCalcOnLoad`, an `.xls` saved
uncalculated (`Uncalced`), and a workbook whose cached array results GridCraft couldn't keep (a
range over 1 million cells, or past 4 million in all) do ask. `recalculate: true` always
calculates. The result's `recalculated` says which happened. A file that is already open, however
its path is spelled, is switched to as it is: the result has `alreadyOpen`, and
`recalculateIgnored` when `recalculate` was given.

The stored results last until something calculates. In automatic mode an edit recalculates the
formulas that depend on it and, as after every edit, the formulas GridCraft always recalculates and
what depends on them: volatile functions (`TODAY()`, `NOW()`, `RAND()`…), `INDIRECT`, `OFFSET`, and
formulas that use names (defined names, `LET` and `LAMBDA` variables), table references or
functions it doesn't know. So the first edit can change those from their stored results, even when
it is unrelated to them. In manual calculation mode an edit calculates the formulas it enters
(entering an array formula can recalculate more). Edits that change the sheet's structure, such as
inserting or deleting rows, columns, cells or sheets, sorting, filtering, grouping, hiding rows or
columns or changing row heights, recalculate the whole workbook in either mode.
`formulas.calculateNow` (F9) calculates everything.

`{command: "file.open", params: {path: "Approval.xlsx", readOnly: true}}` opens a workbook for
reading only. Commands that would change it are refused before they do anything or open a dialog,
with "command `…` is not available right now: the workbook is open read-only", and
`engine.commands` lists them as disabled with that reason: every command that records an undo step,
except those that only change the view (zoom, frozen panes, gridlines, headings, zeros,
right-to-left, Show Formulas) or follow a hyperlink, and also undo and redo, Cut, Edit Cell (F2),
Calculate Now and Calculate Sheet (the workbook keeps showing the results it opened with), Record
Actions, and saving or deleting a custom view. Selecting, copying, finding, reading cells, the
Watch Window, print preview, printing and exporting a PDF work, and what a read-only workbook
allows is neither an undo step nor an unsaved change. Nothing it allows calculates, but opening
does, by default: to show the workbook exactly as it was saved, pass `recalculate: false` too
(otherwise a `TODAY()` shows the day it is opened).

Its file is never written: Save, Save As and exports to that file, however its path is spelled,
are refused, from it or from any other workbook, and so are a save whose temporary file
(`<path>.sctmp`) would be that file and a `ui.screenshot` to it. Save As another name (or Save with
another `path`) and `file.saveBytes` write a copy. The copy doesn't become the workbook's file, as
it does for a workbook open for editing: the workbook stays read-only, on its own file, and the
copy opens for editing like any other file.

`file.enableEditing` turns read-only off in every window on the workbook (`view.newWindow` on a
read-only workbook opens a read-only window) and answers `{readOnly: false, windows}`. It isn't an
undo step. It is journaled, so a journal or recorded script that enabled editing replays: on a
workbook that isn't read-only it does nothing and answers `{readOnly: false, alreadyEditable:
true}`.

`lockReadOnly: true` opens the workbook read-only for good, for a page that must only show it:
`file.enableEditing` is refused in every window on it ("the workbook was opened read-only and
can't be switched to editing", also its `disabled_reason` in `engine.commands`). It implies
`readOnly: true` and can't go with `readOnly: false`. Opening the file again switches to the
workbook, still locked, so the file opens for editing only once the workbook is closed. (The lock
keeps this workbook and its file from changing; a copy, `file.saveBytes` or copied cells give
editable data like any other.)

The result's `readOnly` and `lockReadOnly` say how the workbook opened, and `document.inspect`
reports them. A `readOnly` or `lockReadOnly` that isn't true or false is refused before the file is
read, and so are `recalculate`, `readOnly` and `lockReadOnly` without `path` or `base64`: no dialog
asks for the file, and one picked there would open without them (`file.open {}`, as File › Open,
still asks). A file that is already open, however its path is spelled, is switched to without
being read again, and its read-only state only tightens: `readOnly` or `lockReadOnly` is refused
while it is open for editing, `lockReadOnly` locks it while it is read-only (in every window on
it), and `readOnlyIgnored` in the result says it stays more read-only than asked (`readOnly:
false`, `lockReadOnly: false`).

In the window, a read-only workbook shows an info bar (with Enable Editing unless `lockReadOnly`)
and "[Read-Only]" after its title. Typing, F2, the formula bar, headers and footers, text boxes and
sheet tab names open no editor (`ui.edit.begin` answers an error), the filter and drop-down list
menus stay shut, margins don't drag on the rulers, Save asks where to save a copy, and AutoSave
leaves it alone.

For a headless equivalent (no window), use `gridcraft-cli mcp` or `gridcraft-cli run`;
see [`mcp.md`](mcp.md) and [`cli.md`](cli.md).

### Native printing

Desktop `engine.commands {"search":"print"}` (and connected MCP `list_commands`)
lists `ui.printSettings` and `ui.printSubmit` with their parameter and result fields.

`engine.execute` can call `ui.printSettings` with `{}` to inspect the printer list, selected
printer, copies, duplex, grayscale, discovery/submission status and backend receipt/error.
`{"refresh":true}` starts background discovery; poll the same command until `discovering` is
false. Set `printer` to a discovered name, integer `copies` from 1 to 999, `duplex` to
`printerDefault`, `off`, `longEdge` or `shortEdge`, and boolean `grayscale`. Invalid updates are
rejected without changing earlier settings. `ui.printSubmit` explicitly submits the current
print-ready PDF without opening a dialog; inspect `ui.printSettings` until `busy` becomes false
and check `status`/`detail`. The browser keeps its own print hook. PDF export remains separate.
GridCraft accepts the existing `file.print` sheet/pagination params on `ui.printSubmit`; the engine `file.print` command returns the PDF without spooling.
