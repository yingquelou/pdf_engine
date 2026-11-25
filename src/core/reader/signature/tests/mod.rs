//! 数字签名读取（12.8）单元测试。
//!
//! 按主题拆分：
//! - `model`：签名字典解析、`/SubFilter` 与 `/ByteRange` 结构错误；
//! - `digest`：ByteRange 覆盖与分段摘要；
//! - `references`：`/Reference` 变换参数（DocMDP/FieldMDP/UR）；
//! - `perms`：目录 `/Perms`（Table 268）；
//! - `cms`：`/Contents` 的 PKCS#7/CMS DER 解析；
//! - `integrity`：`messageDigest` 绑定校验与增量签名端到端；
//! - `der`：CMS 测试用 DER TLV 构造器。

use super::*;
use crate::core::types::{Dict, Document, Object, PdfVersion, StartXref, Trailer, Value, Xref};
use crate::core::writer::DocumentWriter;

mod cms;
mod der;
mod digest;
mod integrity;
mod model;
mod perms;
mod references;
mod verify;

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
