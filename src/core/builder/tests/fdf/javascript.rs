//! FDF 写端 `/JavaScript` 单元测试（Table 243 续表 raw L24933–L24934 →
//! Table 245 raw L24951–L24981）：类型化脚本字典的序列化优先级、
//! 原值保留（含 `/Doc` 未完整解码的配对）、缺省不物化，以及非法形态
//! 在装配字节之前零字节拒绝与 `/AfterPermsReady` 的 PDF 1.6 门禁。

use super::*;
use crate::core::reader::fdf::{FdfJavaScript, FdfJavaScriptScript};
use crate::core::types::{PdfTextString, StreamData};

/// 写端文本串策略的期望十六进制：与 `types/fdf.rs::text_string_value`
/// 同一编码路径，避免测试里手写码位。
fn hex_text(text: &str) -> String {
    PdfTextString::new(text)
        .encode()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect()
}

/// 把 JavaScript 字典模型挂进 FDF 字典读模型。
fn model_with_javascript(scripts: Option<FdfJavaScript>) -> FdfDocument {
    let mut fdf = bare_fdf_dictionary();
    fdf.javascript_scripts = scripts;
    FdfDocument {
        catalog_version: None,
        catalog: Dict::new(),
        fdf,
    }
}

/// 只带一个条目的 JavaScript 字典（装配校验用例的最小形态）。
fn javascript_catalog(key: &str, value: Value) -> Dict {
    catalog_of(Dict::new().insert("JavaScript", Value::Dict(Dict::new().insert(key, value))))
}

fn export_text(scripts: Option<FdfJavaScript>, version: PdfVersion) -> String {
    String::from_utf8(
        PdfBuilder::new()
            .export_fdf(&model_with_javascript(scripts), version)
            .expect("导出应成功"),
    )
    .expect("ASCII")
}

#[test]
fn fdf_export_javascript_scripts_typed_wins_over_stale_raw_entries() {
    // Table 245 — 类型化条目为准：`/Before`、`/After` 写成十六进制文本串
    // 并覆盖原字典里的同名串值；`/Doc` 完整解码时同样以类型化为准。
    let dict = Dict::new()
        .insert("Before", Value::String("stale".into()))
        .insert(
            "Doc",
            Value::Array(Array(vec![
                Value::Name("init".into()),
                Value::String("var a = 1;".into()),
            ])),
        );
    let scripts = FdfJavaScript {
        before: Some("before();".into()),
        after: Some("after();".into()),
        after_perms_ready: None,
        doc_scripts: vec![FdfJavaScriptScript {
            name: "init".into(),
            script: "var a = 1;".into(),
        }],
        dict,
    };
    let mut model = model_with_javascript(Some(scripts));
    // 原值字段即使与类型化视图冲突也不参与写出（类型化优先）。
    model.fdf.javascript = Some(Value::String("ignored".into()));
    let text = String::from_utf8(
        PdfBuilder::new()
            .export_fdf(&model, PdfVersion::V1_4)
            .expect("类型化 /JavaScript 导出"),
    )
    .expect("ASCII");
    assert!(
        text.contains(&format!("/Before <{}>", hex_text("before();"))),
        "/Before 应写成十六进制文本串：{text}"
    );
    assert!(
        text.contains(&format!("/After <{}>", hex_text("after();"))),
        "/After 应写成十六进制文本串：{text}"
    );
    assert!(
        text.contains(&format!("/Doc [/init <{}>]", hex_text("var a = 1;"))),
        "完整解码的 /Doc 以类型化配对写出：{text}"
    );
    assert!(!text.contains("stale"), "类型化字段应覆盖原值：{text}");
    assert!(
        !text.contains("ignored"),
        "类型化优先于 javascript 原值字段：{text}"
    );
}

#[test]
fn fdf_export_javascript_partial_doc_array_keeps_raw_pairs() {
    // 含流/间接形态的 `/Doc` 配对只被部分解码：类型化列表不覆盖原数组
    // （否则未确证的那一对会被静默丢弃）；类型化为 None 的 `/Before`
    // 间接引用原值同样透传。
    let dict = Dict::new().insert("Before", Value::Ref(4, 0)).insert(
        "Doc",
        Value::Array(Array(vec![
            Value::Name("full".into()),
            Value::Ref(4, 0),
            Value::Name("tail".into()),
            Value::String("t".into()),
        ])),
    );
    let scripts = FdfJavaScript {
        before: None,
        after: None,
        after_perms_ready: None,
        doc_scripts: vec![FdfJavaScriptScript {
            name: "tail".into(),
            script: "t".into(),
        }],
        dict,
    };
    let text = export_text(Some(scripts), PdfVersion::V1_4);
    assert!(
        text.contains("/Before 4 0 R"),
        "未类型化的间接引用原值透传：{text}"
    );
    assert!(
        text.contains("/Doc [/full 4 0 R /tail (t)]"),
        "未完整解码的 /Doc 原数组保留：{text}"
    );
    assert!(
        !text.contains(&format!("/tail <{}>", hex_text("t"))),
        "保留原数组时不应写入类型化副本：{text}"
    );
    // 原数组缺失时才按类型化配对补写（程序化构建模型的路径）。
    let scripts = FdfJavaScript {
        before: None,
        after: None,
        after_perms_ready: None,
        doc_scripts: vec![FdfJavaScriptScript {
            name: "only".into(),
            script: "x".into(),
        }],
        dict: Dict::new(),
    };
    let text = export_text(Some(scripts), PdfVersion::V1_4);
    assert!(
        text.contains(&format!("/Doc [/only <{}>]", hex_text("x"))),
        "原数组缺失时按类型化写出：{text}"
    );
}

#[test]
fn fdf_export_javascript_raw_only_passthrough_and_absent_not_materialized() {
    // 类型化为 None 时原字典透传（非确证形态不丢弃）。
    let raw = Dict::new().insert("Before", Value::String("raw script".into()));
    let mut fdf = bare_fdf_dictionary();
    fdf.javascript = Some(Value::Dict(raw));
    let text = String::from_utf8(
        PdfBuilder::new()
            .export_fdf(
                &FdfDocument {
                    catalog_version: None,
                    catalog: Dict::new(),
                    fdf,
                },
                PdfVersion::V1_4,
            )
            .expect("原值透传"),
    )
    .expect("ASCII");
    assert!(
        text.contains("/JavaScript << /Before (raw script) >>"),
        "原值字典透传：{text}"
    );

    // 全缺省不物化：类型化与原值都 None 时删除原字典里的同名条目。
    let mut fdf = bare_fdf_dictionary();
    fdf.dict = fdf.dict.insert(
        "JavaScript",
        Value::Dict(Dict::new().insert("After", Value::String("ghost".into()))),
    );
    let text = String::from_utf8(
        PdfBuilder::new()
            .export_fdf(
                &FdfDocument {
                    catalog_version: None,
                    catalog: Dict::new(),
                    fdf,
                },
                PdfVersion::V1_4,
            )
            .expect("缺省导出"),
    )
    .expect("ASCII");
    assert!(!text.contains("/JavaScript"), "缺省条目不物化：{text}");
}

#[test]
fn fdf_javascript_non_dict_and_illegal_script_forms_rejected_before_assembly() {
    // Table 243 — `/JavaScript` 类型是 dictionary：其他形态装配前拒绝。
    for value in [
        Value::Integer(1),
        Value::String("app.init();".into()),
        Value::Array(Array::new()),
        Value::Name("Script".into()),
    ] {
        let catalog = catalog_of(Dict::new().insert("JavaScript", value));
        assert!(
            matches!(
                PdfBuilder::new().build_fdf(&catalog, PdfVersion::V1_4),
                Err(WriteError::InvalidObject(_))
            ),
            "非字典 /JavaScript 应零字节拒绝"
        );
    }
    // Table 245 — 脚本条目既非文本串又非文本流：拒绝。
    for value in [
        Value::Integer(3),
        Value::Name("script".into()),
        Value::Array(Array::new()),
        // 直接内联流：单间接对象写出无法承载流字节（12.7.7.2）。
        Value::Stream(
            Dict::new().insert("Length", Value::Integer(4)),
            StreamData::Owned(b"f();".to_vec()),
        ),
    ] {
        for key in ["Before", "After", "AfterPermsReady"] {
            let catalog = javascript_catalog(key, value.clone());
            assert!(
                matches!(
                    PdfBuilder::new().build_fdf(&catalog, PdfVersion::V1_6),
                    Err(WriteError::InvalidObject(_))
                ),
                "/JavaScript /{key} = {value:?} 应零字节拒绝"
            );
        }
    }
    // Table 245 — `/Doc` 必须是数组、偶数长度、配对首项是 name、
    // 第二项是文本串或文本流。
    for value in [
        Value::String("doc".into()),
        Value::Dict(Dict::new()),
        Value::Array(Array(vec![Value::Name("a".into())])),
        Value::Array(Array(vec![
            Value::String("not-a-name".into()),
            Value::String("s".into()),
        ])),
        Value::Array(Array(vec![Value::Name("a".into()), Value::Integer(4)])),
    ] {
        let catalog = javascript_catalog("Doc", value.clone());
        assert!(
            matches!(
                PdfBuilder::new().build_fdf(&catalog, PdfVersion::V1_4),
                Err(WriteError::InvalidObject(_))
            ),
            "/Doc = {value:?} 应零字节拒绝"
        );
    }
    // 合法形态放行：文本串、间接引用的文本流都可原值写出。
    let ok = javascript_catalog(
        "Doc",
        Value::Array(Array(vec![
            Value::Name("a".into()),
            Value::Ref(4, 0),
            Value::Name("b".into()),
            Value::HexString(hex_text("s")),
        ])),
    );
    assert!(PdfBuilder::new().build_fdf(&ok, PdfVersion::V1_4).is_ok());
    // 模型导出路径同样拒绝（类型化字典里带非法条目）。
    let scripts = FdfJavaScript {
        before: None,
        after: None,
        after_perms_ready: None,
        doc_scripts: Vec::new(),
        dict: Dict::new().insert("Before", Value::Integer(3)),
    };
    assert!(matches!(
        PdfBuilder::new().export_fdf(&model_with_javascript(Some(scripts)), PdfVersion::V1_4),
        Err(WriteError::InvalidObject(_))
    ));
    // 档位门禁先于类型校验：1.3 声明下的非串 /JavaScript 仍报
    // UnsupportedVersion（与既有门禁测试口径一致）。
    let bad = catalog_of(Dict::new().insert("JavaScript", Value::Integer(1)));
    assert!(matches!(
        PdfBuilder::new().build_fdf(&bad, PdfVersion::V1_3),
        Err(WriteError::UnsupportedVersion(_))
    ));
}

#[test]
fn fdf_javascript_after_perms_ready_requires_pdf_1_6() {
    // Table 245 raw L24961–L24969 — `/AfterPermsReady`（Optional; PDF 1.6）。
    let catalog = javascript_catalog("AfterPermsReady", Value::String("perms ready".into()));
    for version in [
        PdfVersion::V1_2,
        PdfVersion::V1_3,
        PdfVersion::V1_4,
        PdfVersion::V1_5,
    ] {
        assert!(
            matches!(
                PdfBuilder::new().build_fdf(&catalog, version),
                Err(WriteError::UnsupportedVersion(_))
            ),
            "/AfterPermsReady 应要求 1.6，声明 {version} 时零字节拒绝"
        );
    }
    assert!(
        PdfBuilder::new()
            .build_fdf(&catalog, PdfVersion::V1_6)
            .is_ok()
    );
    // 档位门禁先于类型校验：1.5 声明下的非法类型也报 UnsupportedVersion。
    let bad = javascript_catalog("AfterPermsReady", Value::Integer(3));
    assert!(matches!(
        PdfBuilder::new().build_fdf(&bad, PdfVersion::V1_5),
        Err(WriteError::UnsupportedVersion(_))
    ));
    // 模型导出路径：1.5 拒绝、1.6 放行。
    let scripts = FdfJavaScript {
        before: None,
        after: None,
        after_perms_ready: Some("perms ready".into()),
        doc_scripts: Vec::new(),
        dict: Dict::new(),
    };
    assert!(matches!(
        PdfBuilder::new().export_fdf(
            &model_with_javascript(Some(scripts.clone())),
            PdfVersion::V1_5
        ),
        Err(WriteError::UnsupportedVersion(_))
    ));
    let text = export_text(Some(scripts), PdfVersion::V1_6);
    assert!(
        text.contains(&format!("/AfterPermsReady <{}>", hex_text("perms ready"))),
        "1.6 档位下类型化 /AfterPermsReady 写出：{text}"
    );
}
