//! 数字签名读取（12.8）单元测试。

use super::*;
use crate::core::types::{Object, PdfVersion, StartXref, Trailer, Xref};
use crate::core::writer::DocumentWriter;

fn temp_path(tag: &str) -> std::path::PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!(
        "demo-sig-{}-{}-{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ))
}

fn parse_bytes(bytes: &[u8], tag: &str) -> Document {
    use std::io::Write as _;
    let path = temp_path(tag);
    {
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(bytes).unwrap();
        f.flush().unwrap();
    }
    let file = std::fs::File::open(&path).unwrap();
    let mut p = crate::core::reader::Parser::new(file).unwrap();
    p.parse_document().unwrap()
}

const PLACEHOLDER: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// 构造一个「已按 ByteRange 占位并回填」的最小签名 PDF：
/// 目录挂 `/SigFlags 3` 与 `/AcroForm`，Sig 字段 `/V` 指向签名字典，
/// `/Contents` 为 64 字节十六进制占位。ByteRange 数字的宽度会移动
/// Contents 的偏移，故迭代重算直到布局不动点收敛。
fn signed_file() -> Vec<u8> {
    let contents = format!("<{PLACEHOLDER}>");
    let mut range = [0i64; 4];
    let mut out = Vec::new();
    let mut stable = false;
    for _ in 0..8 {
        let doc = build_sig_document(PLACEHOLDER, range);
        let mut buf = Vec::new();
        DocumentWriter::new()
            .with_version(PdfVersion::V1_4)
            .build(&doc, &mut buf)
            .expect("base build");
        let at = find_occurrence(&buf, contents.as_bytes()).expect("placeholder present");
        assert!(
            find_occurrence(&buf[at + 1..], contents.as_bytes()).is_none(),
            "placeholder must be unique"
        );
        let after = (at + contents.len()) as i64;
        let next = [0, at as i64, after, buf.len() as i64 - after];
        out = buf;
        if next == range {
            stable = true;
            break;
        }
        range = next;
    }
    assert!(stable, "byte range fixed point did not converge");
    out
}

fn find_occurrence(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// 手写一个四对象的签名文档（1.4 经典写出为明文顶层对象）。
/// `/ByteRange` 以固定四整数写入；`contents_hex` 为 `/Contents` 的
/// 十六进制串原始字符（不含 `<>`）。
fn build_sig_document(contents_hex: &str, byte_range: [i64; 4]) -> Document {
    let range_value = Value::Array(
        crate::core::types::Array::new()
            .push(Value::Integer(byte_range[0]))
            .push(Value::Integer(byte_range[1]))
            .push(Value::Integer(byte_range[2]))
            .push(Value::Integer(byte_range[3])),
    );
    let catalog = Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("SigFlags", Value::Integer(3))
                .insert("AcroForm", Value::Ref(2, 0)),
        ),
    );
    let acro_form = Object::new(
        2,
        0,
        Value::Dict(Dict::new().insert(
            "Fields",
            Value::Array(crate::core::types::Array::new().push(Value::Ref(3, 0))),
        )),
    );
    let field = Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Annot".into()))
                .insert("Subtype", Value::Name("Widget".into()))
                .insert(
                    "Rect",
                    Value::Array(
                        crate::core::types::Array::new()
                            .push(Value::Integer(72))
                            .push(Value::Integer(700))
                            .push(Value::Integer(200))
                            .push(Value::Integer(740)),
                    ),
                )
                .insert("FT", Value::Name("Sig".into()))
                .insert("V", Value::Ref(4, 0)),
        ),
    );
    let signature = Object::new(
        4,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Sig".into()))
                .insert("Filter", Value::Name("Adobe.PPKLite".into()))
                .insert("SubFilter", Value::Name("adbe.pkcs7.detached".into()))
                .insert("ByteRange", range_value)
                .insert("Contents", Value::HexString(contents_hex.into()))
                .insert("Name", Value::String("Signer".into()))
                .insert("M", Value::String("D:20261028000000Z".into())),
        ),
    );
    let mut doc = Document::from_objects(
        vec![catalog, acro_form, field, signature],
        Xref::new(),
        Trailer::new(),
        StartXref(0),
    );
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc
}

/// 用 `signed_file` 的产物解析出签名对象（供摘要类测试共享）。
fn parsed_signed(tag: &str) -> (Document, Vec<u8>, DocumentSignature) {
    let bytes = signed_file();
    let doc = parse_bytes(&bytes, tag);
    let sigs = doc.document_signatures().expect("signatures");
    assert_eq!(sigs.len(), 1);
    (doc, bytes, sigs.into_iter().next().unwrap())
}

#[test]
fn signature_flags_exposed_on_catalog() {
    let (doc, _, _) = parsed_signed("flags");
    assert_eq!(doc.signature_flags().expect("flags"), Some(3));
}

#[test]
fn document_signatures_finds_sig_field_value() {
    let (doc, _, sig) = parsed_signed("discover");
    assert_eq!(sig.reference, Some((4, 0)));
    assert_eq!(sig.filter.as_deref(), Some("Adobe.PPKLite"));
    assert_eq!(sig.sub_filter, Some(SignatureSubFilter::Pkcs7Detached));
    assert_eq!(sig.name.as_deref(), Some("Signer"));
    assert_eq!(sig.location, None);
    assert_eq!(sig.reason, None);
    // 完整字典保留：未建模条目可经 dict 访问。
    assert!(sig.dict.get("M").is_some());
    let _ = &doc;
}

#[test]
fn byte_range_segments_exclude_contents_placeholder() {
    let (doc, bytes, sig) = parsed_signed("ranges");
    let ranges = doc
        .validate_signature_byte_range(&sig)
        .expect("ranges valid");
    let file_len = bytes.len() as u64;
    let [(o1, l1), (o2, l2)] = ranges;
    assert_eq!(o1, 0);
    assert_eq!(o2 + l2, file_len, "第二段应覆盖到文件末尾");
    let gap = o2 - (o1 + l1);
    // 间隙 = "<" + 128 hex 字符 + ">"，即整个 Contents 字符串。
    assert_eq!(gap, PLACEHOLDER.len() as u64 + 2);
    assert_eq!(sig.contents_len(), Some(PLACEHOLDER.len() / 2));
    assert_eq!(sig.covered_len(), file_len - gap);
}

#[test]
fn signature_digest_matches_covered_bytes() {
    let (doc, bytes, sig) = parsed_signed("digest");
    let [(o1, l1), (o2, l2)] = doc
        .validate_signature_byte_range(&sig)
        .expect("ranges valid");
    let backend = RustCryptoSignatureBackend;
    let mut manual = backend.start(DigestAlgorithm::Sha256);
    manual.push(&bytes[o1 as usize..(o1 + l1) as usize]);
    manual.push(&bytes[o2 as usize..(o2 + l2) as usize]);
    let expected = manual.finish();
    let actual = doc
        .signature_digest(&sig, DigestAlgorithm::Sha256, &backend)
        .expect("digest");
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 32);
}

#[test]
fn tampering_covered_bytes_changes_signature_digest() {
    let bytes = signed_file();
    let doc = parse_bytes(&bytes, "tamper");
    let sig = &doc.document_signatures().expect("signatures")[0];
    let backend = RustCryptoSignatureBackend;
    let clean = doc
        .signature_digest(sig, DigestAlgorithm::Sha256, &backend)
        .expect("digest clean");
    let mut mutated = bytes.clone();
    // 篡改覆盖段内、解析安全的字节：字典键序使 /Name 字面量物理上
    // 位于 Contents 占位之后（第二段覆盖范围内）。改字符不影响解析，
    // 但必须落入 ByteRange 覆盖范围。
    let at = find_occurrence(&bytes, b"(Signer)").expect("name literal");
    let [(o1, l1), (o2, l2)] = sig.covered_ranges();
    assert!(
        (at + 1) >= o2 as usize && (at + 1) < (o2 + l2) as usize,
        "/Name 位于第二覆盖段内（第一段止于 {o1}+{l1}）"
    );
    mutated[at + 1] ^= 0x20;
    let doc2 = parse_bytes(&mutated, "tampered");
    let sig2 = &doc2.document_signatures().expect("signatures tampered")[0];
    let dirty = doc2
        .signature_digest(sig2, DigestAlgorithm::Sha256, &backend)
        .expect("digest tampered");
    assert_ne!(clean, dirty, "ByteRange 必须覆盖被篡改字节");
}

#[test]
fn filling_contents_preserves_digest_of_other_bytes() {
    // 「先占位后填充」流程稳定性：/Contents 不在覆盖范围内，
    // 把占位 hex 换成任意等长内容，摘要不变。
    let bytes = signed_file();
    let doc = parse_bytes(&bytes, "stable");
    let sig = &doc.document_signatures().expect("signatures")[0];
    let backend = RustCryptoSignatureBackend;
    let before = doc
        .signature_digest(sig, DigestAlgorithm::Sha256, &backend)
        .expect("digest before");
    let needle = format!("<{PLACEHOLDER}>");
    let filled = format!("<{}>", "AB".repeat(PLACEHOLDER.len() / 2));
    let at = find_occurrence(&bytes, needle.as_bytes()).expect("placeholder");
    let mut patched = bytes.clone();
    patched.splice(at..at + needle.len(), filled.into_bytes());
    let doc2 = parse_bytes(&patched, "filled");
    let sig2 = &doc2.document_signatures().expect("signatures filled")[0];
    let after = doc2
        .signature_digest(sig2, DigestAlgorithm::Sha256, &backend)
        .expect("digest after");
    assert_eq!(before, after);
}

#[test]
fn subfilter_mapping_and_default_digest() {
    assert_eq!(
        SignatureSubFilter::from_name("adbe.pkcs7.sha1").default_digest(),
        Some(DigestAlgorithm::Sha1)
    );
    assert_eq!(
        SignatureSubFilter::from_name("adbe.x509.rsa_sha1"),
        SignatureSubFilter::X509RsaSha1
    );
    assert_eq!(
        SignatureSubFilter::from_name("ETSI.CAdES.detached").default_digest(),
        None
    );
    assert_eq!(
        SignatureSubFilter::from_name("custom.vendor").as_str(),
        "custom.vendor"
    );
    let bytes = signed_file();
    let doc = parse_bytes(&bytes, "subfilter");
    // 换成未知 SubFilter 后仍可读取（名称原样保留）。
    let mut obj = doc.get_object(4).unwrap().unwrap();
    let Value::Dict(d) = obj.value.clone() else {
        panic!("sig dict");
    };
    obj.value = Value::Dict(d.insert("SubFilter", Value::Name("vendor.detached".into())));
    doc.set_object(obj);
    let sig = &doc.document_signatures().expect("unknown subfilter")[0];
    assert_eq!(
        sig.sub_filter,
        Some(SignatureSubFilter::Unknown("vendor.detached".into()))
    );
}

#[test]
fn malformed_byte_ranges_are_typed_errors() {
    let doc = build_sig_document("00", [0, 10, 20, 30]);
    let value = Value::Ref(4, 0);
    assert!(doc.signature(&value).is_ok());
    // 数组长度 3。
    let bad = Object::new(
        5,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Sig".into()))
                .insert(
                    "ByteRange",
                    Value::Array(crate::core::types::Array::new().push(Value::Integer(0))),
                ),
        ),
    );
    doc.set_object(bad);
    let err = doc.signature(&Value::Ref(5, 0)).unwrap_err();
    assert!(matches!(err, SignatureError::Malformed(_)));
    // 含负数。
    let bad2 = Object::new(
        6,
        0,
        Value::Dict(
            Dict::new().insert(
                "ByteRange",
                Value::Array(
                    crate::core::types::Array::new()
                        .push(Value::Integer(0))
                        .push(Value::Integer(-1))
                        .push(Value::Integer(2))
                        .push(Value::Integer(3)),
                ),
            ),
        ),
    );
    doc.set_object(bad2);
    assert!(matches!(
        doc.signature(&Value::Ref(6, 0)).unwrap_err(),
        SignatureError::Malformed(_)
    ));
    // 非整数字段。
    let bad3 = Object::new(
        7,
        0,
        Value::Dict(
            Dict::new().insert(
                "ByteRange",
                Value::Array(
                    crate::core::types::Array::new()
                        .push(Value::Name("zero".into()))
                        .push(Value::Integer(1))
                        .push(Value::Integer(2))
                        .push(Value::Integer(3)),
                ),
            ),
        ),
    );
    doc.set_object(bad3);
    assert!(matches!(
        doc.signature(&Value::Ref(7, 0)).unwrap_err(),
        SignatureError::Malformed(_)
    ));
    // 首段起点非 0。
    let bad4 = Object::new(
        8,
        0,
        Value::Dict(
            Dict::new().insert(
                "ByteRange",
                Value::Array(
                    crate::core::types::Array::new()
                        .push(Value::Integer(4))
                        .push(Value::Integer(1))
                        .push(Value::Integer(2))
                        .push(Value::Integer(3)),
                ),
            ),
        ),
    );
    doc.set_object(bad4);
    let sig4 = doc.signature(&Value::Ref(8, 0)).expect("parses");
    assert!(matches!(
        doc.validate_signature_byte_range(&sig4).unwrap_err(),
        SignatureError::Malformed(_)
    ));
}

#[test]
fn non_monotonic_or_overlong_byte_range_is_malformed() {
    let bytes = signed_file();
    let doc = parse_bytes(&bytes, "badrange");
    let put = |num: i64, range: [i64; 4]| {
        doc.set_object(Object::new(
            num,
            0,
            Value::Dict(
                Dict::new().insert(
                    "ByteRange",
                    Value::Array(
                        crate::core::types::Array::new()
                            .push(Value::Integer(range[0]))
                            .push(Value::Integer(range[1]))
                            .push(Value::Integer(range[2]))
                            .push(Value::Integer(range[3])),
                    ),
                ),
            ),
        ));
    };
    // 第二段越过文件末尾。
    put(9, [0, 10, 20, 10_000_000]);
    let over = doc.signature(&Value::Ref(9, 0)).expect("parses");
    assert!(matches!(
        doc.validate_signature_byte_range(&over).unwrap_err(),
        SignatureError::Malformed(_)
    ));
    // 无正间隔（o2 == o1 + l1）。
    put(10, [0, 10, 10, 5]);
    let no_gap = doc.signature(&Value::Ref(10, 0)).expect("parses");
    assert!(matches!(
        doc.validate_signature_byte_range(&no_gap).unwrap_err(),
        SignatureError::Malformed(_)
    ));
    // 零长度段。
    put(11, [0, 0, 10, 5]);
    let zero_len = doc.signature(&Value::Ref(11, 0)).expect("parses");
    assert!(matches!(
        doc.validate_signature_byte_range(&zero_len).unwrap_err(),
        SignatureError::Malformed(_)
    ));
}

#[test]
fn signature_requires_dict_type_sig() {
    let doc = build_sig_document("00", [0, 1, 2, 3]);
    doc.set_object(Object::new(
        9,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("EmbeddedFile".into()))
                .insert(
                    "ByteRange",
                    Value::Array(
                        crate::core::types::Array::new()
                            .push(Value::Integer(0))
                            .push(Value::Integer(1))
                            .push(Value::Integer(2))
                            .push(Value::Integer(3)),
                    ),
                ),
        ),
    ));
    assert!(matches!(
        doc.signature(&Value::Ref(9, 0)).unwrap_err(),
        SignatureError::Malformed(_)
    ));
    // /Type 缺失（宽容形态）仍可解析。
    doc.set_object(Object::new(
        10,
        0,
        Value::Dict(
            Dict::new().insert(
                "ByteRange",
                Value::Array(
                    crate::core::types::Array::new()
                        .push(Value::Integer(0))
                        .push(Value::Integer(1))
                        .push(Value::Integer(2))
                        .push(Value::Integer(3)),
                ),
            ),
        ),
    ));
    assert!(doc.signature(&Value::Ref(10, 0)).is_ok());
}

#[test]
fn signature_digest_requires_file_backed_document() {
    let doc = build_sig_document(PLACEHOLDER, [0, 10, 200, 50]);
    let sig = doc.signature(&Value::Ref(4, 0)).expect("parses");
    let backend = RustCryptoSignatureBackend;
    assert_eq!(
        doc.signature_digest(&sig, DigestAlgorithm::Sha256, &backend)
            .unwrap_err(),
        SignatureError::NoSource
    );
}
