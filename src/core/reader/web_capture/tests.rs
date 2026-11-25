//! Web Capture（14.10）读取单元测试。

use crate::core::types::{Array, Dict, Document, Object, Trailer, Value};

use super::{ContentSet, UrlAliases, WebCaptureError};

fn base_document(content_set: Value, source: Value) -> (Document, Vec<u8>) {
    let id: Vec<u8> = vec![0x01, 0x02, 0x03, 0xFF];
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0))
                .insert("SpiderInfo", Value::Ref(5, 0))
                .insert(
                    "Names",
                    Value::Dict(
                        Dict::new()
                            .insert(
                                "URLS",
                                Value::Dict(
                                    Dict::new().insert(
                                        "Names",
                                        Value::Array(
                                            Array::new()
                                                .push(Value::String("http://example.com/".into()))
                                                .push(Value::Ref(6, 0)),
                                        ),
                                    ),
                                ),
                            )
                            .insert(
                                "IDS",
                                Value::Dict(
                                    Dict::new().insert(
                                        "Names",
                                        Value::Array(
                                            Array::new()
                                                .push(Value::String(
                                                    id.iter().map(|b| *b as char).collect(),
                                                ))
                                                .push(Value::Ref(6, 0)),
                                        ),
                                    ),
                                ),
                            ),
                    ),
                ),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array::new())),
        ),
    ));
    doc.set_object(Object::new(
        5,
        0,
        Value::Dict(
            Dict::new()
                .insert("V", Value::Float(1.0))
                .insert("C", Value::Array(Array::new())),
        ),
    ));
    doc.set_object(Object::new(6, 0, content_set));
    doc.set_object(Object::new(7, 0, source));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    (doc, id)
}

fn page_set() -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("SpiderContentSet".into()))
            .insert("S", Value::Name("SPS".into()))
            .insert("ID", Value::HexString("010203FF".into()))
            .insert("O", Value::Array(Array::new().push(Value::Ref(3, 0))))
            .insert("SI", Value::Ref(7, 0))
            .insert("CT", Value::String("text/html".into()))
            .insert("T", Value::String("Example".into()))
            .insert("TID", Value::HexString("0A0B".into())),
    )
}

fn source() -> Value {
    Value::Dict(
        Dict::new()
            .insert("AU", Value::String("http://example.com/".into()))
            .insert("S", Value::Integer(1)),
    )
}

fn command_document(command: Value) -> Document {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0))
                .insert("SpiderInfo", Value::Ref(5, 0)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array::new())),
        ),
    ));
    doc.set_object(Object::new(
        5,
        0,
        Value::Dict(
            Dict::new()
                .insert("V", Value::Float(1.0))
                .insert("C", Value::Array(Array::new().push(Value::Ref(8, 0)))),
        ),
    ));
    doc.set_object(Object::new(8, 0, command));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc
}

#[test]
fn web_capture_reads_command_settings() {
    let doc = Document::new();
    let value = Value::Dict(
        Dict::new()
            .insert("G", Value::Dict(Dict::new().insert("X", Value::Integer(1))))
            .insert(
                "C",
                Value::Dict(Dict::new().insert(
                    "acme:foo:1:html",
                    Value::Dict(Dict::new().insert("Y", Value::Integer(2))),
                )),
            ),
    );
    let settings = doc
        .web_capture_command_settings(&value)
        .expect("settings")
        .expect("present");
    assert_eq!(
        settings.global.as_ref().and_then(|g| g.get("X")),
        Some(&Value::Integer(1))
    );
    assert_eq!(
        settings
            .engines
            .get("acme:foo:1:html")
            .and_then(|engine| engine.get("Y")),
        Some(&Value::Integer(2))
    );

    let malformed = Value::Dict(Dict::new().insert("G", Value::Integer(1)));
    let error = doc
        .web_capture_command_settings(&malformed)
        .expect_err("/G must be a dictionary");
    assert!(matches!(error, WebCaptureError::Malformed(_)), "{error}");
}

#[test]
fn web_capture_reads_command_dictionary() {
    let command = Value::Dict(
        Dict::new()
            .insert("URL", Value::String("http://example.com/".into()))
            .insert("L", Value::Integer(2))
            .insert("F", Value::Integer(0b011))
            .insert("CT", Value::String("text/html".into()))
            .insert("H", Value::String("X-Test: 1".into()))
            .insert("P", Value::String("a=1".into())),
    );
    let doc = command_document(command);
    let info = doc.web_capture_info().expect("info").expect("present");
    let commands = doc.web_capture_commands(&info).expect("commands");
    assert_eq!(commands.len(), 1);
    let command = &commands[0];
    assert_eq!(command.url, "http://example.com/");
    assert_eq!(command.levels, 2);
    assert!(command.same_site());
    assert!(command.same_path());
    assert!(!command.submit());
    assert_eq!(command.content_type.as_deref(), Some("text/html"));
    assert_eq!(command.headers.as_deref(), Some("X-Test: 1"));
    assert_eq!(command.posted, Some(Value::String("a=1".into())));
}

#[test]
fn web_capture_command_defaults_and_rejections_are_typed() {
    let minimal = Value::Dict(Dict::new().insert("URL", Value::String("http://a/".into())));
    let doc = command_document(minimal);
    let info = doc.web_capture_info().expect("info").expect("present");
    let commands = doc.web_capture_commands(&info).expect("commands");
    assert_eq!(commands[0].levels, 1, "缺省 /L = 1");
    assert_eq!(commands[0].flags, 0, "缺省 /F = 0");

    let reserved = Value::Dict(
        Dict::new()
            .insert("URL", Value::String("http://a/".into()))
            .insert("F", Value::Integer(0b1000)),
    );
    let doc = command_document(reserved);
    let info = doc.web_capture_info().expect("info").expect("present");
    let error = doc.web_capture_commands(&info).expect_err("reserved bits");
    assert!(matches!(error, WebCaptureError::Malformed(_)), "{error}");

    let missing = Value::Dict(Dict::new().insert("L", Value::Integer(1)));
    let doc = command_document(missing);
    let info = doc.web_capture_info().expect("info").expect("present");
    let error = doc.web_capture_commands(&info).expect_err("missing URL");
    assert!(matches!(error, WebCaptureError::Malformed(_)), "{error}");
}

#[test]
fn web_capture_reads_info_and_page_set_by_url_and_id() {
    let (doc, id) = base_document(page_set(), source());
    let info = doc.web_capture_info().expect("info").expect("present");
    assert_eq!(info.version, 1.0);
    assert!(info.commands.is_empty());

    let by_url = doc
        .web_capture_sets_by_url("http://example.com/")
        .expect("url lookup");
    assert_eq!(by_url.len(), 1);
    let ContentSet::Page(set) = &by_url[0] else {
        panic!("expected page set");
    };
    assert_eq!(set.id, vec![0x01, 0x02, 0x03, 0xFF]);
    assert_eq!(set.objects.len(), 1);
    assert_eq!(set.content_type.as_deref(), Some("text/html"));
    assert_eq!(set.title.as_deref(), Some("Example"));
    assert_eq!(set.text_id.as_deref(), Some(&[0x0A, 0x0B][..]));
    assert_eq!(set.sources.len(), 1);
    assert_eq!(set.sources[0].submission, Some(1));
    assert_eq!(
        set.sources[0].urls,
        UrlAliases::Single("http://example.com/".to_string())
    );

    let by_id = doc.web_capture_sets_by_id(&id).expect("id lookup");
    assert_eq!(by_id.len(), 1);
    assert_eq!(by_id[0].id(), id.as_slice());
}

#[test]
fn web_capture_reads_image_set_reference_counts() {
    let image_set = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("SpiderContentSet".into()))
            .insert("S", Value::Name("SIS".into()))
            .insert("ID", Value::HexString("FF".into()))
            .insert(
                "O",
                Value::Array(Array::new().push(Value::Ref(3, 0)).push(Value::Ref(4, 0))),
            )
            .insert("SI", Value::Ref(7, 0))
            .insert(
                "R",
                Value::Array(Array::new().push(Value::Integer(2)).push(Value::Integer(0))),
            ),
    );
    let (doc, _) = base_document(image_set, source());
    let sets = doc
        .web_capture_sets_by_url("http://example.com/")
        .expect("lookup");
    let ContentSet::Image(set) = &sets[0] else {
        panic!("expected image set");
    };
    assert_eq!(set.reference_counts, vec![2, 0]);
}

#[test]
fn web_capture_reads_url_alias_chains() {
    let aliased = Value::Dict(
        Dict::new().insert(
            "AU",
            Value::Dict(
                Dict::new().insert(
                    "C",
                    Value::Array(
                        Array::new().push(Value::Array(
                            Array::new()
                                .push(Value::String("http://a/".into()))
                                .push(Value::String("http://b/".into())),
                        )),
                    ),
                ),
            ),
        ),
    );
    let (doc, _) = base_document(page_set(), aliased);
    let sets = doc
        .web_capture_sets_by_url("http://example.com/")
        .expect("lookup");
    let sources = sets[0].sources();
    assert_eq!(
        sources[0].urls,
        UrlAliases::Chains(vec![vec!["http://a/".to_string(), "http://b/".to_string()]])
    );
}

#[test]
fn web_capture_rejects_malformed_entries() {
    let bad_subtype = Value::Dict(
        Dict::new()
            .insert("S", Value::Name("BAD".into()))
            .insert("ID", Value::HexString("00".into()))
            .insert("O", Value::Array(Array::new()))
            .insert("SI", Value::Ref(7, 0)),
    );
    let (doc, _) = base_document(bad_subtype, source());
    let error = doc
        .web_capture_sets_by_url("http://example.com/")
        .expect_err("bad subtype");
    assert!(matches!(error, WebCaptureError::Malformed(_)), "{error}");

    let bad_source = Value::Dict(
        Dict::new()
            .insert("S", Value::Name("SPS".into()))
            .insert("ID", Value::HexString("00".into()))
            .insert("O", Value::Array(Array::new()))
            .insert("SI", Value::Ref(7, 0))
            .insert("TID", Value::Integer(1)),
    );
    let (doc, _) = base_document(bad_source, source());
    let error = doc
        .web_capture_sets_by_url("http://example.com/")
        .expect_err("bad /TID");
    assert!(matches!(error, WebCaptureError::Malformed(_)), "{error}");
}
