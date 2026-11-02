//! `/ImageMask`、`/Mask`（显式与颜色键）、`/SMask`、`/SMaskInData` 与
//! `/Matte` 的建模与校验。

use super::support::{dict, doc_with, image_dict, stream};
use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::reader::image::{ImageError, ImageMaskModel};
use crate::core::types::{Array, Value};

#[test]
fn soft_mask_and_smask_in_data_are_parsed() {
    let doc = doc_with(vec![
        (
            10,
            stream(
                image_dict(2, 2, "DeviceRGB", 8)
                    .insert("SMask", Value::Ref(11, 0))
                    .insert("SMaskInData", Value::Integer(1)),
                vec![0u8; 12],
            ),
        ),
        (
            11,
            stream(image_dict(2, 2, "DeviceGray", 8), vec![0u8, 64, 128, 255]),
        ),
        (12, stream(image_dict(1, 1, "DeviceGray", 8), vec![0u8])),
    ]);

    let image = doc.image(&Value::Ref(10, 0)).expect("image should parse");
    let smask = image.smask.expect("SMask should be parsed");
    assert_eq!(smask.width, 2);
    assert_eq!(smask.height, 2);
    assert_eq!(smask.color_space, Some(ColorSpaceModel::DeviceGray));
    assert_eq!(image.smask_in_data, Some(1));

    let mismatched = doc_with(vec![
        (
            20,
            stream(
                image_dict(2, 2, "DeviceRGB", 8).insert("SMask", Value::Ref(21, 0)),
                vec![0u8; 12],
            ),
        ),
        (21, stream(image_dict(1, 1, "DeviceGray", 8), vec![0u8])),
    ]);
    let error = mismatched.image(&Value::Ref(20, 0)).unwrap_err();
    assert!(
        error.to_string().contains("SMask dimensions"),
        "unexpected error: {error}"
    );

    let bad_flag = doc_with(vec![(
        30,
        stream(
            image_dict(1, 1, "DeviceGray", 8).insert("SMaskInData", Value::Integer(3)),
            vec![0u8],
        ),
    )]);
    assert!(matches!(
        bad_flag.image(&Value::Ref(30, 0)),
        Err(ImageError::Invalid(message)) if message.contains("SMaskInData")
    ));
}

#[test]
fn explicit_and_color_key_masks_are_parsed() {
    let doc = doc_with(vec![
        (
            10,
            stream(
                image_dict(2, 1, "DeviceRGB", 8).insert("Mask", Value::Ref(11, 0)),
                vec![1u8, 2, 3, 4, 5, 6],
            ),
        ),
        (
            11,
            stream(
                dict(vec![
                    ("Type", Value::Name("XObject".into())),
                    ("Subtype", Value::Name("Image".into())),
                    ("Width", Value::Integer(4)),
                    ("Height", Value::Integer(1)),
                    ("ImageMask", Value::Bool(true)),
                    ("BitsPerComponent", Value::Integer(1)),
                ]),
                [0b1000_0000u8],
            ),
        ),
        (
            20,
            stream(
                image_dict(2, 1, "DeviceRGB", 8).insert(
                    "Mask",
                    Value::Array(Array(vec![
                        Value::Integer(0),
                        Value::Integer(10),
                        Value::Integer(20),
                        Value::Integer(30),
                        Value::Integer(40),
                        Value::Integer(50),
                    ])),
                ),
                vec![0u8, 10, 20, 30, 40, 50],
            ),
        ),
        (
            30,
            stream(
                image_dict(2, 1, "DeviceRGB", 8).insert("Mask", Value::Ref(31, 0)),
                vec![0u8; 6],
            ),
        ),
        (31, stream(image_dict(1, 1, "DeviceGray", 8), vec![0u8])),
    ]);

    let explicit = doc.image(&Value::Ref(10, 0)).expect("image");
    let Some(ImageMaskModel::Explicit(mask)) = explicit.mask else {
        panic!("expected an explicit mask, got {:?}", explicit.mask);
    };
    assert!(mask.image_mask);
    assert_eq!((mask.width, mask.height), (4, 1));
    assert_eq!(mask.samples().unwrap().values, vec![1.0, 0.0, 0.0, 0.0]);

    let keyed = doc.image(&Value::Ref(20, 0)).expect("image");
    assert_eq!(
        keyed.mask,
        Some(ImageMaskModel::ColorKey(vec![0, 10, 20, 30, 40, 50]))
    );

    let not_a_mask = doc.image(&Value::Ref(30, 0)).unwrap_err();
    assert!(
        not_a_mask.to_string().contains("image mask"),
        "unexpected error: {not_a_mask}"
    );
}

#[test]
fn color_key_mask_length_is_validated() {
    let doc = doc_with(vec![(
        10,
        stream(
            image_dict(1, 1, "DeviceRGB", 8).insert(
                "Mask",
                Value::Array(Array(vec![Value::Integer(0), Value::Integer(10)])),
            ),
            vec![0u8; 3],
        ),
    )]);
    let error = doc.image(&Value::Ref(10, 0)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("color key /Mask must contain 6 integers"),
        "unexpected error: {error}"
    );
}

#[test]
fn image_reference_cycles_are_bounded() {
    let doc = doc_with(vec![(
        10,
        stream(
            image_dict(1, 1, "DeviceGray", 8).insert("Mask", Value::Ref(10, 0)),
            vec![0u8],
        ),
    )]);
    let error = doc.image(&Value::Ref(10, 0)).unwrap_err();
    assert!(
        error.to_string().contains("nesting"),
        "unexpected error: {error}"
    );
}

#[test]
fn image_mask_decode_must_be_zero_one_or_one_zero() {
    let doc = doc_with(vec![(
        10,
        stream(
            dict(vec![
                ("Width", Value::Integer(1)),
                ("Height", Value::Integer(1)),
                ("ImageMask", Value::Bool(true)),
                (
                    "Decode",
                    Value::Array(Array(vec![Value::Float(0.0), Value::Float(0.5)])),
                ),
            ]),
            vec![0b1000_0000],
        ),
    )]);
    let error = doc.image(&Value::Ref(10, 0)).unwrap_err();
    assert!(
        error.to_string().contains("image mask /Decode"),
        "unexpected error: {error}"
    );
}

#[test]
fn image_matte_is_parsed_on_soft_mask() {
    // 基图 1x1 RGB；其 SMask 带 /Matte（3 个分量，按基图颜色空间解释）。
    let smask = stream(
        image_dict(1, 1, "DeviceGray", 8).insert(
            "Matte",
            Value::Array(Array(vec![
                Value::Float(0.1),
                Value::Float(0.2),
                Value::Float(0.3),
            ])),
        ),
        vec![128u8],
    );
    let base = image_dict(1, 1, "DeviceRGB", 8).insert("SMask", Value::Ref(11, 0));
    let doc = doc_with(vec![(10, stream(base, vec![0u8; 3])), (11, smask)]);

    let image = doc.image(&Value::Ref(10, 0)).expect("image should parse");
    let smask = image.smask.expect("image should have an SMask");
    assert_eq!(smask.matte, Some(vec![0.1, 0.2, 0.3]));
    assert!(
        !smask.other.contains_key("Matte"),
        "Matte should not fall through to other"
    );
}

#[test]
fn smask_matte_length_must_match_base_components() {
    let smask = stream(
        image_dict(1, 1, "DeviceGray", 8)
            .insert("Matte", Value::Array(Array(vec![Value::Float(0.5)]))),
        vec![128u8],
    );
    let base = image_dict(1, 1, "DeviceRGB", 8).insert("SMask", Value::Ref(11, 0));
    let doc = doc_with(vec![(10, stream(base, vec![0u8; 3])), (11, smask)]);

    let error = doc.image(&Value::Ref(10, 0)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("SMask /Matte must contain 3 numbers"),
        "unexpected error: {error}"
    );
}

#[test]
fn image_matte_rejects_non_finite_and_image_masks() {
    let bad = stream(
        image_dict(1, 1, "DeviceGray", 8)
            .insert("Matte", Value::Array(Array(vec![Value::Float(f64::NAN)]))),
        vec![128u8],
    );
    let doc = doc_with(vec![(10, bad)]);
    let error = doc.image(&Value::Ref(10, 0)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("/Matte must contain finite numbers"),
        "unexpected error: {error}"
    );

    let mask = stream(
        dict(vec![
            ("Width", Value::Integer(1)),
            ("Height", Value::Integer(1)),
            ("ImageMask", Value::Bool(true)),
            ("Matte", Value::Array(Array(vec![Value::Float(0.5)]))),
        ]),
        vec![0b1000_0000],
    );
    let doc = doc_with(vec![(10, mask)]);
    let error = doc.image(&Value::Ref(10, 0)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("image mask must not have a /Matte"),
        "unexpected error: {error}"
    );
}
