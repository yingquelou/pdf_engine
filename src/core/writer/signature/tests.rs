//! 写端签名增量更新测试（12.8.1）：ByteRange 不动点收敛、Contents 等长
//! 回填、既有 AcroForm 复用与拒绝路径；产物一律经读端 `Document` 回读验证。

use crate::core::reader::Parser;
use crate::core::reader::signature::{
    DigestAlgorithm, DocmdpPermission, RustCryptoSignatureBackend, TransformMethod,
};
use crate::core::types::Document;
use crate::core::types::{Array, Dict, Object, PdfVersion, Trailer, Value};
use crate::core::writer::signature::SigningSpec;
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
        Some("adbe.pkcs7.detached")
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
