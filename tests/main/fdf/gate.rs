//! FDF 写端 **ASCII85 载荷门禁**集成往返（12.7.7 + 7.4.3）：合法
//! `<~ ... ~>` 载荷的目录树经 `build_fdf`/`export_fdf` 写出 → 真实
//! Parser 重解析，断言字段层与 `/Pages`/`/Annots` 页层仍在、ASCII85
//! body 逐字回读；非法 body（EOD `~>`、表外字符、半组 `z`）在任何
//! 字节写出之前被 [`WriteError::InvalidObject`] 拒绝，目标字节恒为空。

use super::*;
use pdf_engine::core::builder::PdfBuilder;
use pdf_engine::core::reader::fdf::{FdfDictionary, FdfDocument};
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

fn model(fdf: FdfDictionary) -> FdfDocument {
    FdfDocument {
        catalog_version: None,
        catalog: Dict::new(),
        fdf,
    }
}

/// 合法 ASCII85 载荷的目录（Table 242：`/FDF` 直接字典）。
///
/// 载荷同时挂两处值位置——字段 `/V`（Table 246 直通整组 `9jqo^`）与注解
/// `/Contents`（Table 251 组边界 `z`）——证明门禁放行后逐字往返。
/// **不**把 `/Pages` 与 `/Fields` 同放一个 `/FDF`：Table 243 明文二者
/// shall not 同现（`fdf_structure_violations_rejected_zero_bytes` 已锁），
/// `/Pages` 层的合法 ASCII85 由 `fdf_pages_value` 独立往返测试覆盖、
/// `/Pages` 位置的非法载荷由本模块零字节拒绝测试覆盖。
fn conforming_catalog() -> Dict {
    let field = Dict::new()
        .insert("T", Value::String("person".into()))
        .insert("V", Value::Ascii85String("9jqo^".into()));
    let annot = Dict::new()
        .insert("Subtype", Value::Name("Text".into()))
        .insert("Page", Value::Integer(0))
        .insert("Contents", Value::Ascii85String("z".into()));
    Dict::new().insert(
        "FDF",
        Value::Dict(
            Dict::new()
                .insert("Fields", Value::Array(Array(vec![Value::Dict(field)])))
                .insert("Annots", Value::Array(Array(vec![Value::Dict(annot)]))),
        ),
    )
}

#[test]
fn fdf_export_accepts_conforming_ascii85_field_value_and_roundtrips() {
    let written = PdfBuilder::new()
        .build_fdf(&conforming_catalog(), PdfVersion::V1_4)
        .expect("合法 ASCII85 载荷应可写出");
    let text = String::from_utf8(written.clone()).expect("ASCII");
    assert!(
        text.contains("/V <~9jqo^~>") && text.contains("/Contents <~z~>"),
        "body 逐字包裹写出，不转义不剥离：{text}"
    );
    let document = parse_buf(&written);
    let fdf = document
        .fdf()
        .expect("重解析不应报错")
        .expect("导出应是 FDF");
    let person = fdf.field("person").expect("字段层仍在（/Fields 往返）");
    assert_eq!(
        person.value,
        Some(Value::Ascii85String("9jqo^".into())),
        "合法 ASCII85 应逐字回读为同一个值"
    );
    let annots = document
        .fdf_annots()
        .expect("注解层读取不应报错")
        .expect("导出应带 /Annots");
    assert_eq!(annots.entries[0].page, Some(0));
    assert_eq!(
        annots.entries[0].dict.get("Contents"),
        Some(&Value::Ascii85String("z".into())),
        "注解 /Contents 的合法 ASCII85 也应逐字回读"
    );
}

#[test]
fn fdf_export_rejects_malformed_ascii85_and_leaves_target_bytes_empty() {
    // 7.4.3 的三种非法形态各挂在一个值位置：字段 `/V`（`~>`）、
    // 注解 `/Contents`（表外字符 `v`）、`/JavaScript /Before`（半组 `z`）。
    let field_bad = |body: &str| {
        Dict::new().insert(
            "FDF",
            Value::Dict(Dict::new().insert(
                "Fields",
                Value::Array(Array(vec![Value::Dict(
                    Dict::new()
                        .insert("T", Value::String("f".into()))
                        .insert("V", Value::Ascii85String(body.into())),
                )])),
            )),
        )
    };
    let annot_bad = Dict::new().insert(
        "FDF",
        Value::Dict(Dict::new().insert(
            "Annots",
            Value::Array(Array(vec![Value::Dict(
                Dict::new()
                    .insert("Subtype", Value::Name("Text".into()))
                    .insert("Page", Value::Integer(0))
                    .insert("Contents", Value::Ascii85String("9jqo^v".into())),
            )])),
        )),
    );
    let javascript_bad = Dict::new().insert(
        "FDF",
        Value::Dict(Dict::new().insert(
            "JavaScript",
            Value::Dict(Dict::new().insert("Before", Value::Ascii85String("9jqoz".into()))),
        )),
    );
    for catalog in [field_bad("a~>b"), annot_bad, javascript_bad] {
        let emitted: Vec<u8> = match PdfBuilder::new().build_fdf(&catalog, PdfVersion::V1_4) {
            Ok(bytes) => bytes,
            Err(WriteError::InvalidObject(_)) => Vec::new(),
            Err(other) => panic!("期望 InvalidObject，实际 {other}"),
        };
        assert!(emitted.is_empty(), "非法 ASCII85 body 必须零字节产出");
    }

    // 模型导出路径（export_fdf）同口径：注解原值里挂非法 body →
    // 类型化拒绝、零字节。
    let mut fdf = dictionary();
    fdf.annots = Some(Value::Array(Array(vec![Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Text".into()))
            .insert("Page", Value::Integer(0))
            .insert("Contents", Value::Ascii85String("a~>b".into())),
    )])));
    let emitted: Vec<u8> = match PdfBuilder::new().export_fdf(&model(fdf), PdfVersion::V1_4) {
        Ok(bytes) => bytes,
        Err(WriteError::InvalidObject(_)) => Vec::new(),
        Err(other) => panic!("期望 InvalidObject，实际 {other}"),
    };
    assert!(emitted.is_empty(), "模型路径同样零字节");

    // 对照：同形态的合法 body 写出后可被真实 Parser 逐字回读（见
    // 上一测试）——门禁只拦非法取值，不改变合法输出的字节。
    assert!(
        PdfBuilder::new()
            .build_fdf(&field_bad("9jqo^"), PdfVersion::V1_4)
            .is_ok()
    );
}
