//! 12.10 Document Requirements 读端的集成验证：本切片只做读端，写端不产出
//! `/Requirements`（也不新增 `PdfFeature` 门禁），故这里按 `xref_streams.rs`
//! 的做法**手工拼字节**造最小 PDF（经典交叉引用表 + trailer `/Root`），把目录
//! `/Requirements` 数组写成 Table 264/265 的各种形态，再从公开 API
//! （`Document::requirements` / `Document::requirements_issues` /
//! `Document::requires_javascript`）读回类型化视图与 shall 违例。

use pdf_engine::core::reader::navigation::{HandlerType, RequirementType, RequirementsIssue};

use super::*;

const CATALOG: i64 = 1;
const PAGES: i64 = 2;
const PAGE: i64 = 3;

/// 手工装配一个单页最小 PDF：目录文本由调用方注入（对象 1），额外对象
/// （如间接的需求字典）按 `(对象号, 正文)` 给出；交叉引用表逐号覆盖，
/// 缺号写 free 条目，故 `/Size` 始终与实际布局一致。
fn build_pdf(catalog_entries: &str, extra: &[(i64, &str)]) -> Vec<u8> {
    let mut buf = b"%PDF-1.7\n".to_vec();
    let mut offsets: Vec<(i64, usize)> = Vec::new();
    let mut write = |number: i64, body: String| {
        offsets.push((number, buf.len()));
        buf.extend_from_slice(format!("{number} 0 obj\n{body}\nendobj\n").as_bytes());
    };
    write(
        CATALOG,
        format!("<< /Type /Catalog /Pages {PAGES} 0 R {catalog_entries} >>"),
    );
    write(
        PAGES,
        format!("<< /Type /Pages /Kids [{PAGE} 0 R] /Count 1 >>"),
    );
    write(
        PAGE,
        format!("<< /Type /Page /Parent {PAGES} 0 R /MediaBox [0 0 612 792] >>"),
    );
    for (number, body) in extra {
        write(*number, (*body).to_owned());
    }
    let max = offsets.iter().map(|(number, _)| *number).max().unwrap_or(0);
    let xref_offset = buf.len();
    buf.extend_from_slice(format!("xref\n0 {}\n", max + 1).as_bytes());
    for number in 0..=max {
        match offsets.iter().find(|(found, _)| *found == number) {
            Some((_, offset)) => {
                buf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
            }
            None => buf.extend_from_slice(b"0000000000 65535 f \n"),
        }
    }
    buf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root {CATALOG} 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            max + 1
        )
        .as_bytes(),
    );
    buf
}

/// 装配 + 解析（与生产路径一致：落盘后由 `Parser` 读回）。
fn parse(catalog_entries: &str, extra: &[(i64, &str)]) -> Document {
    parse_buf(&build_pdf(catalog_entries, extra))
}

/// 一条合法的 `EnableJavaScripts` 需求（Table 264 全条目）。
const VALID: &str = "<< /Type /Requirement /S /EnableJavaScripts >>";

#[test]
fn document_requirements_absent_reads_none() {
    let doc = parse("", &[]);
    assert_eq!(
        doc.page_count().expect("page count"),
        1,
        "最小文件仍可正常读"
    );
    assert_eq!(doc.requirements().expect("read"), None);
    assert_eq!(doc.requirements_issues().expect("read"), Vec::new());
    assert!(!doc.requires_javascript().expect("read"));
}

#[test]
fn document_requirements_enable_javascripts_roundtrips_as_typed_model() {
    let doc = parse(&format!("/Requirements [{VALID}]"), &[]);
    let model = doc.requirements().expect("read").expect("present");
    assert!(model.is_conforming(), "合规需求不应有违例: {model:?}");
    assert_eq!(model.requirements.len(), 1);
    let entry = &model.requirements[0];
    assert_eq!(entry.type_name.as_deref(), Some("Requirement"));
    assert_eq!(entry.subtype, RequirementType::EnableJavaScripts);
    assert!(entry.handlers.is_empty());
    assert!(doc.requires_javascript().expect("read"));
}

#[test]
fn document_requirements_rh_with_enable_javascripts_reports_shall_violation() {
    // 12.10.1 L26123–L26125：「The RH entry shall not be used in PDF 1.7」。
    let handler = "<< /Type /ReqHandler /S /JS /Script (ValidateDoc) >>";
    let requirement = format!("<< /Type /Requirement /S /EnableJavaScripts /RH [{handler}] >>");
    let doc = parse(&format!("/Requirements [{requirement}]"), &[]);
    let model = doc.requirements().expect("read").expect("present");
    let entry = &model.requirements[0];
    assert_eq!(
        entry.issues,
        vec![RequirementsIssue::RhUsedWithEnableJavaScripts {
            index: 0,
            handler_count: 1
        }]
    );
    let parsed = &entry.handlers[0];
    assert_eq!(parsed.subtype, HandlerType::JavaScript);
    assert_eq!(parsed.script.as_deref(), Some("ValidateDoc"));
    assert_eq!(parsed.type_name.as_deref(), Some("ReqHandler"));
    assert!(
        entry.issues[0]
            .to_string()
            .contains("the RH entry shall not be used in PDF 1.7"),
        "Display 须带规范原文: {}",
        entry.issues[0]
    );
    assert!(
        doc.requires_javascript().expect("read"),
        "违例不影响类型化判定"
    );
}

#[test]
fn document_requirements_unknown_subtype_is_retained_with_violation() {
    let doc = parse(
        "/Requirements [<< /Type /Requirement /S /EnableRichMedia >>]",
        &[],
    );
    let model = doc.requirements().expect("read").expect("present");
    let entry = &model.requirements[0];
    assert_eq!(
        entry.subtype,
        RequirementType::Unknown("EnableRichMedia".into()),
        "不认识的需求类型不得丢整条"
    );
    assert_eq!(
        entry.issues,
        vec![RequirementsIssue::SubtypeNotEnableJavaScripts {
            index: 0,
            name: "EnableRichMedia".to_owned()
        }]
    );
    assert!(!doc.requires_javascript().expect("read"));
}

#[test]
fn document_requirements_noop_handler_and_script_are_typed() {
    // Table 265：NoOp 不添加任何条目；Script 仅在 S = JS 时合法；
    // `<4A617661>` 是 hex string 形态的 "Java"。
    let handlers = [
        "<< /Type /ReqHandler /S /NoOp >>",
        "<< /S /JS /Script <4A617661> >>",
        "<< /S /NoOp /Script (Bad) >>",
    ]
    .join(" ");
    let requirement = format!("<< /Type /Requirement /S /EnableRichMedia /RH [{handlers}] >>",);
    let doc = parse(&format!("/Requirements [{requirement}]"), &[]);
    let model = doc.requirements().expect("read").expect("present");
    let entry = &model.requirements[0];
    assert_eq!(entry.handlers.len(), 3);
    assert_eq!(entry.handlers[0].subtype, HandlerType::NoOp);
    assert_eq!(entry.handlers[0].script, None);
    assert_eq!(entry.handlers[1].subtype, HandlerType::JavaScript);
    assert_eq!(entry.handlers[1].script.as_deref(), Some("Java"));
    assert_eq!(entry.handlers[2].subtype, HandlerType::NoOp);
    assert_eq!(
        entry.handlers[2].script.as_deref(),
        Some("Bad"),
        "违例仍保留原值"
    );
    assert_eq!(
        entry.issues,
        vec![
            RequirementsIssue::SubtypeNotEnableJavaScripts {
                index: 0,
                name: "EnableRichMedia".to_owned()
            },
            RequirementsIssue::ScriptOnlyForJavaScript {
                index: 0,
                handler: 2
            },
        ],
        "未知需求类型不触发 RH 的 shall，只报 S 与 Script 的违例"
    );
}

#[test]
fn document_requirements_indirect_entries_are_resolved() {
    // 规范要求直接对象；读端宽容解引用（库策略，同 extensions.rs）。
    let doc = parse(
        "/Requirements [4 0 R 5 0 R]",
        &[
            (4, VALID),
            (5, "<< /Type /Requirement /S /EnableJavaScripts >>"),
        ],
    );
    let model = doc.requirements().expect("read").expect("present");
    assert_eq!(model.requirements.len(), 2);
    assert!(model.is_conforming());
    assert!(doc.requires_javascript().expect("read"));
}

#[test]
fn document_requirements_non_array_degrades_without_failing_parse() {
    // Table 3：/Requirements shall 是数组；写成 name 时只记违例、不整读失败。
    let doc = parse("/Requirements /EnableJavaScripts", &[]);
    let model = doc.requirements().expect("read").expect("present");
    assert_eq!(
        model.issues,
        vec![RequirementsIssue::CatalogEntryNotArray { found: "name" }]
    );
    assert!(model.requirements.is_empty());
    assert!(!model.is_conforming());
    assert_eq!(doc.page_count().expect("page count"), 1);
    assert_eq!(
        doc.requirements_issues().expect("read"),
        vec![RequirementsIssue::CatalogEntryNotArray { found: "name" }]
    );
}

#[test]
fn document_requirements_malformed_entries_are_kept_as_issues() {
    // 元素是整数、悬空引用与缺 /S 的字典：违例按元素下标保留，其余照常建模。
    let doc = parse(
        "/Requirements [42 9 0 R << /Type /Requirement >> << /Type /Requirement /S /EnableJavaScripts >>]",
        &[],
    );
    let model = doc.requirements().expect("read").expect("present");
    assert_eq!(
        model.issues.len(),
        2,
        "两个坏元素各一条违例: {:?}",
        model.issues
    );
    assert!(
        matches!(
            model.issues[0],
            RequirementsIssue::EntryNotDictionary {
                index: 0,
                found: "integer"
            }
        ),
        "{:?}",
        model.issues
    );
    match &model.issues[1] {
        RequirementsIssue::EntryUnresolvable { index, message } => {
            assert_eq!(*index, 1);
            assert!(message.contains("not found"), "原错误保留: {message}");
        }
        other => panic!("悬空元素应记 EntryUnresolvable: {other:?}"),
    }
    assert_eq!(model.requirements.len(), 2);
    assert_eq!(model.requirements[0].subtype, RequirementType::Missing);
    assert_eq!(
        model.requirements[0].issues,
        vec![RequirementsIssue::SubtypeMissing { index: 2 }],
        "缺 S 是 Table 264 的 Required 违例，下标仍是数组位置"
    );
    assert_eq!(
        model.requirements[1].subtype,
        RequirementType::EnableJavaScripts
    );
    assert!(doc.requires_javascript().expect("read"));
    assert_eq!(model.all_issues().len(), 3, "数组级违例 + 逐条违例全在案");
}
