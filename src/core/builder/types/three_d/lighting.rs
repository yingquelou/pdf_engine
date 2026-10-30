//! 3D 光照方案字典（Table 309/310）的构建态。
//!
//! `/Type` 恒为 `3DLightingScheme`，`/Subtype` 由
//! [`ThreeDLightingScheme::new`] 显式声明并总是写出；`Other` 方案名须为
//! 合法 PDF 名字，否则写出前拒绝。

use super::super::validate_name_value;
use crate::core::reader::three_d::ThreeDLightingSchemeType;
use crate::core::types::{Dict, Value};

/// 3D 光照方案字典（Table 309）的构建态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreeDLightingScheme {
    subtype: ThreeDLightingSchemeType,
}

impl ThreeDLightingScheme {
    /// 新建光照方案字典；`subtype` 为 `/Subtype`（Table 310）。
    pub fn new(subtype: ThreeDLightingSchemeType) -> Self {
        Self { subtype }
    }

    /// 校验 `/Subtype` 名字语法。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if let ThreeDLightingSchemeType::Other(name) = &self.subtype {
            validate_name_value(name, "3D lighting scheme /Subtype")?;
        }
        Ok(())
    }

    /// 写成光照方案字典。
    pub(crate) fn to_dict(&self) -> Dict {
        Dict::new()
            .insert("Type", Value::Name("3DLightingScheme".into()))
            .insert("Subtype", Value::Name(self.subtype.as_name().to_string()))
    }
}
