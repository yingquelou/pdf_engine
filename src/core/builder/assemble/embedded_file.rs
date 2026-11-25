//! 嵌入文件写出（7.11.2–7.11.4）：文件规格 + 嵌入文件流 +
//! 目录 `/Names /EmbeddedFiles` 名称树。
//!
//! 名称树叶子节点复用导航层（E1）的 `name_tree` 构造：键按 PDF 文本串
//! 编码升序排列。导航 `apply` 写 `/Dests`、`/Pages` 时与已存在的
//! `/Names` 字典合并，两类条目可共存。
//!
//! 版本门禁（assemble 最前校验，零字节契约）：嵌入文件自 PDF 1.3 起；
//! 非 ASCII 展示名（`/UF`）与 `/Desc` 要求 1.4。1.4 及以上同时写出
//! `/F` 与 `/UF`。文件规格 `/CI` 集合项要求 PDF 1.7，且按 Table 44
//! 写成独立间接对象。

use std::fmt::Write as _;

use crate::core::builder::navigation::{Collection, CollectionFieldType, name_tree};
use crate::core::builder::types::EmbeddedFile;
use crate::core::builder::types::text_string_value;
use crate::core::builder::types::{CollectionItemValue, CollectionSubitemValue};
use crate::core::types::{Dict, Document, Object, PdfFeature, PdfVersion, StreamData, Value};
use crate::core::writer::WriteError;

/// 单个注册项的对象号：文件规格在前、嵌入流在后；有 `/CI` 时再占一个。
pub(super) struct EmbeddedSlot {
    pub(super) filespec_num: i64,
    pub(super) stream_num: i64,
    pub(super) collection_item_num: Option<i64>,
    /// 每条 `/RF` 关联文件的嵌入流对象号（按声明顺序）。
    pub(super) related_file_nums: Vec<i64>,
}

/// 本文件规格实际写出的 `/EF` 载体键；`/RF` 的键必须是其子集。
fn ef_carriers(file: &EmbeddedFile, version: PdfVersion) -> Vec<&'static str> {
    let mut carriers = Vec::new();
    if file.short_name().is_some() {
        carriers.push("F");
    }
    if version >= PdfVersion::V1_7 {
        carriers.push("UF");
    }
    carriers
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
        if file.description().is_some() && version < PdfVersion::V1_6 {
            return Err(WriteError::UnsupportedVersion(format!(
                "file specification descriptions (/Desc) require PDF {}, got {}",
                PdfVersion::V1_6.as_str(),
                version.as_str()
            )));
        }
        if !file.name_is_ascii() && version < PdfVersion::V1_7 {
            return Err(WriteError::UnsupportedVersion(format!(
                "unicode file names (/UF) require PDF {}, got {}",
                PdfVersion::V1_7.as_str(),
                version.as_str()
            )));
        }
        if file.collection_item().is_some() && version < PdfVersion::V1_7 {
            return Err(WriteError::UnsupportedVersion(format!(
                "collection items (/CI) require PDF {}, got {}",
                PdfVersion::V1_7.as_str(),
                version.as_str()
            )));
        }
        let carriers = ef_carriers(file, version);
        for related in file.related_files() {
            if !carriers.contains(&related.carrier()) {
                return Err(WriteError::InvalidObject(format!(
                    "related file carrier {:?} is absent from /EF of {:?}",
                    related.carrier(),
                    file.name()
                )));
            }
        }
    }
    Ok(())
}

/// 集合交叉校验（12.3.5 Table 155 / 7.11.6 Table 48/49）：`/D` 必须
/// 指向 EmbeddedFiles 名称树中的展示名；`/CI` 键必须在集合模式中声明，
/// 值类型须与字段 `/Subtype` 一致。在分配任何对象号之前调用，非法输入
/// 保持零字节输出契约。
pub(super) fn validate_collection_references(
    files: &[EmbeddedFile],
    collection: Option<&Collection>,
) -> Result<(), WriteError> {
    let Some(collection) = collection else {
        if let Some(file) = files.iter().find(|file| file.collection_item().is_some()) {
            return Err(WriteError::InvalidObject(format!(
                "embedded file {:?} has a collection item but no collection is set",
                file.name()
            )));
        }
        return Ok(());
    };
    if let Some(name) = &collection.initial_document
        && !files.iter().any(|file| file.name() == name)
    {
        return Err(WriteError::InvalidObject(format!(
            "collection initial document {name:?} is not an embedded file name"
        )));
    }
    if !files.iter().any(|file| file.collection_item().is_some()) {
        return Ok(());
    }
    let Some(schema) = collection.schema.as_deref() else {
        return Err(WriteError::InvalidObject(
            "collection items require a collection schema".into(),
        ));
    };
    for file in files {
        let Some(item) = file.collection_item() else {
            continue;
        };
        for (key, value) in item.values() {
            let Some(field) = schema.iter().find(|field| &field.key == key) else {
                return Err(WriteError::InvalidObject(format!(
                    "collection item field {key:?} of embedded file {:?} is not declared in the collection schema",
                    file.name()
                )));
            };
            let expected = expected_item_type(field.field_type);
            let actual = match value {
                CollectionItemValue::Text(_) => ItemType::Text,
                CollectionItemValue::Number(_) => ItemType::Number,
                CollectionItemValue::Subitem(subitem) => match subitem.datum() {
                    Some(CollectionSubitemValue::Text(_)) => ItemType::Text,
                    Some(CollectionSubitemValue::Number(_)) => ItemType::Number,
                    // 只有前缀、没有数据时无从比较类型。
                    None => continue,
                },
            };
            if actual != expected {
                return Err(WriteError::InvalidObject(format!(
                    "collection item field {key:?} of embedded file {:?} expects {}, got {}",
                    file.name(),
                    expected.describe(),
                    actual.describe()
                )));
            }
        }
    }
    Ok(())
}

/// 集合字段的数据形态，用于对照 `/CI` 值与 `/Subtype`。
#[derive(Clone, Copy, PartialEq, Eq)]
enum ItemType {
    Text,
    Number,
}

impl ItemType {
    const fn describe(self) -> &'static str {
        match self {
            Self::Text => "a text string",
            Self::Number => "a number",
        }
    }
}

/// Table 157 `/Subtype` → `/CI` 值形态：`N`/`Size` 为数，其余为文本串。
const fn expected_item_type(field: CollectionFieldType) -> ItemType {
    match field {
        CollectionFieldType::Number | CollectionFieldType::Size => ItemType::Number,
        CollectionFieldType::Text
        | CollectionFieldType::Date
        | CollectionFieldType::FileName
        | CollectionFieldType::Description
        | CollectionFieldType::ModificationDate
        | CollectionFieldType::CreationDate => ItemType::Text,
    }
}

/// 为每个注册项分配（文件规格，嵌入流）两个对象号；`/CI` 另占一个。
pub(super) fn allocate(files: &[EmbeddedFile], next: &mut i64) -> Vec<EmbeddedSlot> {
    files
        .iter()
        .map(|file| {
            let filespec_num = *next;
            let stream_num = *next + 1;
            *next += 2;
            let collection_item_num = file.collection_item().is_some().then(|| {
                let num = *next;
                *next += 1;
                num
            });
            let related_file_nums = file
                .related_files()
                .iter()
                .map(|_| {
                    let num = *next;
                    *next += 1;
                    num
                })
                .collect();
            EmbeddedSlot {
                filespec_num,
                stream_num,
                collection_item_num,
                related_file_nums,
            }
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
        if version >= PdfVersion::V1_7 {
            filespec = filespec.insert("UF", text_string_value(file.name()));
        }
        if let Some(description) = file.description() {
            filespec = filespec.insert("Desc", text_string_value(description));
        }
        if let Some(item) = file.collection_item() {
            let ci_num = slot
                .collection_item_num
                .expect("collection item object number is allocated");
            filespec = filespec.insert("CI", Value::Ref(ci_num, 0));
            doc.set_object(Object::new(ci_num, 0, Value::Dict(item.to_dict())));
            doc.features.borrow_mut().insert(PdfFeature::CollectionItem);
        }
        let mut carriers = Dict::new();
        if file.short_name().is_some() {
            carriers = carriers.insert("F", Value::Ref(slot.stream_num, 0));
        }
        if version >= PdfVersion::V1_7 {
            carriers = carriers.insert("UF", Value::Ref(slot.stream_num, 0));
        }
        filespec = filespec.insert("EF", Value::Dict(carriers));
        // `/RF`（7.11.4.2）：与 `/EF` 同结构的字典，键为已存在的载体，
        // 值为 `[文件名 text string, 嵌入流引用]` 成对数组。
        if !file.related_files().is_empty() {
            let mut rf = Dict::new();
            for (related, num) in file.related_files().iter().zip(&slot.related_file_nums) {
                let array = rf
                    .get(related.carrier())
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let array = array
                    .push(text_string_value(related.name()))
                    .push(Value::Ref(*num, 0));
                rf = rf.insert(related.carrier(), Value::Array(array));
            }
            filespec = filespec.insert("RF", Value::Dict(rf));
        }
        doc.set_object(Object::new(slot.filespec_num, 0, Value::Dict(filespec)));

        // 关联文件的嵌入流对象（Table 45 同构：`/Type` `/Params`）。
        for (related, num) in file.related_files().iter().zip(&slot.related_file_nums) {
            let params = Dict::new()
                .insert("Size", Value::Integer(related.data().len() as i64))
                .insert("CheckSum", Value::HexString(hex_encode(&related.checksum())));
            let dict = Dict::new()
                .insert("Type", Value::Name("EmbeddedFile".into()))
                .insert("Params", Value::Dict(params))
                .insert("Length", Value::Integer(related.data().len() as i64));
            doc.set_object(Object::new(
                *num,
                0,
                Value::Stream(dict, StreamData::Owned(related.data().to_vec())),
            ));
        }

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
