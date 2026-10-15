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

`{command: "app.font.add", params: {name: "Noto Sans KR", base64: "…", index: 0}}` adds a font
(a TrueType or OpenType file, not WOFF or WOFF2; `index`, default 0, picks the face in a
collection) to the end of every font family, after the system's fallback faces and any font added
before it (the first one added wins where two cover the same character), so text in a script the
other fonts lack draws with it. It returns `{name, added, covers: {hangul, han, kana}}`: `added` is
false when a font of that name was already added, which is then left as it was; `covers` is a hint
from a few sample characters of each script in the font's character map, not a check of every
character. A file egui can't read, one it can't draw from (no glyph outlines, as in a file cut off
in transfer after its character map, or no units per em), one over 64 MiB, or a font past 32
(256 MiB in all) is refused, with an error that says which; over the desktop channel the base64
text has to fit a 64 MiB line, so about 48 MiB of font is the practical limit there. Added fonts
stay, one copy each, until the app quits. The web build has no system fonts, so a page that
embeds it (`?host=parent`) sends its CJK fonts as `gridcraft:font`
messages (`{name, bytes, index?}`, bytes an `ArrayBuffer` or `Uint8Array`; see
`apps/gridcraft-web/src/host.rs`), each answered with `gridcraft:fontResult`, in this order:
`gridcraft:ready` (from GridCraft, with `"font"` in `capabilities`), then `gridcraft:font`, then
`gridcraft:open`, so the workbook's first frame already has the glyphs.

The web build saves no preferences: each visit starts in the browser's language, or in the one
the page's URL asks for with `lang` (`index.html?host=parent&lang=ko`; any language code
`app.language.set` takes, an unknown one is ignored). A Chinese, Japanese or Korean interface
needs fonts the build doesn't have, so it shows English until the fonts added draw every East
Asian character of that interface's labels (checked against their character maps, not just
`covers`: a Korean font with Hanja and kana still lacks most of the Simplified Chinese interface;
a refused font, such as one cut off in transfer, counts for nothing), then switches in the frame
after the `gridcraft:fontResult`. A language the user picks in the View tab or File › Options is
kept: fonts that arrive later don't change it.

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

## Web

The browser build has no port to listen on. Opened in an `<iframe>` with `?host=parent`, it takes
the same methods from the page that embeds it, as `postMessage` messages. Its
`gridcraft:ready` message (see `apps/gridcraft-web/src/host.rs` for the other messages) lists
`"control"` in `capabilities`; send requests after it, to the frame's window with your own origin
as the target origin:

```json
{"type": "gridcraft:control", "id": 1, "method": "engine.execute", "params": {"command": "cell.set", "params": {"cell": "B2", "input": "=SUM(A1:A9)"}}}
```

GridCraft runs the requests at its next frame, in the order they arrived, together with the
workbooks sent as `gridcraft:open`: a request sent after a workbook works on that workbook. Each
request is answered in that frame, on the parent window:

```json
{"type": "gridcraft:reply", "id": 1, "ok": true, "result": {"cell": "B2", "value": "45"}}
{"type": "gridcraft:reply", "id": 1, "ok": false, "error": "…"}
```

- `id` (a number or a string) is required and comes back in the reply; match replies by it.
  `params` is an object and may be left out. A message GridCraft can't read (no `id`, `method`
  not a string, `params` not an object, not representable as JSON) is answered with an error
  under its `id`, or `id: null` when it has no usable one.
- A control message holds at most 16 MiB of JSON text (UTF-8 bytes; one over 16 Mi UTF-16 code
  units is refused before GridCraft reads it). Binary values (an `ArrayBuffer` or typed array)
  are refused: send bytes as base64 strings. Send a workbook as `gridcraft:open` rather than
  `app.open {base64}`: its bytes don't count against the limit.
- While the page is visible, the next frame is the next display refresh. While it is hidden,
  browsers wake it with timers instead: about once a second, and in Chrome as rarely as once a
  minute after five minutes hidden, so answers take that long.
- Input methods (`ui.click`, `ui.key`, `ui.text`, `ui.drag`…) answer `ok` at once; their effect
  lands over the following frames (a drag takes several), so read the outcome with a later
  request.
- Only the parent window is heard, and only when its messages come from the origin of
  GridCraft's own URL (`location.origin`); replies are posted to that origin only. When that
  URL has no origin (`location.origin` is `"null"`: a scheme the browser gives no origin, a
  `data:` URL), the parent can't be checked, so host mode stays off and GridCraft posts
  `gridcraft:failed` saying why (a fixed text, posted to any origin). A custom scheme the web
  view gives an origin works (WKWebView gives `tauri://localhost` the origin `tauri://localhost`).
- In a frame sandboxed without `allow-same-origin`, GridCraft's document has an opaque origin
  while `location.origin` stays its URL's: the parent's messages only reach it when posted with
  the target origin `"*"`, and its messages reach the parent with `event.origin` `"null"`, so
  the parent has to recognise them by `event.source`.
- The channel gives the parent nothing it doesn't have: a same-origin parent can already reach
  into the frame (`iframe.contentWindow`) and run any code there. Other origins are never heard.
- There is no file system. `app.open` takes the workbook's bytes (`{name?, base64, password?}`,
  with `recalculate`, `readOnly` and `lockReadOnly` to open it as saved or for reading only);
  `file.saveBytes {format?}` returns `{name, base64, bytes}`; `app.save` hands the workbook over
  as `gridcraft:save`, as the Save button does, and from that moment it counts as saved (an edit
  after it makes it unsaved again); `file.exportPdf` without a `path` returns
  `{pages, bytes, base64}`: a requested PDF is in the reply, not also handed over as
  `gridcraft:save` (as it is when the user exports from a menu).
- `ui.screenshot` isn't available (it is answered with an error); `ui.resize`, `ui.focus` and
  `app.quit` do nothing in a browser tab.
