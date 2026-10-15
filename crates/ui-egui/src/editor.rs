//! In-cell / formula-bar editing state: point mode, reference colouring, function
//! autocomplete and argument hints.

use std::ops::Range;

use egui::Color32;
use gridcraft_engine::core::{CellRef, RangeRef};

/// Colours for references in a formula being edited (our own palette).
pub const REF_COLORS: [Color32; 8] = [
    Color32::from_rgb(0x2E, 0x6F, 0xD8),
    Color32::from_rgb(0xD0, 0x3C, 0x3C),
    Color32::from_rgb(0x7A, 0x4F, 0xC9),
    Color32::from_rgb(0x2B, 0x9A, 0x4A),
    Color32::from_rgb(0xC2, 0x5E, 0x1E),
    Color32::from_rgb(0x1D, 0x92, 0xA6),
    Color32::from_rgb(0xB8, 0x3B, 0x8E),
    Color32::from_rgb(0x8A, 0x7A, 0x1F),
];

#[derive(Clone, Debug)]
pub struct EditState {
    pub sheet: usize,
    /// Syntax of this edit, captured when it starts (prevents a language switch reinterpreting it).
    pub locale: gridcraft_engine::formula::FormulaLocale,
    pub cell: CellRef,
    pub text: String,
    /// Started by typing (Enter mode: arrows commit) rather than F2 (Edit mode: arrows move the caret).
    pub enter_mode: bool,
    pub from_formula_bar: bool,
    /// Caret position (char index), mirrored from the text widget.
    pub caret: usize,
    /// The reference most recently inserted by point mode (byte range) — arrows/clicks replace it.
    pub point: Option<(usize, usize)>,
    /// Where point mode's keyboard cursor is.
    pub point_cell: Option<CellRef>,
    pub autocomplete: Vec<String>,
    pub ac_index: usize,
    pub request_focus: bool,
    /// The text changed programmatically: push `caret` into the widget on the next frame.
    pub sync_caret: bool,
    /// Column AutoComplete: the full entry the typed text completes to.
    pub completion: Option<String>,
    /// Move the caret to the end on the next frame.
    pub caret_to_end: bool,
    /// The edit began with an input-method composition while only the cell was selected, and
    /// this widget (the grid) keeps the keyboard until the composition ends: focusing the text
    /// field would interrupt it. Meanwhile the editor applies the IME events itself
    /// (`formula_bar::editor_widget`), then hands the keyboard to the text field.
    pub ime_routing: Option<egui::Id>,
    /// The text being composed (the IME's pre-edit text) in `text`, as a char range, while the
    /// editor applies the IME events itself.
    pub preedit: Option<Range<usize>>,
}

/// An IME event with text to compose or commit: not the empty ones some integrations repeat while
/// nothing is composed (Wayland on every text-input `done`, so also whenever the IME is turned on
/// or off) or send to end a composition, nor a newline (Enter's job).
pub fn composes_text(event: &egui::Event) -> bool {
    matches!(event, egui::Event::Ime(egui::ImeEvent::Preedit { text, .. } | egui::ImeEvent::Commit(text)) if !text.is_empty() && text != "\n" && text != "\r")
}

impl EditState {
    pub fn new(sheet: usize, cell: CellRef, text: String, enter_mode: bool, from_formula_bar: bool) -> EditState {
        let caret = text.chars().count();
        EditState {
            sheet,
            locale: Default::default(),
            cell,
            text,
            enter_mode,
            from_formula_bar,
            caret,
            point: None,
            point_cell: None,
            autocomplete: vec![],
            ac_index: 0,
            request_focus: true,
            sync_caret: true,
            caret_to_end: true,
            completion: None,
            ime_routing: None,
            preedit: None,
        }
    }

    /// Applies an IME event the way egui's `TextEdit` does: pre-edit text replaces the previous
    /// pre-edit text at the caret, a commit replaces it for good. Empty events while nothing is
    /// being composed (some integrations repeat them) and newlines (Enter's job) change nothing.
    /// Returns whether the text changed.
    pub fn apply_ime(&mut self, event: &egui::ImeEvent) -> bool {
        let at = self.preedit.clone().unwrap_or(self.caret..self.caret);
        match event {
            egui::ImeEvent::Preedit { text, .. } | egui::ImeEvent::Commit(text)
                if (text.is_empty() && self.preedit.is_none()) || text == "\n" || text == "\r" =>
            {
                false
            }
            egui::ImeEvent::Preedit { text, .. } => {
                let composed = self.splice(at, text);
                self.caret = composed.end;
                self.preedit = (!composed.is_empty()).then_some(composed);
                true
            }
            egui::ImeEvent::Commit(text) => {
                self.caret = self.splice(at, text).end;
                self.preedit = None;
                true
            }
            egui::ImeEvent::DeleteSurrounding { before_chars, after_chars } => {
                let len = self.text.chars().count();
                let (start, end) = (at.start.min(len), at.end.min(len));
                let (before, after) = (start.saturating_sub(*before_chars), end.saturating_add(*after_chars).min(len));
                if (before, after) == (start, end) {
                    return false;
                }
                self.splice(end..after, "");
                self.splice(before..start, "");
                let shift = start - before;
                self.preedit = self.preedit.as_ref().map(|_| before..end - shift);
                self.caret = self.caret.min(len).saturating_sub(shift);
                true
            }
            _ => false,
        }
    }

    /// Typed text (not from the IME) at the caret: it replaces the pre-edit text, as `TextEdit`
    /// replaces its selection. Returns whether the text changed.
    pub fn insert_typed(&mut self, typed: &str) -> bool {
        if typed.is_empty() || typed == "\n" || typed == "\r" {
            return false;
        }
        let at = self.preedit.take().unwrap_or(self.caret..self.caret);
        self.caret = self.splice(at, typed).end;
        true
    }

    /// Drops the text being composed (the composition was cancelled). Returns whether there was any.
    pub fn cancel_preedit(&mut self) -> bool {
        let Some(at) = self.preedit.take() else { return false };
        self.caret = self.splice(at, "").start;
        true
    }

    /// Byte offset of char `index`, or the end of the text.
    fn byte_at(&self, index: usize) -> usize {
        self.text.char_indices().nth(index).map(|(b, _)| b).unwrap_or(self.text.len())
    }

    /// Replaces the chars in `range` (clamped to the text) with `with`; the char range it now takes.
    fn splice(&mut self, range: Range<usize>, with: &str) -> Range<usize> {
        let (start, end) = (self.byte_at(range.start), self.byte_at(range.end.max(range.start)));
        let (Some(a), Some(b)) = (self.text.get(..start), self.text.get(end..)) else { return self.caret..self.caret };
        let at = a.chars().count();
        self.text = format!("{a}{with}{b}");
        at..at + with.chars().count()
    }

    pub fn is_formula(&self) -> bool {
        self.text.starts_with('=')
            || (self.text.starts_with('+') && self.text.len() > 1)
            || (self.text.starts_with('-') && self.text.len() > 1 && !self.text[1..].starts_with(|c: char| c.is_ascii_digit()))
    }

    fn byte_caret(&self) -> usize {
        self.text.char_indices().nth(self.caret).map(|(i, _)| i).unwrap_or(self.text.len())
    }

    /// Point mode: the caret follows an operator, `(`, `,` or `=` in a formula, so clicking a
    /// cell or pressing an arrow inserts a reference.
    pub fn can_point(&self) -> bool {
        if !self.is_formula() {
            return false;
        }
        if let Some((_, e)) = self.point
            && e == self.byte_caret()
        {
            return true;
        }
        let before = self.text.get(..self.byte_caret()).unwrap_or("");
        let last = before.trim_end().chars().last();
        matches!(last, Some('=' | '(' | ',' | '+' | '-' | '*' | '/' | '^' | '&' | '<' | '>' | ':' | ';' | '{'))
    }

    /// Inserts (or replaces the last pointed) reference text at the caret.
    pub fn insert_ref(&mut self, r: &str) {
        let caret_b = self.byte_caret();
        let (start, end) = match self.point {
            Some((s, e)) if e == caret_b => (s, e),
            _ => (caret_b, caret_b),
        };
        let (Some(a), Some(b)) = (self.text.get(..start), self.text.get(end..)) else { return };
        self.text = format!("{a}{r}{b}");
        let new_end = start + r.len();
        self.point = Some((start, new_end));
        self.caret = self.text.get(..new_end).map(|s| s.chars().count()).unwrap_or(0);
        self.caret_to_end = false;
        self.sync_caret = true;
    }

    /// Updates function autocomplete for the word before the caret.
    pub fn update_autocomplete(&mut self, names: &[String]) {
        self.autocomplete.clear();
        if !self.is_formula() {
            return;
        }
        let before = self.text.get(..self.byte_caret()).unwrap_or("");
        let word: String =
            before.chars().rev().take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '_').collect::<String>().chars().rev().collect();
        if word.is_empty() || word.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return;
        }
        let u = word.to_ascii_uppercase();
        // Not after a column letter of a reference like A1 (digits follow) – fine either way.
        self.autocomplete = names.iter().filter(|n| n.starts_with(&u)).take(12).cloned().collect();
        if self.autocomplete.len() == 1 && self.autocomplete[0] == u {
            self.autocomplete.clear();
        }
        self.ac_index = self.ac_index.min(self.autocomplete.len().saturating_sub(1));
    }

    /// Accepts the highlighted autocomplete entry: replaces the word and adds `(`.
    pub fn accept_autocomplete(&mut self) -> bool {
        let Some(name) = self.autocomplete.get(self.ac_index).cloned() else { return false };
        let caret_b = self.byte_caret();
        let before = self.text.get(..caret_b).unwrap_or("");
        let wlen: usize = before.chars().rev().take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '_').map(char::len_utf8).sum();
        let start = caret_b - wlen;
        let (Some(a), Some(b)) = (self.text.get(..start), self.text.get(caret_b..)) else { return false };
        self.text = format!("{a}{name}({b}");
        self.caret = self.text.get(..start + name.len() + 1).map(|s| s.chars().count()).unwrap_or(0);
        self.autocomplete.clear();
        self.caret_to_end = false;
        self.sync_caret = true;
        true
    }

    /// The function call the caret is inside and the argument index (for the hint tooltip).
    pub fn current_function(&self) -> Option<(String, usize)> {
        if !self.is_formula() {
            return None;
        }
        let before = self.text.get(..self.byte_caret())?;
        let mut depth = 0i32;
        let mut arg = 0usize;
        let chars: Vec<char> = before.chars().collect();
        let mut in_str = false;
        let mut i = chars.len();
        while i > 0 {
            i -= 1;
            let c = chars[i];
            if c == '"' {
                in_str = !in_str;
                continue;
            }
            if in_str {
                continue;
            }
            match c {
                ')' | '}' | ']' => depth += 1,
                '{' | '[' => depth -= 1,
                '(' => {
                    if depth == 0 {
                        let name: String = chars[..i]
                            .iter()
                            .rev()
                            .take_while(|c| c.is_ascii_alphanumeric() || **c == '.' || **c == '_')
                            .collect::<String>()
                            .chars()
                            .rev()
                            .collect();
                        if name.is_empty() {
                            return None;
                        }
                        return Some((name.to_ascii_uppercase(), arg));
                    }
                    depth -= 1;
                }
                c if c == self.locale.list_separator() && depth == 0 => arg += 1,
                _ => {}
            }
        }
        None
    }
}

/// References in formula text with their byte spans and colour index (same reference text ⇒
/// same colour), for colouring the editor and outlining ranges on the grid.
pub fn formula_refs(text: &str) -> Vec<(usize, usize, RangeRef, Option<String>, usize)> {
    let body = text.strip_prefix('=').or_else(|| text.strip_prefix('+')).unwrap_or(text);
    let offset = text.len() - body.len();
    let Ok(toks) = gridcraft_engine::formula::lexer::tokenize(body) else { return vec![] };
    let mut out: Vec<(usize, usize, RangeRef, Option<String>, usize)> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        use gridcraft_engine::formula::lexer::Tok;
        let (sheet, start, j) = match &toks[i].tok {
            Tok::Sheet(a, _) => (Some(a.clone()), toks[i].start, i + 1),
            _ => (None, toks[i].start, i),
        };
        if let Some(Tok::Word(w)) = toks.get(j).map(|t| &t.tok)
            && let Some(c) = CellRef::parse(w)
        {
            let mut end = toks[j].end;
            let mut range = RangeRef::cell(c);
            if let (Some(Tok::Colon | Tok::Trim(_)), Some(Tok::Word(w2))) = (toks.get(j + 1).map(|t| &t.tok), toks.get(j + 2).map(|t| &t.tok))
                && let Some(c2) = CellRef::parse(w2)
            {
                range = RangeRef::new(c, c2);
                end = toks[j + 2].end;
                i = j + 3;
            } else {
                i = j + 1;
            }
            let key = body.get(start..end).unwrap_or("").replace('$', "").to_ascii_uppercase();
            let idx = match seen.iter().position(|k| *k == key) {
                Some(k) => k,
                None => {
                    seen.push(key);
                    seen.len() - 1
                }
            };
            out.push((start + offset, end + offset, range, sheet, idx % REF_COLORS.len()));
            continue;
        }
        i += 1;
    }
    out
}

/// Excel-style AutoComplete: the one distinct text entry in the same column (contiguous data
/// above and below) that starts with `typed` (case-insensitive). `None` when ambiguous.
pub fn column_completion(sheet: &gridcraft_engine::model::Sheet, at: CellRef, typed: &str) -> Option<String> {
    if typed.is_empty() || typed.starts_with('=') || typed.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-') {
        return None;
    }
    let lower = typed.to_lowercase();
    let mut found: Option<String> = None;
    let mut ambiguous = false;
    let mut scan = |row: u32| -> bool {
        match sheet.value(CellRef::new(row, at.col)) {
            gridcraft_engine::core::Value::Empty => false,
            gridcraft_engine::core::Value::Text(t) => {
                let folded = t.to_lowercase();
                if folded.starts_with(&lower) && folded.len() > lower.len() {
                    match &found {
                        Some(f) if f.to_lowercase() != folded => ambiguous = true,
                        None => found = Some(t.to_string()),
                        _ => {}
                    }
                }
                true
            }
            _ => true,
        }
    };
    let mut r = at.row;
    let mut n = 0;
    while r > 0 && n < 5000 {
        r -= 1;
        n += 1;
        if !scan(r) {
            break;
        }
    }
    let mut r = at.row + 1;
    n = 0;
    while n < 5000 && scan(r) {
        r += 1;
        n += 1;
    }
    if ambiguous { None } else { found }
}

/// The part of `full` after the `typed` prefix, counted in characters: case folding can change
/// a character's byte length ("ß" is two bytes, "ẞ" three).
pub fn completion_rest<'a>(full: &'a str, typed: &str) -> Option<&'a str> {
    let start = full.char_indices().nth(typed.chars().count())?.0;
    full.get(start..)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_mode() {
        let mut e = EditState::new(0, CellRef::new(0, 0), "=SUM(".into(), true, false);
        assert!(e.can_point());
        e.insert_ref("A1");
        assert_eq!(e.text, "=SUM(A1");
        e.insert_ref("A2");
        assert_eq!(e.text, "=SUM(A2");
        e.text.push(':');
        e.caret += 1;
        e.point = None;
        assert!(e.can_point());
        let e2 = EditState::new(0, CellRef::new(0, 0), "hello".into(), true, false);
        assert!(!e2.can_point());
    }

    #[test]
    fn autocomplete_and_hints() {
        let names: Vec<String> = ["SUM", "SUMIF", "SUMPRODUCT", "AVERAGE"].iter().map(|s| s.to_string()).collect();
        let mut e = EditState::new(0, CellRef::new(0, 0), "=su".into(), true, false);
        e.update_autocomplete(&names);
        assert_eq!(e.autocomplete.len(), 3);
        assert!(e.accept_autocomplete());
        assert_eq!(e.text, "=SUM(");
        let e = EditState::new(0, CellRef::new(0, 0), "=IF(A1>0,SUM(B1,".into(), true, false);
        assert_eq!(e.current_function(), Some(("SUM".into(), 1)));
    }

    #[test]
    fn ime_composition() {
        use egui::ImeEvent;
        let preedit = |text: &str| ImeEvent::Preedit { text: text.into(), active_range_chars: None };
        let mut e = EditState::new(0, CellRef::new(0, 0), String::new(), true, false);
        let state = |e: &EditState| (e.text.clone(), e.preedit.clone(), e.caret);
        // Pre-edit text replaces the previous pre-edit text; a commit makes it final.
        for text in ["ㅎ", "하", "한"] {
            assert!(e.apply_ime(&preedit(text)));
            assert_eq!(state(&e), (text.to_string(), Some(0..1), 1));
        }
        assert!(e.apply_ime(&ImeEvent::Commit("한".into())));
        assert!(e.apply_ime(&preedit("ㄱ")));
        assert_eq!(state(&e), ("한ㄱ".into(), Some(1..2), 2));
        // Empty pre-edit text ends the composition without a commit.
        assert!(e.apply_ime(&preedit("")));
        assert_eq!(state(&e), ("한".into(), None, 1));
        // Nothing composed: empty events (repeated by some integrations) and newlines change nothing.
        for event in [preedit(""), ImeEvent::Commit(String::new()), ImeEvent::Commit("\n".into()), preedit("\r")] {
            assert!(!e.apply_ime(&event));
        }
        assert_eq!(state(&e), ("한".into(), None, 1));
        // Typed text and cancelling replace or drop only the pre-edit text.
        e.apply_ime(&preedit("ㄱ"));
        assert!(e.insert_typed(" "));
        assert_eq!(state(&e), ("한 ".into(), None, 2));
        e.apply_ime(&preedit("글"));
        assert!(e.cancel_preedit());
        assert!(!e.cancel_preedit());
        assert_eq!(state(&e), ("한 ".into(), None, 2));
    }

    #[test]
    fn ime_delete_surrounding() {
        use egui::ImeEvent;
        // A phone keyboard rewrites the last syllable: delete it, then compose its replacement.
        let mut e = EditState::new(0, CellRef::new(0, 0), "가나".into(), true, false);
        assert!(e.apply_ime(&ImeEvent::DeleteSurrounding { before_chars: 1, after_chars: 0 }));
        assert_eq!((e.text.as_str(), e.caret), ("가", 1));
        e.apply_ime(&ImeEvent::Preedit { text: "낙".into(), active_range_chars: None });
        // Around a composition, deleting counts from its ends and keeps it whole.
        assert!(e.apply_ime(&ImeEvent::DeleteSurrounding { before_chars: 5, after_chars: 5 }));
        assert_eq!((e.text.as_str(), e.preedit.clone(), e.caret), ("낙", Some(0..1), 1));
        assert!(!e.apply_ime(&ImeEvent::DeleteSurrounding { before_chars: 0, after_chars: 0 }));
        // Emoji and other multi-byte characters count as one each.
        let mut e = EditState::new(0, CellRef::new(0, 0), "a😀b".into(), true, false);
        e.caret = 2;
        assert!(e.apply_ime(&ImeEvent::DeleteSurrounding { before_chars: 1, after_chars: 1 }));
        assert_eq!((e.text.as_str(), e.caret), ("a", 1));
    }

    #[test]
    fn composes_text_skips_empty_and_newline_events() {
        use egui::{Event, ImeEvent};
        let preedit = |text: &str| Event::Ime(ImeEvent::Preedit { text: text.into(), active_range_chars: None });
        assert!(composes_text(&preedit("ㅎ")));
        assert!(composes_text(&Event::Ime(ImeEvent::Commit("日本".into()))));
        for event in [
            preedit(""),
            preedit("\n"),
            Event::Ime(ImeEvent::Commit(String::new())),
            Event::Ime(ImeEvent::Commit("\r".into())),
            Event::Ime(ImeEvent::DeleteSurrounding { before_chars: 1, after_chars: 0 }),
            Event::Text("a".into()),
        ] {
            assert!(!composes_text(&event), "{event:?}");
        }
    }

    #[test]
    fn ref_spans() {
        let r = formula_refs("=A1+B2:C3*a1+Sheet2!D4");
        assert_eq!(r.len(), 4);
        assert_eq!(r[0].4, r[2].4); // A1 and a1 share a colour
        assert_eq!(r[1].2.a1(), "B2:C3");
        assert_eq!(r[3].3.as_deref(), Some("Sheet2"));
    }
}
