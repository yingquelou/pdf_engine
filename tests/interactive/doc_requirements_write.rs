//! 文档需求写出 → 解析 → 回读往返（ISO 32000-1 12.10、Table 264/265 与
//! Table 28 的 `/Requirements` 条目）：目录里的直接对象数组、元素
//! `/Type /Requirement` 与 `/S /EnableJavaScripts`、对象号布局不变，
//! 以及被拒路径（PDF 1.7 门禁、非空 `/RH`）确实零字节。
//!
//! 读端（12.10 的类型化 API）由另一条车道并行开发，本文件只用**通用对象
//! API**（解析回 [`pdf_engine::core::types::Document`] 后直接读目录字典键值）
//! 断言写端产物。

use super::*;
use pdf_engine::core::builder::{
    DocumentRequirement, DocumentRequirementType, PageSize, PdfBuilder, RequirementHandler,
};
use pdf_engine::core::types::{Document, PdfVersion};
use pdf_engine::core::writer::WriteError;

fn catalog_of(doc: &Document) -> Dict {
    let (root, _generation) = doc.trailer().root().expect("/Root");
    doc.get_object(root)
        .expect("catalog lookup")
        .expect("catalog object")
        .value
        .as_dict()
        .expect("catalog dict")
        .clone()
}

/// 目录 `/Requirements` 必须是**直接**数组（12.10.1 只说它是 requirement
/// 字典数组，未要求间接引用；写端一律嵌套在目录内）。
fn catalog_requirements(doc: &Document) -> Array {
    match catalog_of(doc).get("Requirements") {
        Some(Value::Array(array)) => array.clone(),
        other => panic!("catalog /Requirements must be a direct array, got {other:?}"),
    }
}

fn parse(builder: &mut PdfBuilder) -> Document {
    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build document requirements");
    let doc = parse_buf(&buf);
    doc.load_all_objects().expect("load all objects");
    doc
}

/// 表 264 的单个 requirement 字典：断言 `Type`/`S` 与「无 `RH`」。
fn assert_requirement_dict(element: &Value) {
    let Value::Dict(dict) = element else {
        panic!("requirement 元素必须是字典，got {}", element.type_name());
    };
    assert_eq!(
        dict.get("Type").and_then(Value::as_name),
        Some("Requirement"),
        "Table 264：/Type 存在时 shall 为 Requirement"
    );
    assert_eq!(
        dict.get("S").and_then(Value::as_name),
        Some("EnableJavaScripts"),
        "Table 264：/S 必需，12.10.1 里唯一已定义的 requirement 类型"
    );
    assert_eq!(
        dict.get("RH"),
        None,
        "raw L26125：The RH entry shall not be used in PDF 1.7"
    );
    assert_eq!(dict.0.len(), 2, "除 Type 与 S 外不写任何条目：{dict:?}");
}

#[test]
fn doc_requirements_write_roundtrip_reads_direct_catalog_array() {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::Letter);
    builder.add_document_requirement(DocumentRequirement::enable_javascripts());

    let requirements = catalog_requirements(&parse(&mut builder));
    assert_eq!(requirements.0.len(), 1, "数组含一个 requirement 字典");
    assert_requirement_dict(&requirements.0[0]);
}

#[test]
fn doc_requirements_write_roundtrip_keeps_registration_order() {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::Letter);
    builder.add_document_requirement(DocumentRequirement::enable_javascripts());
    // 公有字段构造路径同样可用（模型保真入口）。
    builder.add_document_requirement(DocumentRequirement {
        requirement_type: DocumentRequirementType::EnableJavaScripts,
        handlers: Vec::new(),
    });

    let requirements = catalog_requirements(&parse(&mut builder));
    assert_eq!(requirements.0.len(), 2, "注册顺序即数组顺序");
    for element in &requirements.0 {
        assert_requirement_dict(element);
    }
}

#[test]
fn doc_requirements_write_allocates_no_extra_indirect_objects() {
    // 全部内容嵌套在目录字典里 ⇒ 对象号布局与不带 `/Requirements` 的同构
    // 文档完全一致。
    let mut control = PdfBuilder::with_version(PdfVersion::V1_7);
    control.page(PageSize::Letter);
    let control = parse(&mut control);

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::Letter);
    builder.add_document_requirement(DocumentRequirement::enable_javascripts());
    let doc = parse(&mut builder);

    let nums = |doc: &Document| {
        doc.objects_for_write()
            .iter()
            .map(|object| object.num)
            .collect::<Vec<i64>>()
    };
    assert_eq!(nums(&control), nums(&doc), "不得新增间接对象");
    assert_eq!(catalog_requirements(&doc).0.len(), 1);
    assert!(
        catalog_of(&control).get("Requirements").is_none(),
        "未配置时目录不写条目"
    );
}

#[test]
fn doc_requirements_rejected_by_version_gate_writes_no_bytes() {
    for version in [PdfVersion::V1_0, PdfVersion::V1_4, PdfVersion::V1_6] {
        let mut builder = PdfBuilder::with_version(version);
        builder.page(PageSize::Letter);
        builder.add_document_requirement(DocumentRequirement::enable_javascripts());
        let mut buf = Vec::new();
        let error = builder
            .build(&mut buf)
            .expect_err("Table 28（raw L3692）：/Requirements 是 PDF 1.7 条目");
        match error {
            WriteError::UnsupportedVersion(message) => assert!(
                message.contains("1.7") && message.contains("/Requirements"),
                "门禁原因须可读：{message}"
            ),
            other => panic!("version {version:?}: expected UnsupportedVersion, got {other}"),
        }
        assert!(buf.is_empty(), "版本门禁失败必须零字节");
    }
}

#[test]
fn doc_requirements_rejected_handler_backed_requirement_writes_no_bytes() {
    // 模型可承载 handler，默认写出路径按 raw L26125 拒绝 ⇒ 零字节。
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::Letter);
    builder.add_document_requirement(
        DocumentRequirement::enable_javascripts()
            .with_handlers(vec![RequirementHandler::javascript("startup.js")]),
    );
    let mut buf = Vec::new();
    let error = builder
        .build(&mut buf)
        .expect_err("raw L26125：/RH shall not be used in PDF 1.7");
    match error {
        WriteError::InvalidObject(message) => assert!(
            message.contains("RH") && message.contains("L26125"),
            "拒绝理由须引到规范行号：{message}"
        ),
        other => panic!("expected InvalidObject, got {other}"),
    }
    assert!(buf.is_empty(), "非空 /RH 必须零字节");
}
