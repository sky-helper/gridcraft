//! Typing into a selected cell through an input method (Korean, Japanese, Chinese): the first
//! composition starts while only the cell is selected and ends up in the cell, as plain typing does.
//!
//! The events are what the egui integrations send: `Ime(Preedit)` while composing, `Ime(Commit)`
//! when a character is final. Events pushed together arrive in one frame, as the integrations
//! deliver the events of one keystroke. Where the platforms differ, a test says whose order it
//! follows: winit (desktop) sends an empty pre-edit before each commit, `[Preedit(""), Commit]`,
//! and on Windows also delivers the key that completed a syllable right after its commit, while
//! the web runner sends a commit alone (`compositionend`).

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

fn button(at: Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
}

/// A click as winit delivers it: the pointer gets there, then press and release in frames of
/// their own, and an idle frame.
fn click(h: &mut Harness, at: Pos2) {
    frame(h, [Event::PointerMoved(at)]);
    frame(h, [button(at, true)]);
    frame(h, [button(at, false)]);
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
    // Enter commits the syllable and the cell at once: Windows' order, where the Korean IME
    // completes the syllable and lets Enter through (macOS keeps that Enter: press it twice).
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
    // A space ends the syllable and is typed after it (winit's order); the editor gets the
    // keyboard and goes on from the caret.
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
    // The editor's own handling, for a platform or input method that lets the key through: the
    // web runner and Windows keep Escape for the IME while it composes (the browser marks the
    // keydown as composing; egui-winit drops keys the IME processed).
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
    // A platform that commits the composition when told to drop it doesn't bring it back.
    frame(&mut h, [preedit(""), commit("ㄱ")]);
    assert_eq!(editing(&h).as_deref(), Some("한"));
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
    let to = cell_rect(&h, "C4").center();
    click(&mut h, to);
    assert!(h.state().editor.is_none());
    assert_eq!(input(&h, "A1"), "한");
    assert_eq!(active(&h), "C4");
    // macOS drops the composition silently; Linux input methods may report an empty one late
    // (and Wayland repeats empty pre-edit text): that starts nothing.
    frame(&mut h, [preedit(""), commit("")]);
    assert!(h.state().editor.is_none());
}

#[test]
fn clicking_another_cell_on_the_web_enters_the_composition_once() {
    // The web runner: the press takes the browser's focus off its IME text field, which ends the
    // composition (`compositionend`, a commit) before the release, in a frame of its own or in
    // the release's.
    for with_release in [false, true] {
        let mut h = harness("A1");
        frame(&mut h, [preedit("ㅎ")]);
        frame(&mut h, [preedit("한")]);
        let to = cell_rect(&h, "C4").center();
        frame(&mut h, [Event::PointerMoved(to)]);
        frame(&mut h, [button(to, true)]);
        if with_release {
            frame(&mut h, [commit("한"), button(to, false)]);
        } else {
            frame(&mut h, [commit("한")]);
            frame(&mut h, [button(to, false)]);
        }
        h.step();
        assert!(h.state().editor.is_none(), "editing {:?}", editing(&h));
        assert_eq!(input(&h, "A1"), "한");
        assert_eq!(active(&h), "C4");
    }
}

#[test]
fn the_late_commit_of_what_a_click_entered_starts_no_edit() {
    // Windows: the click turns the IME off (and on again for the next cell), and winit sends what
    // the IME was composing as a commit when that ends the composition (`WM_IME_ENDCOMPOSITION`
    // while composing). The click already entered it in A1.
    let mut h = harness("A1");
    frame(&mut h, [preedit("ㅎ")]);
    frame(&mut h, [preedit("한")]);
    let to = cell_rect(&h, "C4").center();
    click(&mut h, to);
    assert_eq!(input(&h, "A1"), "한");
    assert_eq!(active(&h), "C4");
    frame(&mut h, [preedit(""), commit("한")]);
    h.step();
    assert!(h.state().editor.is_none(), "the late commit started an edit with {:?}", editing(&h));
    assert_eq!(input(&h, "C4"), "");
    // The same when the cell editor's text field was composing: after F2 (as before this change,
    // when the grid ignored IME events) or from the second syllable on.
    for f2 in [true, false] {
        let mut h = harness("A1");
        if f2 {
            frame(&mut h, press(Key::F2));
            h.step();
        } else {
            frame(&mut h, [preedit("ㅎ")]);
            frame(&mut h, [preedit(""), commit("한")]);
            h.step();
            assert!(cell_editor_focused(&h));
        }
        frame(&mut h, [preedit("ㄱ")]);
        frame(&mut h, [preedit("가")]);
        let composed = if f2 { "old가" } else { "한가" };
        assert_eq!(editing(&h).as_deref(), Some(composed));
        let to = cell_rect(&h, "C4").center();
        click(&mut h, to);
        assert_eq!(input(&h, "A1"), composed);
        frame(&mut h, [preedit(""), commit("가")]);
        h.step();
        assert!(h.state().editor.is_none(), "the late commit started an edit with {:?}", editing(&h));
    }
}

#[test]
fn only_the_late_commit_itself_is_dropped() {
    let composed_then_clicked = || {
        let mut h = harness("A1");
        frame(&mut h, [preedit("ㅎ")]);
        frame(&mut h, [preedit("한")]);
        let to = cell_rect(&h, "C4").center();
        click(&mut h, to);
        h
    };
    // Without a late commit (macOS, the web), the next composition starts as usual, also of the
    // same syllable: it starts with pre-edit text.
    let mut h = composed_then_clicked();
    frame(&mut h, [preedit("ㅎ")]);
    assert_composing(&h, "ㅎ");
    frame(&mut h, [preedit("한")]);
    frame(&mut h, [preedit(""), commit("한")]);
    frame(&mut h, press(Key::Enter));
    h.step();
    assert_eq!(input(&h, "C4"), "한");
    // Other text committed straight away.
    let mut h = composed_then_clicked();
    frame(&mut h, [commit("가")]);
    assert_eq!(editing(&h).as_deref(), Some("가"));
    // The same text once the platform's moment has passed.
    let mut h = composed_then_clicked();
    h.run_steps(20);
    frame(&mut h, [commit("한")]);
    assert_eq!(editing(&h).as_deref(), Some("한"));
}

#[test]
fn moving_to_the_formula_bar_while_composing_keeps_the_syllable_once() {
    // A click in the formula bar takes the keyboard from the grid mid-composition: what was
    // composed stays as typed text, and editing goes on there. Windows' late commit of it (see
    // above) must not add it again. The same from the cell editor's text field after F2.
    for f2 in [false, true] {
        let mut h = harness("A1");
        if f2 {
            frame(&mut h, press(Key::F2));
            h.step();
        }
        frame(&mut h, [preedit("ㅎ")]);
        frame(&mut h, [preedit("한")]);
        let composed = if f2 { "old한" } else { "한" };
        assert_eq!(editing(&h).as_deref(), Some(composed));
        let bar = Id::new("gridcraft.formula_bar");
        let field = h.ctx.read_response(bar).expect("the formula bar shows the edit").rect;
        click(&mut h, field.right_center() - egui::vec2(8.0, 0.0));
        assert_eq!(h.ctx.memory(|m| m.focused()), Some(bar));
        assert_eq!(editing(&h).as_deref(), Some(composed));
        frame(&mut h, [preedit(""), commit("한")]);
        h.step();
        assert_eq!(editing(&h).as_deref(), Some(composed), "the late commit was typed again");
        frame(&mut h, [Event::Text("x".into())]);
        assert_eq!(editing(&h).as_deref(), Some(format!("{composed}x").as_str()));
        frame(&mut h, press(Key::Enter));
        h.step();
        assert_eq!(input(&h, "A1"), format!("{composed}x"));
    }
}

#[test]
fn a_paste_or_key_in_the_frame_the_composition_ends_is_kept() {
    // Web: Cmd+V while composing commits the composition, then the paste event comes. The text
    // field takes the keyboard in this frame; it still gets the paste.
    let mut h = harness("A1");
    frame(&mut h, [preedit("ㅎ")]);
    frame(&mut h, [preedit("한")]);
    frame(&mut h, [preedit(""), commit("한"), Event::Paste("x".into())]);
    h.step();
    assert_eq!(editing(&h).as_deref(), Some("한x"));
    // Lines pasted into the cell's one-line field join with spaces, as the field does.
    frame(&mut h, [preedit("ㄱ")]);
    frame(&mut h, [preedit(""), commit("ㄱ"), Event::Paste("y\nz".into())]);
    h.step();
    assert_eq!(editing(&h).as_deref(), Some("한xㄱy z"));
    // Windows: a key the IME doesn't keep (Home here) completes the syllable and comes right
    // after its commit.
    let mut h = harness("A1");
    frame(&mut h, [preedit("ㅎ")]);
    frame(&mut h, [preedit("한")]);
    let [down, up] = press(Key::Home);
    frame(&mut h, [preedit(""), commit("한"), down, up]);
    frame(&mut h, [Event::Text("x".into())]);
    assert_eq!(editing(&h).as_deref(), Some("x한"));
}

#[test]
fn a_composition_starting_right_after_enter_is_not_interrupted() {
    // The first syllable for the next cell can come in the frame right after Enter closed the
    // editor, with no idle frame between (a busy machine): taking the keyboard back from the
    // closed editor must not interrupt it.
    let mut h = harness("A1");
    frame(&mut h, [Event::Text("a".into())]);
    h.step();
    frame(&mut h, press(Key::Enter));
    assert!(h.state().editor.is_none());
    frame(&mut h, [preedit("ㅎ")]);
    assert_composing(&h, "ㅎ");
    frame(&mut h, [preedit("한")]);
    frame(&mut h, [preedit(""), commit("한")]);
    frame(&mut h, press(Key::Enter));
    h.step();
    assert_eq!(input(&h, "A1"), "a");
    assert_eq!(input(&h, "A2"), "한");
    // The same after Escape cancelled an edit.
    frame(&mut h, [Event::Text("b".into())]);
    h.step();
    frame(&mut h, press(Key::Escape));
    assert!(h.state().editor.is_none());
    frame(&mut h, [preedit("ㄱ")]);
    assert_composing(&h, "ㄱ");
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
    assert_eq!(input(&h, "C3"), "abc");
    assert_eq!(active(&h), "C4");
    // The grid takes the keyboard back as Enter closes the editor and locks the arrows to itself
    // in the next frame, the one egui runs after any input; an arrow in that very frame would
    // still move egui's focus off the grid.
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
