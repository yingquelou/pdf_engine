//! `messageDigest` 与 `/ByteRange` 摘要绑定校验（含增量签名端到端）。

use crate::core::writer::{SignedIncremental, SigningSpec};

use super::der::*;
use super::*;

/// 最小基线文档：Catalog（1）+ 载荷字典（2），trailer 只有 `/Root`。
fn base_pdf_bytes() -> Vec<u8> {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(Dict::new().insert("Payload", Value::Integer(42))),
    ));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    let mut buf = Vec::new();
    DocumentWriter::new()
        .with_version(PdfVersion::V1_4)
        .build(&doc, &mut buf)
        .expect("base build");
    buf
}

/// 用给定 CMS 长度与签名器回填 `/Contents`，再回读为 `Document`。
fn signed_document_with_signer(
    tag: &str,
    cms_len: usize,
    signer: impl Fn(&[u8]) -> Vec<u8> + 'static,
) -> (Document, SignedIncremental) {
    let base = base_pdf_bytes();
    let source = parse_bytes(&base, tag);
    let spec = SigningSpec::new(DigestAlgorithm::Sha256, cms_len).with_signer(cms_len, signer);
    let signed = DocumentWriter::new()
        .sign_incremental(&source, &spec, &RustCryptoSignatureBackend)
        .expect("incremental signing");
    let doc = parse_bytes(&signed.bytes, &format!("{tag}-read"));
    (doc, signed)
}

/// 固定 CMS 内容（长度即占位宽度）。
fn signed_document_with_cms(tag: &str, cms: Vec<u8>) -> (Document, SignedIncremental) {
    let cms_len = cms.len();
    signed_document_with_signer(tag, cms_len, move |_| cms.clone())
}

#[test]
fn signature_integrity_matches_cms_message_digest() {
    let cms_len = cms_with_message_digest(&[0u8; 32]).len();
    let (doc, signed) = signed_document_with_signer("cms-match", cms_len, cms_with_message_digest);
    let sigs = doc.document_signatures().expect("signatures");
    assert_eq!(sigs.len(), 1);
    assert_eq!(sigs[0].byte_range, signed.byte_range);

    let cms = doc
        .signature_cms(&sigs[0])
        .expect("CMS parses")
        .expect("CMS present");
    assert_eq!(cms.signers.len(), 1);
    assert_eq!(cms.content_type, OID_DATA);

    let integrity = doc
        .verify_signature_integrity(&sigs[0], &RustCryptoSignatureBackend)
        .expect("integrity check");
    assert!(integrity.all_matched());
    assert_eq!(integrity.signers, vec![SignerIntegrity::Match]);
    let digest = doc
        .signature_digest(
            &sigs[0],
            DigestAlgorithm::Sha256,
            &RustCryptoSignatureBackend,
        )
        .expect("digest");
    assert_eq!(
        integrity.cms.signers[0].message_digest.as_deref(),
        Some(digest.as_slice()),
        "CMS messageDigest 必须等于 ByteRange 覆盖字节的 SHA-256"
    );
    assert_eq!(
        integrity.cms.signers[0]
            .signing_time
            .as_ref()
            .map(|time| time.text.as_str()),
        Some("261028000000Z")
    );
}

#[test]
fn signature_integrity_reports_mismatch_for_wrong_message_digest() {
    let cms = cms_with_message_digest(&[0xAA; 32]);
    let (doc, _) = signed_document_with_cms("cms-mismatch", cms);
    let sig = &doc.document_signatures().expect("signatures")[0];
    let integrity = doc
        .verify_signature_integrity(sig, &RustCryptoSignatureBackend)
        .expect("integrity check");
    assert!(!integrity.all_matched());
    match &integrity.signers[0] {
        SignerIntegrity::Mismatch { expected, actual } => {
            assert_eq!(expected, &vec![0xAA; 32], "CMS 声明的摘要");
            assert_eq!(actual.len(), 32);
            assert_ne!(actual, expected);
        }
        other => panic!("expected Mismatch, got {other:?}"),
    }
}

#[test]
fn signature_integrity_reports_missing_message_digest_attribute() {
    let cms = cms_with_attributes(&[signing_time_attribute("261028000000Z")], OID_SHA256);
    let (doc, _) = signed_document_with_cms("cms-missing", cms);
    let sig = &doc.document_signatures().expect("signatures")[0];
    let integrity = doc
        .verify_signature_integrity(sig, &RustCryptoSignatureBackend)
        .expect("integrity check");
    assert_eq!(integrity.signers, vec![SignerIntegrity::MissingAttribute]);
    assert!(!integrity.all_matched());
}

#[test]
fn signature_integrity_reports_unsupported_digest_algorithm() {
    let cms = cms_with_attributes(&[message_digest_attribute(&[0u8; 32])], OID_UNKNOWN_DIGEST);
    let (doc, _) = signed_document_with_cms("cms-unknown", cms);
    let sig = &doc.document_signatures().expect("signatures")[0];
    let integrity = doc
        .verify_signature_integrity(sig, &RustCryptoSignatureBackend)
        .expect("integrity check");
    assert_eq!(
        integrity.signers,
        vec![SignerIntegrity::UnsupportedAlgorithm]
    );
    assert_eq!(
        integrity.cms.signers[0].digest_algorithm,
        CmsDigestAlgorithm::Other(OID_UNKNOWN_DIGEST.to_string())
    );
}

#[test]
fn signature_cms_absent_empty_and_placeholder_contents_are_none() {
    // 全零占位（未回填的签名占位）。
    let doc = build_sig_document(PLACEHOLDER, [0, 10, 20, 30], None, &[]);
    let sig = doc.signature(&Value::Ref(4, 0)).expect("signature");
    assert_eq!(doc.signature_cms(&sig).expect("placeholder"), None);

    // 空字符串 /Contents。
    let empty = build_sig_document("", [0, 10, 20, 30], None, &[]);
    let empty_sig = empty.signature(&Value::Ref(4, 0)).expect("signature");
    assert_eq!(empty.signature_cms(&empty_sig).expect("empty"), None);

    // /Contents 缺失。
    let missing = build_sig_document(PLACEHOLDER, [0, 10, 20, 30], None, &[]);
    missing.set_object(Object::new(
        4,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Sig".into()))
                .insert(
                    "ByteRange",
                    Value::Array(
                        crate::core::types::Array::new()
                            .push(Value::Integer(0))
                            .push(Value::Integer(10))
                            .push(Value::Integer(20))
                            .push(Value::Integer(30)),
                    ),
                ),
        ),
    ));
    let missing_sig = missing.signature(&Value::Ref(4, 0)).expect("signature");
    assert_eq!(missing.signature_cms(&missing_sig).expect("missing"), None);
}

#[test]
fn verify_signature_integrity_without_cms_is_typed_error() {
    let (doc, _, sig) = parsed_signed("cms-no-contents");
    assert_eq!(
        doc.verify_signature_integrity(&sig, &RustCryptoSignatureBackend)
            .unwrap_err(),
        SignatureError::NoContents
    );
}

#[test]
fn signature_cms_rejects_malformed_contents_der() {
    let doc = build_sig_document("3100", [0, 10, 20, 30], None, &[]);
    let sig = doc.signature(&Value::Ref(4, 0)).expect("signature");
    let error = doc.signature_cms(&sig).expect_err("bad DER must be typed");
    assert!(matches!(
        error,
        SignatureError::Cms(CmsError::UnexpectedTag { .. })
    ));
}

#[test]
fn signature_cms_rejects_non_string_contents() {
    let doc = build_sig_document(PLACEHOLDER, [0, 10, 20, 30], None, &[]);
    doc.set_object(Object::new(
        4,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Sig".into()))
                .insert("Contents", Value::Integer(7))
                .insert(
                    "ByteRange",
                    Value::Array(
                        crate::core::types::Array::new()
                            .push(Value::Integer(0))
                            .push(Value::Integer(10))
                            .push(Value::Integer(20))
                            .push(Value::Integer(30)),
                    ),
                ),
        ),
    ));
    let sig = doc.signature(&Value::Ref(4, 0)).expect("signature");
    assert!(matches!(
        doc.signature_cms(&sig),
        Err(SignatureError::Malformed(_))
    ));
}
