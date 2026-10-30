//! Watermark 注解专有条目（12.5.6.22，Table 190）与固定打印字典（Table 191）。
//!
//! Table 190 只给 Watermark 声明一项专有条目 `/FixedPrint`，且要求它「shall be a
//! dictionary」；规范示例把它写成间接引用，读端两种形态都解引用后类型化为
//! [`FixedPrint`]。Table 191 的缺省值（单位矩阵、`/H` `/V` 为 0）不物化，
//! 缺失以 `None` 表示，由渲染端按表补默认。

use super::{
    Result,
    entries::{number, parse_optional_number},
    malformed,
};
use crate::core::types::{Dict, Document, Value};

/// 固定打印字典（Table 191）：注解相对目标介质尺寸的固定大小与位置。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FixedPrint {
    /// `/Matrix` 预变换注解矩形的矩阵（缺省单位矩阵 `[1 0 0 1 0 0]`）。
    pub matrix: Option<[f64; 6]>,
    /// `/H` 水平平移量，按目标介质（未知时取页面 `/MediaBox`）宽度的百分比，
    /// 1.0 表示 100%（缺省 0）。
    pub horizontal: Option<f64>,
    /// `/V` 垂直平移量，按目标介质（未知时取页面 `/MediaBox`）高度的百分比，
    /// 1.0 表示 100%（缺省 0）。
    pub vertical: Option<f64>,
}

/// 解析注解字典的 `/FixedPrint` 条目；缺失返回 `None`。
pub(super) fn parse_fixed_print(document: &Document, dict: &Dict) -> Result<Option<FixedPrint>> {
    let Some(value) = dict.get("FixedPrint") else {
        return Ok(None);
    };
    let resolved = crate::core::reader::navigation::resolve(document, value)
        .map_err(|err| super::AnnotationError::Malformed(err.to_string()))?;
    let fixed = resolved
        .as_dict()
        .ok_or_else(|| malformed("annotation /FixedPrint must be a dictionary"))?;
    if let Some(name) = fixed.get("Type").and_then(Value::as_name)
        && name != "FixedPrint"
    {
        return Err(malformed(format!(
            "annotation /FixedPrint /Type must be FixedPrint, found {name}"
        )));
    }
    Ok(Some(FixedPrint {
        matrix: parse_matrix(fixed)?,
        horizontal: parse_optional_number(fixed, "H")?,
        vertical: parse_optional_number(fixed, "V")?,
    }))
}

/// `/Matrix` 六项有限数值；规范要求非负的平移建议不在此拦截（Table 191 用 should）。
fn parse_matrix(dict: &Dict) -> Result<Option<[f64; 6]>> {
    let Some(value) = dict.get("Matrix") else {
        return Ok(None);
    };
    let array = value
        .as_array()
        .ok_or_else(|| malformed("annotation /FixedPrint /Matrix is not an array"))?;
    if array.len() != 6 {
        return Err(malformed(
            "annotation /FixedPrint /Matrix must have six numbers",
        ));
    }
    let mut out = [0.0f64; 6];
    for (index, item) in array.iter().enumerate() {
        out[index] = number(item, "annotation /FixedPrint /Matrix")?;
    }
    Ok(Some(out))
}
