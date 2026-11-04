//! 图像字典必需条目、标志与过滤器名、`/Alternates` 与页面资源查找。

use super::support::{dict, doc_with, document_with_page, image_dict, stream};
use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::types::{Array, Dict, Object, Value};

#[test]
fn image_requires_dimensions_color_space_and_valid_bits() {
    let missing_height = doc_with(vec![(
        10,
        stream(
            dict(vec![
                ("Subtype", Value::Name("Image".into())),
                ("Width", Value::Integer(1)),
                ("ColorSpace", Value::Name("DeviceGray".into())),
                ("BitsPerComponent", Value::Integer(8)),
            ]),
            vec![0u8],
        ),
    )]);
    let error = missing_height.image(&Value::Ref(10, 0)).unwrap_err();
    assert!(error.to_string().contains("/Height"), "unexpected: {error}");

    let missing_space = doc_with(vec![(
        11,
        stream(
            dict(vec![
                ("Subtype", Value::Name("Image".into())),
                ("Width", Value::Integer(1)),
                ("Height", Value::Integer(1)),
                ("BitsPerComponent", Value::Integer(8)),
            ]),
            vec![0u8],
        ),
    )]);
    let error = missing_space.image(&Value::Ref(11, 0)).unwrap_err();
    assert!(
        error.to_string().contains("/ColorSpace"),
        "unexpected: {error}"
    );

    let bad_bits = doc_with(vec![(
        12,
        stream(image_dict(1, 1, "DeviceGray", 3), vec![0u8]),
    )]);
    let error = bad_bits.image(&Value::Ref(12, 0)).unwrap_err();
    assert!(
        error.to_string().contains("unsupported /BitsPerComponent"),
        "unexpected: {error}"
    );

    let form = doc_with(vec![(
        13,
        stream(
            dict(vec![
                ("Subtype", Value::Name("Form".into())),
                ("Width", Value::Integer(1)),
                ("Height", Value::Integer(1)),
                ("ColorSpace", Value::Name("DeviceGray".into())),
                ("BitsPerComponent", Value::Integer(8)),
            ]),
            vec![0u8],
        ),
    )]);
    let error = form.image(&Value::Ref(13, 0)).unwrap_err();
    assert!(
        error.to_string().contains("/Subtype"),
        "unexpected: {error}"
    );
}

#[test]
fn image_flags_intent_and_filters_are_parsed() {
    let doc = doc_with(vec![(
        10,
        stream(
            image_dict(2, 1, "DeviceGray", 8)
                .insert("Interpolate", Value::Bool(true))
                .insert("Intent", Value::Name("Perceptual".into()))
                .insert("Filter", Value::Name("AHx".into())),
            b"00FF>".to_vec(),
        ),
    )]);
    let image = doc.image(&Value::Ref(10, 0)).expect("image");
    assert!(image.interpolate);
    assert_eq!(image.intent.as_deref(), Some("Perceptual"));
    assert_eq!(image.filters, vec!["ASCIIHexDecode".to_string()]);
    assert_eq!(image.data, vec![0x00, 0xFF]);
    assert_eq!(image.samples().unwrap().values, vec![0.0, 1.0]);
}

#[test]
fn alternates_are_traversable_and_nested_images_parse() {
    let indexed_space = Value::Array(Array(vec![
        Value::Name("Indexed".into()),
        Value::Name("DeviceRGB".into()),
        Value::Integer(1),
        Value::HexString("FF0000000000".into()),
    ]));
    let doc = doc_with(vec![
        (
            10,
            stream(
                image_dict(1, 1, "DeviceRGB", 8).insert(
                    "Alternates",
                    Value::Array(Array(vec![Value::Ref(11, 0), Value::Ref(12, 0)])),
                ),
                vec![0u8, 0, 0],
            ),
        ),
        (11, stream(image_dict(1, 1, "DeviceGray", 8), vec![128u8])),
        (
            12,
            stream(
                image_dict(1, 1, "DeviceGray", 8)
                    .insert("ColorSpace", indexed_space)
                    .insert(
                        "Decode",
                        Value::Array(Array(vec![Value::Integer(0), Value::Integer(1)])),
                    ),
                vec![1u8],
            ),
        ),
    ]);

    let mut image = doc.image(&Value::Ref(10, 0)).expect("image");
    assert_eq!(image.alternates.len(), 2);
    assert_eq!(
        image.alternates[0].color_space,
        Some(ColorSpaceModel::DeviceGray)
    );
    assert!(matches!(
        image.alternates[1].color_space,
        Some(ColorSpaceModel::Indexed(_))
    ));
    // The Indexed alternate's explicit /Decode targets palette indices.
    let error = image.alternates[1].samples().unwrap_err();
    assert!(
        error.to_string().contains("Indexed"),
        "unexpected error: {error}"
    );

    doc.set_object(Object::new(
        12,
        0,
        stream(
            image_dict(1, 1, "DeviceGray", 8).insert(
                "ColorSpace",
                Value::Array(Array(vec![
                    Value::Name("Indexed".into()),
                    Value::Name("DeviceRGB".into()),
                    Value::Integer(1),
                    Value::HexString("FF0000000000".into()),
                ])),
            ),
            vec![1u8],
        ),
    ));
    image = doc.image(&Value::Ref(10, 0)).expect("image");
    assert_eq!(image.alternates.len(), 2);
    assert_eq!(
        image.alternates[1].samples().unwrap().values,
        vec![0.0, 0.0, 0.0]
    );
}

#[test]
fn image_alternates_accept_table_90_dictionaries() {
    let alternate = stream(image_dict(1, 1, "DeviceRGB", 8), vec![0u8; 3]);
    let alternates = Value::Array(Array(vec![Value::Dict(
        Dict::new().insert("Image", Value::Ref(11, 0)),
    )]));
    let base = image_dict(2, 2, "DeviceRGB", 8).insert("Alternates", alternates);
    let doc = doc_with(vec![(10, stream(base, vec![0u8; 12])), (11, alternate)]);

    let image = doc.image(&Value::Ref(10, 0)).expect("image should parse");
    assert_eq!(image.alternates.len(), 1);
    assert_eq!(image.alternates[0].width, 1);
    assert_eq!(
        image.alternates[0].color_space,
        Some(ColorSpaceModel::DeviceRGB)
    );
}

#[test]
fn page_image_resolves_page_xobject_resource() {
    let doc = document_with_page(
        Some(Value::Dict(dict(vec![(
            "XObject",
            Value::Dict(dict(vec![("Im1", Value::Ref(10, 0))])),
        )]))),
        vec![(10, stream(image_dict(1, 1, "DeviceGray", 8), vec![255u8]))],
    );

    let image = doc
        .page_image(0, "Im1")
        .expect("page image lookup")
        .expect("Im1 should exist");
    assert_eq!(image.width, 1);
    assert_eq!(image.samples().unwrap().values, vec![1.0]);
    assert!(doc.page_image(0, "Missing").unwrap().is_none());
    assert!(doc.page_image(9, "Im1").is_err());
}

/// 7.4.1 / Table 5 + 7.3.10：`/Filter` 数组元素写成间接引用
/// （如 `[ /AHx 11 0 R ]`）时，规范名列表与全直接写法一致，解码数据与
/// 样本读取行为不变。
#[test]
fn image_filter_array_with_reference_element_matches_inline_form() {
    // ASCIIHex → RunLength 两级链：`0100FF80>` → 0x01 0x00 0xFF 0x80 →
    // 字面段 [0x00, 0xFF] + EOD。
    let encoded = b"0100FF80>".to_vec();
    let indirect = doc_with(vec![
        (
            10,
            stream(
                image_dict(2, 1, "DeviceGray", 8).insert(
                    "Filter",
                    Value::Array(Array(vec![Value::Name("AHx".into()), Value::Ref(11, 0)])),
                ),
                encoded.clone(),
            ),
        ),
        (11, Value::Name("RunLengthDecode".into())),
    ]);
    let direct = doc_with(vec![(
        10,
        stream(
            image_dict(2, 1, "DeviceGray", 8).insert(
                "Filter",
                Value::Array(Array(vec![
                    Value::Name("ASCIIHexDecode".into()),
                    Value::Name("RunLengthDecode".into()),
                ])),
            ),
            encoded,
        ),
    )]);

    let image = indirect
        .image(&Value::Ref(10, 0))
        .expect("间接引用元素形态的 /Filter 数组必须可解析");
    let baseline = direct
        .image(&Value::Ref(10, 0))
        .expect("全直接写法应作为基准可解析");
    assert_eq!(
        image.filters,
        vec!["ASCIIHexDecode".to_string(), "RunLengthDecode".to_string()]
    );
    assert_eq!(image.filters, baseline.filters);
    assert_eq!(image.data, baseline.data);
    assert_eq!(image.data, vec![0x00, 0xFF]);
    assert_eq!(
        image.samples().unwrap().values,
        baseline.samples().unwrap().values
    );
    assert_eq!(image.samples().unwrap().values, vec![0.0, 1.0]);
    assert!(
        indirect.warnings().is_empty(),
        "合法形态不得产诊断：{:?}",
        indirect.warnings()
    );
}

/// `/Filter` 元素引用指向缺失对象：产出 `7.4.1` 诊断并明确失败，
/// 绝不静默按「该元素不存在／无筛选器」误解码像素。
#[test]
fn image_filter_array_reference_to_absent_object_is_diagnosed_and_rejected() {
    let doc = doc_with(vec![(
        10,
        stream(
            image_dict(2, 1, "DeviceGray", 8).insert(
                "Filter",
                Value::Array(Array(vec![
                    Value::Name("ASCIIHexDecode".into()),
                    Value::Ref(99, 0),
                ])),
            ),
            b"0100FF80>".to_vec(),
        ),
    )]);
    let error = doc
        .image(&Value::Ref(10, 0))
        .expect_err("缺失对象的引用元素必须失败而非误解码");
    assert!(
        error
            .to_string()
            .contains("/Filter array has a non-name element"),
        "got: {error}"
    );
    assert!(
        doc.warnings()
            .iter()
            .any(|warning| { warning.contains("7.4.1") && warning.contains("absent object") }),
        "诊断缺失：{:?}",
        doc.warnings()
    );
}

/// `/Filter` 顶层引用成环：与 `Document::decode_stream_bytes` 同一立场，
/// 保留原值 + 环诊断 + 类型化失败（不 panic、不递归爆炸）。
#[test]
fn image_filter_cyclic_reference_is_diagnosed_and_rejected() {
    let doc = doc_with(vec![
        (
            10,
            stream(
                image_dict(2, 1, "DeviceGray", 8)
                    .insert("Filter", Value::Array(Array(vec![Value::Ref(11, 0)]))),
                b"0100FF80>".to_vec(),
            ),
        ),
        (11, Value::Ref(11, 0)),
    ]);
    assert!(
        doc.image(&Value::Ref(10, 0))
            .expect_err("环引用不得解析成功")
            .to_string()
            .contains("/Filter array has a non-name element"),
    );
    assert!(
        doc.warnings()
            .iter()
            .any(|warning| warning.contains("7.4.1") && warning.contains("cyclic")),
        "环诊断缺失：{:?}",
        doc.warnings()
    );
}
