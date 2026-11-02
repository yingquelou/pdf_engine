use super::*;

#[test]
fn type3_font_parses_font_matrix_bbox_widths_and_char_procs() {
    let document = Document::new();
    let font = type3_font(differences_encoding());
    let parsed = document.type3_font(&font).unwrap().unwrap();

    assert_eq!(parsed.font_matrix, [0.001, 0.0, 0.0, 0.001, 0.0, 0.0]);
    assert_eq!(parsed.font_bbox, [0.0, 0.0, 1000.0, 1000.0]);
    assert_eq!(parsed.first_char, 65);
    assert_eq!(parsed.widths, vec![500.0, 600.0]);
    assert_eq!(parsed.char_procs, vec!["A".to_string(), "B".to_string()]);
    assert_eq!(parsed.width_for(65), Some(500.0));
    assert_eq!(parsed.width_for(66), Some(600.0));
    assert_eq!(parsed.width_for(64), None);
    assert_eq!(parsed.width_for(67), None);

    // The same entries resolved through indirect objects.
    let document = Document::new();
    document.set_object(Object::new(
        10,
        0,
        Value::Array(
            Array::new()
                .push(Value::Float(0.002))
                .push(Value::Integer(0))
                .push(Value::Integer(0))
                .push(Value::Float(0.002))
                .push(Value::Integer(0))
                .push(Value::Integer(0)),
        ),
    ));
    document.set_object(Object::new(
        11,
        0,
        Value::Array(
            Array::new()
                .push(Value::Integer(0))
                .push(Value::Integer(0))
                .push(Value::Integer(2000))
                .push(Value::Integer(2000)),
        ),
    ));
    document.set_object(Object::new(
        12,
        0,
        Value::Dict(Dict::new().insert("A", char_proc())),
    ));
    document.set_object(Object::new(
        13,
        0,
        Value::Array(Array::new().push(Value::Integer(750))),
    ));
    let indirect = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type3".into()))
            .insert("FontBBox", Value::Ref(11, 0))
            .insert("FontMatrix", Value::Ref(10, 0))
            .insert("CharProcs", Value::Ref(12, 0))
            .insert("FirstChar", Value::Integer(65))
            .insert("LastChar", Value::Integer(65))
            .insert("Widths", Value::Ref(13, 0))
            .insert("Encoding", Value::Name("WinAnsiEncoding".into())),
    );
    let parsed = document.type3_font(&indirect).unwrap().unwrap();
    assert_eq!(parsed.font_matrix, [0.002, 0.0, 0.0, 0.002, 0.0, 0.0]);
    assert_eq!(parsed.font_bbox, [0.0, 0.0, 2000.0, 2000.0]);
    assert_eq!(parsed.widths, vec![750.0]);
    assert_eq!(parsed.char_procs, vec!["A".to_string()]);
    assert_eq!(parsed.width_for(65), Some(750.0));
}

#[test]
fn type3_font_requires_char_procs() {
    let document = Document::new();

    let mut dict = type3_dict(differences_encoding());
    dict.0.remove("CharProcs");
    assert_contains(
        document.type3_font(&Value::Dict(dict)).unwrap_err(),
        "/CharProcs",
    );

    let mut dict = type3_dict(differences_encoding());
    dict.0.remove("FontMatrix");
    assert_contains(
        document.type3_font(&Value::Dict(dict)).unwrap_err(),
        "/FontMatrix",
    );

    let mut dict = type3_dict(differences_encoding());
    dict.0.remove("FontBBox");
    assert_contains(
        document.type3_font(&Value::Dict(dict)).unwrap_err(),
        "/FontBBox",
    );

    let mut dict = type3_dict(differences_encoding());
    dict.0.remove("Encoding");
    assert_contains(
        document.type3_font(&Value::Dict(dict)).unwrap_err(),
        "/Encoding",
    );

    // Non-Type-3 dictionaries are not Type 3 fonts.
    let other = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Helvetica".into())),
    );
    assert!(document.type3_font(&other).unwrap().is_none());
}
