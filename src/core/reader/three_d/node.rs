//! 3D 节点字典（Table 312）。
//!
//! `/N` 必填文本串（节点名，原值保留字面串/十六进制串形态）；`/O` 不透明度
//! 与 `/V` 可见性缺省表示沿用上层节点；`/M` 为恰 12 个有限数的 3D 矩阵。
//! 显式 `false` 可见性与缺省（继承）在语义上不同，故均不物化缺省值。
//! 同一节点在 `/NA` 中重复出现时，呈现端按深度优先取最后一项；读端按
//! 数组顺序原样保留，由调用方决定去重。

use crate::core::types::{Dict, Document, Value};

use super::{
    ThreeDError, array_value, check_type, malformed, number_value, optional_bool, optional_number,
    resolve_value, text_value,
};

/// 3D 节点字典（Table 312）。
#[derive(Debug, Clone, PartialEq)]
pub struct ThreeDNode {
    /// `/N` 节点名（必填文本串，原值保留）。
    pub name: Value,
    /// `/O` 不透明度（`0..=1`）；`None` 表示继承。
    pub opacity: Option<f64>,
    /// `/V` 可见性；`None` 表示继承。
    pub visible: Option<bool>,
    /// `/M` 12 元 3D 矩阵。
    pub matrix: Option<[f64; 12]>,
}

/// 解析已解引用的节点字典。
pub(super) fn parse_node(
    document: &Document,
    dict: &Dict,
    context: &str,
) -> Result<ThreeDNode, ThreeDError> {
    check_type(dict, "3DNode", context)?;
    let name = dict
        .get("N")
        .ok_or_else(|| malformed(format!("{context} is missing required /N")))?;
    let name = text_value(&resolve_value(document, name)?, &format!("{context} /N"))?;
    let opacity = optional_number(document, dict, "O", context)?;
    if let Some(opacity) = opacity
        && !(0.0..=1.0).contains(&opacity)
    {
        return Err(malformed(format!("{context} /O must be within 0..=1")));
    }
    let visible = optional_bool(document, dict, "V", context)?;
    let matrix = match dict.get("M") {
        None => None,
        Some(value) => Some(parse_matrix(document, value, &format!("{context} /M"))?),
    };
    Ok(ThreeDNode {
        name,
        opacity,
        visible,
        matrix,
    })
}

/// 恰 12 个有限数值的数组。
fn parse_matrix(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<[f64; 12], ThreeDError> {
    let array = array_value(document, value, context)?;
    if array.len() != 12 {
        return Err(malformed(format!("{context} must hold exactly 12 numbers")));
    }
    let mut out = [0.0; 12];
    for (index, item) in array.iter().enumerate() {
        let resolved = resolve_value(document, item)?;
        out[index] = number_value(&resolved, &format!("{context} entry"))?;
    }
    Ok(out)
}
