//! 同全限定名字段的多重表示（12.7.3.2「Field Names」）。
//!
//! 无 `/T` 的字段字典与其共同祖先共享全限定名，它们是同一底层字段的不同
//! 表示，只允许在描述视觉外观的条目上不同；规范明确要求各表示的字段类型
//! `/FT`、字段值 `/V` 与默认值 `/DV` 一致，冲突即 `FormError::Malformed`。

use std::collections::HashMap;

use super::{AcroForm, Field, FieldFlags, FieldType, FormOption, Result, malformed};
use crate::core::types::{Document, Value};

/// 按全限定名归并后的逻辑字段视图。
#[derive(Debug, Clone, PartialEq)]
pub struct LogicalField<'a> {
    /// 全限定名（`/T` 链以 `.` 连接）。
    pub fully_qualified_name: String,
    /// 各表示一致的生效 `/FT`。
    pub field_type: Option<FieldType>,
    /// 各表示一致的生效 `/V`（原值保留）。
    pub value: Option<&'a Value>,
    /// 各表示一致的生效 `/DV`。
    pub default_value: Option<&'a Value>,
    /// 首个表示的生效 `/Ff`（外观类条目，允许各表示不同）。
    pub flags: FieldFlags,
    /// 首个表示的生效 `/Opt`。
    pub options: Option<&'a Vec<FormOption>>,
    /// 共享该全限定名的节点，按前序排列（含带 `/T` 的祖先节点）。
    pub representations: Vec<&'a Field>,
    /// `representations` 中声明 `/Rect` 的 Widget 表示。
    pub widgets: Vec<&'a Field>,
}

/// 前序收集字段树节点。
fn collect<'a>(field: &'a Field, out: &mut Vec<&'a Field>) {
    out.push(field);
    for kid in &field.kids {
        collect(kid, out);
    }
}

/// 按全限定名分组，顺序按名字首次出现；无名（空全限定名）节点不成组。
fn groups(fields: &[Field]) -> Vec<LogicalField<'_>> {
    let mut nodes = Vec::new();
    for field in fields {
        collect(field, &mut nodes);
    }
    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut groups: Vec<LogicalField<'_>> = Vec::new();
    for node in nodes {
        if node.fully_qualified_name.is_empty() {
            continue;
        }
        let position = match index.get(node.fully_qualified_name.as_str()) {
            Some(position) => *position,
            None => {
                groups.push(LogicalField {
                    fully_qualified_name: node.fully_qualified_name.clone(),
                    field_type: node.field_type.clone(),
                    value: node.value.as_ref(),
                    default_value: node.default_value.as_ref(),
                    flags: node.flags,
                    options: node.options.as_ref(),
                    representations: Vec::new(),
                    widgets: Vec::new(),
                });
                let position = groups.len() - 1;
                index.insert(node.fully_qualified_name.as_str(), position);
                position
            }
        };
        let group = &mut groups[position];
        group.representations.push(node);
        if node.rect.is_some() {
            group.widgets.push(node);
        }
    }
    groups
}

impl AcroForm {
    /// 同一底层字段的全部表示按全限定名归并后的视图（12.7.3.2），
    /// 顺序按名字在字段树中首次出现；未命名节点不在其中。
    pub fn logical_fields(&self) -> Vec<LogicalField<'_>> {
        groups(&self.fields)
    }

    /// 按全限定名取归并视图。
    pub fn logical_field(&self, name: &str) -> Option<LogicalField<'_>> {
        self.logical_fields()
            .into_iter()
            .find(|field| field.fully_qualified_name == name)
    }
}

/// 校验同名字段的多重表示：`/FT`、`/V`、`/DV` 须逐一致。
pub(super) fn validate_representations(document: &Document, fields: &[Field]) -> Result<()> {
    for group in groups(fields) {
        let Some((first, rest)) = group.representations.split_first() else {
            continue;
        };
        for other in rest {
            if first.field_type != other.field_type {
                return Err(malformed(format!(
                    "field {} has representations with different /FT",
                    group.fully_qualified_name
                )));
            }
            if !same_value(
                document,
                &first.value,
                &other.value,
                "/V",
                &group.fully_qualified_name,
            )? {
                return Err(malformed(format!(
                    "field {} has representations with different /V",
                    group.fully_qualified_name
                )));
            }
            if !same_value(
                document,
                &first.default_value,
                &other.default_value,
                "/DV",
                &group.fully_qualified_name,
            )? {
                return Err(malformed(format!(
                    "field {} has representations with different /DV",
                    group.fully_qualified_name
                )));
            }
        }
    }
    Ok(())
}

fn same_value(
    document: &Document,
    expected: &Option<Value>,
    actual: &Option<Value>,
    entry: &str,
    name: &str,
) -> Result<bool> {
    match (expected, actual) {
        (None, None) => Ok(true),
        (Some(left), Some(right)) => {
            let left = crate::core::reader::navigation::resolve(document, left)
                .map_err(|err| malformed(format!("field {name} {entry}: {err}")))?;
            let right = crate::core::reader::navigation::resolve(document, right)
                .map_err(|err| malformed(format!("field {name} {entry}: {err}")))?;
            Ok(left == right)
        }
        _ => Ok(false),
    }
}
