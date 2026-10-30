use crate::core::reader::output_intent::{OutputIntentError, OutputIntentSubtype};
use crate::core::types::{Array, Dict, Document, Object, StreamData, Trailer, Value};

fn dict(entries: Vec<(&str, Value)>) -> Dict {
    let mut dict = Dict::new();
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    dict
}

fn hex_of(text: &str) -> String {
    text.bytes().map(|byte| format!("{byte:02X}")).collect()
}

fn catalog(output_intents: Option<Value>) -> Document {
    let mut entries = Dict::new().insert("Type", Value::Name("Catalog".into()));
    if let Some(value) = output_intents {
        entries = entries.insert("OutputIntents", value);
    }
    let mut document = Document::new();
    document.set_object(Object::new(1, 0, Value::Dict(entries)));
    document.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    document
}

fn document_with_entries(entries: Vec<Value>) -> Document {
    catalog(Some(Value::Array(Array(entries))))
}

fn with_object(document: Document, num: i64, value: Value) -> Document {
    document.set_object(Object::new(num, 0, value));
    document
}

fn profile(n: Option<i64>, data: &[u8]) -> Value {
    let mut entries = Dict::new().insert("Length", Value::Integer(data.len() as i64));
    if let Some(n) = n {
        entries = entries.insert("N", Value::Integer(n));
    }
    Value::Stream(entries, StreamData::Owned(data.to_vec()))
}

fn assert_malformed(document: &Document, needle: &str) {
    match document.output_intents() {
        Err(OutputIntentError::Malformed(message)) => {
            assert!(
                message.contains(needle),
                "expected {needle:?}, got {message}"
            );
        }
        other => panic!("expected malformed error containing {needle:?}, got {other:?}"),
    }
}

#[test]
fn output_intents_empty_by_default() {
    let document = catalog(None);
    assert_eq!(document.output_intents().expect("absent"), Vec::new());

    let missing = Document::new();
    assert_malformed(&missing, "catalog is missing or malformed");

    let document = catalog(Some(Value::Array(Array::new())));
    assert_eq!(document.output_intents().expect("empty"), Vec::new());
}

#[test]
fn output_intents_read_table365_entries() {
    let entry = Value::Dict(dict(vec![
        ("Type", Value::Name("OutputIntent".into())),
        ("S", Value::Name("GTS_PDFX".into())),
        ("OutputCondition", Value::String("Coated FOGRA39".into())),
        (
            "OutputConditionIdentifier",
            Value::HexString(hex_of("FOGRA39")),
        ),
        ("RegistryName", Value::String("http://www.color.org".into())),
        ("Info", Value::HexString(hex_of("ICC profile"))),
        ("DestOutputProfile", Value::Ref(5, 0)),
        ("CustomKey", Value::Integer(7)),
    ]));
    let unknown = Value::Dict(dict(vec![
        ("S", Value::Name("VENDOR_FOO".into())),
        (
            "OutputConditionIdentifier",
            Value::String("vendor condition".into()),
        ),
    ]));
    let document = with_object(
        document_with_entries(vec![Value::Ref(4, 0), unknown]),
        4,
        entry,
    );
    let document = with_object(document, 5, profile(Some(4), &[1, 2, 3]));

    let intents = document.output_intents().expect("output intents");
    assert_eq!(intents.len(), 2);

    let first = &intents[0];
    assert_eq!(first.subtype, OutputIntentSubtype::GtsPdfX);
    assert_eq!(first.subtype.name(), "GTS_PDFX");
    assert_eq!(first.output_condition.as_deref(), Some("Coated FOGRA39"));
    assert_eq!(first.output_condition_identifier, "FOGRA39");
    assert_eq!(first.registry_name.as_deref(), Some("http://www.color.org"));
    assert_eq!(first.info.as_deref(), Some("ICC profile"));
    assert_eq!(
        first.dict.get("CustomKey"),
        Some(&Value::Integer(7)),
        "原始字典应保留未建模条目"
    );
    let output_profile = first.destination_profile.as_ref().expect("profile");
    assert_eq!(output_profile.reference, Some((5, 0)));
    assert_eq!(output_profile.components, 4);
    assert_eq!(output_profile.data, vec![1, 2, 3]);

    let second = &intents[1];
    assert_eq!(
        second.subtype,
        OutputIntentSubtype::Other("VENDOR_FOO".into())
    );
    assert_eq!(second.output_condition, None);
    assert_eq!(second.registry_name, None);
    assert_eq!(second.info, None);
    assert_eq!(second.destination_profile, None);
    assert_eq!(
        second.dict.get("OutputConditionIdentifier"),
        Some(&Value::String("vendor condition".into()))
    );

    assert_eq!(
        OutputIntentSubtype::from_name("GTS_PDFA1").name(),
        "GTS_PDFA1"
    );
    assert_eq!(
        OutputIntentSubtype::from_name("ISO_PDFE1").name(),
        "ISO_PDFE1"
    );
}

#[test]
fn output_intents_reject_malformed_entries() {
    // /OutputIntents 不是数组。
    let document = catalog(Some(Value::Integer(1)));
    assert_malformed(&document, "/OutputIntents is not an array");

    // 条目不是字典。
    let document = document_with_entries(vec![Value::Integer(3)]);
    assert_malformed(&document, "is not a dictionary");

    // /Type 存在但不是 OutputIntent。
    let document = document_with_entries(vec![Value::Dict(dict(vec![
        ("Type", Value::Name("Foo".into())),
        ("S", Value::Name("GTS_PDFX".into())),
        ("OutputConditionIdentifier", Value::String("id".into())),
    ]))]);
    assert_malformed(&document, "/Type is not OutputIntent");

    // 缺 /S。
    let document = document_with_entries(vec![Value::Dict(dict(vec![(
        "OutputConditionIdentifier",
        Value::String("id".into()),
    )]))]);
    assert_malformed(&document, "has no /S");

    // /S 不是名称。
    let document = document_with_entries(vec![Value::Dict(dict(vec![
        ("S", Value::String("GTS_PDFX".into())),
        ("OutputConditionIdentifier", Value::String("id".into())),
    ]))]);
    assert_malformed(&document, "has no /S");

    // 缺 /OutputConditionIdentifier。
    let document = document_with_entries(vec![Value::Dict(dict(vec![(
        "S",
        Value::Name("GTS_PDFX".into()),
    )]))]);
    assert_malformed(&document, "has no /OutputConditionIdentifier");

    // /OutputConditionIdentifier 类型错误（非文本串）。
    let document = document_with_entries(vec![Value::Dict(dict(vec![
        ("S", Value::Name("GTS_PDFX".into())),
        ("OutputConditionIdentifier", Value::Integer(3)),
    ]))]);
    assert_malformed(&document, "expected a text string");

    // /DestOutputProfile 不是流。
    let document = document_with_entries(vec![Value::Dict(dict(vec![
        ("S", Value::Name("GTS_PDFX".into())),
        ("OutputConditionIdentifier", Value::String("id".into())),
        ("DestOutputProfile", Value::Integer(3)),
    ]))]);
    assert_malformed(&document, "/DestOutputProfile is not a stream");

    for n in [None, Some(0), Some(5)] {
        let document = with_object(
            document_with_entries(vec![Value::Dict(dict(vec![
                ("S", Value::Name("GTS_PDFX".into())),
                ("OutputConditionIdentifier", Value::String("id".into())),
                ("DestOutputProfile", Value::Ref(6, 0)),
            ]))]),
            6,
            profile(n, &[0; 4]),
        );
        match n {
            None => assert_malformed(&document, "has no integer /N"),
            Some(_) => assert_malformed(&document, "must be 1 to 4 components"),
        }
    }

    // 条目数上限 64。
    let entries = vec![
        Value::Dict(dict(vec![
            ("S", Value::Name("GTS_PDFA1".into())),
            ("OutputConditionIdentifier", Value::String("id".into())),
        ]));
        65
    ];
    match document_with_entries(entries).output_intents() {
        Err(OutputIntentError::LimitExceeded(what)) => assert_eq!(what, "output intents"),
        other => panic!("expected entry limit, got {other:?}"),
    }
}
