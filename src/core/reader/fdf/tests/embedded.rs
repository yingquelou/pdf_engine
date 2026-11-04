//! FDF 读端 `/Differences`（Table 243 续表 raw L24906–L24926）与
//! `/EmbeddedFDFs`（raw L24930–L24932）的类型化单元测试：流形态识别与
//! 解引用（直接/引用/悬空/非流）、7.11 文件规范逐项类型化、递归子 FDF
//! 文档、Table 244 `/EncryptionRevision` 识别 + 12.7.7.3 的 FDF 侧解密
//! （见 `embedded/decrypt.rs`；不带口令的旧入口依旧不解密）、条目/深度/环
//! 上限降级，以及 PDF 1.4 版本语义的宽容报告。

use super::*;
use crate::core::reader::fdf::embedded::{MAX_FDF_EMBEDDED_DEPTH, MAX_FDF_EMBEDDED_ENTRIES};
use crate::core::reader::fdf::{FdfEmbeddedFdf, FdfEmbeddedFdfs};
use crate::core::types::StreamData;

/// 12.7.7.3 的 FDF 侧解密（密钥派生向量、RC4 对称性、带口令入口）。
mod decrypt;
/// 递归放大防护（跨层全局条目预算）与子文档 `/Differences` 就地类型化。
mod limits;

/// 「目录 → FDF 字典」两段装配（FDF 字典条目由调用方给出）。
fn fdf_doc_with(entries: Dict) -> Document {
    doc_from_objects(vec![
        catalog_obj(Value::Ref(2, 0)),
        fdf_dict_obj(entries, 2),
    ])
}

/// 带 `/Version` 的 FDF 目录（Table 242），用于版本语义报告断言。
fn fdf_doc_with_catalog_version(entries: Dict, version: &str) -> Document {
    let catalog = Dict::new()
        .insert("FDF", Value::Ref(2, 0))
        .insert("Version", name_entry(version));
    doc_from_objects(vec![
        Object::new(1, 0, Value::Dict(catalog)),
        fdf_dict_obj(entries, 2),
    ])
}

/// 一个内嵌 FDF 载体流（无过滤器，字节即给定内容）。
fn carrier_stream(bytes: Vec<u8>, extra: Dict) -> Value {
    let dict = extra
        .insert("Type", name_entry("EmbeddedFile"))
        .insert("Length", Value::Integer(bytes.len() as i64));
    Value::Stream(dict, StreamData::Owned(bytes))
}

/// 一份可被递归解析的最小内层 FDF 文件字节（单对象目录，无 xref）。
fn inner_fdf_bytes(status: &str) -> Vec<u8> {
    format!("1 0 obj\n<< /FDF << /Status ({status}) /Fields [ << /T (child) /V (42) >> ] >> >>\nendobj\n%%EOF\n").into_bytes()
}

/// 逐层自嵌套的内层 FDF 字节（用于深度上限）：第 `level` 层的
/// `/EmbeddedFDFs` 指向承载第 `level-1` 层字节的间接流对象。
fn nested_fdf_bytes(level: usize) -> Vec<u8> {
    if level == 0 {
        return b"1 0 obj\n<< /FDF << /Status (leaf) >> >>\nendobj\n%%EOF\n".to_vec();
    }
    let inner = nested_fdf_bytes(level - 1);
    let mut out = Vec::new();
    out.extend_from_slice(
        format!(
            "1 0 obj\n<< /FDF << /Status (level{level}) /EmbeddedFDFs [ 2 0 R ] >> >>\nendobj\n"
        )
        .as_bytes(),
    );
    out.extend_from_slice(
        format!("2 0 obj\n<< /F (level{level}.fdf) /EF 3 0 R >>\nendobj\n").as_bytes(),
    );
    out.extend_from_slice(format!("3 0 obj\n<< /Length {} >>\nstream\n", inner.len()).as_bytes());
    out.extend_from_slice(&inner);
    out.extend_from_slice(b"\nendstream\nendobj\n%%EOF\n");
    out
}

/// 装配「FDF 字典 `/EmbeddedFDFs` = 给定值」的文件，载体流等其余对象
/// 由调用方补充。
fn with_embedded_fdfs(value: Value, extra_objects: Vec<Object>) -> Document {
    let entries = Dict::new().insert("EmbeddedFDFs", value);
    let mut objects = vec![catalog_obj(Value::Ref(2, 0)), fdf_dict_obj(entries, 2)];
    objects.extend(extra_objects);
    doc_from_objects(objects)
}

/// 一个引用型文件规范：`num` 号对象是 `/F` + `/EF -> ef_num` 的文件规格，
/// `ef_num` 号对象承载 `bytes`。
fn referenced_filespec(num: i64, ef_num: i64, name: &str, bytes: Vec<u8>) -> Vec<Object> {
    let spec = Dict::new()
        .insert("F", Value::String(name.to_string()))
        .insert("EF", Value::Ref(ef_num, 0));
    vec![
        Object::new(num, 0, Value::Dict(spec)),
        Object::new(ef_num, 0, carrier_stream(bytes, Dict::new())),
    ]
}

fn embedded_of(document: &Document) -> FdfEmbeddedFdfs {
    document
        .fdf_embedded_fdfs()
        .expect("嵌入式 FDF 读端不应报错")
        .expect("/EmbeddedFDFs 存在时应给出类型化视图")
}

fn first_entry(view: &FdfEmbeddedFdfs) -> &FdfEmbeddedFdf {
    view.entries.first().expect("至少一个条目")
}

// ── /Differences ────────────────────────────────────────────────────

#[test]
fn differences_direct_stream_is_typed_and_keeps_dictionary_entries() {
    // 直接形态：确证是流，流字典条目原值保留（/Length /Filter 不裁剪）。
    let stream = Value::Stream(
        Dict::new()
            .insert("Length", Value::Integer(11))
            .insert("Filter", name_entry("ASCIIHexDecode"))
            .insert("Custom", Value::String("kept".to_string())),
        StreamData::Owned(b"48656C6C6F>".to_vec()),
    );
    let document = fdf_doc_with(Dict::new().insert("Differences", stream.clone()));
    let view = document
        .fdf_differences()
        .expect("读端不应报错")
        .expect("条目存在时应给出类型化视图");
    assert!(view.is_stream(), "直接流形态应被确证");
    assert_eq!(view.raw, stream, "原值一条不丢");
    assert_eq!(view.reference, None, "直接形态无对象号");
    assert_eq!(view.entry("Filter"), Some(&name_entry("ASCIIHexDecode")));
    assert_eq!(view.entry("Custom"), Some(&Value::String("kept".into())));
    assert_eq!(view.declared_length(), Some(11));
    assert!(view.dict().expect("流字典").get("Length").is_some());
    assert!(
        view.issues.is_empty(),
        "宽容读取不该记诊断：{:?}",
        view.issues
    );
}

#[test]
fn differences_reference_form_is_dereferenced_to_the_stream_object() {
    // 间接引用形态 `5 0 R`：解引用到流对象，同时保留引用号与原值。
    let stream = carrier_stream(b"append".to_vec(), Dict::new());
    let document = doc_from_objects(vec![
        catalog_obj(Value::Ref(2, 0)),
        fdf_dict_obj(Dict::new().insert("Differences", Value::Ref(5, 0)), 2),
        Object::new(5, 0, stream.clone()),
    ]);
    let view = document.fdf_differences().expect("ok").expect("视图");
    assert_eq!(view.reference, Some((5, 0)));
    assert_eq!(view.raw, Value::Ref(5, 0), "原值仍是引用形态");
    assert!(view.is_stream(), "引用应被解引用到流对象");
    assert_eq!(view.stream, Some(stream));
}

#[test]
fn differences_dangling_reference_keeps_raw_and_records_issue() {
    // 悬空引用（本 crate 单对象写端会产出这种对象号冲突）：不 panic、
    // 不报错，降级为诊断 + 原值保留。
    let document = fdf_doc_with(Dict::new().insert("Differences", Value::Ref(9, 0)));
    let view = document.fdf_differences().expect("ok").expect("视图");
    assert!(!view.is_stream());
    assert_eq!(view.stream, None);
    assert_eq!(view.raw, Value::Ref(9, 0));
    assert_eq!(view.reference, Some((9, 0)));
    let joined = view.issues.join("|");
    assert!(
        joined.contains("could not be resolved"),
        "解引用失败要记诊断：{joined}"
    );
}

#[test]
fn differences_non_stream_form_is_tolerated_and_kept_raw() {
    // Table 243 声明类型是 stream；名字形态读端宽容：类型化为未确证，
    // 原值保留，仅记诊断。
    let document = fdf_doc_with(Dict::new().insert("Differences", name_entry("not-a-stream")));
    let view = document.fdf_differences().expect("ok").expect("视图");
    assert!(!view.is_stream());
    assert_eq!(view.raw, name_entry("not-a-stream"));
    let joined = view.issues.join("|");
    assert!(joined.contains("declares a stream"), "got {joined}");
}

#[test]
fn differences_decoded_bytes_go_through_the_existing_filter_pipeline() {
    // 复用既有过滤器入口（`Document::decode_stream_bytes`）：ASCIIHexDecode
    // 在读取时展开，本车道不触碰 `src/core/filters/**`。
    let stream = Value::Stream(
        Dict::new()
            .insert("Length", Value::Integer(11))
            .insert("Filter", name_entry("ASCIIHexDecode")),
        StreamData::Owned(b"48656C6C6F>".to_vec()),
    );
    let document = fdf_doc_with(Dict::new().insert("Differences", stream));
    let view = document.fdf_differences().expect("ok").expect("视图");
    assert_eq!(
        view.decoded_bytes(&document).expect("解码应成功"),
        b"Hello".to_vec()
    );
}

#[test]
fn differences_absent_yields_no_view_and_keeps_raw_field_none() {
    // 缺省条目不物化：无 `/Differences` 时视图与 `differences` 原值都是 None。
    let document = fdf_doc_with(Dict::new().insert("Status", Value::String("ok".to_string())));
    assert_eq!(document.fdf_differences().expect("ok"), None);
    let fdf = document.fdf().expect("ok").expect("FDF");
    assert_eq!(fdf.fdf.differences, None);
}

// ── /EmbeddedFDFs：文件规范与递归 ───────────────────────────────────

#[test]
fn embedded_fdfs_array_items_are_typed_as_filespecifications() {
    // 7.11 文件规范逐项类型化（复用 reader/embedded.rs），/EF 载体流即
    // 嵌入式 FDF 文件；数组写成间接引用也接。
    let inner = inner_fdf_bytes("inner-ok");
    let spec = Dict::new()
        .insert("F", Value::String("tax-form.fdf".to_string()))
        .insert("UF", Value::HexString("FEFF0055".to_string()))
        .insert("EF", Value::Ref(7, 0));
    let document = with_embedded_fdfs(
        Value::Ref(6, 0),
        vec![
            Object::new(6, 0, field_array(vec![Value::Dict(spec.clone())])),
            Object::new(7, 0, carrier_stream(inner, Dict::new())),
        ],
    );
    let view = embedded_of(&document);
    assert_eq!(view.reference, Some((6, 0)), "数组引用形态应记下对象号");
    assert!(
        matches!(view.raw, Some(Value::Ref(6, 0))),
        "原值保留：{:?}",
        view.raw
    );
    assert_eq!(view.len(), 1);
    assert!(view.issues.is_empty(), "got {:?}", view.issues);
    let entry = first_entry(&view);
    // 展示名 `/UF` 优先（Table 44），故这里是解码后的 Unicode 名。
    assert_eq!(entry.name(), Some("U"));
    let filespec = entry.filespec.as_ref().expect("文件规范应被类型化");
    assert_eq!(filespec.short_name.as_deref(), Some("tax-form.fdf"));
    assert_eq!(filespec.unicode_name.as_deref(), Some("U"));
    assert!(filespec.primary().is_some(), "/EF 载体流应被识别");
    assert_eq!(
        entry.data().expect("载体字节").len(),
        inner_fdf_bytes("inner-ok").len()
    );
}

#[test]
fn embedded_fdfs_recurses_into_the_inner_fdf_document() {
    // 可解出的嵌入式 FDF 流：递归调用现有 FDF 根字典解析入口得到子文档
    // 视图（Table 242/243/246 全部照常类型化）。
    let inner = inner_fdf_bytes("inner-ok");
    let spec = Dict::new()
        .insert("F", Value::String("child.fdf".to_string()))
        .insert("EF", carrier_stream(inner, Dict::new()));
    let document = with_embedded_fdfs(field_array(vec![Value::Dict(spec)]), Vec::new());
    let view = embedded_of(&document);
    let entry = first_entry(&view);
    let child = entry.document.clone().expect("子 FDF 文档应被递归解析");
    assert_eq!(child.fdf.status.as_deref(), Some("inner-ok"));
    let names: Vec<&str> = child
        .all_fields()
        .iter()
        .map(|field| field.fully_qualified_name.as_str())
        .collect();
    assert_eq!(names, vec!["child"]);
    assert_eq!(
        child.field("child").expect("按全限定名查找子字段").value,
        Some(Value::String("42".to_string()))
    );
    // 子文档没有自己的 /EmbeddedFDFs：不物化。
    assert!(entry.nested.is_none());
    assert!(entry.issues.is_empty(), "got {:?}", entry.issues);
    // 便捷按名查找。
    assert!(view.document("child.fdf").is_some());
    assert!(view.document("absent.fdf").is_none());
}

#[test]
fn embedded_fdfs_cycle_is_detected_and_recursion_skipped() {
    // 同一对象号不得重复进入：第二个条目复用同一文件规范/载体流 → 记
    // 诊断并停止递归，但条目本身仍类型化、原值不丢。
    let inner = inner_fdf_bytes("once");
    let objects = referenced_filespec(6, 7, "shared.fdf", inner);
    let document = with_embedded_fdfs(
        field_array(vec![Value::Ref(6, 0), Value::Ref(6, 0)]),
        objects,
    );
    let view = embedded_of(&document);
    assert_eq!(view.len(), 2);
    assert!(view.entries[0].document.is_some(), "首次进入应递归");
    assert_eq!(view.entries[1].name(), Some("shared.fdf"));
    assert!(view.entries[1].document.is_none(), "环路条目不再展开");
    assert!(view.entries[1].nested.is_none());
    let joined = view.entries[1].issues.join("|");
    assert!(joined.contains("already visited"), "got {joined}");
    assert!(
        matches!(&view.entries[1].raw, Value::Ref(num, 0) if *num == 6),
        "原值保留：{:?}",
        view.entries[1].raw
    );
}

#[test]
fn embedded_fdfs_depth_limit_stops_recursion_and_is_reported() {
    // 自嵌套链超过库策略层数上限：前 MAX_FDF_EMBEDDED_DEPTH 层有子文档，
    // 其后一条记「recursion limit」且不再展开。
    let total = MAX_FDF_EMBEDDED_DEPTH + 3;
    let inner = nested_fdf_bytes(total);
    let spec = Dict::new()
        .insert("F", Value::String("l0.fdf".to_string()))
        .insert("EF", carrier_stream(inner, Dict::new()));
    let document = with_embedded_fdfs(field_array(vec![Value::Dict(spec)]), Vec::new());
    let top = embedded_of(&document);
    let mut current: &FdfEmbeddedFdfs = &top;
    let mut expanded = 0usize;
    loop {
        let entry = current.entries.first().expect("每层一个条目");
        let (Some(child), Some(nested)) = (entry.document.as_deref(), entry.nested.as_deref())
        else {
            assert_eq!(expanded, MAX_FDF_EMBEDDED_DEPTH, "层数上限必须精确生效");
            let joined = entry.issues.join("|");
            assert!(joined.contains("recursion limit"), "got {joined}");
            assert!(entry.nested.is_none(), "超限层不再有子视图");
            break;
        };
        assert!(
            entry.issues.is_empty(),
            "层 {expanded} 不该有诊断：{:?}",
            entry.issues
        );
        // 每下一层就是 nested_fdf_bytes 里低一级的 Status。
        assert_eq!(
            child.fdf.status.as_deref(),
            Some(&format!("level{}", total - expanded)[..])
        );
        expanded += 1;
        current = nested;
    }
}

#[test]
fn embedded_fdfs_malformed_entry_is_skipped_without_failing_the_file() {
    // 畸形条目（这里是名字而非文件规范）：跳过并记录 + 保留原值，
    // 同数组里的正常条目照常类型化，整份 FDF 解析不失败。
    let inner = inner_fdf_bytes("mixed");
    let good = Dict::new()
        .insert("F", Value::String("good.fdf".to_string()))
        .insert("EF", carrier_stream(inner, Dict::new()));
    let document = with_embedded_fdfs(
        field_array(vec![name_entry("garbage"), Value::Dict(good)]),
        Vec::new(),
    );
    let view = embedded_of(&document);
    assert_eq!(view.len(), 2);
    assert_eq!(view.entries[0].filespec, None);
    assert_eq!(view.entries[0].raw, name_entry("garbage"));
    assert_eq!(view.entries[0].name(), None);
    assert!(view.entries[0].document.is_none());
    let joined = view.entries[0].issues.join("|");
    assert!(joined.contains("file specification"), "got {joined}");
    assert_eq!(view.entries[1].name(), Some("good.fdf"));
    assert!(view.entries[1].document.is_some());
}

#[test]
fn embedded_fdfs_entry_without_carrier_stream_is_reported_and_kept() {
    // 只有文件名没有 `/EF` 的文件规范（引用型/外部文件）合法存在：
    // 类型化到文件规范一层，递归不启动并记一条说明。
    let spec = Dict::new().insert("F", Value::String("external.fdf".to_string()));
    let document = with_embedded_fdfs(field_array(vec![Value::Dict(spec)]), Vec::new());
    let view = embedded_of(&document);
    let entry = first_entry(&view);
    assert_eq!(entry.name(), Some("external.fdf"));
    assert!(entry.carrier().is_none());
    assert!(entry.document.is_none());
    let joined = entry.issues.join("|");
    assert!(
        joined.contains("no /EF embedded file stream"),
        "got {joined}"
    );
}

#[test]
fn embedded_fdfs_entry_count_limit_skips_the_tail() {
    // 数组长度是不可信输入：超过库策略上限后停止展开并记诊断（不 panic、
    // 不预分配）。
    let items: Vec<Value> = (0..MAX_FDF_EMBEDDED_ENTRIES + 5)
        .map(|index| name_entry(&format!("garbage{index}")))
        .collect();
    let document = with_embedded_fdfs(field_array(items), Vec::new());
    let view = embedded_of(&document);
    assert_eq!(view.len(), MAX_FDF_EMBEDDED_ENTRIES);
    let joined = view.issues.join("|");
    assert!(joined.contains("tail was skipped"), "got {joined}");
}

#[test]
fn embedded_fdfs_non_array_form_is_tolerated_and_kept_raw() {
    // Table 243 声明类型是 array；字典形态读端宽容：条目为空、原值保留。
    let dict = Dict::new().insert("F", Value::String("nope.fdf".to_string()));
    let document = fdf_doc_with(Dict::new().insert("EmbeddedFDFs", Value::Dict(dict.clone())));
    let view = embedded_of(&document);
    assert!(view.is_empty());
    assert_eq!(view.raw, Some(Value::Dict(dict)));
    let joined = view.issues.join("|");
    assert!(joined.contains("declares an array"), "got {joined}");
}

#[test]
fn embedded_fdfs_dangling_array_reference_keeps_raw() {
    // 数组引用悬空：与 `/Differences` 同口径降级（诊断 + 原值）。
    let document = with_embedded_fdfs(Value::Ref(42, 0), Vec::new());
    let view = embedded_of(&document);
    assert!(view.is_empty());
    assert_eq!(view.raw, Some(Value::Ref(42, 0)));
    let joined = view.issues.join("|");
    assert!(joined.contains("could not be resolved"), "got {joined}");
}

// ── Table 244 /EncryptionRevision ───────────────────────────────────

fn encrypted_document(revision: Value) -> Document {
    let inner = inner_fdf_bytes("encrypted");
    let carrier = carrier_stream(inner, Dict::new().insert("EncryptionRevision", revision));
    let spec = Dict::new()
        .insert("F", Value::String("crypto.fdf".to_string()))
        .insert("EF", carrier);
    with_embedded_fdfs(field_array(vec![Value::Dict(spec)]), Vec::new())
}

#[test]
fn embedded_fdf_encryption_revision_one_without_password_is_typed_but_not_decrypted() {
    // Table 244：存在且为整数 → 类型化；旧入口不带口令 ⇒ 依旧不解密。
    let document = encrypted_document(Value::Integer(1));
    let view = embedded_of(&document);
    let entry = first_entry(&view);
    assert!(entry.is_encrypted());
    assert_eq!(entry.encryption_revision, Some(1));
    assert!(entry.encryption_revision_supported());
    assert!(!entry.is_decrypted(), "无口令入口不解密");
    assert!(entry.document.is_none(), "加密内容不得当作明文递归");
    let joined = entry.issues.join("|");
    assert!(joined.contains("no user password"), "got {joined}");
    assert!(joined.contains("undecrypted"), "got {joined}");
    assert!(
        !joined.contains("only defined value 1"),
        "值 1 不该报畸形：{joined}"
    );
}

#[test]
fn embedded_fdf_encryption_revision_other_than_one_is_reported() {
    // ≠1 是「可判定畸形」（规范只定义 1）：报诊断，且**不 Attempt 解密**。
    let document = encrypted_document(Value::Integer(2));
    let view = embedded_of(&document);
    let entry = first_entry(&view);
    assert_eq!(entry.encryption_revision, Some(2));
    assert!(!entry.encryption_revision_supported());
    let joined = entry.issues.join("|");
    assert!(joined.contains("only defined value 1"), "got {joined}");
    assert!(joined.contains("returned undecrypted"), "got {joined}");
}

#[test]
fn embedded_fdf_encryption_revision_non_integer_is_reported_and_kept() {
    // 类型不符（Table 244 声明 integer）：类型化为 None、记诊断，
    // 流字典里的原值不丢，且仍按明文递归。
    let document = encrypted_document(name_entry("two"));
    let view = embedded_of(&document);
    let entry = first_entry(&view);
    assert_eq!(entry.encryption_revision, None);
    assert!(!entry.is_encrypted());
    let joined = entry.issues.join("|");
    assert!(joined.contains("declares an integer"), "got {joined}");
    assert!(entry.document.is_some(), "未声明加密时递归照常");
}

#[test]
fn embedded_fdf_without_encryption_revision_is_unencrypted() {
    let document = fdf_doc_with(Dict::new());
    assert_eq!(document.fdf_embedded_fdfs().expect("ok"), None);
    let inner = inner_fdf_bytes("plain");
    let spec = Dict::new()
        .insert("F", Value::String("plain.fdf".to_string()))
        .insert("EF", carrier_stream(inner, Dict::new()));
    let document = with_embedded_fdfs(field_array(vec![Value::Dict(spec)]), Vec::new());
    let view = embedded_of(&document);
    let entry = first_entry(&view);
    assert!(!entry.is_encrypted());
    assert!(entry.encryption_revision_supported());
}

// ── PDF 1.4 版本语义（宽容读取 + 可报告）───────────────────────────

#[test]
fn pdf14_entries_are_read_and_reported_when_declared_version_is_lower() {
    // `/Differences` 与 `/EmbeddedFDFs` 都是 PDF 1.4 条目（Table 243 续表）；
    // FDF 目录 `/Version /1.3` 时仍照常类型化，只各记一条诊断。
    let inner = inner_fdf_bytes("inner");
    let spec = Dict::new()
        .insert("F", Value::String("old.fdf".to_string()))
        .insert("EF", carrier_stream(inner, Dict::new()));
    let entries = Dict::new()
        .insert(
            "Differences",
            carrier_stream(b"delta".to_vec(), Dict::new()),
        )
        .insert("EmbeddedFDFs", field_array(vec![Value::Dict(spec)]));
    let document = fdf_doc_with_catalog_version(entries, "1.3");
    let differences = document.fdf_differences().expect("ok").expect("视图存在");
    let embedded = embedded_of(&document);
    assert!(differences.is_stream(), "低版本声明不得阻止类型化");
    assert_eq!(embedded.len(), 1);
    assert!(
        differences
            .issues
            .iter()
            .any(|issue| issue.contains("/Differences is a PDF 1.4 entry")
                && issue.contains("1.3")),
        "got {:?}",
        differences.issues
    );
    assert!(
        embedded
            .issues
            .iter()
            .any(|issue| issue.contains("/EmbeddedFDFs is a PDF 1.4 entry")),
        "got {:?}",
        embedded.issues
    );
    // 递归子文档也带自己的版本报告：内层无 `/Version` → 用源文件版本
    // （构造式 Document 默认 1.7），故子条目无诊断。
    assert!(first_entry(&embedded).issues.is_empty());
}

#[test]
fn pdf14_entries_are_silent_when_declared_version_is_at_least_1_4() {
    let document = fdf_doc_with_catalog_version(
        Dict::new().insert(
            "Differences",
            carrier_stream(b"delta".to_vec(), Dict::new()),
        ),
        "1.4",
    );
    let differences = document.fdf_differences().expect("ok").expect("视图");
    assert!(
        differences.issues.is_empty(),
        "got {:?}",
        differences.issues
    );
}

#[test]
fn plain_pdf_yields_no_differences_or_embedded_views() {
    // 非 FDF 文件：两个类型化视图都安静地给出 None（与 `Document::fdf`
    // 的探测宽容一致）。
    let document = doc_from_objects(vec![Object::new(
        1,
        0,
        Value::Dict(Dict::new().insert("Type", name_entry("Catalog"))),
    )]);
    assert_eq!(document.fdf_differences().expect("ok"), None);
    assert_eq!(document.fdf_embedded_fdfs().expect("ok"), None);
}
