//! A workbook opened read-only (`file.open {readOnly: true}`) in the interface: typing opens no
//! editor, nor do headers and footers, text boxes or sheet tabs; the AutoFilter menu (which can set
//! a table's filter without a command) and the drop-down list stay shut, and margins don't drag; the
//! info bar says so and offers Enable Editing (not for `lockReadOnly`); Save asks for a copy's
//! name and AutoSave leaves the file alone; the control channel says when it opens no editor and
//! writes no screenshot over the workbook's file.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Shape, pos2};
use egui_kittest::kittest::Queryable as _;
use gridcraft_engine::Session;
use gridcraft_engine::core::{CellRef, Value};
use gridcraft_ui_egui::control::{self, Outcome};
use gridcraft_ui_egui::{Services, SheetApp, grid};
use serde_json::json;

type Harness = egui_kittest::Harness<'static, SheetApp>;

/// A new workbook made by `build`, saved and opened again with `file.open` and `params`.
fn reopened_with(build: impl FnOnce(&mut Session), params: serde_json::Value) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    build(&mut s);
    let book = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap()["base64"].as_str().unwrap().to_string();
    let mut p = json!({"name": "book.xlsx", "base64": book});
    p.as_object_mut().unwrap().extend(params.as_object().unwrap().clone());
    let mut s = Session::new();
    s.execute("file.open", p).unwrap();
    s
}

/// A table over A1:B3 (a header row with filter buttons), saved and opened again with `file.open`
/// and `params`.
fn reopened(params: serde_json::Value) -> Session {
    reopened_with(
        |s| {
            s.execute("range.setValues", json!({"range": "A1", "values": [["Item", "Qty"], ["a", 1], ["b", 2]]})).unwrap();
            s.execute("insert.table", json!({"range": "A1:B3", "header": true})).unwrap();
        },
        params,
    )
}

/// The table workbook opened read-only.
fn read_only_session() -> Session {
    reopened(json!({"readOnly": true}))
}

/// A new directory for one test's files.
fn test_dir(tag: &str) -> std::path::PathBuf {
    #[allow(clippy::disallowed_methods)] // native-only test: a unique temp dir name
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("gridcraft-ui-{tag}-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    dir
}

/// A workbook (A1 = "kept") saved at `path`.
fn save_kept(path: &std::path::Path) -> Vec<u8> {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("cell.set", json!({"cell": "A1", "input": "kept"})).unwrap();
    s.execute("file.save", json!({"path": path})).unwrap();
    std::fs::read(path).unwrap()
}

fn harness(session: Session, services: Services) -> Harness {
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_step_dt(1.0 / 60.0).build_ui_state(
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

/// A primary press or release at `pos`, then a frame.
fn button(h: &mut Harness, pos: Pos2, pressed: bool) {
    h.input_mut()
        .events
        .extend([Event::PointerMoved(pos), Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }]);
    h.step();
}

fn click(h: &mut Harness, pos: Pos2) {
    // Apart from any earlier click: a double click needs two within egui's interval.
    h.run_steps(40);
    button(h, pos, true);
    button(h, pos, false);
    h.run_steps(2);
}

fn double_click(h: &mut Harness, pos: Pos2) {
    h.run_steps(40);
    button(h, pos, true);
    button(h, pos, false);
    h.step();
    button(h, pos, true);
    button(h, pos, false);
    h.run_steps(2);
}

/// Where `text` was painted this frame.
fn painted(h: &Harness, text: &str) -> Option<egui::Rect> {
    fn find(shape: &Shape, text: &str) -> Option<egui::Rect> {
        match shape {
            Shape::Text(t) if t.galley.job.text == text => Some(t.visual_bounding_rect()),
            Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, text)),
            _ => None,
        }
    }
    h.output().shapes.iter().find_map(|s| find(&s.shape, text))
}

/// Runs a control channel method; returns its reply (a screenshot it would take is `None`).
fn control(h: &mut Harness, method: &str, params: serde_json::Value) -> Option<serde_json::Value> {
    match control::handle(h.state_mut(), &egui::Context::default(), method, &params) {
        Outcome::Done(reply) => Some(reply),
        Outcome::Screenshot { .. } => None,
    }
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
    let dir = test_dir("read-only");
    let path = dir.join("book.xlsx");
    let saved = save_kept(&path);
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

#[test]
fn the_drop_down_list_stays_shut_on_a_read_only_workbook() {
    let session = reopened_with(
        |s| {
            s.execute("data.validation", json!({"range": "B2", "type": "list", "formula1": "\"Red,Green,Blue\""})).unwrap();
        },
        json!({"readOnly": true}),
    );
    let mut h = harness(session, Services::default());
    h.state_mut().run("selection.set", json!({"cell": "B2"})).unwrap();
    h.run_steps(2);
    let arrow = {
        let app = h.state();
        let sh = app.session.active().unwrap().wb.active().unwrap();
        let geo = grid::Geo::new(sh, app.grid.rect.unwrap(), app.view().scroll);
        grid::validation_arrow(sh, &geo, CellRef::parse("B2").unwrap()).expect("the list's arrow shows").center()
    };
    click(&mut h, arrow);
    assert!(h.state().grid.list_picker.is_none(), "the arrow opens no list: picking an item enters it");
    assert!(toast(&h).contains("read-only"), "{:?}", toast(&h));
    key(&mut h, Key::ArrowDown, Modifiers::ALT);
    h.run_steps(2);
    assert!(h.state().grid.list_picker.is_none(), "nor does Alt+Down");
    assert_eq!(value(&h, "B2"), Value::Empty);
}

/// The first page's top-margin centre and its left margin edge on the horizontal ruler, in a
/// sheet shown in Page Layout view.
fn page_points(h: &Harness) -> (Pos2, Pos2) {
    let g = h.state().grid.geo.clone().expect("the grid is laid out");
    let pg = g.paper.clone().expect("Page Layout view lays out pages");
    let screen = |x: f64, y: f64| pos2(g.cells.left() + (x - g.scroll.x as f64) as f32 * g.z, g.cells.top() + (y - g.scroll.y as f64) as f32 * g.z);
    let (px, py, w, _) = pg.page_rect(0, 0);
    let header = screen(px + w / 2.0, py + pg.margins[2] / 2.0);
    let edge = pos2(screen(pg.page_x(0) + pg.margins[0], 0.0).x, h.state().grid.rect.unwrap().top() + 9.0);
    (header, edge)
}

#[test]
fn headers_footers_and_margins_dont_change_in_a_read_only_workbook() {
    let session = reopened_with(
        |s| {
            for r in 1..=30 {
                s.execute("cell.set", json!({"cell": format!("A{r}"), "input": r.to_string()})).unwrap();
            }
            s.execute("pageLayout.headerFooter", json!({"header": "&LLeft"})).unwrap();
            s.execute("view.pageLayout", json!({})).unwrap();
        },
        json!({"readOnly": true}),
    );
    let mut h = harness(session, Services::default());
    h.run_steps(4);
    let print = h.state().session.active().unwrap().wb.active().unwrap().print.clone();
    let (header, edge) = page_points(&h);
    click(&mut h, header);
    assert!(h.state().grid.hf_edit.is_none(), "clicking the header opens no editor");
    assert!(toast(&h).contains("read-only"), "{:?}", toast(&h));
    // Insert › Header & Footer neither.
    gridcraft_ui_egui::paper_view::start_editing(h.state_mut());
    h.run_steps(4);
    assert!(h.state().grid.hf_edit.is_none(), "Insert › Header & Footer opens no editor");
    // Dragging a margin on the ruler doesn't start.
    h.run_steps(40);
    button(&mut h, edge, true);
    for dx in [5.0, 60.0] {
        h.input_mut().events.push(Event::PointerMoved(pos2(edge.x + dx, edge.y)));
        h.step();
    }
    assert!(h.state().grid.ruler_drag.is_none(), "no margin is dragged");
    button(&mut h, pos2(edge.x + 60.0, edge.y), false);
    h.run_steps(3);
    assert_eq!(h.state().session.active().unwrap().wb.active().unwrap().print, print, "headers and margins are as they were");
}

#[test]
fn text_boxes_open_no_editor_on_a_read_only_workbook() {
    let session = reopened_with(
        |s| {
            s.execute("insert.textBox", json!({"at": "B2", "width": 240, "height": 120, "text": "Original"})).unwrap();
        },
        json!({"readOnly": true}),
    );
    let mut h = harness(session, Services::default());
    let (id, centre) = {
        let app = h.state();
        let sh = app.session.active().unwrap().wb.active().unwrap();
        let shape = sh.shapes.iter().find(|s| s.kind == gridcraft_engine::model::ShapeKind::TextBox).expect("the text box");
        let geo = grid::Geo::new(sh, app.grid.rect.unwrap(), app.view().scroll);
        let a = shape.anchor;
        let at = pos2(geo.x(sh, a.cell.col) + a.dx * geo.z, geo.y(sh, a.cell.row) + a.dy * geo.z);
        (shape.id, at + egui::vec2(a.width, a.height) * geo.z / 2.0)
    };
    double_click(&mut h, centre);
    assert!(h.state().text_box_editor.is_none(), "double-clicking the box opens no editor");
    assert!(toast(&h).contains("read-only"), "{:?}", toast(&h));
    assert!(!h.state_mut().begin_text_box_edit(id), "nor does starting an edit directly");
    let sh = h.state().session.active().unwrap().wb.active().unwrap().clone();
    assert_eq!(sh.shapes.iter().find(|s| s.id == id).unwrap().text, "Original");
}

#[test]
fn sheet_tabs_dont_rename_on_a_read_only_workbook() {
    let mut h = harness(read_only_session(), Services::default());
    let tab = painted(&h, "Sheet1").expect("the sheet tab is painted").center();
    double_click(&mut h, tab);
    assert_eq!(h.state().grid.renaming_tab, None, "double-clicking the tab opens no name field");
    assert!(toast(&h).contains("read-only"), "{:?}", toast(&h));
    assert_eq!(h.state().session.active().unwrap().wb.sheets[0].name, "Sheet1");
}

/// AutoSave never writes a read-only workbook. (Nothing a read-only workbook allows leaves an
/// unsaved change; here one is made by hand, to show that it is the read-only state that keeps
/// AutoSave away: once editing is enabled, AutoSave saves the next change.)
#[test]
fn autosave_never_writes_a_read_only_workbook() {
    let dir = test_dir("read-only-autosave");
    let path = dir.join("book.xlsx");
    let saved = save_kept(&path);
    let mut s = Session::new();
    s.execute("file.open", json!({"path": path, "readOnly": true})).unwrap();
    s.active_mut().unwrap().saved = Arc::new(gridcraft_engine::model::Workbook::new());
    let mut h = harness(s, Services::default());
    h.state_mut().grid.autosave = true;
    h.run_steps(6);
    assert_eq!(h.state().grid.last_autosave, 0.0, "AutoSave didn't try");
    assert!(h.state().session.active().unwrap().is_dirty(), "AutoSave didn't save it");
    assert_eq!(std::fs::read(&path).unwrap(), saved, "the file is untouched");
    h.state_mut().run("file.enableEditing", json!({})).unwrap();
    h.state_mut().run("cell.set", json!({"cell": "B1", "input": "auto"})).unwrap();
    h.run_steps(6);
    let mut check = Session::new();
    check.execute("file.open", json!({"path": path})).unwrap();
    assert_eq!(check.active().unwrap().wb.active().unwrap().value(CellRef::parse("B1").unwrap()), Value::from("auto"), "AutoSave saves it now");
    std::fs::remove_dir_all(dir).unwrap();
}

/// Enable Editing in a second window on a read-only workbook enables the workbook, so AutoSave
/// saves that window's changes to its file (it used to fail without a word, the first window
/// keeping the file read-only).
#[test]
fn autosave_saves_a_new_window_editing_was_enabled_in() {
    let dir = test_dir("read-only-window-autosave");
    let path = dir.join("book.xlsx");
    save_kept(&path);
    let mut s = Session::new();
    s.execute("file.open", json!({"path": path, "readOnly": true})).unwrap();
    s.execute("view.newWindow", json!({})).unwrap();
    let mut h = harness(s, Services::default());
    h.state_mut().grid.autosave = true;
    h.get_by_label("Enable Editing").click();
    h.run_steps(4);
    assert!(h.state().session.documents().iter().all(|d| !d.read_only), "both windows are enabled");
    h.state_mut().run("cell.set", json!({"cell": "B1", "input": "from the second window"})).unwrap();
    h.run_steps(6);
    assert!(h.state().message.is_none(), "{:?}", h.state().message);
    let mut check = Session::new();
    check.execute("file.open", json!({"path": path})).unwrap();
    assert_eq!(check.active().unwrap().wb.active().unwrap().value(CellRef::parse("B1").unwrap()), Value::from("from the second window"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_control_channel_says_a_read_only_workbook_opens_no_editor() {
    let mut h = harness(read_only_session(), Services::default());
    let reply = control(&mut h, "ui.edit.begin", json!({"text": "x"})).unwrap();
    assert_eq!(reply["ok"], json!(false), "{reply}");
    assert!(reply["error"].as_str().unwrap().contains("the workbook is open read-only"), "{reply}");
    assert!(h.state().editor.is_none());
    h.state_mut().run("file.enableEditing", json!({})).unwrap();
    let reply = control(&mut h, "ui.edit.begin", json!({"text": "x"})).unwrap();
    assert_eq!(reply["ok"], json!(true), "{reply}");
    assert!(h.state().editor.is_some());
}

#[test]
fn screenshots_never_write_over_a_read_only_workbook() {
    let dir = test_dir("read-only-screenshot");
    let path = dir.join("book.xlsx");
    let saved = save_kept(&path);
    let mut s = Session::new();
    s.execute("file.open", json!({"path": path, "readOnly": true})).unwrap();
    let mut h = harness(s, Services::default());
    for over in [path.clone(), dir.join(".").join("book.xlsx")] {
        let reply = control(&mut h, "ui.screenshot", json!({"path": over})).expect("refused at once");
        assert_eq!(reply["ok"], json!(false), "{reply}");
        assert!(reply["error"].as_str().unwrap().contains("read-only"), "{reply}");
    }
    assert!(control(&mut h, "ui.screenshot", json!({"path": dir.join("shot.png")})).is_none(), "another path is taken");
    assert!(control(&mut h, "ui.screenshot", json!({})).is_none(), "and so is none (base64)");
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    std::fs::remove_dir_all(dir).unwrap();
}

/// `app.open` takes how to open a workbook as `file.open` does (`recalculate`, `readOnly`,
/// `lockReadOnly`), for its bytes or a file, and says in its reply how it opened it, so the
/// embedding page knows; a value `file.open` refuses is an error reply, and opens nothing.
#[test]
fn app_open_opens_read_only_and_says_how_it_opened() {
    // RAND() is calculated again on open: its stored result shows only without recalculating.
    let mut s = Session::new();
    s.new_workbook();
    s.execute("cell.set", json!({"cell": "A1", "input": "=RAND()"})).unwrap();
    let stored = s.active().unwrap().wb.active().unwrap().value(CellRef::new(0, 0));
    let book = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap()["base64"].as_str().unwrap().to_string();
    let mut h = harness(Session::new(), Services::default());
    let reply =
        control(&mut h, "app.open", json!({"name": "ledger.xlsx", "base64": book, "recalculate": false, "readOnly": true, "lockReadOnly": true}));
    assert_eq!(reply, Some(json!({"ok": true, "result": {"title": "ledger.xlsx", "readOnly": true, "lockReadOnly": true, "recalculated": false}})));
    let info = h.state_mut().session.run("document.inspect", json!({})).unwrap();
    assert_eq!((info["readOnly"].clone(), info["lockReadOnly"].clone()), (json!(true), json!(true)));
    assert_eq!(value(&h, "A1"), stored, "the result stored in the file");
    let n = h.state().session.documents().len();
    for params in [json!({"name": "x.xlsx", "base64": book, "readOnly": "yes"}), json!({"name": "x.xlsx", "base64": book, "recalculate": 0})] {
        let reply = control(&mut h, "app.open", params.clone()).unwrap();
        assert_eq!(reply["ok"], json!(false), "{params}: {reply}");
        assert!(reply["error"].as_str().unwrap().contains("must be true or false"), "{params}: {reply}");
    }
    assert_eq!(h.state().session.documents().len(), n, "nothing more was opened");
    // A file takes them the same way; opened again, it is switched to as it is.
    let dir = test_dir("app-open");
    let path = dir.join("book.xlsx");
    save_kept(&path);
    let reply = control(&mut h, "app.open", json!({"path": path, "readOnly": true})).unwrap();
    assert_eq!(reply["result"], json!({"title": "book.xlsx", "readOnly": true, "lockReadOnly": false, "recalculated": true}), "{reply}");
    let reply = control(&mut h, "app.open", json!({"path": path, "lockReadOnly": true})).unwrap();
    assert_eq!(reply["result"], json!({"title": "book.xlsx", "readOnly": true, "lockReadOnly": true, "alreadyOpen": true}), "{reply}");
    let reply = control(&mut h, "app.open", json!({"path": path, "readOnly": false, "recalculate": true})).unwrap();
    assert_eq!(
        reply["result"],
        json!({"title": "book.xlsx", "readOnly": true, "lockReadOnly": true, "alreadyOpen": true, "recalculateIgnored": true, "readOnlyIgnored": true}),
        "{reply}"
    );
    assert_eq!(control(&mut h, "app.open", json!({})).unwrap()["error"], json!("missing `path` or `base64`"));
    std::fs::remove_dir_all(dir).unwrap();
}
