//! 目录 `/Perms`（12.8.5 Table 268）。

use super::*;

/// 构造带目录 `/Perms` 的签名 PDF（Table 268）。文档仅作结构用途，
/// ByteRange 不做定点收敛（/Perms 测试不校验摘要）。
fn signed_file_with_perms(perms: Dict) -> Vec<u8> {
    let doc = build_sig_document(PLACEHOLDER, [0; 4], None, &[]);
    let catalog_value = doc
        .get_object(1)
        .expect("catalog")
        .expect("catalog object")
        .value
        .clone();
    let mut dict = match catalog_value {
        Value::Dict(dict) => dict,
        _ => panic!("catalog must be a dictionary"),
    };
    dict = dict.insert("Perms", Value::Dict(perms));
    doc.set_object(Object::new(1, 0, Value::Dict(dict)));
    let mut buf = Vec::new();
    DocumentWriter::new()
        .with_version(PdfVersion::V1_4)
        .build(&doc, &mut buf)
        .expect("perms build");
    buf
}

#[test]
fn perms_absent_returns_none_and_empty_signatures() {
    let doc = parse_bytes(&signed_file(), "perms-absent");
    assert_eq!(
        doc.perms_entry(PermsEntryName::DocumentPermissions)
            .expect("none"),
        None
    );
    assert_eq!(doc.perms_signatures().expect("empty").len(), 0);
    assert_eq!(
        doc.perms_document_modification_permission().expect("none"),
        None
    );
}

#[test]
fn perms_docmdp_resolves_indirect_signature_and_permission() {
    let perms = Dict::new().insert("DocMDP", Value::Ref(4, 0));
    let doc = parse_bytes(&signed_file_with_perms(perms), "perms-docmdp");
    let entry = doc
        .perms_entry(PermsEntryName::DocumentPermissions)
        .expect("entry")
        .expect("present");
    assert_eq!(entry.name, PermsEntryName::DocumentPermissions);
    assert_eq!(entry.raw, Value::Ref(4, 0));
    let signature = entry.signature.as_ref().expect("signature resolves");
    assert_eq!(signature.reference, Some((4, 0)));
    assert_eq!(signature.name.as_deref(), Some("Signer"));
    // DocMDP 签名字典未带整数 /P：级别为 None，便捷入口同为 None。
    assert_eq!(entry.permission, None);
    assert_eq!(
        doc.perms_document_modification_permission().expect("none"),
        None
    );
}

#[test]
fn perms_docmdp_reads_permission_from_signature_dictionary() {
    let doc = build_sig_document(PLACEHOLDER, [0; 4], None, &[]);
    let sig_value = doc
        .get_object(4)
        .expect("sig")
        .expect("sig object")
        .value
        .clone();
    let mut sig_dict = match sig_value {
        Value::Dict(dict) => dict,
        _ => panic!("signature must be a dictionary"),
    };
    sig_dict = sig_dict.insert("P", Value::Integer(2));
    doc.set_object(Object::new(4, 0, Value::Dict(sig_dict)));
    let perms = Dict::new().insert("DocMDP", Value::Ref(4, 0));
    let catalog_value = doc
        .get_object(1)
        .expect("catalog")
        .expect("catalog object")
        .value
        .clone();
    let mut catalog = match catalog_value {
        Value::Dict(dict) => dict,
        _ => panic!("catalog must be a dictionary"),
    };
    catalog = catalog.insert("Perms", Value::Dict(perms));
    doc.set_object(Object::new(1, 0, Value::Dict(catalog)));
    let mut buf = Vec::new();
    DocumentWriter::new()
        .with_version(PdfVersion::V1_4)
        .build(&doc, &mut buf)
        .expect("perms p build");
    let parsed = parse_bytes(&buf, "perms-docmdp-p");
    assert_eq!(
        parsed
            .perms_document_modification_permission()
            .expect("permission"),
        Some(DocmdpPermission::FillAndAnnotate)
    );
}

#[test]
fn perms_sig_array_returns_all_signatures() {
    let perms = Dict::new().insert(
        "Sig",
        Value::Array(crate::core::types::Array::new().push(Value::Ref(4, 0))),
    );
    let doc = parse_bytes(&signed_file_with_perms(perms), "perms-sig");
    let sigs = doc.perms_signatures().expect("perms sigs");
    assert_eq!(sigs.len(), 1);
    assert_eq!(sigs[0].reference, Some((4, 0)));
    assert_eq!(sigs[0].sub_filter, Some(SignatureSubFilter::Pkcs7Detached));
}

#[test]
fn perms_sig_flags_entry_keeps_raw_value_without_signature() {
    let perms = Dict::new()
        .insert("SigFlags", Value::Integer(1))
        .insert("Weird", Value::Name("Something".into()));
    let doc = parse_bytes(&signed_file_with_perms(perms), "perms-flags");
    let entry = doc
        .perms_entry(PermsEntryName::SignatureFlag)
        .expect("entry")
        .expect("present");
    assert_eq!(entry.raw, Value::Integer(1));
    assert!(entry.signature.is_none());
    let unknown = doc
        .perms_entry(PermsEntryName::from_key("Weird"))
        .expect("unknown entry")
        .expect("present");
    assert_eq!(unknown.name, PermsEntryName::Unknown("Weird".into()));
}

#[test]
fn perms_malformed_shapes_are_typed_errors() {
    // /Perms 不是字典。
    let doc = build_sig_document(PLACEHOLDER, [0; 4], None, &[]);
    let catalog_value = doc
        .get_object(1)
        .expect("catalog")
        .expect("catalog object")
        .value
        .clone();
    let mut catalog = match catalog_value {
        Value::Dict(dict) => dict,
        _ => panic!("catalog must be a dictionary"),
    };
    catalog = catalog.insert("Perms", Value::Integer(7));
    doc.set_object(Object::new(1, 0, Value::Dict(catalog)));
    let mut buf = Vec::new();
    DocumentWriter::new()
        .with_version(PdfVersion::V1_4)
        .build(&doc, &mut buf)
        .expect("bad perms build");
    let parsed = parse_bytes(&buf, "perms-not-dict");
    let error = parsed
        .perms_entry(PermsEntryName::DocumentPermissions)
        .expect_err("must error");
    assert!(matches!(error, SignatureError::Malformed(_)));
    // /Perms /DocMDP /P 越界：签名字典带非法 /P。
    let doc2 = build_sig_document(PLACEHOLDER, [0; 4], None, &[]);
    let sig_value = doc2
        .get_object(4)
        .expect("sig")
        .expect("sig object")
        .value
        .clone();
    let mut sig_dict = match sig_value {
        Value::Dict(dict) => dict,
        _ => panic!("signature must be a dictionary"),
    };
    sig_dict = sig_dict.insert("P", Value::Integer(9));
    doc2.set_object(Object::new(4, 0, Value::Dict(sig_dict)));
    let catalog_value2 = doc2
        .get_object(1)
        .expect("catalog")
        .expect("catalog object")
        .value
        .clone();
    let mut catalog2 = match catalog_value2 {
        Value::Dict(dict) => dict,
        _ => panic!("catalog must be a dictionary"),
    };
    catalog2 = catalog2.insert(
        "Perms",
        Value::Dict(Dict::new().insert("DocMDP", Value::Ref(4, 0))),
    );
    doc2.set_object(Object::new(1, 0, Value::Dict(catalog2)));
    let mut buf2 = Vec::new();
    DocumentWriter::new()
        .with_version(PdfVersion::V1_4)
        .build(&doc2, &mut buf2)
        .expect("bad p build");
    let parsed2 = parse_bytes(&buf2, "perms-bad-p");
    let error2 = parsed2
        .perms_entry(PermsEntryName::DocumentPermissions)
        .expect_err("out-of-range /P must error");
    assert!(matches!(error2, SignatureError::Malformed(_)));
}
