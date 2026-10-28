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
    signed_file_with(None, &[])
}

/// 同上，可附加签名字典 `/Reference` 数组与额外对象（如 `/Data`
/// 指向的嵌入 PDF 流）。额外对象号从 5 起分配。
fn signed_file_with(references: Option<Value>, extra_objects: &[Object]) -> Vec<u8> {
    let contents = format!("<{PLACEHOLDER}>");
    let mut range = [0i64; 4];
    let mut out = Vec::new();
    let mut stable = false;
    for _ in 0..8 {
        let doc = build_sig_document(PLACEHOLDER, range, references.clone(), extra_objects);
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

/// 手写一个签名文档（1.4 经典写出为明文顶层对象）。
/// `/ByteRange` 以固定四整数写入；`contents_hex` 为 `/Contents` 的
/// 十六进制串原始字符（不含 `<>`）；`references` 写入签名字典
/// `/Reference`；`extra_objects` 追加为独立对象。
fn build_sig_document(
    contents_hex: &str,
    byte_range: [i64; 4],
    references: Option<Value>,
    extra_objects: &[Object],
) -> Document {
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
    let mut signature_dict = Dict::new()
        .insert("Type", Value::Name("Sig".into()))
        .insert("Filter", Value::Name("Adobe.PPKLite".into()))
        .insert("SubFilter", Value::Name("adbe.pkcs7.detached".into()))
        .insert("ByteRange", range_value)
        .insert("Contents", Value::HexString(contents_hex.into()))
        .insert("Name", Value::String("Signer".into()))
        .insert("M", Value::String("D:20261028000000Z".into()));
    if let Some(value) = references {
        signature_dict = signature_dict.insert("Reference", value);
    }
    let signature = Object::new(4, 0, Value::Dict(signature_dict));
    let mut objects = vec![catalog, acro_form, field, signature];
    objects.extend(extra_objects.iter().cloned());
    let mut doc = Document::from_objects(objects, Xref::new(), Trailer::new(), StartXref(0));
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
    let doc = build_sig_document("00", [0, 10, 20, 30], None, &[]);
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
    let doc = build_sig_document("00", [0, 1, 2, 3], None, &[]);
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
    let doc = build_sig_document(PLACEHOLDER, [0, 10, 200, 50], None, &[]);
    let sig = doc.signature(&Value::Ref(4, 0)).expect("parses");
    let backend = RustCryptoSignatureBackend;
    assert_eq!(
        doc.signature_digest(&sig, DigestAlgorithm::Sha256, &backend)
            .unwrap_err(),
        SignatureError::NoSource
    );
}

/// 变换参数字典（/Type /TransformParams + 方法专属条目）。
fn transform_dict(entries: Vec<(&str, Value)>) -> Value {
    let mut dict = Dict::new().insert("Type", Value::Name("TransformParams".into()));
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    Value::Dict(dict)
}

/// 签名字典引用条目（Table 257）。
fn reference_dict(method: &str, params: Value, extra: Vec<(&str, Value)>) -> Value {
    let mut dict = Dict::new()
        .insert("Type", Value::Name("SigReference".into()))
        .insert("TransformMethod", Value::Name(method.into()))
        .insert("TransformParams", params);
    for (key, value) in extra {
        dict = dict.insert(key, value);
    }
    Value::Dict(dict)
}

fn docmdp_reference(permission: i64) -> Value {
    reference_dict(
        "DocMDP",
        transform_dict(vec![("P", Value::Integer(permission))]),
        vec![],
    )
}

fn refs_array(items: Vec<Value>) -> Value {
    let mut array = crate::core::types::Array::new();
    for item in items {
        array = array.push(item);
    }
    Value::Array(array)
}

#[test]
fn docmdp_reference_exposes_permission_through_discovery() {
    let bytes = signed_file_with(Some(refs_array(vec![docmdp_reference(2)])), &[]);
    let doc = parse_bytes(&bytes, "docmdp");
    let sigs = doc.document_signatures().expect("signatures");
    let refs = doc.signature_references(&sigs[0]).expect("references");
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].transform_method, Some(TransformMethod::Docmdp));
    assert_eq!(
        refs[0].params,
        Some(TransformParams::Docmdp {
            permission: DocmdpPermission::FillAndAnnotate
        })
    );
    assert_eq!(
        doc.doc_modification_permission(&sigs[0]).expect("mdp"),
        Some(DocmdpPermission::FillAndAnnotate)
    );
}

#[test]
fn fieldmdp_reference_decodes_scope_and_annotation_bits() {
    let params = transform_dict(vec![
        ("P", Value::Name("Include".into())),
        ("Annots", Value::HexString("8040".into())),
    ]);
    let reference = reference_dict("FieldMDP", params, vec![]);
    let doc = build_sig_document(
        PLACEHOLDER,
        [0, 10, 20, 30],
        Some(refs_array(vec![reference])),
        &[],
    );
    let sig = doc.signature(&Value::Ref(4, 0)).expect("signature");
    let refs = doc.signature_references(&sig).expect("references");
    match &refs[0].params {
        Some(TransformParams::Fieldmdp {
            permission,
            annotations,
            ..
        }) => {
            assert_eq!(*permission, FieldMdPermission::Include);
            assert_eq!(annotations, &vec![0x80, 0x40]);
        }
        other => panic!("expected FieldMDP params, got {other:?}"),
    }
    assert_eq!(
        doc.field_modification_params(&sig)
            .expect("mdp")
            .map(|params| matches!(params, TransformParams::Fieldmdp { .. })),
        Some(true)
    );
}

#[test]
fn ur_reference_models_scope_version_and_reason() {
    let params = transform_dict(vec![
        ("V", Value::Integer(12)),
        (
            "Scope",
            Value::Array(
                crate::core::types::Array::new()
                    .push(Value::Name("Form".into()))
                    .push(Value::Name("Annotation".into())),
            ),
        ),
        ("NeedAppearances", Value::Bool(true)),
        ("Reason", Value::String("review".into())),
    ]);
    let doc = build_sig_document(
        PLACEHOLDER,
        [0, 10, 20, 30],
        Some(refs_array(vec![reference_dict("UR", params, vec![])])),
        &[],
    );
    let sig = doc.signature(&Value::Ref(4, 0)).expect("signature");
    let refs = doc.signature_references(&sig).expect("references");
    match &refs[0].params {
        Some(TransformParams::Ur {
            version,
            scope,
            needs_appearances,
            reason,
            ..
        }) => {
            assert_eq!(*version, 12);
            assert!(scope.form && scope.annotation && !scope.doc_attachment);
            assert!(needs_appearances);
            assert_eq!(reason.as_deref(), Some("review"));
        }
        other => panic!("expected UR params, got {other:?}"),
    }
}

#[test]
fn timestamp_reference_without_method_is_skipped() {
    // 12.8.4.4：无 /TransformMethod 的时间戳引用不进变换模型。
    let timestamp = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("SigReference".into()))
            .insert("RelativePeriod", Value::Integer(86_400)),
    );
    let doc = build_sig_document(
        PLACEHOLDER,
        [0, 10, 20, 30],
        Some(refs_array(vec![timestamp])),
        &[],
    );
    let sig = doc.signature(&Value::Ref(4, 0)).expect("signature");
    assert!(
        doc.signature_references(&sig)
            .expect("references")
            .is_empty()
    );
}

#[test]
fn malformed_transform_params_are_typed_errors() {
    let cases = vec![
        // DocMDP 缺 /P。
        reference_dict("DocMDP", transform_dict(vec![]), vec![]),
        // DocMDP /P 越界。
        docmdp_reference(4),
        // FieldMDP /P 非法名。
        reference_dict(
            "FieldMDP",
            transform_dict(vec![("P", Value::Name("Some".into()))]),
            vec![],
        ),
        // UR 缺 /Scope。
        reference_dict(
            "UR",
            transform_dict(vec![("V", Value::Integer(12))]),
            vec![],
        ),
        // 未知变换方法。
        reference_dict("SomethingElse", transform_dict(vec![]), vec![]),
    ];
    for case in cases {
        let doc = build_sig_document(
            PLACEHOLDER,
            [0, 10, 20, 30],
            Some(refs_array(vec![case])),
            &[],
        );
        let sig = doc.signature(&Value::Ref(4, 0)).expect("signature");
        assert!(
            matches!(
                doc.signature_references(&sig),
                Err(SignatureError::Malformed(_))
            ),
            "expected malformed error"
        );
    }
}

#[test]
fn reference_digest_matches_embedded_pdf_stream() {
    // 12.8.3：/DigestValue 是 /Data 指向的嵌入 PDF 流字节的摘要，
    // /DigestLength 声明其字节数。
    const EMBEDDED: &[u8] = b"%PDF-1.4 embedded AAAA";
    let digest = {
        let backend = RustCryptoSignatureBackend;
        let mut sink = backend.start(DigestAlgorithm::Sha256);
        sink.push(EMBEDDED);
        sink.finish()
    };
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<String>();
    let data_stream = Object::new(
        6,
        0,
        Value::Stream(
            Dict::new()
                .insert("Type", Value::Name("EmbeddedPDF".into()))
                .insert("Subtype", Value::Name("Application/pdf".into())),
            crate::core::types::StreamData::Owned(EMBEDDED.to_vec()),
        ),
    );
    let reference = reference_dict(
        "DocMDP",
        transform_dict(vec![("P", Value::Integer(1))]),
        vec![
            ("Data", Value::Ref(6, 0)),
            ("DigestValue", Value::HexString(hex)),
            ("DigestLength", Value::Integer(32)),
        ],
    );
    let bytes = signed_file_with(
        Some(Value::Array(
            crate::core::types::Array::new().push(Value::Ref(5, 0)),
        )),
        &[
            Object::new(
                5,
                0,
                match reference {
                    Value::Dict(dict) => Value::Dict(dict),
                    _ => unreachable!(),
                },
            ),
            data_stream,
        ],
    );
    let doc = parse_bytes(&bytes, "digest-ref");
    let sig = doc.document_signatures().expect("signatures").remove(0);
    let refs = doc.signature_references(&sig).expect("references");
    assert_eq!(refs[0].data_reference(), Some((6, 0)));
    assert_eq!(
        refs[0].matches_digest_length(DigestAlgorithm::Sha256),
        Some(true)
    );
    let data_value = doc.get_object(6).expect("object").unwrap().value;
    let embedded = doc
        .decode_stream_bytes(&data_value)
        .expect("embedded bytes");
    assert_eq!(embedded, EMBEDDED);
    let backend = RustCryptoSignatureBackend;
    let mut sink = backend.start(DigestAlgorithm::Sha256);
    sink.push(&embedded);
    let recomputed = sink.finish();
    let Value::HexString(declared_hex) = refs[0].digest_value.as_ref().expect("digest") else {
        panic!("digest value shape");
    };
    let declared = (0..declared_hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&declared_hex[i..i + 2], 16).unwrap())
        .collect::<Vec<u8>>();
    assert_eq!(recomputed, declared);
    // 篡改嵌入字节（等长替换保持文件结构）后摘要不再匹配。
    let at = find_occurrence(&bytes, b"AAAA").expect("embedded marker");
    let mut mutated = bytes.clone();
    mutated[at..at + 4].copy_from_slice(b"BBBB");
    let doc2 = parse_bytes(&mutated, "digest-ref-tampered");
    let data2 = doc2.get_object(6).expect("object").unwrap().value;
    let embedded2 = doc2.decode_stream_bytes(&data2).expect("embedded2");
    let mut sink2 = backend.start(DigestAlgorithm::Sha256);
    sink2.push(&embedded2);
    assert_ne!(sink2.finish(), declared);
}

/// 构造带目录 `/Perms` 的签名 PDF（Table 268）。文档仅作结构用途，
/// ByteRange 不做定点收敛（/Perms 测试不校验摘要）。
fn signed_file_with_perms(perms: Dict) -> Vec<u8> {
    let doc = build_sig_document(PLACEHOLDER, [0; 4], None, &[]);
    let catalog_value = doc
        .get_object(1)
        .expect("catalog")
        .expect("catalog object")
        .value
        .clone();
    let mut dict = match catalog_value {
        Value::Dict(dict) => dict,
        _ => panic!("catalog must be a dictionary"),
    };
    dict = dict.insert("Perms", Value::Dict(perms));
    doc.set_object(Object::new(1, 0, Value::Dict(dict)));
    let mut buf = Vec::new();
    DocumentWriter::new()
        .with_version(PdfVersion::V1_4)
        .build(&doc, &mut buf)
        .expect("perms build");
    buf
}

#[test]
fn perms_absent_returns_none_and_empty_signatures() {
    let doc = parse_bytes(&signed_file(), "perms-absent");
    assert_eq!(
        doc.perms_entry(PermsEntryName::DocumentPermissions)
            .expect("none"),
        None
    );
    assert_eq!(doc.perms_signatures().expect("empty").len(), 0);
    assert_eq!(
        doc.perms_document_modification_permission().expect("none"),
        None
    );
}

#[test]
fn perms_docmdp_resolves_indirect_signature_and_permission() {
    let perms = Dict::new().insert("DocMDP", Value::Ref(4, 0));
    let doc = parse_bytes(&signed_file_with_perms(perms), "perms-docmdp");
    let entry = doc
        .perms_entry(PermsEntryName::DocumentPermissions)
        .expect("entry")
        .expect("present");
    assert_eq!(entry.name, PermsEntryName::DocumentPermissions);
    assert_eq!(entry.raw, Value::Ref(4, 0));
    let signature = entry.signature.as_ref().expect("signature resolves");
    assert_eq!(signature.reference, Some((4, 0)));
    assert_eq!(signature.name.as_deref(), Some("Signer"));
    // DocMDP 签名字典未带整数 /P：级别为 None，便捷入口同为 None。
    assert_eq!(entry.permission, None);
    assert_eq!(
        doc.perms_document_modification_permission().expect("none"),
        None
    );
}

#[test]
fn perms_docmdp_reads_permission_from_signature_dictionary() {
    let doc = build_sig_document(PLACEHOLDER, [0; 4], None, &[]);
    let sig_value = doc
        .get_object(4)
        .expect("sig")
        .expect("sig object")
        .value
        .clone();
    let mut sig_dict = match sig_value {
        Value::Dict(dict) => dict,
        _ => panic!("signature must be a dictionary"),
    };
    sig_dict = sig_dict.insert("P", Value::Integer(2));
    doc.set_object(Object::new(4, 0, Value::Dict(sig_dict)));
    let perms = Dict::new().insert("DocMDP", Value::Ref(4, 0));
    let catalog_value = doc
        .get_object(1)
        .expect("catalog")
        .expect("catalog object")
        .value
        .clone();
    let mut catalog = match catalog_value {
        Value::Dict(dict) => dict,
        _ => panic!("catalog must be a dictionary"),
    };
    catalog = catalog.insert("Perms", Value::Dict(perms));
    doc.set_object(Object::new(1, 0, Value::Dict(catalog)));
    let mut buf = Vec::new();
    DocumentWriter::new()
        .with_version(PdfVersion::V1_4)
        .build(&doc, &mut buf)
        .expect("perms p build");
    let parsed = parse_bytes(&buf, "perms-docmdp-p");
    assert_eq!(
        parsed
            .perms_document_modification_permission()
            .expect("permission"),
        Some(DocmdpPermission::FillAndAnnotate)
    );
}

#[test]
fn perms_sig_array_returns_all_signatures() {
    let perms = Dict::new().insert(
        "Sig",
        Value::Array(crate::core::types::Array::new().push(Value::Ref(4, 0))),
    );
    let doc = parse_bytes(&signed_file_with_perms(perms), "perms-sig");
    let sigs = doc.perms_signatures().expect("perms sigs");
    assert_eq!(sigs.len(), 1);
    assert_eq!(sigs[0].reference, Some((4, 0)));
    assert_eq!(sigs[0].sub_filter, Some(SignatureSubFilter::Pkcs7Detached));
}

#[test]
fn perms_sig_flags_entry_keeps_raw_value_without_signature() {
    let perms = Dict::new()
        .insert("SigFlags", Value::Integer(1))
        .insert("Weird", Value::Name("Something".into()));
    let doc = parse_bytes(&signed_file_with_perms(perms), "perms-flags");
    let entry = doc
        .perms_entry(PermsEntryName::SignatureFlag)
        .expect("entry")
        .expect("present");
    assert_eq!(entry.raw, Value::Integer(1));
    assert!(entry.signature.is_none());
    let unknown = doc
        .perms_entry(PermsEntryName::from_key("Weird"))
        .expect("unknown entry")
        .expect("present");
    assert_eq!(unknown.name, PermsEntryName::Unknown("Weird".into()));
}

#[test]
fn perms_malformed_shapes_are_typed_errors() {
    // /Perms 不是字典。
    let doc = build_sig_document(PLACEHOLDER, [0; 4], None, &[]);
    let catalog_value = doc
        .get_object(1)
        .expect("catalog")
        .expect("catalog object")
        .value
        .clone();
    let mut catalog = match catalog_value {
        Value::Dict(dict) => dict,
        _ => panic!("catalog must be a dictionary"),
    };
    catalog = catalog.insert("Perms", Value::Integer(7));
    doc.set_object(Object::new(1, 0, Value::Dict(catalog)));
    let mut buf = Vec::new();
    DocumentWriter::new()
        .with_version(PdfVersion::V1_4)
        .build(&doc, &mut buf)
        .expect("bad perms build");
    let parsed = parse_bytes(&buf, "perms-not-dict");
    let error = parsed
        .perms_entry(PermsEntryName::DocumentPermissions)
        .expect_err("must error");
    assert!(matches!(error, SignatureError::Malformed(_)));
    // /Perms /DocMDP /P 越界：签名字典带非法 /P。
    let doc2 = build_sig_document(PLACEHOLDER, [0; 4], None, &[]);
    let sig_value = doc2
        .get_object(4)
        .expect("sig")
        .expect("sig object")
        .value
        .clone();
    let mut sig_dict = match sig_value {
        Value::Dict(dict) => dict,
        _ => panic!("signature must be a dictionary"),
    };
    sig_dict = sig_dict.insert("P", Value::Integer(9));
    doc2.set_object(Object::new(4, 0, Value::Dict(sig_dict)));
    let catalog_value2 = doc2
        .get_object(1)
        .expect("catalog")
        .expect("catalog object")
        .value
        .clone();
    let mut catalog2 = match catalog_value2 {
        Value::Dict(dict) => dict,
        _ => panic!("catalog must be a dictionary"),
    };
    catalog2 = catalog2.insert(
        "Perms",
        Value::Dict(Dict::new().insert("DocMDP", Value::Ref(4, 0))),
    );
    doc2.set_object(Object::new(1, 0, Value::Dict(catalog2)));
    let mut buf2 = Vec::new();
    DocumentWriter::new()
        .with_version(PdfVersion::V1_4)
        .build(&doc2, &mut buf2)
        .expect("bad p build");
    let parsed2 = parse_bytes(&buf2, "perms-bad-p");
    let error2 = parsed2
        .perms_entry(PermsEntryName::DocumentPermissions)
        .expect_err("out-of-range /P must error");
    assert!(matches!(error2, SignatureError::Malformed(_)));
}
