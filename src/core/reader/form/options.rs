//! `/Opt` 选项数组（Table 227 按钮字段 / Table 231 选择字段）。
//!
//! 选择字段每个元素是文本串或 `[导出值 显示文本]` 两元素数组（Table 231）；
//! 复选框与单选按钮字段每个元素只能是单个文本串，表示对应 Widget 的开
//! 状态（Table 227，PDF 1.4）。

use super::{FieldType, Result, malformed};
use crate::core::reader::content::text_value;
use crate::core::types::{Document, Value};

/// 选项条目（`/Opt` 数组元素）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormOption {
    export: String,
    display: Option<String>,
}

impl FormOption {
    /// 导出值；按钮字段中为对应 Widget 开状态的导出值。
    pub fn export(&self) -> &str {
        &self.export
    }

    /// 显示文本；仅选择字段的 `[导出值 显示文本]` 元素提供。
    pub fn display(&self) -> Option<&str> {
        self.display.as_deref()
    }
}

/// 解析 `/Opt`：按钮字段要求单个文本串（Table 227），其余字段接受
/// 文本串或两元素 `[导出值 显示文本]` 数组（Table 231）。
pub(super) fn parse_options(
    document: &Document,
    value: &Value,
    field_type: Option<&FieldType>,
) -> Result<Vec<FormOption>> {
    let resolved = crate::core::reader::navigation::resolve(document, value)
        .map_err(|err| malformed(err.to_string()))?;
    let array = resolved
        .as_array()
        .ok_or_else(|| malformed("form field /Opt is not an array"))?;
    let button = matches!(field_type, Some(FieldType::Button));
    let mut options = Vec::with_capacity(array.0.len());
    for entry in &array.0 {
        let resolved = crate::core::reader::navigation::resolve(document, entry)
            .map_err(|err| malformed(err.to_string()))?;
        match &resolved {
            Value::String(_) | Value::HexString(_) => {
                let export = text_value(&resolved).ok_or_else(|| {
                    malformed("form field /Opt entry is not a decodable text string")
                })?;
                options.push(FormOption {
                    export,
                    display: None,
                });
            }
            Value::Array(pair) if !button => {
                if pair.0.len() != 2 {
                    return Err(malformed(
                        "choice field /Opt entries must be a text string or two text strings",
                    ));
                }
                let export = resolve_text(document, &pair.0[0], "export value")?;
                let display = resolve_text(document, &pair.0[1], "display value")?;
                options.push(FormOption {
                    export,
                    display: Some(display),
                });
            }
            Value::Array(_) => {
                return Err(malformed(
                    "button field /Opt entries must be single text strings",
                ));
            }
            _ => return Err(malformed("form field /Opt entry is not a text string")),
        }
    }
    Ok(options)
}

fn resolve_text(document: &Document, value: &Value, role: &'static str) -> Result<String> {
    let resolved = crate::core::reader::navigation::resolve(document, value)
        .map_err(|err| malformed(err.to_string()))?;
    if !matches!(resolved, Value::String(_) | Value::HexString(_)) {
        return Err(malformed(format!(
            "choice field /Opt {role} is not a text string"
        )));
    }
    text_value(&resolved)
        .ok_or_else(|| malformed(format!("choice field /Opt {role} is not decodable")))
}
