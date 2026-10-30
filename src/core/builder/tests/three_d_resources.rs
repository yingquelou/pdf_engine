//! 3D 流 `/Resources` 名称树（13.6.3.1 Table 300）写端测试：文本串键、
//! 排序写出、直接对象校验与零字节失败契约。

use std::collections::BTreeMap;

use super::support::annot_object;
use super::*;
use crate::core::builder::types::text_string_value;
use crate::core::types::{Array, Dict, Document, PdfVersion, StreamData, Value};

/// 取条目中的间接引用对象号；缺失或非引用即 panic。
fn referenced_num(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Ref(num, 0)) => *num,
        other => panic!("expected indirect reference, got {other:?}"),
    }
}

/// 取 3D 流对象字典。
fn stream_dict(doc: &Document, stream_num: i64) -> Dict {
    let object = doc
        .get_object(stream_num)
        .expect("3D stream object")
        .expect("3D stream exists");
    match &object.value {
        Value::Stream(dict, _) => dict.clone(),
        other => panic!("expected 3D stream at object {stream_num}, got {other:?}"),
    }
}

fn three_d_annotation(handle: usize) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::ThreeD, [72.0, 600.0, 200.0, 680.0]).with_three_d(handle)
}

/// `/Resources` 写成名称树：`/Names` 键为文本串十六进制编码，按键排序，
/// 值原样写出。
#[test]
fn three_d_stream_resources_write_name_tree() {
    let mut resources = BTreeMap::new();
    resources.insert(
        "alpha".to_string(),
        Value::Dict(
            Dict::new()
                .insert("Mode", Value::Name("Turntable".into()))
                .insert("Speed", Value::Integer(2)),
        ),
    );
    resources.insert(
        "é".to_string(),
        Value::Dict(Dict::new().insert("Mask", Value::Name("Wheel".into()))),
    );

    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    let artwork = b.add_three_d_artwork(
        ThreeDArtwork::new(vec![1, 2, 3, 4]).with_resources(resources.clone()),
    );
    b.add_annotation(p0, three_d_annotation(artwork));

    let doc = b.assemble_document().expect("assemble 3D /Resources");
    let stream_num = referenced_num(annot_object(&doc, 3, 0).get("3DD"));
    let stream = stream_dict(&doc, stream_num);
    let tree = stream
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("/Resources name tree dictionary");
    assert_eq!(tree.get("Kids"), None, "写端只产出单层 /Names 节点");
    let names = tree
        .get("Names")
        .and_then(Value::as_array)
        .expect("/Names array");
    assert_eq!(names.len(), 4, "两个名称各占一对键值");

    assert_eq!(names.0[0], text_string_value("alpha"));
    assert_eq!(names.0[0], Value::HexString("616C706861".into()));
    assert_eq!(names.0[2], text_string_value("é"));
    assert_eq!(
        names.0[2],
        Value::HexString("E9".into()),
        "非 ASCII 名称按 PDFDocEncoding 编码"
    );
    for (index, (name, value)) in resources.iter().enumerate() {
        assert_eq!(names.0[index * 2], text_string_value(name));
        assert_eq!(names.0[index * 2 + 1], *value);
    }
}

/// 间接引用（含嵌套位置）在写出前被拒绝，零字节输出。
#[test]
fn invalid_three_d_resources_are_rejected_without_output() {
    for value in [
        Value::Ref(1, 0),
        Value::Array(Array(vec![Value::Dict(Dict::new().insert(
            "Nested",
            Value::Stream(Dict::new(), StreamData::Owned(Vec::new())),
        ))])),
    ] {
        let mut resources = BTreeMap::new();
        resources.insert("mode".to_string(), value);
        let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
        let p0 = b.page(PageSize::A4);
        let artwork = b.add_three_d_artwork(ThreeDArtwork::new(vec![1]).with_resources(resources));
        b.add_annotation(p0, three_d_annotation(artwork));

        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("invalid /Resources value");
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "expected InvalidObject, got {error}"
        );
        assert!(error.to_string().contains("/Resources"), "{error}");
        assert_eq!(out.len(), 0, "rejected build must not emit bytes");
    }
}
