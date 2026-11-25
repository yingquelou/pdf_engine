//! F1.3 验证编排：摘要绑定、后端签名值/链验证与时间戳原样暴露。

use crate::core::reader::signature::{
    ChainCheck, CmsAlgorithmIdentifier, SignatureCheck, SignatureVerifyBackend, TimestampCheck,
    VerifyOptions,
};
use crate::core::writer::{DocumentWriter, SigningSpec};

use super::der::*;
use super::*;
use crate::core::types::{Array, Object, StreamData};

#[derive(Debug, Clone, Copy)]
struct StubVerifyBackend {
    signature: SignatureCheck,
    chain: ChainCheck,
    timestamp: TimestampCheck,
}

impl SignatureVerifyBackend for StubVerifyBackend {
    fn verify_signature(
        &self,
        _signed_attrs_der: &[u8],
        _signature: &[u8],
        _algorithm: &CmsAlgorithmIdentifier,
        _certificates: &[Vec<u8>],
    ) -> SignatureCheck {
        self.signature
    }

    fn verify_chain(&self, _certificates: &[Vec<u8>], _anchors: &[Vec<u8>]) -> ChainCheck {
        self.chain
    }

    fn verify_timestamp(&self, _token_der: &[u8]) -> TimestampCheck {
        self.timestamp
    }
}

/// 最小基线 + 增量签名，签名器按覆盖字节摘要产出 CMS（长度固定）。
fn signed_doc_with_cms(tag: &str, cms: impl Fn(&[u8]) -> Vec<u8> + 'static) -> (Document, Vec<u8>) {
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
    let mut base = Vec::new();
    DocumentWriter::new()
        .with_version(PdfVersion::V1_4)
        .build(&doc, &mut base)
        .expect("base build");
    let source = parse_bytes(&base, &format!("{tag}-base"));
    let len = cms(&[0u8; 32]).len();
    let spec = SigningSpec::new(DigestAlgorithm::Sha256, len).with_signer(len, cms);
    let signed = DocumentWriter::new()
        .sign_incremental(&source, &spec, &RustCryptoSignatureBackend)
        .expect("incremental signing");
    let parsed = parse_bytes(&signed.bytes, &format!("{tag}-read"));
    (parsed, signed.bytes)
}

/// 带一份证书的 CMS：链验证才能区分「注入锚可信」与「未注入未提供」。
fn cms_with_certificate(digest: &[u8]) -> Vec<u8> {
    let signer = signer_info(&SignerParts {
        digest_oid: OID_SHA256,
        attributes: Some(signed_attrs(&[message_digest_attribute(digest)])),
        subject_key_id: None,
        signature: RAW_SIGNATURE,
    });
    content_info(&signed_data(&SignedDataParts {
        version: 1,
        digest_oids: &[OID_SHA256],
        encapsulated: None,
        certificates: &[sequence(&[integer(1)])],
        crls: None,
        signers: &[signer],
    }))
}

#[test]
fn without_backend_nothing_is_reported_as_verified() {
    let (doc, _) = signed_doc_with_cms("verify-none", |digest| cms_with_message_digest(digest));
    let sigs = doc.document_signatures().expect("signatures");
    let report = doc
        .verify_signature(
            &sigs[0],
            &VerifyOptions::default(),
            &RustCryptoSignatureBackend,
            None,
        )
        .expect("verify without backend");
    assert_eq!(report.signers.len(), 1);
    assert_eq!(report.signers[0].integrity, SignerIntegrity::Match);
    assert_eq!(report.signers[0].signature, SignatureCheck::NotProvided);
    assert_eq!(report.signers[0].chain, ChainCheck::NotProvided);
    assert!(!report.all_matched(), "未注入后端不得报通过");
}

#[test]
fn tampered_covered_bytes_report_mismatch() {
    let (_, bytes) = signed_doc_with_cms("verify-tamper", |digest| cms_with_message_digest(digest));
    let needle = b"Payload 42";
    let at = bytes
        .windows(needle.len())
        .position(|window| window == needle)
        .expect("payload in covered bytes");
    let mut tampered = bytes;
    tampered[at + needle.len() - 1] = b'3';
    let doc = parse_bytes(&tampered, "verify-tamper-read");
    let sigs = doc.document_signatures().expect("signatures");
    let backend = StubVerifyBackend {
        signature: SignatureCheck::Match,
        chain: ChainCheck::Trusted,
        timestamp: TimestampCheck::Valid,
    };
    let options = VerifyOptions {
        trust_anchors: vec![vec![0x30, 0x00]],
    };
    let report = doc
        .verify_signature(
            &sigs[0],
            &options,
            &RustCryptoSignatureBackend,
            Some(&backend),
        )
        .expect("verify tampered");
    assert!(
        matches!(
            report.signers[0].integrity,
            SignerIntegrity::Mismatch { .. }
        ),
        "篡改覆盖字节后 messageDigest 绑定必须为不匹配"
    );
    assert!(!report.all_matched(), "覆盖字节被改动后不得通过");
}

#[test]
fn trusted_anchor_yields_trusted_chain_missing_anchor_is_not_provided() {
    let (doc, _) = signed_doc_with_cms("verify-chain", cms_with_certificate);
    let sigs = doc.document_signatures().expect("signatures");
    let backend = StubVerifyBackend {
        signature: SignatureCheck::Match,
        chain: ChainCheck::Trusted,
        timestamp: TimestampCheck::Valid,
    };
    let with_anchor = VerifyOptions {
        trust_anchors: vec![vec![0x30, 0x00]],
    };
    let report = doc
        .verify_signature(
            &sigs[0],
            &with_anchor,
            &RustCryptoSignatureBackend,
            Some(&backend),
        )
        .expect("verify with anchor");
    assert_eq!(report.signers[0].chain, ChainCheck::Trusted);
    assert!(report.all_matched());

    let no_anchor = VerifyOptions::default();
    let report = doc
        .verify_signature(
            &sigs[0],
            &no_anchor,
            &RustCryptoSignatureBackend,
            Some(&backend),
        )
        .expect("verify without anchor");
    assert_eq!(report.signers[0].chain, ChainCheck::NotProvided);
    assert!(!report.all_matched());
}

#[test]
fn timestamp_reference_is_exposed_raw_and_delegated() {
    let (doc, _) = signed_doc_with_cms("verify-ts", |digest| cms_with_message_digest(digest));
    let mut signature = doc
        .document_signatures()
        .expect("signatures")
        .into_iter()
        .next()
        .expect("one signature");
    const TOKEN: &[u8] = b"RFC3161-TOKEN";
    doc.set_object(Object::new(
        9,
        0,
        Value::Stream(
            Dict::new().insert("Type", Value::Name("TimeStampToken".into())),
            StreamData::Owned(TOKEN.to_vec()),
        ),
    ));
    signature.references = Some(Value::Array(
        Array::new().push(Value::Dict(
            Dict::new()
                .insert("Data", Value::Ref(9, 0))
                .insert("DigestValue", Value::HexString("00".into()))
                .insert("DigestLength", Value::Integer(32)),
        )),
    ));
    let refs = doc
        .signature_timestamp_references(&signature)
        .expect("timestamp references");
    assert_eq!(refs.len(), 1, "无 TransformMethod 的条目按时间戳暴露");
    assert_eq!(refs[0].digest_length, Some(32));

    let backend = StubVerifyBackend {
        signature: SignatureCheck::Match,
        chain: ChainCheck::Trusted,
        timestamp: TimestampCheck::Valid,
    };
    let report = doc
        .verify_signature(
            &signature,
            &VerifyOptions::default(),
            &RustCryptoSignatureBackend,
            Some(&backend),
        )
        .expect("verify timestamp");
    assert_eq!(report.timestamps.len(), 1);
    assert_eq!(report.timestamps[0].token.as_deref(), Some(TOKEN));
    assert_eq!(report.timestamps[0].status, TimestampCheck::Valid);

    let report = doc
        .verify_signature(
            &signature,
            &VerifyOptions::default(),
            &RustCryptoSignatureBackend,
            None,
        )
        .expect("verify timestamp without backend");
    assert_eq!(report.timestamps[0].status, TimestampCheck::NotProvided);
}
