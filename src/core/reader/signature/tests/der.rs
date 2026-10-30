//! CMS 测试用 DER TLV 构造器（只覆盖测试需要的定长编码）。

/// 编码 DER 长度。
fn encode_length(len: usize, out: &mut Vec<u8>) {
    if len < 0x80 {
        out.push(len as u8);
    } else {
        let bytes = len.to_be_bytes();
        let start = bytes
            .iter()
            .position(|byte| *byte != 0)
            .unwrap_or(bytes.len() - 1);
        let count = bytes.len() - start;
        out.push(0x80 | count as u8);
        out.extend_from_slice(&bytes[start..]);
    }
}

/// 单个 TLV：标签 + 定长 + 内容。
pub(super) fn der(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    encode_length(content.len(), &mut out);
    out.extend_from_slice(content);
    out
}

/// 顺序拼接多个 TLV。
pub(super) fn concat(parts: &[Vec<u8>]) -> Vec<u8> {
    parts.iter().flat_map(|part| part.iter().copied()).collect()
}

pub(super) fn sequence(parts: &[Vec<u8>]) -> Vec<u8> {
    der(0x30, &concat(parts))
}

pub(super) fn set(parts: &[Vec<u8>]) -> Vec<u8> {
    der(0x31, &concat(parts))
}

/// 上下文标签 `[n]`（构造型）。
pub(super) fn context(number: u8, content: &[u8]) -> Vec<u8> {
    der(0xA0 + number, content)
}

fn encode_arc(mut value: u64, out: &mut Vec<u8>) {
    let mut buffer = [0u8; 10];
    let mut index = buffer.len() - 1;
    buffer[index] = (value & 0x7F) as u8;
    value >>= 7;
    while value > 0 {
        index -= 1;
        buffer[index] = (value & 0x7F) as u8 | 0x80;
        value >>= 7;
    }
    out.extend_from_slice(&buffer[index..]);
}

/// 点分 OID 的 DER 编码。
pub(super) fn oid(dotted: &str) -> Vec<u8> {
    let arcs: Vec<u64> = dotted
        .split('.')
        .map(|part| part.parse::<u64>().expect("OID arc"))
        .collect();
    assert!(arcs.len() >= 2, "OID needs at least two arcs");
    let mut content = Vec::new();
    encode_arc(arcs[0] * 40 + arcs[1], &mut content);
    for arc in &arcs[2..] {
        encode_arc(*arc, &mut content);
    }
    der(0x06, &content)
}

/// 最小补码编码的 INTEGER。
pub(super) fn integer(value: i64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let mut start = 0;
    while start + 1 < bytes.len()
        && ((bytes[start] == 0x00 && bytes[start + 1] & 0x80 == 0)
            || (bytes[start] == 0xFF && bytes[start + 1] & 0x80 != 0))
    {
        start += 1;
    }
    der(0x02, &bytes[start..])
}

pub(super) fn octet_string(bytes: &[u8]) -> Vec<u8> {
    der(0x04, bytes)
}

pub(super) fn utc_time(text: &str) -> Vec<u8> {
    der(0x17, text.as_bytes())
}

pub(super) fn generalized_time(text: &str) -> Vec<u8> {
    der(0x18, text.as_bytes())
}

pub(super) fn algorithm(oid_str: &str) -> Vec<u8> {
    sequence(&[oid(oid_str)])
}

/// `Attribute ::= SEQUENCE { attrType OID, attrValues SET OF AttributeValue }`。
pub(super) fn attribute(oid_str: &str, values: &[Vec<u8>]) -> Vec<u8> {
    sequence(&[oid(oid_str), set(values)])
}

pub(super) fn message_digest_attribute(digest: &[u8]) -> Vec<u8> {
    attribute(OID_MESSAGE_DIGEST, &[octet_string(digest)])
}

pub(super) fn signing_time_attribute(text: &str) -> Vec<u8> {
    attribute(OID_SIGNING_TIME, &[utc_time(text)])
}

/// `signedAttrs [0] IMPLICIT SET OF Attribute`（CMS 中的原始形态）。
pub(super) fn signed_attrs(attributes: &[Vec<u8>]) -> Vec<u8> {
    der(0xA0, &concat(attributes))
}

/// `IssuerAndSerialNumber ::= SEQUENCE { issuer Name, serialNumber INTEGER }`。
pub(super) fn issuer_and_serial_sid() -> Vec<u8> {
    let name = sequence(&[set(&[sequence(&[oid("2.5.4.3"), der(0x0C, b"Signer")])])]);
    sequence(&[name, integer(0x1F2E)])
}

/// `SignerInfo` 构造参数。
pub(super) struct SignerParts<'a> {
    pub digest_oid: &'a str,
    pub attributes: Option<Vec<u8>>,
    pub subject_key_id: Option<&'a [u8]>,
    pub signature: &'a [u8],
}

/// 构造一个 `SignerInfo`（version 1）。
pub(super) fn signer_info(parts: &SignerParts<'_>) -> Vec<u8> {
    let sid = match parts.subject_key_id {
        Some(ski) => der(0x80, ski),
        None => issuer_and_serial_sid(),
    };
    let mut fields = vec![integer(1), sid, algorithm(parts.digest_oid)];
    if let Some(attributes) = parts.attributes.clone() {
        fields.push(attributes);
    }
    fields.push(algorithm(OID_RSA));
    fields.push(octet_string(parts.signature));
    sequence(&fields)
}

/// `SignedData` 构造参数。
pub(super) struct SignedDataParts<'a> {
    pub version: i64,
    pub digest_oids: &'a [&'a str],
    pub encapsulated: Option<&'a [u8]>,
    pub certificates: &'a [Vec<u8>],
    pub crls: Option<Vec<u8>>,
    pub signers: &'a [Vec<u8>],
}

/// 构造 `SignedData` SEQUENCE。
pub(super) fn signed_data(parts: &SignedDataParts<'_>) -> Vec<u8> {
    let digests: Vec<Vec<u8>> = parts
        .digest_oids
        .iter()
        .map(|oid_str| algorithm(oid_str))
        .collect();
    let encap = match parts.encapsulated {
        Some(content) => sequence(&[oid(OID_DATA), context(0, &octet_string(content))]),
        None => sequence(&[oid(OID_DATA)]),
    };
    let mut fields = vec![integer(parts.version), set(&digests), encap];
    if !parts.certificates.is_empty() {
        fields.push(context(0, &concat(parts.certificates)));
    }
    if let Some(crls) = parts.crls.clone() {
        fields.push(context(1, &crls));
    }
    fields.push(set(parts.signers));
    sequence(&fields)
}

/// `ContentInfo ::= SEQUENCE { contentType signedData, [0] EXPLICIT SignedData }`。
pub(super) fn content_info(signed_data: &[u8]) -> Vec<u8> {
    sequence(&[oid(OID_SIGNED_DATA), context(0, signed_data)])
}

/// 未封装内容的 SignedData：一个 SHA-256 签署者。
pub(super) fn signed_data_with_signer(
    digest_oid: &str,
    attributes: Option<Vec<u8>>,
    subject_key_id: Option<&[u8]>,
    signature: &[u8],
) -> Vec<u8> {
    let signer = signer_info(&SignerParts {
        digest_oid,
        attributes,
        subject_key_id,
        signature,
    });
    content_info(&signed_data(&SignedDataParts {
        version: 1,
        digest_oids: &[digest_oid],
        encapsulated: None,
        certificates: &[],
        crls: None,
        signers: &[signer],
    }))
}

/// 带 signedAttrs（messageDigest + signingTime）的 CMS。
pub(super) fn cms_with_attributes(attributes: &[Vec<u8>], digest_oid: &str) -> Vec<u8> {
    let attrs = if attributes.is_empty() {
        None
    } else {
        Some(signed_attrs(attributes))
    };
    signed_data_with_signer(digest_oid, attrs, None, RAW_SIGNATURE)
}

/// 端到端测试用：messageDigest 等于给定摘要的 CMS（长度与摘要内容无关）。
pub(super) fn cms_with_message_digest(digest: &[u8]) -> Vec<u8> {
    cms_with_attributes(
        &[
            message_digest_attribute(digest),
            signing_time_attribute("261028000000Z"),
        ],
        OID_SHA256,
    )
}

pub(super) const OID_SIGNED_DATA: &str = "1.2.840.113549.1.7.2";
pub(super) const OID_DATA: &str = "1.2.840.113549.1.7.1";
pub(super) const OID_SHA1: &str = "1.3.14.3.2.26";
pub(super) const OID_SHA256: &str = "2.16.840.1.101.3.4.2.1";
pub(super) const OID_RSA: &str = "1.2.840.113549.1.1.1";
pub(super) const OID_MESSAGE_DIGEST: &str = "1.2.840.113549.1.9.4";
pub(super) const OID_SIGNING_TIME: &str = "1.2.840.113549.1.9.5";
pub(super) const OID_UNKNOWN_DIGEST: &str = "1.2.3.4.5.6";
pub(super) const RAW_SIGNATURE: &[u8] = &[0x5A; 8];
