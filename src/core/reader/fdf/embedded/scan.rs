//! 嵌入式 FDF **字节层**的宽容扫描与子文档组装（12.7.7.2：FDF 文件的
//! 交叉引用表可选、对象只允许 generation 0；raw L24765–L24767）。
//!
//! `/EmbeddedFDFs` 的 `/EF` 载体流字节是一份**独立 FDF 文件**的字节，
//! 但没有 `File` 就无法走文件级 `Parser`，故这里用词法层 + 间接对象
//! 解析函数（`reader/object.rs`）在内存缓冲上直接扫出对象列表，再用
//! [`Document::from_objects`] 组装一份可交给
//! [`fdf_document`](super::super::parse::fdf_document) 解析入口的子
//! [`Document`]。全部输入按不可信处理：对象数上限、下标一律 checked、
//! 游标不前进即停止、任何语法断点只停止而不报错。

use crate::core::reader::lexer::Lexer;
use crate::core::reader::object::{parse_indirect_object, parse_trailer};
use crate::core::types::{
    Dict, Document, Object, PdfVersion, StartXref, StreamData, Trailer, Value, Xref,
};

/// 内层 FDF 字节可扫描出的间接对象上限（库策略，防护不可信输入）。
pub(crate) const MAX_FDF_EMBEDDED_OBJECTS: usize = 2048;
/// 内层 FDF 里可整体驻留内存的流字节上限；超过则保留引用不内联
/// （字典级类型化仍可用，字节读取不可用并记诊断）。
pub(crate) const MAX_FDF_EMBEDDED_INLINE_BYTES: usize = 8 * 1024 * 1024;

/// 一次内层 FDF 字节扫描的结果。
pub(crate) struct Scanned {
    pub(crate) objects: Vec<Object>,
    pub(crate) trailer: Option<Trailer>,
    /// 是否因对象数上限提前停止（不可信输入防护）。
    pub(crate) truncated: bool,
}

/// 宽容扫描内层 FDF 字节里的间接对象。`%FDF-x.y` 头与 `%%EOF` 都是注释，
/// 由词法层的注释规则自然跳过；遇到首个语法断点即停止（已收集的对象
/// 照常可用），读到 `xref`/`startxref` 亦停止。
pub(crate) fn scan_objects(bytes: &[u8]) -> Scanned {
    let mut objects: Vec<Object> = Vec::new();
    let mut trailer: Option<Trailer> = None;
    let mut truncated = false;
    let mut lex = Lexer::at(bytes, 0);
    loop {
        lex.skip_ws();
        if lex.peek().is_none() {
            break;
        }
        if lex.starts_with(b"trailer") {
            match parse_trailer(&mut lex) {
                Ok(parsed) => trailer = Some(parsed),
                Err(_) => break,
            }
            continue;
        }
        // 经典 xref 表之后只剩表项与尾部，对象已全部收集。
        if lex.starts_with(b"xref") || lex.starts_with(b"startxref") {
            break;
        }
        let before = lex.pos;
        match parse_indirect_object(&mut lex) {
            Ok(object) => objects.push(object),
            Err(_) => break,
        }
        // 前进保证：任何情况下都不允许游标不动的循环。
        if lex.pos <= before {
            break;
        }
        if objects.len() >= MAX_FDF_EMBEDDED_OBJECTS {
            truncated = true;
            break;
        }
    }
    if trailer.is_none() {
        trailer = trailer_from(bytes, lex.pos);
    }
    Scanned {
        objects,
        trailer,
        truncated,
    }
}

/// 在停止点之后寻找 `trailer` 关键字并解析（FDF 允许省略 xref，故这里
/// 只是尽力而为；找不到就按无 trailer 处理）。
fn trailer_from(bytes: &[u8], from: usize) -> Option<Trailer> {
    let tail = bytes.get(from..)?;
    let offset = find_keyword(tail, b"trailer")?;
    let start = from.checked_add(offset)?;
    let rest = bytes.get(start..)?;
    let mut lex = Lexer::at(rest, 0);
    parse_trailer(&mut lex).ok()
}

/// 字节序列里第一个 `keyword` 的起始下标（逐位置比较，不做窗口预分配）。
fn find_keyword(bytes: &[u8], keyword: &[u8]) -> Option<usize> {
    if keyword.is_empty() || bytes.len() < keyword.len() {
        return None;
    }
    (0..=bytes.len() - keyword.len()).find(|start| bytes[*start..].starts_with(keyword))
}

/// 用扫描到的对象组装一份可交给 FDF 根字典解析入口的子 [`Document`]。
///
/// `/Root` 取值优先序：trailer `/Root` 指向且含 `/FDF` 的对象 → 首个含
/// `/FDF` 的对象（Table 242）→ 无（调用方记诊断）。流对象的内层引用
/// （`StreamData::Ref`）在此按内存缓冲就地内联为 `Owned`，否则子文档
/// 无源文件、字节不可读；超过内联上限的保留原样并记诊断。
pub(crate) fn build_child_document(
    scanned: Scanned,
    bytes: &[u8],
    issues: &mut Vec<String>,
) -> Option<Document> {
    let root = pick_root(&scanned.objects, scanned.trailer.as_ref())?;
    let trailer = match scanned.trailer {
        Some(trailer) if trailer.root().is_some() => trailer,
        _ => Trailer {
            dict: Dict::new().insert("Root", Value::Ref(root, 0)),
        },
    };
    let objects = scanned
        .objects
        .into_iter()
        .map(|object| inline_stream(object, bytes, issues))
        .collect();
    Some(Document::from_objects(
        objects,
        Xref::new(),
        trailer,
        StartXref(0),
    ))
}

/// 挑选子 FDF 的目录对象号（Table 242：含 `/FDF` 条目的字典）。
fn pick_root(objects: &[Object], trailer: Option<&Trailer>) -> Option<i64> {
    if let Some((num, _)) = trailer.and_then(Trailer::root)
        && objects
            .iter()
            .any(|object| object.num == num && is_fdf_catalog(&object.value))
    {
        return Some(num);
    }
    objects
        .iter()
        .find(|object| is_fdf_catalog(&object.value))
        .map(|object| object.num)
}

fn is_fdf_catalog(value: &Value) -> bool {
    matches!(value, Value::Dict(dict) if dict.get("FDF").is_some())
}

/// `StreamData::Ref{offset,len}` → `StreamData::Owned`（下标全部
/// checked，越界/超上限即原样保留，绝不 panic）。
fn inline_stream(object: Object, bytes: &[u8], issues: &mut Vec<String>) -> Object {
    let Object {
        num,
        generation,
        value,
    } = object;
    let Value::Stream(dict, StreamData::Ref { offset, len }) = value else {
        return Object::new(num, generation, value);
    };
    // 三种「保留原样」出口共用同一个重建值。
    let keep = || {
        Object::new(
            num,
            generation,
            Value::Stream(dict.clone(), StreamData::Ref { offset, len }),
        )
    };
    let Some(start) = usize::try_from(offset).ok() else {
        return keep();
    };
    let Some(length) = usize::try_from(len).ok() else {
        return keep();
    };
    let Some(end) = start.checked_add(length).filter(|end| *end <= bytes.len()) else {
        issues.push(format!(
            "FDF embedded FDF object {num} stream range {start}+{length} lies outside the \
             carrier bytes; its dictionary is still typed"
        ));
        return keep();
    };
    if length > MAX_FDF_EMBEDDED_INLINE_BYTES {
        issues.push(format!(
            "FDF embedded FDF object {num} stream holds {length} bytes above the \
             {MAX_FDF_EMBEDDED_INLINE_BYTES} byte inline limit; bytes are not readable"
        ));
        return keep();
    }
    Object::new(
        num,
        generation,
        Value::Stream(dict, StreamData::Owned(bytes[start..end].to_vec())),
    )
}

/// 从源文件头部解析 `%FDF-x.y` 声明（普通 PDF 与无源文件的构造式
/// [`Document`] 返回 `None`）。
pub(crate) fn fdf_header_version(document: &Document) -> Option<PdfVersion> {
    let head = document.read_at(0, 512)?;
    let text = String::from_utf8_lossy(&head);
    let marker = text.find("%FDF-")?;
    PdfVersion::from_name(&text[marker + 5..])
}
