//! FDF `/Differences`（Table 243 续表 raw L24906–L24926）与
//! `/EmbeddedFDFs`（raw L24930–L24932）+ Table 244 `/EncryptionRevision`
//! 的集成往返：手写字节（12.7.7.2 的 `%FDF-x.y` 头 + 对象体 + 经典 xref +
//! trailer）→ **真实 Parser** → 类型化只读视图。内层嵌入式 FDF 的字节
//! 一处用手写最小文件、一处用本 crate 写端 `PdfBuilder::export_fdf` 的
//! **真实产出**，证明递归子文档解析在真实字节上成立。

use super::*;
use pdf_engine::core::builder::PdfBuilder;
use pdf_engine::core::reader::fdf::{FdfDictionary, FdfDocument, FdfField};

/// 全 None 裸字段（Table 246 读模型；写端不消费全限定名）。
fn bare_field(partial: &str) -> FdfField {
    FdfField {
        partial_name: partial.into(),
        fully_qualified_name: String::new(),
        value: None,
        field_flags: None,
        set_field_flags: None,
        clear_field_flags: None,
        widget_flags: None,
        set_widget_flags: None,
        clear_widget_flags: None,
        options: None,
        appearance: None,
        appearance_reference: None,
        icon_fit: None,
        action: None,
        additional_actions: None,
        rich_value: None,
        kids: Vec::new(),
        dict: Dict::new(),
    }
}

/// 全 None 裸 FDF 字典（Table 243 读模型）。
fn bare_dictionary() -> FdfDictionary {
    FdfDictionary {
        source_file: None,
        identifier: None,
        fields: None,
        status: None,
        pages: None,
        encoding: None,
        annots: None,
        differences: None,
        target: None,
        target_frame: None,
        embedded_fdfs: None,
        javascript: None,
        javascript_scripts: None,
        dict: Dict::new(),
    }
}

fn model(fdf: &FdfDictionary) -> FdfDocument {
    FdfDocument {
        catalog_version: None,
        catalog: Dict::new(),
        fdf: fdf.clone(),
    }
}

/// FDF 文件拼装器：登记每个间接对象的偏移，收尾时产经典 xref 表。
struct FdfFile {
    buf: Vec<u8>,
    offsets: Vec<(i64, usize)>,
}

impl FdfFile {
    /// `header` 为文件头声明（`%FDF-1.4\n` 之类，按注释被词法层跳过）。
    fn new(header: &str) -> Self {
        let mut buf = Vec::new();
        buf.extend_from_slice(header.as_bytes());
        FdfFile {
            buf,
            offsets: Vec::new(),
        }
    }

    /// 写一个 generation 0 间接对象（FDF 只允许 gen 0，12.7.7.2）。
    fn object(&mut self, num: i64, body: &[u8]) -> &mut Self {
        self.offsets.push((num, self.buf.len()));
        self.buf
            .extend_from_slice(format!("{num} 0 obj\n").as_bytes());
        self.buf.extend_from_slice(body);
        self.buf.extend_from_slice(b"\nendobj\n");
        self
    }

    /// 收尾：`root` 写进 trailer 的 `/Root`，未使用的对象号登记为自由项。
    fn finish(mut self, root: i64) -> Vec<u8> {
        let top = self
            .offsets
            .iter()
            .map(|(num, _)| *num)
            .max()
            .unwrap_or(root)
            .max(root);
        let size = top + 1;
        let xref_at = self.buf.len();
        self.buf
            .extend_from_slice(format!("xref\n0 {size}\n").as_bytes());
        self.buf.extend_from_slice(b"0000000000 65535 f \n");
        for num in 1..size {
            match self.offsets.iter().find(|(found, _)| *found == num) {
                Some((_, offset)) => self
                    .buf
                    .extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes()),
                None => self.buf.extend_from_slice(b"0000000000 65535 f \n"),
            }
        }
        self.buf.extend_from_slice(
            format!("trailer\n<< /Root {root} 0 R /Size {size} >>\nstartxref\n{xref_at}\n%%EOF\n")
                .as_bytes(),
        );
        self.buf
    }
}

/// 流对象体：`extra` 是流字典的附加条目文本（`/Filter`、
/// `/EncryptionRevision` 等），`/Length` 按给定字节数如实声明。
fn stream_body(bytes: &[u8], extra: &str) -> Vec<u8> {
    let mut body = format!("<< /Length {} {extra} >>\nstream\n", bytes.len()).into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\nendstream");
    body
}

/// 手写最小内层 FDF（无 xref，12.7.7.2 允许省略）：`/Status` + 一个字段。
fn inner_fdf_bytes(status: &str, name: &str, value: &str) -> Vec<u8> {
    format!(
        "1 0 obj\n<< /FDF << /Status ({status}) /Fields [ << /T ({name}) /V ({value}) >> ] >> >>\nendobj\n%%EOF\n"
    )
    .into_bytes()
}

/// 本 crate **写端**真实产出的内层 FDF 字节（供嵌入后递归回读）。
fn writer_inner_fdf_bytes(status: &str, name: &str, value: &str) -> Vec<u8> {
    let mut field = bare_field(name);
    field.value = Some(Value::String(value.to_string()));
    let mut fdf = bare_dictionary();
    fdf.status = Some(status.to_string());
    fdf.fields = Some(vec![field]);
    PdfBuilder::new()
        .export_fdf(&model(&fdf), PdfVersion::V1_4)
        .expect("内层 FDF 导出应成功")
}

/// 一个文件规范对象体：`/F (name)` + `/EF ef 0 R`。
fn filespec_body(name: &str, ef: i64) -> String {
    format!("<< /F ({name}) /EF {ef} 0 R >>")
}

/// 一份带 `/Differences`（引用形态）与 `/EmbeddedFDFs` 的外层 FDF 文件。
fn outer_with_differences(header: &str, entries: &str, extra: &[(i64, Vec<u8>)]) -> Vec<u8> {
    let mut file = FdfFile::new(header);
    file.object(1, b"<< /FDF 2 0 R >>");
    file.object(2, format!("<< {entries} >>").as_bytes());
    for (num, body) in extra {
        file.object(*num, body);
    }
    file.finish(1)
}

#[test]
fn differences_stream_and_form_mismatch_read_from_a_real_fdf_file() {
    // 真实文件里流只能是以间接对象形态存在（7.3.7），故 `/Differences`
    // 的实际形态是 `N 0 R`：解引用后确证是流，流字典条目（/Length、
    // /Filter、未建模条目）原值保留，字节走既有过滤器入口解压。
    let hex = b"48656C6C6F>";
    let document = parse_buf(&outer_with_differences(
        "%FDF-1.4\n",
        "/F (tax-return.pdf) /Status (Imported OK) /Differences 5 0 R",
        &[(5, stream_body(hex, "/Filter /ASCIIHexDecode"))],
    ));
    let fdf = document
        .fdf()
        .expect("解析不应报错")
        .expect("应被识别为 FDF");
    assert_eq!(
        fdf.fdf.differences,
        Some(Value::Ref(5, 0)),
        "原值仍是引用形态"
    );
    let view = document
        .fdf_differences()
        .expect("读端不应报错")
        .expect("条目存在时应类型化");
    assert_eq!(view.reference, Some((5, 0)));
    assert!(view.is_stream(), "引用应被解引用到流对象");
    assert_eq!(view.declared_length(), Some(hex.len() as i64));
    assert_eq!(
        view.entry("Filter"),
        Some(&Value::Name("ASCIIHexDecode".into()))
    );
    assert!(
        view.issues.is_empty(),
        "1.4 文件不该记诊断：{:?}",
        view.issues
    );
    assert_eq!(
        view.decoded_bytes(&document).expect("过滤器链照常应用"),
        b"Hello".to_vec()
    );

    // 解引用后的流对象：未建模的流字典条目原样可取，无过滤器时字节即原值。
    let document = parse_buf(&outer_with_differences(
        "%FDF-1.4\n",
        "/Status (ok) /Differences 5 0 R",
        &[(5, stream_body(b"append", "/Custom (kept)"))],
    ));
    let view = document
        .fdf_differences()
        .expect("ok")
        .expect("解引用后的流应类型化");
    assert!(view.is_stream());
    assert_eq!(view.reference, Some((5, 0)));
    assert_eq!(
        view.entry("Custom"),
        Some(&Value::String("kept".into())),
        "未建模流字典条目原样可取"
    );
    assert_eq!(
        view.decoded_bytes(&document).expect("无过滤器即原字节"),
        b"append".to_vec()
    );
    // 非流形态（Table 243 声明 stream，实际写成名字）：读端宽容，只记诊断。
    let document = parse_buf(&outer_with_differences(
        "%FDF-1.4\n",
        "/Status (ok) /Differences /not-a-stream",
        &[],
    ));
    let view = document.fdf_differences().expect("ok").expect("视图");
    assert!(!view.is_stream());
    assert_eq!(view.raw, Value::Name("not-a-stream".into()));
    assert!(
        view.issues
            .iter()
            .any(|issue| issue.contains("declares a stream")),
        "got {:?}",
        view.issues
    );
}

#[test]
fn differences_dangling_reference_degrades_without_failing_the_real_file() {
    // 悬空引用（对象号未登记 / 与本 crate 写端的 xref 流对象号相撞）：
    // 不 panic、不报错，降级为诊断 + 原值保留，同文件其余条目照常类型化。
    let document = parse_buf(&outer_with_differences(
        "%FDF-1.4\n",
        "/Status (Imported OK) /Differences 42 0 R",
        &[],
    ));
    let view = document
        .fdf_differences()
        .expect("读端宽容不应报错")
        .expect("条目存在时应类型化");
    assert!(!view.is_stream());
    assert_eq!(view.raw, Value::Ref(42, 0));
    assert!(
        view.issues
            .iter()
            .any(|issue| issue.contains("could not be resolved")),
        "got {:?}",
        view.issues
    );
    // 同文件的其余读模型不受影响。
    let fdf = document.fdf().expect("ok").expect("FDF");
    assert_eq!(fdf.fdf.status.as_deref(), Some("Imported OK"));
}

#[test]
fn embedded_fdfs_recurses_into_writer_produced_inner_fdf() {
    // 往返校准：内层嵌入式 FDF 的字节由本 crate 写端 export_fdf 真实产出，
    // 外层手写字节 → 真实 Parser → `/EmbeddedFDFs` 逐项按 7.11 类型化，
    // `/EF` 载体流递归出子文档视图（Table 242/243/246 照常建模）。
    let inner = writer_inner_fdf_bytes("from writer", "name", "Ada");
    let mut file = FdfFile::new("%FDF-1.4\n");
    file.object(1, b"<< /FDF 2 0 R >>");
    file.object(2, b"<< /Status (outer) /EmbeddedFDFs [ 3 0 R ] >>");
    file.object(3, filespec_body("inner.fdf", 4).as_bytes());
    file.object(4, &stream_body(&inner, "/Type /EmbeddedFile"));
    let document = parse_buf(&file.finish(1));
    let view = document
        .fdf_embedded_fdfs()
        .expect("读端不应报错")
        .expect("条目存在时应类型化");
    assert_eq!(view.len(), 1);
    assert!(view.issues.is_empty(), "got {:?}", view.issues);
    let entry = view.entries.first().expect("一个条目");
    assert_eq!(entry.name(), Some("inner.fdf"));
    assert!(!entry.is_encrypted(), "未声明 /EncryptionRevision");
    assert!(entry.carrier().is_some(), "/EF 载体流应被识别");
    assert_eq!(entry.data().expect("载体字节").len(), inner.len());
    let child = entry
        .document
        .as_ref()
        .expect("写端产出的内层 FDF 应被递归解析");
    assert_eq!(child.fdf.status.as_deref(), Some("from writer"));
    let field = child.field("name").expect("子文档字段按全限定名可查");
    assert_eq!(field.value, Some(Value::String("Ada".into())));
    assert!(entry.issues.is_empty(), "got {:?}", entry.issues);
    // 便捷按名取子文档。
    assert!(view.document("inner.fdf").is_some());
    assert!(view.document("absent.fdf").is_none());
}

#[test]
fn embedded_fdfs_nested_levels_and_cycle_read_from_a_real_fdf_file() {
    // 两层自嵌套（子文档的 `/EmbeddedFDFs` 指向孙层载体）：逐层递归；
    // 同一数组里重复引用同一文件规范对象 → 环检测，第二项不再展开。
    let grandchild = inner_fdf_bytes("leaf", "deep", "1");
    let child = {
        let mut file = FdfFile::new("%FDF-1.4\n");
        file.object(1, b"<< /FDF 2 0 R >>");
        file.object(2, b"<< /Status (child) /EmbeddedFDFs [ 3 0 R ] >>");
        file.object(3, filespec_body("grandchild.fdf", 4).as_bytes());
        file.object(4, &stream_body(&grandchild, "/Type /EmbeddedFile"));
        file.finish(1)
    };
    let mut file = FdfFile::new("%FDF-1.4\n");
    file.object(1, b"<< /FDF 2 0 R >>");
    file.object(2, b"<< /Status (outer) /EmbeddedFDFs [ 3 0 R 3 0 R ] >>");
    file.object(3, filespec_body("child.fdf", 4).as_bytes());
    file.object(4, &stream_body(&child, "/Type /EmbeddedFile"));
    let document = parse_buf(&file.finish(1));
    let view = document.fdf_embedded_fdfs().expect("ok").expect("视图");
    assert_eq!(view.len(), 2);
    let first = view.entries.first().expect("首项");
    let child_doc = first.document.as_ref().expect("子文档应被递归解析");
    assert_eq!(child_doc.fdf.status.as_deref(), Some("child"));
    let nested = first.nested.as_ref().expect("孙层视图");
    assert_eq!(nested.len(), 1);
    let grand = nested.entries[0]
        .document
        .as_ref()
        .expect("孙文档应被递归解析");
    assert_eq!(grand.fdf.status.as_deref(), Some("leaf"));
    assert_eq!(
        grand.field("deep").expect("孙字段").value,
        Some(Value::String("1".into()))
    );
    // 环路项：类型化保留原值，但不再向下展开。
    let second = &view.entries[1];
    assert_eq!(second.name(), Some("child.fdf"));
    assert!(second.document.is_none(), "重复对象号不该再展开");
    assert!(
        second
            .issues
            .iter()
            .any(|issue| issue.contains("already visited")),
        "got {:?}",
        second.issues
    );
}

#[test]
fn embedded_fdfs_encrypted_carrier_is_typed_but_never_decrypted() {
    // Table 244：`/EncryptionRevision` 存在即识别（本切片不解密）；
    // 值 ≠ 1 额外记「唯一被定义的值是 1」的诊断，加密项一律不产子文档。
    let clear = inner_fdf_bytes("plain", "k", "v");
    let mut file = FdfFile::new("%FDF-1.4\n");
    file.object(1, b"<< /FDF 2 0 R >>");
    file.object(2, b"<< /Status (outer) /EmbeddedFDFs [ 3 0 R 5 0 R ] >>");
    file.object(3, filespec_body("rev-one.fdf", 4).as_bytes());
    file.object(
        4,
        &stream_body(&clear, "/Type /EmbeddedFile /EncryptionRevision 1"),
    );
    file.object(5, filespec_body("rev-three.fdf", 6).as_bytes());
    file.object(
        6,
        &stream_body(&clear, "/Type /EmbeddedFile /EncryptionRevision 3"),
    );
    let document = parse_buf(&file.finish(1));
    let view = document.fdf_embedded_fdfs().expect("ok").expect("视图");
    assert_eq!(view.len(), 2);
    let one = &view.entries[0];
    assert_eq!(one.encryption_revision, Some(1));
    assert!(one.is_encrypted());
    assert!(one.encryption_revision_supported());
    assert!(one.document.is_none(), "加密流不解密故无子文档");
    let joined = one.issues.join("|");
    assert!(
        joined.contains("decryption is not implemented"),
        "got {joined}"
    );
    assert!(
        !joined.contains("not the only defined value"),
        "got {joined}"
    );
    // 字节仍可原值取回（未做 FDF 侧解扰）。
    assert_eq!(one.data().expect("载体字节"), clear.as_slice());
    let three = &view.entries[1];
    assert_eq!(three.encryption_revision, Some(3));
    assert!(!three.encryption_revision_supported());
    let joined = three.issues.join("|");
    assert!(
        joined.contains("not the only defined value 1"),
        "got {joined}"
    );
    assert!(
        joined.contains("Table 244 raw L24937-L24939"),
        "got {joined}"
    );
}

#[test]
fn pdf14_entries_below_declared_version_are_reported_in_a_real_file() {
    // 版本语义：`%FDF-1.2` 声明下 PDF 1.4 条目仍**读宽容**，只各记一条
    // 诊断（不新增 PdfFeature 变体，直接比较 PdfVersion 常量）。
    let inner = inner_fdf_bytes("kid", "k", "v");
    let mut file = FdfFile::new("%FDF-1.2\n");
    file.object(1, b"<< /FDF 2 0 R >>");
    file.object(
        2,
        b"<< /Status (ok) /Differences 3 0 R /EmbeddedFDFs [ 4 0 R ] >>",
    );
    file.object(3, &stream_body(b"delta", ""));
    file.object(4, filespec_body("kid.fdf", 5).as_bytes());
    file.object(5, &stream_body(&inner, "/Type /EmbeddedFile"));
    let document = parse_buf(&file.finish(1));
    let differences = document.fdf_differences().expect("ok").expect("视图");
    assert!(differences.is_stream(), "低版本声明不该拒绝读取");
    let joined = differences.issues.join("|");
    assert!(joined.contains("PDF 1.4 entry"), "got {joined}");
    assert!(joined.contains("declares 1.2"), "got {joined}");
    let embedded = document.fdf_embedded_fdfs().expect("ok").expect("视图");
    assert_eq!(embedded.len(), 1);
    assert!(
        embedded
            .entries
            .first()
            .expect("条目")
            .document
            .as_ref()
            .expect("子文档")
            .fdf
            .status
            .as_deref()
            == Some("kid")
    );
    let joined = embedded.issues.join("|");
    assert!(joined.contains("PDF 1.4 entry"), "got {joined}");

    // 对照：目录 `/Version`（Table 242）声明 1.4 时安静无诊断。
    let mut file = FdfFile::new("%FDF-1.2\n");
    file.object(1, b"<< /FDF 2 0 R /Version /1.4 >>");
    file.object(2, b"<< /Status (ok) /Differences 3 0 R >>");
    file.object(3, &stream_body(b"delta", ""));
    let document = parse_buf(&file.finish(1));
    let differences = document.fdf_differences().expect("ok").expect("视图");
    assert!(
        differences.issues.is_empty(),
        "目录 /Version 优先于文件头：{:?}",
        differences.issues
    );
}

#[test]
fn plain_pdf_yields_no_fdf_entry_views() {
    // 非 FDF 文件：两个视图都安静 None，不产诊断。
    let document = sample_doc();
    assert_eq!(document.fdf_differences().expect("ok"), None);
    assert_eq!(document.fdf_embedded_fdfs().expect("ok"), None);
}

#[test]
fn embedded_child_document_differences_is_typed_from_real_bytes() {
    // 递归子文档的 `/Differences` 在装配时就地类型化：子 `Document` 不落进
    // 视图，[`FdfDocument`] 里的原值只是一个调用方无法解引用的对象号，
    // 故必须由 `FdfEmbeddedFdf::differences` 给出确证后的流。
    let hex = b"64656C7461>";
    // 内层 FDF（12.7.7.2：交叉引用表可选）：目录声明 `/Differences 2 0 R`，
    // 2 号对象是 ASCIIHex 编码的增量更新流。
    let inner = format!(
        "1 0 obj\n<< /FDF << /Status (kid) /Differences 2 0 R >> >>\nendobj\n2 0 obj\n<< /Length {} /Filter /ASCIIHexDecode /Custom (kept) >>\nstream\n{}\nendstream\nendobj\n%%EOF\n",
        hex.len(),
        String::from_utf8_lossy(hex),
    );
    let mut file = FdfFile::new("%FDF-1.4\n");
    file.object(1, b"<< /FDF 2 0 R >>");
    file.object(2, b"<< /Status (outer) /EmbeddedFDFs [ 3 0 R ] >>");
    file.object(3, filespec_body("kid.fdf", 4).as_bytes());
    file.object(4, &stream_body(&inner.into_bytes(), "/Type /EmbeddedFile"));
    let document = parse_buf(&file.finish(1));
    let view = document
        .fdf_embedded_fdfs()
        .expect("读端不应报错")
        .expect("条目存在时应类型化");
    let entry = view.entries.first().expect("一个条目");
    let child = entry.document.as_ref().expect("子文档应被递归解析");
    assert_eq!(child.fdf.status.as_deref(), Some("kid"));
    assert_eq!(
        child.fdf.differences,
        Some(Value::Ref(2, 0)),
        "子模型里的原值不丢，但调用方没有子 Document 可解引用"
    );
    let differences = entry
        .differences
        .as_ref()
        .expect("子文档 /Differences 应就地类型化");
    assert!(differences.is_stream(), "内层引用应被解引用到内层流对象");
    assert_eq!(differences.reference, Some((2, 0)));
    assert_eq!(differences.raw, Value::Ref(2, 0));
    assert_eq!(differences.declared_length(), Some(hex.len() as i64));
    assert_eq!(
        differences.entry("Custom"),
        Some(&Value::String("kept".into())),
        "内层流字典的未建模条目原值保留"
    );
    assert!(
        differences.issues.is_empty(),
        "真实字节路径不该有降级诊断：{:?}",
        differences.issues
    );
    // 内层流字节随载体解码内联，过滤链入口照常可用（以调用方手里的外层
    // Document 为上下文）。
    assert_eq!(
        differences
            .decoded_bytes(&document)
            .expect("ASCIIHex 照常解压"),
        b"delta".to_vec()
    );
    // 外层自己没有 `/Differences`。
    assert_eq!(document.fdf_differences().expect("ok"), None);
}
