//! `/Contents` 的 PKCS#7/CMS SignedData DER 解析测试。

use super::der::*;
use super::*;

#[test]
fn cms_parses_minimal_signed_data_with_issuer_and_serial() {
    let digest = [0x11u8; 32];
    let attributes = vec![
        message_digest_attribute(&digest),
        signing_time_attribute("261028000000Z"),
    ];
    let signer = signer_info(&SignerParts {
        digest_oid: OID_SHA256,
        attributes: Some(signed_attrs(&attributes)),
        subject_key_id: None,
        signature: &[0xA1, 0xB2, 0xC3],
    });
    let der = content_info(&signed_data(&SignedDataParts {
        version: 1,
        digest_oids: &[OID_SHA256, OID_SHA1],
        encapsulated: None,
        certificates: &[],
        crls: None,
        signers: &[signer],
    }));
    let cms = CmsSignedData::parse(&der).expect("minimal SignedData parses");
    assert_eq!(cms.version, 1);
    assert_eq!(cms.content_type, OID_DATA);
    assert_eq!(cms.content, None);
    assert_eq!(
        cms.digest_algorithms,
        vec![
            CmsDigestAlgorithm::Known(DigestAlgorithm::Sha256),
            CmsDigestAlgorithm::Known(DigestAlgorithm::Sha1),
        ]
    );
    assert!(cms.certificates.is_empty());
    assert_eq!(cms.signers.len(), 1);
    let signer = &cms.signers[0];
    assert_eq!(signer.version, 1);
    match &signer.signer_id {
        CmsSignerIdentifier::IssuerAndSerialNumber { issuer_der, serial } => {
            assert_eq!(serial, &vec![0x1F, 0x2E]);
            assert_eq!(issuer_der.first(), Some(&0x30), "issuer Name is a SEQUENCE");
        }
        other => panic!("expected issuerAndSerialNumber, got {other:?}"),
    }
    assert_eq!(
        signer.digest_algorithm,
        CmsDigestAlgorithm::Known(DigestAlgorithm::Sha256)
    );
    assert_eq!(signer.message_digest.as_deref(), Some(digest.as_slice()));
    let time = signer.signing_time.as_ref().expect("signingTime attribute");
    assert!(!time.generalized);
    assert_eq!(time.text, "261028000000Z");
    assert_eq!(signer.signature_algorithm.oid, OID_RSA);
    assert_eq!(signer.signature, vec![0xA1, 0xB2, 0xC3]);
    assert_eq!(
        signer.signed_attributes_der.as_deref(),
        Some(set(&attributes).as_slice()),
        "signedAttrs 重新打上 SET OF 标签供后续签名验证"
    );
}

#[test]
fn cms_parses_subject_key_identifier_signer() {
    let ski = [0xDE, 0xAD, 0xBE, 0xEF];
    let der = signed_data_with_signer(
        OID_SHA1,
        Some(signed_attrs(&[
            message_digest_attribute(&[0x22; 20]),
            attribute(OID_SIGNING_TIME, &[generalized_time("20261028000000Z")]),
        ])),
        Some(&ski),
        &[0x01, 0x02],
    );
    let cms = CmsSignedData::parse(&der).expect("SKI signer parses");
    assert_eq!(cms.signers.len(), 1);
    assert_eq!(
        cms.signers[0].signer_id,
        CmsSignerIdentifier::SubjectKeyIdentifier(ski.to_vec())
    );
    assert_eq!(
        cms.signers[0].digest_algorithm,
        CmsDigestAlgorithm::Known(DigestAlgorithm::Sha1)
    );
    assert_eq!(
        cms.signers[0].message_digest.as_deref(),
        Some(&[0x22; 20][..])
    );
    let time = cms.signers[0]
        .signing_time
        .as_ref()
        .expect("generalized signingTime");
    assert!(time.generalized);
    assert_eq!(time.text, "20261028000000Z");
}

#[test]
fn cms_parses_encapsulated_content_certificates_and_skips_crls() {
    let certificates = vec![sequence(&[integer(1)]), sequence(&[integer(2)])];
    let crls = sequence(&[sequence(&[integer(7)])]);
    let signer = signer_info(&SignerParts {
        digest_oid: OID_SHA256,
        attributes: None,
        subject_key_id: None,
        signature: &[9],
    });
    let der = content_info(&signed_data(&SignedDataParts {
        version: 3,
        digest_oids: &[OID_SHA256],
        encapsulated: Some(b"detached payload"),
        certificates: &certificates,
        crls: Some(crls),
        signers: &[signer],
    }));
    let cms = CmsSignedData::parse(&der).expect("SignedData with certs/crls parses");
    assert_eq!(cms.version, 3);
    assert_eq!(cms.content.as_deref(), Some(&b"detached payload"[..]));
    assert_eq!(cms.certificates, certificates);
    assert_eq!(cms.signers[0].message_digest, None);
    assert_eq!(cms.signers[0].signed_attributes_der, None);
}

#[test]
fn cms_unknown_digest_oid_is_explicit_other() {
    let der = cms_with_attributes(&[message_digest_attribute(&[0x33; 32])], OID_UNKNOWN_DIGEST);
    let cms = CmsSignedData::parse(&der).expect("unknown OID is not a parse error");
    let algorithm = &cms.signers[0].digest_algorithm;
    assert_eq!(algorithm.known(), None);
    assert_eq!(algorithm.oid(), OID_UNKNOWN_DIGEST);
    assert_eq!(
        algorithm,
        &CmsDigestAlgorithm::Other(OID_UNKNOWN_DIGEST.to_string())
    );
    assert_eq!(
        cms.digest_algorithms,
        vec![CmsDigestAlgorithm::Other(OID_UNKNOWN_DIGEST.to_string())]
    );
}

#[test]
fn cms_digest_oid_mapping_covers_sha384_and_sha512() {
    assert_eq!(
        CmsDigestAlgorithm::from_oid("2.16.840.1.101.3.4.2.2"),
        CmsDigestAlgorithm::Known(DigestAlgorithm::Sha384)
    );
    assert_eq!(
        CmsDigestAlgorithm::from_oid("2.16.840.1.101.3.4.2.3"),
        CmsDigestAlgorithm::Known(DigestAlgorithm::Sha512)
    );
    assert_eq!(
        CmsDigestAlgorithm::Known(DigestAlgorithm::Sha384).oid(),
        "2.16.840.1.101.3.4.2.2"
    );
    assert_eq!(DigestAlgorithm::Sha384.output_len(), 48);
    assert_eq!(DigestAlgorithm::Sha512.output_len(), 64);
}

#[test]
fn cms_malformed_der_is_typed_error() {
    // 外层不是 SEQUENCE。
    assert!(matches!(
        CmsSignedData::parse(&set(&[])),
        Err(CmsError::UnexpectedTag { .. })
    ));
    // 高位标签号不支持。
    assert!(matches!(
        CmsSignedData::parse(&[0x3F, 0x00]),
        Err(CmsError::UnexpectedTag { .. })
    ));
    // DER 不允许 indefinite length。
    assert!(matches!(
        CmsSignedData::parse(&[0x30, 0x80, 0x00, 0x00]),
        Err(CmsError::IndefiniteLength { .. })
    ));
    // 长度字段本身被截断。
    assert!(matches!(
        CmsSignedData::parse(&[0x30, 0x82, 0x01]),
        Err(CmsError::Truncated { .. })
    ));
    // 声明长度越过剩余字节。
    assert!(matches!(
        CmsSignedData::parse(&[0x30, 0x0A, 0x06, 0x03]),
        Err(CmsError::LengthExceedsRemaining { .. })
    ));
    // 长度字节数超过 8（u64 宽度）。
    assert!(matches!(
        CmsSignedData::parse(&[0x30, 0x89, 0, 0, 0, 0, 0, 0, 0, 0]),
        Err(CmsError::LengthOverflow { .. })
    ));
}

#[test]
fn cms_rejects_wrong_content_type_and_trailing_data() {
    // 内容类型不是 signedData。
    let wrong = sequence(&[oid(OID_DATA), context(0, &sequence(&[]))]);
    assert!(matches!(
        CmsSignedData::parse(&wrong),
        Err(CmsError::NotSignedData(oid)) if oid == OID_DATA
    ));
    // PDF /Contents 的尾部零填充可接受，非零尾随数据报错。
    let mut padded = cms_with_message_digest(&[0x44; 32]);
    padded.extend_from_slice(&[0, 0, 0]);
    assert!(CmsSignedData::parse(&padded).is_ok(), "zero padding");
    padded.push(1);
    assert!(matches!(
        CmsSignedData::parse(&padded),
        Err(CmsError::TrailingData { .. })
    ));
}

#[test]
fn cms_without_signers_is_typed_error() {
    let der = content_info(&signed_data(&SignedDataParts {
        version: 1,
        digest_oids: &[OID_SHA256],
        encapsulated: None,
        certificates: &[],
        crls: None,
        signers: &[],
    }));
    assert!(matches!(
        CmsSignedData::parse(&der),
        Err(CmsError::NoSigners)
    ));
}

#[test]
fn cms_limits_enforced_for_size_elements_and_depth() {
    let der = cms_with_message_digest(&[0x44; 32]);
    assert!(matches!(
        CmsSignedData::parse_with_limits(
            &der,
            CmsLimits {
                max_der_bytes: 8,
                ..CmsLimits::default()
            }
        ),
        Err(CmsError::DerTooLarge { .. })
    ));
    assert!(matches!(
        CmsSignedData::parse_with_limits(
            &der,
            CmsLimits {
                max_elements: 3,
                ..CmsLimits::default()
            }
        ),
        Err(CmsError::ElementLimit { limit: 3 })
    ));
    assert!(matches!(
        CmsSignedData::parse_with_limits(
            &der,
            CmsLimits {
                max_depth: 2,
                ..CmsLimits::default()
            }
        ),
        Err(CmsError::DepthLimit { limit: 2 })
    ));
    assert!(
        CmsSignedData::parse(&der).is_ok(),
        "默认上限下同一输入可解析"
    );
}
