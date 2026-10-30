//! `rdf:parseType="Collection"` 集合解析测试（14.3.2）。

use super::*;

#[test]
fn xmp_collection_parses_item_shapes() {
    let packet = format!(
        concat!(
            "<rdf:RDF xmlns:rdf=\"{}\" xmlns:ex=\"urn:ex\" xmlns:x=\"urn:x\">",
            "<rdf:Description rdf:about=\"\">",
            "<ex:items rdf:parseType=\"Collection\">",
            "<rdf:li>plain</rdf:li>",
            "<rdf:Description ex:name=\"Ada\"/>",
            "<ex:ref rdf:resource=\"urn:ex:r\"/>",
            "<ex:markup rdf:parseType=\"Literal\"><x:b>hi</x:b></ex:markup>",
            "<ex:typed rdf:parseType=\"Resource\">",
            "<rdf:value>42</rdf:value>",
            "<rdf:datatype>http://www.w3.org/2001/XMLSchema#integer</rdf:datatype>",
            "</ex:typed>",
            "<ex:nested rdf:parseType=\"Collection\"/>",
            "</ex:items>",
            "<ex:empty rdf:parseType=\"Collection\"/>",
            "</rdf:Description></rdf:RDF>"
        ),
        RDF
    );
    let xmp = XmpMetadata::parse(&packet).expect("parse collection packet");

    assert_eq!(
        xmp.properties.get("{urn:ex}items"),
        Some(&XmpValue::Collection(vec![
            XmpValue::Text("plain".into()),
            XmpValue::Resource(BTreeMap::from([(
                "{urn:ex}name".to_string(),
                XmpValue::Text("Ada".into()),
            )])),
            XmpValue::Reference("urn:ex:r".into()),
            XmpValue::Literal("<x:b>hi</x:b>".into()),
            XmpValue::Typed {
                value: Box::new(XmpValue::Text("42".into())),
                datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
                language: None,
            },
            XmpValue::Collection(Vec::new()),
        ]))
    );
    assert_eq!(
        xmp.properties.get("{urn:ex}empty"),
        Some(&XmpValue::Collection(Vec::new()))
    );
    assert_eq!(xmp.text("{urn:ex}items"), None);
    assert_eq!(xmp.text("{urn:ex}empty"), None);
}

#[test]
fn xmp_collection_rejects_text_and_mixed_items() {
    let text_content = format!(
        concat!(
            "<rdf:RDF xmlns:rdf=\"{}\" xmlns:ex=\"urn:ex\">",
            "<rdf:Description><ex:p rdf:parseType=\"Collection\">",
            "stray<rdf:li>ok</rdf:li>",
            "</ex:p></rdf:Description></rdf:RDF>"
        ),
        RDF
    );
    match XmpMetadata::parse(&text_content) {
        Err(XmpError::Malformed(message)) => assert!(
            message.contains("must not contain text content"),
            "{message}"
        ),
        other => panic!("collection text content must be rejected, got {other:?}"),
    }

    let mixed_item = format!(
        concat!(
            "<rdf:RDF xmlns:rdf=\"{}\" xmlns:ex=\"urn:ex\">",
            "<rdf:Description><ex:p rdf:parseType=\"Collection\">",
            "<rdf:li>text<ex:q/></rdf:li>",
            "</ex:p></rdf:Description></rdf:RDF>"
        ),
        RDF
    );
    match XmpMetadata::parse(&mixed_item) {
        Err(XmpError::Malformed(message)) => assert!(
            message.contains("must not mix text with child elements"),
            "{message}"
        ),
        other => panic!("mixed collection item must be rejected, got {other:?}"),
    }
}
