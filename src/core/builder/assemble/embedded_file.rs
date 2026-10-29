//! 嵌入文件写出（7.11.2–7.11.4）：文件规格 + 嵌入文件流 +
//! 目录 `/Names /EmbeddedFiles` 名称树。
//!
//! 名称树叶子节点复用导航层（E1）的 `name_tree` 构造：键按 PDF 文本串
//! 编码升序排列。导航 `apply` 写 `/Dests`、`/Pages` 时与已存在的
//! `/Names` 字典合并，两类条目可共存。
//!
//! 版本门禁（assemble 最前校验，零字节契约）：嵌入文件自 PDF 1.3 起；
//! 非 ASCII 展示名（`/UF`）与 `/Desc` 要求 1.4。1.4 及以上同时写出
//! `/F` 与 `/UF`。

use std::fmt::Write as _;

use crate::core::builder::navigation::name_tree;
use crate::core::builder::types::EmbeddedFile;
use crate::core::builder::types::text_string_value;
use crate::core::types::{Dict, Document, Object, PdfVersion, StreamData, Value};
use crate::core::writer::WriteError;

/// 单个注册项的对象号：文件规格在前、嵌入流在后。
pub(super) struct EmbeddedSlot {
    pub(super) filespec_num: i64,
    pub(super) stream_num: i64,
}

/// 注册校验：逐项合法性、展示名重复与版本门禁。在分配任何对象号之前
/// 调用，非法输入保持零字节输出契约。
pub(super) fn validate_registration(
    files: &[EmbeddedFile],
    version: PdfVersion,
) -> Result<(), WriteError> {
    if files.is_empty() {
        return Ok(());
    }
    if version < PdfVersion::V1_3 {
        return Err(WriteError::UnsupportedVersion(format!(
            "embedded files require PDF {}, got {}",
            PdfVersion::V1_3.as_str(),
            version.as_str()
        )));
    }
    let mut seen = Vec::with_capacity(files.len());
    for file in files {
        file.validate()
            .map_err(|message| WriteError::InvalidObject(format!("embedded file: {message}")))?;
        if seen.contains(&file.name()) {
            return Err(WriteError::InvalidObject(format!(
                "duplicate embedded file name {:?}",
                file.name()
            )));
        }
        seen.push(file.name());
        let uses_1_4 = !file.name_is_ascii() || file.description().is_some();
        if uses_1_4 && version < PdfVersion::V1_4 {
            return Err(WriteError::UnsupportedVersion(format!(
                "unicode file names (/UF) and descriptions (/Desc) require PDF {}, got {}",
                PdfVersion::V1_4.as_str(),
                version.as_str()
            )));
        }
    }
    Ok(())
}

/// 为每个注册项分配（文件规格，嵌入流）两个对象号。
pub(super) fn allocate(files: &[EmbeddedFile], next: &mut i64) -> Vec<EmbeddedSlot> {
    files
        .iter()
        .map(|_| {
            let slot = EmbeddedSlot {
                filespec_num: *next,
                stream_num: *next + 1,
            };
            *next += 2;
            slot
        })
        .collect()
}

/// 构造目录 `/Names /EmbeddedFiles` 节点（单层叶子名称树，键为展示名）。
pub(super) fn embedded_files_node(files: &[EmbeddedFile], slots: &[EmbeddedSlot]) -> Value {
    let entries = files
        .iter()
        .zip(slots)
        .map(|(file, slot)| (file.name().to_string(), Value::Ref(slot.filespec_num, 0)))
        .collect();
    name_tree(entries)
}

/// 写出文件规格字典与嵌入文件流；`/EF` 按 Table 44 写载体字典。
pub(super) fn write_objects(
    doc: &mut Document,
    files: &[EmbeddedFile],
    slots: &[EmbeddedSlot],
    version: PdfVersion,
) {
    for (file, slot) in files.iter().zip(slots) {
        let mut filespec = Dict::new().insert("Type", Value::Name("Filespec".into()));
        if let Some(short) = file.short_name() {
            filespec = filespec.insert("F", text_string_value(short));
        }
        if version >= PdfVersion::V1_4 {
            filespec = filespec.insert("UF", text_string_value(file.name()));
        }
        if let Some(description) = file.description() {
            filespec = filespec.insert("Desc", text_string_value(description));
        }
        let mut carriers = Dict::new();
        if file.short_name().is_some() {
            carriers = carriers.insert("F", Value::Ref(slot.stream_num, 0));
        }
        if version >= PdfVersion::V1_4 {
            carriers = carriers.insert("UF", Value::Ref(slot.stream_num, 0));
        }
        filespec = filespec.insert("EF", Value::Dict(carriers));
        doc.set_object(Object::new(slot.filespec_num, 0, Value::Dict(filespec)));

        let mut dict = Dict::new().insert("Type", Value::Name("EmbeddedFile".into()));
        if let Some(subtype) = file.subtype() {
            dict = dict.insert("Subtype", Value::Name(subtype.into()));
        }
        let mut params = Dict::new()
            .insert("Size", Value::Integer(file.data().len() as i64))
            .insert("CheckSum", Value::HexString(hex_encode(&file.checksum())));
        if let Some(date) = file.creation_date() {
            params = params.insert("CreationDate", text_string_value(date));
        }
        if let Some(date) = file.modification_date() {
            params = params.insert("ModDate", text_string_value(date));
        }
        dict = dict.insert("Params", Value::Dict(params));
        let data = file.data().to_vec();
        let len = data.len();
        doc.set_object(Object::new(
            slot.stream_num,
            0,
            Value::Stream(
                dict.insert("Length", Value::Integer(len as i64)),
                StreamData::Owned(data),
            ),
        ));
    }
}

/// 16 字节校验值 → 大写十六进制串（`/CheckSum` 为任意字节串，
/// 与 trailer `/ID` 同一写出方式）。
fn hex_encode(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(hex, "{byte:02X}");
    }
    hex
}
