//! 样本读取 API（[`ImageModel::samples`]）：解码、`/Decode` 映射与长度校验。

use super::support::{dict, doc_with, image_dict, stream};
use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::types::{Array, Value};

#[test]
fn parses_rgb_image_dictionary_and_samples() {
    let doc = doc_with(vec![(
        10,
        stream(
            image_dict(2, 2, "DeviceRGB", 8),
            [255u8, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255],
        ),
    )]);

    let image = doc.image(&Value::Ref(10, 0)).expect("image should parse");
    assert_eq!(image.width, 2);
    assert_eq!(image.height, 2);
    assert_eq!(image.color_space, Some(ColorSpaceModel::DeviceRGB));
    assert_eq!(image.bits_per_component, 8);
    assert_eq!(image.component_count(), Some(3));
    assert!(!image.image_mask);
    assert!(image.mask.is_none() && image.smask.is_none());
    assert!(!image.interpolate);
    assert_eq!(image.intent, None);
    assert!(image.filters.is_empty());

    let samples = image.samples().expect("samples should decode");
    assert_eq!(samples.width, 2);
    assert_eq!(samples.height, 2);
    assert_eq!(samples.components, 3);
    assert_eq!(samples.ranges, vec![(0.0, 1.0); 3]);
    assert_eq!(samples.values.len(), 12);
    assert_eq!(samples.pixel(0, 0), Some(&[1.0, 0.0, 0.0][..]));
    assert_eq!(samples.pixel(1, 0), Some(&[0.0, 1.0, 0.0][..]));
    assert_eq!(samples.pixel(0, 1), Some(&[0.0, 0.0, 1.0][..]));
    assert_eq!(samples.pixel(1, 1), Some(&[1.0, 1.0, 1.0][..]));
    assert_eq!(samples.pixel(2, 0), None);
}

#[test]
fn decode_array_scales_and_inverts_samples() {
    let doc = doc_with(vec![(
        10,
        stream(
            image_dict(2, 1, "DeviceGray", 8).insert(
                "Decode",
                Value::Array(Array(vec![Value::Integer(1), Value::Integer(0)])),
            ),
            [0u8, 255],
        ),
    )]);
    let samples = doc
        .image(&Value::Ref(10, 0))
        .unwrap()
        .samples()
        .expect("samples");
    assert_eq!(samples.ranges, vec![(1.0, 0.0)]);
    assert_eq!(samples.values, vec![1.0, 0.0]);
}

#[test]
fn indexed_explicit_decode_remaps_palette_indices() {
    let indexed = Value::Array(Array(vec![
        Value::Name("Indexed".into()),
        Value::Name("DeviceRGB".into()),
        Value::Integer(1),
        Value::HexString("FF000000FF00".into()),
    ]));
    let doc = doc_with(vec![(
        10,
        stream(
            image_dict(2, 1, "DeviceGray", 8)
                .insert("ColorSpace", indexed)
                .insert(
                    "Decode",
                    Value::Array(Array(vec![Value::Integer(1), Value::Integer(0)])),
                ),
            [0u8, 255],
        ),
    )]);
    let samples = doc
        .image(&Value::Ref(10, 0))
        .unwrap()
        .samples()
        .expect("samples");
    assert_eq!(samples.ranges, vec![(0.0, 1.0); 3]);
    // 反向映射：索引 0 → 1（绿），索引 255 → 0（红）。
    assert_eq!(samples.pixel(0, 0), Some(&[0.0, 1.0, 0.0][..]));
    assert_eq!(samples.pixel(1, 0), Some(&[1.0, 0.0, 0.0][..]));
}

#[test]
fn separation_explicit_decode_maps_tint_inputs() {
    let separation = Value::Array(Array(vec![
        Value::Name("Separation".into()),
        Value::Name("None".into()),
        Value::Name("DeviceGray".into()),
        Value::Null,
    ]));
    let doc = doc_with(vec![(
        10,
        stream(
            image_dict(2, 1, "DeviceGray", 8)
                .insert("ColorSpace", separation)
                .insert(
                    "Decode",
                    Value::Array(Array(vec![Value::Integer(1), Value::Integer(0)])),
                ),
            [0u8, 255],
        ),
    )]);
    let samples = doc
        .image(&Value::Ref(10, 0))
        .unwrap()
        .samples()
        .expect("samples");
    assert_eq!(samples.ranges, vec![(0.0, 1.0)]);
    assert_eq!(samples.values, vec![1.0, 0.0]);
}

#[test]
fn special_color_space_decode_length_is_validated() {
    let separation = Value::Array(Array(vec![
        Value::Name("Separation".into()),
        Value::Name("None".into()),
        Value::Name("DeviceGray".into()),
        Value::Null,
    ]));
    let doc = doc_with(vec![(
        10,
        stream(
            image_dict(1, 1, "DeviceGray", 8)
                .insert("ColorSpace", separation)
                .insert(
                    "Decode",
                    Value::Array(Array(vec![
                        Value::Integer(0),
                        Value::Integer(1),
                        Value::Integer(0),
                        Value::Integer(1),
                    ])),
                ),
            [0u8],
        ),
    )]);
    // 长度校验发生在解析期（`parse.rs` 按 `component_count() * 2` 核对），
    // 因此错误由 `Document::image` 抛出，而不是 `samples()`。
    let error = doc.image(&Value::Ref(10, 0)).unwrap_err();
    assert!(
        error.to_string().contains("must contain 2 numbers"),
        "unexpected error: {error}"
    );
}

#[test]
fn lab_default_decode_uses_range() {
    let lab = Value::Array(Array(vec![
        Value::Name("Lab".into()),
        Value::Dict(dict(vec![(
            "WhitePoint",
            Value::Array(Array(vec![
                Value::Float(0.9505),
                Value::Float(1.0),
                Value::Float(1.089),
            ])),
        )])),
    ]));
    let doc = doc_with(vec![(
        10,
        stream(
            image_dict(1, 1, "DeviceGray", 8).insert("ColorSpace", lab),
            [255u8, 0, 255],
        ),
    )]);
    let samples = doc
        .image(&Value::Ref(10, 0))
        .unwrap()
        .samples()
        .expect("samples");
    assert_eq!(
        samples.ranges,
        vec![(0.0, 100.0), (-100.0, 100.0), (-100.0, 100.0)]
    );
    assert_eq!(samples.values, vec![100.0, -100.0, 100.0]);
}

#[test]
fn image_data_length_mismatch_is_invalid() {
    let short = doc_with(vec![(
        10,
        stream(image_dict(2, 2, "DeviceRGB", 8), vec![0u8; 9]),
    )]);
    let error = short
        .image(&Value::Ref(10, 0))
        .unwrap()
        .samples()
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("does not match expected 12 bytes"),
        "unexpected error: {error}"
    );

    let long = doc_with(vec![(
        11,
        stream(image_dict(2, 2, "DeviceRGB", 8), vec![0u8; 13]),
    )]);
    assert!(long.image(&Value::Ref(11, 0)).unwrap().samples().is_err());

    let mask = doc_with(vec![(
        12,
        stream(
            dict(vec![
                ("Width", Value::Integer(8)),
                ("Height", Value::Integer(1)),
                ("ImageMask", Value::Bool(true)),
            ]),
            Vec::new(),
        ),
    )]);
    let error = mask
        .image(&Value::Ref(12, 0))
        .unwrap()
        .samples()
        .unwrap_err();
    assert!(
        error.to_string().contains("image mask data length"),
        "unexpected error: {error}"
    );
}
