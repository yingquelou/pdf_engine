//! Type 3 font dictionary parsing.
//!
//! A Type 3 font describes its glyphs as content streams (the `/CharProcs`
//! procedures) instead of an embedded font program. This module resolves the
//! dictionary metadata needed by readers and text extraction: font matrix,
//! bounding box, character widths, `/Encoding`, and the set of glyph
//! procedure names. The leading `d0`/`d1` operator of each glyph procedure is
//! interpreted on demand via [`Type3Font::glyph_metrics`]; executing the
//! remaining graphics operators of a glyph procedure is out of scope here.

mod char_proc;

use std::collections::BTreeSet;

use crate::core::types::{Dict, Document, Value};

use super::{
    FontDescriptorMetrics, FontError, FontResult, SimpleFontEncoding, number, parse_descriptor,
};

/// `/Resources` 间接引用链的最大深度（PDF 32000-1 Table 112）。
const MAX_RESOURCE_CHAIN: usize = 16;

/// Resolved metadata from a Type 3 font dictionary (PDF 32000-1 §9.6.5).
#[derive(Debug, Clone, PartialEq)]
pub struct Type3Font {
    /// Font matrix mapping glyph space to text space (`/FontMatrix`).
    pub font_matrix: [f64; 6],
    /// Glyph bounding box in glyph space (`/FontBBox`).
    pub font_bbox: [f64; 4],
    /// First character code covered by `/Widths`.
    pub first_char: u8,
    /// Raw glyph-space widths from `/Widths`, indexed from `first_char`.
    ///
    /// These are glyph-space values: apply [`Self::font_matrix`] to convert
    /// them to text space. Rendered width also depends on the `d0`/`d1`
    /// operators inside each character procedure, which are not interpreted.
    pub widths: Vec<f64>,
    /// Parsed `/Encoding` (base encoding plus `/Differences`).
    pub encoding: SimpleFontEncoding,
    /// Glyph procedure names from `/CharProcs`, sorted ascending.
    pub char_procs: Vec<String>,
    /// 可选 `/Name`（Table 112；PDF 1.0 必需、后续版本可选）。
    pub name: Option<String>,
    /// 可选 `/FontDescriptor` 度量（Table 112；Tagged PDF 必需）。
    pub descriptor: Option<FontDescriptorMetrics>,
    /// 是否提供 `/ToUnicode` 流（Table 112 可选）。
    ///
    /// 只记录条目存在且解析为流；CMap 内容由文本提取经
    /// `Document::simple_font_to_unicode` 解析。
    pub has_to_unicode: bool,
    /// 已解析（解引用一层）的 `/CharProcs` 字典：字形名 → 字形程序
    /// （`Value::Stream` 或指向流对象的 `Value::Ref`），
    /// 供 [`Self::glyph_metrics`] 按需解码。
    char_proc_streams: Dict,
    /// 原样保存的 `/Resources` 值：直接字典或指向字典的间接引用，
    /// 由 [`Self::resources`] 在查询时解引用。`None` 表示字体字典
    /// 未提供该条目。
    resources: Option<Value>,
}

/// Type 3 字形程序开头 `d0`/`d1` 操作符声明的指标（PDF 32000-1 §9.6.5）。
///
/// 所有数值均为 glyph 空间坐标；换算到 text space 需施加
/// [`Type3Font::font_matrix`]。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Type3GlyphMetrics {
    /// 声明宽度 `(wx, wy)`。
    pub width: [f64; 2],
    /// `d1` 声明的包围盒 `(llx, lly, urx, ury)`；`d0` 无包围盒，为 `None`。
    pub bbox: Option<[f64; 4]>,
}

impl Type3Font {
    /// Return the raw glyph-space width for a one-byte character code.
    pub fn width_for(&self, code: u8) -> Option<f64> {
        let index = usize::from(code).checked_sub(usize::from(self.first_char))?;
        self.widths.get(index).copied()
    }

    /// 按字符码查询字形程序开头 `d0`/`d1` 声明的指标（PDF 32000-1 §9.6.5）。
    ///
    /// 字符码先经 `/Encoding` 解析为字形名，再到 `/CharProcs` 中取出对应流，
    /// 经 [`Document`] 现有解码路径（含 `/Filter`）解码后，只解释第一个
    /// 操作符：跳过前导空白与注释后，它必须是 `d0`（2 个数值操作数，仅
    /// 声明宽度）或 `d1`（6 个数值操作数，声明宽度与包围盒）。
    ///
    /// 返回 `Ok(None)` 表示 `/Encoding` 未给该字符码分配字形名（无从定位
    /// 字形程序）。`/Widths` 的原始宽度与这里的声明宽度是两个独立数据源，
    /// 两者均可查询，本方法不做一致性校验。
    ///
    /// 以下情况返回 [`FontError::Invalid`]：`/CharProcs` 缺少该字形条目、
    /// 条目不是流或间接引用无法解析、流解码失败、首操作符缺失或不是
    /// `d0`/`d1`、操作数个数/类型不符、首个操作符前出现非数值 token。
    pub fn glyph_metrics(
        &self,
        document: &Document,
        code: u8,
    ) -> FontResult<Option<Type3GlyphMetrics>> {
        let Some(name) = self.encoding.glyph_name_for(code) else {
            return Ok(None);
        };
        let value = self.char_proc_streams.get(&name).ok_or_else(|| {
            FontError::Invalid(format!("/CharProcs has no entry for glyph /{name}"))
        })?;
        let value = document.resolve_value(value)?;
        let Value::Stream(_, _) = value else {
            return Err(FontError::Invalid(format!(
                "character procedure for glyph /{name} is not a stream"
            )));
        };
        let bytes = document.decode_stream_bytes(&value).map_err(|error| {
            FontError::Invalid(format!(
                "cannot decode character procedure for glyph /{name}: {error}"
            ))
        })?;
        char_proc::scan_glyph_metrics_header(&bytes, &name).map(Some)
    }

    /// 查询字体字典的 `/Resources`（PDF 32000-1 Table 112）。
    ///
    /// Type 3 字体的字形程序是内容流，可引用 `/Resources` 中的
    /// `/XObject`、`/ColorSpace`、`/Pattern` 等资源。字体字典未提供
    /// `/Resources` 时返回 `Ok(None)`；直接字典原样返回，间接引用沿
    /// 引用链解引用（最多 [`MAX_RESOURCE_CHAIN`] 层）。
    ///
    /// 返回 [`FontError::Invalid`] 的情况：间接引用无法解析、出现引用环、
    /// 超出深度上限、`/Resources`（或链终点）不是字典。
    pub fn resources(&self, document: &Document) -> FontResult<Option<Dict>> {
        let Some(value) = &self.resources else {
            return Ok(None);
        };
        let value = resolve_chain(document, value, "/Resources")?;
        value.as_dict().cloned().map(Some).ok_or_else(|| {
            FontError::Invalid("/Resources in Type 3 font is not a dictionary".to_string())
        })
    }

    /// 按类别与名称查询字体级资源（PDF 32000-1 Table 112 + 7.3.5）。
    ///
    /// `category` 是资源类别键（如 `"XObject"`、`"ColorSpace"`），
    /// `name` 是资源名（不含前导 `/`）。类别字典本身可为间接引用链。
    /// 名称先精确匹配，再按 `#xx` 转义解码后匹配；返回值可能仍是间接
    /// 引用，由调用方按需解引用。
    ///
    /// 未提供 `/Resources`、类别缺失或类别不是字典时返回 `Ok(None)`；
    /// `/Resources` 链本身非法时返回与 [`Self::resources`] 相同的错误。
    pub fn resource(
        &self,
        document: &Document,
        category: &str,
        name: &str,
    ) -> FontResult<Option<Value>> {
        let Some(resources) = self.resources(document)? else {
            return Ok(None);
        };
        let Some(category_value) = resources.get(category) else {
            return Ok(None);
        };
        let category_value = resolve_chain(document, category_value, "/Resources category")?;
        let Some(category_dict) = category_value.as_dict() else {
            return Ok(None);
        };
        if let Some(value) = category_dict.get(name) {
            return Ok(Some(value.clone()));
        }
        for (key, value) in category_dict.0.iter() {
            if crate::core::reader::page_content::decode_name_escapes(key) == name {
                return Ok(Some(value.clone()));
            }
        }
        Ok(None)
    }
}

impl Document {
    /// Resolve a Type 3 font dictionary from direct or indirect values.
    ///
    /// Returns `Ok(None)` for non-Type-3 font dictionaries. Type 3 fonts have
    /// no `/BaseFont`; `/FontBBox`, `/FontMatrix`, `/CharProcs`, `/Encoding`,
    /// `/FirstChar`, `/LastChar`, and `/Widths` are required by this method,
    /// and `/Widths` must contain exactly `LastChar - FirstChar + 1` entries.
    ///
    /// `/Encoding` must be present (PDF 32000-1 §9.6.5.2). A name selects
    /// that predefined encoding; a dictionary selects `/BaseEncoding` when
    /// present and otherwise falls back to StandardEncoding, because Type 3
    /// has no `/BaseFont` from which to derive a default. This fallback is an
    /// approximation for encoding dictionaries that omit `/BaseEncoding`.
    ///
    /// `/Resources` is optional and kept as-is; query it with
    /// [`Type3Font::resources`], which resolves indirect references.
    ///
    /// The character procedures' graphics operators are not executed; their
    /// leading `d0`/`d1` declarations can be queried per character code via
    /// [`Type3Font::glyph_metrics`].
    pub fn type3_font(&self, value: &Value) -> FontResult<Option<Type3Font>> {
        let value = self.resolve_value(value)?;
        let dict = value
            .as_dict()
            .ok_or_else(|| FontError::Invalid("font value is not a dictionary".to_string()))?;
        if dict.get("Subtype").and_then(Value::as_name) != Some("Type3") {
            return Ok(None);
        }

        let font_bbox = self.read_type3_numbers::<4>(dict, "FontBBox")?;
        let font_matrix = self.read_type3_numbers::<6>(dict, "FontMatrix")?;

        let (first_char, widths) = self.read_widths(dict)?;
        let first_char = first_char.ok_or_else(|| {
            FontError::Invalid(
                "Type 3 fonts require /FirstChar, /LastChar, and /Widths".to_string(),
            )
        })?;

        let char_procs_value = dict
            .get("CharProcs")
            .ok_or_else(|| FontError::Invalid("missing /CharProcs in Type 3 font".to_string()))?;
        let char_procs_value = self.resolve_value(char_procs_value)?;
        let char_procs = char_procs_value
            .as_dict()
            .ok_or_else(|| FontError::Invalid("/CharProcs is not a dictionary".to_string()))?;
        let char_proc_streams = char_procs.clone();
        let char_procs = char_procs.0.keys().cloned().collect();

        let encoding = self
            .simple_font_encoding(&value)?
            .ok_or_else(|| FontError::Invalid("Type 3 fonts require /Encoding".to_string()))?;

        // `/Resources` 可选；原样保存，查询时由 `resources` 解引用。
        let resources = dict.get("Resources").cloned();

        // Table 112：`/Name` 与 `/FontDescriptor` 可选（后者在 Tagged PDF
        // 中必需），`/ToUnicode` 可选；条目存在时按各自类型严格校验。
        let name = match dict.get("Name") {
            Some(value) => {
                let value = self.resolve_value(value)?;
                Some(
                    value
                        .as_name()
                        .ok_or_else(|| {
                            FontError::Invalid("/Name in Type 3 font is not a name".to_string())
                        })?
                        .to_string(),
                )
            }
            None => None,
        };
        let descriptor = match dict.get("FontDescriptor") {
            Some(value) => {
                let value = self.resolve_value(value)?;
                let descriptor = value.as_dict().ok_or_else(|| {
                    FontError::Invalid("/FontDescriptor is not a dictionary".to_string())
                })?;
                Some(parse_descriptor(self, descriptor)?)
            }
            None => None,
        };
        let has_to_unicode = match dict.get("ToUnicode") {
            Some(value) => {
                let value = self.resolve_value(value)?;
                if !matches!(value, Value::Stream(_, _)) {
                    return Err(FontError::Invalid(
                        "/ToUnicode in Type 3 font is not a stream".to_string(),
                    ));
                }
                true
            }
            None => false,
        };

        Ok(Some(Type3Font {
            font_matrix,
            font_bbox,
            first_char,
            widths,
            encoding,
            char_procs,
            name,
            descriptor,
            has_to_unicode,
            char_proc_streams,
            resources,
        }))
    }

    fn read_type3_numbers<const N: usize>(&self, dict: &Dict, key: &str) -> FontResult<[f64; N]> {
        let value = dict
            .get(key)
            .ok_or_else(|| FontError::Invalid(format!("missing /{key} in Type 3 font")))?;
        let value = self.resolve_value(value)?;
        let array = value
            .as_array()
            .ok_or_else(|| FontError::Invalid(format!("/{key} is not an array")))?;
        if array.len() != N {
            return Err(FontError::Invalid(format!(
                "/{key} contains {} entries; expected {N}",
                array.len()
            )));
        }
        let mut numbers = [0.0; N];
        for (slot, value) in numbers.iter_mut().zip(array.iter()) {
            *slot = number(value).ok_or_else(|| {
                FontError::Invalid(format!("/{key} contains a non-finite or non-numeric value"))
            })?;
        }
        Ok(numbers)
    }
}

/// 沿间接引用链解析 `value`，最多 [`MAX_RESOURCE_CHAIN`] 层。
fn resolve_chain(document: &Document, value: &Value, label: &str) -> FontResult<Value> {
    let mut current = value.clone();
    let mut visited = BTreeSet::new();
    for _ in 0..=MAX_RESOURCE_CHAIN {
        let Value::Ref(num, generation) = current else {
            return Ok(current);
        };
        if !visited.insert((num, generation)) {
            return Err(FontError::Invalid(format!(
                "{label} in Type 3 font has a reference cycle at {num} {generation} R"
            )));
        }
        let object = document.get_object(num)?.ok_or_else(|| {
            FontError::Invalid(format!("unresolved object reference {num} {generation} R"))
        })?;
        current = object.value;
    }
    Err(FontError::Invalid(format!(
        "{label} in Type 3 font exceeds {MAX_RESOURCE_CHAIN} indirect references"
    )))
}

#[cfg(test)]
mod tests;
