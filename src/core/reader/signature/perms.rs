//! 目录 `/Perms`（12.8.5 Table 268）条目模型。

use crate::core::types::Value;

use super::entries::DocumentSignature;
use super::reference::DocmdpPermission;

/// 目录 `/Perms` 的标准条目名（12.8.5 Table 268）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermsEntryName {
    /// `/DocMDP`：文档修改防护（DocMDP 变换）。
    DocumentPermissions,
    /// `/UR`：允许阅读器更新表单（UR 变换）。
    UniversalResources,
    /// `/Form`：表单字段变更允许（FieldMDP 变换）。
    FormFieldChanges,
    /// `/SigFlags`：签署状态位。
    SignatureFlag,
    /// 其他/自定义条目，原样保留名称。
    Unknown(String),
}

impl PermsEntryName {
    /// 由 `/Perms` 键名解析。
    pub fn from_key(key: &str) -> Self {
        match key {
            "DocMDP" => Self::DocumentPermissions,
            "UR" => Self::UniversalResources,
            "Form" => Self::FormFieldChanges,
            "SigFlags" => Self::SignatureFlag,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// 键名（不含前导 `/`）。
    pub fn as_str(&self) -> &str {
        match self {
            Self::DocumentPermissions => "DocMDP",
            Self::UniversalResources => "UR",
            Self::FormFieldChanges => "Form",
            Self::SignatureFlag => "SigFlags",
            Self::Unknown(name) => name,
        }
    }
}

/// 目录 `/Perms` 的单条条目（Table 268）。
#[derive(Debug, Clone, PartialEq)]
pub struct PermsEntry {
    /// 条目键的类型化名称。
    pub name: PermsEntryName,
    /// 原始值（不解析，可能是引用/整数/日期）。
    pub raw: Value,
    /// 值解析为字典后的签名字典（仅当 `/Type /Sig`）；否则 `None`。
    pub signature: Option<DocumentSignature>,
    /// 文档修改防护级别：仅 `/DocMDP` 字典带整数 `/P` 时填充。
    pub permission: Option<DocmdpPermission>,
}
