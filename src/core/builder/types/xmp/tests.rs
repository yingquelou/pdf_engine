//! 类型化 XMP 包序列化单元测试（14.3.2）：四种值形态、转义、前缀
//! 确定性与非法输入拒绝，并经读端 `XmpMetadata::parse` 往返断言。

use std::collections::BTreeMap;

use super::*;
use crate::core::reader::xmp::{
    XmpListItem as ReaderListItem, XmpListKind as ReaderListKind, XmpMetadata,
    XmpValue as ReaderValue,
};

const DC_NS: &str = "http://purl.org/dc/elements/1.1/";
const EX_NS: &str = "urn:example:xmp";
const ADOBE_NS: &str = "adobe:ns:meta/";
const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
const XMLNS_NS: &str = "http://www.w3.org/2000/xmlns/";
const W3CDTF: &str = "http://purl.org/dc/terms/W3CDTF";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

fn clark(namespace: &str, local: &str) -> String {
    format!("{{{namespace}}}{local}")
}

fn text(value: &str) -> XmpValue {
    XmpValue::Text(value.to_string())
}

fn item(lang: Option<&str>, value: &str) -> XmpListItem {
    XmpListItem {
        lang: lang.map(str::to_string),
        value: value.to_string(),
    }
}

fn sample_packet() -> XmpPacket {
    let mut packet = XmpPacket::new();
    packet
        .set(clark(DC_NS, "title"), text("Sample & Title"))
        .set(
            clark(DC_NS, "creator"),
            XmpValue::List {
                kind: XmpListKind::Seq,
                items: vec![item(None, "Ada"), item(None, "Grace")],
            },
        )
        .set(
            clark(DC_NS, "description"),
            XmpValue::List {
                kind: XmpListKind::Alt,
                items: vec![
                    item(Some("x-default"), "A sample"),
                    item(Some("fr"), "Un exemple"),
                ],
            },
        )
        .set(
            clark(EX_NS, "keywords"),
            XmpValue::List {
                kind: XmpListKind::Bag,
                items: Vec::new(),
            },
        )
        .set(
            clark(EX_NS, "author"),
            XmpValue::Resource(BTreeMap::from([
                (clark(EX_NS, "name"), text("Ada")),
                (
                    clark(EX_NS, "address"),
                    XmpValue::Resource(BTreeMap::from([
                        (clark(EX_NS, "city"), text("London")),
                        (clark(EX_NS, "geo"), XmpValue::Resource(BTreeMap::new())),
                    ])),
                ),
                (
                    clark(EX_NS, "tags"),
                    XmpValue::List {
                        kind: XmpListKind::Bag,
                        items: vec![item(None, "math"), item(None, "code")],
                    },
                ),
                (
                    clark(EX_NS, "level"),
                    XmpValue::Typed {
                        value: Box::new(text("high")),
                        datatype: None,
                        language: Some("en".into()),
                    },
                ),
            ])),
        )
        .set(
            clark(EX_NS, "created"),
            XmpValue::Typed {
                value: Box::new(text("2024-05-01T09:00:00Z")),
                datatype: Some(W3CDTF.into()),
                language: None,
            },
        )
        .set(
            clark(EX_NS, "label"),
            XmpValue::Typed {
                value: Box::new(text("Bonjour")),
                datatype: None,
                language: Some("fr".into()),
            },
        )
        .set(
            clark(EX_NS, "count"),
            XmpValue::Typed {
                value: Box::new(text("42")),
                datatype: Some(XSD_INTEGER.into()),
                language: Some("en".into()),
            },
        );
    packet
}

fn reader_value(value: &XmpValue) -> ReaderValue {
    match value {
        XmpValue::Text(value) => ReaderValue::Text(value.clone()),
        XmpValue::List { kind, items } => ReaderValue::List {
            kind: match kind {
                XmpListKind::Alt => ReaderListKind::Alt,
                XmpListKind::Seq => ReaderListKind::Seq,
                XmpListKind::Bag => ReaderListKind::Bag,
            },
            items: items
                .iter()
                .map(|item| ReaderListItem {
                    lang: item.lang.clone(),
                    value: item.value.clone(),
                })
                .collect(),
        },
        XmpValue::Collection(items) => {
            ReaderValue::Collection(items.iter().map(reader_value).collect())
        }
        XmpValue::Resource(map) => ReaderValue::Resource(
            map.iter()
                .map(|(key, value)| (key.clone(), reader_value(value)))
                .collect(),
        ),
        XmpValue::Typed {
            value,
            datatype,
            language,
        } => ReaderValue::Typed {
            value: Box::new(reader_value(value)),
            datatype: datatype.clone(),
            language: language.clone(),
        },
        XmpValue::Reference(uri) => ReaderValue::Reference(uri.clone()),
        XmpValue::Literal(xml) => ReaderValue::Literal(xml.clone()),
    }
}

#[test]
fn all_value_shapes_roundtrip_through_reader() {
    let packet = sample_packet();
    let xml = packet.to_xml().expect("serialize");
    assert!(
        xml.starts_with("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">"),
        "{xml}"
    );

    let parsed = XmpMetadata::parse(&xml).expect("parse serialized packet");
    let expected: BTreeMap<String, ReaderValue> = packet
        .properties()
        .iter()
        .map(|(key, value)| (key.clone(), reader_value(value)))
        .collect();
    assert_eq!(parsed.properties, expected);

    assert_eq!(
        parsed.properties.get(&clark(DC_NS, "description")),
        Some(&ReaderValue::List {
            kind: ReaderListKind::Alt,
            items: vec![
                ReaderListItem {
                    lang: Some("x-default".into()),
                    value: "A sample".into(),
                },
                ReaderListItem {
                    lang: Some("fr".into()),
                    value: "Un exemple".into(),
                },
            ],
        })
    );
    let Some(ReaderValue::Resource(author)) = parsed.properties.get(&clark(EX_NS, "author")) else {
        panic!("author must parse back as a resource");
    };
    assert_eq!(
        author.get(&clark(EX_NS, "name")),
        Some(&ReaderValue::Text("Ada".into()))
    );
    let Some(ReaderValue::Resource(address)) = author.get(&clark(EX_NS, "address")) else {
        panic!("address must parse back as a nested resource");
    };
    assert_eq!(
        address.get(&clark(EX_NS, "city")),
        Some(&ReaderValue::Text("London".into()))
    );
    assert_eq!(
        address.get(&clark(EX_NS, "geo")),
        Some(&ReaderValue::Resource(BTreeMap::new()))
    );
    assert_eq!(
        author.get(&clark(EX_NS, "level")),
        Some(&ReaderValue::Typed {
            value: Box::new(ReaderValue::Text("high".into())),
            datatype: None,
            language: Some("en".into()),
        })
    );
    assert_eq!(
        parsed.properties.get(&clark(EX_NS, "count")),
        Some(&ReaderValue::Typed {
            value: Box::new(ReaderValue::Text("42".into())),
            datatype: Some(XSD_INTEGER.into()),
            language: Some("en".into()),
        })
    );
    assert_eq!(
        parsed.properties.get(&clark(EX_NS, "keywords")),
        Some(&ReaderValue::List {
            kind: ReaderListKind::Bag,
            items: Vec::new(),
        })
    );
}

#[test]
fn empty_packet_serializes_minimal_description() {
    let xml = XmpPacket::new().to_xml().expect("serialize empty packet");
    assert!(xml.contains("<rdf:Description rdf:about=\"\"></rdf:Description>"));
    let parsed = XmpMetadata::parse(&xml).expect("parse empty packet");
    assert!(parsed.properties.is_empty());
}

#[test]
fn markup_is_escaped_in_text_and_attributes() {
    let snippet = "a & b < c > d \"e\" 'f'";
    let language = "en&<>\"'";
    let namespace = "urn:example:xmp?a=1&b=2";
    let datatype = "urn:example:type?a=1&b=2";
    let mut packet = XmpPacket::new();
    packet.set(clark(namespace, "text"), text(snippet)).set(
        clark(namespace, "typed"),
        XmpValue::Typed {
            value: Box::new(text("v")),
            datatype: Some(datatype.into()),
            language: Some(language.into()),
        },
    );

    let xml = packet.to_xml().expect("serialize escaped values");
    for entity in ["&amp;", "&lt;", "&gt;", "&quot;", "&apos;"] {
        assert!(xml.contains(entity), "missing {entity} in {xml}");
    }
    let parsed = XmpMetadata::parse(&xml).expect("parse escaped values");
    assert_eq!(parsed.text(&clark(namespace, "text")), Some(snippet));
    assert_eq!(
        parsed.namespaces.get("ns1").map(String::as_str),
        Some(namespace)
    );
    assert_eq!(
        parsed.properties.get(&clark(namespace, "typed")),
        Some(&ReaderValue::Typed {
            value: Box::new(ReaderValue::Text("v".into())),
            datatype: Some(datatype.into()),
            language: Some(language.into()),
        })
    );
}

#[test]
fn namespace_prefixes_are_deterministic() {
    let mut packet = XmpPacket::new();
    packet
        .set(clark("urn:z", "last"), text("z"))
        .set(clark("urn:a", "first"), text("a"))
        .set(clark(ADOBE_NS, "Label"), text("label"));

    let xml = packet.to_xml().expect("serialize");
    assert_eq!(xml, packet.to_xml().expect("serialize again"));
    let first = xml
        .find("xmlns:ns1=\"urn:a\"")
        .unwrap_or_else(|| panic!("ns1 must bind urn:a: {xml}"));
    let second = xml
        .find("xmlns:ns2=\"urn:z\"")
        .unwrap_or_else(|| panic!("ns2 must bind urn:z: {xml}"));
    assert!(first < second, "prefixes follow URI order: {xml}");
    assert!(
        xml.find("<ns1:first>").expect("ns1 property")
            < xml.find("<ns2:last>").expect("ns2 property")
    );
    assert_eq!(xml.matches("xmlns:rdf=").count(), 1);
    assert_eq!(xml.matches("xmlns:x=").count(), 1);
    assert!(xml.contains("<x:Label>label</x:Label>"), "{xml}");
    assert!(!xml.contains("xmlns:ns3=\"adobe:ns:meta/\""), "{xml}");

    let parsed = XmpMetadata::parse(&xml).expect("parse");
    assert_eq!(
        parsed.properties.get(&clark(ADOBE_NS, "Label")),
        Some(&ReaderValue::Text("label".into()))
    );
}

#[test]
fn invalid_clark_keys_are_rejected() {
    let cases = vec![
        "plain".to_string(),
        "{urn:example}".to_string(),
        "{}value".to_string(),
        "{urn:example}1value".to_string(),
        "{urn:example}bad:name".to_string(),
        "{urn:example}bad name".to_string(),
        "{urn:example}bad/name".to_string(),
        "{urn: example}value".to_string(),
        clark(RDF_NS, "value"),
        clark(XML_NS, "lang"),
        clark(XMLNS_NS, "x"),
    ];
    for key in cases {
        let mut packet = XmpPacket::new();
        packet.set(key, text("v"));
        assert!(
            matches!(packet.to_xml(), Err(XmpPacketError::InvalidKey(_))),
            "key should be rejected"
        );
    }

    let mut nested = XmpPacket::new();
    nested.set(
        clark(EX_NS, "root"),
        XmpValue::Resource(BTreeMap::from([(
            "{urn:example}bad name".to_string(),
            text("v"),
        )])),
    );
    assert!(matches!(
        nested.to_xml(),
        Err(XmpPacketError::InvalidKey(_))
    ));
}

#[test]
fn invalid_xml_characters_are_rejected() {
    for snippet in [
        "nul\u{0}",
        "bell\u{7}",
        "vt\u{b}",
        "del\u{7f}",
        "fffe\u{fffe}",
    ] {
        let mut packet = XmpPacket::new();
        packet.set(clark(EX_NS, "value"), text(snippet));
        assert!(
            matches!(packet.to_xml(), Err(XmpPacketError::InvalidCharacter(_))),
            "text should be rejected: {snippet:?}"
        );
    }

    let mut list = XmpPacket::new();
    list.set(
        clark(EX_NS, "list"),
        XmpValue::List {
            kind: XmpListKind::Seq,
            items: vec![item(None, "bad\u{1}")],
        },
    );
    assert!(matches!(
        list.to_xml(),
        Err(XmpPacketError::InvalidCharacter(_))
    ));

    let mut language = XmpPacket::new();
    language.set(
        clark(EX_NS, "typed"),
        XmpValue::Typed {
            value: Box::new(text("v")),
            datatype: None,
            language: Some("en US".into()),
        },
    );
    assert!(matches!(
        language.to_xml(),
        Err(XmpPacketError::InvalidCharacter(_))
    ));

    let mut allowed = XmpPacket::new();
    let snippet = "a\tb\nc\rd";
    allowed.set(clark(EX_NS, "value"), text(snippet));
    let xml = allowed
        .to_xml()
        .expect("Tab/LF/CR are valid XML characters");
    let parsed = XmpMetadata::parse(&xml).expect("parse");
    assert_eq!(parsed.text(&clark(EX_NS, "value")), Some(snippet));
}

#[test]
fn invalid_datatype_uris_are_rejected() {
    let cases = [
        "",
        "not-a-uri",
        "1urn:example",
        ":value",
        "urn:",
        "urn:example value",
        "urn:example\u{1}",
    ];
    for datatype in cases {
        let mut packet = XmpPacket::new();
        packet.set(
            clark(EX_NS, "typed"),
            XmpValue::Typed {
                value: Box::new(text("v")),
                datatype: Some(datatype.into()),
                language: None,
            },
        );
        assert!(
            matches!(packet.to_xml(), Err(XmpPacketError::InvalidDatatype(_))),
            "datatype should be rejected: {datatype:?}"
        );
    }
}

#[test]
fn typed_values_require_text_primary_value() {
    let mut packet = XmpPacket::new();
    packet.set(
        clark(EX_NS, "typed"),
        XmpValue::Typed {
            value: Box::new(XmpValue::List {
                kind: XmpListKind::Seq,
                items: vec![item(None, "v")],
            }),
            datatype: Some("urn:example:type".into()),
            language: None,
        },
    );
    assert!(matches!(
        packet.to_xml(),
        Err(XmpPacketError::UnsupportedValue(_))
    ));
}

#[test]
fn references_and_literals_serialize() {
    let literal = r#"<span xmlns="urn:ex">hi &amp; bye</span>"#;
    let mut packet = XmpPacket::new();
    packet
        .set(
            clark(EX_NS, "link"),
            XmpValue::Reference(r#"urn:example:a?x=1&y=2<z>"q"'s'"#.into()),
        )
        .set(clark(EX_NS, "markup"), XmpValue::Literal(literal.into()))
        .set(clark(EX_NS, "empty"), XmpValue::Literal(String::new()));

    let xml = packet.to_xml().expect("serialize references and literals");
    // 引用按属性值转义五种实体，字面量标记逐字输出。
    assert!(
        xml.contains(
            r#"<ns1:link rdf:resource="urn:example:a?x=1&amp;y=2&lt;z&gt;&quot;q&quot;&apos;s&apos;"/>"#
        ),
        "{xml}"
    );
    assert!(
        xml.contains(
            r#"<ns1:markup rdf:parseType="Literal"><span xmlns="urn:ex">hi &amp; bye</span></ns1:markup>"#
        ),
        "{xml}"
    );
    assert!(
        xml.contains(r#"<ns1:empty rdf:parseType="Literal"></ns1:empty>"#),
        "{xml}"
    );
    assert!(xml.contains("hi &amp; bye"), "literal is not re-escaped");
    assert!(!xml.contains("&amp;amp;"), "{xml}");
}

#[test]
fn invalid_reference_and_literal_values_are_rejected() {
    for uri in [
        "",
        "urn:example:a b",
        "urn:example:a\u{1}",
        "urn:example:a\u{0}",
    ] {
        let mut packet = XmpPacket::new();
        packet.set(clark(EX_NS, "link"), XmpValue::Reference(uri.into()));
        assert!(
            matches!(packet.to_xml(), Err(XmpPacketError::InvalidReference(_))),
            "reference should be rejected: {uri:?}"
        );
    }

    for markup in [
        "<?xml version=\"1.0\"?>",
        "<?pi data?>",
        "<!DOCTYPE x>",
        "<!doctype x [<!ENTITY e \"v\">]>",
        "a ]]> b",
        "bad\u{0}",
    ] {
        let mut packet = XmpPacket::new();
        packet.set(clark(EX_NS, "markup"), XmpValue::Literal(markup.into()));
        assert!(
            matches!(packet.to_xml(), Err(XmpPacketError::InvalidLiteral(_))),
            "literal should be rejected: {markup:?}"
        );
    }
}

#[test]
fn references_and_literals_roundtrip_through_reader() {
    let literal = r#"<span xmlns="urn:ex">hi &amp; bye</span>"#;
    let reference = "https://example.com/a?x=1&y=2";
    let mut packet = XmpPacket::new();
    packet
        .set(clark(EX_NS, "link"), XmpValue::Reference(reference.into()))
        .set(clark(EX_NS, "markup"), XmpValue::Literal(literal.into()))
        .set(clark(EX_NS, "empty"), XmpValue::Literal(String::new()));

    let xml = packet.to_xml().expect("serialize");
    let parsed = XmpMetadata::parse(&xml).expect("parse serialized packet");
    assert_eq!(
        parsed.properties.get(&clark(EX_NS, "link")),
        Some(&ReaderValue::Reference(reference.into()))
    );
    assert_eq!(
        parsed.properties.get(&clark(EX_NS, "markup")),
        Some(&ReaderValue::Literal(literal.into()))
    );
    assert_eq!(
        parsed.properties.get(&clark(EX_NS, "empty")),
        Some(&ReaderValue::Literal(String::new()))
    );
    for local in ["link", "markup", "empty"] {
        assert_eq!(parsed.text(&clark(EX_NS, local)), None);
    }
}

mod collection;
