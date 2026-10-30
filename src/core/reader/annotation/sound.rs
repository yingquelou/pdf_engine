//! Sound 注解专有条目读取（12.5.6.16 Table 185）。
//!
//! `/Sound` 是声音流对象的间接引用，Sound 注解必填；流对象在 PDF 语法中
//! 总是间接的，因此直接流值视为格式错误。`/Name` 图标名与 Text 注解共用
//! [`super::subtype`] 的通用解析（Sound 的标准值为 `Speaker`、`Mic`，
//! 缺省 `Speaker`），语义由查看器定义。
//!
//! `/AP` 外观流优先于本注解的声音与图标呈现（Table 185 注）。

use crate::core::types::{Dict, Value};

use super::{AnnotationSubtype, Result, malformed};

/// 解析 Sound 注解的 `/Sound` 条目。
///
/// 非 Sound 子类型出现 `/Sound` 时报错；Sound 子类型缺少 `/Sound`、
/// 或值不是间接引用时报错。
pub(super) fn parse_sound_entries(
    dict: &Dict,
    subtype: &AnnotationSubtype,
) -> Result<Option<Value>> {
    let value = dict.get("Sound");
    if *subtype != AnnotationSubtype::Sound {
        return match value {
            Some(_) => Err(malformed(
                "annotation /Sound is specific to Sound annotations",
            )),
            None => Ok(None),
        };
    }
    match value {
        None => Err(malformed("Sound annotation is missing /Sound")),
        Some(Value::Ref(..)) => Ok(value.cloned()),
        Some(_) => Err(malformed(
            "Sound annotation /Sound must be an indirect reference to a sound stream",
        )),
    }
}
