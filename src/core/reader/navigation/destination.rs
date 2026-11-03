//! 显式与命名目的地解析（ISO 32000-1 12.3.2）。

use crate::core::types::{Array, Document, Value};

use super::{NavigationError, PageTreeIndex, catalog, malformed, name_tree::string_key, resolve};

/// 命名目标经 `/A` go-to 动作间接引用（`/D` 为名称/文本串）时的最大跳数。
///
/// 与结构属性继承（`MAX_STRUCTURE_DEPTH`）同用 64 惯例：环检测已能拒掉
/// 自环与互环，此上限是防御畸形文件里“互不相同但极长”的间接链导致的深递归。
const MAX_NAMED_TARGET_HOPS: usize = 64;

/// 命名目标间接引用的解析上下文：跳数上限 + 已见名称环检测。
#[derive(Default)]
struct TargetContext {
    hops: usize,
    seen: std::collections::BTreeSet<String>,
}

/// 本地显式目的地（12.3.2.2）。
///
/// 页面以 0 基索引表示；`/XYZ` 的三个参数为 `null` 或缺省时为 `None`。
#[derive(Debug, Clone, PartialEq)]
pub enum Destination {
    Xyz {
        page: usize,
        left: Option<f64>,
        top: Option<f64>,
        zoom: Option<f64>,
    },
    Fit {
        page: usize,
    },
    FitH {
        page: usize,
        top: Option<f64>,
    },
    FitV {
        page: usize,
        left: Option<f64>,
    },
    FitR {
        page: usize,
        rect: [f64; 4],
    },
    FitB {
        page: usize,
    },
    FitBH {
        page: usize,
        top: Option<f64>,
    },
    FitBV {
        page: usize,
        left: Option<f64>,
    },
}

impl Destination {
    /// 目标页面索引。
    pub fn page(&self) -> usize {
        match self {
            Self::Xyz { page, .. }
            | Self::Fit { page }
            | Self::FitH { page, .. }
            | Self::FitV { page, .. }
            | Self::FitR { page, .. }
            | Self::FitB { page }
            | Self::FitBH { page, .. }
            | Self::FitBV { page, .. } => *page,
        }
    }
}

/// 命名目标值 → 目的地；值可为目的地数组或含 `/D`/`/A` 的字典。
///
/// 两条命名目标通道共用本函数，取值形态由同一句规范授权（12.3.2.3，
/// PDF32000_2008.pdf `pdftotext -raw` L20522–L20524）：「The value of this
/// entry shall be a dictionary in which each key is a destination name and the
/// corresponding value is either an array defining the destination, using the
/// syntax shown in Table 151, or a dictionary with a D entry whose value is
/// such an array.」紧随其后的 NOTE 2（L20525–L20526）说明字典形态用于附加
/// 属性与 go-to 动作目标，原文「The latter form allows additional attributes
/// to be associated with the destination, as well as enabling a go-to action
/// (…) that shall be used as the target of a named destination.」。因此字典
/// 分支按此取值：有 `/D` 用 `/D`；无 `/D` 但有 `/A` 时按 go-to 动作解析其
/// `/D`；字典里未知的额外键只做到不解析、不报错、原样保留。
/// 1.2+ 名称树段落的「The destination value associated with a key in the name
/// tree may be either an array or a dictionary, as described in the preceding
/// paragraph.」（L20530–L20531）把同一套形态也给了名称树通道。
pub(crate) fn parse_named_destination(
    document: &Document,
    value: &Value,
    pages: &PageTreeIndex,
) -> Result<Destination, NavigationError> {
    let mut context = TargetContext::default();
    parse_named_destination_inner(document, value, pages, &mut context)
}

/// [`parse_named_destination`] 的递归内核，携带跳数/环检测上下文。
fn parse_named_destination_inner(
    document: &Document,
    value: &Value,
    pages: &PageTreeIndex,
    context: &mut TargetContext,
) -> Result<Destination, NavigationError> {
    let value = resolve(document, value)?;
    if let Value::Dict(dict) = &value {
        // `/D` 优先。规范对 `/D`、`/A` 并存未给 shall 排序，故此优先级属
        // **库策略**：Table 151 句只保证 `/D` 形态，`/A` 仅由 informative 的
        // NOTE 2 引入，取显式 `/D` 更贴近规范性那句。
        if let Some(dest) = dict.get("D") {
            return parse_destination(document, dest, pages);
        }
        // 无 `/D`：NOTE 2 授权 go-to 动作作为命名目标的载体。
        if let Some(action) = dict.get("A") {
            return parse_go_to_action_target(document, action, pages, context);
        }
        return Err(malformed(
            "named destination dictionary is missing /D and /A",
        ));
    }
    parse_destination(document, &value, pages)
}

/// 解析 `<< /A <动作> >>` 形态的目的地：只接受 `/S /GoTo`，取其 `/D`。
///
/// NOTE 2 只点名 go-to 动作，故其它 `/S` 返回 `Malformed`（**库策略**，
/// 规范未对附加属性字典的 `/A`  subtype 作 shall）。`/D` 的数组形态复用
/// [`parse_destination`]（Table 151），名称/文本串形态递归解析命名目标。
fn parse_go_to_action_target(
    document: &Document,
    action: &Value,
    pages: &PageTreeIndex,
    context: &mut TargetContext,
) -> Result<Destination, NavigationError> {
    let action = resolve(document, action)?;
    let dict = action
        .as_dict()
        .ok_or_else(|| malformed("named destination /A is not an action dictionary"))?;
    let subtype = dict
        .get("S")
        .and_then(Value::as_name)
        .ok_or_else(|| malformed("named destination /A action is missing a name /S"))?;
    if subtype != "GoTo" {
        return Err(malformed(format!(
            "named destination /A action must be /GoTo, got /{subtype}"
        )));
    }
    let target = dict
        .get("D")
        .ok_or_else(|| malformed("named destination /A GoTo action is missing /D"))?;
    let target = resolve(document, target)?;
    match &target {
        Value::Array(_) => parse_destination(document, &target, pages),
        Value::Name(_) | Value::String(_) | Value::HexString(_) => {
            let name = go_to_target_name(&target)?;
            follow_named_target(document, &name, pages, context)
        }
        _ => Err(malformed(
            "named destination /A GoTo /D is neither a destination array nor a name",
        )),
    }
}

/// go-to 动作 `/D` 的名称形态目标名（名字对象或文本串）。
fn go_to_target_name(value: &Value) -> Result<String, NavigationError> {
    if let Value::Name(name) = value {
        return Ok(name.clone());
    }
    string_key(value).ok_or_else(|| malformed("named destination /A GoTo /D string is malformed"))
}

/// 按名跳转解析一个命名目标：做环检测与跳数上限后递归进入条目值解析。
fn follow_named_target(
    document: &Document,
    name: &str,
    pages: &PageTreeIndex,
    context: &mut TargetContext,
) -> Result<Destination, NavigationError> {
    if !context.seen.insert(name.to_owned()) {
        return Err(malformed(format!(
            "named destination go-to action references /{name} in a cycle"
        )));
    }
    context.hops += 1;
    if context.hops > MAX_NAMED_TARGET_HOPS {
        return Err(malformed(format!(
            "named destination go-to action chain exceeds {MAX_NAMED_TARGET_HOPS} hops"
        )));
    }
    let entry = named_destination_entry(document, name)?
        .ok_or_else(|| malformed(format!("named destination /{name} is not defined")))?;
    parse_named_destination_inner(document, &entry, pages, context)
}

/// 按名在两条命名目标通道取值：先 `/Names /Dests` 名称树，再目录旧式
/// `/Dests` 字典（键既接受名字对象也接受带转义的名字），与
/// [`Document::named_destination`] 的优先级一致。
fn named_destination_entry(
    document: &Document,
    name: &str,
) -> Result<Option<Value>, NavigationError> {
    if let Some(value) = document.name_tree("Dests")?.get(name) {
        return Ok(Some(value.clone()));
    }
    let catalog = catalog(document)?;
    let Some(dests) = catalog.get("Dests") else {
        return Ok(None);
    };
    let dests = resolve(document, dests)?;
    let dests = dests
        .as_dict()
        .ok_or_else(|| malformed("catalog /Dests is not a dictionary"))?;
    if let Some(entry) = dests.get(name) {
        return Ok(Some(entry.clone()));
    }
    Ok(dests
        .0
        .iter()
        .find(|(key, _)| crate::core::reader::page_content::decode_name_escapes(key) == name)
        .map(|(_, value)| value.clone()))
}

/// 解析本地显式目的地的目的地数组。
pub(crate) fn parse_destination(
    document: &Document,
    value: &Value,
    pages: &PageTreeIndex,
) -> Result<Destination, NavigationError> {
    let value = resolve(document, value)?;
    let array = value
        .as_array()
        .ok_or_else(|| malformed("destination is not an array"))?;
    if array.len() < 2 {
        return Err(malformed("destination array is shorter than 2 elements"));
    }
    let Some((num, generation)) = array.0[0].as_ref() else {
        return Err(malformed(
            "destination page is not an indirect page reference",
        ));
    };
    let page = pages
        .index_of(num, generation)
        .ok_or(NavigationError::PageNotFound)?;
    let Some(kind) = array.0[1].as_name() else {
        return Err(malformed("destination type is not a name"));
    };
    match kind {
        "XYZ" => {
            ensure_no_extra(document, array, 5)?;
            Ok(Destination::Xyz {
                page,
                left: optional_number(document, array, 2)?,
                top: optional_number(document, array, 3)?,
                zoom: optional_number(document, array, 4)?,
            })
        }
        "Fit" => {
            ensure_no_extra(document, array, 2)?;
            Ok(Destination::Fit { page })
        }
        "FitH" => {
            ensure_no_extra(document, array, 3)?;
            Ok(Destination::FitH {
                page,
                top: optional_number(document, array, 2)?,
            })
        }
        "FitV" => {
            ensure_no_extra(document, array, 3)?;
            Ok(Destination::FitV {
                page,
                left: optional_number(document, array, 2)?,
            })
        }
        "FitR" => {
            ensure_no_extra(document, array, 6)?;
            let rect = [
                required_number(document, array, 2)?,
                required_number(document, array, 3)?,
                required_number(document, array, 4)?,
                required_number(document, array, 5)?,
            ];
            Ok(Destination::FitR { page, rect })
        }
        "FitB" => {
            ensure_no_extra(document, array, 2)?;
            Ok(Destination::FitB { page })
        }
        "FitBH" => {
            ensure_no_extra(document, array, 3)?;
            Ok(Destination::FitBH {
                page,
                top: optional_number(document, array, 2)?,
            })
        }
        "FitBV" => {
            ensure_no_extra(document, array, 3)?;
            Ok(Destination::FitBV {
                page,
                left: optional_number(document, array, 2)?,
            })
        }
        other => Err(malformed(format!("unknown destination type /{other}"))),
    }
}

/// 超出该类型参数个数的尾部操作数只允许为 `null`（宽容真实文件中的占位）。
fn ensure_no_extra(
    document: &Document,
    array: &Array,
    limit: usize,
) -> Result<(), NavigationError> {
    for index in limit..array.len() {
        let value = resolve(document, &array.0[index])?;
        if !matches!(value, Value::Null) {
            return Err(malformed(format!(
                "destination has unexpected operand at index {index}"
            )));
        }
    }
    Ok(())
}

fn optional_number(
    document: &Document,
    array: &Array,
    index: usize,
) -> Result<Option<f64>, NavigationError> {
    let Some(value) = array.0.get(index) else {
        return Ok(None);
    };
    let value = resolve(document, value)?;
    match value {
        Value::Null => Ok(None),
        Value::Integer(number) => Ok(Some(number as f64)),
        Value::Float(number) => Ok(Some(number)),
        other => Err(malformed(format!(
            "destination operand {index} is not a number: {other:?}"
        ))),
    }
}

fn required_number(
    document: &Document,
    array: &Array,
    index: usize,
) -> Result<f64, NavigationError> {
    optional_number(document, array, index)?
        .ok_or_else(|| malformed(format!("destination operand {index} is missing")))
}

/// 大纲 `/Dest`（数组/名称/字符串）→ 大纲目的地。
pub(crate) fn parse_outline_destination(
    document: &Document,
    value: &Value,
    pages: &PageTreeIndex,
) -> Result<super::OutlineDestination, NavigationError> {
    let value = resolve(document, value)?;
    match &value {
        Value::Array(_) => {
            parse_destination(document, &value, pages).map(super::OutlineDestination::Explicit)
        }
        Value::Name(name) => Ok(super::OutlineDestination::Named(name.clone())),
        Value::String(_) | Value::HexString(_) => string_key(&value)
            .map(super::OutlineDestination::Named)
            .ok_or_else(|| malformed("outline /Dest string is malformed")),
        _ => Err(malformed(
            "outline /Dest is neither a destination array nor a name",
        )),
    }
}
