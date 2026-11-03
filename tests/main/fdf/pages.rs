//! FDF `/Pages`（Table 248/249/250）与 `/Annots`（Table 251）的**集成
//! 往返**测试：手写字节 → 真实 Parser → 类型化视图（`Document::fdf_pages`
//! / `fdf_annots` / `fdf_import_merge`）→ `PdfBuilder::fdf_pages_value` /
//! `fdf_annots_value` 写回原值字段 → `export_fdf` 产出真实字节 → 再经
//! 真实 Parser 重解析，断言四层结构、可选条目缺省不物化（Table 249
//! `/Rename`「Default value: true」、Table 250 `/F` 规范推定）、未建模
//! 原值不丢弃与 Annex J 前缀建议的保真；并验证写端装配前校验对四张表
//! Required 条目（Table 248 `/Templates`、Table 249 `/TRef`、Table 250
//! `/Name`、Table 251 `/Page`）、Table 243 注解类型黑名单与 PDF 1.3 档位
//! 门禁的零字节拒绝。

use super::*;
use pdf_engine::core::builder::PdfBuilder;
use pdf_engine::core::reader::fdf::{FdfDictionary, FdfDocument, FdfPagesIssue};
use pdf_engine::core::writer::WriteError;

/// 全 None 裸 FDF 字典（Table 243 读模型）。
fn dictionary() -> FdfDictionary {
    FdfDictionary {
        source_file: None,
        identifier: None,
        fields: None,
        status: None,
        pages: None,
        encoding: None,
        annots: None,
        differences: None,
        target: None,
        target_frame: None,
        embedded_fdfs: None,
        javascript: None,
        javascript_scripts: None,
        dict: Dict::new(),
    }
}

fn model(fdf: &FdfDictionary) -> FdfDocument {
    FdfDocument {
        catalog_version: None,
        catalog: Dict::new(),
        fdf: fdf.clone(),
    }
}

/// 单间接对象（目录 `/FDF` 为直接字典）的最小 FDF 文件；`header` 决定
/// `%FDF-x.y` 文件头版本，`entries` 是 FDF 字典条目的 PDF 字面量。
fn fdf_file(header: &str, entries: &str) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(format!("%FDF-{header}\n").as_bytes());
    let object_at = buf.len();
    buf.extend_from_slice(format!("1 0 obj\n<< /FDF << {entries} >> >>\nendobj\n").as_bytes());
    let xref_at = buf.len();
    buf.extend_from_slice(b"xref\n0 2\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{object_at:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(b"trailer\n<< /Root 1 0 R /Size 2 >>\n");
    buf.extend_from_slice(format!("startxref\n{xref_at}\n%%EOF\n").as_bytes());
    buf
}

/// 按键序装配一个直接字典值（写端拒绝测试用）。
fn dict_value(pairs: &[(&str, Value)]) -> Value {
    let mut dict = Dict::new();
    for (key, value) in pairs {
        dict = dict.insert(*key, value.clone());
    }
    Value::Dict(dict)
}

fn array(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

/// 两页 × 各一模板、同名根字段 `tax`、第二页 `/Rename false` 的
/// `/Pages` 字面量（Table 248 → 249 → 250 三层 + Table 246 字段）。
const CONFLICTING_PAGES_BODY: &str = "/Pages [ << /Templates [ << /TRef << /Name (Template.A) >> /Fields [ << /T (tax) >> ] >> ] >> << /Templates [ << /TRef << /Name (Template.B) >> /Fields [ << /T (tax) >> ] /Rename false >> ] >> ]";

#[test]
fn fdf_pages_written_from_typed_view_roundtrip_keeps_layers_defaults_and_raw() {
    // 读 → 类型化视图 → fdf_pages_value 写回 → export_fdf → 重解析：
    // Table 248 `/Info` 与未建模 `/CustomKey` 原值不丢；Table 249
    // `/Rename` 缺省 true 不物化（导出文本里只出现显式 false 那一次）；
    // Table 250 `/F` 缺席 = 「页在关联 PDF 文件里」的规范推定（raw
    // L25127–L25128），两条模板各自保持。
    let bytes = fdf_file(
        "1.3",
        "/Pages [ << /Templates [ << /TRef << /Name (Template.A) >> /Fields [ << /T (tax) >> ] >> << /TRef << /Name (Template.B) /F (tpl.fdf) >> /Rename false >> ] /Info << /Author (Adobe) >> /CustomKey 7 >> ]",
    );
    let document = parse_buf(&bytes);
    let view = document
        .fdf_pages()
        .expect("读端宽容，不应报错")
        .expect("/Pages 存在时总有视图");
    assert!(view.issues.is_empty(), "{:?}", view.issues);
    let page = view.page(0).expect("页 0");
    assert_eq!(page.entry("CustomKey"), Some(&Value::Integer(7)));
    assert_eq!(page.templates.len(), 2);
    let first = &page.templates[0];
    assert_eq!(first.name(), Some("Template.A"));
    assert_eq!(first.rename, None, "缺省 true 不物化");
    assert!(first.rename_effective());
    assert_eq!(first.fields.len(), 1);
    assert_eq!(first.fields[0].partial_name, "tax");
    let reference = first.named_page.as_ref().expect("/TRef");
    assert!(reference.resides_in_associated_pdf());
    let second = &page.templates[1];
    assert_eq!(second.rename, Some(false));
    assert_eq!(
        second.named_page.as_ref().expect("/TRef").file,
        Some(Value::String("tpl.fdf".into()))
    );
    let info_before = page.info.clone().expect("/Info 原值");

    let mut fdf = dictionary();
    fdf.pages = Some(PdfBuilder::fdf_pages_value(&view.entries));
    let written = PdfBuilder::new()
        .export_fdf(&model(&fdf), PdfVersion::V1_3)
        .expect("类型化视图应能写回");
    let text = String::from_utf8(written.clone()).expect("ASCII 字节");
    assert_eq!(
        text.matches("/Rename").count(),
        1,
        "只有显式 false 才写出；Default value: true 不物化：{text}"
    );
    let reparsed = parse_buf(&written);
    let out = reparsed
        .fdf_pages()
        .expect("重解析不应报错")
        .expect("导出应带 /Pages");
    assert!(
        out.entries.iter().all(|entry| entry.issues.is_empty()
            && entry
                .templates
                .iter()
                .all(|template| template.issues.is_empty())),
        "合法往返不应产生诊断"
    );
    let out_page = out.page(0).expect("页 0");
    assert_eq!(out_page.entry("CustomKey"), Some(&Value::Integer(7)));
    assert_eq!(out_page.info.as_ref(), Some(&info_before), "/Info 原样往返");
    assert_eq!(out_page.templates.len(), 2);
    let out_first = &out_page.templates[0];
    assert_eq!(out_first.name(), Some("Template.A"));
    assert_eq!(out_first.rename, None, "写回后再读仍不物化缺省");
    assert_eq!(out_first.fields[0].partial_name, "tax");
    assert!(
        out_first
            .named_page
            .as_ref()
            .expect("/TRef")
            .resides_in_associated_pdf()
    );
    let out_second = &out_page.templates[1];
    assert_eq!(out_second.rename, Some(false));
    assert_eq!(
        out_second.named_page.as_ref().expect("/TRef").file,
        Some(Value::String("tpl.fdf".into()))
    );
}

#[test]
fn fdf_annots_written_from_typed_view_roundtrip_keeps_page_ordinals() {
    // Table 251 `/Page` 序数（0 起）与 12.5.2 公共条目原值（/Contents、
    // /QuadPoints）经 fdf_annots_value → export_fdf → 重解析逐项还原。
    let bytes = fdf_file(
        "1.3",
        "/Annots [ << /Type /Annot /Subtype /Text /Page 0 /Contents (hi) >> << /Type /Annot /Subtype /Highlight /Page 2 /QuadPoints [ 0 0 1 1 2 2 3 3 ] >> ]",
    );
    let view = parse_buf(&bytes)
        .fdf_annots()
        .expect("ok")
        .expect("/Annots 视图");
    assert_eq!(view.len(), 2);
    assert_eq!(view.pages(), vec![0, 2]);
    assert!(view.entries.iter().all(|annot| annot.issues.is_empty()));

    let mut fdf = dictionary();
    fdf.annots = Some(PdfBuilder::fdf_annots_value(&view.entries));
    let written = PdfBuilder::new()
        .export_fdf(&model(&fdf), PdfVersion::V1_3)
        .expect("注解视图应能写回");
    let out = parse_buf(&written)
        .fdf_annots()
        .expect("重解析不应报错")
        .expect("导出应带 /Annots");
    assert!(out.issues.iter().all(|issue| !issue.is_advisory()));
    assert_eq!(out.entries[0].page, Some(0));
    assert_eq!(out.entries[1].page, Some(2));
    assert_eq!(out.pages(), vec![0, 2]);
    assert_eq!(out.entries[0].subtype.as_deref(), Some("Text"));
    assert_eq!(out.entries[1].subtype.as_deref(), Some("Highlight"));
    assert!(out.entries.iter().all(|annot| annot.subtype_permitted()));
    assert_eq!(
        out.entries[0].entry("Contents"),
        Some(&Value::String("hi".into())),
        "原值不丢"
    );
    assert_eq!(
        out.entries[1].entry("QuadPoints"),
        view.entries[1].entry("QuadPoints"),
        "数组原值逐项往返"
    );
    assert!(out.entries.iter().all(|annot| annot.issues.is_empty()));
}

#[test]
fn fdf_import_merge_survives_export_and_reparse_with_rename_prefix_advice() {
    // Annex J（informative）报告在真实写出→重解析后不变：同名根字段
    // `tax` 的冲突、`all_rename`（false，因第二页 `/Rename false`）与
    // J.2 形态前缀建议 `P0.Template.A_0.`。
    let bytes = fdf_file("1.3", CONFLICTING_PAGES_BODY);
    let document = parse_buf(&bytes);
    let merge = document.fdf_import_merge().expect("ok").expect("报告存在");
    assert_eq!(
        merge.conflicts,
        vec![pdf_engine::core::reader::fdf::FdfFieldNameConflict {
            name: "tax".into(),
            sources: vec![(0, 0), (1, 0)],
            all_rename: false,
        }]
    );
    assert_eq!(
        merge.templates[0].rename_prefix.as_deref(),
        Some("P0.Template.A_0.")
    );
    assert_eq!(merge.templates[1].rename_prefix, None);
    assert!(merge.issues.is_empty(), "{:?}", merge.issues);

    let view = document.fdf_pages().expect("ok").expect("视图");
    let mut fdf = dictionary();
    fdf.pages = Some(PdfBuilder::fdf_pages_value(&view.entries));
    let written = PdfBuilder::new()
        .export_fdf(&model(&fdf), PdfVersion::V1_3)
        .expect("写回");
    let out = parse_buf(&written)
        .fdf_import_merge()
        .expect("重解析")
        .expect("报告存在");
    assert_eq!(out.conflicts, merge.conflicts, "冲突按声明序往返");
    assert_eq!(
        out.templates
            .iter()
            .map(|entry| entry.rename_prefix.clone())
            .collect::<Vec<_>>(),
        merge
            .templates
            .iter()
            .map(|entry| entry.rename_prefix.clone())
            .collect::<Vec<_>>()
    );
    assert!(out.issues.is_empty(), "{:?}", out.issues);
}

#[test]
fn fdf_pages_writer_rejects_missing_required_entries_with_zero_bytes() {
    // Table 248 `/Templates`、Table 249 `/TRef`、Table 250 `/Name` 的
    // Required 违例与非字典数组项，都在装配字节之前 InvalidObject、零字节。
    let cases = [
        // 页字典缺 /Templates（Table 248 Required）。
        array(vec![dict_value(&[])]),
        // /Pages 项不是字典。
        array(vec![Value::Integer(7)]),
        // 模板缺 /TRef（Table 249 Required）。
        array(vec![dict_value(&[(
            "Templates",
            array(vec![dict_value(&[])]),
        )])]),
        // /TRef 缺 /Name（Table 250 Required）。
        array(vec![dict_value(&[(
            "Templates",
            array(vec![dict_value(&[("TRef", dict_value(&[]))])]),
        )])]),
        // /TRef /Name 不是文本串。
        array(vec![dict_value(&[(
            "Templates",
            array(vec![dict_value(&[(
                "TRef",
                dict_value(&[("Name", Value::Integer(9))]),
            )])]),
        )])]),
        // /Rename 写成整数（Table 249 声明 boolean）。
        array(vec![dict_value(&[(
            "Templates",
            array(vec![dict_value(&[
                ("TRef", dict_value(&[("Name", Value::String("T".into()))])),
                ("Rename", Value::Integer(1)),
            ])]),
        )])]),
    ];
    for pages in cases {
        let mut fdf = dictionary();
        fdf.pages = Some(pages);
        let error = PdfBuilder::new()
            .export_fdf(&model(&fdf), PdfVersion::V1_3)
            .expect_err("Required 条目违例应零字节拒绝");
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "错误形态：{error}"
        );
    }
}

#[test]
fn fdf_annots_writer_rejects_bad_page_ordinals_and_prohibited_subtypes() {
    // Table 251 的 `/Page`（缺失/非整型/负序数）与 Table 243 的注解类型
    // 黑名单（raw L24887–L24889）都在装配字节之前拒绝。
    let cases = [
        // 缺 /Page（Required for annotations in FDF files）。
        dict_value(&[("Subtype", Value::Name("Text".into()))]),
        // /Page 负序数（page 0 is the first page）。
        dict_value(&[
            ("Subtype", Value::Name("Text".into())),
            ("Page", Value::Integer(-1)),
        ]),
        // /Page 非整型。
        dict_value(&[
            ("Subtype", Value::Name("Text".into())),
            ("Page", Value::Float(1.5)),
        ]),
        // /Subtype 命中被排除的类型。
        dict_value(&[
            ("Subtype", Value::Name("Widget".into())),
            ("Page", Value::Integer(0)),
        ]),
        // /Subtype 缺失（Table 169 Required，无法核对白名单）。
        dict_value(&[("Page", Value::Integer(0))]),
    ];
    for annot in cases {
        let mut fdf = dictionary();
        fdf.annots = Some(array(vec![annot]));
        let error = PdfBuilder::new()
            .export_fdf(&model(&fdf), PdfVersion::V1_3)
            .expect_err("注解违例应零字节拒绝");
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "错误形态：{error}"
        );
    }
    // /Annots 数组项不是字典。
    let mut fdf = dictionary();
    fdf.annots = Some(array(vec![Value::Integer(9)]));
    assert!(matches!(
        PdfBuilder::new().export_fdf(&model(&fdf), PdfVersion::V1_3),
        Err(WriteError::InvalidObject(_))
    ));
}

#[test]
fn fdf_pages_and_annots_below_pdf_1_3_are_rejected_by_writer_and_reported_by_reader() {
    // 写端：Table 243 的 `/Pages` `/Annots` 都是 PDF 1.3 档位，1.2 声明
    // → UnsupportedVersion、零字节。读端：同一份 1.2 文件里的条目照常
    // 类型化，只各附一条版本门禁诊断（只报告不拒绝）。
    let mut fdf = dictionary();
    fdf.pages = Some(array(Vec::new()));
    assert!(matches!(
        PdfBuilder::new().export_fdf(&model(&fdf), PdfVersion::V1_2),
        Err(WriteError::UnsupportedVersion(_))
    ));
    let mut fdf = dictionary();
    fdf.annots = Some(array(Vec::new()));
    assert!(matches!(
        PdfBuilder::new().export_fdf(&model(&fdf), PdfVersion::V1_2),
        Err(WriteError::UnsupportedVersion(_))
    ));

    let document = parse_buf(&fdf_file(
        "1.2",
        "/Pages [ << /Templates [ ] >> ] /Annots [ << /Type /Annot /Subtype /Text /Page 0 >> ]",
    ));
    let pages = document.fdf_pages().expect("ok").expect("视图");
    assert!(
        pages
            .issues
            .iter()
            .any(|issue| matches!(issue, FdfPagesIssue::PagesVersionGate { declared } if declared == "1.2")),
        "{:?}",
        pages.issues
    );
    assert_eq!(pages.len(), 1, "低于声明档位仍照常类型化");
    let annots = document.fdf_annots().expect("ok").expect("视图");
    assert!(
        annots
            .issues
            .iter()
            .any(|issue| matches!(issue, FdfPagesIssue::AnnotsVersionGate { declared } if declared == "1.2")),
        "{:?}",
        annots.issues
    );
    assert_eq!(annots.entries[0].page, Some(0));
}
