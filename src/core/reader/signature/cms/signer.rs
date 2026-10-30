//! `SignerInfo` / `signedAttrs` 解析（RFC 5652 §5.3）。

use super::der::{Cursor, ParseState, Tlv, decode_integer, decode_oid, decode_text};
use super::{
    CmsAlgorithmIdentifier, CmsDigestAlgorithm, CmsError, CmsSignerIdentifier, CmsSignerInfo,
    CmsSigningTime,
};

/// `messageDigest` 属性 OID（`id-messageDigest`）。
const OID_MESSAGE_DIGEST: &str = "1.2.840.113549.1.9.4";
/// `signingTime` 属性 OID（`id-signingTime`）。
const OID_SIGNING_TIME: &str = "1.2.840.113549.1.9.5";

#[derive(Default)]
struct SignedAttributes {
    message_digest: Option<Vec<u8>>,
    signing_time: Option<CmsSigningTime>,
}

/// 解析一个 `SignerInfo` SEQUENCE。
pub(super) fn parse_signer_info(
    tlv: &Tlv<'_>,
    state: &mut ParseState,
) -> Result<CmsSignerInfo, CmsError> {
    state.enter()?;
    let mut cursor = Cursor::new(tlv.content, tlv.content_offset());
    let version_tlv = cursor.read_tag(0x02, "SignerInfo version INTEGER", state)?;
    let version = decode_integer(version_tlv.content, version_tlv.content_offset())?;
    let signer_id = parse_signer_identifier(&mut cursor, state)?;
    let digest_tlv = cursor.read_tag(0x30, "digestAlgorithm SEQUENCE", state)?;
    let digest_identifier = parse_algorithm_identifier(&digest_tlv, state)?;
    let digest_algorithm = CmsDigestAlgorithm::from_oid(&digest_identifier.oid);

    let mut message_digest = None;
    let mut signing_time = None;
    let mut signed_attributes_der = None;
    if cursor.peek_tag() == Some(0xA0) {
        let attributes = cursor.read_tag(0xA0, "signedAttrs [0]", state)?;
        let parsed =
            parse_signed_attributes(attributes.content, attributes.content_offset(), state)?;
        message_digest = parsed.message_digest;
        signing_time = parsed.signing_time;
        // CMS 签名覆盖的是 SET OF 编码：把 IMPLICIT [0] 标签换回 0x31。
        let mut der = attributes.raw.to_vec();
        der[0] = 0x31;
        signed_attributes_der = Some(der);
    }

    let signature_algorithm_tlv = cursor.read_tag(0x30, "signatureAlgorithm SEQUENCE", state)?;
    let signature_algorithm = parse_algorithm_identifier(&signature_algorithm_tlv, state)?;
    let signature_tlv = cursor.read_tag(0x04, "signature OCTET STRING", state)?;
    if cursor.peek_tag() == Some(0xA1) {
        // unsignedAttrs [1]：时间戳等，按长度跳过。
        let _unsigned = cursor.read_tag(0xA1, "unsignedAttrs [1]", state)?;
    }
    cursor.finish()?;
    state.leave();
    Ok(CmsSignerInfo {
        version,
        signer_id,
        digest_algorithm,
        message_digest,
        signing_time,
        signature_algorithm,
        signature: signature_tlv.content.to_vec(),
        signed_attributes_der,
    })
}

fn parse_signer_identifier(
    cursor: &mut Cursor<'_>,
    state: &mut ParseState,
) -> Result<CmsSignerIdentifier, CmsError> {
    match cursor.peek_tag() {
        Some(0x30) => {
            let sid = cursor.read_tag(0x30, "issuerAndSerialNumber SEQUENCE", state)?;
            state.enter()?;
            let mut inner = Cursor::new(sid.content, sid.content_offset());
            let issuer = inner.read_tag(0x30, "issuer Name SEQUENCE", state)?;
            let serial = inner.read_tag(0x02, "serialNumber INTEGER", state)?;
            inner.finish()?;
            state.leave();
            Ok(CmsSignerIdentifier::IssuerAndSerialNumber {
                issuer_der: issuer.raw.to_vec(),
                serial: serial.content.to_vec(),
            })
        }
        Some(0x80) => {
            let ski = cursor.read_tag(0x80, "subjectKeyIdentifier [0]", state)?;
            Ok(CmsSignerIdentifier::SubjectKeyIdentifier(
                ski.content.to_vec(),
            ))
        }
        Some(tag) => Err(CmsError::UnexpectedTag {
            expected: "SignerInfo sid",
            tag,
            offset: cursor.next_offset(),
        }),
        None => Err(CmsError::Truncated {
            offset: cursor.next_offset(),
        }),
    }
}

/// 解析 `AlgorithmIdentifier` SEQUENCE { OID, parameters OPTIONAL }。
pub(super) fn parse_algorithm_identifier(
    tlv: &Tlv<'_>,
    state: &mut ParseState,
) -> Result<CmsAlgorithmIdentifier, CmsError> {
    state.enter()?;
    let mut cursor = Cursor::new(tlv.content, tlv.content_offset());
    let oid_tlv = cursor.read_tag(0x06, "algorithm OBJECT IDENTIFIER", state)?;
    let oid = decode_oid(oid_tlv.content, oid_tlv.content_offset())?;
    let parameters = if cursor.is_empty() {
        None
    } else {
        let parameter = cursor.read(state)?;
        Some(parameter.raw.to_vec())
    };
    cursor.finish()?;
    state.leave();
    Ok(CmsAlgorithmIdentifier { oid, parameters })
}

fn parse_signed_attributes(
    content: &[u8],
    base: usize,
    state: &mut ParseState,
) -> Result<SignedAttributes, CmsError> {
    state.enter()?;
    let mut cursor = Cursor::new(content, base);
    let mut out = SignedAttributes::default();
    while !cursor.is_empty() {
        let attribute = cursor.read_tag(0x30, "Attribute SEQUENCE", state)?;
        state.enter()?;
        let mut inner = Cursor::new(attribute.content, attribute.content_offset());
        let oid_tlv = inner.read_tag(0x06, "Attribute type OBJECT IDENTIFIER", state)?;
        let oid = decode_oid(oid_tlv.content, oid_tlv.content_offset())?;
        let values = inner.read_tag(0x31, "Attribute values SET", state)?;
        inner.finish()?;
        if oid == OID_MESSAGE_DIGEST && out.message_digest.is_none() {
            let mut value_cursor = Cursor::new(values.content, values.content_offset());
            let value = value_cursor.read_tag(0x04, "messageDigest OCTET STRING", state)?;
            out.message_digest = Some(value.content.to_vec());
        } else if oid == OID_SIGNING_TIME && out.signing_time.is_none() {
            let mut value_cursor = Cursor::new(values.content, values.content_offset());
            let value = value_cursor.read(state)?;
            let generalized = match value.tag {
                0x17 => false,
                0x18 => true,
                tag => {
                    return Err(CmsError::UnexpectedTag {
                        expected: "UTCTime or GeneralizedTime",
                        tag,
                        offset: value.offset,
                    });
                }
            };
            let text = decode_text(value.content, value.content_offset(), "signingTime")?;
            out.signing_time = Some(CmsSigningTime { generalized, text });
        }
        state.leave();
    }
    state.leave();
    Ok(out)
}
