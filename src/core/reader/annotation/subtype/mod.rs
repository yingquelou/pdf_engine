//! 注解子类型专有条目解析（12.5.6）。
//!
//! 通用条目由父模块解析；此处只按各子类型的表格提取专有条目，
//! 读端保持宽容：条目存在即解析，不校验声明的 `/Subtype`
//! （归属校验由写端 `PdfAnnotation::validate` 负责），非标准的端点样式、
//! 意图与题注位置名一律落成 `Other` 变体保留原文。
//!
//! - Text（Table 172）：`/Name`、`/Open`、`/State`、`/StateModel`
//! - Caret（Table 180）：`/Symbol`
//! - Highlight/Underline/Squiggly/StrikeOut（Table 179）：`/QuadPoints`
//!   按 8 个数一组类型化（[`Annotation::text_markup_quad_groups`]），
//!   四型共用同一组条目；规范仅要求每组逆时针四点、AP 存在时优先，
//!   未对 `/QuadPoints` 与 `/Rect` 的包含关系作 shall/should（见下注）
//! - Stamp（Table 181）：`/Name` 图章图标名类型化为 [`StampIcon`]，
//!   14 个标准名之外的名字原样保留在 [`StampIcon::Other`] 且可判别；
//!   缺省图标为 `Draft`（Table 181 原文，`/Name` 本身无版本标注）
//! - Ink（Table 182）：`/InkList` 路径
//! - Line（Table 175/176）：`/L`、`/LE`、`/LL`、`/LLE`、`/Cap`、`/IT`、
//!   `/LLO`、`/CP`、`/CO`（`/IC` 属通用条目，见 `Annotation::interior_color`）
//! - FreeText（Table 174 + Table 167）：`/DA`、`/Q`、`/DS`、`/CL`、`/IT`、
//!   `/BE`、`/RD`、`/LE`（`/RC` 属通用条目，`/BS` 属 12.5.4 边框）
//! - Square/Circle（Table 177）：`/BE`、`/RD`（`/IC` 属通用条目）
//! - Polygon/PolyLine（Table 178）：`/Vertices`、`/LE`、`/BE`、`/IT`
//! - Link（Table 173）：`/H` 高亮模式、`/PA` 按下 URI 动作（解析在 [`super::link`]）
//! - Popup（Table 183）：`/Open` 与 Text 共用（`/Parent` 由父模块按通用引用解析）
//!
//! `/LE` 有两种形态：Line/Polygon/PolyLine 是 1–2 个名字的数组，
//! FreeText 是单个名字。解析不校验声明的 `/Subtype`，因此数组形态
//! 只填 [`SubtypeEntries::line_endings`]、裸名字只填
//! [`SubtypeEntries::callout_ending`]，两者互不报错。
//!
//! `/IT` 归 Line、FreeText 与 Polygon/PolyLine 三家，名字集合互不相交：
//! 命中哪一家就填哪一家；三家都不认识的名字才按声明的 `/Subtype` 归属决定
//! 落进哪个 `Other`（`FreeText` 归 FreeText，`Polygon`/`PolyLine` 归
//! Polygon，其余仍按 Line 的 `Other` 保留原文）。
//!
//! `/BE` 与 `/RD` 是 FreeText 与几何类注解共用的键：`/BE` 为 Table 167
//! 边框效果（FreeText PDF 1.6，Square/Circle/Polygon PDF 1.5），`/RD` 为
//! Rect 与实际边界的四边差值（FreeText PDF 1.6，Square/Circle PDF 1.5）。

mod line;
mod markup;
mod text_free;

pub use self::line::{CaptionPosition, LineEndStyle, LineIntent, PolygonIntent};
pub use self::markup::StampIcon;
pub use self::text_free::{BorderEffect, BorderEffectStyle, CalloutLine, FreeTextIntent};

use self::line::{parse_bare_line_ending, parse_line_endings, parse_vertices};
use self::markup::parse_ink_list;
use self::text_free::{
    parse_border_effect, parse_callout_line, parse_quadding, parse_text_rectangle,
};
use super::{
    Result,
    entries::{parse_number_array, parse_optional_bool, parse_optional_number},
    link::{LinkEntries, parse_link_entries},
    malformed,
};
use crate::core::types::{Dict, Value};

/// `/InkList` 路径数上限（防护不可信文件）。
const MAX_INK_PATHS: usize = 4096;
/// 单条墨迹路径的坐标数值上限（防护不可信文件）。
const MAX_INK_PATH_VALUES: usize = 8192;
/// Polygon/PolyLine `/Vertices` 的坐标数值上限（防护不可信文件）。
const MAX_VERTEX_VALUES: usize = 8192;

/// 一次子类型专有条目解析的结果，字段由父模块并入 [`super::Annotation`]。
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct SubtypeEntries {
    pub(super) icon_name: Option<String>,
    pub(super) is_open: Option<bool>,
    pub(super) state: Option<String>,
    pub(super) state_model: Option<String>,
    pub(super) symbol: Option<String>,
    pub(super) ink_paths: Vec<Vec<f64>>,
    pub(super) line_endpoints: Option<[f64; 4]>,
    pub(super) line_endings: Option<[LineEndStyle; 2]>,
    pub(super) leader_length: Option<f64>,
    pub(super) leader_extension: Option<f64>,
    pub(super) caption: Option<bool>,
    pub(super) line_intent: Option<LineIntent>,
    pub(super) leader_line_offset: Option<f64>,
    pub(super) caption_position: Option<CaptionPosition>,
    pub(super) caption_offset: Option<[f64; 2]>,
    pub(super) default_appearance: Option<String>,
    pub(super) quadding: Option<u8>,
    pub(super) default_style: Option<String>,
    pub(super) callout_line: Option<CalloutLine>,
    pub(super) free_text_intent: Option<FreeTextIntent>,
    pub(super) border_effect: Option<BorderEffect>,
    pub(super) text_rectangle: Option<[f64; 4]>,
    pub(super) callout_ending: Option<LineEndStyle>,
    pub(super) vertices: Option<Vec<f64>>,
    pub(super) polygon_intent: Option<PolygonIntent>,
    pub(super) link: LinkEntries,
}

/// 解析注解字典中的子类型专有条目。
pub(super) fn parse_subtype_entries(dict: &Dict) -> Result<SubtypeEntries> {
    let (line_intent, free_text_intent, polygon_intent) = parse_intents(dict)?;
    Ok(SubtypeEntries {
        icon_name: parse_name_or_text(dict, "Name")?,
        is_open: match dict.get("Open") {
            None => None,
            Some(Value::Bool(flag)) => Some(*flag),
            Some(_) => return Err(malformed("annotation /Open must be a boolean")),
        },
        state: parse_name_or_text(dict, "State")?,
        state_model: parse_name_or_text(dict, "StateModel")?,
        symbol: parse_name_or_text(dict, "Symbol")?,
        ink_paths: parse_ink_list(dict)?,
        line_endpoints: parse_exact_numbers(dict, "L", 4)?.map(|v| [v[0], v[1], v[2], v[3]]),
        line_endings: parse_line_endings(dict)?,
        leader_length: parse_optional_number(dict, "LL")?,
        leader_extension: parse_optional_number(dict, "LLE")?,
        caption: parse_optional_bool(dict, "Cap")?,
        line_intent,
        leader_line_offset: parse_optional_number(dict, "LLO")?,
        caption_position: parse_name_or_text(dict, "CP")?
            .map(|name| CaptionPosition::from_name(&name)),
        caption_offset: parse_exact_numbers(dict, "CO", 2)?.map(|v| [v[0], v[1]]),
        default_appearance: parse_name_or_text(dict, "DA")?,
        quadding: parse_quadding(dict)?,
        default_style: parse_name_or_text(dict, "DS")?,
        callout_line: parse_callout_line(dict)?,
        free_text_intent,
        border_effect: parse_border_effect(dict)?,
        text_rectangle: parse_text_rectangle(dict)?,
        callout_ending: parse_bare_line_ending(dict)?,
        vertices: parse_vertices(dict)?,
        polygon_intent,
        link: parse_link_entries(dict)?,
    })
}

/// `/IT` 同时属于 Line（Table 175）、FreeText（Table 174）与
/// Polygon/PolyLine（Table 178），三集合互不相交：命中哪一侧就填哪一侧；
/// 都不认识的名字按声明的 `/Subtype` 归属路由，缺省落进
/// [`LineIntent::Other`] 保留原文。
fn parse_intents(
    dict: &Dict,
) -> Result<(
    Option<LineIntent>,
    Option<FreeTextIntent>,
    Option<PolygonIntent>,
)> {
    let Some(name) = parse_name_or_text(dict, "IT")? else {
        return Ok((None, None, None));
    };
    let free_text = FreeTextIntent::from_name(&name);
    if !matches!(free_text, FreeTextIntent::Other(_)) {
        return Ok((None, Some(free_text), None));
    }
    let line = LineIntent::from_name(&name);
    if !matches!(line, LineIntent::Other(_)) {
        return Ok((Some(line), None, None));
    }
    let polygon = PolygonIntent::from_name(&name);
    if !matches!(polygon, PolygonIntent::Other(_)) {
        return Ok((None, None, Some(polygon)));
    }
    match dict.get("Subtype").and_then(Value::as_name) {
        Some("FreeText") => Ok((None, Some(free_text), None)),
        Some("Polygon" | "PolyLine") => Ok((None, None, Some(polygon))),
        _ => Ok((Some(line), None, None)),
    }
}

/// 恰好 `len` 个有限数值的数组条目（Line `/L` 四项、`/CO` 两项）。
fn parse_exact_numbers(dict: &Dict, key: &str, len: usize) -> Result<Option<Vec<f64>>> {
    let Some(values) = parse_number_array(dict, key, false)? else {
        return Ok(None);
    };
    if values.len() != len {
        return Err(malformed(format!(
            "annotation /{key} must hold {len} numbers"
        )));
    }
    Ok(Some(values))
}

/// 名字或文本串条目（Text 注解 `/Name`/`/State`/`/StateModel`、Caret `/Symbol`）
/// 统一解码为 `String`；文本串按 `/DF`/UTF-16BE/PDF 文本串规则解码。
fn parse_name_or_text(dict: &Dict, key: &str) -> Result<Option<String>> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    if let Some(name) = value.as_name() {
        return Ok(Some(name.to_string()));
    }
    let decoded = crate::core::reader::content::text_value(value)
        .ok_or_else(|| malformed(format!("annotation /{key} must be a name or text string")))?;
    Ok(Some(decoded))
}
