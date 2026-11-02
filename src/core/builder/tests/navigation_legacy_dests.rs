//! 旧式命名目标写端测试（ISO 32000-1 12.3.2.3 的 PDF 1.1 形态：目录
//! `/Dests` 字典）。
//!
//! 覆盖 Table 28 的「shall be an indirect reference」形态约束、Table 151
//! 目标数组的值装配、名字对象键（非文本串）的字节序排列，以及空名、非
//! ASCII 名、重名、页码越界与非有限操作数的零字节拒绝、1.1 版本门控，
//! 以及与 1.2+ `/Names /Dests` 名称树通道共存。
//!
//! 形态类断言用 PDF 1.4（无对象流）构建，此时每个对象都以 `N 0 obj`
//! 明文写出，便于按对象号取回正文；1.7 仅额外断言目录条目仍为引用。

use super::*;
use crate::core::reader::navigation::Destination;
use crate::core::types::PdfVersion;
use crate::core::writer::WriteError;

/// `/Type /Catalog` 所在对象的正文（含首尾 `<<`/`>>`）；要求未启用对象流。
fn catalog_text(rendered: &str) -> String {
    let at = rendered
        .find("/Type /Catalog")
        .expect("catalog object should be written inline");
    let start = rendered[..at].rfind("obj").expect("object header") + 3;
    let end = at + rendered[at..].find("endobj").expect("endobj marker");
    rendered[start..end].trim().to_string()
}

/// 目录条目 `/{key}` 的值文本，截到下一个键或字典收尾之前。
fn catalog_entry_value(rendered: &str, key: &str) -> String {
    let catalog = catalog_text(rendered);
    let marker = format!("/{key} ");
    let at = catalog
        .find(&marker)
        .unwrap_or_else(|| panic!("catalog should contain {marker}: {catalog}"));
    let rest = &catalog[at + marker.len()..];
    let end = rest
        .find(" /")
        .or_else(|| rest.find(" >>"))
        .unwrap_or(rest.len());
    rest[..end].trim().to_string()
}

/// 间接对象 `N 0 obj` 的正文；要求未启用对象流。
fn indirect_object(rendered: &str, num: &str) -> String {
    let marker = format!("{num} 0 obj");
    let at = rendered
        .find(&marker)
        .unwrap_or_else(|| panic!("object {num} should be written as an indirect object"));
    let start = at + marker.len();
    let end = start + rendered[start..].find("endobj").expect("endobj marker");
    rendered[start..end].trim().to_string()
}

/// 目录 `/Dests` 条目是否为间接引用（`N 0 R`），而非内联字典。
///
/// 对象流把目录字典搬进流里也不影响判定：只看条目后面的三个记号。
fn dests_entry_is_reference(rendered: &str) -> bool {
    let Some(at) = rendered.find("/Dests ") else {
        return false;
    };
    let mut tokens = rendered[at + "/Dests ".len()..].split_whitespace();
    matches!(
        (tokens.next(), tokens.next(), tokens.next()),
        (Some(num), Some("0"), Some("R")) if !num.is_empty() && num.bytes().all(|b| b.is_ascii_digit())
    )
}

/// 构建失败时的错误与已写出的字节数（零字节契约断言）。
fn build_error(builder: &mut PdfBuilder) -> (WriteError, usize) {
    let mut out = Vec::new();
    let error = builder.build(&mut out).expect_err("build should fail");
    (error, out.len())
}

/// 2 页文档 + `intro`（Fit）与 `zoom`（XYZ）两个旧式命名目标。
fn builder_with_legacy_dests() -> PdfBuilder {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    b.page(PageSize::A4);
    b.page(PageSize::A4);
    b.legacy_named_destination("intro", Destination::Fit { page: 0 });
    b.legacy_named_destination(
        "zoom",
        Destination::Xyz {
            page: 1,
            left: Some(10.0),
            top: None,
            zoom: Some(2.0),
        },
    );
    b
}

/// 注册旧式命名目标的闭包：版本门控与形态用例共用。
fn register_legacy(b: &mut PdfBuilder) {
    b.page(PageSize::A4);
    b.legacy_named_destination("intro", Destination::Fit { page: 0 });
}

#[test]
fn legacy_named_destinations_write_an_indirect_dict_referenced_by_catalog() {
    let mut b = builder_with_legacy_dests();
    let s = render(&mut b);

    // Table 28：目录 `/Dests` 必须是间接引用，字典本身另成一个对象。
    assert!(dests_entry_is_reference(&s), "{s}");
    assert!(!s.contains("/Dests <<"), "{s}");
    let entry = catalog_entry_value(&s, "Dests");
    let num = entry.split_whitespace().next().expect("object number");
    let dests = indirect_object(&s, num);
    assert!(
        dests.starts_with("<< /intro") && dests.ends_with(">>"),
        "{dests}"
    );
    assert!(dests.contains("/intro"), "{dests}");
    assert!(dests.contains("/zoom"), "{dests}");
    // 只注册旧式通道时不产生 `/Names` 名称字典。
    assert!(!catalog_text(&s).contains("/Names"), "{s}");
}

#[test]
fn legacy_named_destinations_stay_indirect_under_object_streams() {
    // 1.7 默认启用对象流：目录被压进 ObjStm，`/Dests` 条目仍是间接引用。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    register_legacy(&mut b);
    let s = render(&mut b);
    assert!(dests_entry_is_reference(&s), "{s}");
    assert!(s.contains("/intro [3 0 R /Fit]"), "{s}");
}

#[test]
fn legacy_named_destination_values_are_table_151_arrays() {
    let mut b = builder_with_legacy_dests();
    let s = render(&mut b);

    let num = catalog_entry_value(&s, "Dests")
        .split_whitespace()
        .next()
        .expect("object number")
        .to_string();
    let dests = indirect_object(&s, &num);
    // 首元素是页面对象引用（第 1 页 = 3 0 号、第 2 页 = 5 0 号），
    // 其后是 Table 151 的形态名与操作数；缺省操作数写 null。
    assert!(dests.contains("/intro [3 0 R /Fit]"), "{dests}");
    assert!(
        dests.contains("/zoom [5 0 R /XYZ 10.0 null 2.0]"),
        "{dests}"
    );
}

#[test]
fn legacy_named_destination_keys_are_names_sorted_in_byte_order() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    b.page(PageSize::A4);
    b.legacy_named_destination("gamma", Destination::Fit { page: 0 });
    b.legacy_named_destination("Alpha", Destination::FitB { page: 0 });
    b.legacy_named_destination("beta", Destination::Fit { page: 0 });
    let s = render(&mut b);

    let num = catalog_entry_value(&s, "Dests")
        .split_whitespace()
        .next()
        .expect("object number")
        .to_string();
    let dests = indirect_object(&s, &num);

    // 键是名字对象而非文本串：不出现 UTF-16BE 文本串编码。
    assert!(!dests.contains("<FEFF"), "{dests}");
    // 按名字字节序升序：'A'(0x41) < 'b'(0x62) < 'g'(0x67)。
    let alpha = dests.find("/Alpha").expect("/Alpha");
    let beta = dests.find("/beta").expect("/beta");
    let gamma = dests.find("/gamma").expect("/gamma");
    assert!(alpha < beta && beta < gamma, "{dests}");
}

#[test]
fn legacy_named_destinations_reject_duplicate_names_without_output() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.legacy_named_destination("dup", Destination::Fit { page: 0 });
    b.legacy_named_destination("dup", Destination::FitB { page: 0 });
    let (error, len) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(len, 0);
}

#[test]
fn legacy_named_destinations_reject_invalid_names_without_output() {
    let nul = {
        let mut text = String::from("a");
        text.push('\0');
        text.push('b');
        text
    };
    let cases: [(&str, String); 4] = [
        ("empty name", String::new()),
        ("non ascii", "附录".into()),
        ("nul", nul),
        ("control char", "a\nb".into()),
    ];
    for (what, name) in cases {
        let mut b = PdfBuilder::new();
        b.page(PageSize::A4);
        b.legacy_named_destination(name, Destination::Fit { page: 0 });
        let (error, len) = build_error(&mut b);
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "{what}: {error}"
        );
        assert_eq!(len, 0, "{what}");
    }
}

#[test]
fn legacy_named_destinations_reject_out_of_range_page_without_output() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.legacy_named_destination("bad", Destination::Fit { page: 1 });
    let (error, len) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(len, 0);

    // 非有限操作数同样零字节拒绝（与名称树通道共用目标校验）。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.legacy_named_destination(
        "nan",
        Destination::FitH {
            page: 0,
            top: Some(f64::NAN),
        },
    );
    let (error, len) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(len, 0);
}

#[test]
fn legacy_named_destinations_require_pdf_1_1() {
    // 低于 1.1：UnsupportedVersion 且零字节。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_0);
    register_legacy(&mut b);
    let (error, len) = build_error(&mut b);
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert_eq!(len, 0);

    // 1.1 起可用。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_1);
    register_legacy(&mut b);
    let mut out = Vec::new();
    b.build(&mut out)
        .expect("PDF 1.1 legacy dests should build");
    assert!(!out.is_empty());
}

#[test]
fn legacy_named_destinations_coexist_with_dests_name_tree() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    b.page(PageSize::A4);
    b.legacy_named_destination("intro", Destination::Fit { page: 0 });
    // 同名字在名称树通道再注册一次：跨通道不判重名（规范无此 shall，
    // 读端按名称树优先解析旧式字典）。
    b.named_destination("intro", Destination::FitB { page: 0 });
    b.named_destination("tree-only", Destination::Fit { page: 0 });
    let s = render(&mut b);

    // 名称树通道照旧内联写出 `/Names /Dests << /Names [<…文本串…> […] ] >>`。
    assert!(s.contains("/Dests <<"), "{s}");
    assert!(catalog_text(&s).contains("/Names <<"), "{s}");
    // 旧式通道仍是独立间接对象，且只含自己的名字。
    let entry = catalog_entry_value(&s, "Dests");
    let num = entry.split_whitespace().next().expect("object number");
    let dests = indirect_object(&s, num);
    assert!(dests.contains("/intro"), "{dests}");
    assert!(!dests.contains("tree-only"), "{dests}");
}

#[test]
fn legacy_named_destinations_absent_leaves_catalog_without_dests_entry() {
    // 只注册名称树通道时，目录本身不出现旧式 `/Dests` 条目
    // （`/Dests` 仅作为 `/Names` 的子类别内联出现）。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    b.page(PageSize::A4);
    b.named_destination("intro", Destination::Fit { page: 0 });
    let s = render(&mut b);

    assert!(s.contains("/Dests <<"), "{s}");
    assert!(!dests_entry_is_reference(&s), "{s}");
}
