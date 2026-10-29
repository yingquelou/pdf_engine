//! XMP 包解析单元测试（14.3.2）。

use super::*;

const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const DC: &str = "http://purl.org/dc/elements/1.1/";
const PDFX: &str = "http://ns.adobe.com/pdfx/1.3/";

fn fixture_packet() -> String {
    [
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>",
        "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"Adobe XMP Core 5.0-c060\">",
        "  <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">",
        "    <rdf:Description rdf:about=\"\"",
        "        xmlns:dc=\"http://purl.org/dc/elements/1.1/\"",
        "        xmlns:pdfx='http://ns.adobe.com/pdfx/1.3/'",
        "        dc:source=\"Martin Thoma\"",
        "        pdfx:other=\"worlds\"",
        "        pdfx:٢٣F0=\"time\">",
        "      <pdfx:Style>FooBarStyle</pdfx:Style>",
        "      <dc:creator><rdf:Seq><rdf:li>John Doe</rdf:li></rdf:Seq></dc:creator>",
        "      <dc:description>This is a text</dc:description>",
        "      <dc:date>1990-04-28</dc:date>",
        "      <dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">Sample PDF with XMP Metadata</rdf:li></rdf:Alt></dc:title>",
        "    </rdf:Description>",
        "  </rdf:RDF>",
        "</x:xmpmeta>",
        "<?xpacket end=\"w\"?>",
    ]
    .concat()
}

#[test]
fn xmp_packet_parses_namespaced_attributes_and_lists() {
    let packet = fixture_packet();
    let xmp = XmpMetadata::parse(&packet).expect("parse fixture packet");
    assert_eq!(xmp.packet, packet);
    assert_eq!(xmp.namespaces.get("dc").map(String::as_str), Some(DC));
    assert_eq!(xmp.namespaces.get("pdfx").map(String::as_str), Some(PDFX));
    assert_eq!(xmp.namespaces.get("rdf").map(String::as_str), Some(RDF));
    assert_eq!(
        xmp.namespaces.get("x").map(String::as_str),
        Some("adobe:ns:meta/")
    );

    assert_eq!(xmp.text(&format!("{{{DC}}}source")), Some("Martin Thoma"));
    assert_eq!(xmp.text(&format!("{{{PDFX}}}other")), Some("worlds"));
    assert_eq!(xmp.text(&format!("{{{PDFX}}}٢٣F0")), Some("time"));
    assert_eq!(xmp.text(&format!("{{{PDFX}}}Style")), Some("FooBarStyle"));
    assert_eq!(xmp.text(&format!("{{{DC}}}date")), Some("1990-04-28"));

    let creator = xmp
        .properties
        .get(&format!("{{{DC}}}creator"))
        .expect("dc:creator");
    assert_eq!(
        creator,
        &XmpValue::List {
            kind: XmpListKind::Seq,
            items: vec![XmpListItem {
                lang: None,
                value: "John Doe".into(),
            }],
        }
    );

    let title = xmp
        .properties
        .get(&format!("{{{DC}}}title"))
        .expect("dc:title");
    assert_eq!(
        title,
        &XmpValue::List {
            kind: XmpListKind::Alt,
            items: vec![XmpListItem {
                lang: Some("x-default".into()),
                value: "Sample PDF with XMP Metadata".into(),
            }],
        }
    );
    assert_eq!(
        xmp.text(&format!("{{{DC}}}title")),
        Some("Sample PDF with XMP Metadata")
    );
}

#[test]
fn xmp_decodes_entities_bag_lists_and_cdata() {
    let packet = format!(
        concat!(
            "<rdf:RDF xmlns:rdf=\"{}\">",
            "<rdf:Description xmlns:ex=\"urn:ex\" ex:attr=\"a &amp; b &#233; &#x4E2D;\">",
            "<ex:bag><rdf:Bag><rdf:li>one</rdf:li><rdf:li>two</rdf:li></rdf:Bag></ex:bag>",
            "<ex:raw><![CDATA[<not-a-tag>]]></ex:raw>",
            "</rdf:Description></rdf:RDF>"
        ),
        RDF
    );
    let xmp = XmpMetadata::parse(&packet).expect("parse");
    assert_eq!(xmp.text("{urn:ex}attr"), Some("a & b é 中"));
    assert_eq!(
        xmp.properties.get("{urn:ex}bag"),
        Some(&XmpValue::List {
            kind: XmpListKind::Bag,
            items: vec![
                XmpListItem {
                    lang: None,
                    value: "one".into()
                },
                XmpListItem {
                    lang: None,
                    value: "two".into()
                },
            ],
        })
    );
    assert_eq!(xmp.text("{urn:ex}raw"), Some("<not-a-tag>"));
}

#[test]
fn malformed_xmp_packets_are_typed_errors() {
    let cases = [
        String::new(),
        format!("<rdf:RDF xmlns:rdf=\"{RDF}\"><rdf:Description></rdf:RDF>"),
        format!("<rdf:RDF xmlns:rdf=\"{RDF}\"><rdf:Description bad=\"&bogus;\"/></rdf:RDF>"),
        format!("<rdf:RDF xmlns:rdf=\"{RDF}\"><rdf:Description foo:bar=\"1\"/></rdf:RDF>"),
        format!("<rdf:RDF xmlns:rdf=\"{RDF}\"/>"),
        format!("<rdf:RDF xmlns:rdf=\"{RDF}\"/><extra/>"),
    ];
    for packet in cases {
        assert!(
            matches!(XmpMetadata::parse(&packet), Err(XmpError::Malformed(_))),
            "packet should be rejected: {packet}"
        );
    }
}

#[test]
fn xmp_limits_are_enforced() {
    let mut deep = format!("<rdf:RDF xmlns:rdf=\"{RDF}\">");
    for _ in 0..40 {
        deep.push_str("<rdf:Description>");
    }
    for _ in 0..40 {
        deep.push_str("</rdf:Description>");
    }
    deep.push_str("</rdf:RDF>");
    assert_eq!(
        XmpMetadata::parse(&deep).expect_err("depth limit"),
        XmpError::LimitExceeded("XMP element depth")
    );

    let mut wide = format!("<rdf:RDF xmlns:rdf=\"{RDF}\">");
    for _ in 0..4100 {
        wide.push_str("<rdf:Description/>");
    }
    wide.push_str("</rdf:RDF>");
    assert_eq!(
        XmpMetadata::parse(&wide).expect_err("element limit"),
        XmpError::LimitExceeded("XMP element count")
    );

    let mut attrs = format!("<rdf:RDF xmlns:rdf=\"{RDF}\">");
    attrs.push_str("<rdf:Description xmlns:ex=\"urn:ex\"");
    for index in 0..1025 {
        attrs.push_str(&format!(" ex:p{index}=\"v\""));
    }
    attrs.push_str("/></rdf:RDF>");
    assert_eq!(
        XmpMetadata::parse(&attrs).expect_err("property limit"),
        XmpError::LimitExceeded("XMP property count")
    );
}
