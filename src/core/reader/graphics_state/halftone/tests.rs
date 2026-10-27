use std::collections::BTreeMap;

use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};

use super::{CompositeHalftone, HalftoneModel, Type1Halftone, Type2Halftone};

fn document_with_objects(objects: Vec<(i64, Value)>) -> Document {
    let document = Document::new();
    for (num, value) in objects {
        document.set_object(Object::new(num, 0, value));
    }
    document
}

fn dict(entries: Vec<(&str, Value)>) -> Dict {
    let mut dict = Dict::new();
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    dict
}

fn function_dict() -> Value {
    Value::Dict(dict(vec![
        ("FunctionType", Value::Integer(2)),
        (
            "Domain",
            Value::Array(Array(vec![Value::Integer(0), Value::Integer(1)])),
        ),
        (
            "Range",
            Value::Array(Array(vec![Value::Integer(0), Value::Integer(1)])),
        ),
    ]))
}

fn function_stream() -> Value {
    Value::Stream(
        dict(vec![("Filter", Value::Name("ASCIIHexDecode".into()))]),
        StreamData::Owned(Vec::new()),
    )
}

fn assert_halftone_invalid(document: &Document, value: Value, needle: &str) {
    match document.halftone(&value) {
        Err(error) => {
            let message = error.to_string();
            assert!(
                message.contains(needle),
                "error {message:?} should mention {needle}"
            );
        }
        Ok(model) => panic!("expected an invalid halftone, got {model:?}"),
    }
}

#[test]
fn parses_type1_halftone_dictionary() {
    let document = document_with_objects(vec![(9, function_stream())]);
    let value = Value::Dict(dict(vec![
        ("Type", Value::Name("Halftone".into())),
        ("HalftoneType", Value::Integer(1)),
        ("XFrequency", Value::Float(187.5)),
        ("Angle", Value::Integer(45)),
        ("SpotFunction", Value::Ref(9, 0)),
        ("TransferFunction", function_dict()),
    ]));

    let model = document
        .halftone(&value)
        .expect("type 1 halftone should parse");

    match model {
        HalftoneModel::Type1(Type1Halftone {
            x_frequency,
            angle,
            spot_function,
            transfer_function,
        }) => {
            assert_eq!(x_frequency, 187.5);
            assert_eq!(angle, 45.0);
            assert_eq!(spot_function, function_stream());
            assert_eq!(transfer_function, Some(function_dict()));
        }
        other => panic!("expected a type 1 halftone, got {other:?}"),
    }
}

#[test]
fn parses_type2_halftone_with_uncompensated_flag() {
    let document = Document::new();
    let value = Value::Dict(dict(vec![
        ("HalftoneType", Value::Integer(2)),
        ("Frequency", Value::Integer(60)),
        ("Angle", Value::Float(75.5)),
        ("Width", Value::Integer(0)),
        ("Uncompensated", Value::Bool(true)),
        ("SpotFunction", function_dict()),
    ]));

    let model = document
        .halftone(&value)
        .expect("type 2 halftone should parse");

    match model {
        HalftoneModel::Type2(Type2Halftone {
            frequency,
            angle,
            width,
            uncompensated,
            spot_function,
            transfer_function,
        }) => {
            assert_eq!(frequency, 60.0);
            assert_eq!(angle, 75.5);
            assert_eq!(width, 0.0);
            assert!(uncompensated);
            assert_eq!(spot_function, function_dict());
            assert_eq!(transfer_function, None);
        }
        other => panic!("expected a type 2 halftone, got {other:?}"),
    }
}

#[test]
fn type3_to_type5_and_named_halftones_are_unmodeled() {
    let document = Document::new();
    for kind in 3..=5u8 {
        let value = Value::Dict(dict(vec![
            ("HalftoneType", Value::Integer(kind as i64)),
            ("ScreenList", Value::Array(Array(vec![Value::Ref(4, 0)]))),
        ]));
        let model = document
            .halftone(&value)
            .expect("type 3-5 should stay unmodeled");
        assert_eq!(model, HalftoneModel::Unmodeled(value));
    }
    let named = document
        .halftone(&Value::Name("70LineScreen".into()))
        .expect("named halftone reference should stay unmodeled");
    assert_eq!(
        named,
        HalftoneModel::Unmodeled(Value::Name("70LineScreen".into()))
    );
}

#[test]
fn parses_halftone_composite_dictionary() {
    let type3 = Value::Dict(dict(vec![
        ("HalftoneType", Value::Integer(3)),
        ("ScreenList", Value::Array(Array(vec![Value::Integer(1)]))),
    ]));
    let type1 = Value::Dict(dict(vec![
        ("HalftoneType", Value::Integer(1)),
        ("XFrequency", Value::Integer(85)),
        ("Angle", Value::Integer(0)),
        ("SpotFunction", function_dict()),
    ]));
    let document = document_with_objects(vec![(5, type3)]);
    let value = Value::Dict(dict(vec![
        ("DefaultHalftoneType", Value::Integer(1)),
        (
            "DefaultColorant",
            Value::Dict(dict(vec![("Cyan", Value::Integer(0))])),
        ),
        ("CY", Value::Ref(5, 0)),
        ("M", type1.clone()),
        (
            "K",
            Value::Array(Array(vec![Value::Integer(1), Value::Integer(2)])),
        ),
        ("*HalftoneType", Value::Integer(1)),
        ("Duplex", Value::Name("DuplexFlipY".into())),
    ]));

    let model = document
        .halftone(&value)
        .expect("composite halftone should parse");

    match model {
        HalftoneModel::Composite(CompositeHalftone {
            default_type,
            colorants,
            other,
        }) => {
            assert_eq!(default_type, Some(1));
            assert_eq!(
                colorants.get("CY"),
                Some(&HalftoneModel::Unmodeled(Value::Ref(5, 0))),
                "indirect unmodeled colorant keeps its written reference"
            );
            assert_eq!(
                colorants.get("K"),
                Some(&HalftoneModel::Unmodeled(Value::Array(Array(vec![
                    Value::Integer(1),
                    Value::Integer(2),
                ]))))
            );
            assert!(
                matches!(colorants.get("M"), Some(HalftoneModel::Type1(_))),
                "inline type 1 colorant must be typed"
            );
            assert_eq!(colorants.len(), 3);
            let mut expected_other: BTreeMap<String, Value> = BTreeMap::new();
            expected_other.insert(
                "DefaultColorant".into(),
                Value::Dict(dict(vec![("Cyan", Value::Integer(0))])),
            );
            expected_other.insert("*HalftoneType".into(), Value::Integer(1));
            expected_other.insert("Duplex".into(), Value::Name("DuplexFlipY".into()));
            assert_eq!(other, expected_other);
        }
        other => panic!("expected a composite halftone, got {other:?}"),
    }
}

#[test]
fn rejects_malformed_halftone_dictionaries() {
    let document = Document::new();
    let good_spot = function_dict();

    assert_halftone_invalid(
        &document,
        Value::Integer(4),
        "halftone must be a dictionary or name",
    );
    assert_halftone_invalid(
        &document,
        Value::Dict(dict(vec![("Frequency", Value::Integer(60))])),
        "requires /HalftoneType or /DefaultHalftoneType",
    );
    assert_halftone_invalid(
        &document,
        Value::Dict(dict(vec![("HalftoneType", Value::Integer(6))])),
        "/HalftoneType 6 is outside 1..=5",
    );
    assert_halftone_invalid(
        &document,
        Value::Dict(dict(vec![
            ("HalftoneType", Value::Integer(1)),
            ("XFrequency", Value::Integer(0)),
        ])),
        "/XFrequency must be positive",
    );
    assert_halftone_invalid(
        &document,
        Value::Dict(dict(vec![
            ("HalftoneType", Value::Integer(2)),
            ("Frequency", Value::Integer(60)),
            ("Angle", Value::Integer(0)),
            ("Width", Value::Integer(-1)),
            ("SpotFunction", good_spot.clone()),
        ])),
        "/Width must be non-negative",
    );
    assert_halftone_invalid(
        &document,
        Value::Dict(dict(vec![
            ("HalftoneType", Value::Integer(1)),
            ("XFrequency", Value::Integer(85)),
            ("Angle", Value::Integer(0)),
        ])),
        "/SpotFunction is required",
    );
    assert_halftone_invalid(
        &document,
        Value::Dict(dict(vec![
            ("HalftoneType", Value::Integer(2)),
            ("Frequency", Value::Integer(60)),
            ("Angle", Value::Integer(0)),
            ("Width", Value::Integer(1)),
            ("Uncompensated", Value::Integer(1)),
            ("SpotFunction", good_spot.clone()),
        ])),
        "/Uncompensated must be a boolean",
    );
    assert_halftone_invalid(
        &document,
        Value::Dict(dict(vec![
            ("DefaultHalftoneType", Value::Integer(9)),
            ("CY", good_spot),
        ])),
        "/DefaultHalftoneType must be an integer within 1..=5",
    );
}

#[test]
fn rejects_unresolved_halftone_reference() {
    let document = Document::new();
    let error = document
        .halftone(&Value::Ref(42, 0))
        .expect_err("missing object must fail");
    assert!(
        error
            .to_string()
            .contains("unresolved object reference 42 0 R")
    );
}
