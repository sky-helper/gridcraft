//! Typing into a selected cell through an input method (Korean, Japanese, Chinese): the first
//! composition starts while only the cell is selected and ends up in the cell, as plain typing does.
//!
//! The events are what the egui integrations send: `Ime(Preedit)` while composing, `Ime(Commit)`
//! when a character is final. Events pushed together arrive in one frame, as the integrations
//! deliver the events of one keystroke.

use egui::output::IMEOutput;
use egui::{Event, Id, ImeEvent, Key, Modifiers, Pos2, TouchDeviceId, TouchId, TouchPhase};
use gridcraft_engine::Session;
use gridcraft_engine::core::CellRef;
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

type Harness = egui_kittest::Harness<'static, SheetApp>;

fn harness(selected: &str) -> Harness {
    let mut session = Session::new();
    session.new_workbook();
    session.execute("range.setValues", json!({"range": "A1", "values": [["old", "keep"], ["below", 2]]})).unwrap();
    session.execute("selection.set", json!({"range": selected})).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        SheetApp::new(session, Default::default()),
    );
    h.run_steps(4);
    h
}

fn preedit(text: &str) -> Event {
    Event::Ime(ImeEvent::Preedit { text: text.into(), active_range_chars: None })
}

fn commit(text: &str) -> Event {
    Event::Ime(ImeEvent::Commit(text.into()))
}

fn press(key: Key) -> [Event; 2] {
    let event = |pressed| Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers: Modifiers::NONE };
    [event(true), event(false)]
}

/// One frame with these events.
fn frame(h: &mut Harness, events: impl IntoIterator<Item = Event>) {
    h.input_mut().events.extend(events);
    h.step();
}

fn ime(h: &Harness) -> IMEOutput {
    h.output().platform_output.ime.expect("the frame asks for the IME")
}

fn editing(h: &Harness) -> Option<String> {
    h.state().editor.as_ref().map(|e| e.text.clone())
}

fn cell_editor_focused(h: &Harness) -> bool {
    h.ctx.memory(|m| m.focused()) == Some(Id::new("gridcraft.cell_editor"))
}

fn input(h: &Harness, a1: &str) -> String {
    h.state().session.active().unwrap().wb.active().unwrap().input_text(CellRef::parse(a1).unwrap())
}

fn active(h: &Harness) -> String {
    h.state().session.active().unwrap().selection.active.a1()
}

/// Where the cell is on screen in the last frame.
fn cell_rect(h: &Harness, a1: &str) -> egui::Rect {
    let app = h.state();
    let sheet = app.session.active().unwrap().wb.active().unwrap();
    let geo = app.grid.geo.as_ref().unwrap();
    geo.mirror_rect(geo.cell_rect(sheet, CellRef::parse(a1).unwrap()))
}

/// Still composing: the grid keeps the keyboard and nothing interrupts the composition.
fn assert_composing(h: &Harness, shown: &str) {
    assert_eq!(editing(h).as_deref(), Some(shown));
    assert!(!cell_editor_focused(h), "moving focus mid-composition would interrupt it");
    assert!(!ime(h).should_interrupt_composition, "the composition of {shown:?} was interrupted");
}

#[test]
fn a_selected_cell_asks_for_the_ime_where_typing_goes() {
    let mut h = harness("B2");
    let out = ime(&h);
    assert_eq!(out.rect, cell_rect(&h, "B2"));
    assert!(out.rect.contains(out.cursor_rect.center()), "the IME window opens at the cell");
    assert!(!out.should_interrupt_composition);
    assert!(h.state().editor.is_none(), "asking for the IME doesn't start an edit");
    frame(&mut h, press(Key::ArrowDown));
    h.step();
    assert_eq!(active(&h), "B3");
    assert_eq!(ime(&h).rect, cell_rect(&h, "B3"));
}

#[test]
fn hangul_composed_in_a_selected_cell_is_entered() {
    let mut h = harness("A1");
    for text in ["ㅎ", "하", "한"] {
        frame(&mut h, [preedit(text)]);
        assert_composing(&h, text);
    }
    // The composition ended: the cell editor takes the keyboard (nothing left to interrupt).
    frame(&mut h, [commit("한")]);
    assert_eq!(editing(&h).as_deref(), Some("한"));
    assert!(cell_editor_focused(&h));
    for (event, shown) in [(preedit("ㄱ"), "한ㄱ"), (preedit("그"), "한그"), (preedit("글"), "한글"), (commit("글"), "한글")] {
        frame(&mut h, [event]);
        assert_eq!(editing(&h).as_deref(), Some(shown));
        assert!(!ime(&h).should_interrupt_composition);
    }
    frame(&mut h, press(Key::Enter));
    h.step();
    assert!(h.state().editor.is_none());
    assert_eq!(input(&h, "A1"), "한글");
    assert_eq!(active(&h), "A2");
}

#[test]
fn hangul_typed_without_pause_stays_one_composition_until_enter() {
    let mut h = harness("A1");
    frame(&mut h, [preedit("ㅎ")]);
    frame(&mut h, [preedit("하")]);
    frame(&mut h, [preedit("한")]);
    // The keystroke that starts the next syllable commits the last one, in the same frame.
    frame(&mut h, [preedit(""), commit("한"), preedit("ㄱ")]);
    assert_composing(&h, "한ㄱ");
    frame(&mut h, [preedit("그")]);
    frame(&mut h, [preedit("글")]);
    assert_composing(&h, "한글");
    // Enter commits the syllable and the cell at once.
    let [down, up] = press(Key::Enter);
    frame(&mut h, [preedit(""), commit("글"), down, up]);
    h.step();
    assert!(h.state().editor.is_none());
    assert_eq!(input(&h, "A1"), "한글");
    assert_eq!(active(&h), "A2");
    // Back to a selected cell: the IME is there for the next one.
    assert_eq!(ime(&h).rect, cell_rect(&h, "A2"));
}

#[test]
fn a_key_just_before_a_composition_counts_once() {
    // Enter and the first syllable in one frame: Enter moves down, the composition starts there,
    // and the editor doesn't take the same Enter as committing it.
    let mut h = harness("A1");
    let [down, up] = press(Key::Enter);
    frame(&mut h, [down, up, preedit("ㅎ")]);
    assert_eq!(active(&h), "A2");
    assert_composing(&h, "ㅎ");
    assert_eq!(h.state().editor.as_ref().map(|e| e.cell.a1()).as_deref(), Some("A2"));
    frame(&mut h, [preedit(""), commit("한")]);
    frame(&mut h, press(Key::Enter));
    h.step();
    assert_eq!(input(&h, "A2"), "한");
    assert_eq!(input(&h, "A1"), "old");
    assert_eq!(active(&h), "A3");
}

#[test]
fn text_typed_after_a_composition_follows_it() {
    let mut h = harness("A1");
    frame(&mut h, [preedit("ㅎ")]);
    frame(&mut h, [preedit("한")]);
    // A space ends the syllable; the editor gets the keyboard and goes on from the caret.
    let [down, up] = press(Key::Space);
    frame(&mut h, [preedit(""), commit("한"), down, Event::Text(" ".into()), up]);
    assert_eq!(editing(&h).as_deref(), Some("한 "));
    frame(&mut h, [Event::Text("A".into())]);
    frame(&mut h, [Event::Text("1".into())]);
    assert_eq!(editing(&h).as_deref(), Some("한 A1"));
    frame(&mut h, press(Key::Tab));
    h.step();
    assert_eq!(input(&h, "A1"), "한 A1");
    assert_eq!(active(&h), "B1");
}

#[test]
fn a_japanese_conversion_enters_the_chosen_candidate() {
    let mut h = harness("A1");
    for text in ["n", "に", "にh", "にほ", "にほn", "にほん"] {
        frame(&mut h, [preedit(text)]);
        assert_composing(&h, text);
    }
    // Space converts; picking another candidate replaces the pre-edit text in place.
    for text in ["日本", "二本", "日本"] {
        frame(&mut h, [Event::Ime(ImeEvent::Preedit { text: text.into(), active_range_chars: Some(0..2) })]);
        assert_composing(&h, text);
    }
    // Enter confirms the conversion (the IME keeps that key); the edit stays open.
    frame(&mut h, [preedit(""), commit("日本")]);
    assert_eq!(editing(&h).as_deref(), Some("日本"));
    h.step();
    assert!(cell_editor_focused(&h));
    frame(&mut h, press(Key::Enter));
    h.step();
    assert_eq!(input(&h, "A1"), "日本");
    assert_eq!(active(&h), "A2");
}

#[test]
fn escape_while_composing_drops_only_the_composition() {
    let mut h = harness("A1");
    frame(&mut h, [preedit("ㅎ")]);
    frame(&mut h, [preedit("한")]);
    frame(&mut h, [preedit(""), commit("한"), preedit("ㄱ")]);
    assert_composing(&h, "한ㄱ");
    // The syllable being composed goes, and the platform is told to drop it too; what was
    // already composed stays in the open editor.
    frame(&mut h, press(Key::Escape));
    assert_eq!(editing(&h).as_deref(), Some("한"));
    assert!(ime(&h).should_interrupt_composition);
    h.step();
    assert!(cell_editor_focused(&h));
    // Escape again cancels the edit: the cell keeps its value.
    frame(&mut h, press(Key::Escape));
    h.step();
    assert!(h.state().editor.is_none());
    assert_eq!(input(&h, "A1"), "old");

    // Nothing composed yet: the editor stays open and empty until the next Escape.
    frame(&mut h, [preedit("ㅎ")]);
    frame(&mut h, press(Key::Escape));
    assert_eq!(editing(&h).as_deref(), Some(""));
    h.step();
    frame(&mut h, press(Key::Escape));
    h.step();
    assert!(h.state().editor.is_none());
    assert_eq!(input(&h, "A1"), "old");
}

#[test]
fn clicking_another_cell_while_composing_enters_what_was_composed() {
    let mut h = harness("A1");
    frame(&mut h, [preedit("ㅎ")]);
    frame(&mut h, [preedit("한")]);
    let to: Pos2 = cell_rect(&h, "C4").center();
    let button = |pressed| Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
    frame(&mut h, [Event::PointerMoved(to)]);
    frame(&mut h, [button(true)]);
    frame(&mut h, [button(false)]);
    h.step();
    assert!(h.state().editor.is_none());
    assert_eq!(input(&h, "A1"), "한");
    assert_eq!(active(&h), "C4");
    // The platform drops its composition: an empty one arriving late starts nothing.
    frame(&mut h, [preedit(""), commit("")]);
    assert!(h.state().editor.is_none());
}

#[test]
fn an_empty_ime_event_leaves_keytips_open() {
    // Wayland: winit sends empty pre-edit text on each text-input `done`, also when the IME is
    // turned off, as it is when keytips open (the grid stops asking for it).
    let mut h = harness("A1");
    frame(&mut h, press(Key::F10));
    assert!(h.state().keytips.prefix().is_some());
    let empty_at_caret = Event::Ime(ImeEvent::Preedit { text: String::new(), active_range_chars: Some(0..0) });
    frame(&mut h, [preedit(""), empty_at_caret, commit("")]);
    assert!(h.state().keytips.prefix().is_some(), "an empty IME event closed the keytips");
    frame(&mut h, press(Key::H));
    assert_eq!(h.state().keytips.prefix(), Some("H"));
    // Composed text closes them, as before.
    frame(&mut h, [commit("가")]);
    assert!(h.state().keytips.prefix().is_none());
}

#[test]
fn typing_arrows_shortcuts_and_f2_are_unchanged() {
    let mut h = harness("C3");
    for c in ["a", "b", "c"] {
        frame(&mut h, [Event::Text(c.into())]);
    }
    assert_eq!(editing(&h).as_deref(), Some("abc"));
    assert!(cell_editor_focused(&h));
    frame(&mut h, press(Key::Enter));
    h.step();
    assert_eq!(input(&h, "C3"), "abc");
    assert_eq!(active(&h), "C4");
    // The grid takes the keyboard back from the closed editor and locks the arrows to itself a
    // frame later; an arrow in between would also move egui's focus off the grid.
    h.step();
    frame(&mut h, press(Key::ArrowUp));
    h.step();
    assert_eq!(active(&h), "C3");
    // ⌘C marks the cell for pasting and starts no edit.
    frame(&mut h, [Event::Copy]);
    assert!(h.state().session.clipboard.is_some());
    assert!(h.state().editor.is_none());
    frame(&mut h, press(Key::Escape));
    h.step();
    assert!(h.state().session.clipboard.is_none());
    // F2 edits the content in the cell editor, which composes by itself.
    frame(&mut h, press(Key::F2));
    h.step();
    assert!(cell_editor_focused(&h));
    for event in [preedit("ㄱ"), preedit("가"), commit("가")] {
        frame(&mut h, [event]);
    }
    assert_eq!(editing(&h).as_deref(), Some("abc가"));
    frame(&mut h, press(Key::Enter));
    h.step();
    assert_eq!(input(&h, "C3"), "abc가");
}

#[test]
fn empty_ime_events_start_no_edit() {
    // Some integrations repeat empty pre-edit text while nothing is being composed.
    let mut h = harness("A1");
    let empty_at_caret = Event::Ime(ImeEvent::Preedit { text: String::new(), active_range_chars: Some(0..0) });
    frame(&mut h, [preedit(""), empty_at_caret, commit(""), commit("\n")]);
    h.step();
    assert!(h.state().editor.is_none());
    assert_eq!(input(&h, "A1"), "old");
}

#[test]
fn no_ime_while_keytips_wait_for_a_letter_or_on_a_touch_screen() {
    let mut h = harness("A1");
    // F10 shows the keytips: the next letter picks a ribbon tab, so the IME must not take it.
    frame(&mut h, press(Key::F10));
    assert!(h.state().keytips.prefix().is_some());
    assert!(h.output().platform_output.ime.is_none());
    frame(&mut h, press(Key::Escape));
    h.step();
    assert!(h.state().keytips.prefix().is_none());
    assert!(h.output().platform_output.ime.is_some());
    // Asking for the IME on a touch screen would bring up the on-screen keyboard for a tap.
    let at = cell_rect(&h, "B2").center();
    let touch = |phase| Event::Touch { device_id: TouchDeviceId(1), id: TouchId(1), phase, pos: at, force: None };
    frame(&mut h, [touch(TouchPhase::Start), touch(TouchPhase::End)]);
    h.step();
    assert!(h.output().platform_output.ime.is_none());
}
