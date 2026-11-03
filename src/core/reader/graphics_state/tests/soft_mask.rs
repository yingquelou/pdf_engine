//! Table 144 软遮罩字典：`/S` `/G` `/BC` `/TR` `/SMaskInData` `/Matte`
//! 的解析与逐项拒绝。

use super::*;

#[test]
fn parses_soft_mask_dictionary_from_ext_g_state() {
    let document = document_with_objects(vec![
        (
            5,
            Value::Dict(state(vec![
                ("S", Value::Name("Alpha".into())),
                ("G", Value::Ref(7, 0)),
                (
                    "BC",
                    Value::Array(Array(vec![
                        Value::Integer(0),
                        Value::Float(0.5),
                        Value::Integer(1),
                    ])),
                ),
                ("TR", Value::Name("Identity".into())),
            ])),
        ),
        (6, Value::Dict(state(vec![("SMask", Value::Ref(5, 0))]))),
        (7, xobject_stream(Some("Form"))),
        (8, xobject_stream(Some("Image"))),
    ]);

    let gs = document
        .ext_g_state(&Value::Ref(6, 0))
        .expect("graphics state should parse");
    let raw = gs.soft_mask.as_ref().expect("/SMask should be kept");
    let mask = document.soft_mask(raw).expect("soft mask should parse");

    assert_eq!(mask.subtype, SoftMaskSubtype::Alpha);
    assert_eq!(mask.group, Value::Ref(7, 0));
    assert_eq!(mask.backdrop, Some(vec![0.0, 0.5, 1.0]));
    assert_eq!(mask.transfer, Some(Value::Name("Identity".into())));

    let image_mask = document
        .soft_mask(&Value::Dict(state(vec![
            ("S", Value::Name("Luminosity".into())),
            ("G", Value::Ref(8, 0)),
        ])))
        .expect("image /G is allowed");
    assert_eq!(image_mask.subtype, SoftMaskSubtype::Luminosity);
    assert_eq!(image_mask.backdrop, None);
    assert_eq!(image_mask.transfer, None);
}

#[test]
fn rejects_malformed_soft_masks() {
    let objects = vec![
        (7, xobject_stream(Some("Form"))),
        (8, Value::Integer(1)),
        (9, xobject_stream(None)),
        (10, xobject_stream(Some("Weird"))),
    ];

    let assert_soft_mask_invalid = |entries: Vec<(&str, Value)>, needle: &str| {
        let document = document_with_objects(objects.clone());
        match document.soft_mask(&Value::Dict(state(entries))) {
            Err(GraphicsStateError::Invalid(message)) => assert!(
                message.contains(needle),
                "error {message:?} should mention {needle}"
            ),
            other => panic!("expected an invalid soft mask, got {other:?}"),
        }
    };

    assert_soft_mask_invalid(vec![], "soft mask /S is required");
    assert_soft_mask_invalid(
        vec![("S", Value::Name("Luminance".into()))],
        "soft mask /S must be /Alpha or /Luminosity",
    );
    assert_soft_mask_invalid(
        vec![("S", Value::Name("Alpha".into()))],
        "soft mask /G is required",
    );
    assert_soft_mask_invalid(
        vec![("S", Value::Name("Alpha".into())), ("G", Value::Ref(8, 0))],
        "soft mask /G must be a Form or Image XObject stream",
    );
    for num in [9, 10] {
        assert_soft_mask_invalid(
            vec![
                ("S", Value::Name("Alpha".into())),
                ("G", Value::Ref(num, 0)),
            ],
            "soft mask /G must be a Form or Image XObject stream",
        );
    }

    let form = || ("G", Value::Ref(7, 0));
    assert_soft_mask_invalid(
        vec![
            ("S", Value::Name("Alpha".into())),
            form(),
            ("BC", Value::Integer(0)),
        ],
        "soft mask /BC must be an array",
    );
    assert_soft_mask_invalid(
        vec![
            ("S", Value::Name("Alpha".into())),
            form(),
            ("BC", Value::Array(Array::new())),
        ],
        "soft mask /BC must not be empty",
    );
    assert_soft_mask_invalid(
        vec![
            ("S", Value::Name("Alpha".into())),
            form(),
            ("BC", Value::Array(Array(vec![Value::Name("x".into())]))),
        ],
        "soft mask /BC entries must be numbers",
    );
    assert_soft_mask_invalid(
        vec![
            ("S", Value::Name("Alpha".into())),
            form(),
            ("BC", Value::Array(Array(vec![Value::Float(1.5)]))),
        ],
        "soft mask /BC entries must be within 0..=1",
    );
    assert_soft_mask_invalid(
        vec![
            ("S", Value::Name("Alpha".into())),
            form(),
            ("TR", Value::Name("Multiply".into())),
        ],
        "soft mask /TR name must be /Identity",
    );

    let document = Document::new();
    match document.soft_mask(&Value::Integer(1)) {
        Err(GraphicsStateError::Invalid(message)) => assert!(
            message.contains("soft mask must be a dictionary"),
            "{message}"
        ),
        other => panic!("expected an invalid soft mask, got {other:?}"),
    }
}

#[test]
fn parses_soft_mask_transfer_function_smask_in_data_and_matte() {
    let transfer = Value::Dict(state(vec![
        ("FunctionType", Value::Integer(2)),
        (
            "Domain",
            Value::Array(Array(vec![Value::Integer(0), Value::Integer(1)])),
        ),
        (
            "Range",
            Value::Array(Array(vec![Value::Integer(0), Value::Integer(1)])),
        ),
    ]));
    let document = document_with_objects(vec![
        (7, xobject_stream(Some("Form"))),
        (9, transfer.clone()),
    ]);

    let mask = document
        .soft_mask(&Value::Dict(state(vec![
            ("S", Value::Name("Luminosity".into())),
            ("G", Value::Ref(7, 0)),
            ("TR", Value::Ref(9, 0)),
            ("SMaskInData", Value::Integer(2)),
            (
                "Matte",
                Value::Array(Array(vec![Value::Float(0.1), Value::Integer(0)])),
            ),
            ("Foo", Value::Integer(7)),
        ])))
        .expect("extended soft mask should parse");

    assert_eq!(mask.transfer, Some(transfer));
    assert_eq!(mask.smask_in_data, Some(2));
    assert_eq!(mask.matte, Some(vec![0.1, 0.0]));
    assert_eq!(mask.other.get("Foo"), Some(&Value::Integer(7)));
}

#[test]
fn rejects_invalid_smask_in_data_and_matte() {
    let objects = vec![(7, xobject_stream(Some("Form")))];
    let assert_invalid_mask = |extra: Vec<(&str, Value)>, needle: &str| {
        let document = document_with_objects(objects.clone());
        let mut entries = vec![("S", Value::Name("Alpha".into())), ("G", Value::Ref(7, 0))];
        entries.extend(extra);
        match document.soft_mask(&Value::Dict(state(entries))) {
            Err(GraphicsStateError::Invalid(message)) => assert!(
                message.contains(needle),
                "error {message:?} should mention {needle}"
            ),
            other => panic!("expected an invalid soft mask, got {other:?}"),
        }
    };

    assert_invalid_mask(
        vec![("SMaskInData", Value::Integer(3))],
        "soft mask /SMaskInData must be 0, 1 or 2",
    );
    assert_invalid_mask(
        vec![("SMaskInData", Value::Name("Alpha".into()))],
        "soft mask /SMaskInData must be an integer",
    );
    assert_invalid_mask(
        vec![("Matte", Value::Array(Array(Vec::new())))],
        "soft mask /Matte must not be empty",
    );
    assert_invalid_mask(
        vec![(
            "Matte",
            Value::Array(Array(vec![Value::Name("Red".into())])),
        )],
        "soft mask /Matte entries must be numbers",
    );
}
