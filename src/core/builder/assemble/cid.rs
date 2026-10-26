//! CIDFont 后代字典、Type 0 父字体与相关流。

use super::cmap::{cid_encoding_cmap, cmap_name, to_unicode_cmap};
use crate::core::builder::{
    CidFont, CidFontSubtype, CidVerticalMetrics, FontProgram, OpenTypeFlavor,
};
use crate::core::reader::font::predefined_cmaps::resolve_predefined_cmap;
use crate::core::reader::font::{subset_cff, subset_truetype_with_mapping};
use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};
use crate::core::writer::WriteError;
use std::collections::BTreeMap;

/// 写出一个 CIDFont 的全部对象：描述符（含 `/CIDSet` 与嵌入程序流）、
/// CIDFont 后代字典、Type 0 父字体（含嵌入编码 CMap 与 ToUnicode CMap）。
/// 描述符缺少的 Table 122 必需项由
/// [`FontDescriptor::fill_required_defaults`](crate::core::builder::FontDescriptor)
/// 补齐。
///
/// 自定义编码在 build 阶段校验：`wmode` 仅 0/1，且至少一条映射；
/// 子类型与嵌入程序类型不匹配时返回 [`WriteError::InvalidObject`]。
/// OpenType 程序按 SFNT 版本标签写出：CIDFontType0 只接受 CFF-flavored
/// （`OTTO`，写 `/FontFile3` `/Subtype /OpenType`），CIDFontType2 只接受
/// TrueType-flavored（写 `/FontFile2`，ISO 32000-1 §9.9）。
pub(super) fn write_cid_font(
    doc: &mut Document,
    cid_font: &CidFont,
    type0_num: i64,
    num_out: &mut i64,
) -> Result<(), WriteError> {
    let mut num = *num_out;
    // OpenType 程序的 SFNT 版本标签决定轮廓类型，构建期校验。
    let open_type_flavor = match &cid_font.font_program {
        Some(program @ FontProgram::OpenType(_)) => {
            Some(program.open_type_flavor().ok_or_else(|| {
                WriteError::InvalidObject(format!(
                    "OpenType font program for /BaseFont {} has an unsupported SFNT flavor; \
                     expected OTTO, 0x00010000, or 'true'",
                    cid_font.name
                ))
            })?)
        }
        _ => None,
    };
    // 预定义 CMap 名称引用与自定义嵌入 CMap 互斥。
    if cid_font.encoding.is_some() && cid_font.predefined_encoding.is_some() {
        return Err(WriteError::InvalidObject(
            "CidFont predefined encoding and custom CidEncoding are mutually exclusive".into(),
        ));
    }
    // 预定义 CMap 名称复用读端名册校验；未知名称（含 -V 竖排变体）报错。
    if let Some(name) = &cid_font.predefined_encoding {
        resolve_predefined_cmap(name)
            .map_err(|_| WriteError::InvalidObject(format!("unknown predefined CMap: {name}")))?;
    }
    // 自定义编码在 build 阶段校验：wmode 仅 0/1、至少一条映射、码长
    // 1..=4、/CMapName 合法、usecmap 基座已知且不与 CMapName 同名。
    if let Some(encoding) = &cid_font.encoding {
        if encoding.wmode > 1 {
            return Err(WriteError::InvalidObject(format!(
                "CidEncoding wmode must be 0 or 1, got {}",
                encoding.wmode
            )));
        }
        if encoding.mappings.is_empty() && encoding.byte_mappings.is_empty() {
            return Err(WriteError::InvalidObject(
                "CidEncoding requires at least one code-to-CID mapping".into(),
            ));
        }
        if let Some(code) = encoding
            .byte_mappings
            .keys()
            .find(|code| !(1..=4).contains(&code.len()))
        {
            return Err(WriteError::InvalidObject(format!(
                "CidEncoding character code must be 1 to 4 bytes, got {} bytes",
                code.len()
            )));
        }
        if let Some(name) = &encoding.cmap_name
            && !is_valid_pdf_name(name)
        {
            return Err(WriteError::InvalidObject(format!(
                "CidEncoding /CMapName {name:?} is not a valid PDF name"
            )));
        }
        if let Some(base) = &encoding.usecmap {
            let known = matches!(base.as_str(), "Identity-H" | "Identity-V")
                || resolve_predefined_cmap(base).is_ok();
            if !known {
                return Err(WriteError::InvalidObject(format!(
                    "CidEncoding usecmap base /{base} is not a supported CMap"
                )));
            }
            if cmap_name(encoding) == *base {
                return Err(WriteError::InvalidObject(
                    "CidEncoding usecmap base must differ from /CMapName".into(),
                ));
            }
        }
    }
    // ToUnicode 字节键：长度须为 1..=4，且不与 2 字节映射键重复（读端会把
    // 重复源码视为非法 CMap）。
    for code in cid_font.to_unicode_bytes.keys() {
        if !(1..=4).contains(&code.len()) {
            return Err(WriteError::InvalidObject(format!(
                "CidFont ToUnicode character code must be 1 to 4 bytes, got {} bytes",
                code.len()
            )));
        }
        if let [high, low] = code.as_slice()
            && cid_font
                .to_unicode
                .contains_key(&u16::from_be_bytes([*high, *low]))
        {
            return Err(WriteError::InvalidObject(
                "CidFont ToUnicode maps the same 2-byte source code twice".into(),
            ));
        }
    }
    // CID 子集请求：CIDFontType0 需嵌入 CFF，CIDFontType2 需嵌入 TrueType。
    if let Some(cids) = &cid_font.subset_cids {
        let supported = matches!(
            (&cid_font.subtype, &cid_font.font_program),
            (CidFontSubtype::CIDFontType0, Some(FontProgram::CFF(_)))
                | (CidFontSubtype::CIDFontType2, Some(FontProgram::TrueType(_)))
        );
        if !supported {
            let expected = match cid_font.subtype {
                CidFontSubtype::CIDFontType0 => "CIDFontType0 with an embedded CFF program",
                CidFontSubtype::CIDFontType2 => "CIDFontType2 with an embedded TrueType program",
            };
            return Err(WriteError::InvalidObject(format!(
                "subset_cids requires {expected}, but /BaseFont {} is {} with {}",
                cid_font.name,
                cid_font.subtype.as_name(),
                program_kind(&cid_font.font_program)
            )));
        }
        // TrueType 子集按 CID→GID 映射裁剪；显式映射须覆盖每个请求的 CID。
        if cid_font.subtype == CidFontSubtype::CIDFontType2
            && let Some(mapping) = &cid_font.cid_to_gid
        {
            for cid in cids {
                if usize::from(*cid) >= mapping.len() {
                    return Err(WriteError::InvalidObject(format!(
                        "subset CID {cid} for /BaseFont {} is outside its {} entry /CIDToGIDMap",
                        cid_font.name,
                        mapping.len()
                    )));
                }
            }
        }
    }
    // `/CIDToGIDMap` 只存在于 CIDFontType2（PDF 32000-1 Table 116），
    // 且语义是把 CID 映射到嵌入 TrueType 程序（/FontFile2）的 glyph ID；
    // 未嵌入 TrueType 时该条目不可解释（Table 117）。
    if cid_font.cid_to_gid.is_some() {
        if cid_font.subtype != CidFontSubtype::CIDFontType2 {
            return Err(WriteError::InvalidObject(format!(
                "cid_to_gid requires CIDFontType2, but /BaseFont {} is {}",
                cid_font.name,
                cid_font.subtype.as_name()
            )));
        }
        // TrueType-flavored OpenType 同样以 /FontFile2 嵌入，glyph ID
        // 语义与 TrueType 程序一致。
        let embeds_truetype_glyphs = match &cid_font.font_program {
            Some(FontProgram::TrueType(_)) => true,
            Some(FontProgram::OpenType(_)) => open_type_flavor == Some(OpenTypeFlavor::TrueType),
            _ => false,
        };
        if !embeds_truetype_glyphs {
            return Err(WriteError::InvalidObject(format!(
                "cid_to_gid requires an embedded TrueType font program, \
                 but /BaseFont {} has none",
                cid_font.name
            )));
        }
    }
    // /DW2 与 /W2：数值须有限，区间自左向右（PDF 32000-1 §9.7.4.3）。
    if !cid_font
        .default_vertical_metrics
        .iter()
        .all(|value| value.is_finite())
    {
        return Err(WriteError::InvalidObject(format!(
            "/DW2 for /BaseFont {} contains a non-finite value",
            cid_font.name
        )));
    }
    for entry in &cid_font.vertical_metrics {
        let (first, last, metrics) = match entry {
            CidVerticalMetrics::Single(cid, w1y, v1x, v1y) => (*cid, *cid, [*w1y, *v1x, *v1y]),
            CidVerticalMetrics::Range(first, last, w1y, v1x, v1y) => {
                (*first, *last, [*w1y, *v1x, *v1y])
            }
        };
        if last < first {
            return Err(WriteError::InvalidObject(format!(
                "/W2 range for /BaseFont {} ends before it starts",
                cid_font.name
            )));
        }
        if !metrics.iter().all(|value| value.is_finite()) {
            return Err(WriteError::InvalidObject(format!(
                "/W2 for /BaseFont {} contains a non-finite value",
                cid_font.name
            )));
        }
    }
    // 子集字体：BaseFont 加六字母标签前缀（PDF 32000-1 §9.6.4），
    // 标签同时纳入保留 CID 列表，Type 0 父字体与 CIDFont 后代保持一致。
    let base_font_name = match (&cid_font.font_program, &cid_font.subset_cids) {
        (Some(program), Some(cids)) => {
            let mut discriminator = Vec::with_capacity(1 + cids.len() * 2);
            discriminator.push(1);
            for cid in cids {
                discriminator.extend_from_slice(&cid.to_be_bytes());
            }
            format!(
                "{}+{}",
                program.subset_tag_for_name(&cid_font.name, &discriminator),
                cid_font.name
            )
        }
        _ => cid_font.name.clone(),
    };

    // 1. FontDescriptor 字典（/CIDSet 作为独立流对象引用）
    let descriptor_num = num;
    num += 1;
    // Table 122 必需项缺省时补默认值，用户显式提供的度量保留。
    let mut descriptor = cid_font.descriptor.clone();
    descriptor.fill_required_defaults(&base_font_name);
    let mut descriptor_value = descriptor.to_value();
    // /CIDSet：有子集请求时按子集 CID 交集裁剪位图（CID 0 恒保留）。
    let cid_set_data: Option<Vec<u8>> =
        match (cid_font.descriptor.cid_set_data(), &cid_font.subset_cids) {
            (Some(data), Some(cids)) => Some(mask_cid_set(data, cids)),
            (Some(data), None) => Some(data.to_vec()),
            (None, _) => None,
        };
    if let Some(cid_set_data) = cid_set_data {
        let cid_set_num = num;
        num += 1;
        if let Value::Dict(ref d) = descriptor_value {
            descriptor_value = Value::Dict(d.clone().insert("CIDSet", Value::Ref(cid_set_num, 0)));
        }
        let cid_set_len = cid_set_data.len();
        doc.set_object(Object::new(
            cid_set_num,
            0,
            Value::Stream(
                Dict::new().insert("Length", Value::Integer(cid_set_len as i64)),
                StreamData::Owned(cid_set_data),
            ),
        ));
    }
    // 嵌入 CIDFont 字体程序：Type2 → /FontFile2，Type0 → /FontFile3
    // （流 /Subtype /CIDFontType0C）。builder 的 Owned 流写明文。
    // TrueType 子集化后 glyph ID 会重排，需要用新映射替换用户提供的
    // /CIDToGIDMap（Identity 也不例外）。
    let mut cid_to_gid_override: Option<Vec<u16>> = None;
    if let Some(program) = &cid_font.font_program {
        // (描述符键, FontFile3 流 /Subtype, 是否写 Length1)
        let (program_key, stream_subtype, needs_length1) =
            match (cid_font.subtype, program, open_type_flavor) {
                (CidFontSubtype::CIDFontType2, FontProgram::TrueType(_), _) => {
                    ("FontFile2", None, true)
                }
                (CidFontSubtype::CIDFontType0, FontProgram::CFF(_), _) => {
                    ("FontFile3", Some("CIDFontType0C"), false)
                }
                (
                    CidFontSubtype::CIDFontType0,
                    FontProgram::OpenType(_),
                    Some(OpenTypeFlavor::Cff),
                ) => ("FontFile3", Some("OpenType"), false),
                (
                    CidFontSubtype::CIDFontType2,
                    FontProgram::OpenType(_),
                    Some(OpenTypeFlavor::TrueType),
                ) => ("FontFile2", None, true),
                (
                    CidFontSubtype::CIDFontType0,
                    FontProgram::OpenType(_),
                    Some(OpenTypeFlavor::TrueType),
                ) => {
                    return Err(WriteError::InvalidObject(format!(
                        "CIDFontType0 requires a CFF-flavored OpenType program, \
                         but /BaseFont {} embeds a TrueType-flavored one",
                        cid_font.name
                    )));
                }
                (
                    CidFontSubtype::CIDFontType2,
                    FontProgram::OpenType(_),
                    Some(OpenTypeFlavor::Cff),
                ) => {
                    return Err(WriteError::InvalidObject(format!(
                        "CIDFontType2 requires a TrueType-flavored OpenType program, \
                         but /BaseFont {} embeds a CFF-flavored one",
                        cid_font.name
                    )));
                }
                (subtype, _, _) => {
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
        // CID 子集化：写流前先用子集重写结果替换原始 CFF 程序字节
        // （PDF 32000-1 §9.6.4）；类型组合已在上方校验。
        let subset_program = match (&cid_font.subset_cids, program) {
            (Some(cids), FontProgram::CFF(data)) => {
                Some(subset_cff(data, cids).map_err(|error| {
                    WriteError::InvalidObject(format!(
                        "CFF subsetting for /BaseFont {} failed: {error}",
                        cid_font.name
                    ))
                })?)
            }
            (Some(cids), FontProgram::TrueType(data)) => {
                let glyphs = subset_glyph_ids(cid_font, cids)?;
                let (subset, mapping) =
                    subset_truetype_with_mapping(data, &glyphs).map_err(|error| {
                        WriteError::InvalidObject(format!(
                            "TrueType subsetting for /BaseFont {} failed: {error}",
                            cid_font.name
                        ))
                    })?;
                cid_to_gid_override = Some(remap_cid_to_gid(cids, &glyphs, &mapping));
                Some(subset)
            }
            _ => None,
        };
        let program_bytes = subset_program.as_deref().unwrap_or_else(|| program.data());
        let mut stream_dict = Dict::new();
        if let Some(subtype) = stream_subtype {
            stream_dict = stream_dict.insert("Subtype", Value::Name(subtype.into()));
        }
        // Table 127：FontFile2 的 TrueType 程序（含 TrueType-flavored
        // OpenType）需 Length1；FontFile3 不得带 Length1/2/3。
        if needs_length1 {
            stream_dict = stream_dict.insert("Length1", Value::Integer(program_bytes.len() as i64));
        }
        stream_dict = stream_dict.insert("Length", Value::Integer(program_bytes.len() as i64));
        doc.set_object(Object::new(
            program_num,
            0,
            Value::Stream(stream_dict, StreamData::Owned(program_bytes.to_vec())),
        ));
        if let Value::Dict(d) = descriptor_value {
            descriptor_value = Value::Dict(d.insert(program_key, Value::Ref(program_num, 0)));
        }
    }
    doc.set_object(Object::new(descriptor_num, 0, descriptor_value));

    // /CIDToGIDMap 流：按 CID 索引的 2 字节大端 glyph ID 数组。
    // 子集化重排 glyph ID 后优先使用重映射结果。
    let mut cid_to_gid_ref = None;
    if let Some(mapping) = cid_to_gid_override
        .as_deref()
        .or(cid_font.cid_to_gid.as_deref())
    {
        let map_num = num;
        num += 1;
        let mut bytes = Vec::with_capacity(mapping.len() * 2);
        for gid in mapping {
            bytes.extend_from_slice(&gid.to_be_bytes());
        }
        let map_len = bytes.len();
        doc.set_object(Object::new(
            map_num,
            0,
            Value::Stream(
                Dict::new().insert("Length", Value::Integer(map_len as i64)),
                StreamData::Owned(bytes),
            ),
        ));
        cid_to_gid_ref = Some(map_num);
    }

    // 2. CIDFont 后代字典
    let cid_font_num = num;
    num += 1;
    let mut cid_dict = Dict::new()
        .insert("Type", Value::Name("Font".into()))
        .insert("Subtype", Value::Name(cid_font.subtype.as_name().into()))
        .insert("BaseFont", Value::Name(base_font_name.clone()))
        .insert("CIDSystemInfo", cid_font.system_info.to_value())
        .insert("FontDescriptor", Value::Ref(descriptor_num, 0));
    if (cid_font.default_width - 1000.0).abs() > f64::EPSILON {
        cid_dict = cid_dict.insert("DW", Value::Float(cid_font.default_width));
    }
    if !cid_font.widths.is_empty() {
        cid_dict = cid_dict.insert("W", cid_font.widths_to_value());
    }
    let [vertical_displacement, vertical_advance] = cid_font.default_vertical_metrics;
    if (vertical_displacement - 880.0).abs() > f64::EPSILON
        || (vertical_advance + 1000.0).abs() > f64::EPSILON
    {
        cid_dict = cid_dict.insert(
            "DW2",
            Value::Array(
                Array::new()
                    .push(Value::Float(vertical_displacement))
                    .push(Value::Float(vertical_advance)),
            ),
        );
    }
    if !cid_font.vertical_metrics.is_empty() {
        cid_dict = cid_dict.insert("W2", cid_font.vertical_metrics_to_value());
    }
    if let Some(map_num) = cid_to_gid_ref {
        cid_dict = cid_dict.insert("CIDToGIDMap", Value::Ref(map_num, 0));
    }
    doc.set_object(Object::new(cid_font_num, 0, Value::Dict(cid_dict)));

    // 3. Type 0 父字体字典；有 CID→Unicode 映射时附带 /ToUnicode CMap
    let mut type0_dict = Dict::new()
        .insert("Type", Value::Name("Font".into()))
        .insert("Subtype", Value::Name("Type0".into()))
        .insert("BaseFont", Value::Name(base_font_name))
        .insert("Encoding", Value::Name("Identity-H".into()))
        .insert(
            "DescendantFonts",
            Value::Array(Array::new().push(Value::Ref(cid_font_num, 0))),
        );
    if let Some(name) = &cid_font.predefined_encoding {
        // 预定义 CMap：/Encoding 直接写名称引用，不生成嵌入 CMap 流。
        // 名称已在上方按读端名册校验，只含安全的 Name 字符。
        type0_dict = type0_dict.insert("Encoding", Value::Name(name.clone()));
    } else if let Some(encoding) = &cid_font.encoding {
        // 非 Identity 编码：嵌入 CMap 流作为 /Encoding 引用。
        let encoding_num = num;
        num += 1;
        let cmap = cid_encoding_cmap(encoding)?;
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
    if !cid_font.to_unicode.is_empty() || !cid_font.to_unicode_bytes.is_empty() {
        let cmap_num = num;
        num += 1;
        let cmap = to_unicode_cmap(&cid_font.to_unicode, &cid_font.to_unicode_bytes);
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

/// 校验名称可作为不含前导 `/` 的 PDF Name 原样写出：非空、仅可打印
/// ASCII，且不含空白、分隔符与 `#`（`#` 在 Name 中表示转义前缀）。
fn is_valid_pdf_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            (0x21..=0x7E).contains(&byte)
                && !matches!(
                    byte,
                    b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%' | b'#'
                )
        })
}

/// 把子集 CID 列表经 `/CIDToGIDMap`（缺省 Identity）翻译为 glyph ID。
///
/// 显式映射缺少条目时返回 [`WriteError::InvalidObject`]；调用方已在
/// 子集校验阶段确认每个 CID 都落在映射范围内。
fn subset_glyph_ids(cid_font: &CidFont, cids: &[u16]) -> Result<Vec<u16>, WriteError> {
    cids.iter()
        .map(|cid| match &cid_font.cid_to_gid {
            Some(mapping) => mapping.get(usize::from(*cid)).copied().ok_or_else(|| {
                WriteError::InvalidObject(format!(
                    "subset CID {cid} for /BaseFont {} is outside its {} entry /CIDToGIDMap",
                    cid_font.name,
                    mapping.len()
                ))
            }),
            None => Ok(*cid),
        })
        .collect()
}

/// 按子集后的旧→新 glyph 映射重建 CIDToGIDMap（CID 索引，未保留的 CID 为 0）。
fn remap_cid_to_gid(cids: &[u16], glyphs: &[u16], old_to_new: &BTreeMap<u16, u16>) -> Vec<u16> {
    let max_cid = cids.iter().copied().max().unwrap_or(0);
    let mut map = vec![0u16; usize::from(max_cid) + 1];
    for (&cid, &old_gid) in cids.iter().zip(glyphs) {
        if let Some(&new_gid) = old_to_new.get(&old_gid) {
            map[usize::from(cid)] = new_gid;
        }
    }
    map
}

fn program_kind(program: &Option<FontProgram>) -> &'static str {
    match program {
        Some(FontProgram::Type1(_)) => "a Type1 program",
        Some(FontProgram::TrueType(_)) => "a TrueType program",
        Some(FontProgram::CFF(_)) => "a CFF program",
        Some(FontProgram::OpenType(_)) => "an OpenType program",
        None => "no embedded program",
    }
}

/// 按子集 CID 交集裁剪 `/CIDSet` 位图（MSB-first）：CID 0 恒保留，
/// 其余仅保留原位图与子集请求的交集。长度与原位图一致。
fn mask_cid_set(data: &[u8], cids: &[u16]) -> Vec<u8> {
    let mut out = vec![0u8; data.len()];
    let mut keep = |cid: u16| {
        let byte = usize::from(cid / 8);
        if byte < out.len() {
            out[byte] |= 0x80 >> (cid % 8);
        }
    };
    keep(0);
    for &cid in cids {
        // 与原位图取交集：原位图未标记的 CID 不补位。
        let byte = usize::from(cid / 8);
        let present = data
            .get(byte)
            .is_some_and(|bits| bits & (0x80 >> (cid % 8)) != 0);
        if present {
            keep(cid);
        }
    }
    out
}
