//! 旧式命名目标的**字典形态**写端测试（ISO 32000-1 12.3.2.3 第 2 段 +
//! Table 28）：目录 `/Dests` 的条目值除 Table 151 数组外，还可为
//! `<< /D <目标数组> >>`。
//!
//! 覆盖两种形态混排进同一个间接字典时的写出、跨形态重名与名字/页码/操作数
//! 的零字节拒绝、与数组形态**共用**的 PDF 1.1 版本门（字典形态不需要更高
//! 版本）、与 1.2+ `/Names /Dests` 名称树同名共存时**不**判重名，以及写出后
//! 用本库 reader 重新解析、两条通道各自的取值与优先级。

use std::sync::atomic::{AtomicU64, Ordering};

use super::*;
use crate::core::reader::navigation::Destination;
use crate::core::types::{PdfVersion, Value};
use crate::core::writer::WriteError;

/// 间接对象 `N 0 obj` 的正文；要求未启用对象流，便于按对象号取回正文。
fn indirect_object(rendered: &str, num: &str) -> String {
    let marker = format!("{num} 0 obj");
    let at = rendered
        .find(&marker)
        .unwrap_or_else(|| panic!("object {num} should be written as an indirect object"));
    let start = at + marker.len();
    let end = start + rendered[start..].find("endobj").expect("endobj marker");
    rendered[start..end].trim().to_string()
}

/// `/Type /Catalog` 所在对象的正文（含首尾 `<<`/`>>`）；要求未启用对象流。
fn catalog_text(rendered: &str) -> String {
    let at = rendered
        .find("/Type /Catalog")
        .expect("catalog object should be written inline");
    let start = rendered[..at].rfind("obj").expect("object header") + 3;
    let end = at + rendered[at..].find("endobj").expect("endobj marker");
    rendered[start..end].trim().to_string()
}

/// 目录 `/Dests` 引用的对象号（Table 28「shall be an indirect reference」）。
fn catalog_dests_object_number(rendered: &str) -> String {
    let catalog = catalog_text(rendered);
    let marker = "/Dests ";
    let at = catalog
        .find(marker)
        .unwrap_or_else(|| panic!("catalog should contain /Dests: {catalog}"));
    let mut tokens = catalog[at + marker.len()..].split_whitespace();
    let num = tokens.next().unwrap_or_default();
    let generation = tokens.next().unwrap_or_default();
    let reference = tokens.next().unwrap_or_default();
    assert!(
        !num.is_empty() && num.bytes().all(|byte| byte.is_ascii_digit()),
        "catalog /Dests shall reference an object number: {catalog}"
    );
    assert_eq!(
        (generation, reference),
        ("0", "R"),
        "catalog /Dests shall be an indirect reference: {catalog}"
    );
    num.to_string()
}

/// 目录 `/Dests` 条目是否为间接引用（`N 0 R`），而非内联字典。
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

/// 写出后重新解析，走 reader 的懒加载对象路径（真·往返）。
fn build_and_parse(b: &mut PdfBuilder) -> Document {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut out = Vec::new();
    b.build(&mut out).expect("build");
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    std::fs::create_dir_all(&dir).expect("create test-tmp");
    let path = dir.join(format!(
        "pdf-engine-legacy-dests-dict-{}-{}.pdf",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::write(&path, &out).expect("write temp pdf");
    let file = std::fs::File::open(&path).expect("open temp pdf");
    let mut parser = crate::core::reader::Parser::new(file).expect("parser");
    let doc = parser.parse_document().expect("parse");
    let _ = std::fs::remove_file(&path);
    doc
}

/// 2 页文档：`intro` 走数组形态，`zoom`/`plain` 走 `/D` 字典形态。
///
/// 注册顺序故意打乱字母序，让键序断言有意义（'i' < 'p' < 'z'）。
fn builder_with_both_forms() -> PdfBuilder {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    b.page(PageSize::A4);
    b.page(PageSize::A4);
    b.legacy_named_destination("intro", Destination::Fit { page: 0 });
    b.legacy_named_destination_dict(
        "zoom",
        Destination::Xyz {
            page: 1,
            left: Some(10.0),
            top: None,
            zoom: Some(2.0),
        },
    );
    b.legacy_named_destination_dict("plain", Destination::FitB { page: 1 });
    b
}

#[test]
fn legacy_named_destination_dict_writes_a_d_entry_in_the_indirect_dests_dict() {
    let mut b = builder_with_both_forms();
    let s = render(&mut b);

    // Table 28：字典形态不改变通道的载体形态，目录条目仍是间接引用。
    assert!(dests_entry_is_reference(&s), "{s}");
    assert!(!s.contains("/Dests <<"), "{s}");
    let num = catalog_dests_object_number(&s);
    let dests = indirect_object(&s, &num);

    // `/D` 的值仍是 Table 151 数组：第 2 页是 5 号对象，缺省操作数写 null。
    assert!(
        dests.contains("/zoom << /D [5 0 R /XYZ 10.0 null 2.0] >>"),
        "{dests}"
    );
    assert!(dests.contains("/plain << /D [5 0 R /FitB] >>"), "{dests}");
    // 数组形态条目原样写裸数组，不被误包成字典。
    assert!(dests.contains("/intro [3 0 R /Fit]"), "{dests}");
}

#[test]
fn legacy_named_destination_dict_keys_share_one_name_byte_order_with_array_form() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    b.page(PageSize::A4);
    b.legacy_named_destination_dict("gamma", Destination::Fit { page: 0 });
    b.legacy_named_destination("Alpha", Destination::FitB { page: 0 });
    b.legacy_named_destination_dict("beta", Destination::Fit { page: 0 });
    let s = render(&mut b);

    let num = catalog_dests_object_number(&s);
    let dests = indirect_object(&s, &num);
    // 键仍是名字对象（无 UTF-16BE 文本串编码），且跨形态同序：
    // 'A'(0x41) < 'b'(0x62) < 'g'(0x67)。
    assert!(!dests.contains("<FEFF"), "{dests}");
    let alpha = dests.find("/Alpha").expect("/Alpha");
    let beta = dests.find("/beta").expect("/beta");
    let gamma = dests.find("/gamma").expect("/gamma");
    assert!(alpha < beta && beta < gamma, "{dests}");
}

#[test]
fn legacy_named_destination_dict_stays_indirect_under_object_streams() {
    // 1.7 默认启用对象流：目录被压进 ObjStm，`/Dests` 仍是引用、`/D` 仍在。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.legacy_named_destination_dict("intro", Destination::Fit { page: 0 });
    let s = render(&mut b);
    assert!(dests_entry_is_reference(&s), "{s}");
    assert!(s.contains("/intro << /D [3 0 R /Fit] >>"), "{s}");
}

#[test]
fn legacy_named_destination_dict_uses_the_same_pdf_1_1_gate() {
    // 12.3.2.3 把两种形态写在同一句里、同属 PDF 1.1 通道，故低于 1.1 拒绝、
    // 1.1 即可写出字典形态（无更高的新门）。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_0);
    b.page(PageSize::A4);
    b.legacy_named_destination_dict("intro", Destination::Fit { page: 0 });
    let (error, len) = build_error(&mut b);
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert_eq!(len, 0);

    let mut b = PdfBuilder::with_version(PdfVersion::V1_1);
    b.page(PageSize::A4);
    b.legacy_named_destination_dict("intro", Destination::Fit { page: 0 });
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.1 dict form should build");
    let s = String::from_utf8_lossy(&out).into_owned();
    assert!(s.contains("/intro << /D [3 0 R /Fit] >>"), "{s}");
}

#[test]
fn legacy_named_destination_dict_rejects_duplicate_names_within_the_channel() {
    // 两个字典形态同名。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.legacy_named_destination_dict("dup", Destination::Fit { page: 0 });
    b.legacy_named_destination_dict("dup", Destination::FitB { page: 0 });
    let (error, len) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(len, 0);

    // 跨形态同名同样重名：两种形态写进的是**同一个** `/Dests` 字典。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.legacy_named_destination("dup", Destination::Fit { page: 0 });
    b.legacy_named_destination_dict("dup", Destination::FitB { page: 0 });
    let (error, len) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(len, 0);
}

#[test]
fn legacy_named_destination_dict_rejects_invalid_names_without_output() {
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
        b.legacy_named_destination_dict(name, Destination::Fit { page: 0 });
        let (error, len) = build_error(&mut b);
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "{what}: {error}"
        );
        assert_eq!(len, 0, "{what}");
    }
}

#[test]
fn legacy_named_destination_dict_rejects_bad_targets_without_output() {
    // 页码越界。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.legacy_named_destination_dict("bad", Destination::Fit { page: 1 });
    let (error, len) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(len, 0);

    // 非有限操作数（与数组形态、名称树通道共用同一套目标校验）。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.legacy_named_destination_dict(
        "nan",
        Destination::FitH {
            page: 0,
            top: Some(f64::NAN),
        },
    );
    let (error, len) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(len, 0);

    // 非正的 `/XYZ` zoom 同理（Table 151 要求 0 之外）。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.legacy_named_destination_dict(
        "zero",
        Destination::Xyz {
            page: 0,
            left: None,
            top: None,
            zoom: Some(0.0),
        },
    );
    let (error, len) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(len, 0);
}

#[test]
fn legacy_named_destination_dict_coexists_with_the_name_tree_channel() {
    // NOTE 3「A document that supports PDF 1.2 can contain both types」：
    // 跨通道同名不是重名，写端不加规范未要求的拒绝。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    b.page(PageSize::A4);
    b.legacy_named_destination_dict("intro", Destination::Fit { page: 0 });
    b.named_destination("intro", Destination::FitB { page: 0 });
    let s = render(&mut b);

    assert!(s.contains("/Dests <<"), "{s}");
    assert!(catalog_text(&s).contains("/Names <<"), "{s}");
    let num = catalog_dests_object_number(&s);
    let dests = indirect_object(&s, &num);
    assert!(dests.contains("/intro << /D [3 0 R /Fit] >>"), "{dests}");
}

#[test]
fn legacy_named_dests_dict_roundtrips_both_forms_through_the_parser() {
    let mut b = builder_with_both_forms();
    let doc = build_and_parse(&mut b);

    // 字典形态：`/zoom` 的目的地在 `/D` 之下，reader 解析成同一套类型化模型。
    assert_eq!(
        doc.named_destination("zoom").unwrap(),
        Some(Destination::Xyz {
            page: 1,
            left: Some(10.0),
            top: None,
            zoom: Some(2.0),
        })
    );
    assert_eq!(
        doc.named_destination("plain").unwrap(),
        Some(Destination::FitB { page: 1 })
    );
    // 同字典里的数组形态条目照旧解析，两形态互不干扰。
    assert_eq!(
        doc.named_destination("intro").unwrap(),
        Some(Destination::Fit { page: 0 })
    );
    assert_eq!(doc.named_destination("absent").unwrap(), None);
}

#[test]
fn legacy_named_dests_dict_yields_to_the_name_tree_channel_when_reparsed() {
    // 读端优先级（库策略，依据 NOTE 3 末尾对写者的 should）：名称树优先于
    // 目录旧式 `/Dests`，两通道同名时取名称树的值。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.page(PageSize::A4);
    b.legacy_named_destination_dict("shared", Destination::Fit { page: 0 });
    b.named_destination(
        "shared",
        Destination::FitR {
            page: 1,
            rect: [1.0, 2.0, 3.0, 4.0],
        },
    );
    b.legacy_named_destination_dict(
        "legacy-only",
        Destination::FitBV {
            page: 1,
            left: Some(42.0),
        },
    );
    let doc = build_and_parse(&mut b);

    assert_eq!(
        doc.named_destination("shared").unwrap(),
        Some(Destination::FitR {
            page: 1,
            rect: [1.0, 2.0, 3.0, 4.0],
        })
    );
    // 只登记在旧式字典（字典形态）的名字仍由 `/Dests` 解析出来。
    assert_eq!(
        doc.named_destination("legacy-only").unwrap(),
        Some(Destination::FitBV {
            page: 1,
            left: Some(42.0),
        })
    );

    // 目录条目确为引用、旧式字典里不含名称树专有的名字。
    let catalog = doc
        .trailer
        .dict
        .get("Root")
        .and_then(Value::as_ref)
        .and_then(|(num, _)| doc.get_object(num).ok().flatten())
        .and_then(|object| object.value.as_dict().cloned())
        .expect("catalog dictionary");
    assert!(
        catalog.get("Dests").and_then(Value::as_ref).is_some(),
        "catalog /Dests shall be an indirect reference: {catalog:?}"
    );
    let dests = catalog
        .get("Dests")
        .and_then(Value::as_ref)
        .and_then(|(num, _)| doc.get_object(num).ok().flatten())
        .and_then(|object| object.value.as_dict().cloned())
        .expect("legacy /Dests dictionary object");
    assert!(dests.get("legacy-only").is_some(), "{dests:?}");
    assert!(dests.get("shared").is_some(), "{dests:?}");
}

// ---- go-to 动作形态（12.3.2.3 NOTE 2）：写出、往返、跨形态校验、版本门 ----

/// `/Dests` 字典条目是否为 `<< /A << /S /GoTo /D … >> >>` go-to 动作形态。
fn go_to_builder_with_forms() -> PdfBuilder {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    b.page(PageSize::A4);
    b.page(PageSize::A4);
    // 三种形态混排进同一个间接 `/Dests` 字典（'a' < 'b' < 'c'）。
    b.legacy_named_destination("alpha", Destination::Fit { page: 0 });
    b.legacy_named_destination_dict(
        "beta",
        Destination::Xyz {
            page: 1,
            left: Some(5.0),
            top: None,
            zoom: Some(3.0),
        },
    );
    b.legacy_named_destination_go_to("gamma", Destination::FitB { page: 1 });
    b
}

#[test]
fn legacy_named_destination_go_to_writes_action_dict_in_indirect_dests() {
    let mut b = go_to_builder_with_forms();
    let s = render(&mut b);

    // Table 28：go-to 形态不改变通道载体，目录条目仍是间接引用。
    assert!(dests_entry_is_reference(&s), "{s}");
    let num = catalog_dests_object_number(&s);
    let dests = indirect_object(&s, &num);

    // 外层 `<< /A << … >> >>`，内层动作字典是排序 `BTreeMap`，故 `/D` 先于
    // `/S`；`/D` 数组的页面引用与另两形态走同一条装配路径（第 2 页是 5 号对象）。
    assert!(
        dests.contains("/gamma << /A << /D [5 0 R /FitB] /S /GoTo >> >>"),
        "{dests}"
    );
    // 同字典里数组形态与 `/D` 字典形态原样共存，互不干扰。
    assert!(dests.contains("/alpha [3 0 R /Fit]"), "{dests}");
    assert!(
        dests.contains("/beta << /D [5 0 R /XYZ 5.0 null 3.0] >>"),
        "{dests}"
    );
}

#[test]
fn legacy_named_dests_go_to_roundtrips_both_forms_through_the_parser() {
    // 实际跑过的写出→重解析往返：go-to 形态条目经 reader 的 `/A` 分支解析后，
    // `Document::named_destination` 返回值与写入时完全一致。
    let mut b = go_to_builder_with_forms();
    let doc = build_and_parse(&mut b);

    assert_eq!(
        doc.named_destination("gamma").unwrap(),
        Some(Destination::FitB { page: 1 })
    );
    // 与数组形态、`/D` 字典形态同处一个 `/Dests` 字典，三形态各自解析正确。
    assert_eq!(
        doc.named_destination("alpha").unwrap(),
        Some(Destination::Fit { page: 0 })
    );
    assert_eq!(
        doc.named_destination("beta").unwrap(),
        Some(Destination::Xyz {
            page: 1,
            left: Some(5.0),
            top: None,
            zoom: Some(3.0),
        })
    );
    assert_eq!(doc.named_destination("absent").unwrap(), None);
}

#[test]
fn legacy_named_destination_go_to_shares_name_validation_and_duplicate_rejection() {
    // 与数组/字典形态跨形态重名 → InvalidObject 零字节（共用同一套校验）。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.legacy_named_destination("dup", Destination::Fit { page: 0 });
    b.legacy_named_destination_go_to("dup", Destination::FitB { page: 0 });
    let (error, len) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(len, 0);

    // 两个 go-to 形态同名。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.legacy_named_destination_go_to("dup", Destination::Fit { page: 0 });
    b.legacy_named_destination_go_to("dup", Destination::FitB { page: 0 });
    let (error, len) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(len, 0);

    // 非法名字（非可打印 ASCII）与越界页码同样零字节拒绝。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.legacy_named_destination_go_to("附录", Destination::Fit { page: 0 });
    let (error, len) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(len, 0);

    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.legacy_named_destination_go_to("bad", Destination::Fit { page: 1 });
    let (error, len) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(len, 0);
}

#[test]
fn legacy_named_destination_go_to_uses_the_same_pdf_1_1_gate() {
    // go-to 形态仍属 PDF 1.1 通道：低于 1.1 拒绝、1.1 即可写出（无新门）。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_0);
    b.page(PageSize::A4);
    b.legacy_named_destination_go_to("intro", Destination::Fit { page: 0 });
    let (error, len) = build_error(&mut b);
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert_eq!(len, 0);

    let mut b = PdfBuilder::with_version(PdfVersion::V1_1);
    b.page(PageSize::A4);
    b.legacy_named_destination_go_to("intro", Destination::Fit { page: 0 });
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.1 go-to form should build");
    let s = String::from_utf8_lossy(&out).into_owned();
    assert!(
        s.contains("/intro << /A << /D [3 0 R /Fit] /S /GoTo >> >>"),
        "{s}"
    );
}
