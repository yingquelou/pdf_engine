//! 文档需求写出 → 解析 → 回读往返（ISO 32000-1 12.10、Table 264/265 与
//! Table 28 的 `/Requirements` 条目）：目录里的直接对象数组、元素
//! `/Type /Requirement` 与 `/S /EnableJavaScripts`、对象号布局不变，
//! 以及被拒路径（PDF 1.7 门禁、非空 `/RH`）确实零字节。
//!
//! 断言分两层：
//!
//! - **通用对象 API**：解析回 [`pdf_engine::core::types::Document`] 后直接
//!   读目录字典键值，验证写出的语法形态；
//! - **类型化读 API**（`Document::requirements` / `requirements_issues` /
//!   `requires_javascript`）：复验读写口径是否对齐（注册序、
//!   `EnableJavaScripts` 类型化、handler `JS`↔`/Script` 动作名、特征画像
//!   里的 `PdfFeature::DocumentRequirements`）。写端默认通道不产出 `/RH`
//!   （raw L26125），故 handler 一条改用通用对象 API 注入与写端
//!   `handler_value` **逐字段同形**的字典再回读。

use super::*;
use pdf_engine::core::builder::{
    DocumentRequirement, DocumentRequirementType, PageSize, PdfBuilder, RequirementHandler,
};
use pdf_engine::core::reader::navigation::{HandlerType, RequirementType, RequirementsIssue};
use pdf_engine::core::types::{Document, PdfFeature, PdfTextString, PdfVersion};
use pdf_engine::core::writer::{self, WriteError};

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

/// 解析后文档的特征画像集合。
fn features_of(doc: &Document) -> Vec<PdfFeature> {
    doc.features.borrow().iter().copied().collect()
}

/// 与写端 `handler_value`（Table 265）**逐字段同形**的 handler 字典：
/// `/Type /ReqHandler` + `/S /JS` + `/Script` 为 UTF-16BE 十六进制文本串
/// （规范里 `Script` 的类型列是 text string，不是 name）。
fn js_handler_value() -> Value {
    let script: String = PdfTextString::from("startup.js")
        .encode()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect();
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("ReqHandler".into()))
            .insert("S", Value::Name("JS".into()))
            .insert("Script", Value::HexString(script)),
    )
}

/// 用通用对象 API 覆盖目录并重新序列化、解析（写端默认通道拒写 `/RH`，
/// handler 与畸形形态两条口径只能这样注入）。
fn rewrite_catalog_and_parse(doc: &Document, catalog: Dict) -> Document {
    let (root, generation) = doc.trailer().root().expect("/Root");
    doc.set_object(Object::new(root, generation, Value::Dict(catalog)));
    let mut buf = Vec::new();
    writer::write_document(doc, &mut buf).expect("rewrite catalog");
    let reparsed = parse_buf(&buf);
    reparsed.load_all_objects().expect("load all objects");
    reparsed
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

/// 类型化读 API 复验写端产物：注册序、`EnableJavaScripts` 类型化、
/// `requires_javascript()` 与 `PdfFeature::DocumentRequirements` 画像。
#[test]
fn doc_requirements_typed_read_roundtrip_reports_javascript_requirement() {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::Letter);
    builder.add_document_requirement(DocumentRequirement::enable_javascripts());
    builder.add_document_requirement(DocumentRequirement {
        requirement_type: DocumentRequirementType::EnableJavaScripts,
        handlers: Vec::new(),
    });
    let doc = parse(&mut builder);

    let model = doc
        .requirements()
        .expect("typed read")
        .expect("catalog /Requirements present");
    assert_eq!(model.requirements.len(), 2, "注册顺序即数组顺序");
    for requirement in &model.requirements {
        assert_eq!(
            requirement.subtype,
            RequirementType::EnableJavaScripts,
            "Table 264 的 /S 须类型化为 EnableJavaScripts"
        );
        assert_eq!(
            requirement.type_name.as_deref(),
            Some("Requirement"),
            "Table 264：写出的 /Type 读回为 Requirement"
        );
        assert!(
            requirement.handlers.is_empty(),
            "写端不产出 /RH（raw L26125），类型化模型亦应为空"
        );
        assert!(
            requirement.is_conforming(),
            "写端产物须无任何 shall 违例：{:?}",
            requirement.issues
        );
    }
    assert!(model.is_conforming(), "整数组无违例");
    assert!(
        doc.requirements_issues()
            .expect("requirements_issues")
            .is_empty(),
        "读端不得对写端产物报违例"
    );
    assert!(
        doc.requires_javascript().expect("requires_javascript"),
        "12.10.1：EnableJavaScripts ⇒ 要求启用 JavaScript 执行"
    );
    assert!(
        features_of(&doc).contains(&PdfFeature::DocumentRequirements),
        "画像须登记 DocumentRequirements：{:?}",
        features_of(&doc)
    );
    // 通用对象 API 口径同步保留：数组仍直接嵌套在目录里。
    assert_eq!(catalog_requirements(&doc).0.len(), 2);
}

/// 未写 `/Requirements` 时：类型化读返回 `None`、无违例、
/// `requires_javascript() == false`，画像亦不含该特征（负向）。
#[test]
fn doc_requirements_typed_read_absent_reports_none_and_no_feature() {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::Letter);
    let doc = parse(&mut builder);

    assert!(
        doc.requirements().expect("typed read").is_none(),
        "目录无条目 ⇒ None"
    );
    assert!(
        doc.requirements_issues()
            .expect("requirements_issues")
            .is_empty()
    );
    assert!(
        !doc.requires_javascript().expect("requires_javascript"),
        "无需求数组不得判定为需要 JavaScript"
    );
    assert!(
        !features_of(&doc).contains(&PdfFeature::DocumentRequirements),
        "未写 /Requirements 不应登记特征：{:?}",
        features_of(&doc)
    );
}

/// handler 口径对齐：`/S /JS` ↔ `/Script` 文本串动作名 ↔ 类型化
/// `HandlerType::JavaScript`；同时读端把 raw L26123–L26125 的 `/RH` 违例
/// 类型化报出（与写端拒写同一口径）。
#[test]
fn doc_requirements_typed_read_aligns_handler_js_with_script_action_name() {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::Letter);
    builder.add_document_requirement(DocumentRequirement::enable_javascripts());
    let doc = parse(&mut builder);

    let requirement = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Requirement".into()))
            .insert("S", Value::Name("EnableJavaScripts".into()))
            .insert("RH", Value::Array(Array(vec![js_handler_value()]))),
    );
    let catalog = catalog_of(&doc).insert("Requirements", Value::Array(Array(vec![requirement])));
    let reparsed = rewrite_catalog_and_parse(&doc, catalog);

    let model = reparsed
        .requirements()
        .expect("typed read")
        .expect("injected /Requirements present");
    assert_eq!(model.requirements.len(), 1);
    let entry = &model.requirements[0];
    assert_eq!(entry.subtype, RequirementType::EnableJavaScripts);
    assert_eq!(entry.handlers.len(), 1, "`/RH` 数组须解析出处理器");
    let handler = &entry.handlers[0];
    assert_eq!(
        handler.subtype,
        HandlerType::JavaScript,
        "Table 265：/S /JS 类型化为 JavaScript"
    );
    assert_eq!(
        handler.script.as_deref(),
        Some("startup.js"),
        "Table 265：/Script 是文本串（不是 name），解码后为文档级 JS 动作名"
    );
    assert_eq!(handler.type_name.as_deref(), Some("ReqHandler"));
    assert!(
        entry
            .issues
            .contains(&RequirementsIssue::RhUsedWithEnableJavaScripts {
                index: 0,
                handler_count: 1
            }),
        "读端须报出 /RH 违例（写端同口径拒写）：{:?}",
        entry.issues
    );
    assert!(
        reparsed.requires_javascript().expect("requires_javascript"),
        "违例不影响 EnableJavaScripts 判定"
    );
    assert!(
        features_of(&reparsed).contains(&PdfFeature::DocumentRequirements),
        "注入的条目同样登记进画像"
    );
}

/// 画像宽容口径：`/Requirements` 值形态畸形（非数组）时**仍**登记特征
/// （画像管版本下限，不管合规），违例由类型化读 API 报
/// `CatalogEntryNotArray`；`requires_javascript()` 不因畸形而 panic。
#[test]
fn doc_requirements_malformed_non_array_still_registers_feature() {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::Letter);
    builder.add_document_requirement(DocumentRequirement::enable_javascripts());
    let doc = parse(&mut builder);

    let catalog = catalog_of(&doc).insert("Requirements", Value::Name("EnableJavaScripts".into()));
    let reparsed = rewrite_catalog_and_parse(&doc, catalog);

    let model = reparsed
        .requirements()
        .expect("typed read")
        .expect("malformed entry still read");
    assert!(
        model
            .issues
            .contains(&RequirementsIssue::CatalogEntryNotArray { found: "name" }),
        "非数组须类型化报违例：{:?}",
        model.issues
    );
    assert!(model.requirements.is_empty());
    assert!(
        !reparsed.requires_javascript().expect("requires_javascript"),
        "畸形数组不得判定为需要 JavaScript"
    );
    assert!(
        features_of(&reparsed).contains(&PdfFeature::DocumentRequirements),
        "畸形值形态不撤销画像登记（inspect 按键存在登记）：{:?}",
        features_of(&reparsed)
    );
}
