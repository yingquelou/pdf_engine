//! `rdf:parseType="Collection"` 序列化与往返测试（14.3.2）。

use std::collections::BTreeMap;

use super::super::{XmpListKind, XmpPacket, XmpPacketError, XmpValue};
use super::{EX_NS, XSD_INTEGER, clark, item, reader_value, text};
use crate::core::reader::xmp::XmpMetadata;

#[test]
fn collection_roundtrips_through_reader() {
    let collection = XmpValue::Collection(vec![
        text("plain"),
        XmpValue::Resource(BTreeMap::from([
            (clark(EX_NS, "name"), text("Ada")),
            (
                clark(EX_NS, "address"),
                XmpValue::Resource(BTreeMap::from([(clark(EX_NS, "city"), text("London"))])),
            ),
        ])),
        XmpValue::Reference("urn:example:ref".into()),
        XmpValue::Literal(r#"<span xmlns="urn:ex">hi &amp; bye</span>"#.into()),
        XmpValue::Typed {
            value: Box::new(text("42")),
            datatype: Some(XSD_INTEGER.into()),
            language: Some("en".into()),
        },
        XmpValue::List {
            kind: XmpListKind::Seq,
            items: vec![item(Some("en"), "one"), item(None, "two")],
        },
        XmpValue::Collection(Vec::new()),
        text(""),
    ]);
    let mut packet = XmpPacket::new();
    packet.set(clark(EX_NS, "items"), collection.clone());

    let xml = packet.to_xml().expect("serialize collection");
    assert!(xml.contains("rdf:parseType=\"Collection\""), "{xml}");
    assert!(xml.contains("<rdf:li"), "{xml}");

    let parsed = XmpMetadata::parse(&xml).expect("parse serialized collection");
    assert_eq!(
        parsed.properties.get(&clark(EX_NS, "items")),
        Some(&reader_value(&collection))
    );
    assert_eq!(parsed.text(&clark(EX_NS, "items")), None);
}

#[test]
fn empty_collection_serializes_self_closing() {
    let mut packet = XmpPacket::new();
    packet.set(clark(EX_NS, "empty"), XmpValue::Collection(Vec::new()));
    let xml = packet.to_xml().expect("serialize empty collection");
    assert!(
        xml.contains(r#"<ns1:empty rdf:parseType="Collection"/>"#),
        "{xml}"
    );
    let parsed = XmpMetadata::parse(&xml).expect("parse empty collection");
    assert_eq!(
        parsed.properties.get(&clark(EX_NS, "empty")),
        Some(&reader_value(&XmpValue::Collection(Vec::new())))
    );
}

#[test]
fn invalid_collection_items_are_rejected() {
    let mut literal = XmpPacket::new();
    literal.set(
        clark(EX_NS, "items"),
        XmpValue::Collection(vec![XmpValue::Literal("<?pi?>".into())]),
    );
    assert!(matches!(
        literal.to_xml(),
        Err(XmpPacketError::InvalidLiteral(_))
    ));

    let mut reference = XmpPacket::new();
    reference.set(
        clark(EX_NS, "items"),
        XmpValue::Collection(vec![XmpValue::Reference(String::new())]),
    );
    assert!(matches!(
        reference.to_xml(),
        Err(XmpPacketError::InvalidReference(_))
    ));

    let mut resource = XmpPacket::new();
    resource.set(
        clark(EX_NS, "items"),
        XmpValue::Collection(vec![XmpValue::Resource(BTreeMap::from([(
            "bad".into(),
            text("x"),
        )]))]),
    );
    assert!(matches!(
        resource.to_xml(),
        Err(XmpPacketError::InvalidKey(_))
    ));
}
