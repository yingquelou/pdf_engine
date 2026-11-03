//! 文档需求写端测试（ISO 32000-1 12.10、Table 264/265）：目录
//! `/Requirements` 的直接对象数组写出、`Type`/`S` 拼写与 `Script`↔`S`
//! 联动校验、`/RH` 默认拒绝、PDF 1.7 门禁与零字节输出契约。

use super::*;
use crate::core::builder::navigation::requirements;
use crate::core::builder::types::requirements::{handler_value, requirement_value};
use crate::core::types::{Array, PdfFeature, PdfTextString, PdfVersion};

fn hex_of(text: &str) -> String {
    PdfTextString::from(text)
        .encode()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect()
}

fn object_dict(doc: &Document, num: i64) -> Dict {
    doc.get_object(num)
        .expect("object")
        .expect("object exists")
        .value
        .as_dict()
        .expect("object dict")
        .clone()
}

fn catalog(doc: &Document) -> Dict {
    let (root, _generation) = doc
        .trailer
        .dict
        .get("Root")
        .and_then(Value::as_ref)
        .expect("/Root");
    object_dict(doc, root)
}

/// 写出后重新解析，走 reader 的懒加载对象路径。
///
/// 文件名带进程号 + 单调计数：同一次 `cargo test` 里并行线程可能产出字节数
/// 完全相同的文档，用 `out.len()` 做唯一键会互相覆盖临时夹具。
fn build_and_parse(b: &mut PdfBuilder) -> Document {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
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
        "pdf-engine-requirements-{}-{}.pdf",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    std::fs::write(&path, &out).expect("write temp pdf");
    let file = std::fs::File::open(&path).expect("open temp pdf");
    let mut parser = crate::core::reader::Parser::new(file).expect("parser");
    let doc = parser.parse_document().expect("parse");
    let _ = std::fs::remove_file(&path);
    doc
}

fn build_error(b: &mut PdfBuilder) -> (WriteError, Vec<u8>) {
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("build must fail");
    (error, out)
}

/// 目录 `/Requirements` 数组（直接对象），断言其确为数组而非间接引用。
fn catalog_requirements(doc: &Document) -> Array {
    let catalog = catalog(doc);
    match catalog.get("Requirements") {
        Some(Value::Array(array)) => array.clone(),
        other => panic!("catalog /Requirements must be a direct array, got {other:?}"),
    }
}

fn js_requirement() -> DocumentRequirement {
    DocumentRequirement::enable_javascripts()
}

/// 解析后文档的特征画像集合（读端在解析时按 inspect 口径填充）。
fn features(doc: &Document) -> Vec<PdfFeature> {
    doc.features.borrow().iter().copied().collect()
}

fn builder_with(version: PdfVersion) -> PdfBuilder {
    let mut b = PdfBuilder::with_version(version);
    b.page(PageSize::A4);
    b
}

#[test]
fn requirements_write_enable_javascripts_dict_into_catalog() {
    let mut b = builder_with(PdfVersion::V1_7);
    b.add_document_requirement(js_requirement());
    let doc = build_and_parse(&mut b);

    // 12.10.1：`/Requirements` 是 requirement 字典数组，字典直接嵌套在目录里。
    let requirements = catalog_requirements(&doc);
    assert_eq!(requirements.0.len(), 1, "一个 requirement 字典");
    let Value::Dict(dict) = &requirements.0[0] else {
        panic!("requirement 元素必须是字典");
    };
    assert_eq!(
        dict.get("Type").and_then(Value::as_name),
        Some("Requirement"),
        "Table 264：/Type 存在时 shall 为 Requirement"
    );
    assert_eq!(
        dict.get("S").and_then(Value::as_name),
        Some("EnableJavaScripts"),
        "Table 264：/S 必需，12.10.1 唯一已定义类型"
    );
    assert_eq!(dict.get("RH"), None, "raw L26125：PDF 1.7 shall not use RH");
    assert_eq!(dict.0.len(), 2, "只写 Type 与 S 两个条目：{dict:?}");
}

#[test]
fn requirements_write_keeps_registration_order_for_multiple_dicts() {
    let mut b = builder_with(PdfVersion::V1_7);
    b.add_document_requirement(js_requirement());
    b.add_document_requirement(js_requirement());
    let requirements = catalog_requirements(&build_and_parse(&mut b));
    assert_eq!(requirements.0.len(), 2, "数组按注册顺序保留两项");
    for element in &requirements.0 {
        let Value::Dict(dict) = element else {
            panic!("requirement 元素必须是字典");
        };
        assert_eq!(
            dict.get("S").and_then(Value::as_name),
            Some("EnableJavaScripts")
        );
    }
}

#[test]
fn requirements_absent_writes_no_catalog_entry() {
    let mut b = builder_with(PdfVersion::V1_7);
    let doc = build_and_parse(&mut b);
    assert!(
        catalog(&doc).get("Requirements").is_none(),
        "无配置不写目录条目"
    );
}

#[test]
fn requirements_write_registers_document_requirements_feature() {
    // 写出目录 `/Requirements` ⇒ 特征画像登记 `DocumentRequirements`
    // （写端 `navigation::apply` 显式 insert，读端 inspect 按键存在复核）。
    let mut b = builder_with(PdfVersion::V1_7);
    b.add_document_requirement(js_requirement());
    let doc = build_and_parse(&mut b);
    // 懒加载：画像在对象真正载入时填充，故先取目录再断言画像。
    assert!(
        matches!(catalog(&doc).get("Requirements"), Some(Value::Array(_))),
        "目录里应有 `/Requirements` 数组"
    );

    let registered = features(&doc);
    assert!(
        registered.contains(&PdfFeature::DocumentRequirements),
        "特性画像应登记 DocumentRequirements：{registered:?}"
    );
    assert_eq!(
        PdfFeature::DocumentRequirements.to_string(),
        "document requirements (catalog /Requirements)",
        "Display 口径与 feature_display 表一致"
    );
}

#[test]
fn requirements_absent_does_not_register_feature() {
    // 负向（同 `tests/extensions.rs` 的「未使用则不含」写法）：不配置
    // `/Requirements` 时画像不得凭空含该特征。
    let mut b = builder_with(PdfVersion::V1_7);
    let doc = build_and_parse(&mut b);
    assert!(
        catalog(&doc).get("Requirements").is_none(),
        "无配置不写目录条目"
    );
    assert!(
        !features(&doc).contains(&PdfFeature::DocumentRequirements),
        "未写 /Requirements 不应登记特性：{:?}",
        features(&doc)
    );
}

#[test]
fn requirements_serialize_requirement_and_handler_dicts() {
    // 唯一序列化路径的类型化输出：requirement 字典（Table 264）。
    let Value::Dict(dict) = requirement_value(&js_requirement()) else {
        panic!("requirement 序列化为字典");
    };
    assert_eq!(
        dict.get("Type").and_then(Value::as_name),
        Some("Requirement")
    );
    assert_eq!(
        dict.get("S").and_then(Value::as_name),
        Some(DocumentRequirementType::EnableJavaScripts.name())
    );

    // handler 字典（Table 265）：JS 带文本串 /Script，NoOp 不附加条目。
    let Value::Dict(js) = handler_value(&RequirementHandler::javascript("startup.js")) else {
        panic!("handler 序列化为字典");
    };
    assert_eq!(js.get("Type").and_then(Value::as_name), Some("ReqHandler"));
    assert_eq!(js.get("S").and_then(Value::as_name), Some("JS"));
    assert_eq!(
        js.get("Script"),
        Some(&Value::HexString(hex_of("startup.js"))),
        "Table 265：/Script 是 text string（不是 name），值为名称树里的键"
    );
    let Value::Dict(no_op) = handler_value(&RequirementHandler::no_op()) else {
        panic!("handler 序列化为字典");
    };
    assert_eq!(no_op.get("S").and_then(Value::as_name), Some("NoOp"));
    assert_eq!(no_op.get("Script"), None, "NoOp 不附加专有条目");
    assert_eq!(no_op.0.len(), 2);
}

#[test]
fn requirements_gate_rejects_versions_below_minimum_with_zero_bytes() {
    for version in [PdfVersion::V1_0, PdfVersion::V1_3, PdfVersion::V1_6] {
        let mut b = builder_with(version);
        b.add_document_requirement(js_requirement());
        let (error, out) = build_error(&mut b);
        match error {
            WriteError::UnsupportedVersion(message) => {
                assert!(
                    message.contains("1.7") && message.contains("/Requirements"),
                    "门禁原因须可读：{message}"
                );
            }
            other => panic!("version {version:?}: expected UnsupportedVersion, got {other}"),
        }
        assert!(out.is_empty(), "版本门禁失败必须零字节");
    }
    // 门禁阈值改由 PdfFeature::DocumentRequirements 能力模型给出（Table 28
    // raw L3692 把目录 `/Requirements` 标为 PDF 1.7），写端不再直接比常量。
    assert_eq!(
        PdfFeature::DocumentRequirements.min_version(),
        PdfVersion::V1_7,
        "12.10 门起版本即 Table 28 的 PDF 1.7"
    );
    assert!(
        !PdfVersion::V1_6.supports(PdfFeature::DocumentRequirements),
        "1.6 及更低版本不支持 `/Requirements`"
    );
    assert!(PdfVersion::V1_7.supports(PdfFeature::DocumentRequirements));
}

#[test]
fn requirements_gate_accepts_pdf_1_7() {
    // Table 28（raw L3692）把 `/Requirements` 标为 PDF 1.7；writer 支持区间
    // 上界即 1.7，故 1.7 是唯一可写出的目标版本。
    let mut b = builder_with(PdfVersion::V1_7);
    b.add_document_requirement(js_requirement());
    let requirements = catalog_requirements(&build_and_parse(&mut b));
    assert_eq!(requirements.0.len(), 1);
}

#[test]
fn requirements_rejects_non_empty_rh_array_with_zero_bytes() {
    // raw L26125「The RH entry shall not be used in PDF 1.7」+ `/Requirements`
    // 只在 1.7 及以后合法 ⇒ 模型可承载 handler，写出路径一律拒绝。
    let mut b = builder_with(PdfVersion::V1_7);
    b.add_document_requirement(
        js_requirement().with_handlers(vec![RequirementHandler::javascript("startup.js")]),
    );
    let (error, out) = build_error(&mut b);
    match error {
        WriteError::InvalidObject(message) => assert!(
            message.contains("RH") && message.contains("L26125"),
            "拒绝理由须引到规范行号：{message}"
        ),
        other => panic!("expected InvalidObject, got {other}"),
    }
    assert!(out.is_empty(), "非空 /RH 必须零字节");
}

#[test]
fn requirements_rejects_js_handler_without_script_with_zero_bytes() {
    let handler = RequirementHandler {
        handler_type: RequirementHandlerType::JavaScript,
        script: None,
    };
    let mut b = builder_with(PdfVersion::V1_7);
    b.add_document_requirement(js_requirement().with_handlers(vec![handler]));
    let (error, out) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty(), "S↔Script 联动失败必须零字节");
}

#[test]
fn requirements_rejects_noop_handler_with_script_and_empty_script_name() {
    let noop_with_script = RequirementHandler {
        handler_type: RequirementHandlerType::NoOp,
        script: Some("startup.js".into()),
    };
    let js_with_empty_script = RequirementHandler {
        handler_type: RequirementHandlerType::JavaScript,
        script: Some(String::new()),
    };
    for (handler, label) in [
        (noop_with_script, "NoOp 带 /Script"),
        (js_with_empty_script, "空 /Script 名字"),
    ] {
        let mut b = builder_with(PdfVersion::V1_7);
        b.add_document_requirement(js_requirement().with_handlers(vec![handler]));
        let (error, out) = build_error(&mut b);
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "{label}: {error}"
        );
        assert!(out.is_empty(), "{label} 必须零字节");
    }
}

#[test]
fn requirements_written_rejects_empty_array_and_non_dict_element() {
    // 数组元素必须是字典（Table 264 的数组元素类型）。
    let empty = Value::Array(Array(Vec::new()));
    assert!(matches!(
        requirements::validate_written(&empty),
        Err(WriteError::InvalidObject(_))
    ));
    for element in [
        Value::Integer(1),
        Value::Name("Requirement".into()),
        Value::Ref(3, 0),
    ] {
        let array = Value::Array(Array(vec![element]));
        let error = requirements::validate_written(&array)
            .expect_err("non-dictionary element must be rejected");
        assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    }
}

#[test]
fn requirements_written_rejects_misspelled_type_and_non_name_type() {
    let valid = Value::Array(Array(vec![Value::Dict(
        Dict::new().insert("S", Value::Name("EnableJavaScripts".into())),
    )]));
    // /Type 缺失是允许的（Table 264 标 Optional）。
    requirements::validate_written(&valid).expect("omitted /Type is allowed");
    for (key, value) in [
        ("Type", Value::Name("Requirements".into())),
        ("Type", Value::Name("requirement".into())),
        ("Type", Value::String("Requirement".into())),
    ] {
        let dict = Dict::new()
            .insert(key, value)
            .insert("S", Value::Name("EnableJavaScripts".into()));
        let error = requirements::validate_written(&Value::Array(Array(vec![Value::Dict(dict)])))
            .expect_err("misspelled /Type must be rejected");
        assert!(
            matches!(&error, WriteError::InvalidObject(message) if message.contains("shall be Requirement")),
            "{error}"
        );
    }
}

#[test]
fn requirements_written_rejects_missing_or_unknown_s() {
    // /S 必需（Required）；且 12.10.1 里唯一合法值是 EnableJavaScripts。
    let without_s = Value::Dict(Dict::new().insert("Type", Value::Name("Requirement".into())));
    assert!(matches!(
        requirements::validate_written(&Value::Array(Array(vec![without_s]))),
        Err(WriteError::InvalidObject(message)) if message.contains("Table 264 marks it Required")
    ));
    for value in [
        Value::Name("EnableJS".into()),
        Value::Name("GoToRemote".into()),
        Value::String("EnableJavaScripts".into()),
    ] {
        let label = format!("{value:?}");
        let dict = Dict::new().insert("S", value);
        let result = requirements::validate_written(&Value::Array(Array(vec![Value::Dict(dict)])));
        assert!(
            matches!(result, Err(WriteError::InvalidObject(_))),
            "非法 /S {label} 必须被拒"
        );
    }
}

#[test]
fn requirements_written_checks_handler_shape() {
    let requirement_with = |handler: Value| {
        Value::Array(Array(vec![Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Requirement".into()))
                .insert("S", Value::Name("EnableJavaScripts".into()))
                .insert("RH", Value::Array(Array(vec![handler]))),
        )]))
    };
    let rejected = [
        // 元素非字典
        requirement_with(Value::Integer(7)),
        // /Type 拼写（Table 265 shall 为 ReqHandler）
        requirement_with(Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("RequirementHandler".into()))
                .insert("S", Value::Name("JS".into()))
                .insert("Script", Value::String("startup.js".into())),
        )),
        // /S 闭集之外
        requirement_with(Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("ReqHandler".into()))
                .insert("S", Value::Name("JavaScript".into())),
        )),
        // /S 缺失
        requirement_with(Value::Dict(
            Dict::new().insert("Type", Value::Name("ReqHandler".into())),
        )),
        // JS 缺 /Script
        requirement_with(Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("ReqHandler".into()))
                .insert("S", Value::Name("JS".into())),
        )),
        // NoOp 带 /Script（Table 265：valid only if /S is JS）
        requirement_with(Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("ReqHandler".into()))
                .insert("S", Value::Name("NoOp".into()))
                .insert("Script", Value::String("startup.js".into())),
        )),
        // /Script 非文本串
        requirement_with(Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("ReqHandler".into()))
                .insert("S", Value::Name("JS".into()))
                .insert("Script", Value::Name("startup.js".into())),
        )),
        // /Script 空文本串
        requirement_with(Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("ReqHandler".into()))
                .insert("S", Value::Name("JS".into()))
                .insert("Script", Value::String(String::new())),
        )),
    ];
    for (index, array) in rejected.iter().enumerate() {
        let error = requirements::validate_written(array)
            .expect_err("handler index {index} must be rejected");
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "handler index {index}: {error}"
        );
    }

    // 合形 handler（模型层拒绝写出，形态自检须放行）。
    let js = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("ReqHandler".into()))
            .insert("S", Value::Name("JS".into()))
            .insert("Script", Value::HexString(hex_of("startup.js"))),
    );
    requirements::validate_written(&requirement_with(js)).expect("well-formed JS handler");
    let no_op = Value::Dict(Dict::new().insert("S", Value::Name("NoOp".into())));
    requirements::validate_written(&requirement_with(no_op)).expect("well-formed NoOp handler");
}
