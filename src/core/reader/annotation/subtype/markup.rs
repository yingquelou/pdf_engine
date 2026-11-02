//! Text 标记类与 Rubber Stamp、Ink 子类型专有条目（Table 172/179/181/182）：
//! 图章图标名类型化、`/InkList` 路径解析，及 `/QuadPoints` 与图章的类型化访问。

use super::super::{Annotation, AnnotationSubtype, Result, entries::number, malformed};
use super::{MAX_INK_PATH_VALUES, MAX_INK_PATHS};
use crate::core::types::Dict;

/// Rubber Stamp 注解图标名（`/Name`，Table 181，12.5.6.12）。
///
/// 规范列出查看器至少须提供图标的 14 个标准名，并允许「Additional
/// names may be supported as well」——未收录的名字原样保留在
/// [`StampIcon::Other`]，可经 [`StampIcon::is_standard`] 判别。
/// `/Name` 缺省值为 `Draft`（Table 181 原文；条目本身无 PDF 版本标注）。
/// `/AP` 存在时外观流优先于本条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StampIcon {
    /// `/Approved`。
    Approved,
    /// `/Experimental`。
    Experimental,
    /// `/NotApproved`。
    NotApproved,
    /// `/AsIs`。
    AsIs,
    /// `/Expired`。
    Expired,
    /// `/NotForPublicRelease`。
    NotForPublicRelease,
    /// `/Confidential`。
    Confidential,
    /// `/Final`。
    Final,
    /// `/Sold`。
    Sold,
    /// `/Departmental`。
    Departmental,
    /// `/ForComment`。
    ForComment,
    /// `/TopSecret`。
    TopSecret,
    /// `/Draft`（Table 181 缺省）。
    Draft,
    /// `/ForPublicRelease`。
    ForPublicRelease,
    /// 标准名之外的名字（原样保留）。
    Other(String),
}

impl StampIcon {
    /// 由 `/Name` 名字解析；非标准名返回 [`StampIcon::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "Approved" => Self::Approved,
            "Experimental" => Self::Experimental,
            "NotApproved" => Self::NotApproved,
            "AsIs" => Self::AsIs,
            "Expired" => Self::Expired,
            "NotForPublicRelease" => Self::NotForPublicRelease,
            "Confidential" => Self::Confidential,
            "Final" => Self::Final,
            "Sold" => Self::Sold,
            "Departmental" => Self::Departmental,
            "ForComment" => Self::ForComment,
            "TopSecret" => Self::TopSecret,
            "Draft" => Self::Draft,
            "ForPublicRelease" => Self::ForPublicRelease,
            other => Self::Other(other.to_string()),
        }
    }

    /// 标准名（`/Name` 值）；[`StampIcon::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::Approved => "Approved",
            Self::Experimental => "Experimental",
            Self::NotApproved => "NotApproved",
            Self::AsIs => "AsIs",
            Self::Expired => "Expired",
            Self::NotForPublicRelease => "NotForPublicRelease",
            Self::Confidential => "Confidential",
            Self::Final => "Final",
            Self::Sold => "Sold",
            Self::Departmental => "Departmental",
            Self::ForComment => "ForComment",
            Self::TopSecret => "TopSecret",
            Self::Draft => "Draft",
            Self::ForPublicRelease => "ForPublicRelease",
            Self::Other(other) => other,
        }
    }

    /// 是否命中 Table 181 的 14 个标准图章名。
    pub fn is_standard(&self) -> bool {
        !matches!(self, Self::Other(_))
    }
}

impl Annotation {
    /// 文本标记注解（Highlight/Underline/Squiggly/StrikeOut，Table 179）的
    /// `/QuadPoints` 按 8 个数一组的四边形返回，顶点保持逆时针原序
    /// `(x1, y1) … (x4, y4)`；非文本标记子类型返回 `None`（Link/Redact 的
    /// `/QuadPoints` 经 [`Annotation::quad_groups`] 访问），无条目返回 `None`。
    ///
    /// Table 179 未对 `/QuadPoints` 与 `/Rect` 的包含关系作 shall/should
    /// （仅要求每个四边形须 encompass 底层文字、`/AP` 存在时优先），
    /// 因此读端不校验、也不报告越界四边形。
    pub fn text_markup_quad_groups(&self) -> Option<Vec<[f64; 8]>> {
        self.subtype
            .is_text_markup()
            .then(|| self.quad_groups())
            .filter(|groups| !groups.is_empty())
    }

    /// `/QuadPoints` 原始值按 8 个数一组切分（文本标记 Table 179、
    /// Link Table 173 与 Redact Table 192 共用此键）；解析期已保证
    /// 总长为 8 的倍数、至少一组且坐标有限，此处不再校验子类型归属。
    pub fn quad_groups(&self) -> Vec<[f64; 8]> {
        self.quad_points
            .as_deref()
            .unwrap_or_default()
            .as_chunks::<8>()
            .0
            .to_vec()
    }

    /// Rubber Stamp 注解（Table 181）的 `/Name` 图标名类型化：14 个标准名
    /// 命中对应变体，其余原样保留在 [`StampIcon::Other`]；非 Stamp 子类型
    /// 返回 `None`，Stamp 无 `/Name` 也返回 `None`（查看器缺省按 `Draft`
    /// 呈现，是 Table 181 的缺省值而非文档中的条目，读端不代填）。
    pub fn stamp_icon(&self) -> Option<StampIcon> {
        if self.subtype != AnnotationSubtype::Stamp {
            return None;
        }
        self.icon_name.as_deref().map(StampIcon::from_name)
    }
}

/// `/InkList`：数组的数组，每条路径为 x/y 交替的偶数个有限数值。
pub(super) fn parse_ink_list(dict: &Dict) -> Result<Vec<Vec<f64>>> {
    let Some(value) = dict.get("InkList") else {
        return Ok(Vec::new());
    };
    let outer = value
        .as_array()
        .ok_or_else(|| malformed("annotation /InkList is not an array"))?;
    if outer.0.len() > MAX_INK_PATHS {
        return Err(malformed("annotation /InkList has too many paths"));
    }
    let mut paths = Vec::with_capacity(outer.0.len());
    for path in outer.0.iter() {
        let inner = path
            .as_array()
            .ok_or_else(|| malformed("annotation /InkList path is not an array"))?;
        if inner.0.len() % 2 != 0 || inner.0.is_empty() {
            return Err(malformed(
                "annotation /InkList path must hold an even, nonzero number of values",
            ));
        }
        if inner.0.len() > MAX_INK_PATH_VALUES {
            return Err(malformed("annotation /InkList path has too many values"));
        }
        let mut points = Vec::with_capacity(inner.0.len());
        for item in inner.0.iter() {
            points.push(number(item, "annotation /InkList")?);
        }
        paths.push(points);
    }
    Ok(paths)
}
