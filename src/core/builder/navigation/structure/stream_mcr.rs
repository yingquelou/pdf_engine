//! 流形态 MCR（14.7.4.2，Table 324 `/Stm` `/StmOwn`）以
//! [`StructureKid::StreamMarkedContent`] 建模，写出 EXAMPLE 5（raw
//! 29588–29594 行）的 `/K << /Type /MCR /Pg … /Stm … /MCID … >>` 形态。
//! Table 324 原文（raw 29460–29481 行）：`/Stm` 是 shall 级间接引用
//! 「(Optional; shall be an indirect reference) The content stream
//! containing the marked-content sequence」（29469–29470 行），并附 should
//! 级定位条件「This entry should be present only if the marked-content
//! sequence resides in a content stream other than the content stream for
//! the page」（29471–29473 行）与 shall「If this entry is absent, the
//! marked-content sequence shall be contained in the content stream of the
//! page identified by Pg」（29474–29476 行）；`/StmOwn` 是
//! 「(Optional; shall be an indirect reference) The PDF object owning the
//! stream identified by Stm; … annotation to which an appearance stream
//! belongs」（29477–29479 行）；`/MCID` 是「MCID integer (Required) …」（
//! 29480–29481 行）——本切片基线（PDF 32000-1:2008）的 Table 324 把
//! `/MCID` 标为 Required，「存在 `/StmOwn` 时可缺省」在本地副本无依据，
//! 故流形态同样必填 MCID，不引入 `Option<i64>`；`/Pg` 是「This entry
//! overrides any Pg entry in the structure element containing the
//! marked-content reference; it shall be required if the structure element
//! has no such entry」（29464–29468 行）。Table 324 各行均无更高版本标注
//! （MCR 随 14.7.4 整体为 PDF 1.3），不新增 feature 门禁，复用结构树既有
//! ≥1.3 门槛。
//!
//! 库策略（规范未强制，比 shall 更严或更窄，理由写在括号里）：
//! - 流形态 MCR 的 `/Stm` 目标限于文档级 Form XObject 句柄（写端唯一稳定的
//!   「句柄→内容流对象号」通道）；页面内容流的序列继续走既有
//!   [`StructureKid::MarkedContent`] 通道而不写 `/Stm`（Table 324 的 should：
//!   条目只用于「不在页面内容流」的序列）；3D 流、缩略图、媒体与注解外观
//!   流的直接形态没有独立句柄通道，留作缺口。
//! - `/StmOwn` 由调用方显式给注解句柄、不自动推断（自动推断需要在页资源
//!   与 `/AP` 之间做归属搜索，越界风险大于收益）；构建期核对「所有者」
//!   shall——该注解的 `/AP`（`/N` 单引用或 `/N` 状态子字典、`/D`、`/R`）
//!   必须存在指向 `/Stm` 流对象的外观引用，否则拒绝。
//! - `/Pg` 解析次序：引用显式给出 → `/StmOwn` 注解绑定页 → 元素 `/Pg`；
//!   引用给出的页与注解绑定页不一致即谎报渲染页，拒绝；三路皆空按
//!   Table 324 的 shall（「it shall be required if the structure element
//!   has no such entry」）拒绝。仅在前三路之一实际生效且非继承元素
//!   `/Pg` 时写出显式 `/Pg`，继承时省略（由元素 `/Pg` 定位）。
//! - 父树数组按「容器对象号」判重：有 `/StmOwn` 时容器是所有者，否则是
//!   `/Stm` 流；容器同时被 OBJR 声称（写出 `/StructParent`）违反 Table 326
//!   的「not both」shall，两路都拒绝。
use crate::core::builder::navigation::invalid;
use crate::core::types::{Dict, Document, Object, Value};
use crate::core::writer::WriteError;

use super::flatten::PlannedKid;
use super::plan::Plan;
/// 校验流形态 MCR（Table 324）的目标对象形态；仍在结构对象号分配之前。
///
/// 规则（shall + 库策略，理由见模块注释）：`/Stm` 对象必须存在且是带
/// `/Subtype /Form` 的流（写端唯一可解析的内容流来源）；`/StmOwn` 对象
/// 必须存在且是普通注解字典，其 `/AP`（`/N` 单引用、`/N` 状态子字典、
/// `/D`、`/R`）必须引用 `/Stm`——Table 324 的 `/StmOwn` 是「The PDF
/// object owning the stream identified by Stm」，找不到外观引用就不是
/// 所有者。容器与 `/Stm` 字典已带 `/StructParent`/`/StructParents` 按
/// Table 326 互斥拒绝（公开 API 上写端不会预置这两个条目，该分支经测试
/// 篡改 `Document` 触达）。
pub(super) fn validate_stream_mcr_targets(
    doc: &Document,
    plan: &Plan<'_>,
) -> Result<(), WriteError> {
    let objects = doc.objects_for_write();
    for flat in &plan.flat {
        for kid in &flat.kids {
            let PlannedKid::StreamMarkedContent {
                stream_num,
                owner_num,
                ..
            } = kid
            else {
                continue;
            };
            let stream_object = objects
                .iter()
                .find(|item| item.num == *stream_num && item.generation == 0)
                .ok_or_else(|| {
                    invalid(format!(
                        "structure MCR /Stm object {stream_num} 0 is missing"
                    ))
                })?;
            let Value::Stream(stream_dict, _) = &stream_object.value else {
                return Err(invalid(format!(
                    "structure MCR /Stm object {stream_num} 0 is not a content stream ({})",
                    stream_object.value.type_name()
                )));
            };
            if stream_dict.get("Subtype").and_then(Value::as_name) != Some("Form") {
                return Err(invalid(format!(
                    "structure MCR /Stm object {stream_num} 0 is not a /Subtype /Form content stream (Table 324)"
                )));
            }
            let mut container_dicts: Vec<(&str, i64, &Dict)> =
                vec![("/Stm", *stream_num, stream_dict)];
            if let Some(owner_num) = owner_num {
                let owner_object = objects
                    .iter()
                    .find(|item| item.num == *owner_num && item.generation == 0)
                    .ok_or_else(|| {
                        invalid(format!(
                            "structure MCR /StmOwn object {owner_num} 0 is missing"
                        ))
                    })?;
                let Value::Dict(owner_dict) = &owner_object.value else {
                    return Err(invalid(format!(
                        "structure MCR /StmOwn object {owner_num} 0 is not an annotation dictionary (Table 324: the object owning the /Stm stream)"
                    )));
                };
                if !appearance_references(owner_dict, *stream_num) {
                    return Err(invalid(format!(
                        "structure MCR /StmOwn object {owner_num} 0 does not own the /Stm object {stream_num} 0: no appearance stream under its /AP references it (Table 324)"
                    )));
                }
                container_dicts.push(("/StmOwn", *owner_num, owner_dict));
            }
            for (role, num, dict) in container_dicts {
                for entry in ["StructParent", "StructParents"] {
                    if dict.get(entry).is_some() {
                        return Err(invalid(format!(
                            "structure MCR {role} object {num} 0 already has /{entry}; at most one of /StructParent and /StructParents may appear in an object (Table 326)"
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

/// 注解字典的 `/AP` 是否引用对象号 `stream_num` 的外观流：`/N` 可以是
/// 单条流引用，也可以是「状态名 → 流引用」子字典（Table 168 多状态
/// Widget），`/D`、`/R` 为单引用。
fn appearance_references(dict: &Dict, stream_num: i64) -> bool {
    let Some(appearance) = dict.get("AP").and_then(Value::as_dict) else {
        return false;
    };
    ["N", "D", "R"].iter().any(|key| match appearance.get(key) {
        Some(Value::Ref(num, _)) => *num == stream_num,
        Some(Value::Dict(states)) => states
            .0
            .values()
            .any(|value| matches!(value, Value::Ref(num, _) if *num == stream_num)),
        _ => false,
    })
}

/// 流形态 MCR 的容器字典追加 `/StructParents`（Table 326；14.7.4.4 的
/// 「the StructParents entry of the page object or other content stream
/// in which the sequence resides」，raw 29722–29723 行）。容器是
/// `/StmOwn` 给出的所有者注解字典，或无所有者时的 `/Stm`（Form
/// XObject）流字典；流形态只替换字典、字节数据原样保留（`/Length` 由
/// 写出阶段按实际字节重算）。
pub(super) fn write_stream_parents(doc: &mut Document, plan: &Plan<'_>) -> Result<(), WriteError> {
    for (container, key) in &plan.stream_keys {
        let objects = doc.objects_for_write();
        let object = objects
            .iter()
            .find(|item| item.num == *container && item.generation == 0)
            .ok_or_else(|| {
                invalid(format!(
                    "structure MCR /Stm container object {container} 0 is missing"
                ))
            })?;
        let mut dict = match &object.value {
            Value::Dict(dict) => dict.clone(),
            Value::Stream(dict, data) => {
                let mut dict = dict.clone();
                dict = dict.insert("StructParents", Value::Integer(*key));
                doc.set_object(Object::new(
                    *container,
                    0,
                    Value::Stream(dict, data.clone()),
                ));
                continue;
            }
            _ => {
                return Err(invalid(format!(
                    "structure MCR /Stm container object {container} 0 is not a dictionary"
                )));
            }
        };
        dict = dict.insert("StructParents", Value::Integer(*key));
        doc.set_object(Object::new(*container, 0, Value::Dict(dict)));
    }
    Ok(())
}
