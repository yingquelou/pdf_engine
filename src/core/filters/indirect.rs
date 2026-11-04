//! ISO 32000-1:2008 7.4.1「Filter Processes」+ Table 5：流字典里 `/Filter`
//! 与 `/DecodeParms` 的**间接引用形态**。
//!
//! 规范依据（`docs/PDF32000_2008.pdf`，`pdftotext -raw` 抽取）：
//!
//! - 7.4.1 General：「The filter or filters for a stream **shall** be specified
//!   by the Filter entry in the stream's dictionary …」与「These optional
//!   parameters **shall** be specified by the DecodeParms entry in the stream's
//!   dictionary …」；
//! - Table 5（Entries common to all stream dictionaries）：`Filter` 的类型是
//!   「name or array」、`DecodeParms` 是「dictionary or array」，且「If there
//!   are multiple filters and any of the filters has parameters set to
//!   nondefault values, DecodeParms **shall** be an array with one entry for
//!   each filter」；
//! - 7.3.10 Indirect Objects：「**Any** object in a PDF file may be labelled as
//!   an indirect object … by which other objects can refer to it (for example,
//!   as an element of an array or as the value of a dictionary entry)」——故上述
//!   两个条目的**值**、以及 Name/参数数组的**元素**都允许是间接引用，读端必须
//!   先解引用再按 Table 5 的形态处理（本仓库此前的缺口正是把它按不支持处理）；
//! - 7.5.8.2（交叉引用流字典）是**例外**：「If the stream is encoded, the
//!   Filter and DecodeParms entries in Table 5 **shall** also be direct
//!   objects」——所以 xref 流读端（`crate::core::reader::xref`）不需要、也不应
//!   解引用这两个条目，本模块只服务普通流对象。
//!
//! # 解引用失败的立场
//!
//! 「保守解引用」：任何一跳失败（对象缺失、解出 `null`、形态非法、引用成环、
//! 链长超限）都**保留原值**并产出一条可诊断信息，交给调用方并入
//! `Document::warnings`；原值留在字典里会让 [`super::decode_stream`] 按
//! 「unsupported /Filter value」失败，即**绝不静默丢字节、绝不 panic**。
//!
//! 注：7.3.9 有「An indirect object reference to a nonexistent object shall be
//! treated the same as a null object」「null 作为字典条目值等价于省略该条目」的
//! 措辞，按字面可把缺失引用降级成「无筛选器 → 原样返回未解码字节」。本实现
//! 不取该降级：未解码字节被当作解码结果返回同样是静默错误，故统一走
//! 「保留原值 + 诊断 + 解码失败」。

use crate::core::types::{Array, Dict, Value};

/// 单个条目（或其数组元素）允许的最多解引用跳数。
///
/// 合法文件恒为 0 跳（直接对象）或 1 跳（引用指向 Name/字典）；本仓库各读端
/// 模块的深度封顶惯例（`MAX_STRUCTURE_DEPTH`/`MAX_TREE_DEPTH` = 64、
/// `MAX_TARGET_DEPTH` = 32）对这里过于宽松，取 16 既能容忍「引用 → 引用」的
/// 真实畸形文件，又保证环检测之外的第二道护栏。
const MAX_INDIRECT_HOPS: usize = 16;

/// 间接引用解引用器：把 `Value::Ref` 换成被引用对象的值。
///
/// 由调用方（`Document` 侧）提供，走仓库既有的懒加载设施
/// （`Document::get_object`：缓存 → xref → 对象流），本模块不碰 xref 读取。
/// 返回 `Ok(None)` 表示该对象号在 xref 里不存在／是自由条目；
/// 返回 `Err` 表示解析或读取该对象失败。
pub(crate) type ObjectResolver<'a> =
    dyn Fn(&Value) -> std::result::Result<Option<Value>, String> + 'a;

/// 规范化流字典的 `/Filter` 与 `/DecodeParms`（含缩写 `/DP`）间接引用形态。
///
/// 返回 `(规范化字典, 诊断列表)`：
/// - 解引用成功且形态合法 → 对应条目被替换为 Name / Name 数组 / 字典 /
///   字典数组，后续 [`super::decode_stream`] 的解析结果与「本来就写直接对象」
///   的字典**逐字节相同**；
/// - 任一形态非法 → 该条目**原样保留**并追加一条诊断；
/// - 另外按 Table 5 的 shall 约束交叉核对 `/DecodeParms` 与 `/Filter` 的
///   个数（多过滤器配单字典、参数字典个数与筛选器个数不匹配、无 `/Filter`
///   却有参数字典），只记诊断、不改数据。
///
/// 无间接引用且无 shall 违例时返回字典的克隆、诊断为空，调用方可无条件使用。
pub(crate) fn normalize_filter_entries(
    dict: &Dict,
    resolve: &ObjectResolver<'_>,
) -> (Dict, Vec<String>) {
    let mut issues = Vec::new();
    let mut result = dict.clone();

    if let Some(value) = dict.get("Filter")
        && let Some(normalized) = normalize_filter_value(value, resolve, &mut issues)
    {
        result.0.insert("Filter".to_string(), normalized);
    }
    let parms_key = ["DecodeParms", "DP"]
        .into_iter()
        .find(|key| dict.get(key).is_some());
    if let Some(key) = parms_key
        && let Some(normalized) =
            normalize_parms_value(dict.get(key).expect("key just found"), resolve, &mut issues)
    {
        result.0.insert(key.to_string(), normalized);
    }

    check_table5_counts(&result, &mut issues);
    (result, issues)
}

/// `/Filter` 形态规范化：需要解引用时返回替换后的值，否则返回 `None`。
fn normalize_filter_value(
    value: &Value,
    resolve: &ObjectResolver<'_>,
    issues: &mut Vec<String>,
) -> Option<Value> {
    match value {
        Value::Ref(_, _) => match dereference("/Filter", value, resolve, issues) {
            Some(Value::Name(name)) => Some(Value::Name(name)),
            Some(Value::Array(array)) => Some(Value::Array(names_from_array(
                "/Filter", &array, resolve, issues,
            ))),
            Some(other) => {
                issues.push(format!(
                    "/Filter indirect reference resolves to {} (Table 5: name or array of names); \
                     original value kept",
                    other.type_name()
                ));
                None
            }
            None => None,
        },
        // 数组元素本身也可以是间接引用（7.3.10），逐个解引用。
        Value::Array(array) if array.iter().any(|item| matches!(item, Value::Ref(_, _))) => Some(
            Value::Array(names_from_array("/Filter", array, resolve, issues)),
        ),
        _ => None,
    }
}

/// `/DecodeParms`／`/DP` 形态规范化，规则同 [`normalize_filter_value`]。
fn normalize_parms_value(
    value: &Value,
    resolve: &ObjectResolver<'_>,
    issues: &mut Vec<String>,
) -> Option<Value> {
    match value {
        Value::Ref(_, _) => match dereference("/DecodeParms", value, resolve, issues) {
            Some(Value::Dict(params)) => Some(Value::Dict(params)),
            Some(Value::Array(array)) => Some(Value::Array(Array(
                array
                    .0
                    .iter()
                    .map(|item| parms_entry(item, resolve, issues))
                    .collect(),
            ))),
            Some(other) => {
                issues.push(format!(
                    "/DecodeParms indirect reference resolves to {} (Table 5: dictionary or array \
                     of dictionaries); original value kept",
                    other.type_name()
                ));
                None
            }
            None => None,
        },
        Value::Array(array) if array.iter().any(|item| matches!(item, Value::Ref(_, _))) => {
            Some(Value::Array(Array(
                array
                    .0
                    .iter()
                    .map(|item| parms_entry(item, resolve, issues))
                    .collect(),
            )))
        }
        _ => None,
    }
}

/// 把 `/Filter` 数组元素解引用为 Name；非法元素保留原值并记诊断。
fn names_from_array(
    key: &str,
    array: &Array,
    resolve: &ObjectResolver<'_>,
    issues: &mut Vec<String>,
) -> Array {
    Array(
        array
            .0
            .iter()
            .map(|item| match item {
                Value::Name(name) => Value::Name(name.clone()),
                Value::Ref(_, _) => match dereference(key, item, resolve, issues) {
                    Some(Value::Name(name)) => Value::Name(name),
                    Some(other) => {
                        issues.push(format!(
                            "{key} array element resolves to {} (Table 5: name); original value \
                             kept",
                            other.type_name()
                        ));
                        item.clone()
                    }
                    None => item.clone(),
                },
                other => other.clone(),
            })
            .collect(),
    )
}

/// 把 `/DecodeParms` 数组元素解引用为字典或 `null`；非法元素保留原值并记诊断。
fn parms_entry(item: &Value, resolve: &ObjectResolver<'_>, issues: &mut Vec<String>) -> Value {
    match item {
        Value::Ref(_, _) => match dereference("/DecodeParms", item, resolve, issues) {
            Some(Value::Dict(params)) => Value::Dict(params),
            Some(Value::Null) => Value::Null,
            Some(other) => {
                issues.push(format!(
                    "/DecodeParms array element resolves to {} (Table 5: dictionary or null); \
                     original value kept",
                    other.type_name()
                ));
                item.clone()
            }
            None => item.clone(),
        },
        other => other.clone(),
    }
}

/// 沿引用链解引用到**非引用**值；链上的环、超长、缺失对象一律记诊断并
/// 返回 `None`（调用方据此保留原值）。
fn dereference(
    key: &str,
    value: &Value,
    resolve: &ObjectResolver<'_>,
    issues: &mut Vec<String>,
) -> Option<Value> {
    let mut current = value.clone();
    let mut visited: Vec<(i64, i64)> = Vec::new();
    loop {
        let (num, generation) = match current {
            Value::Ref(num, generation) => (num, generation),
            other => return Some(other),
        };
        if visited.contains(&(num, generation)) {
            issues.push(format!(
                "{key}: cyclic indirect reference {num} {generation} R (chain \
                 {:?}); original value kept",
                visited
                    .iter()
                    .map(|&(n, g)| format!("{n} {g} R"))
                    .collect::<Vec<_>>()
            ));
            return None;
        }
        if visited.len() >= MAX_INDIRECT_HOPS {
            issues.push(format!(
                "{key}: indirect chain longer than {MAX_INDIRECT_HOPS} hops ending at \
                 {num} {generation} R; original value kept"
            ));
            return None;
        }
        visited.push((num, generation));
        match resolve(&current) {
            Ok(Some(next)) => current = next,
            Ok(None) => {
                issues.push(format!(
                    "{key}: indirect reference {num} {generation} R names an absent object \
                     (7.3.10); original value kept"
                ));
                return None;
            }
            Err(problem) => {
                issues.push(format!(
                    "{key}: resolving indirect reference {num} {generation} R failed: {problem}; \
                     original value kept"
                ));
                return None;
            }
        }
    }
}

/// Table 5 的 `/Filter`↔`/DecodeParms` 个数 shall 约束（只诊断，不改数据）。
fn check_table5_counts(dict: &Dict, issues: &mut Vec<String>) {
    let parms_key = if dict.get("DecodeParms").is_some() {
        "DecodeParms"
    } else {
        "DP"
    };
    let parms = dict.get(parms_key);
    let Some(filters) = dict.get("Filter") else {
        if matches!(parms, Some(Value::Dict(_)) | Some(Value::Array(_))) {
            issues.push(format!(
                "/{parms_key} present without /Filter (Table 5: the parameters are used by the \
                 filters specified by Filter)"
            ));
        }
        return;
    };
    let Some(filter_count) = entry_count(filters) else {
        return;
    };
    let Some(parms_value) = parms else { return };
    match parms_value {
        Value::Dict(_) if filter_count > 1 => issues.push(format!(
            "/{parms_key} is a single dictionary but /Filter applies {filter_count} filters \
             (Table 5: DecodeParms shall be an array with one entry for each filter)"
        )),
        Value::Array(array) => {
            let parms_count = array.len();
            if filter_count == 1 && parms_count == 1 {
                // 单过滤器配单元素数组：可容忍，不产噪声。
                return;
            }
            if parms_count != filter_count {
                issues.push(format!(
                    "/{parms_key} array has {parms_count} entries but /Filter has \
                     {filter_count} (Table 5: one entry per filter, the null object where a \
                     filter has no parameters)"
                ));
            }
        }
        _ => {}
    }
}

/// 已规范化条目的筛选器／参数个数；形态仍非法（如残留引用）时返回 `None`。
fn entry_count(value: &Value) -> Option<usize> {
    match value {
        Value::Name(_) | Value::Dict(_) => Some(1),
        Value::Null => Some(0),
        Value::Array(array) => Some(array.len()),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
