//! 写端签名增量更新测试（12.8.1）：ByteRange 不动点收敛、Contents 等长
//! 回填、既有 AcroForm 复用与拒绝路径；产物一律经读端 `Document` 回读验证。

use crate::core::reader::Parser;
use crate::core::reader::signature::{
    CmsDigestAlgorithm, CmsSignerIdentifier, DigestAlgorithm, DocmdpPermission,
    RustCryptoSignatureBackend, SignatureSubFilter, SignerIntegrity, TransformMethod,
};
use crate::core::types::Document;
use crate::core::types::{Array, Dict, Object, PdfVersion, Trailer, Value};
use crate::core::writer::signature::{
    CmsSignBackend, CmsSignerId, CmsSignerMaterial, ContentsPayload, SigningSpec,
};
use crate::core::writer::{DocumentWriter, WriteError};
use std::io::Write as _;

/// 与 `incremental/tests.rs` 相同策略：临时文件落在 target/test-tmp/ 下。
fn temp_path(tag: &str) -> std::path::PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!(
        "demo-sign-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ))
}

fn parse_bytes(bytes: &[u8], tag: &str) -> Document {
    let path = temp_path(tag);
    {
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(bytes).unwrap();
        f.flush().unwrap();
    }
    let file = std::fs::File::open(&path).unwrap();
    let mut p = Parser::new(file).unwrap();
    p.parse_document().unwrap()
}

/// 基线文档：Catalog（1）+ 载荷字典（2），trailer 只有 /Root。
fn base_bytes(version: Option<PdfVersion>) -> Vec<u8> {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0))
                .insert("Note", Value::Integer(1)),
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
    let mut writer = DocumentWriter::new();
    if let Some(v) = version {
        writer = writer.with_version(v);
    }
    writer.build(&doc, &mut buf).unwrap();
    buf
}

/// 带既有 AcroForm（1 个文本域）的基线：/AcroForm=3、/Fields=4、域=5。
fn acroform_base_bytes() -> Vec<u8> {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0))
                .insert("AcroForm", Value::Ref(3, 0)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(Dict::new().insert("Payload", Value::Integer(42))),
    ));
    doc.set_object(Object::new(
        3,
        0,
        Value::Dict(Dict::new().insert("Fields", Value::Ref(4, 0))),
    ));
    doc.set_object(Object::new(
        4,
        0,
        Value::Array(Array::new().push(Value::Ref(5, 0))),
    ));
    doc.set_object(Object::new(
        5,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Annot".into()))
                .insert("Subtype", Value::Name("Widget".into()))
                .insert("FT", Value::Name("Tx".into()))
                .insert("T", Value::String("Note".into()))
                .insert(
                    "Rect",
                    Value::Array(
                        Array::new()
                            .push(Value::Integer(0))
                            .push(Value::Integer(0))
                            .push(Value::Integer(100))
                            .push(Value::Integer(20)),
                    ),
                ),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    let mut buf = Vec::new();
    DocumentWriter::new().build(&doc, &mut buf).unwrap();
    buf
}

fn hex_decode(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
        .collect()
}

#[test]
fn signed_incremental_field_roundtrips_and_digest_matches_contents() {
    let base = base_bytes(None);
    let doc = parse_bytes(&base, "roundtrip");
    let spec = SigningSpec::new(DigestAlgorithm::Sha256, 32)
        .with_field_name("Sig1")
        .with_name("Qoder")
        .with_location("Hangzhou")
        .with_reason("approval");
    let signed = DocumentWriter::new()
        .sign_incremental(&doc, &spec, &RustCryptoSignatureBackend)
        .expect("sign_incremental should succeed");
    assert!(
        signed.bytes.starts_with(&base),
        "输出应以源文件字节为前缀（增量更新）"
    );
    let placeholder = format!("<{}>", "0".repeat(64));
    assert!(
        !std::str::from_utf8(&signed.bytes)
            .map(|s| s.contains(&placeholder))
            .unwrap_or(false),
        "占位串必须被签名数据回填"
    );

    let revised = parse_bytes(&signed.bytes, "roundtrip-read");
    assert_eq!(revised.signature_flags().expect("flags"), Some(1));
    let sigs = revised.document_signatures().expect("signatures");
    assert_eq!(sigs.len(), 1, "只发现本次写入的 Sig 字段");
    let sig = &sigs[0];
    assert_eq!(sig.name.as_deref(), Some("Qoder"));
    assert_eq!(sig.location.as_deref(), Some("Hangzhou"));
    assert_eq!(sig.reason.as_deref(), Some("approval"));
    assert_eq!(sig.byte_range, signed.byte_range);
    revised
        .validate_signature_byte_range(sig)
        .expect("byte range should validate against the signed file");
    let digest = revised
        .signature_digest(sig, DigestAlgorithm::Sha256, &RustCryptoSignatureBackend)
        .expect("digest");
    let Value::HexString(hex) = sig.contents.as_ref().expect("contents") else {
        panic!("contents shape");
    };
    assert_eq!(
        hex_decode(hex),
        digest,
        "默认签名器为恒等映射：Contents 应等于覆盖段的 SHA-256 摘要"
    );

    let field = revised
        .get_object(signed.field_object)
        .unwrap()
        .expect("sig field object");
    assert_eq!(
        field
            .value
            .as_dict()
            .unwrap()
            .get("FT")
            .and_then(Value::as_name),
        Some("Sig")
    );
    assert_eq!(
        field.value.as_dict().unwrap().get("T"),
        Some(&Value::String("Sig1".into()))
    );
    let sig_obj = revised
        .get_object(signed.signature_object)
        .unwrap()
        .expect("signature dict");
    assert_eq!(
        sig_obj
            .value
            .as_dict()
            .unwrap()
            .get("SubFilter")
            .and_then(Value::as_name),
        None,
        "F1.1：裸摘要默认载荷不得声明 CMS 类 /SubFilter"
    );
    // 历史载荷对象沿 /Prev 链仍可读。
    let payload = revised.get_object(2).unwrap().expect("history payload");
    assert_eq!(
        payload.value.as_dict().unwrap().get("Payload"),
        Some(&Value::Integer(42))
    );
}

#[test]
fn signing_with_docmdp_writes_transform_reference() {
    let doc = parse_bytes(&base_bytes(None), "mdp");
    let spec = SigningSpec::new(DigestAlgorithm::Sha1, 20).with_docmdp(2);
    let signed = DocumentWriter::new()
        .sign_incremental(&doc, &spec, &RustCryptoSignatureBackend)
        .expect("sign with docmdp");
    let revised = parse_bytes(&signed.bytes, "mdp-read");
    let sigs = revised.document_signatures().expect("signatures");
    assert_eq!(sigs.len(), 1);
    let refs = revised.signature_references(&sigs[0]).expect("references");
    assert_eq!(refs.len(), 1, "写入了一条 DocMDP 变换引用");
    assert_eq!(refs[0].transform_method, Some(TransformMethod::Docmdp));
    assert_eq!(refs[0].data_reference(), Some((signed.signature_object, 0)));
    assert_eq!(
        revised.doc_modification_permission(&sigs[0]).expect("mdp"),
        Some(DocmdpPermission::FillAndAnnotate)
    );
    // 恒等签名器 + Sha1（20 字节 → 40 个十六进制字符）。
    let Value::HexString(hex) = sigs[0].contents.as_ref().expect("contents") else {
        panic!("contents shape");
    };
    assert_eq!(hex.len(), 40);
    let digest = revised
        .signature_digest(&sigs[0], DigestAlgorithm::Sha1, &RustCryptoSignatureBackend)
        .expect("sha1 digest");
    assert_eq!(hex_decode(hex), digest);
}

#[test]
fn signing_extends_existing_acroform_fields_array() {
    let base = acroform_base_bytes();
    let doc = parse_bytes(&base, "form");
    let spec = SigningSpec::new(DigestAlgorithm::Sha256, 32);
    let signed = DocumentWriter::new()
        .sign_incremental(&doc, &spec, &RustCryptoSignatureBackend)
        .expect("sign document with existing acroform");
    assert!(
        signed.field_object > 5,
        "新字段号应避开既有对象号，实际 {}",
        signed.field_object
    );
    let revised = parse_bytes(&signed.bytes, "form-read");
    // /AcroForm 与 /Fields 对象号复用；数组追加签名域引用。
    let form = revised.get_object(3).unwrap().expect("acroform");
    assert_eq!(
        form.value.as_dict().unwrap().get("Fields"),
        Some(&Value::Ref(4, 0))
    );
    let fields = revised.get_object(4).unwrap().expect("fields array");
    let array = fields.value.as_array().expect("/Fields array");
    assert_eq!(array.len(), 2, "既有文本域 + 新签名域");
    assert_eq!(array.0.first(), Some(&Value::Ref(5, 0)));
    assert_eq!(array.0.get(1), Some(&Value::Ref(signed.field_object, 0)));
    // 发现：文本域被跳过，只剩签名字段。
    let sigs = revised.document_signatures().expect("signatures");
    assert_eq!(sigs.len(), 1);
    assert_eq!(revised.signature_flags().expect("flags"), Some(1));
}

#[test]
fn signing_rejects_memory_document_and_pre_1_3_source() {
    let doc = Document::new();
    let error = DocumentWriter::new()
        .sign_incremental(
            &doc,
            &SigningSpec::new(DigestAlgorithm::Sha256, 32),
            &RustCryptoSignatureBackend,
        )
        .expect_err("in-memory document has no source bytes");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error
            .to_string()
            .contains("requires a document parsed from a file"),
        "{error}"
    );

    let old = parse_bytes(&base_bytes(Some(PdfVersion::V1_2)), "v12");
    let error = DocumentWriter::new()
        .sign_incremental(
            &old,
            &SigningSpec::new(DigestAlgorithm::Sha256, 32),
            &RustCryptoSignatureBackend,
        )
        .expect_err("signatures need PDF 1.3");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(error.to_string().contains("PDF 1.3"), "{error}");
}

#[test]
fn signing_rejects_encrypted_trailer() {
    let mut doc = Document::new();
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Encrypt", Value::Ref(9, 0)),
    };
    let error = DocumentWriter::new()
        .sign_incremental(
            &doc,
            &SigningSpec::new(DigestAlgorithm::Sha256, 32),
            &RustCryptoSignatureBackend,
        )
        .expect_err("trailer /Encrypt must be rejected");
    assert!(matches!(error, WriteError::EncryptedDocument), "{error}");
}

#[test]
fn signing_rejects_bad_signer_output_and_field_names() {
    let doc = parse_bytes(&base_bytes(None), "reject");
    let backend = RustCryptoSignatureBackend;

    let spec = SigningSpec::new(DigestAlgorithm::Sha256, 32).with_signer(32, |_| vec![9u8; 20]);
    let error = DocumentWriter::new()
        .sign_incremental(&doc, &spec, &backend)
        .expect_err("signer output length must match the placeholder");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("signer returned 20 bytes"),
        "{error}"
    );

    for name in ["a.b", ""] {
        let spec = SigningSpec::new(DigestAlgorithm::Sha256, 32).with_field_name(name);
        let error = DocumentWriter::new()
            .sign_incremental(&doc, &spec, &backend)
            .expect_err("field name with a dot (or empty) is invalid");
        assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
        assert!(
            error.to_string().contains("invalid signature field name"),
            "{error}"
        );
    }
}

#[test]
fn signed_output_survives_full_rewrite() {
    // 签名增量产物可再被全量重写（结构性对象被正确剔除）。
    let doc = parse_bytes(&base_bytes(None), "rewrite");
    let spec = SigningSpec::new(DigestAlgorithm::Sha256, 32).with_docmdp(1);
    let signed = DocumentWriter::new()
        .sign_incremental(&doc, &spec, &RustCryptoSignatureBackend)
        .expect("sign");
    let revised = parse_bytes(&signed.bytes, "rewrite-read");
    revised.load_all_objects().expect("load all");
    let mut rewritten = Vec::new();
    DocumentWriter::new()
        .build(&revised, &mut rewritten)
        .expect("full rewrite of signed incremental document");
    let again = parse_bytes(&rewritten, "rewrite-again");
    let sigs = again
        .document_signatures()
        .expect("signatures after rewrite");
    assert_eq!(sigs.len(), 1);
    assert_eq!(
        again.doc_modification_permission(&sigs[0]).expect("mdp"),
        Some(DocmdpPermission::NoChanges)
    );
    match &sigs[0]
        .references
        .as_ref()
        .and_then(|value| value.as_array())
        .and_then(|array| array.0.first())
    {
        Some(Value::Ref(num, _)) => {
            let reference = again.get_object(*num).unwrap().expect("sig reference");
            assert_eq!(
                reference
                    .value
                    .as_dict()
                    .unwrap()
                    .get("TransformMethod")
                    .and_then(Value::as_name),
                Some("DocMDP")
            );
        }
        _ => panic!("reference entry should be an indirect dict"),
    }
}

// ===== F1.1：/SubFilter 与 /Contents 载荷自洽 =====

/// 测试用最小 DER 编码器（只覆盖定长 TLV）。
fn cms_der(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    if content.len() < 0x80 {
        out.push(content.len() as u8);
    } else {
        let bytes = content.len().to_be_bytes();
        let start = bytes
            .iter()
            .position(|b| *b != 0)
            .unwrap_or(bytes.len() - 1);
        out.push(0x80 | (bytes.len() - start) as u8);
        out.extend_from_slice(&bytes[start..]);
    }
    out.extend_from_slice(content);
    out
}

fn cms_seq(parts: &[Vec<u8>]) -> Vec<u8> {
    cms_der(0x30, &parts.iter().flatten().copied().collect::<Vec<_>>())
}

fn cms_set(parts: &[Vec<u8>]) -> Vec<u8> {
    cms_der(0x31, &parts.iter().flatten().copied().collect::<Vec<_>>())
}

fn cms_oid(dotted: &str) -> Vec<u8> {
    let arcs: Vec<u64> = dotted.split('.').map(|s| s.parse().unwrap()).collect();
    let mut content = Vec::new();
    let mut arcs = arcs.into_iter();
    let first = arcs.next().unwrap() * 40 + arcs.next().unwrap();
    for value in std::iter::once(first).chain(arcs) {
        let mut buf = [0u8; 10];
        let mut i = buf.len() - 1;
        let mut v = value;
        buf[i] = (v & 0x7F) as u8;
        v >>= 7;
        while v > 0 {
            i -= 1;
            buf[i] = (v & 0x7F) as u8 | 0x80;
            v >>= 7;
        }
        content.extend_from_slice(&buf[i..]);
    }
    cms_der(0x06, &content)
}

fn cms_integer(n: u8) -> Vec<u8> {
    cms_der(0x02, &[n])
}

fn cms_octet(bytes: &[u8]) -> Vec<u8> {
    cms_der(0x04, bytes)
}

fn cms_algorithm(oid: &str) -> Vec<u8> {
    cms_seq(&[cms_oid(oid)])
}

/// 一个结构合法（无签名值验证）的 detached CMS SignedData。
fn minimal_cms_detached() -> Vec<u8> {
    let sid = cms_seq(&[cms_seq(&[]), cms_integer(1)]);
    let signer = cms_seq(&[
        cms_integer(1),
        sid,
        cms_algorithm("2.16.840.1.101.3.4.2.1"), // sha256
        cms_algorithm("1.2.840.113549.1.1.1"),   // rsaEncryption
        cms_octet(&[0x5A; 8]),
    ]);
    let signed = cms_seq(&[
        cms_integer(1),
        cms_set(&[cms_algorithm("2.16.840.1.101.3.4.2.1")]),
        cms_seq(&[cms_oid("1.2.840.113549.1.7.1")]), // eContentType = data, 无 eContent
        cms_set(&[signer]),
    ]);
    cms_seq(&[cms_oid("1.2.840.113549.1.7.2"), cms_der(0xA0, &signed)])
}

#[test]
fn signing_rejects_cms_subfilter_with_raw_payload() {
    // 当前默认载荷即 Raw（恒等签名器 = 裸摘要），不得声明 CMS 子过滤器。
    let doc = parse_bytes(&base_bytes(None), "cms-raw");
    let spec = SigningSpec::new(DigestAlgorithm::Sha256, 32).with_sub_filter("adbe.pkcs7.detached");
    let error = DocumentWriter::new()
        .sign_incremental(&doc, &spec, &RustCryptoSignatureBackend)
        .expect_err("CMS SubFilter with bare-digest payload must be rejected");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error
            .to_string()
            .contains("incompatible with /Contents payload"),
        "{error}"
    );
}

#[test]
fn signing_rejects_non_cms_subfilter_with_cms_payload() {
    let doc = parse_bytes(&base_bytes(None), "raw-cms");
    let spec = SigningSpec::new(DigestAlgorithm::Sha256, 32)
        .with_contents_payload(ContentsPayload::CmsDetached)
        .with_sub_filter("adbe.x509.rsa_sha1");
    let error = DocumentWriter::new()
        .sign_incremental(&doc, &spec, &RustCryptoSignatureBackend)
        .expect_err("non-CMS SubFilter with CMS payload must be rejected");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
}

#[test]
fn signing_rejects_cms_payload_that_is_not_der() {
    // 声明 CMS 载荷但签名器返回裸摘要：写出前必须被结构校验拦下。
    let doc = parse_bytes(&base_bytes(None), "cms-not-der");
    let spec = SigningSpec::new(DigestAlgorithm::Sha256, 32).with_cms_payload();
    let error = DocumentWriter::new()
        .sign_incremental(&doc, &spec, &RustCryptoSignatureBackend)
        .expect_err("bare digest must not masquerade as CMS SignedData");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("not valid CMS SignedData"),
        "{error}"
    );
}

#[test]
fn signing_accepts_cms_subfilter_with_valid_cms_payload_and_roundtrips() {
    let blob = minimal_cms_detached();
    let len = blob.len();
    let doc = parse_bytes(&base_bytes(None), "cms-ok");
    let spec = SigningSpec::new(DigestAlgorithm::Sha256, len)
        .with_cms_payload()
        .with_signer(len, move |_| blob.clone());
    let signed = DocumentWriter::new()
        .sign_incremental(&doc, &spec, &RustCryptoSignatureBackend)
        .expect("valid CMS payload should be accepted");
    let revised = parse_bytes(&signed.bytes, "cms-ok-read");
    let sigs = revised.document_signatures().expect("signatures");
    assert_eq!(sigs.len(), 1);
    assert_eq!(sigs[0].sub_filter, Some(SignatureSubFilter::Pkcs7Detached));
    revised
        .validate_signature_byte_range(&sigs[0])
        .expect("byte range validates");
    let cms = revised
        .signature_cms(&sigs[0])
        .expect("parse cms")
        .expect("cms present");
    assert_eq!(cms.signers.len(), 1, "CMS SignedData 可经读端解析");
    assert_eq!(cms.signers[0].signature, vec![0x5A; 8]);
    assert_eq!(cms.content, None, "detached：eContent 缺省");
}

// ===== F1.2：写端 detached CMS 构造与签名值后端 =====

const FAKE_CERT: &[u8] = &[0x30, 0x03, 0x02, 0x01, 0x07];

/// 结构测试用后端：签名值 = 摘要长度 + 后端收到的 signedAttrs DER，
/// 以此证明「后端签的字节」与「读端重建供验证的字节」完全一致。
#[derive(Debug, Clone, Copy)]
struct FakeCmsSignBackend;

impl CmsSignBackend for FakeCmsSignBackend {
    fn sign(
        &self,
        signed_attrs_der: &[u8],
        digest_algorithm: DigestAlgorithm,
    ) -> Result<CmsSignerMaterial, String> {
        let mut signature = vec![digest_algorithm.output_len() as u8];
        signature.extend_from_slice(signed_attrs_der);
        Ok(CmsSignerMaterial {
            signature_algorithm: "1.2.840.113549.1.1.1".into(),
            signer_id: CmsSignerId::IssuerAndSerial {
                issuer_der: vec![0x30, 0x00],
                serial: vec![0x2A],
            },
            signature,
            certificates: vec![FAKE_CERT.to_vec()],
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct FailingCmsSignBackend;

impl CmsSignBackend for FailingCmsSignBackend {
    fn sign(
        &self,
        _signed_attrs_der: &[u8],
        _digest_algorithm: DigestAlgorithm,
    ) -> Result<CmsSignerMaterial, String> {
        Err("no key".into())
    }
}

#[derive(Debug, Clone, Copy)]
struct SkiCmsSignBackend;

impl CmsSignBackend for SkiCmsSignBackend {
    fn sign(
        &self,
        signed_attrs_der: &[u8],
        _digest_algorithm: DigestAlgorithm,
    ) -> Result<CmsSignerMaterial, String> {
        Ok(CmsSignerMaterial {
            signature_algorithm: "1.2.840.113549.1.1.1".into(),
            signer_id: CmsSignerId::SubjectKeyIdentifier(vec![0xDE, 0xAD]),
            signature: signed_attrs_der.to_vec(),
            certificates: vec![],
        })
    }
}

#[test]
fn cms_backend_builds_detached_signed_data_and_roundtrips() {
    let doc = parse_bytes(&base_bytes(None), "cms-build");
    let spec = SigningSpec::new(DigestAlgorithm::Sha256, 512).with_cms_backend(FakeCmsSignBackend);
    let signed = DocumentWriter::new()
        .sign_incremental(&doc, &spec, &RustCryptoSignatureBackend)
        .expect("sign with CMS backend");
    let revised = parse_bytes(&signed.bytes, "cms-build-read");
    let sigs = revised.document_signatures().expect("signatures");
    assert_eq!(sigs.len(), 1);
    assert_eq!(sigs[0].sub_filter, Some(SignatureSubFilter::Pkcs7Detached));
    revised
        .validate_signature_byte_range(&sigs[0])
        .expect("byte range");

    let digest = revised
        .signature_digest(
            &sigs[0],
            DigestAlgorithm::Sha256,
            &RustCryptoSignatureBackend,
        )
        .expect("digest");
    let cms = revised
        .signature_cms(&sigs[0])
        .expect("parse cms")
        .expect("cms present");
    assert_eq!(
        cms.digest_algorithms,
        vec![CmsDigestAlgorithm::Known(DigestAlgorithm::Sha256)]
    );
    assert_eq!(cms.content, None, "detached：eContent 缺省");
    assert_eq!(cms.certificates, vec![FAKE_CERT.to_vec()]);
    assert_eq!(cms.signers.len(), 1);
    let signer = &cms.signers[0];
    assert_eq!(signer.message_digest.as_deref(), Some(digest.as_slice()));
    assert_eq!(signer.signature_algorithm.oid, "1.2.840.113549.1.1.1");
    match &signer.signer_id {
        CmsSignerIdentifier::IssuerAndSerialNumber { issuer_der, serial } => {
            assert_eq!(issuer_der, &vec![0x30, 0x00]);
            assert_eq!(serial, &vec![0x2A]);
        }
        other => panic!("unexpected signer id {other:?}"),
    }
    // 后端收到的待签 DER 与读端为验证重建的 signedAttrs 逐字节一致。
    assert_eq!(signer.signature[0], 32);
    assert_eq!(
        &signer.signature[1..],
        signer
            .signed_attributes_der
            .as_deref()
            .expect("signedAttrs present")
    );
    let integrity = revised
        .verify_signature_integrity(&sigs[0], &RustCryptoSignatureBackend)
        .expect("integrity");
    assert_eq!(integrity.signers, vec![SignerIntegrity::Match]);

    // /Contents 槽位尾部补零，读端按 PDF 惯例接受。
    let Value::HexString(hex) = sigs[0].contents.as_ref().expect("contents") else {
        panic!("contents shape");
    };
    let contents = hex_decode(hex);
    assert_eq!(contents.len(), 512);
    assert!(
        contents[400..].iter().all(|byte| *byte == 0),
        "未使用的 /Contents 槽位应补零"
    );
}

#[test]
fn cms_backend_rejects_oversized_payload() {
    let doc = parse_bytes(&base_bytes(None), "cms-oversize");
    let spec = SigningSpec::new(DigestAlgorithm::Sha256, 8).with_cms_backend(FakeCmsSignBackend);
    let error = DocumentWriter::new()
        .sign_incremental(&doc, &spec, &RustCryptoSignatureBackend)
        .expect_err("CMS larger than the /Contents slot must be rejected");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error
            .to_string()
            .contains("exceeding the 8-byte /Contents slot"),
        "{error}"
    );
}

#[test]
fn cms_backend_error_is_typed() {
    let doc = parse_bytes(&base_bytes(None), "cms-err");
    let spec =
        SigningSpec::new(DigestAlgorithm::Sha256, 256).with_cms_backend(FailingCmsSignBackend);
    let error = DocumentWriter::new()
        .sign_incremental(&doc, &spec, &RustCryptoSignatureBackend)
        .expect_err("backend error must abort the write");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error
            .to_string()
            .contains("CMS signing backend failed: no key"),
        "{error}"
    );
}

#[test]
fn cms_backend_subject_key_identifier_uses_version_three() {
    let doc = parse_bytes(&base_bytes(None), "cms-ski");
    let spec = SigningSpec::new(DigestAlgorithm::Sha256, 256).with_cms_backend(SkiCmsSignBackend);
    let signed = DocumentWriter::new()
        .sign_incremental(&doc, &spec, &RustCryptoSignatureBackend)
        .expect("sign with SKI backend");
    let revised = parse_bytes(&signed.bytes, "cms-ski-read");
    let sigs = revised.document_signatures().expect("signatures");
    let cms = revised
        .signature_cms(&sigs[0])
        .expect("parse cms")
        .expect("cms present");
    assert_eq!(cms.version, 3);
    assert_eq!(cms.signers[0].version, 3);
    assert_eq!(
        cms.signers[0].signer_id,
        CmsSignerIdentifier::SubjectKeyIdentifier(vec![0xDE, 0xAD])
    );
}
