//! 普通字体字典与字体程序嵌入（含 TrueType / Type 1 / name-keyed CFF 子集）。

use crate::core::builder::{Font, FontEncoding, FontProgram, FontWidths, OpenTypeFlavor};
use crate::core::reader::font::{
    font_file_lengths, strip_subset_tag, subset_cff_type1c, subset_truetype, subset_type1,
};
use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};
use crate::core::writer::WriteError;

use super::mmtype1;

/// 构造普通（非 Type 3）字体字典，并在 `doc` 中写出描述符与字体程序流。
///
/// `/Subtype` 由嵌入程序类型决定：TrueType 轮廓（含 TrueType-flavored
/// OpenType）为 `TrueType`，其余为 `Type1`；OpenType 程序按 SFNT 版本
/// 标签写 `/FontFile2`（TrueType 轮廓）或 `/FontFile3` `/Subtype /OpenType`
/// （CFF 轮廓）。请求字形子集时先重写程序字节，
/// `/BaseFont` 的六字母标签仍按原始程序推导（PDF 32000-1 §9.6.4）。
/// 简单字体字典按 Table 111 写出 `/FirstChar`、`/LastChar` 与 `/Widths`；
/// 描述符缺失的 Table 122 必需项由
/// [`FontDescriptor::fill_required_defaults`](crate::core::builder::FontDescriptor)
/// 补齐。子集请求与程序类型不匹配、编码名未知、`/Widths` 非法、
/// 嵌入程序的非标准 14 字体缺少 `/Widths` 时返回 [`WriteError::InvalidObject`]。
///
/// [`Font::mmtype1`](crate::core::builder::Font::mmtype1) 标记的字体走同一条
/// 通路写出 §9.6.2.3 的 MM 实例字典：`/Subtype` 取 `MMType1`，`/BaseFont`
/// 按实例名把空格换成下划线，`/Widths` 与 `/FontDescriptor` 一律必需
/// （标准 14 豁免只覆盖 `/Type1`），嵌入程序只接受 Table 126 许可的
/// `/FontFile` 与 `/FontFile3 /Type1C`；校验集中在
/// [`super::mmtype1`]，MMType1 自 PDF 1.0 起可用（Table 110 无 `(PDF 1.x)`
/// 标注），因此没有新的 `PdfFeature` 门禁。非 MM 字体的既有输出逐字节不变。
pub(super) fn font_dict(
    doc: &mut Document,
    f: &Font,
    num_out: &mut i64,
) -> Result<Dict, WriteError> {
    let mut num = *num_out;
    // MMType1 的嵌入程序格式（Table 126 + §9.6.2.3 快照）先于其他校验，
    // 保证拒绝理由指向 MM 而不是程序细节。
    mmtype1::check_program_format(f)?;
    // 未知编码名无法写出合法的 `/Encoding`：读端会拒绝整份字体，构建期即报错。
    if let Some(name) = f.encoding.unknown_name() {
        return Err(WriteError::InvalidObject(format!(
            "/Encoding /{name} for /BaseFont {} is not a supported predefined encoding",
            f.name
        )));
    }
    // OpenType 程序的 SFNT 版本标签决定轮廓类型；非法标签不写出任何对象。
    let open_type_flavor = match &f.font_program {
        Some(program @ FontProgram::OpenType(_)) => {
            Some(program.open_type_flavor().ok_or_else(|| {
                WriteError::InvalidObject(format!(
                    "OpenType font program for /BaseFont {} has an unsupported SFNT flavor; \
                     expected OTTO, 0x00010000, or 'true'",
                    f.name
                ))
            })?)
        }
        _ => None,
    };
    // 嵌入程序决定字体 `/Subtype`：TrueType 轮廓（含 TrueType-flavored
    // OpenType）为 TrueType；Type 1、CFF 与 CFF-flavored OpenType 为 Type1。
    // MM 实例按 §9.6.2.3（raw 13974）写 MMType1，条目与 Type 1 同构。
    let subtype = if f.mmtype1 {
        mmtype1::SUBTYPE
    } else {
        match (&f.font_program, open_type_flavor) {
            (Some(FontProgram::TrueType(_)), _) => "TrueType",
            (Some(FontProgram::OpenType(_)), Some(OpenTypeFlavor::TrueType)) => "TrueType",
            _ => "Type1",
        }
    };
    // 三种子集字形列表互斥：分别针对 TrueType gid、Type 1 字形名与
    // name-keyed CFF 字形名。
    let subset_kinds = [
        ("subset_glyphs", f.subset_glyphs.is_some()),
        ("subset_type1_glyphs", f.subset_type1_glyphs.is_some()),
        ("subset_cff_glyphs", f.subset_cff_glyphs.is_some()),
    ];
    let active: Vec<&str> = subset_kinds
        .iter()
        .filter(|(_, present)| *present)
        .map(|(name, _)| *name)
        .collect();
    if active.len() > 1 {
        return Err(WriteError::InvalidObject(format!(
            "{} cannot be combined for /BaseFont {}",
            active.join(" and "),
            f.name
        )));
    }
    // CFF 子集化会移除程序内的 `/Encoding`（PDF 32000-1 §9.6.4），与
    // `/Encoding` 缺省表示的内建编码语义冲突。
    if f.subset_cff_glyphs.is_some() && matches!(f.encoding, FontEncoding::BuiltIn) {
        return Err(WriteError::InvalidObject(format!(
            "subset_cff_glyphs cannot be combined with FontEncoding::BuiltIn for /BaseFont {}: \
             CFF subsetting removes the program's /Encoding",
            f.name
        )));
    }
    // name-keyed CFF 子集字形名必须由嵌入的 CFF 程序承载。
    if f.subset_cff_glyphs.is_some() {
        match &f.font_program {
            Some(FontProgram::CFF(_)) => {}
            Some(FontProgram::Type1(_)) => {
                return Err(WriteError::InvalidObject(format!(
                    "subset_cff_glyphs requires an embedded CFF font program, \
                     but /BaseFont {} embeds a Type 1 program",
                    f.name
                )));
            }
            Some(FontProgram::TrueType(_)) => {
                return Err(WriteError::InvalidObject(format!(
                    "subset_cff_glyphs requires an embedded CFF font program, \
                     but /BaseFont {} embeds a TrueType program",
                    f.name
                )));
            }
            Some(FontProgram::OpenType(_)) => {
                return Err(WriteError::InvalidObject(format!(
                    "subset_cff_glyphs requires an embedded CFF font program, \
                     but /BaseFont {} embeds an OpenType program",
                    f.name
                )));
            }
            None => {
                return Err(WriteError::InvalidObject(format!(
                    "subset_cff_glyphs requires an embedded CFF font program, \
                     but /BaseFont {} has none",
                    f.name
                )));
            }
        }
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
            Some(FontProgram::OpenType(_)) => {
                return Err(WriteError::InvalidObject(format!(
                    "subset_type1_glyphs requires an embedded Type 1 font program, \
                     but /BaseFont {} embeds an OpenType program",
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
    // 标签同时纳入子集列表，确保同一程序的不同子集得到不同标签。
    // MM 实例的 `/BaseFont` 先按 §9.6.2.3 把实例名的空格换成下划线，
    // 其余字符按字面写出（raw 13975–13977）。
    let literal_name = if f.mmtype1 {
        mmtype1::base_font_for(&f.name)
    } else {
        f.name.clone()
    };
    let base_font = match (&f.font_program, f.subset) {
        (Some(program), true) => format!(
            "{}+{}",
            program.subset_tag_for_name(&literal_name, &subset_discriminator(f)),
            literal_name
        ),
        _ => literal_name,
    };
    // `/Widths` 的构建期校验：非空、落在单字节码空间、宽度有限且非负。
    if let Some(widths) = &f.widths {
        validate_widths(f, widths)?;
    }
    // 非标准 14 简单字体必须给出 `/Widths`（Table 111 必需项），
    // 无论是否嵌入字体程序；MMType1 不享受标准 14 豁免（§9.6.2.2 只列
    // Type 1 字体），缺 `/Widths` 同样在构建期拒绝。
    if f.widths.is_none() && !mmtype1::may_omit_widths(f, &base_font) {
        return Err(if f.mmtype1 {
            mmtype1::missing_widths(f)
        } else {
            WriteError::InvalidObject(format!(
                "/BaseFont {} is not one of the standard 14 fonts \
                 and requires explicit /Widths; call Font::with_widths",
                f.name
            ))
        });
    }
    let mut font_dict = Dict::new()
        .insert("Type", Value::Name("Font".into()))
        .insert("Subtype", Value::Name(subtype.into()))
        .insert("BaseFont", Value::Name(base_font.clone()));
    if let Some(encoding) = f.encoding.to_value() {
        font_dict = font_dict.insert("Encoding", encoding);
    }
    if let Some(widths) = &f.widths {
        let last_char = widths.first_char as usize + widths.widths.len() - 1;
        let mut array = Array::new();
        for width in &widths.widths {
            array = array.push(Value::Float(*width));
        }
        font_dict = font_dict
            .insert("FirstChar", Value::Integer(i64::from(widths.first_char)))
            .insert("LastChar", Value::Integer(last_char as i64))
            .insert("Widths", Value::Array(array));
    }
    // 子集字形列表必须由嵌入的 TrueType 程序承载。
    if f.subset_glyphs.is_some() && f.font_program.is_none() {
        return Err(WriteError::InvalidObject(format!(
            "subset_glyphs requires an embedded TrueType font program, \
             but /BaseFont {} has none",
            f.name
        )));
    }
    // 写入字体描述符（如果提供或有字体程序）；MMType1 必须有 /FontDescriptor
    // （Table 111 必需项，标准 14 豁免不覆盖 MM 实例）。
    let writes_descriptor = f.descriptor.is_some() || f.font_program.is_some();
    mmtype1::check_descriptor(f, writes_descriptor)?;
    if writes_descriptor {
        let descriptor_ref = num;
        num += 1;
        font_dict = font_dict.insert("FontDescriptor", Value::Ref(descriptor_ref, 0));
        // 构建字体描述符字典：Table 122 必需项缺省时补默认值，
        // 用户显式提供的度量保留。
        let mut descriptor = f.descriptor.clone().unwrap_or_default();
        descriptor.fill_required_defaults(&base_font);
        let mut descriptor_dict = match descriptor.to_value() {
            Value::Dict(d) => d,
            _ => Dict::new().insert("Type", Value::Name("FontDescriptor".into())),
        };
        // 写入字体程序流（如果有）
        if let Some(program) = &f.font_program {
            let program_ref = num;
            num += 1;
            // 子集化：请求了字形列表时，用子集重写结果替换原始
            // 程序字节（PDF 32000-1 §9.6.4）。BaseFont 标签仍由
            // 原始程序推导，保证确定性。
            let subset_program = match (
                &f.subset_glyphs,
                &f.subset_type1_glyphs,
                &f.subset_cff_glyphs,
            ) {
                (Some(glyph_ids), None, None) => match program {
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
                            FontProgram::OpenType(_) => "OpenType",
                            FontProgram::TrueType(_) => unreachable!("checked above"),
                        };
                        return Err(WriteError::InvalidObject(format!(
                            "subset_glyphs requires an embedded TrueType font program, \
                                 but /BaseFont {} embeds a {kind} program",
                            f.name
                        )));
                    }
                },
                (None, Some(glyph_names), None) => match program {
                    FontProgram::Type1(data) => {
                        let names: Vec<&str> = glyph_names.iter().map(String::as_str).collect();
                        Some(subset_type1(data, &names).map_err(|error| {
                            WriteError::InvalidObject(format!(
                                "Type 1 subsetting for /BaseFont {} failed: {error}",
                                f.name
                            ))
                        })?)
                    }
                    _ => unreachable!("program type checked above"),
                },
                (None, None, Some(glyph_names)) => match program {
                    FontProgram::CFF(data) => {
                        let names: Vec<&str> = glyph_names.iter().map(String::as_str).collect();
                        Some(subset_cff_type1c(data, &names).map_err(|error| {
                            WriteError::InvalidObject(format!(
                                "CFF subsetting for /BaseFont {} failed: {error}",
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
            // Table 127：Type 1 需 Length1/2/3，TrueType 需 Length1；
            // FontFile3 的 CFF 程序不得带这些条目。
            let stream_dict = match program {
                FontProgram::Type1(_) => {
                    // /FontFile - Type 1 字体程序
                    descriptor_dict =
                        descriptor_dict.insert("FontFile", Value::Ref(program_ref, 0));
                    let (length1, length2, length3) = font_file_lengths(program_bytes);
                    Dict::new()
                        .insert("Length1", Value::Integer(length1 as i64))
                        .insert("Length2", Value::Integer(length2 as i64))
                        .insert("Length3", Value::Integer(length3 as i64))
                }
                FontProgram::TrueType(_) => {
                    // /FontFile2 - TrueType 字体程序
                    descriptor_dict =
                        descriptor_dict.insert("FontFile2", Value::Ref(program_ref, 0));
                    Dict::new().insert("Length1", Value::Integer(program_bytes.len() as i64))
                }
                FontProgram::CFF(_) => {
                    // /FontFile3 - CFF 字体程序
                    descriptor_dict =
                        descriptor_dict.insert("FontFile3", Value::Ref(program_ref, 0));
                    Dict::new().insert("Subtype", Value::Name("Type1C".into()))
                }
                FontProgram::OpenType(_) => match open_type_flavor {
                    Some(OpenTypeFlavor::Cff) => {
                        // /FontFile3 - CFF-flavored OpenType 程序（Table 126）
                        descriptor_dict =
                            descriptor_dict.insert("FontFile3", Value::Ref(program_ref, 0));
                        Dict::new().insert("Subtype", Value::Name("OpenType".into()))
                    }
                    Some(OpenTypeFlavor::TrueType) => {
                        // TrueType-flavored OpenType 按 §9.9 写 /FontFile2，
                        // 并写 Table 127 要求的 Length1。
                        descriptor_dict =
                            descriptor_dict.insert("FontFile2", Value::Ref(program_ref, 0));
                        Dict::new().insert("Length1", Value::Integer(program_bytes.len() as i64))
                    }
                    None => unreachable!("flavor validated at the start of font_dict"),
                },
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

/// 校验简单字体 `/Widths`：非空、覆盖区间落在单字节码空间、宽度有限且非负。
fn validate_widths(f: &Font, widths: &FontWidths) -> Result<(), WriteError> {
    if widths.widths.is_empty() {
        return Err(WriteError::InvalidObject(format!(
            "/Widths for /BaseFont {} must not be empty; call Font::with_widths with at least one width",
            f.name
        )));
    }
    let end = usize::from(widths.first_char) + widths.widths.len();
    if end > 256 {
        return Err(WriteError::InvalidObject(format!(
            "/Widths for /BaseFont {} covers character codes {}..={}, \
             beyond the single-byte range 0..=255",
            f.name,
            widths.first_char,
            end - 1
        )));
    }
    for (offset, width) in widths.widths.iter().enumerate() {
        if !width.is_finite() || *width < 0.0 {
            return Err(WriteError::InvalidObject(format!(
                "/Widths for /BaseFont {} has an invalid width for character code {}: {width}",
                f.name,
                usize::from(widths.first_char) + offset
            )));
        }
    }
    Ok(())
}

/// `/BaseFont`（去子集标签后）是否为 PDF 标准 14 字体之一。
///
/// 标准 14 字体允许省略 `/Widths` 与嵌入程序（PDF 32000-1 §9.6.2.2）。
/// 该豁免只覆盖 Type 1 字体，因此 [`super::mmtype1`] 会为 MM 实例绕过它。
pub(super) fn is_standard_14(base_font: &str) -> bool {
    matches!(
        strip_subset_tag(base_font),
        "Courier"
            | "Courier-Bold"
            | "Courier-Oblique"
            | "Courier-BoldOblique"
            | "Helvetica"
            | "Helvetica-Bold"
            | "Helvetica-Oblique"
            | "Helvetica-BoldOblique"
            | "Times-Roman"
            | "Times-Bold"
            | "Times-Italic"
            | "Times-BoldItalic"
            | "Symbol"
            | "ZapfDingbats"
    )
}

/// 把简单字体的子集列表编码为无歧义的判别字节。
///
/// 首字节区分列表种类（TrueType gid / Type 1 字形名 / CFF 字形名），
/// 名称带长度前缀，避免不同列表拼接后产生相同字节串。
fn subset_discriminator(f: &Font) -> Vec<u8> {
    let mut bytes = Vec::new();
    if let Some(glyphs) = &f.subset_glyphs {
        bytes.push(1);
        for glyph in glyphs {
            bytes.extend_from_slice(&glyph.to_be_bytes());
        }
    } else if let Some(names) = &f.subset_type1_glyphs {
        bytes.push(2);
        for name in names {
            bytes.extend_from_slice(&(name.len() as u32).to_be_bytes());
            bytes.extend_from_slice(name.as_bytes());
        }
    } else if let Some(names) = &f.subset_cff_glyphs {
        bytes.push(3);
        for name in names {
            bytes.extend_from_slice(&(name.len() as u32).to_be_bytes());
            bytes.extend_from_slice(name.as_bytes());
        }
    }
    bytes
}
