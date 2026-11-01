//! Image reader model tests.

use super::{ImageError, ImageMaskModel, JpxColorSpace};
use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::reader::content::InlineImage;
use crate::core::types::{Array, Dict, Document, Object, StreamData, Trailer, Value};

fn dict(entries: Vec<(&str, Value)>) -> Dict {
    let mut dict = Dict::new();
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    dict
}

fn stream(dict: Dict, data: impl Into<Vec<u8>>) -> Value {
    Value::Stream(dict, StreamData::Owned(data.into()))
}

fn doc_with(objects: Vec<(i64, Value)>) -> Document {
    let doc = Document::new();
    for (num, value) in objects {
        doc.set_object(Object::new(num, 0, value));
    }
    doc
}

fn image_dict(width: i64, height: i64, space: &str, bits: i64) -> Dict {
    dict(vec![
        ("Type", Value::Name("XObject".into())),
        ("Subtype", Value::Name("Image".into())),
        ("Width", Value::Integer(width)),
        ("Height", Value::Integer(height)),
        ("ColorSpace", Value::Name(space.into())),
        ("BitsPerComponent", Value::Integer(bits)),
    ])
}

/// Catalog 1 → Pages 2 → Page 3；页面可选带 `/Resources`。
fn document_with_page(resources: Option<Value>, objects: Vec<(i64, Value)>) -> Document {
    let mut page = dict(vec![
        ("Type", Value::Name("Page".into())),
        ("Parent", Value::Ref(2, 0)),
    ]);
    if let Some(resources) = resources {
        page = page.insert("Resources", resources);
    }
    let doc = doc_with(objects);
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(dict(vec![
            ("Type", Value::Name("Catalog".into())),
            ("Pages", Value::Ref(2, 0)),
        ])),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(dict(vec![
            ("Type", Value::Name("Pages".into())),
            ("Kids", Value::Array(Array(vec![Value::Ref(3, 0)]))),
            ("Count", Value::Integer(1)),
        ])),
    ));
    doc.set_object(Object::new(3, 0, Value::Dict(page)));
    let mut doc = doc;
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(4)),
    };
    doc
}

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
fn inline_image_parses_abbreviated_keys_and_decodes_filters() {
    let doc = Document::new();
    let image = doc
        .inline_image(&InlineImage {
            dict: vec![
                ("W".into(), Value::Float(3.0)),
                ("H".into(), Value::Float(1.0)),
                ("BPC".into(), Value::Float(8.0)),
                ("CS".into(), Value::Name("G".into())),
                ("F".into(), Value::Name("AHx".into())),
            ],
            data: b"00FF7F>".to_vec(),
        })
        .expect("inline image should parse");
    assert_eq!((image.width, image.height), (3, 1));
    assert_eq!(image.color_space, Some(ColorSpaceModel::DeviceGray));
    assert_eq!(image.bits_per_component, 8);
    assert_eq!(image.filters, vec!["ASCIIHexDecode".to_string()]);
    assert_eq!(image.data, vec![0x00, 0xFF, 0x7F]);
    assert_eq!(
        image.samples().unwrap().values,
        vec![0.0, 1.0, 127.0 / 255.0]
    );
}

#[test]
fn inline_image_mask_defaults_bits_and_has_no_color_space() {
    let doc = Document::new();
    let image = doc
        .inline_image(&InlineImage {
            dict: vec![
                ("W".into(), Value::Float(2.0)),
                ("H".into(), Value::Float(1.0)),
                ("IM".into(), Value::Bool(true)),
            ],
            data: vec![0b1000_0000],
        })
        .expect("inline image mask should parse");
    assert!(image.image_mask);
    assert_eq!(image.color_space, None);
    assert_eq!(image.bits_per_component, 1);
    assert_eq!(image.samples().unwrap().values, vec![1.0, 0.0]);
}

#[test]
fn page_inline_image_resolves_named_color_space() {
    let indexed_space = Value::Array(Array(vec![
        Value::Name("Indexed".into()),
        Value::Name("DeviceRGB".into()),
        Value::Integer(1),
        Value::HexString("FF000000FF00".into()),
    ]));
    let resources = Value::Dict(dict(vec![(
        "ColorSpace",
        Value::Dict(dict(vec![("CS0", indexed_space)])),
    )]));
    let doc = document_with_page(Some(resources), Vec::new());

    let inline = InlineImage {
        dict: vec![
            ("W".into(), Value::Float(2.0)),
            ("H".into(), Value::Float(1.0)),
            ("BPC".into(), Value::Float(8.0)),
            ("CS".into(), Value::Name("CS0".into())),
        ],
        data: vec![0, 1],
    };
    let image = doc
        .page_inline_image(0, &inline)
        .expect("page resources should resolve /CS0");
    assert!(matches!(
        image.color_space,
        Some(ColorSpaceModel::Indexed(_))
    ));
    assert_eq!(
        image.samples().unwrap().values,
        vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
    );

    assert!(
        doc.inline_image(&inline).is_err(),
        "without page context the resource name must not resolve"
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

// ---------------------------------------------------------------------------
// §7.4.9 JPXDecode：语法层元数据
// ---------------------------------------------------------------------------

/// 手写最小 JP2 文件：签名框 + jp2h{ihdr, colr(sRGB)}，可选 jp2c 码流。
/// `ihdr` 声明 `height`×`width`、3 分量、8 位无符号（BPC 7）。
fn sample_jp2(width: u32, height: u32) -> Vec<u8> {
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&3u16.to_be_bytes());
    ihdr.extend_from_slice(&[7u8, 7, 0, 0]);
    let ihdr = jp2_box(b"ihdr", &ihdr);
    let mut colr = vec![1u8, 0, 1];
    colr.extend_from_slice(&16u32.to_be_bytes()); // EnumCS 16 = sRGB
    let colr = jp2_box(b"colr", &colr);
    let jp2h = jp2_box(b"jp2h", &[ihdr, colr].concat());
    let mut bytes = vec![
        0x00, 0x00, 0x00, 0x0C, b'j', b'P', b' ', b' ', 0x0D, 0x0A, 0x87, 0x0A,
    ];
    bytes.extend_from_slice(&jp2h);
    bytes
}

fn jp2_box(kind: &[u8; 4], content: &[u8]) -> Vec<u8> {
    let mut bytes = (content.len() as u32 + 8).to_be_bytes().to_vec();
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(content);
    bytes
}

fn jpx_image_dict(width: u32, height: u32) -> Dict {
    dict(vec![
        ("Type", Value::Name("XObject".into())),
        ("Subtype", Value::Name("Image".into())),
        ("Width", Value::Integer(width as i64)),
        ("Height", Value::Integer(height as i64)),
        ("Filter", Value::Name("JPXDecode".into())),
    ])
}

#[test]
fn jpx_image_derives_bits_and_color_space_from_header() {
    let jp2 = sample_jp2(8, 6);
    let doc = doc_with(vec![(10, stream(jpx_image_dict(8, 6), jp2.clone()))]);
    let image = doc
        .image(&Value::Ref(10, 0))
        .expect("jpx image without /BPC and /ColorSpace should parse");
    assert_eq!(image.filters, vec!["JPXDecode".to_string()]);
    // JPX 步骤透传：data 是原始 JPX 字节而不是样本。
    assert_eq!(image.data, jp2);
    let header = image.jpx.clone().expect("jp2 header should parse");
    assert_eq!((header.width, header.height), (8, 6));
    assert_eq!(header.component_count(), 3);
    assert_eq!(header.bits_per_component(), Some(8));
    // 缺 /BPC：以头部位深为准；缺 /ColorSpace：枚举 sRGB → DeviceRGB。
    assert_eq!(image.bits_per_component, 8);
    assert_eq!(image.color_space, Some(ColorSpaceModel::DeviceRGB));
    assert_eq!(image.component_count(), Some(3));
    assert!(matches!(
        header.color_space,
        Some(JpxColorSpace::Enumerated { code: 16, .. })
    ));
}

#[test]
fn jpx_ignores_declared_bits_and_keeps_declared_color_space() {
    let mut jp2 = sample_jp2(4, 4);
    // 追加一段裸码流 jp2c（SIZ 4×4、8 位单分量），验证 SIZ 优先路径。
    let mut codestream = vec![0xFF, 0x4F, 0xFF, 0x51];
    codestream.extend_from_slice(&41u16.to_be_bytes());
    codestream.extend_from_slice(&0u16.to_be_bytes());
    for value in [4u32, 4, 0, 0, 4, 4, 0, 0] {
        codestream.extend_from_slice(&value.to_be_bytes());
    }
    codestream.extend_from_slice(&1u16.to_be_bytes());
    codestream.extend_from_slice(&[7u8, 1, 1]);
    jp2.extend_from_slice(&jp2_box(b"jp2c", &codestream));

    let doc = doc_with(vec![(
        10,
        stream(
            jpx_image_dict(4, 4)
                .insert("BitsPerComponent", Value::Integer(1))
                .insert("ColorSpace", Value::Name("DeviceGray".into())),
            jp2,
        ),
    )]);
    let image = doc.image(&Value::Ref(10, 0)).expect("jpx image parses");
    // Table 89：/BPC「存在时必须忽略」；/ColorSpace 声明值仍然生效。
    assert_eq!(image.bits_per_component, 8);
    assert_eq!(image.color_space, Some(ColorSpaceModel::DeviceGray));
    let header = image.jpx.expect("codestream parsed");
    assert_eq!((header.width, header.height), (4, 4));
    assert_eq!(header.reference_offset, Some((0, 0)));
    // 声明空间与头部提示互不覆盖：头部仍记录枚举 sRGB。
    assert_eq!(header.pdf_color_space_hint(), Some("DeviceRGB"));
}

#[test]
fn jpx_payload_without_parsable_header_falls_back_to_dict_entries() {
    let doc = doc_with(vec![(
        10,
        stream(
            image_dict(2, 2, "DeviceRGB", 8).insert("Filter", Value::Name("JPX".into())),
            b"definitely not a jpx payload",
        ),
    )]);
    let image = doc
        .image(&Value::Ref(10, 0))
        .expect("unparsable jpx payload must not fail image parsing");
    assert!(image.jpx.is_none());
    assert_eq!(image.bits_per_component, 8);
    assert_eq!(image.color_space, Some(ColorSpaceModel::DeviceRGB));
    // 旧名 /JPX 同样命中透传路径。
    assert_eq!(image.filters, vec!["JPX".to_string()]);
}

#[test]
fn jpx_samples_are_rejected_as_unsupported() {
    let doc = doc_with(vec![(10, stream(jpx_image_dict(8, 6), sample_jp2(8, 6)))]);
    let image = doc.image(&Value::Ref(10, 0)).expect("jpx image parses");
    let error = image.samples().expect_err("jp2c bytes are not raw samples");
    assert!(
        matches!(error, ImageError::Unsupported(_)),
        "unexpected error: {error}"
    );
    assert!(
        error.to_string().contains("JPXDecode"),
        "unexpected error: {error}"
    );
}

#[test]
fn flated_then_jpx_passes_through_the_raw_jpx_bytes() {
    use flate2::write::ZlibEncoder;
    use std::io::Write;
    let jp2 = sample_jp2(8, 6);
    let mut enc = ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(&jp2).expect("compress");
    let compressed = enc.finish().expect("finish");
    let filter = Value::Array(Array(vec![
        Value::Name("FlateDecode".into()),
        Value::Name("JPXDecode".into()),
    ]));
    let doc = doc_with(vec![(
        10,
        stream(jpx_image_dict(8, 6).insert("Filter", filter), compressed),
    )]);
    let image = doc.image(&Value::Ref(10, 0)).expect("chain parses");
    // 链上 Flate 步骤照常解码，JPX 步骤保留原始字节。
    assert_eq!(image.data, jp2);
    assert_eq!(
        image.filters,
        vec!["FlateDecode".to_string(), "JPXDecode".to_string()]
    );
    assert_eq!(
        image.jpx.map(|header| (header.width, header.height)),
        Some((8, 6))
    );
}

#[test]
fn page_image_exposes_jpx_geometry_through_public_api() {
    let doc = document_with_page(
        Some(Value::Dict(dict(vec![(
            "XObject",
            Value::Dict(dict(vec![("Im1", Value::Ref(10, 0))])),
        )]))),
        vec![(10, stream(jpx_image_dict(10, 5), sample_jp2(10, 5)))],
    );
    let image = doc
        .page_image(0, "Im1")
        .expect("lookup")
        .expect("image exists");
    let header = image.jpx.expect("jp2 header parsed");
    assert_eq!((image.width, image.height), (10, 5));
    assert_eq!((header.width, header.height), (10, 5));
    assert_eq!(header.bits_per_component(), Some(8));
    assert_eq!(image.bits_per_component, 8);
    assert_eq!(image.color_space, Some(ColorSpaceModel::DeviceRGB));
}
