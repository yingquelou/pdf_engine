//! CIDFont 后代字典、Type 0 父字体与相关流。

use super::cmap::{cid_encoding_cmap, to_unicode_cmap};
use crate::core::builder::{CidFont, CidFontSubtype, FontProgram};
use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};
use crate::core::writer::WriteError;

/// 写出一个 CIDFont 的全部对象：描述符（含 `/CIDSet` 与嵌入程序流）、
/// CIDFont 后代字典、Type 0 父字体（含嵌入编码 CMap 与 ToUnicode CMap）。
///
/// 自定义编码在 build 阶段校验：`wmode` 仅 0/1，且至少一条映射；
/// 子类型与嵌入程序类型不匹配时返回 [`WriteError::InvalidObject`]。
pub(super) fn write_cid_font(
    doc: &mut Document,
    cid_font: &CidFont,
    type0_num: i64,
    num_out: &mut i64,
) -> Result<(), WriteError> {
    let mut num = *num_out;
    // 自定义编码在 build 阶段校验：wmode 仅 0/1，且至少一条映射。
    if let Some(encoding) = &cid_font.encoding {
        if encoding.wmode > 1 {
            return Err(WriteError::InvalidObject(format!(
                "CidEncoding wmode must be 0 or 1, got {}",
                encoding.wmode
            )));
        }
        if encoding.mappings.is_empty() {
            return Err(WriteError::InvalidObject(
                "CidEncoding requires at least one code-to-CID mapping".into(),
            ));
        }
    }

    // 1. FontDescriptor 字典（/CIDSet 作为独立流对象引用）
    let descriptor_num = num;
    num += 1;
    let mut descriptor_value = cid_font.descriptor.to_value();
    if let Some(cid_set_data) = cid_font.descriptor.cid_set_data() {
        let cid_set_num = num;
        num += 1;
        if let Value::Dict(ref d) = descriptor_value {
            descriptor_value =
                Value::Dict(d.clone().insert("CIDSet", Value::Ref(cid_set_num, 0)));
        }
        let cid_set_len = cid_set_data.len();
        doc.set_object(Object::new(
            cid_set_num,
            0,
            Value::Stream(
                Dict::new().insert("Length", Value::Integer(cid_set_len as i64)),
                StreamData::Owned(cid_set_data.to_vec()),
            ),
        ));
    }
    // 嵌入 CIDFont 字体程序：Type2 → /FontFile2，Type0 → /FontFile3
    // （流 /Subtype /CIDFontType0C）。builder 的 Owned 流写明文。
    if let Some(program) = &cid_font.font_program {
        let (program_key, stream_subtype) = match (cid_font.subtype, program) {
            (CidFontSubtype::CIDFontType2, FontProgram::TrueType(_)) => ("FontFile2", None),
            (CidFontSubtype::CIDFontType0, FontProgram::CFF(_)) => {
                ("FontFile3", Some("CIDFontType0C"))
            }
            (subtype, _) => {
                let expected = match subtype {
                    CidFontSubtype::CIDFontType2 => "TrueType",
                    CidFontSubtype::CIDFontType0 => "CFF",
                };
                return Err(WriteError::InvalidObject(format!(
                    "{} requires a {expected} font program",
                    subtype.as_name()
                )));
            }
        };
        let program_num = num;
        num += 1;
        let mut stream_dict = Dict::new();
        if let Some(subtype) = stream_subtype {
            stream_dict = stream_dict.insert("Subtype", Value::Name(subtype.into()));
        }
        stream_dict =
            stream_dict.insert("Length", Value::Integer(program.data().len() as i64));
        doc.set_object(Object::new(
            program_num,
            0,
            Value::Stream(stream_dict, StreamData::Owned(program.data().to_vec())),
        ));
        if let Value::Dict(d) = descriptor_value {
            descriptor_value =
                Value::Dict(d.insert(program_key, Value::Ref(program_num, 0)));
        }
    }
    doc.set_object(Object::new(descriptor_num, 0, descriptor_value));

    // 2. CIDFont 后代字典
    let cid_font_num = num;
    num += 1;
    let mut cid_dict = Dict::new()
        .insert("Type", Value::Name("Font".into()))
        .insert("Subtype", Value::Name(cid_font.subtype.as_name().into()))
        .insert("BaseFont", Value::Name(cid_font.name.clone()))
        .insert("CIDSystemInfo", cid_font.system_info.to_value())
        .insert("FontDescriptor", Value::Ref(descriptor_num, 0));
    if (cid_font.default_width - 1000.0).abs() > f64::EPSILON {
        cid_dict = cid_dict.insert("DW", Value::Float(cid_font.default_width));
    }
    if !cid_font.widths.is_empty() {
        cid_dict = cid_dict.insert("W", cid_font.widths_to_value());
    }
    doc.set_object(Object::new(cid_font_num, 0, Value::Dict(cid_dict)));

    // 3. Type 0 父字体字典；有 CID→Unicode 映射时附带 /ToUnicode CMap
    let mut type0_dict = Dict::new()
        .insert("Type", Value::Name("Font".into()))
        .insert("Subtype", Value::Name("Type0".into()))
        .insert("BaseFont", Value::Name(cid_font.name.clone()))
        .insert("Encoding", Value::Name("Identity-H".into()))
        .insert(
            "DescendantFonts",
            Value::Array(Array::new().push(Value::Ref(cid_font_num, 0))),
        );
    if let Some(encoding) = &cid_font.encoding {
        // 非 Identity 编码：嵌入 CMap 流作为 /Encoding 引用。
        let encoding_num = num;
        num += 1;
        let cmap = cid_encoding_cmap(encoding);
        let cmap_len = cmap.len();
        doc.set_object(Object::new(
            encoding_num,
            0,
            Value::Stream(
                Dict::new().insert("Length", Value::Integer(cmap_len as i64)),
                StreamData::Owned(cmap),
            ),
        ));
        type0_dict = type0_dict.insert("Encoding", Value::Ref(encoding_num, 0));
    }
    if !cid_font.to_unicode.is_empty() {
        let cmap_num = num;
        num += 1;
        let cmap = to_unicode_cmap(&cid_font.to_unicode);
        let cmap_len = cmap.len();
        doc.set_object(Object::new(
            cmap_num,
            0,
            Value::Stream(
                Dict::new().insert("Length", Value::Integer(cmap_len as i64)),
                StreamData::Owned(cmap),
            ),
        ));
        type0_dict = type0_dict.insert("ToUnicode", Value::Ref(cmap_num, 0));
    }
    doc.set_object(Object::new(type0_num, 0, Value::Dict(type0_dict)));
    *num_out = num;
    Ok(())
}
