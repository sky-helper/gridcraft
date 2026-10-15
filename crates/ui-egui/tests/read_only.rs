//! A workbook opened read-only (`file.open {readOnly: true}`) in the interface: typing opens no
//! editor, the AutoFilter menu (which can set a table's filter without a command) stays shut, the
//! info bar says so and offers Enable Editing (not for `lockReadOnly`), and Save asks for a copy's
//! name.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::{Event, Key, Modifiers, PointerButton};
use egui_kittest::kittest::Queryable as _;
use gridcraft_engine::Session;
use gridcraft_engine::core::{CellRef, Value};
use gridcraft_ui_egui::{Services, SheetApp, grid};
use serde_json::json;

type Harness = egui_kittest::Harness<'static, SheetApp>;

/// A table over A1:B3 (a header row with filter buttons), saved and opened again with `file.open`
/// and `params`.
fn reopened(params: serde_json::Value) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("range.setValues", json!({"range": "A1", "values": [["Item", "Qty"], ["a", 1], ["b", 2]]})).unwrap();
    s.execute("insert.table", json!({"range": "A1:B3", "header": true})).unwrap();
    let book = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap()["base64"].as_str().unwrap().to_string();
    let mut p = json!({"name": "book.xlsx", "base64": book});
    p.as_object_mut().unwrap().extend(params.as_object().unwrap().clone());
    let mut s = Session::new();
    s.execute("file.open", p).unwrap();
    s
}

/// The table workbook opened read-only.
fn read_only_session() -> Session {
    reopened(json!({"readOnly": true}))
}

fn harness(session: Session, services: Services) -> Harness {
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        SheetApp::new(session, services),
    );
    h.run_steps(4);
    h
}

fn key(h: &mut Harness, k: Key, m: Modifiers) {
    h.input_mut().events.push(Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: m });
    h.input_mut().events.push(Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers: m });
    h.run_steps(2);
}

fn text(h: &mut Harness, t: &str) {
    h.input_mut().events.push(Event::Text(t.into()));
    h.run_steps(2);
}

fn value(h: &Harness, a: &str) -> Value {
    h.state().session.active().and_then(|d| d.wb.active().map(|s| s.value(CellRef::parse(a).unwrap_or_default()))).unwrap_or_default()
}

fn read_only(h: &mut Harness) -> bool {
    h.state_mut().session.run("document.inspect", json!({})).unwrap()["readOnly"] == json!(true)
}

fn toast(h: &Harness) -> String {
    h.state().toast.as_ref().map(|(t, _)| t.clone()).unwrap_or_default()
}

#[test]
fn typing_in_a_read_only_workbook_opens_no_editor() {
    let mut h = harness(read_only_session(), Services::default());
    assert!(read_only(&mut h));
    text(&mut h, "x");
    assert!(h.state().editor.is_none(), "typing opens no editor");
    assert!(toast(&h).contains("read-only"), "and says why: {:?}", toast(&h));
    key(&mut h, Key::F2, Modifiers::NONE);
    assert!(h.state().editor.is_none(), "neither does F2");
    key(&mut h, Key::Backspace, Modifiers::NONE);
    assert!(h.state().editor.is_none(), "nor Backspace");
    // Clicking the formula bar or double-clicking a cell starts editing the same way.
    h.state_mut().begin_edit(None, true);
    h.run_steps(2);
    assert!(h.state().editor.is_none(), "nor the formula bar");
    assert_eq!(value(&h, "A1"), Value::from("Item"), "the cell is as it was");
}

#[test]
fn the_filter_menu_stays_shut_on_a_read_only_workbook() {
    let mut h = harness(read_only_session(), Services::default());
    let wb = h.state().session.active().unwrap().wb.clone();
    let button = {
        let app = h.state();
        let sh = app.session.active().unwrap().wb.active().unwrap();
        assert!(sh.autofilter.is_none() && sh.has_filter_button(CellRef::new(0, 0)), "the table's own filter button");
        let geo = grid::Geo::new(sh, app.grid.rect.unwrap(), app.view().scroll);
        grid::filter_button_rect(&geo, sh, CellRef::new(0, 0)).center()
    };
    h.input_mut().events.push(Event::PointerMoved(button));
    for pressed in [true, false] {
        h.input_mut().events.push(Event::PointerButton { pos: button, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(4);
    assert!(h.state().grid.filter_menu.is_none(), "the filter menu doesn't open");
    assert!(toast(&h).contains("read-only"), "{:?}", toast(&h));
    // However it is asked for (Alt+Down, the control channel), the menu sets no AutoFilter on the
    // table: that would change the workbook without going through a command.
    h.state_mut().grid.filter_menu = Some((0, 0, button));
    h.run_steps(4);
    assert!(h.state().grid.filter_menu.is_none());
    let d = h.state().session.active().unwrap();
    assert!(d.wb.active().unwrap().autofilter.is_none(), "no AutoFilter was set");
    assert!(Arc::ptr_eq(&d.wb, &wb), "the workbook is as it was");
}

#[test]
fn the_info_bar_offers_enable_editing() {
    let mut h = harness(read_only_session(), Services::default());
    assert!(h.query_by_label_contains("Read-only").is_some(), "the info bar says the workbook is read-only");
    h.get_by_label("Enable Editing").click();
    h.run_steps(4);
    assert!(!read_only(&mut h), "Enable Editing turns read-only off");
    assert!(h.query_by_label("Enable Editing").is_none(), "and the bar goes away");
    text(&mut h, "x");
    assert!(h.state().editor.is_some(), "typing edits now");
}

#[test]
fn a_locked_read_only_workbook_offers_no_enable_editing() {
    let mut h = harness(reopened(json!({"readOnly": true, "lockReadOnly": true})), Services::default());
    assert!(h.query_by_label_contains("Read-only").is_some(), "the info bar still says read-only");
    assert!(h.query_by_label("Enable Editing").is_none(), "without Enable Editing");
    text(&mut h, "x");
    assert!(h.state().editor.is_none());
    assert!(toast(&h).contains("read-only") && !toast(&h).contains("Enable Editing"), "{:?}", toast(&h));
    let r = h.state_mut().run("file.enableEditing", json!({}));
    assert!(r.as_ref().is_err_and(|e| e.contains("can't be switched to editing")), "{r:?}");
    h.run_steps(2);
    assert!(read_only(&mut h), "it stays read-only");
}

#[test]
fn save_on_a_read_only_workbook_asks_for_a_copy() {
    #[allow(clippy::disallowed_methods)] // native-only test: a unique temp dir name
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("gridcraft-ui-read-only-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("book.xlsx");
    let mut s = Session::new();
    s.new_workbook();
    s.execute("cell.set", json!({"cell": "A1", "input": "kept"})).unwrap();
    s.execute("file.save", json!({"path": path})).unwrap();
    let saved = std::fs::read(&path).unwrap();
    let mut s = Session::new();
    s.execute("file.open", json!({"path": path, "readOnly": true})).unwrap();
    let asked = Arc::new(AtomicBool::new(false));
    let flag = asked.clone();
    let services = Services {
        pick_save: Some(Box::new(move |_| {
            flag.store(true, Ordering::SeqCst);
            None
        })),
        ..Default::default()
    };
    let mut h = harness(s, services);
    assert!(h.state().grid.last_title.as_deref().is_some_and(|t| t.contains("[Read-Only]")), "the title says so: {:?}", h.state().grid.last_title);
    h.state_mut().run_or_alert("file.save", json!({}));
    assert!(asked.load(Ordering::SeqCst), "Save asks where to save a copy, as Save As does");
    assert!(h.state().message.is_none() && toast(&h).is_empty(), "rather than failing: {:?} {:?}", h.state().message, toast(&h));
    assert_eq!(std::fs::read(&path).unwrap(), saved, "the file is untouched");
    std::fs::remove_dir_all(dir).unwrap();
}
