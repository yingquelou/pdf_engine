//! Artifact 属性表读端测试（ISO 32000-1 14.8.2.2.2 Table 330）：`/Type`、
//! `/BBox`、`/Attached`、`/Subtype` 的类型化解析、未知名保留与类型化错误。

use crate::core::types::{Dict, Value};

use super::super::*;
use super::support::{arr, marked_section, name, text};

#[test]
fn artifact_properties_parse_table330_entries() {
    let properties = Dict::new()
        .insert("Type", name("Pagination"))
        .insert(
            "BBox",
            arr(vec![
                Value::Integer(0),
                Value::Integer(0),
                Value::Float(612.0),
                Value::Integer(792),
            ]),
        )
        .insert("Attached", arr(vec![name("Top"), name("Bottom")]))
        .insert("Subtype", name("Header"));
    let parsed = ArtifactProperties::parse("Artifact", Some(&properties))
        .expect("typed artifact")
        .expect("artifact properties");
    assert_eq!(parsed.artifact_type, Some(ArtifactType::Pagination));
    assert_eq!(parsed.bounding_box, Some([0.0, 0.0, 612.0, 792.0]));
    assert_eq!(
        parsed.attached,
        vec![ArtifactAttachment::Top, ArtifactAttachment::Bottom]
    );
    assert_eq!(parsed.subtype, Some(ArtifactSubtype::Header));
    assert_eq!(
        parsed.artifact_type.as_ref().map(ArtifactType::as_str),
        Some("Pagination")
    );
    assert_eq!(
        parsed.subtype.as_ref().map(ArtifactSubtype::as_str),
        Some("Header")
    );

    let section = marked_section("Artifact", Some(properties), None);
    assert_eq!(
        section.artifact_properties().expect("section artifact"),
        Some(parsed)
    );

    let layout = Dict::new().insert("Type", name("Layout"));
    let parsed = ArtifactProperties::parse("Artifact", Some(&layout))
        .expect("typed layout")
        .expect("layout properties");
    assert_eq!(parsed.artifact_type, Some(ArtifactType::Layout));
    assert_eq!(parsed.bounding_box, None);
    assert!(parsed.attached.is_empty());
    assert_eq!(parsed.subtype, None);
}

#[test]
fn artifact_properties_preserve_unknown_names_and_ignore_other_tag() {
    let properties = Dict::new()
        .insert("Type", name("Background"))
        .insert("Subtype", name("Bates"))
        .insert("Unmodelled", Value::Integer(1));
    let parsed = ArtifactProperties::parse("Artifact", Some(&properties))
        .expect("typed artifact")
        .expect("artifact properties");
    assert_eq!(parsed.artifact_type, Some(ArtifactType::Background));
    assert_eq!(
        parsed.subtype,
        Some(ArtifactSubtype::Other("Bates".to_owned()))
    );
    assert_eq!(
        parsed.subtype.as_ref().map(ArtifactSubtype::as_str),
        Some("Bates"),
        "unknown subtype keeps its spelling"
    );
    assert_eq!(ArtifactType::from_name("Slug").as_str(), "Slug");

    assert_eq!(
        ArtifactProperties::parse("Span", Some(&properties)).expect("non-Artifact tag"),
        None
    );
    assert_eq!(
        ArtifactProperties::parse("Artifact", None).expect("missing property list"),
        None
    );
    assert_eq!(
        ArtifactProperties::parse("artifact", Some(&properties)).expect("case-sensitive tag"),
        None
    );
}

#[test]
fn malformed_artifact_properties_are_typed_errors() {
    fn message(properties: &Dict) -> String {
        match ArtifactProperties::parse("Artifact", Some(properties)) {
            Err(ArtifactError::Malformed(message)) => message,
            other => panic!("expected malformed Artifact properties, got {other:?}"),
        }
    }

    assert_eq!(
        ArtifactError::Malformed("x".to_owned()).to_string(),
        "invalid artifact property list: x"
    );

    let bad_type = message(&Dict::new().insert("Type", text("Pagination")));
    assert!(
        bad_type.contains("Type") && bad_type.contains("string"),
        "{bad_type}"
    );

    let short_bbox = message(&Dict::new().insert("BBox", arr(vec![Value::Integer(0); 3])));
    assert!(
        short_bbox.contains("BBox") && short_bbox.contains("4"),
        "{short_bbox}"
    );
    let named_bbox = message(&Dict::new().insert(
        "BBox",
        arr(vec![
            Value::Integer(0),
            Value::Integer(0),
            name("x"),
            Value::Integer(0),
        ]),
    ));
    assert!(
        named_bbox.contains("BBox") && named_bbox.contains("name"),
        "{named_bbox}"
    );
    let infinite_bbox = message(&Dict::new().insert(
        "BBox",
        arr(vec![
            Value::Float(f64::INFINITY),
            Value::Integer(0),
            Value::Integer(1),
            Value::Integer(1),
        ]),
    ));
    assert!(infinite_bbox.contains("BBox"), "{infinite_bbox}");
    let scalar_bbox = message(&Dict::new().insert("BBox", Value::Integer(0)));
    assert!(
        scalar_bbox.contains("BBox") && scalar_bbox.contains("integer"),
        "{scalar_bbox}"
    );

    let empty_attached = message(&Dict::new().insert("Attached", arr(Vec::new())));
    assert!(empty_attached.contains("Attached"), "{empty_attached}");
    let crowded_attached = message(&Dict::new().insert(
        "Attached",
        arr(vec![
            name("Top"),
            name("Bottom"),
            name("Left"),
            name("Right"),
            name("Top"),
        ]),
    ));
    assert!(crowded_attached.contains("Attached"), "{crowded_attached}");
    let duplicate_attached =
        message(&Dict::new().insert("Attached", arr(vec![name("Top"), name("Top")])));
    assert!(duplicate_attached.contains("Top"), "{duplicate_attached}");
    let unknown_attached = message(&Dict::new().insert("Attached", arr(vec![name("Middle")])));
    assert!(unknown_attached.contains("Middle"), "{unknown_attached}");
    let scalar_attached = message(&Dict::new().insert("Attached", name("Top")));
    assert!(
        scalar_attached.contains("Attached") && scalar_attached.contains("name"),
        "{scalar_attached}"
    );

    let bad_subtype = message(&Dict::new().insert("Subtype", text("Header")));
    assert!(
        bad_subtype.contains("Subtype") && bad_subtype.contains("string"),
        "{bad_subtype}"
    );
}
