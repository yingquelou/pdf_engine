//! 签名字典模型、`/SubFilter` 与 `/ByteRange` 结构错误。

use super::*;

#[test]
fn signature_flags_exposed_on_catalog() {
    let (doc, _, _) = parsed_signed("flags");
    assert_eq!(doc.signature_flags().expect("flags"), Some(3));
}

#[test]
fn document_signatures_finds_sig_field_value() {
    let (doc, _, sig) = parsed_signed("discover");
    assert_eq!(sig.reference, Some((4, 0)));
    assert_eq!(sig.filter.as_deref(), Some("Adobe.PPKLite"));
    assert_eq!(sig.sub_filter, Some(SignatureSubFilter::Pkcs7Detached));
    assert_eq!(sig.name.as_deref(), Some("Signer"));
    assert_eq!(sig.location, None);
    assert_eq!(sig.reason, None);
    // 完整字典保留：未建模条目可经 dict 访问。
    assert!(sig.dict.get("M").is_some());
    let _ = &doc;
}

#[test]
fn signature_date_parses_to_pdf_date() {
    let (_, _, sig) = parsed_signed("date");
    let date = sig.date_parsed().expect("/M parses");
    assert_eq!(date.year(), 2026);
    assert_eq!(date.month(), Some(10));
    assert_eq!(date.day(), Some(28));
    assert_eq!(date.hour(), Some(0));
    assert_eq!(date.minute(), Some(0));
    assert_eq!(date.second(), Some(0));
    assert_eq!(date.to_pdf_string(), "D:20261028000000Z");
    assert_eq!(
        date.timezone(),
        crate::core::types::PdfDateTimezone::Utc,
        "/M carries an explicit UTC marker"
    );
}

#[test]
fn subfilter_mapping_and_default_digest() {
    assert_eq!(
        SignatureSubFilter::from_name("adbe.pkcs7.sha1").default_digest(),
        Some(DigestAlgorithm::Sha1)
    );
    assert_eq!(
        SignatureSubFilter::from_name("adbe.x509.rsa_sha1"),
        SignatureSubFilter::X509RsaSha1
    );
    assert_eq!(
        SignatureSubFilter::from_name("ETSI.CAdES.detached").default_digest(),
        None
    );
    assert_eq!(
        SignatureSubFilter::from_name("custom.vendor").as_str(),
        "custom.vendor"
    );
    let bytes = signed_file();
    let doc = parse_bytes(&bytes, "subfilter");
    // 换成未知 SubFilter 后仍可读取（名称原样保留）。
    let mut obj = doc.get_object(4).unwrap().unwrap();
    let Value::Dict(d) = obj.value.clone() else {
        panic!("sig dict");
    };
    obj.value = Value::Dict(d.insert("SubFilter", Value::Name("vendor.detached".into())));
    doc.set_object(obj);
    let sig = &doc.document_signatures().expect("unknown subfilter")[0];
    assert_eq!(
        sig.sub_filter,
        Some(SignatureSubFilter::Unknown("vendor.detached".into()))
    );
}

#[test]
fn malformed_byte_ranges_are_typed_errors() {
    let doc = build_sig_document("00", [0, 10, 20, 30], None, &[]);
    let value = Value::Ref(4, 0);
    assert!(doc.signature(&value).is_ok());
    // 数组长度 3。
    let bad = Object::new(
        5,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Sig".into()))
                .insert(
                    "ByteRange",
                    Value::Array(crate::core::types::Array::new().push(Value::Integer(0))),
                ),
        ),
    );
    doc.set_object(bad);
    let err = doc.signature(&Value::Ref(5, 0)).unwrap_err();
    assert!(matches!(err, SignatureError::Malformed(_)));
    // 含负数。
    let bad2 = Object::new(
        6,
        0,
        Value::Dict(
            Dict::new().insert(
                "ByteRange",
                Value::Array(
                    crate::core::types::Array::new()
                        .push(Value::Integer(0))
                        .push(Value::Integer(-1))
                        .push(Value::Integer(2))
                        .push(Value::Integer(3)),
                ),
            ),
        ),
    );
    doc.set_object(bad2);
    assert!(matches!(
        doc.signature(&Value::Ref(6, 0)).unwrap_err(),
        SignatureError::Malformed(_)
    ));
    // 非整数字段。
    let bad3 = Object::new(
        7,
        0,
        Value::Dict(
            Dict::new().insert(
                "ByteRange",
                Value::Array(
                    crate::core::types::Array::new()
                        .push(Value::Name("zero".into()))
                        .push(Value::Integer(1))
                        .push(Value::Integer(2))
                        .push(Value::Integer(3)),
                ),
            ),
        ),
    );
    doc.set_object(bad3);
    assert!(matches!(
        doc.signature(&Value::Ref(7, 0)).unwrap_err(),
        SignatureError::Malformed(_)
    ));
    // 首段起点非 0。
    let bad4 = Object::new(
        8,
        0,
        Value::Dict(
            Dict::new().insert(
                "ByteRange",
                Value::Array(
                    crate::core::types::Array::new()
                        .push(Value::Integer(4))
                        .push(Value::Integer(1))
                        .push(Value::Integer(2))
                        .push(Value::Integer(3)),
                ),
            ),
        ),
    );
    doc.set_object(bad4);
    let sig4 = doc.signature(&Value::Ref(8, 0)).expect("parses");
    assert!(matches!(
        doc.validate_signature_byte_range(&sig4).unwrap_err(),
        SignatureError::Malformed(_)
    ));
}

#[test]
fn signature_requires_dict_type_sig() {
    let doc = build_sig_document("00", [0, 1, 2, 3], None, &[]);
    doc.set_object(Object::new(
        9,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("EmbeddedFile".into()))
                .insert(
                    "ByteRange",
                    Value::Array(
                        crate::core::types::Array::new()
                            .push(Value::Integer(0))
                            .push(Value::Integer(1))
                            .push(Value::Integer(2))
                            .push(Value::Integer(3)),
                    ),
                ),
        ),
    ));
    assert!(matches!(
        doc.signature(&Value::Ref(9, 0)).unwrap_err(),
        SignatureError::Malformed(_)
    ));
    // /Type 缺失（宽容形态）仍可解析。
    doc.set_object(Object::new(
        10,
        0,
        Value::Dict(
            Dict::new().insert(
                "ByteRange",
                Value::Array(
                    crate::core::types::Array::new()
                        .push(Value::Integer(0))
                        .push(Value::Integer(1))
                        .push(Value::Integer(2))
                        .push(Value::Integer(3)),
                ),
            ),
        ),
    ));
    assert!(doc.signature(&Value::Ref(10, 0)).is_ok());
}

#[test]
fn signature_digest_requires_file_backed_document() {
    let doc = build_sig_document(PLACEHOLDER, [0, 10, 200, 50], None, &[]);
    let sig = doc.signature(&Value::Ref(4, 0)).expect("parses");
    let backend = RustCryptoSignatureBackend;
    assert_eq!(
        doc.signature_digest(&sig, DigestAlgorithm::Sha256, &backend)
            .unwrap_err(),
        SignatureError::NoSource
    );
}
