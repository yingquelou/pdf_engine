//! `/Reference` 变换参数（DocMDP / FieldMDP / UR）。

use super::*;

/// 变换参数字典（/Type /TransformParams + 方法专属条目）。
fn transform_dict(entries: Vec<(&str, Value)>) -> Value {
    let mut dict = Dict::new().insert("Type", Value::Name("TransformParams".into()));
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    Value::Dict(dict)
}

/// 签名字典引用条目（Table 257）。
fn reference_dict(method: &str, params: Value, extra: Vec<(&str, Value)>) -> Value {
    let mut dict = Dict::new()
        .insert("Type", Value::Name("SigReference".into()))
        .insert("TransformMethod", Value::Name(method.into()))
        .insert("TransformParams", params);
    for (key, value) in extra {
        dict = dict.insert(key, value);
    }
    Value::Dict(dict)
}

fn docmdp_reference(permission: i64) -> Value {
    reference_dict(
        "DocMDP",
        transform_dict(vec![("P", Value::Integer(permission))]),
        vec![],
    )
}

fn refs_array(items: Vec<Value>) -> Value {
    let mut array = crate::core::types::Array::new();
    for item in items {
        array = array.push(item);
    }
    Value::Array(array)
}

#[test]
fn docmdp_reference_exposes_permission_through_discovery() {
    let bytes = signed_file_with(Some(refs_array(vec![docmdp_reference(2)])), &[]);
    let doc = parse_bytes(&bytes, "docmdp");
    let sigs = doc.document_signatures().expect("signatures");
    let refs = doc.signature_references(&sigs[0]).expect("references");
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].transform_method, Some(TransformMethod::Docmdp));
    assert_eq!(
        refs[0].params,
        Some(TransformParams::Docmdp {
            permission: DocmdpPermission::FillAndAnnotate
        })
    );
    assert_eq!(
        doc.doc_modification_permission(&sigs[0]).expect("mdp"),
        Some(DocmdpPermission::FillAndAnnotate)
    );
}

#[test]
fn fieldmdp_reference_decodes_scope_and_annotation_bits() {
    let params = transform_dict(vec![
        ("P", Value::Name("Include".into())),
        ("Annots", Value::HexString("8040".into())),
    ]);
    let reference = reference_dict("FieldMDP", params, vec![]);
    let doc = build_sig_document(
        PLACEHOLDER,
        [0, 10, 20, 30],
        Some(refs_array(vec![reference])),
        &[],
    );
    let sig = doc.signature(&Value::Ref(4, 0)).expect("signature");
    let refs = doc.signature_references(&sig).expect("references");
    match &refs[0].params {
        Some(TransformParams::Fieldmdp {
            permission,
            annotations,
            ..
        }) => {
            assert_eq!(*permission, FieldMdPermission::Include);
            assert_eq!(annotations, &vec![0x80, 0x40]);
        }
        other => panic!("expected FieldMDP params, got {other:?}"),
    }
    assert_eq!(
        doc.field_modification_params(&sig)
            .expect("mdp")
            .map(|params| matches!(params, TransformParams::Fieldmdp { .. })),
        Some(true)
    );
}

#[test]
fn ur_reference_models_scope_version_and_reason() {
    let params = transform_dict(vec![
        ("V", Value::Integer(12)),
        (
            "Scope",
            Value::Array(
                crate::core::types::Array::new()
                    .push(Value::Name("Form".into()))
                    .push(Value::Name("Annotation".into())),
            ),
        ),
        ("NeedAppearances", Value::Bool(true)),
        ("Reason", Value::String("review".into())),
    ]);
    let doc = build_sig_document(
        PLACEHOLDER,
        [0, 10, 20, 30],
        Some(refs_array(vec![reference_dict("UR", params, vec![])])),
        &[],
    );
    let sig = doc.signature(&Value::Ref(4, 0)).expect("signature");
    let refs = doc.signature_references(&sig).expect("references");
    match &refs[0].params {
        Some(TransformParams::Ur {
            version,
            scope,
            needs_appearances,
            reason,
            ..
        }) => {
            assert_eq!(*version, 12);
            assert!(scope.form && scope.annotation && !scope.doc_attachment);
            assert!(needs_appearances);
            assert_eq!(reason.as_deref(), Some("review"));
        }
        other => panic!("expected UR params, got {other:?}"),
    }
}

#[test]
fn timestamp_reference_without_method_is_skipped() {
    // 12.8.4.4：无 /TransformMethod 的时间戳引用不进变换模型。
    let timestamp = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("SigReference".into()))
            .insert("RelativePeriod", Value::Integer(86_400)),
    );
    let doc = build_sig_document(
        PLACEHOLDER,
        [0, 10, 20, 30],
        Some(refs_array(vec![timestamp])),
        &[],
    );
    let sig = doc.signature(&Value::Ref(4, 0)).expect("signature");
    assert!(
        doc.signature_references(&sig)
            .expect("references")
            .is_empty()
    );
}

#[test]
fn malformed_transform_params_are_typed_errors() {
    let cases = vec![
        // DocMDP 缺 /P。
        reference_dict("DocMDP", transform_dict(vec![]), vec![]),
        // DocMDP /P 越界。
        docmdp_reference(4),
        // FieldMDP /P 非法名。
        reference_dict(
            "FieldMDP",
            transform_dict(vec![("P", Value::Name("Some".into()))]),
            vec![],
        ),
        // UR 缺 /Scope。
        reference_dict(
            "UR",
            transform_dict(vec![("V", Value::Integer(12))]),
            vec![],
        ),
        // 未知变换方法。
        reference_dict("SomethingElse", transform_dict(vec![]), vec![]),
    ];
    for case in cases {
        let doc = build_sig_document(
            PLACEHOLDER,
            [0, 10, 20, 30],
            Some(refs_array(vec![case])),
            &[],
        );
        let sig = doc.signature(&Value::Ref(4, 0)).expect("signature");
        assert!(
            matches!(
                doc.signature_references(&sig),
                Err(SignatureError::Malformed(_))
            ),
            "expected malformed error"
        );
    }
}

#[test]
fn reference_digest_matches_embedded_pdf_stream() {
    // 12.8.3：/DigestValue 是 /Data 指向的嵌入 PDF 流字节的摘要，
    // /DigestLength 声明其字节数。
    const EMBEDDED: &[u8] = b"%PDF-1.4 embedded AAAA";
    let digest = {
        let backend = RustCryptoSignatureBackend;
        let mut sink = backend.start(DigestAlgorithm::Sha256);
        sink.push(EMBEDDED);
        sink.finish()
    };
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<String>();
    let data_stream = Object::new(
        6,
        0,
        Value::Stream(
            Dict::new()
                .insert("Type", Value::Name("EmbeddedPDF".into()))
                .insert("Subtype", Value::Name("Application/pdf".into())),
            crate::core::types::StreamData::Owned(EMBEDDED.to_vec()),
        ),
    );
    let reference = reference_dict(
        "DocMDP",
        transform_dict(vec![("P", Value::Integer(1))]),
        vec![
            ("Data", Value::Ref(6, 0)),
            ("DigestValue", Value::HexString(hex)),
            ("DigestLength", Value::Integer(32)),
        ],
    );
    let bytes = signed_file_with(
        Some(Value::Array(
            crate::core::types::Array::new().push(Value::Ref(5, 0)),
        )),
        &[
            Object::new(
                5,
                0,
                match reference {
                    Value::Dict(dict) => Value::Dict(dict),
                    _ => unreachable!(),
                },
            ),
            data_stream,
        ],
    );
    let doc = parse_bytes(&bytes, "digest-ref");
    let sig = doc.document_signatures().expect("signatures").remove(0);
    let refs = doc.signature_references(&sig).expect("references");
    assert_eq!(refs[0].data_reference(), Some((6, 0)));
    assert_eq!(
        refs[0].matches_digest_length(DigestAlgorithm::Sha256),
        Some(true)
    );
    let data_value = doc.get_object(6).expect("object").unwrap().value;
    let embedded = doc
        .decode_stream_bytes(&data_value)
        .expect("embedded bytes");
    assert_eq!(embedded, EMBEDDED);
    let backend = RustCryptoSignatureBackend;
    let mut sink = backend.start(DigestAlgorithm::Sha256);
    sink.push(&embedded);
    let recomputed = sink.finish();
    let Value::HexString(declared_hex) = refs[0].digest_value.as_ref().expect("digest") else {
        panic!("digest value shape");
    };
    let declared = (0..declared_hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&declared_hex[i..i + 2], 16).unwrap())
        .collect::<Vec<u8>>();
    assert_eq!(recomputed, declared);
    // 篡改嵌入字节（等长替换保持文件结构）后摘要不再匹配。
    let at = find_occurrence(&bytes, b"AAAA").expect("embedded marker");
    let mut mutated = bytes.clone();
    mutated[at..at + 4].copy_from_slice(b"BBBB");
    let doc2 = parse_bytes(&mutated, "digest-ref-tampered");
    let data2 = doc2.get_object(6).expect("object").unwrap().value;
    let embedded2 = doc2.decode_stream_bytes(&data2).expect("embedded2");
    let mut sink2 = backend.start(DigestAlgorithm::Sha256);
    sink2.push(&embedded2);
    assert_ne!(sink2.finish(), declared);
}
