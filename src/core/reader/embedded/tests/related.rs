//! `/EF` 载体字典与 `/RF` 关联文件数组测试（7.11.3 Table 44、7.11.4.2）。

use super::*;

#[test]
fn ef_carrier_dictionary_parses_each_stream() {
    let doc = doc_without_names();
    doc.set_object(embedded_stream(80, Dict::new(), b"ascii"));
    doc.set_object(embedded_stream(
        81,
        Dict::new().insert("Subtype", name("text/plain")),
        b"unicode",
    ));
    let filespec = filespec_dict(vec![
        ("F", Value::String("a.txt".into())),
        (
            "EF",
            Value::Dict(
                Dict::new()
                    .insert("F", Value::Ref(80, 0))
                    .insert("UF", Value::Ref(81, 0)),
            ),
        ),
    ]);

    let parsed = doc.filespec(&filespec).expect("filespec");
    assert_eq!(parsed.embedded.len(), 2);
    assert_eq!(parsed.embedded[0].data, b"ascii");
    assert_eq!(parsed.embedded[1].data, b"unicode");
    assert_eq!(parsed.embedded[1].subtype.as_deref(), Some("text/plain"));
}

#[test]
fn related_files_parse_pairs_with_platform_entries() {
    let doc = doc_without_names();
    doc.set_object(embedded_stream(80, Dict::new(), b"main.eps"));
    doc.set_object(embedded_stream(81, Dict::new(), b"sunset-dos"));
    doc.set_object(embedded_stream(82, Dict::new(), b"sunset-mac"));
    let filespec = filespec_dict(vec![
        ("F", Value::String("SUNSET.EPS".into())),
        (
            "EF",
            Value::Dict(Dict::new().insert("F", Value::Ref(80, 0))),
        ),
        ("FS", name("URL")),
        ("V", Value::Bool(true)),
        (
            "ID",
            arr(vec![
                Value::HexString("00112233445566778899AABBCCDDEEFF".into()),
                Value::HexString("FFEEDDCCBBAA99887766554433221100".into()),
            ]),
        ),
        (
            "RF",
            Value::Dict(Dict::new().insert(
                "F",
                arr(vec![
                    Value::String("SUNSET.DOS".into()),
                    Value::Ref(81, 0),
                    Value::String("SUNSET.MAC".into()),
                    Value::Ref(82, 0),
                ]),
            )),
        ),
    ]);

    let parsed = doc.filespec(&filespec).expect("filespec");
    assert_eq!(parsed.file_system.as_deref(), Some("URL"));
    assert_eq!(parsed.volatile, Some(true));
    let id = parsed.file_id.as_ref().expect("/ID parsed");
    assert_eq!(
        id[0],
        vec![
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD,
            0xEE, 0xFF
        ]
    );
    assert_eq!(id[1].len(), 16);
    assert_eq!(parsed.primary().expect("EF stream").data, b"main.eps");

    let related = parsed.related_files.get("F").expect("/RF F");
    assert_eq!(related.len(), 2);
    assert_eq!(related[0].name, "SUNSET.DOS");
    assert_eq!(related[0].file.data, b"sunset-dos");
    assert_eq!(related[1].name, "SUNSET.MAC");
    assert_eq!(related[1].file.data, b"sunset-mac");
}

#[test]
fn rf_requires_ef_and_matching_keys() {
    let doc = doc_without_names();
    doc.set_object(embedded_stream(80, Dict::new(), b"x"));
    let pair = arr(vec![Value::String("related".into()), Value::Ref(80, 0)]);

    // `/RF` 存在时 `/EF` 必填（Table 44）。
    let missing_ef = filespec_dict(vec![
        ("F", Value::String("x".into())),
        ("RF", Value::Dict(Dict::new().insert("F", pair.clone()))),
    ]);
    assert!(matches!(
        doc.filespec(&missing_ef),
        Err(EmbeddedFileError::Malformed(_))
    ));

    // `/RF` 的每个键都须出现在 `/EF` 中。
    let missing_key = filespec_dict(vec![
        ("F", Value::String("x".into())),
        (
            "EF",
            Value::Dict(Dict::new().insert("F", Value::Ref(80, 0))),
        ),
        ("RF", Value::Dict(Dict::new().insert("Mac", pair.clone()))),
    ]);
    assert!(matches!(
        doc.filespec(&missing_key),
        Err(EmbeddedFileError::Malformed(_))
    ));
}

#[test]
fn malformed_related_file_arrays_are_typed_errors() {
    let doc = doc_without_names();
    doc.set_object(embedded_stream(80, Dict::new(), b"x"));

    // 奇数个元素不是 2n 对。
    let odd = filespec_dict(vec![
        ("F", Value::String("x".into())),
        (
            "EF",
            Value::Dict(Dict::new().insert("F", Value::Ref(80, 0))),
        ),
        (
            "RF",
            Value::Dict(Dict::new().insert(
                "F",
                arr(vec![Value::String("related".into()), Value::Ref(80, 0)]),
            )),
        ),
    ]);
    assert!(doc.filespec(&odd).is_ok(), "two-element pair is valid");

    let odd = filespec_dict(vec![
        ("F", Value::String("x".into())),
        (
            "EF",
            Value::Dict(Dict::new().insert("F", Value::Ref(80, 0))),
        ),
        (
            "RF",
            Value::Dict(Dict::new().insert(
                "F",
                arr(vec![
                    Value::String("a".into()),
                    Value::Ref(80, 0),
                    Value::String("b".into()),
                ]),
            )),
        ),
    ]);
    assert!(matches!(
        doc.filespec(&odd),
        Err(EmbeddedFileError::Malformed(_))
    ));

    // 名称不是文本串、文件不是流，均为结构错误。
    for bad_pair in [
        arr(vec![Value::Integer(1), Value::Ref(80, 0)]),
        arr(vec![Value::String("a".into()), Value::Integer(7)]),
    ] {
        let filespec = filespec_dict(vec![
            ("F", Value::String("x".into())),
            (
                "EF",
                Value::Dict(Dict::new().insert("F", Value::Ref(80, 0))),
            ),
            ("RF", Value::Dict(Dict::new().insert("F", bad_pair))),
        ]);
        assert!(matches!(
            doc.filespec(&filespec),
            Err(EmbeddedFileError::Malformed(_))
        ));
    }
}

#[test]
fn malformed_platform_entries_are_typed_errors() {
    let doc = doc_without_names();
    for (key, value) in [
        ("FS", Value::Integer(1)),
        ("V", Value::Integer(1)),
        ("DOS", Value::Integer(1)),
        ("Mac", Value::Array(Array::new())),
        ("Unix", Value::Bool(false)),
        ("ID", arr(vec![Value::String("only-one".into())])),
    ] {
        let filespec = filespec_dict(vec![("F", Value::String("x".into())), (key, value)]);
        assert!(
            matches!(
                doc.filespec(&filespec),
                Err(EmbeddedFileError::Malformed(_))
            ),
            "/{key} should be rejected"
        );
    }
}
