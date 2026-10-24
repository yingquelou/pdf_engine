//! 普通字体字典与字体程序嵌入（含 TrueType / Type 1 子集）。

use crate::core::builder::{Font, FontProgram};
use crate::core::reader::font::{subset_truetype, subset_type1};
use crate::core::types::{Dict, Document, Object, StreamData, Value};
use crate::core::writer::WriteError;

/// 构造普通（非 Type 3）字体字典，并在 `doc` 中写出描述符与字体程序流。
///
/// `/Subtype` 由嵌入程序类型决定；请求字形子集时先重写程序字节，
/// `/BaseFont` 的六字母标签仍按原始程序推导（PDF 32000-1 §9.6.4）。
/// 子集请求与程序类型不匹配时返回 [`WriteError::InvalidObject`]。
pub(super) fn font_dict(
    doc: &mut Document,
    f: &Font,
    num_out: &mut i64,
) -> Result<Dict, WriteError> {
    let mut num = *num_out;
    // 嵌入 TrueType 程序时 `/Subtype` 必须是 TrueType；
    // Type 1 与 CFF（FontFile3 /Type1C）都使用 Type1 子类型。
    let subtype = match &f.font_program {
        Some(FontProgram::TrueType(_)) => "TrueType",
        _ => "Type1",
    };
    // 两种子集字形列表互斥：分别针对 TrueType gid 与 Type 1 字形名。
    if f.subset_glyphs.is_some() && f.subset_type1_glyphs.is_some() {
        return Err(WriteError::InvalidObject(format!(
            "subset_glyphs and subset_type1_glyphs cannot be combined \
             for /BaseFont {}",
            f.name
        )));
    }
    // Type 1 子集字形名必须由嵌入的 Type 1 程序承载。
    if f.subset_type1_glyphs.is_some() {
        match &f.font_program {
            Some(FontProgram::Type1(_)) => {}
            Some(FontProgram::TrueType(_)) => {
                return Err(WriteError::InvalidObject(format!(
                    "subset_type1_glyphs requires an embedded Type 1 font program, \
                     but /BaseFont {} embeds a TrueType program",
                    f.name
                )));
            }
            Some(FontProgram::CFF(_)) => {
                return Err(WriteError::InvalidObject(format!(
                    "subset_type1_glyphs requires an embedded Type 1 font program, \
                     but /BaseFont {} embeds a CFF program",
                    f.name
                )));
            }
            None => {
                return Err(WriteError::InvalidObject(format!(
                    "subset_type1_glyphs requires an embedded Type 1 font program, \
                     but /BaseFont {} has none",
                    f.name
                )));
            }
        }
    }
    // 子集字体：BaseFont 加六字母标签前缀（PDF 32000-1 §9.6.4）。
    let base_font = match (&f.font_program, f.subset) {
        (Some(program), true) => format!("{}+{}", program.subset_tag(), f.name),
        _ => f.name.clone(),
    };
    let mut font_dict = Dict::new()
        .insert("Type", Value::Name("Font".into()))
        .insert("Subtype", Value::Name(subtype.into()))
        .insert("BaseFont", Value::Name(base_font));
    if let Some(encoding) = f.encoding.to_value() {
        font_dict = font_dict.insert("Encoding", encoding);
    }
    // 子集字形列表必须由嵌入的 TrueType 程序承载。
    if f.subset_glyphs.is_some() && f.font_program.is_none() {
        return Err(WriteError::InvalidObject(format!(
            "subset_glyphs requires an embedded TrueType font program, \
             but /BaseFont {} has none",
            f.name
        )));
    }
    // 写入字体描述符（如果提供或有字体程序）
    if f.descriptor.is_some() || f.font_program.is_some() {
        let descriptor_ref = num;
        num += 1;
        font_dict = font_dict.insert("FontDescriptor", Value::Ref(descriptor_ref, 0));
        // 构建字体描述符字典
        let mut descriptor_dict = if let Some(descriptor) = &f.descriptor {
            match descriptor.to_value() {
                Value::Dict(d) => d,
                _ => Dict::new().insert("Type", Value::Name("FontDescriptor".into())),
            }
        } else {
            Dict::new().insert("Type", Value::Name("FontDescriptor".into()))
        };
        // 写入字体程序流（如果有）
        if let Some(program) = &f.font_program {
            let program_ref = num;
            num += 1;
            // 子集化：请求了字形列表时，用子集重写结果替换原始
            // 程序字节（PDF 32000-1 §9.6.4）。BaseFont 标签仍由
            // 原始程序推导，保证确定性。
            let subset_program = match (&f.subset_glyphs, &f.subset_type1_glyphs) {
                (Some(glyph_ids), None) => match program {
                    FontProgram::TrueType(data) => {
                        Some(subset_truetype(data, glyph_ids).map_err(|error| {
                            WriteError::InvalidObject(format!(
                                "TrueType subsetting for /BaseFont {} failed: {error}",
                                f.name
                            ))
                        })?)
                    }
                    other => {
                        let kind = match other {
                            FontProgram::Type1(_) => "Type1",
                            FontProgram::CFF(_) => "CFF",
                            FontProgram::TrueType(_) => unreachable!("checked above"),
                        };
                        return Err(WriteError::InvalidObject(format!(
                            "subset_glyphs requires an embedded TrueType font program, \
                             but /BaseFont {} embeds a {kind} program",
                            f.name
                        )));
                    }
                },
                (None, Some(glyph_names)) => match program {
                    FontProgram::Type1(data) => {
                        let names: Vec<&str> =
                            glyph_names.iter().map(String::as_str).collect();
                        Some(subset_type1(data, &names).map_err(|error| {
                            WriteError::InvalidObject(format!(
                                "Type 1 subsetting for /BaseFont {} failed: {error}",
                                f.name
                            ))
                        })?)
                    }
                    _ => unreachable!("program type checked above"),
                },
                _ => None,
            };
            let program_bytes = subset_program.as_deref().unwrap_or_else(|| program.data());
            // builder 构造的 Owned 流按 writer 约定写明文；writer 会把
            // 声明在 Owned 流上的 FlateDecode 视为读端已解压而移除。
            let stream_dict = match program {
                FontProgram::Type1(_) => {
                    // /FontFile - Type 1 字体程序
                    descriptor_dict =
                        descriptor_dict.insert("FontFile", Value::Ref(program_ref, 0));
                    Dict::new()
                }
                FontProgram::TrueType(_) => {
                    // /FontFile2 - TrueType 字体程序
                    descriptor_dict =
                        descriptor_dict.insert("FontFile2", Value::Ref(program_ref, 0));
                    Dict::new()
                }
                FontProgram::CFF(_) => {
                    // /FontFile3 - CFF 字体程序
                    descriptor_dict =
                        descriptor_dict.insert("FontFile3", Value::Ref(program_ref, 0));
                    Dict::new().insert("Subtype", Value::Name("Type1C".into()))
                }
            }
            .insert("Length", Value::Integer(program_bytes.len() as i64));
            doc.set_object(Object::new(
                program_ref,
                0,
                Value::Stream(stream_dict, StreamData::Owned(program_bytes.to_vec())),
            ));
        }
        doc.set_object(Object::new(descriptor_ref, 0, Value::Dict(descriptor_dict)));
    }
    *num_out = num;
    Ok(font_dict)
}
