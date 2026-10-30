//! XMP 包解析单元测试（14.3.2）。

use std::collections::BTreeMap;

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

#[test]
fn rdf_parse_type_resource_nests_structured_values() {
    let packet = format!(
        concat!(
            "<rdf:RDF xmlns:rdf=\"{}\">",
            "<rdf:Description rdf:about=\"\" xmlns:ex=\"urn:ex\">",
            "<ex:author rdf:parseType=\"Resource\">",
            "<ex:name>Ada</ex:name>",
            "<ex:address rdf:parseType=\"Resource\">",
            "<ex:city>London</ex:city>",
            "<ex:tags><rdf:Bag><rdf:li>math</rdf:li><rdf:li>code</rdf:li></rdf:Bag></ex:tags>",
            "</ex:address>",
            "</ex:author>",
            "<ex:editor><rdf:Description ex:name=\"Grace\"/></ex:editor>",
            "</rdf:Description></rdf:RDF>"
        ),
        RDF
    );
    let xmp = XmpMetadata::parse(&packet).expect("parse");

    let Some(XmpValue::Resource(author)) = xmp.properties.get("{urn:ex}author") else {
        panic!("ex:author must be a resource");
    };
    assert_eq!(
        author.get("{urn:ex}name"),
        Some(&XmpValue::Text("Ada".into()))
    );
    assert!(
        author.get(&format!("{{{RDF}}}parseType")).is_none(),
        "rdf:parseType is syntax, not a property"
    );

    let Some(XmpValue::Resource(address)) = author.get("{urn:ex}address") else {
        panic!("ex:address must be a nested resource");
    };
    assert_eq!(
        address.get("{urn:ex}city"),
        Some(&XmpValue::Text("London".into()))
    );
    assert_eq!(
        address.get("{urn:ex}tags"),
        Some(&XmpValue::List {
            kind: XmpListKind::Bag,
            items: vec![
                XmpListItem {
                    lang: None,
                    value: "math".into(),
                },
                XmpListItem {
                    lang: None,
                    value: "code".into(),
                },
            ],
        })
    );

    // 节点元素形式（属性值直接是 rdf:Description）同样成为资源。
    assert_eq!(
        xmp.properties.get("{urn:ex}editor"),
        Some(&XmpValue::Resource(BTreeMap::from([(
            "{urn:ex}name".to_string(),
            XmpValue::Text("Grace".into()),
        )])))
    );
    assert!(
        !xmp.properties.contains_key("{urn:ex}name"),
        "nested properties must not leak into the top level"
    );
    assert_eq!(xmp.text("{urn:ex}author"), None);
}

#[test]
fn xmp_typed_values_preserve_datatype_and_language() {
    let packet = format!(
        concat!(
            "<rdf:RDF xmlns:rdf=\"{}\" xmlns:ex=\"urn:ex\">",
            "<rdf:Description rdf:about=\"\">",
            "<ex:created><rdf:Description rdf:datatype=\"http://purl.org/dc/terms/W3CDTF\">",
            "<rdf:value>2024-05-01T09:00:00Z</rdf:value></rdf:Description></ex:created>",
            "<ex:title><rdf:Description xml:lang=\"zh-CN\">",
            "<rdf:value>标题</rdf:value></rdf:Description></ex:title>",
            "<ex:count rdf:datatype=\"http://www.w3.org/2001/XMLSchema#integer\">42</ex:count>",
            "<ex:level rdf:parseType=\"Resource\" xml:lang=\"en\">",
            "<rdf:value>high</rdf:value></ex:level>",
            "<ex:inherited xml:lang=\"de\"><rdf:Description>",
            "<rdf:value>Wert</rdf:value></rdf:Description></ex:inherited>",
            "</rdf:Description></rdf:RDF>"
        ),
        RDF
    );
    let xmp = XmpMetadata::parse(&packet).expect("parse");

    assert_eq!(
        xmp.properties.get("{urn:ex}created"),
        Some(&XmpValue::Typed {
            value: Box::new(XmpValue::Text("2024-05-01T09:00:00Z".into())),
            datatype: Some("http://purl.org/dc/terms/W3CDTF".into()),
            language: None,
        })
    );
    assert_eq!(
        xmp.properties.get("{urn:ex}title"),
        Some(&XmpValue::Typed {
            value: Box::new(XmpValue::Text("标题".into())),
            datatype: None,
            language: Some("zh-CN".into()),
        })
    );
    assert_eq!(
        xmp.properties.get("{urn:ex}count"),
        Some(&XmpValue::Typed {
            value: Box::new(XmpValue::Text("42".into())),
            datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
            language: None,
        })
    );
    assert_eq!(
        xmp.properties.get("{urn:ex}level"),
        Some(&XmpValue::Typed {
            value: Box::new(XmpValue::Text("high".into())),
            datatype: None,
            language: Some("en".into()),
        })
    );
    // xml:lang 从承载属性元素继承到类型化值。
    assert_eq!(
        xmp.properties.get("{urn:ex}inherited"),
        Some(&XmpValue::Typed {
            value: Box::new(XmpValue::Text("Wert".into())),
            datatype: None,
            language: Some("de".into()),
        })
    );
    for key in ["created", "title", "count", "level", "inherited"] {
        assert_eq!(xmp.text(&format!("{{urn:ex}}{key}")), None);
    }
}

#[test]
fn xmp_text_accessor_remains_compatible() {
    let packet = format!(
        concat!(
            "<rdf:RDF xmlns:rdf=\"{}\" xmlns:ex=\"urn:ex\">",
            "<rdf:Description rdf:about=\"\" ex:attr=\"attr value\">",
            "<ex:plain>plain text</ex:plain>",
            "<ex:title><rdf:Alt>",
            "<rdf:li xml:lang=\"x-default\">first</rdf:li>",
            "<rdf:li xml:lang=\"fr\">premier</rdf:li>",
            "</rdf:Alt></ex:title>",
            "<ex:struct rdf:parseType=\"Resource\"><ex:leaf>v</ex:leaf></ex:struct>",
            "<ex:typed rdf:datatype=\"urn:ex:Type\">7</ex:typed>",
            "</rdf:Description></rdf:RDF>"
        ),
        RDF
    );
    let xmp = XmpMetadata::parse(&packet).expect("parse");

    assert_eq!(xmp.text("{urn:ex}attr"), Some("attr value"));
    assert_eq!(xmp.text("{urn:ex}plain"), Some("plain text"));
    assert_eq!(xmp.text("{urn:ex}title"), Some("first"));
    assert_eq!(xmp.text("{urn:ex}struct"), None);
    assert_eq!(xmp.text("{urn:ex}typed"), None);
    assert_eq!(xmp.text("{urn:ex}missing"), None);
}

#[test]
fn malformed_parse_type_and_typed_structures_are_rejected() {
    let cases = [
        format!(
            r#"<rdf:RDF xmlns:rdf="{RDF}" xmlns:ex="urn:ex"><rdf:Description><ex:p rdf:parseType="Literal">x</ex:p></rdf:Description></rdf:RDF>"#
        ),
        format!(
            r#"<rdf:RDF xmlns:rdf="{RDF}" xmlns:ex="urn:ex"><rdf:Description><ex:p rdf:parseType="Collection"><ex:q/></ex:p></rdf:Description></rdf:RDF>"#
        ),
        format!(
            r#"<rdf:RDF xmlns:rdf="{RDF}" xmlns:ex="urn:ex"><rdf:Description><ex:p><rdf:Description rdf:datatype="urn:ex:T"><ex:q>1</ex:q></rdf:Description></ex:p></rdf:Description></rdf:RDF>"#
        ),
        format!(
            r#"<rdf:RDF xmlns:rdf="{RDF}" xmlns:ex="urn:ex"><rdf:Description><ex:p><rdf:Description rdf:datatype="urn:ex:T"/></ex:p></rdf:Description></rdf:RDF>"#
        ),
        format!(
            r#"<rdf:RDF xmlns:rdf="{RDF}" xmlns:ex="urn:ex"><rdf:Description><ex:p rdf:datatype="not a uri">x</ex:p></rdf:Description></rdf:RDF>"#
        ),
        format!(
            r#"<rdf:RDF xmlns:rdf="{RDF}" xmlns:ex="urn:ex"><rdf:Description><ex:p rdf:datatype="">x</ex:p></rdf:Description></rdf:RDF>"#
        ),
        format!(
            r#"<rdf:RDF xmlns:rdf="{RDF}" xmlns:ex="urn:ex"><rdf:Description><ex:p rdf:datatype="urn:ex:T"><ex:q/></ex:p></rdf:Description></rdf:RDF>"#
        ),
        format!(
            r#"<rdf:RDF xmlns:rdf="{RDF}" xmlns:ex="urn:ex"><rdf:Description><ex:p rdf:parseType="Resource" rdf:datatype="urn:ex:T"/></rdf:Description></rdf:RDF>"#
        ),
    ];
    for packet in cases {
        assert!(
            matches!(XmpMetadata::parse(&packet), Err(XmpError::Malformed(_))),
            "packet should be rejected: {packet}"
        );
    }
}

#[test]
fn nested_resource_limits_are_enforced() {
    let mut deep = format!("<rdf:RDF xmlns:rdf=\"{RDF}\" xmlns:ex=\"urn:ex\">");
    deep.push_str("<rdf:Description>");
    for _ in 0..40 {
        deep.push_str("<ex:n rdf:parseType=\"Resource\">");
    }
    for _ in 0..40 {
        deep.push_str("</ex:n>");
    }
    deep.push_str("</rdf:Description></rdf:RDF>");
    assert_eq!(
        XmpMetadata::parse(&deep).expect_err("nested depth limit"),
        XmpError::LimitExceeded("XMP element depth")
    );

    // 嵌套资源的属性计入与顶层共用的属性预算。
    let mut attrs = format!("<rdf:RDF xmlns:rdf=\"{RDF}\" xmlns:ex=\"urn:ex\">");
    attrs.push_str("<rdf:Description><ex:root rdf:parseType=\"Resource\"");
    for index in 0..1025 {
        attrs.push_str(&format!(" ex:p{index}=\"v\""));
    }
    attrs.push_str("/></rdf:Description></rdf:RDF>");
    assert_eq!(
        XmpMetadata::parse(&attrs).expect_err("nested property limit"),
        XmpError::LimitExceeded("XMP property count")
    );

    // 嵌套资源内的元素仍受元素总数上限约束。
    let mut wide = format!("<rdf:RDF xmlns:rdf=\"{RDF}\" xmlns:ex=\"urn:ex\">");
    wide.push_str("<rdf:Description><ex:root rdf:parseType=\"Resource\">");
    for _ in 0..4100 {
        wide.push_str("<ex:i/>");
    }
    wide.push_str("</ex:root></rdf:Description></rdf:RDF>");
    assert_eq!(
        XmpMetadata::parse(&wide).expect_err("nested element limit"),
        XmpError::LimitExceeded("XMP element count")
    );
}

#[test]
fn nested_resource_resolves_namespace_prefixes() {
    let packet = format!(
        concat!(
            "<rdf:RDF xmlns:rdf=\"{}\" xmlns:ex=\"urn:ex:outer\">",
            "<rdf:Description rdf:about=\"\">",
            "<ex:outer rdf:parseType=\"Resource\">",
            "<ex:plain>outside</ex:plain>",
            "<ex:inner rdf:parseType=\"Resource\">",
            "<ex:leaf xmlns:ex=\"urn:ex:inner\">inside</ex:leaf>",
            "<ex:other>other</ex:other>",
            "</ex:inner>",
            "<ex:after>after</ex:after>",
            "</ex:outer>",
            "</rdf:Description></rdf:RDF>"
        ),
        RDF
    );
    let xmp = XmpMetadata::parse(&packet).expect("parse");

    let Some(XmpValue::Resource(outer)) = xmp.properties.get("{urn:ex:outer}outer") else {
        panic!("ex:outer must be a resource");
    };
    assert_eq!(
        outer.get("{urn:ex:outer}plain"),
        Some(&XmpValue::Text("outside".into()))
    );
    assert_eq!(
        outer.get("{urn:ex:outer}after"),
        Some(&XmpValue::Text("after".into()))
    );
    let Some(XmpValue::Resource(inner)) = outer.get("{urn:ex:outer}inner") else {
        panic!("ex:inner must be a nested resource");
    };
    // 前缀在 leaf 上重定义，只影响该元素及其子树。
    assert_eq!(
        inner.get("{urn:ex:inner}leaf"),
        Some(&XmpValue::Text("inside".into()))
    );
    assert_eq!(
        inner.get("{urn:ex:outer}other"),
        Some(&XmpValue::Text("other".into()))
    );
    assert!(inner.get("{urn:ex:inner}other").is_none());
}
