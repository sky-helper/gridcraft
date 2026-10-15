//! Synthetic partial fonts: a rendering fallback is not an explicit XLSX font face.
use gridcraft_core::CellRef;
use gridcraft_model::{Color, Fill, StyleId};

use crate::{package::Package, read_xlsx, write_xlsx};

fn fixture() -> Vec<u8> {
    super::minimal(
        r#"<sheetData><row r="1"><c r="A1" s="1" t="inlineStr"><is><t>Header</t></is></c><c r="B1" s="2" t="inlineStr"><is><t>Explicit</t></is></c></row></sheetData>"#,
        &[(
            "xl/styles.xml",
            r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="3"><font><name val="Calibri"/><sz val="11"/></font><font><b/><color rgb="FFFFFFFF"/></font><font><name val="Calibri"/><sz val="11"/><b/><color rgb="FFFFFFFF"/></font></fonts><cellXfs count="3"><xf fontId="0"/><xf fontId="1"/><xf fontId="2"/></cellXfs></styleSheet>"#,
        )],
        r#"<Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
        "",
    )
}

#[test]
fn saving_a_partial_font_does_not_add_a_face_or_merge_it_with_an_explicit_font() {
    let mut wb = read_xlsx(&fixture()).unwrap().0;
    let a = CellRef::parse("A1").unwrap();
    let b = CellRef::parse("B1").unwrap();
    let style = wb.sheets[0].cell(a).unwrap().style;
    let explicit = wb.sheets[0].cell(b).unwrap().style;
    assert_ne!(style, explicit);
    assert!(wb.styles.get(style).font.implicit_name);
    assert!(!wb.styles.get(explicit).font.implicit_name);
    // An unrelated format edit must not make the fallback explicit either.
    let changed = wb.styles.derive(style, |s| s.fill = Fill::solid(Color::Rgb(0x112233)));
    std::sync::Arc::make_mut(&mut wb.sheets[0]).set_style(a, changed);
    for _ in 0..2 {
        let bytes = write_xlsx(&wb).unwrap();
        let mut package = Package::open(&bytes).unwrap();
        let xml = package.read_xml("xl/styles.xml").unwrap().unwrap();
        let fonts = xml.child("fonts").unwrap();
        assert_eq!(fonts.kids("font").filter(|f| f.child("b").is_some() && f.child("name").is_none()).count(), 1);
        assert_eq!(fonts.kids("font").filter(|f| f.child("b").is_some() && f.child_val("name") == Some("Calibri")).count(), 1);
        wb = read_xlsx(&bytes).unwrap().0;
        assert!(wb.styles.get(wb.sheets[0].cell(a).unwrap().style).font.implicit_name);
    }
}

#[test]
fn a_changed_face_is_written_and_new_workbooks_keep_explicit_default_fonts() {
    let mut wb = read_xlsx(&fixture()).unwrap().0;
    let a = CellRef::parse("A1").unwrap();
    let style = wb.sheets[0].cell(a).unwrap().style;
    let changed = wb.styles.derive(style, |s| s.font.name = "Example Sans".into());
    std::sync::Arc::make_mut(&mut wb.sheets[0]).set_style(a, changed);
    let back = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap().0;
    let font = &back.styles.get(back.sheets[0].cell(a).unwrap().style).font;
    assert_eq!(font.name, "Example Sans");
    assert!(!font.implicit_name);
    assert!(!gridcraft_model::Workbook::new().styles.get(StyleId::DEFAULT).font.implicit_name);
}
