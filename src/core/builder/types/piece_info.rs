//! 页面件字典（14.5 Page-Piece Dictionaries，Table 318/319）写端模型。

use crate::core::types::{Dict, PdfDate, Value};
use crate::core::writer::WriteError;

/// 单个产品的 data dictionary（Table 319）。
#[derive(Debug, Clone, PartialEq)]
pub struct PieceInfoData {
    last_modified: PdfDate,
    private: Option<Value>,
}

impl PieceInfoData {
    /// 以 `/LastModified` 日期构造。
    pub fn new(last_modified: PdfDate) -> Self {
        Self {
            last_modified,
            private: None,
        }
    }

    /// 设置 `/Private` 私有数据（任意值，原样写出）。
    pub fn with_private(mut self, private: Value) -> Self {
        self.private = Some(private);
        self
    }

    fn to_dict(&self) -> Dict {
        let mut dict = Dict::new().insert(
            "LastModified",
            Value::String(self.last_modified.to_pdf_string()),
        );
        if let Some(private) = &self.private {
            dict = dict.insert("Private", private.clone());
        }
        dict
    }
}

/// 页面件字典：产品名 → data dictionary（Table 318）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PieceInfo {
    entries: Vec<(String, PieceInfoData)>,
}

impl PieceInfo {
    /// 空页面件字典。
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加一个产品的 data dictionary；产品名不得为空或重复。
    pub fn with(mut self, product: impl Into<String>, data: PieceInfoData) -> Self {
        self.entries.push((product.into(), data));
        self
    }

    /// 校验产品名非空且唯一。
    pub(crate) fn validate(&self) -> Result<(), WriteError> {
        let mut seen = std::collections::HashSet::with_capacity(self.entries.len());
        for (product, _) in &self.entries {
            if product.is_empty() {
                return Err(WriteError::InvalidObject(
                    "page-piece product name must not be empty".into(),
                ));
            }
            if !seen.insert(product.as_str()) {
                return Err(WriteError::InvalidObject(format!(
                    "page-piece product name {product:?} appears more than once"
                )));
            }
        }
        Ok(())
    }

    /// 写出为 `/PieceInfo` 字典。
    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new();
        for (product, data) in &self.entries {
            dict = dict.insert(product.clone(), Value::Dict(data.to_dict()));
        }
        dict
    }
}
