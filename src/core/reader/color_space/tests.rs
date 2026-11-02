//! Color space reader model tests.

use super::*;
use crate::core::types::{Array, Dict, Document, Object, StreamData, Trailer, Value};

fn doc_with_objects(objects: Vec<(i64, Value)>) -> Document {
    let doc = Document::new();
    for (num, value) in objects {
        doc.set_object(Object::new(num, 0, value));
    }
    doc
}

fn space(doc: &Document, value: &Value) -> ColorSpaceModel {
    doc.color_space(value).unwrap()
}

fn cs_name(name: &str) -> Value {
    Value::Name(name.into())
}

fn cs_array(items: Vec<Value>) -> Value {
    Value::Array(Array(items))
}

fn reals(values: &[f64]) -> Value {
    Value::Array(Array(
        values.iter().map(|v| Value::Float(*v)).collect::<Vec<_>>(),
    ))
}

#[test]
fn device_names_parse_to_models() {
    let doc = Document::new();
    assert_eq!(
        space(&doc, &cs_name("DeviceGray")),
        ColorSpaceModel::DeviceGray
    );
    assert_eq!(
        space(&doc, &cs_name("DeviceRGB")),
        ColorSpaceModel::DeviceRGB
    );
    assert_eq!(
        space(&doc, &cs_name("DeviceCMYK")),
        ColorSpaceModel::DeviceCMYK
    );
    assert_eq!(
        space(&doc, &cs_name("Pattern")),
        ColorSpaceModel::Pattern { base: None }
    );
    assert_eq!(ColorSpaceModel::DeviceCMYK.component_count(), Some(4));
    assert_eq!(
        ColorSpaceModel::DeviceRGB.native_ranges(),
        vec![(0.0, 1.0); 3]
    );
    assert_eq!(
        ColorSpaceModel::Pattern { base: None }.component_count(),
        None
    );
}

#[test]
fn cal_gray_parses_with_defaults() {
    let doc = Document::new();
    let value = cs_array(vec![
        cs_name("CalGray"),
        Value::Dict(Dict::new().insert("WhitePoint", reals(&[1.0, 1.0, 1.0]))),
    ]);
    let model = space(&doc, &value);
    let ColorSpaceModel::CalGray(cal) = model else {
        panic!("expected CalGray");
    };
    assert_eq!(cal.white_point, vec![1.0, 1.0, 1.0]);
    assert_eq!(cal.black_point, vec![0.0, 0.0, 0.0]);
    assert_eq!(cal.gamma, 1.0);

    let missing = cs_array(vec![cs_name("CalGray"), Value::Dict(Dict::new())]);
    assert!(matches!(
        doc.color_space(&missing),
        Err(ColorSpaceError::Malformed(message)) if message.contains("WhitePoint")
    ));
}

#[test]
fn cal_rgb_parses_gamma_and_matrix() {
    let doc = Document::new();
    let dict = Dict::new()
        .insert("WhitePoint", reals(&[0.9505, 1.0, 1.089]))
        .insert("Gamma", reals(&[2.2, 2.2, 2.2]))
        .insert(
            "Matrix",
            reals(&[
                0.4497, 0.2446, 0.165, 0.3162, 0.6720, 0.0118, 0.1034, 0.0824, 0.7482,
            ]),
        );
    let model = space(&doc, &cs_array(vec![cs_name("CalRGB"), Value::Dict(dict)]));
    let ColorSpaceModel::CalRGB(cal) = model else {
        panic!("expected CalRGB");
    };
    assert_eq!(cal.gamma, vec![2.2, 2.2, 2.2]);
    assert_eq!(cal.matrix.len(), 9);
    assert_eq!(cal.black_point, vec![0.0, 0.0, 0.0]);
}

#[test]
fn lab_range_defaults_and_custom() {
    let doc = Document::new();
    let base = Dict::new().insert("WhitePoint", reals(&[1.0, 1.0, 1.0]));
    let model = space(&doc, &cs_array(vec![cs_name("Lab"), Value::Dict(base)]));
    let ColorSpaceModel::Lab(lab) = model else {
        panic!("expected Lab");
    };
    assert_eq!(lab.range, vec![(-100.0, 100.0), (-100.0, 100.0)]);
    assert_eq!(
        ColorSpaceModel::Lab(lab.clone()).native_ranges(),
        vec![(0.0, 100.0), (-100.0, 100.0), (-100.0, 100.0)]
    );

    let swapped = Dict::new()
        .insert("WhitePoint", reals(&[1.0, 1.0, 1.0]))
        .insert("Range", reals(&[128.0, -128.0, 107.0, -107.0]));
    let model = space(&doc, &cs_array(vec![cs_name("Lab"), Value::Dict(swapped)]));
    let ColorSpaceModel::Lab(lab) = model else {
        panic!("expected Lab");
    };
    assert_eq!(lab.range, vec![(-128.0, 128.0), (-107.0, 107.0)]);
}

#[test]
fn icc_based_reads_profile_channels_and_defaults() {
    let profile = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
    let stream = Value::Stream(
        Dict::new().insert("N", Value::Integer(3)),
        StreamData::Owned(profile.clone()),
    );
    let doc = doc_with_objects(vec![(7, stream)]);
    let model = space(&doc, &cs_array(vec![cs_name("ICCBased"), Value::Ref(7, 0)]));
    assert_eq!(model.component_count(), Some(3));
    let ColorSpaceModel::IccBased(icc) = model else {
        panic!("expected ICCBased");
    };
    assert_eq!(icc.n, 3);
    assert_eq!(icc.profile, profile);
    assert_eq!(icc.range, vec![(0.0, 1.0); 3]);
    assert_eq!(*icc.alternate, ColorSpaceModel::DeviceRGB);
}

#[test]
fn icc_based_honours_range_and_alternate_and_rejects_bad_n() {
    let stream = Value::Stream(
        Dict::new()
            .insert("N", Value::Integer(1))
            .insert("Range", reals(&[0.0, 0.8]))
            .insert("Alternate", cs_name("DeviceGray")),
        StreamData::Owned(vec![0xAA]),
    );
    let doc = doc_with_objects(vec![(9, stream)]);
    let model = space(&doc, &cs_array(vec![cs_name("ICCBased"), Value::Ref(9, 0)]));
    let ColorSpaceModel::IccBased(icc) = model else {
        panic!("expected ICCBased");
    };
    assert_eq!(icc.range, vec![(0.0, 0.8)]);
    assert_eq!(*icc.alternate, ColorSpaceModel::DeviceGray);

    let bad = Value::Stream(
        Dict::new().insert("N", Value::Integer(9)),
        StreamData::Owned(vec![]),
    );
    let doc = doc_with_objects(vec![(10, bad)]);
    assert!(matches!(
        doc.color_space(&cs_array(vec![cs_name("ICCBased"), Value::Ref(10, 0)])),
        Err(ColorSpaceError::Malformed(message)) if message.contains("/N")
    ));
}

#[test]
fn indexed_parses_hex_lookup_and_decodes_samples() {
    let doc = Document::new();
    // 2 palette entries for DeviceRGB base.
    let lookup = Value::HexString("FF0000 00FF00".into());
    let model = space(
        &doc,
        &cs_array(vec![
            cs_name("Indexed"),
            cs_name("DeviceRGB"),
            Value::Integer(1),
            lookup,
        ]),
    );
    let ColorSpaceModel::Indexed(indexed) = model.clone() else {
        panic!("expected Indexed");
    };
    assert_eq!(indexed.hival, 1);
    assert_eq!(indexed.lookup, vec![0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00]);
    assert_eq!(indexed.base, Box::new(ColorSpaceModel::DeviceRGB));
    assert_eq!(model.component_count(), Some(1));

    let samples = model.decode_samples(8, 2, &[0, 1]).unwrap();
    let expected = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    for (actual, want) in samples.iter().zip(expected) {
        assert!((actual - want).abs() < 1e-9, "got {actual}, want {want}");
    }
    // out-of-range indices clamp to hival
    let samples = model.decode_samples(8, 1, &[200]).unwrap();
    assert!((samples[1] - 1.0).abs() < 1e-9);
}

#[test]
fn indexed_decodes_four_bit_packed_gray_indices() {
    let doc = Document::new();
    let model = space(
        &doc,
        &cs_array(vec![
            cs_name("Indexed"),
            cs_name("DeviceGray"),
            Value::Integer(3),
            Value::String("\u{10}\u{20}\u{30}\u{40}".into()),
        ]),
    );
    let ColorSpaceModel::Indexed(indexed) = model.clone() else {
        panic!("expected Indexed");
    };
    assert_eq!(indexed.lookup, vec![0x10, 0x20, 0x30, 0x40]);
    // one byte holds indices 1 and 3 (MSB-first nibbles)
    let samples = model.decode_samples(4, 2, &[0x13]).unwrap();
    assert_eq!(samples.len(), 2);
    assert!((samples[0] - 0x20 as f64 / 255.0).abs() < 1e-9);
    assert!((samples[1] - 0x40 as f64 / 255.0).abs() < 1e-9);
}

#[test]
fn indexed_rejects_invalid_base_and_short_palette() {
    let doc = Document::new();
    let separation_base = cs_array(vec![
        cs_name("Separation"),
        cs_name("Spot1"),
        cs_name("DeviceRGB"),
        Value::Null,
    ]);
    let nested = cs_array(vec![
        cs_name("Indexed"),
        separation_base,
        Value::Integer(1),
        Value::HexString("00".into()),
    ]);
    assert!(matches!(
        doc.color_space(&nested),
        Err(ColorSpaceError::Unsupported(_))
    ));

    let short = cs_array(vec![
        cs_name("Indexed"),
        cs_name("DeviceRGB"),
        Value::Integer(5),
        Value::HexString("FF0000".into()),
    ]);
    assert!(matches!(
        doc.color_space(&short),
        Err(ColorSpaceError::Malformed(message)) if message.contains("at least")
    ));
}

#[test]
fn separation_evaluates_type2_tint() {
    let tint = Value::Dict(
        Dict::new()
            .insert("FunctionType", Value::Integer(2))
            .insert("Domain", reals(&[0.0, 1.0]))
            .insert("C0", reals(&[0.0, 0.0, 0.0]))
            .insert("C1", reals(&[1.0, 1.0, 1.0]))
            .insert("N", Value::Float(1.0)),
    );
    let doc = Document::new();
    let model = space(
        &doc,
        &cs_array(vec![
            cs_name("Separation"),
            cs_name("PantoneRed"),
            cs_name("DeviceRGB"),
            tint,
        ]),
    );
    let ColorSpaceModel::Separation(separation) = model.clone() else {
        panic!("expected Separation");
    };
    assert_eq!(separation.colorant, "PantoneRed");
    assert!(separation.tint.is_some());

    let samples = model.decode_samples(8, 1, &[0x80]).unwrap();
    assert_eq!(samples.len(), 3);
    let mid = 0x80 as f64 / 255.0;
    for value in &samples {
        assert!((value - mid).abs() < 1e-9);
    }
}

#[test]
fn separation_without_tint_passes_through() {
    let doc = Document::new();
    let model = space(
        &doc,
        &cs_array(vec![
            cs_name("Separation"),
            cs_name("None"),
            cs_name("DeviceGray"),
            Value::Null,
        ]),
    );
    let ColorSpaceModel::Separation(separation) = model.clone() else {
        panic!("expected Separation");
    };
    assert!(separation.tint.is_none());
    let samples = model.decode_samples(8, 1, &[0xFF]).unwrap();
    assert_eq!(samples, vec![1.0]);
}

#[test]
fn devicen_evaluates_type4_tint() {
    let tint = Value::Stream(
        Dict::new()
            .insert("FunctionType", Value::Integer(4))
            .insert("Domain", reals(&[0.0, 1.0, 0.0, 1.0]))
            .insert("Range", reals(&[0.0, 1.0])),
        StreamData::Owned(b"exch add 2 div".to_vec()),
    );
    let doc = Document::new();
    let model = space(
        &doc,
        &cs_array(vec![
            cs_name("DeviceN"),
            Value::Array(Array(vec![cs_name("Cyan"), cs_name("Magenta")])),
            cs_name("DeviceGray"),
            tint,
        ]),
    );
    let ColorSpaceModel::DeviceN(device_n) = model.clone() else {
        panic!("expected DeviceN");
    };
    assert_eq!(device_n.colorants, vec!["Cyan", "Magenta"]);
    assert_eq!(model.component_count(), Some(2));

    let samples = model.decode_samples(8, 1, &[0x00, 0xFF]).unwrap();
    assert_eq!(samples.len(), 1);
    assert!((samples[0] - 0.5).abs() < 1e-9);
}

#[test]
fn malformed_and_unsupported_spaces_are_typed_errors() {
    let doc = Document::new();
    assert!(matches!(
        doc.color_space(&cs_array(vec![cs_name("NChannel"), cs_name("DeviceRGB"), Value::Null, Value::Null, Value::Null])),
        Err(ColorSpaceError::Unsupported(message)) if message.contains("NChannel")
    ));
    assert!(matches!(
        doc.color_space(&Value::Integer(5)),
        Err(ColorSpaceError::Malformed(_))
    ));
    assert!(matches!(
        doc.color_space(&cs_name("Lab")),
        Err(ColorSpaceError::Unsupported(_))
    ));
    assert!(matches!(
        doc.color_space(&cs_array(vec![
            cs_name("Separation"),
            cs_name("X"),
            cs_name("DeviceRGB")
        ])),
        Err(ColorSpaceError::Malformed(_))
    ));
    assert!(matches!(
        doc.color_space(&cs_array(vec![cs_name("DeviceN"), Value::Array(Array(vec![])), cs_name("DeviceRGB"), Value::Null])),
        Err(ColorSpaceError::Malformed(message)) if message.contains("1..=8")
    ));
}

#[test]
fn pattern_space_parses_optional_base() {
    let doc = Document::new();
    assert_eq!(
        space(&doc, &cs_array(vec![cs_name("Pattern")])),
        ColorSpaceModel::Pattern { base: None }
    );
    let model = space(
        &doc,
        &cs_array(vec![cs_name("Pattern"), cs_name("DeviceRGB")]),
    );
    assert_eq!(
        model,
        ColorSpaceModel::Pattern {
            base: Some(Box::new(ColorSpaceModel::DeviceRGB))
        }
    );
    assert!(matches!(
        doc.color_space(&cs_array(vec![
            cs_name("Pattern"),
            cs_array(vec![cs_name("Pattern")]),
        ])),
        Err(ColorSpaceError::Malformed(_))
    ));
}

#[test]
fn device_gray_decodes_sub_byte_and_16_bit_samples() {
    let doc = Document::new();
    let model = space(&doc, &cs_name("DeviceGray"));
    // 4-bit: two pixels per byte, MSB first
    let samples = model.decode_samples(4, 2, &[0x3F]).unwrap();
    assert!((samples[0] - 3.0 / 15.0).abs() < 1e-9);
    assert!((samples[1] - 1.0).abs() < 1e-9);
    // 16-bit big endian
    let samples = model.decode_samples(16, 1, &[0x01, 0x00]).unwrap();
    assert!((samples[0] - 256.0 / 65535.0).abs() < 1e-9);
    // partial rows rejected
    assert!(matches!(
        model.decode_samples(4, 3, &[0xFF]),
        Err(ColorSpaceError::Malformed(_))
    ));
    // 16-bit rejected for Indexed
    let indexed = space(
        &doc,
        &cs_array(vec![
            cs_name("Indexed"),
            cs_name("DeviceGray"),
            Value::Integer(0),
            Value::HexString("80".into()),
        ]),
    );
    assert!(matches!(
        indexed.decode_samples(16, 1, &[0, 1]),
        Err(ColorSpaceError::Malformed(_))
    ));
}

fn make_page_doc(resources: Dict) -> Document {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                .insert("Count", Value::Integer(1)),
        ),
    ));
    doc.set_object(Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Page".into()))
                .insert("Parent", Value::Ref(2, 0))
                .insert("Resources", Value::Dict(resources)),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(4)),
    };
    doc
}

#[test]
fn page_color_space_and_defaults_resolve_through_resources() {
    let resources = Dict::new()
        .insert(
            "ColorSpace",
            Value::Dict(Dict::new().insert("CS1", cs_name("DeviceRGB")).insert(
                "Spot",
                cs_array(vec![
                    cs_name("Separation"),
                    cs_name("PANTONE Red 032 C"),
                    cs_name("DeviceCMYK"),
                    Value::Null,
                ]),
            )),
        )
        .insert("DefaultGray", cs_name("DeviceGray"));
    let doc = make_page_doc(resources);

    let model = doc.page_color_space(0, "CS1").unwrap().unwrap();
    assert_eq!(model, ColorSpaceModel::DeviceRGB);
    let model = doc.page_color_space(0, "Spot").unwrap().unwrap();
    let ColorSpaceModel::Separation(separation) = model else {
        panic!("expected Separation");
    };
    assert_eq!(separation.colorant, "PANTONE Red 032 C");
    assert_eq!(*separation.alternate, ColorSpaceModel::DeviceCMYK);
    assert!(doc.page_color_space(0, "Nope").unwrap().is_none());

    let default = doc
        .page_default_color_space(0, DefaultColorSpace::Gray)
        .unwrap()
        .unwrap();
    assert_eq!(default, ColorSpaceModel::DeviceGray);
    assert!(
        doc.page_default_color_space(0, DefaultColorSpace::Cmyk)
            .unwrap()
            .is_none()
    );
    assert!(doc.page_color_space(9, "CS1").is_err());
}
