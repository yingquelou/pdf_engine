//! ByteRange 覆盖范围与分段摘要。

use super::*;

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
