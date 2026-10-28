//! 增量更新写出（7.5.6）：保留源文件字节，仅追加变更对象与新交叉引用段。
//!
//! 入口为 [`crate::core::writer::DocumentWriter::build_incremental`]。语义：
//! - 宿主必须是从文件解析的 `Document`（源字节可回读、`xref`/`trailer`/
//!   `startxref` 反映最新修订）；
//! - 脏集 = 已被 `get_object`/`set_object` 触入缓存的对象；删除集 =
//!   [`Document::delete_object`] 标记的对象号，写出自由条目遮蔽历史；
//! - 变更对象的 generation 按原条目 +1（压缩对象重写出为未压缩 0 号代）；
//! - 目标版本 = 源版本（文件头不可改写）；≥1.5 追加只覆盖变更号的
//!   XRef 流段（`/Index` 分段），<1.5 追加合并历史条目的经典全表；
//! - trailer 合并：沿用最新 trailer 的非结构键（`/Root`/`/Info` 等），
//!   追加 `/Prev` 指向原 `startxref`；`/ID` 复用 E5 的 [`FileIdentifier`]
//!   ——永久 ID 不变、更新 ID 按链上前一更新 ID 与新段长度重新生成；
//! - 零字节契约：与全量写出相同，先缓冲、成功才写。

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::io::Write;

use crate::core::builder::FileIdentifier;
use crate::core::types::{
    Array, Dict, Document, Object, PdfFeature, PdfVersion, StreamData, Value, XrefKind,
};

use super::WriteError;
use super::compression::StreamCompression;
use super::context::{WriteContext, XrefEntryOut};
use super::serialize::{field_width, push_field};
use super::value::write_value;
use super::version::{validate_content_version, validate_output_version};

/// 源字节复制的分块大小。
const COPY_CHUNK: usize = 64 * 1024;

/// 增量写出一个修订。见模块文档。
pub(super) fn build(
    doc: &Document,
    w: &mut dyn Write,
    version: Option<PdfVersion>,
    compression: Option<StreamCompression>,
) -> Result<(), WriteError> {
    if doc.is_encrypted() {
        return Err(WriteError::EncryptedDocument);
    }
    if doc.source.is_none() {
        return Err(WriteError::InvalidObject(
            "incremental update requires a document parsed from a file".into(),
        ));
    }
    let target = version.unwrap_or(doc.source_version);
    if target != doc.source_version {
        return Err(WriteError::UnsupportedVersion(format!(
            "incremental update cannot change the header version: source is {}, target is {target}; use DocumentWriter::build for a full rewrite",
            doc.source_version
        )));
    }
    validate_output_version(target)?;
    validate_content_version(target, doc)?;
    if let Some(comp) = &compression {
        if target < PdfVersion::V1_2 {
            return Err(WriteError::UnsupportedVersion(format!(
                "flate stream compression requires PDF 1.2, got {target}"
            )));
        }
        if comp.uses_image_predictor() && target < PdfVersion::V1_3 {
            return Err(WriteError::UnsupportedVersion(format!(
                "PNG image predictor requires PDF 1.3, got {target}"
            )));
        }
    }

    // 1. 复制源字节（先缓冲，保证失败零字节）。
    let mut base: Vec<u8> = Vec::new();
    let mut read_off = 0u64;
    loop {
        let chunk = doc.read_at(read_off, COPY_CHUNK).ok_or_else(|| {
            WriteError::Io(std::io::Error::other(
                "incremental update: failed to read source file",
            ))
        })?;
        if chunk.is_empty() {
            break;
        }
        read_off += chunk.len() as u64;
        base.extend_from_slice(&chunk);
        if chunk.len() < COPY_CHUNK {
            break;
        }
    }
    // 空行分隔：源文件可能以无换行的 `%%EOF` 结尾。
    base.push(b'\n');

    // 2. 脏集（剔除历史结构对象）与删除集。
    let deleted = doc.deleted_objects();
    let changed: Vec<Object> = doc
        .objects_for_write()
        .into_iter()
        .filter(|o| !is_structural(o))
        .map(|o| Object {
            generation: revision_generation(doc, o.num, o.generation),
            ..o
        })
        .collect();
    for o in &changed {
        if o.num == 0 {
            return Err(WriteError::InvalidObject(
                "object 0 is the free-list head and cannot be written".into(),
            ));
        }
        if deleted.binary_search(&o.num).is_ok() {
            return Err(WriteError::InvalidObject(format!(
                "object {} is both modified and deleted",
                o.num
            )));
        }
    }

    let base_max = (doc.xref.max_size().max(doc.trailer.size().unwrap_or(0)) - 1).max(0);
    let mut ctx = WriteContext::for_incremental(target, compression, base, base_max);
    ctx.reserve_object_numbers(&changed);
    let refs: Vec<&Object> = changed.iter().collect();
    ctx.write_top_level_objects(doc, &refs)?;

    // 3. 删除号 → 自由条目。
    let mut free_rows: Vec<(i64, i64)> = Vec::new();
    for num in &deleted {
        let free_gen = match doc.xref.get(*num) {
            Some(entry) => match &entry.kind {
                XrefKind::Normal | XrefKind::Free => entry.generation + 1,
                // 压缩对象无 generation；取 1 保证遮蔽原流内条目。
                XrefKind::InObjStream { .. } => 1,
            },
            None => continue,
        };
        free_rows.push((*num, free_gen));
    }

    // 4. /ID 更新（复用 E5 API）：永久 ID 不变，更新 ID 沿链重算。
    let new_id: Option<Value> = doc.file_identifier().map(|(permanent, previous)| {
        let mut material = Vec::with_capacity(48);
        material.extend_from_slice(&permanent);
        material.extend_from_slice(&previous);
        material.extend_from_slice(&read_off.to_le_bytes());
        FileIdentifier::new(permanent, FileIdentifier::generate(&material).update).to_value()
    });

    // 5. 追加交叉引用段与尾部。
    if target.supports(PdfFeature::XRefStream) {
        let entries = ctx.entries.clone();
        write_xref_stream_section(&mut ctx, doc, &entries, &free_rows, new_id.as_ref())?;
    } else {
        write_classic_section(&mut ctx, doc, &free_rows, new_id.as_ref())?;
    }
    w.write_all(&ctx.buf)?;
    Ok(())
}

/// 历史遗留的结构对象：容器本身不进增量段，其内部对象由新条目逐个覆盖。
fn is_structural(obj: &Object) -> bool {
    matches!(
        obj.value
            .as_dict()
            .and_then(|d| d.get("Type"))
            .and_then(|v| v.as_name()),
        Some("ObjStm") | Some("XRef")
    )
}

/// 变更对象的新 generation：在用/自由条目在旧代上 +1；压缩对象重写为
/// 未压缩对象（0 代）；全新对象保持缓存自带代（通常 0）。
fn revision_generation(doc: &Document, num: i64, cached: i64) -> i64 {
    match doc.xref.get(num) {
        Some(entry) if matches!(entry.kind, XrefKind::Normal | XrefKind::Free) => {
            entry.generation.max(cached) + 1
        }
        _ => cached.max(0),
    }
}

/// trailer 合并：沿用最新 trailer 的非结构键，重算 `/Size`、追加 `/Prev`、
/// 更新 `/ID`（缺新 ID 时保留原值）。
fn merged_trailer_dict(doc: &Document, size: i64, prev: i64, new_id: Option<&Value>) -> Dict {
    let mut dict = Dict::new();
    for (key, value) in &doc.trailer.dict.0 {
        match key.as_str() {
            // 结构性/再生成条目：不透传，由下面重算。
            "Type" | "Size" | "W" | "Index" | "Length" | "Prev" | "Root" | "ID" => {}
            _ => {
                dict.0.insert(key.clone(), value.clone());
            }
        }
    }
    dict = dict
        .insert("Root", doc.trailer.root_value())
        .insert("Size", Value::Integer(size));
    if prev > 0 {
        dict = dict.insert("Prev", Value::Integer(prev));
    }
    if let Some(id) = new_id {
        dict = dict.insert("ID", id.clone());
    } else if let Some(id) = doc.trailer.dict.get("ID") {
        dict = dict.insert("ID", id.clone());
    }
    dict
}

/// 经典全表段（<1.5）：条目为「本段变更 → 本段删除 → 历史条目 → 空位」
/// 的合并结果，保证第三方阅读器按单段即可重建全表。
fn write_classic_section(
    ctx: &mut WriteContext,
    doc: &Document,
    free_rows: &[(i64, i64)],
    new_id: Option<&Value>,
) -> Result<(), WriteError> {
    let index: HashMap<i64, XrefEntryOut> = ctx.entry_index();
    let free_map: BTreeMap<i64, i64> = free_rows.iter().copied().collect();
    let size = doc
        .trailer
        .size()
        .unwrap_or(0)
        .max(doc.xref.max_size())
        .max(ctx.max_num + 1);
    let xref_offset = ctx.pos;

    let mut s = String::new();
    s.push_str("xref\n");
    let _ = writeln!(&mut s, "0 {size}");
    let _ = write!(&mut s, "0000000000 65535 f \r\n");
    for num in 1..size {
        if let Some(XrefEntryOut::Normal {
            generation, offset, ..
        }) = index.get(&num)
        {
            let _ = writeln!(&mut s, "{offset:010} {generation:05} n \r");
            continue;
        }
        if let Some(&free_gen) = free_map.get(&num) {
            let _ = writeln!(&mut s, "0000000000 {free_gen:05} f \r");
            continue;
        }
        match doc.xref.get(num) {
            Some(entry) if matches!(entry.kind, XrefKind::Normal) => {
                let _ = writeln!(&mut s, "{:010} {:05} n \r", entry.offset, entry.generation);
            }
            Some(entry) if matches!(entry.kind, XrefKind::Free) => {
                let _ = writeln!(&mut s, "{:010} {:05} f \r", entry.offset, entry.generation);
            }
            _ => {
                let _ = write!(&mut s, "0000000000 00000 f \r\n");
            }
        }
    }
    s.push_str("trailer\n");
    let dict = merged_trailer_dict(doc, size, doc.startxref.0, new_id);
    let mut ts = String::new();
    write_value(&Value::Dict(dict), &mut ts);
    s.push_str(&ts);
    s.push('\n');
    s.push_str("startxref\n");
    let _ = writeln!(&mut s, "{xref_offset}");
    s.push_str("%%EOF\n");
    ctx.buf.extend_from_slice(s.as_bytes());
    Ok(())
}

/// XRef 流段（≥1.5）：只写变更号（含删除自由条目与 XRef 流自身），
/// `/Index` 按连续号分段；未覆盖的对象号由读端沿 `/Prev` 链回退。
fn write_xref_stream_section(
    ctx: &mut WriteContext,
    doc: &Document,
    entries: &[XrefEntryOut],
    free_rows: &[(i64, i64)],
    new_id: Option<&Value>,
) -> Result<(), WriteError> {
    let xref_offset = ctx.pos;
    let xref_num = ctx.next_object_number();

    // 本段覆盖条目：num -> (type, f2, f3)。
    let mut covered: BTreeMap<i64, (u64, u64, u64)> = BTreeMap::new();
    for e in entries {
        if let XrefEntryOut::Normal {
            num,
            generation,
            offset,
        } = e
        {
            covered.insert(*num, (1, *offset, *generation as u64));
        }
    }
    for (num, free_gen) in free_rows {
        covered.insert(*num, (0, 0, *free_gen as u64));
    }
    covered.insert(xref_num, (1, xref_offset, 0));

    // 连续号归并为 /Index 段。
    let mut runs: Vec<(i64, usize)> = Vec::new();
    for &num in covered.keys() {
        match runs.last_mut() {
            Some((start, count)) if *start + *count as i64 == num => *count += 1,
            _ => runs.push((num, 1)),
        }
    }

    let w1 = 1usize;
    let w2 = field_width(covered.values().map(|r| r.1).max().unwrap_or(0), 4);
    let w3 = field_width(covered.values().map(|r| r.2).max().unwrap_or(0), 2);
    let mut stream: Vec<u8> = Vec::new();
    for (start, count) in &runs {
        for num in *start..*start + *count as i64 {
            let (t, f2, f3) = covered.get(&num).copied().unwrap_or((0, 0, 0));
            push_field(&mut stream, t, w1);
            push_field(&mut stream, f2, w2);
            push_field(&mut stream, f3, w3);
        }
    }

    let size = doc
        .trailer
        .size()
        .unwrap_or(0)
        .max(doc.xref.max_size())
        .max(xref_num + 1);
    let mut index_vals: Vec<i64> = Vec::new();
    for (start, count) in &runs {
        index_vals.push(*start);
        index_vals.push(*count as i64);
    }
    let ints = |vals: &[i64]| -> Value {
        Value::Array(
            vals.iter()
                .fold(Array::new(), |a, v| a.push(Value::Integer(*v))),
        )
    };
    let dict = merged_trailer_dict(doc, size, doc.startxref.0, new_id)
        .insert("Type", Value::Name("XRef".into()))
        .insert("W", ints(&[w1 as i64, w2 as i64, w3 as i64]))
        .insert("Index", ints(&index_vals))
        .insert("Length", Value::Integer(stream.len() as i64));
    let obj = Object::new(xref_num, 0, Value::Stream(dict, StreamData::Owned(stream)));
    ctx.write_raw_object(doc, &obj)?;

    let mut s = String::new();
    s.push_str("startxref\n");
    let _ = writeln!(&mut s, "{xref_offset}");
    s.push_str("%%EOF\n");
    ctx.buf.extend_from_slice(s.as_bytes());
    Ok(())
}

#[cfg(test)]
mod tests;
