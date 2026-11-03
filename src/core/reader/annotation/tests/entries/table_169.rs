//! `/Subtype` 名称解码：未知子类型原样保留，与 Table 169 的三列
//! （名字、引入版本、标记归属）逐行核对到各自变体。

use crate::core::reader::annotation::*;
use crate::core::types::{Dict, StreamData, Value};

use super::super::support::*;

#[test]
fn unknown_subtype_is_preserved() {
    // `Hologram` 是 PDF 2.0 的子类型，不在 Table 169 的 PDF 1.7 全集内。
    let annotation =
        parse_annotation(&doc(vec![]), 0, None, &base_annotation("Hologram")).expect("unknown");
    assert_eq!(
        annotation.subtype,
        AnnotationSubtype::Unknown("Hologram".into())
    );
    assert_eq!(annotation.subtype.name(), "Hologram");
    assert!(!annotation.subtype.is_markup());
    assert_eq!(annotation.subtype.min_version(), None);
}

/// Table 169 的三列（子类型名、引入版本、是否标记类）逐行核对。
#[test]
fn table_169_rows_map_to_names_versions_and_markup_flags() {
    assert_eq!(TABLE_169.len(), 26, "PDF 1.7 全集应为 26 种");
    let mut variants = Vec::new();
    for (name, version, markup) in TABLE_169 {
        // Sound 注解的 `/Sound`（Table 185）是必填条目，解析期只要求间接引用。
        let mut dict = base_annotation(name);
        if name == "Sound" {
            dict = dict.insert("Sound", Value::Ref(99, 0));
        }
        // Movie 注解的 `/Movie`（Table 186）是必填电影字典，需最小文件规格。
        if name == "Movie" {
            dict = dict.insert(
                "Movie",
                Value::Dict(
                    Dict::new().insert(
                        "F",
                        Value::Dict(
                            Dict::new()
                                .insert("Type", Value::Name("Filespec".into()))
                                .insert("F", Value::String("clip.mov".into())),
                        ),
                    ),
                ),
            );
        }
        // 3D 注解的 `/3DD`（Table 298）必填，需最小 3D 流字典。
        if name == "3D" {
            dict = dict.insert(
                "3DD",
                Value::Stream(
                    Dict::new()
                        .insert("Type", Value::Name("3D".into()))
                        .insert("Subtype", Value::Name("U3D".into())),
                    StreamData::Owned(Vec::new()),
                ),
            );
        }
        // Printer's mark 注解（14.11.3 Table 362）要求 `/F` 与 `/AP /N` 表单。
        if name == "PrinterMark" {
            dict = dict.insert("F", Value::Integer(68)).insert(
                "AP",
                Value::Dict(Dict::new().insert(
                    "N",
                    Value::Stream(
                        Dict::new().insert("Subtype", Value::Name("Form".into())),
                        StreamData::Owned(Vec::new()),
                    ),
                )),
            );
        }
        // Trap 网络注解（14.11.6 Table 366/367）要求 `/F`、`/AP /N` 表单
        // 与 `/LastModified` 跟踪条目（表单的 `/PCM` 必填）。
        if name == "TrapNet" {
            dict = dict
                .insert("F", Value::Integer(68))
                .insert("LastModified", Value::String("D:20240101000000Z".into()))
                .insert(
                    "AP",
                    Value::Dict(
                        Dict::new().insert(
                            "N",
                            Value::Stream(
                                Dict::new()
                                    .insert("Subtype", Value::Name("Form".into()))
                                    .insert("PCM", Value::Name("DeviceCMYK".into())),
                                StreamData::Owned(Vec::new()),
                            ),
                        ),
                    ),
                );
        }
        let annotation = parse_annotation(&doc(vec![]), 0, None, &dict).expect("subtype");
        assert!(
            !matches!(annotation.subtype, AnnotationSubtype::Unknown(_)),
            "{name} 应是标准子类型"
        );
        assert_eq!(annotation.subtype.name(), name);
        assert_eq!(
            annotation.subtype.min_version(),
            Some(version),
            "{name} 版本"
        );
        assert_eq!(annotation.subtype.is_markup(), markup, "{name} 标记类归属");
        variants.push(format!("{:?}", annotation.subtype));
    }
    variants.sort();
    let before = variants.len();
    variants.dedup();
    assert_eq!(variants.len(), before, "每个 Table 169 行应映射到不同变体");
}
