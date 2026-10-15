//! File menu: new, open, save, close, export, properties, options.

use serde_json::{Value as Json, json};

use super::*;
use crate::DocState;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(noundo "file.new", "New Workbook", ["File"], Some("Cmd+N"), "{sample?: \"budget\"|\"sales\"|\"grades\"}", always, new_workbook),
        cmd!(
            noundo "file.open",
            "Open…",
            ["File"],
            Some("Cmd+O"),
            "{path} | {name, base64}, plus {password} for a password-protected workbook (xlsx, xlsm, xls import, xlsb and ods data import, csv, tsv, txt, json), {recalculate?: bool}: false shows the results stored in the file instead of calculating the formulas again (a formula stored without one is blank until calculated); by default they are calculated again unless the workbook was saved in manual calculation mode and doesn't ask to be calculated on load, {readOnly?: bool}: true opens it for reading only (commands that would change it are refused, and its file isn't written over) until file.enableEditing, and {lockReadOnly?: bool}: true opens it read-only for good (file.enableEditing is refused); these three need `path` or `base64` (no dialog asks for the file) → {index, title, sheets, warnings, recalculated, readOnly, lockReadOnly}; a file already open (however its path is spelled) is switched to as it is → {index, alreadyOpen, readOnly, lockReadOnly, recalculateIgnored?, readOnlyIgnored?}: readOnly or lockReadOnly is refused while it is open for editing, lockReadOnly locks it while it is read-only, and readOnlyIgnored says it stays more read-only than asked",
            always,
            open
        ),
        cmd!(
            noundo "file.save",
            "Save",
            ["File"],
            Some("Cmd+S"),
            "{path?} (xlsx by default; .csv/.tsv/.json/.html/.pdf by extension; a read-only workbook's own file is refused, and another path gets a copy: the workbook stays read-only, on its own file)",
            has_doc,
            |s, p| save(s, p, "file.save")
        ),
        cmd!(
            noundo "file.saveAs",
            "Save As…",
            ["File"],
            Some("Cmd+Shift+S"),
            "{path} (.xlsx, .csv, .tsv, .json, .html, or .pdf: a PDF of the active sheet; a read-only workbook saves a copy under another name and stays read-only, on its own file)",
            has_doc,
            save_as
        ),
        // Journaled: a journal or recorded script that opened a workbook read-only and enabled
        // editing replays; on a workbook that isn't read-only it does nothing.
        cmd!(
            noundo "file.enableEditing",
            "Enable Editing",
            [],
            None,
            "{} lets a workbook opened read-only (file.open {readOnly: true}) be changed and saved again, in every window on it → {readOnly: false, windows}; refused for one opened with {lockReadOnly: true}; on a workbook that isn't read-only it does nothing → {readOnly: false, alreadyEditable: true}",
            can_enable_editing,
            enable_editing
        ),
        cmd!(query "file.saveBytes", "Encode Workbook", [], None, "{format?: xlsx|csv|tsv|json|html} → {base64}", has_doc, save_bytes),
        cmd!(noundo "file.close", "Close", ["File"], Some("Cmd+W"), "{force?: bool}", has_doc, close),
        cmd!(noundo "file.exportCsv", "Export as CSV", ["File", "Export"], None, "{path, sheet?}", has_doc, |s, p| export(s, p, "csv")),
        cmd!(noundo "file.exportHtml", "Save as Web Page", ["File", "Export"], None, "{path}", has_doc, |s, p| export(s, p, "html")),
        cmd!("file.properties", "Properties", ["File"], None, "{title?, subject?, author?, company?, keywords?, description?}", has_doc, properties),
        cmd!(
            query "file.options",
            "Options",
            ["File"],
            None,
            "{} → {calculation: {mode, iterative, maxIterations, maxChange}} for the active workbook (the interface opens File › Options; change settings with their own commands, e.g. formulas.calculationOptions, view.theme, app.language.set)",
            always,
            options
        ),
        cmd!(noundo "window.activate", "Switch Window", ["Window"], None, "{index}", has_doc, activate_window),
    ]
}

fn new_workbook(s: &mut Session, p: &Json) -> Result<Json> {
    let i = match str_param(p, "sample") {
        Some(name) => {
            let wb = crate::sample::build(name).ok_or_else(|| bad("file.new", format!("unknown sample `{name}`")))?;
            let title = crate::sample::title(name);
            s.add_document(DocState::new(wb, None, title))
        }
        None => s.new_workbook(),
    };
    Ok(json!({"index": i, "title": s.doc()?.display_title()}))
}

/// An optional true-or-false param of `file.open`, checked before anything opens: a value that
/// isn't true or false must not pick either quietly.
fn open_flag(p: &Json, key: &str) -> Result<Option<bool>> {
    match p.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Bool(b)) => Ok(Some(*b)),
        Some(v) => Err(bad("file.open", format!("`{key}` must be true or false, not {v}"))),
    }
}

fn open(s: &mut Session, p: &Json) -> Result<Json> {
    let recalculate = open_flag(p, "recalculate")?;
    // Locked read-only is read-only: it can't go with `readOnly: false`.
    let lock_read_only = open_flag(p, "lockReadOnly")?;
    let read_only = match (open_flag(p, "readOnly")?, lock_read_only) {
        (Some(false), Some(true)) => {
            return Err(bad("file.open", "`lockReadOnly: true` opens the workbook read-only; it can't go with `readOnly: false`"));
        }
        (read_only, lock) => read_only.or(lock),
    };
    let (name, bytes) = if let Some(b) = str_param(p, "base64") {
        (str_param(p, "name").unwrap_or("Book.xlsx").to_string(), crate::io::base64_decode(b).ok_or_else(|| bad("file.open", "invalid base64"))?)
    } else if let Some(path) = str_param(p, "path") {
        // Already open (the same file, however its path is spelled)? Switch to it.
        if let Some(i) = s.documents().iter().position(|d| d.path.as_deref().is_some_and(|own| crate::io::same_file(own, path))) {
            return switch_to_open(s, i, recalculate, read_only, lock_read_only);
        }
        (path.to_string(), crate::io::read_file(path)?)
    } else {
        // How to open a file needs the file: no dialog asks for it on a programmatic call, and a
        // file picked in the browser arrives later on its own, so these would be dropped.
        if let Some(key) = ["recalculate", "readOnly", "lockReadOnly"].into_iter().find(|k| p.get(*k).is_some_and(|v| !v.is_null())) {
            return Err(bad("file.open", format!("`{key}` needs the file to open: give `path`, or `name` and `base64`")));
        }
        s.ui_requests.push(crate::UiRequest::Dialog("open".into(), json!({})));
        return ok();
    };
    let encrypted = gridcraft_xlsx::sniff(&bytes) == gridcraft_xlsx::Format::Encrypted;
    let (mut wb, warnings) = crate::io::open_bytes_with_password(&name, &bytes, str_param(p, "password"))?;
    wb.source_dir = str_param(p, "path").and_then(crate::io::source_dir);
    let imported =
        matches!(crate::io::FileKind::from_path(&name), Some(crate::io::FileKind::Ods | crate::io::FileKind::Xlsb | crate::io::FileKind::Xls))
            || matches!(gridcraft_xlsx::sniff(&bytes), gridcraft_xlsx::Format::Ods | gridcraft_xlsx::Format::Xlsb | gridcraft_xlsx::Format::Xls);
    let title = if imported {
        format!("{} (imported).xlsx", std::path::Path::new(&name).file_stem().and_then(|n| n.to_str()).unwrap_or("Book"))
    } else {
        std::path::Path::new(&name).file_name().and_then(|n| n.to_str()).unwrap_or("Book").to_string()
    };
    // An import has no save target: ordinary Save must ask for a new destination. Neither does a
    // workbook decrypted with its password, so Save never replaces the encrypted original with
    // an unprotected copy.
    let path = str_param(p, "path").filter(|_| !imported && !encrypted).map(str::to_string);
    // Replace an untouched blank Book1 like Excel does.
    if s.documents().len() == 1
        && s.active().is_some_and(|d| d.path.is_none() && !d.is_dirty() && d.undo.is_empty() && d.wb.sheets.iter().all(|sh| sh.cells.is_empty()))
    {
        s.close_document(0);
    }
    // Formulas are calculated again unless the caller asks for the results stored in the file, or
    // the workbook was saved in manual calculation mode and doesn't ask to be calculated on load.
    let recalculate = recalculate.unwrap_or(wb.calc.mode != gridcraft_model::CalcMode::Manual || wb.calc.full_calc_on_load);
    let mut doc = DocState::with_recalc(wb, path, title, recalculate);
    doc.read_only = read_only.unwrap_or(false);
    doc.read_only_locked = lock_read_only.unwrap_or(false);
    if crate::io::FileKind::from_path(&name).is_none() && gridcraft_xlsx::sniff(&bytes) == gridcraft_xlsx::Format::Xlsx {
        doc.content_kind = Some(crate::io::FileKind::Xlsx);
    }
    let i = s.add_document(doc);
    // The decrypted-workbook notice is for the info bar, not a message box.
    let shown: Vec<&str> = warnings.iter().map(String::as_str).filter(|w| *w != crate::io::DECRYPTED_NOTICE).collect();
    if !shown.is_empty() {
        let mut message = shown.iter().take(5).copied().collect::<Vec<_>>().join("\n\n");
        if shown.len() > 5 {
            message.push_str(&format!("\n\n{} additional import warnings.", shown.len() - 5));
        }
        s.ui_requests.push(crate::UiRequest::Message(message));
    }
    let d = s.doc()?;
    Ok(json!({
        "index": i,
        "title": d.display_title(),
        "sheets": d.wb.sheets.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
        "warnings": warnings,
        "recalculated": recalculate,
        "readOnly": d.read_only,
        "lockReadOnly": d.read_only_locked,
    }))
}

/// `file.open` of a file that is already open (document `i`): switches to it, as it is, without
/// reading the file again, so `recalculate` doesn't apply (`recalculateIgnored`). How it is open
/// read-only only ever tightens: asking for read-only while it is open for editing is refused (the
/// caller would get a workbook anything can change), `lockReadOnly` locks a read-only workbook in
/// every window on it, and asking for less than it is (`readOnly: false`, `lockReadOnly: false`)
/// leaves it as it is (`readOnlyIgnored`).
fn switch_to_open(s: &mut Session, i: usize, recalculate: Option<bool>, read_only: Option<bool>, lock: Option<bool>) -> Result<Json> {
    let d = s.documents().get(i).ok_or(EngineError::NoDocument)?;
    if read_only == Some(true) && !d.read_only {
        return Err(bad("file.open", format!("{} is already open for editing; close it to open it read-only", d.display_title())));
    }
    let ignored = (read_only == Some(false) && d.read_only) || (lock == Some(false) && d.read_only_locked);
    let book = d.book;
    if lock == Some(true) {
        for w in s.windows_mut(book).filter(|w| w.read_only) {
            w.read_only_locked = true;
        }
    }
    s.set_active(i);
    let d = s.doc()?;
    let mut r = json!({"index": i, "alreadyOpen": true, "readOnly": d.read_only, "lockReadOnly": d.read_only_locked});
    if recalculate.is_some() {
        r["recalculateIgnored"] = json!(true);
    }
    if ignored {
        r["readOnlyIgnored"] = json!(true);
    }
    Ok(r)
}

/// Why Enable Editing is refused for a workbook opened read-only for good (`lockReadOnly`).
const READ_ONLY_LOCKED: &str = "the workbook was opened read-only and can't be switched to editing";

/// Enable Editing is refused only for a workbook opened read-only for good. On a workbook that
/// isn't read-only it does nothing, so a journal or recorded script that enabled editing replays
/// where the workbook was opened for editing.
fn can_enable_editing(s: &Session) -> std::result::Result<(), String> {
    has_doc(s)?;
    if s.active().is_some_and(|d| d.read_only_locked) { Err(READ_ONLY_LOCKED.into()) } else { Ok(()) }
}

fn enable_editing(s: &mut Session, _: &Json) -> Result<Json> {
    let d = s.doc()?;
    if !d.read_only {
        return Ok(json!({"readOnly": false, "alreadyEditable": true}));
    }
    // Every window on the workbook: they are one workbook, and one left read-only would keep the
    // others from saving to its file.
    let book = d.book;
    let mut windows = 0;
    for w in s.windows_mut(book).filter(|w| !w.read_only_locked) {
        w.read_only = false;
        windows += 1;
    }
    Ok(json!({"readOnly": false, "windows": windows}))
}

/// A workbook open read-only never has its file written over: saving or exporting to that file,
/// from it or from any other workbook, is refused, so it can only be saved as a copy elsewhere.
/// [`crate::io::write_file`] writes [`crate::io::temp_path`] first and renames it over the target,
/// so a target whose temporary file is a read-only workbook's file is refused too. Hosts that
/// write files themselves check here as well (the control channel's screenshots).
pub fn refuse_read_only_file(s: &Session, id: &str, path: &str) -> Result<()> {
    let temp = crate::io::temp_path(path);
    let owner = s.documents().iter().filter(|d| d.read_only).find_map(|d| {
        let own = d.path.as_deref()?;
        if crate::io::same_file(own, path) { Some((d, false)) } else { crate::io::same_file(own, &temp).then_some((d, true)) }
    });
    let Some((d, through_temp)) = owner else { return Ok(()) };
    let why = if through_temp {
        format!("{} is open read-only, and saving to {path} writes {temp} first; save under another name", d.display_title())
    } else if s.active().is_some_and(|a| a.uid == d.uid) {
        format!("{READ_ONLY}; save a copy under another name")
    } else {
        format!("{} is open read-only; save under another name", d.display_title())
    };
    Err(EngineError::Disabled(id.into(), why))
}

fn save(s: &mut Session, p: &Json, id: &str) -> Result<Json> {
    let path = match str_param(p, "path").map(str::to_string).or_else(|| s.active().and_then(|d| d.path.clone())) {
        Some(p) => p,
        None => {
            s.ui_requests.push(crate::UiRequest::Dialog("saveAs".into(), json!({})));
            return ok();
        }
    };
    refuse_read_only_file(s, id, &path)?;
    // A PDF is a print of the sheet (Page Setup applies), not a copy of the workbook: the
    // document keeps its file and its unsaved changes.
    if std::path::Path::new(&path).extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
        return super::print::export_pdf(s, &json!({"path": path}));
    }
    let d = s.doc_mut()?;
    // The document's own file keeps the format its content had when opened; any other path
    // goes by its extension. Relative paths to linked workbooks are rewritten for the new folder.
    let kind = crate::io::FileKind::from_path(&path).or(d.content_kind.filter(|_| d.path.as_deref() == Some(path.as_str())));
    let rebased = crate::io::source_dir(&path).and_then(|f| gridcraft_xlsx::rebase_links(&d.wb, &f));
    let wb = rebased.as_ref().unwrap_or(&d.wb);
    let bytes = match kind {
        Some(kind) => crate::io::save_bytes_as(wb, &path, kind)?,
        None => crate::io::save_bytes(wb, &path)?,
    };
    crate::io::write_file(&path, &bytes)?;
    // Only a full-fidelity format clears the dirty flag and becomes the document's file. A
    // read-only workbook only saves copies: it stays on its own file, where opening that file
    // again finds it, read-only, and the copy is a file like any other.
    if !d.read_only && matches!(kind, Some(crate::io::FileKind::Xlsx | crate::io::FileKind::Json)) {
        d.saved = d.wb.clone();
        d.path = Some(path.clone());
    }
    Ok(json!({"path": path, "bytes": bytes.len()}))
}

fn save_as(s: &mut Session, p: &Json) -> Result<Json> {
    if str_param(p, "path").is_none() {
        s.ui_requests.push(crate::UiRequest::Dialog("saveAs".into(), json!({})));
        return ok();
    }
    save(s, p, "file.saveAs")
}

fn save_bytes(s: &mut Session, p: &Json) -> Result<Json> {
    let fmt = str_param(p, "format").unwrap_or("xlsx");
    let d = s.doc()?;
    let bytes = crate::io::save_bytes(&d.wb, &format!("x.{fmt}"))?;
    let name = format!("{}.{fmt}", std::path::Path::new(&d.display_title()).file_stem().and_then(|x| x.to_str()).unwrap_or("Book"));
    Ok(json!({"name": name, "base64": crate::io::base64_encode(&bytes), "bytes": bytes.len()}))
}

fn close(s: &mut Session, p: &Json) -> Result<Json> {
    let force = bool_param(p, "force").unwrap_or(false);
    let d = s.doc()?;
    if d.is_dirty() && !force {
        s.ui_requests.push(crate::UiRequest::Dialog("saveChanges".into(), json!({"title": d.display_title()})));
        return Ok(json!({"closed": false, "dirty": true}));
    }
    let i = s.active_index();
    s.close_document(i);
    Ok(json!({"closed": true}))
}

fn export(s: &mut Session, p: &Json, ext: &str) -> Result<Json> {
    let path = str_param(p, "path").ok_or_else(|| bad("file.export", "missing `path`"))?.to_string();
    let d = s.doc()?;
    let mut wb = (*d.wb).clone();
    if p.get("sheet").is_some() {
        wb.active_sheet = target_sheet(s, p)?;
    }
    let target = if path.ends_with(&format!(".{ext}")) { path.clone() } else { format!("{path}.{ext}") };
    refuse_read_only_file(s, if ext == "csv" { "file.exportCsv" } else { "file.exportHtml" }, &target)?;
    let bytes = crate::io::save_bytes(&wb, &target)?;
    crate::io::write_file(&target, &bytes)?;
    Ok(json!({"path": target, "bytes": bytes.len()}))
}

fn properties(s: &mut Session, p: &Json) -> Result<Json> {
    edit(s, |cx| {
        let pr = &mut cx.wb.props;
        for (k, slot) in [
            ("title", &mut pr.title),
            ("subject", &mut pr.subject),
            ("author", &mut pr.author),
            ("company", &mut pr.company),
            ("keywords", &mut pr.keywords),
            ("description", &mut pr.description),
        ] {
            if let Some(v) = str_param(p, k) {
                *slot = v.to_string();
            }
        }
        Ok(serde_json::to_value(&cx.wb.props).unwrap_or_default())
    })
}

/// File › Options: reports the workbook-level settings and asks the interface to show its Options
/// page. Nothing changes here; every setting has its own command.
fn options(s: &mut Session, _: &Json) -> Result<Json> {
    s.ui_requests.push(crate::UiRequest::Dialog("options".into(), json!({})));
    let calculation = s.active().map(|d| {
        let c = &d.wb.calc;
        let mode = match c.mode {
            gridcraft_model::CalcMode::Automatic => "automatic",
            gridcraft_model::CalcMode::AutomaticExceptTables => "automaticExceptTables",
            gridcraft_model::CalcMode::Manual => "manual",
        };
        json!({"mode": mode, "iterative": c.iterative, "maxIterations": c.max_iterations, "maxChange": c.max_change})
    });
    Ok(json!({"calculation": calculation}))
}

fn activate_window(s: &mut Session, p: &Json) -> Result<Json> {
    let i = u32_param(p, "index").ok_or_else(|| bad("window.activate", "missing `index`"))? as usize;
    if i >= s.documents().len() {
        return Err(bad("window.activate", "no such window"));
    }
    s.set_active(i);
    ok()
}
