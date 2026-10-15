# Localization parity

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-15 · **Change:** minor (a host adds fonts at runtime: CJK on the web build; Korean catalog: dialogs, menus and tooltips; the web build shows a CJK interface once added fonts draw it; an IME composition typed into a selected cell enters it) · **Target:** Microsoft Excel (Microsoft 365), Excel for Mac 16.113.4

Per-language status against Excel, which ships its UI in dozens of languages (Windows language
packs cover 100+), with localized function names, argument separators, number and date formats,
right-to-left sheets and full complex-script support. Part of
[target-app-parity.md](target-app-parity.md). **Localization: ~10% ready, 100–165 h** (estimated).

## Where we are (measured from source on `main`, after the 2026-10-10 merges)

- **A first catalog exists, for the ribbon only.** `crates/ui-egui/src/i18n.rs` (merged 10-10 in
  #31, #104, #82; German added 10-11) adds an interface-language preference that defaults to the
  system language (closes #20) and a View-tab switcher, with English-keyed lookup tables:
  Japanese 268 labels, Simplified Chinese, Korean, Russian, Brazilian Portuguese, German and Czech 266
  each. The tables are used by
  the ribbon, sheet tabs and widgets (`ribbon.rs`, `tabs.rs`, `widgets.rs`), the Format Cells
  dialog, every dialog title and the shared OK/Cancel buttons; the other dialogs, menus,
  context menus, task panes, status messages, command labels and function descriptions are still
  English literals. Untranslated labels fall back to English.
- **About 2,200 user-visible strings** in all (heuristic source scan: ~1,700 UI and command
  strings plus ~500 function descriptions), so each shipped language covers **~12%** (measured
  count / estimated total).
- **en-US only formulas and formats.** Function names, `,` argument separator, `.` decimal
  separator, en-US number and date formats (`crates/numfmt` says "en-US only"). Users ask for
  localized function names and `;` separators (#45). German with localized numbers and formulas
  is in PR #163 (open).
- **Script support.** System-font fallback for CJK, Thai and Hebrew landed in #38 (closes #32;
  #142 is still open pending confirmation). The web build has no system fonts; a host adds fonts
  while the app runs (`app.font.add`, or `gridcraft:font` from a page embedding the web build),
  and every family falls back on them, so CJK text can render there once the page sends a face.
  The web build follows the browser's language, or the page's `?lang=`, into Chinese, Japanese or
  Korean once the added fonts draw every East Asian character of its labels, and shows English
  until then; a language picked in the View tab is kept. There is no complex-text shaping: Arabic
  and Devanagari don't join or reorder, and there is no bidirectional layout. **Sheet Right-to-Left**
  (`view.rightToLeft`, Page Layout › Sheet Options, XLSX `rightToLeft`) mirrors the grid (#146):
  column A and the row headers on the right, the vertical scrollbar on the left; hit-testing,
  arrow keys, editor, popups and floating objects follow; text stays left-to-right inside its box
  (no bidi), and the sheet tabs, horizontal scrollbar and printing/PDF are not mirrored yet.
- **IME input.** Typing into a selected cell can start with an input-method composition: the
  grid asks for the IME at the active cell, and the cell editor takes the composition over until
  it ends. The composition used to be lost: in the web build "가나다" entered nothing and "한글"
  usually "한" or nothing, depending on whether the cell editor took the focus between
  syllables. Verified in the web build in headless Chromium with simulated compositions (CDP
  `Input.imeSetComposition`): Korean "한글" and "가나다" and a Japanese conversion to "日本" land
  in the cell. Real OS input methods (macOS, Windows, Linux) are unverified. A click elsewhere
  mid-composition enters the syllable once: Windows delivers it again as a late commit (winit
  commits the composition when the IME is turned off), which is dropped.
  Desktop side effects of asking for the IME while only a cell is selected, read from the winit and
  egui-winit sources (not measured): on macOS winit keeps the Enter or Tab that ends a
  composition, so entering the cell takes a second press, also after a single syllable; on
  Windows, keys the IME handles arrive as `Process` and egui-winit drops them, so Space,
  Shift+Space (select the row), Ctrl+Space (select the column) and Space on a checkbox cell can
  go to the IME instead, as in Excel while an IME is on; on Linux, IBus and fcitx take Ctrl+Space
  the same way, and on Wayland the empty pre-edit text winit reports whenever the IME is turned on
  or off may keep an idle window repainting. The grid doesn't ask for the IME once a touch screen
  has been used in the session (egui keeps reporting one: on a touch laptop one touch scroll is
  enough, and iPadOS, which presents itself as a Mac, from its first tap), or on Android/iOS, so a
  tap on a cell doesn't bring up the on-screen keyboard; there, compose after F2, or on a tablet or
  phone double-tap the cell or use the formula bar.
- **PDF export** uses only the 14 standard PDF fonts: non-Latin text cannot export.

## Languages

UI strings translated are measured counts against the ~2,200 estimate. Estimates to `full`
include the rest of the catalog work below (10–20 h, counted once), localized function names,
separators, number/date formats, machine translation with agent review, and script work.

| Language | Code | UI strings translated | Dialogs / tooltips / help | Script support | Native review | Status | Estimate to full |
|---|---|---|---|---|---|---|---|
| English | en | ~2,200 / ~2,200 (100%, source) | yes | Latin: yes | yes | full | — |
| Simplified Chinese (Mandarin) | zh-Hans | 266 (~12%) | no | CJK system-font fallback (#38); IME into a selected cell: tests only (native IMEs unverified); PDF can't export | no | menus only | 8–13 h |
| Spanish | es | 0 (0%) | no | Latin: yes | no | none | 6–10 h |
| Hindi | hi | 0 (0%) | no | Devanagari: **no shaping** | no | none | 12–18 h |
| Arabic | ar | 0 (0%) | no | **no shaping, no RTL UI**; Sheet Right-to-Left mirrors the grid (#146) | no | none | 25–35 h |
| French | fr | 0 (0%) | no | Latin: yes | no | none | 6–10 h |
| Portuguese | pt-BR | 923 (~42%) | dialogs and tooltips (dialogs take effect with #669) | Latin: yes | no | menus, File view, dialogs | 2–4 h |
| Indonesian | id | 0 (0%) | no | Latin: yes | no | none | 5–8 h |
| Japanese | ja | 268 (~12%) | no | CJK system-font fallback; IME into a selected cell verified in the web build with a simulated conversion (native IMEs unverified); no furigana display | no | menus only | 8–12 h |
| German | de | 266 (~12%) | no | Latin: yes | no | menus only | 4–8 h |
| Korean | ko | 973 (~44%) | dialogs and tooltips (dialog bodies take effect as they look up the catalog) | Hangul via the CJK fallback; IME into a selected cell verified in the web build with simulated compositions (native IMEs unverified) | no | menus, File view, dialogs | 3–6 h |
| Vietnamese | vi | 0 (0%) | no | Latin with stacked diacritics: unverified | no | none | 6–10 h |

Other shipped languages: Russian (ru) and German (de), 266 labels each (~12%), menus only.

**Total:** catalog work 10–20 h + languages 92–144 h ≈ **100–165 h**. Parallelizes almost fully
once the catalog covers dialogs and commands. **Needs a human:** native-speaker review of every
language (none so far) and of localized function names.

## Infrastructure (do first)

1. Extend the catalog from the ribbon to every user-visible string (dialogs, menus, task panes,
   status and error messages, command labels, function descriptions), with a missing-key check
   in `cargo xtask ci`. Language switching and the system-language default already exist (#31).
2. Locale-aware formulas: localized function-name tables and separators at the formula-bar
   boundary only (files keep en-US invariant names), locale number and date formats, list
   separators in CSV.
3. Text shaping and fallback: a shaping engine for grid, editor and PDF text (Arabic, Devanagari,
   Thai), system font fallback for CJK, bidirectional runs, right-to-left sheet tabs and printing;
   embedded subset fonts in PDF.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-15 | trivial | Web build CJK interface, after review: tests (on any machine) that a font cut off in transfer leaves it in English and that `covers` from sample characters is not the label check; the Chinese catalog is held to the Japanese keys the check lists and the Korean one to Hangul (no Han or kana), so the Korean catalog can grow; wording says every East Asian character |
| 2026-10-15 | minor | Web build: a Chinese, Japanese or Korean browser language, or the page's `?lang=`, is used once the fonts the page added draw every East Asian character of that interface's labels (English until then); a language picked by hand is kept |
| 2026-10-15 | trivial | A font a host adds that egui can't draw from is refused: no glyph outlines (a file cut off in transfer after its character map would take characters and draw them blank) or no units per em (infinite glyph scale) |
| 2026-10-15 | minor | A host adds fonts while the app runs (`app.font.add`, the web host's `gridcraft:font`); every family falls back on them, so the web build can draw CJK text |
| 2026-10-15 | minor | Korean catalog (`i18n/ko.rs`) grows from 398 to 973 entries (~44%): every key the other catalogs translate, plus three that are looked up through `Language::tr` but were in no catalog (Format Cells' No Color and protection note, the sheet navigator's All sheets); `Size` now reads 크기, so the Format Cells font-size slider no longer says 용지 크기 (paper size). About 230 of the new entries show today (ribbon tooltips and menus, dialog titles and OK/Cancel, Format Cells, the PivotTable Fields pane, cell and header context menus); the rest show once their dialogs, panes and messages look up the catalog |
| 2026-10-15 | minor | Typing into a selected cell through an IME: the grid asks for the IME at the active cell and the cell editor takes the composition over until it ends. Windows' late commit of a syllable a click already entered is dropped; desktop side effects (macOS second Enter, Windows/Linux IME keys such as Space and Ctrl+Space) listed above. Verified in the web build in headless Chromium with simulated Korean and Japanese compositions; native OS IMEs unverified; off with a touch screen |
| 2026-10-12 | minor | Format Cells looks up its tabs, labels, options and note through `Language::tr` (Fill tab keyed `Format Cells\|Fill`); dialog titles and OK/Cancel too. Labels already in a table (Number, Currency, Fill…) show translated; the rest fall back to English until the tables add them |
| 2026-10-12 | minor | Brazilian Portuguese catalog (`i18n/pt.rs`) grows from 377 to 923 entries: every key the other catalogs translate, including the dialog strings #669 starts looking up (~42%) |
| 2026-10-11 | minor | German ribbon catalog (`i18n/de.rs`, 266 labels, ~12%); `de` locale tags (de-DE, de-AT, de-CH…) resolve to German. PR #163 (localized German formulas/numbers) still open |
| 2026-10-11 | minor | Sheet Right-to-Left mirrors the grid (#146) |
| 2026-10-11 | minor | Czech (cs) ribbon table: 266 labels (~12%), menus only |
| 2026-10-10 | minor | Re-measured after #31, #104, #82, #38 merged: ribbon translated into ja, zh, ko, pt-BR and ru (266–268 labels, ~12%), system-language default, CJK/Thai/Hebrew font fallback; ~10% ready, 100–165 h |
| 2026-10-10 | major | Created: English only, no catalog, ~2,200 strings, script-support gaps, open language PRs |
