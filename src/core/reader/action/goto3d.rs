//! Go-To-3D-View 动作读取（12.6.4.15 Table 216，PDF 1.6）。
//!
//! `/TA` 是目标 3D 注解的间接引用，`/V` 是视图选择：内联视图字典（Table
//! 304）、`/VA` 数组下标、匹配 `/IN` 的文本串，或名字 `F`/`L`/`N`/`P`/`D`。
//! 名字形态比注解 `/3DV`（Table 298）多 `N` `P` `D`：后三者允许在目标
//! artwork 无 `/VA` 时退回默认视图。
//!
//! 目标 artwork 的 `/VA` 视图数可从目标注解 `/3DD` 解析，故整数 `/V`
//! 下标在可确定视图数时做越界校验；名称形态（含 `N`/`P`/`D`）仍由调用方
//! 结合当前视图处理，目标缺 `/3DD` 或 artwork 不可解析时保持读端宽容。

use crate::core::reader::navigation::resolve;
use crate::core::reader::three_d::{ThreeDView, parse_view_value};
use crate::core::types::{Dict, Document, Value};

use super::{ActionError, malformed};

/// Go-To-3D-View 动作（12.6.4.15 Table 216，PDF 1.6）。
#[derive(Debug, Clone, PartialEq)]
pub struct GoTo3DViewAction {
    /// `/TA` 目标注解的间接引用（对象号, 代号）。
    pub target: (i64, i64),
    /// `/V` 视图选择。
    pub view: GoTo3DViewSelection,
}

/// `/V` 的各形态；名字比注解 `/3DV` 多 `N` `P` `D`。
#[derive(Debug, Clone, PartialEq)]
pub enum GoTo3DViewSelection {
    /// 内联的 3D 视图字典（Table 304）。
    Inline(Box<ThreeDView>),
    /// `/VA` 数组下标（0 基）。
    Index(usize),
    /// 匹配 `/VA` 某个视图 `/IN` 的文本串（原值保留）。
    Name(Value),
    /// 名字 `F`：`/VA` 第一项。
    First,
    /// 名字 `L`：`/VA` 最后一项。
    Last,
    /// 名字 `N`：当前视图之后的下一项（来自 `/VA` 时环绕）。
    Next,
    /// 名字 `P`：当前视图之前的上一项（来自 `/VA` 时环绕）。
    Previous,
    /// 名字 `D`：默认视图。
    Default,
    /// 未识别的名字（原样保留，读端宽容）。
    Other(String),
}

/// 解析 Go-To-3D-View 动作字典；`/S` 已由调用方识别。
pub(super) fn parse_go_to_3d_view_action(
    document: &Document,
    dict: &Dict,
) -> Result<GoTo3DViewAction, ActionError> {
    // 目标 artwork 的 `/VA` 视图数（无法解析时保持宽容，不据此拒绝）。
    let target_view_count: Option<usize>;
    let target = match dict.get("TA") {
        Some(Value::Ref(number, generation)) => {
            let reference = Value::Ref(*number, *generation);
            let resolved = resolve(document, &reference)?;
            let Value::Dict(target_dict) = &resolved else {
                return Err(malformed(
                    "GoTo3DView action /TA does not resolve to an annotation dictionary",
                ));
            };
            // 目标缺省 `/Subtype` 时保持宽容；声明了就必须是 3D。
            match target_dict.get("Subtype").and_then(Value::as_name) {
                Some("3D") | None => {}
                Some(other) => {
                    return Err(malformed(format!(
                        "GoTo3DView action /TA targets a {other} annotation, expected 3D"
                    )));
                }
            }
            target_view_count = crate::core::reader::three_d::view_count(document, target_dict);
            (*number, *generation)
        }
        Some(_) => {
            return Err(malformed(
                "GoTo3DView action /TA must be an indirect reference",
            ));
        }
        None => return Err(malformed("GoTo3DView action is missing /TA")),
    };

    let value = dict
        .get("V")
        .ok_or_else(|| malformed("GoTo3DView action is missing /V"))?;
    let resolved = resolve(document, value)?;
    let view = match &resolved {
        Value::Dict(_) => GoTo3DViewSelection::Inline(Box::new(
            parse_view_value(document, &resolved, "GoTo3DView action /V")
                .map_err(|error| malformed(error.to_string()))?,
        )),
        Value::Integer(index) => GoTo3DViewSelection::Index(
            usize::try_from(*index)
                .map_err(|_| malformed("GoTo3DView action /V index must be nonnegative"))?,
        ),
        // 文本串匹配 `/VA` 中某个视图的 `/IN`；匹配与否由调用方处理。
        Value::String(_) | Value::HexString(_) => GoTo3DViewSelection::Name(resolved.clone()),
        Value::Name(name) => match name.as_str() {
            "F" => GoTo3DViewSelection::First,
            "L" => GoTo3DViewSelection::Last,
            "N" => GoTo3DViewSelection::Next,
            "P" => GoTo3DViewSelection::Previous,
            "D" => GoTo3DViewSelection::Default,
            _ => GoTo3DViewSelection::Other(name.clone()),
        },
        _ => {
            return Err(malformed(
                "GoTo3DView action /V is not a view dictionary, index, text string or name",
            ));
        }
    };

    // `/V` 整数形态是目标 `/VA` 的 0 基下标；能确定视图数时越界即拒绝，
    // 无法确定（目标缺 /3DD 或 artwork 不可解析）时保持读端宽容。
    if let (GoTo3DViewSelection::Index(index), Some(count)) = (&view, target_view_count)
        && *index >= count
    {
        return Err(malformed(format!(
            "GoTo3DView action /V index {index} is out of range for {} views",
            count
        )));
    }

    Ok(GoTo3DViewAction { target, view })
}
