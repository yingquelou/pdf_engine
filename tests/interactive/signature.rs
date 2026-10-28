//! F1 数字签名：签名增量写出的 ByteRange 覆盖与篡改检测。

use super::*;

/// 写端签名增量更新（F1 / 12.8.1）：builder 单页文档 → 解析 →
/// `sign_incremental`（恒等签名器，Contents = 覆盖段 SHA-256 摘要）→
/// 回读验证 ByteRange/摘要一致、页面文本仍可读；篡改覆盖字节后
/// 摘要不再匹配。
#[test]
fn builder_signature_incremental_protects_and_detects_tampering() {
    use pdf_engine::core::builder::{Font, PageSize, PdfBuilder};
    use pdf_engine::core::reader::signature::{DigestAlgorithm, RustCryptoSignatureBackend};
    use pdf_engine::core::writer::{DocumentWriter, SigningSpec};

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    builder.text(p0, &Font::helvetica(24.0), 72.0, 700.0, "Contract");
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let spec = SigningSpec::new(DigestAlgorithm::Sha256, 32)
        .with_field_name("Approval")
        .with_name("Acme");
    let signed = DocumentWriter::new()
        .sign_incremental(&doc, &spec, &RustCryptoSignatureBackend)
        .expect("sign_incremental");
    assert!(
        signed.bytes.starts_with(&buf),
        "签名增量输出应保留原文件字节"
    );

    let revised = parse_buf(&signed.bytes);
    assert_eq!(revised.extract_page_text(0).as_deref(), Some("Contract"));
    let sigs = revised.document_signatures().expect("signatures");
    assert_eq!(sigs.len(), 1);
    let sig = &sigs[0];
    assert_eq!(sig.name.as_deref(), Some("Acme"));
    revised
        .validate_signature_byte_range(sig)
        .expect("byte range covers the signed file");
    let Value::HexString(hex) = sig.contents.as_ref().expect("contents") else {
        panic!("contents shape");
    };
    let stored: Vec<u8> = (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
        .collect();
    let digest = revised
        .signature_digest(sig, DigestAlgorithm::Sha256, &RustCryptoSignatureBackend)
        .expect("digest");
    assert_eq!(stored, digest, "Contents 应等于覆盖段摘要（恒等签名器）");

    // 篡改覆盖段内的文本字节（等长替换），重算摘要应不再匹配。
    let at = signed
        .bytes
        .windows(b"Contract".len())
        .position(|w| w == b"Contract")
        .expect("literal in content stream");
    assert!(
        (at as i64) < sig.byte_range[1],
        "被篡改字节位于第一段覆盖区内"
    );
    let mut tampered = signed.bytes.clone();
    tampered[at + 3] = b'X';
    let tampered_doc = parse_buf(&tampered);
    let tampered_sigs = tampered_doc.document_signatures().expect("tampered sigs");
    let recomputed = tampered_doc
        .signature_digest(
            &tampered_sigs[0],
            DigestAlgorithm::Sha256,
            &RustCryptoSignatureBackend,
        )
        .expect("tampered digest");
    assert_ne!(stored, recomputed, "篡改覆盖字节后摘要必须改变");
}
