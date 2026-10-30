//! 输出意图写端（ISO 32000-1 14.11.5 Table 365）。
//!
//! [`OutputIntent`] 是目录 `/OutputIntents` 数组条目的构建态：`/S` 复用
//! 读端子类型枚举，文本条目按 PDF 文本串写成十六进制字符串，`/DestOutputProfile`
//! 写成独立 ICC 配置文件流（`/N` 1–4）。Table 365 的两项条件性要求只在
//! 可判定时强制：`/OutputConditionIdentifier` 为 `Custom` 时须同时带
//! `/Info` 与 `/DestOutputProfile`（由 assemble 层校验）。

use crate::core::reader::output_intent::OutputIntentSubtype;

/// 输出配置文件（`/DestOutputProfile` 流）的构建态。
#[derive(Debug, Clone, PartialEq)]
pub struct OutputIntentProfile {
    /// `/N`：颜色分量数，1–4。
    pub components: u8,
    /// ICC 配置文件字节（原样写为流数据）。
    pub data: Vec<u8>,
}

impl OutputIntentProfile {
    /// 按分量数与配置文件字节构造。
    pub fn new(components: u8, data: impl Into<Vec<u8>>) -> Self {
        Self {
            components,
            data: data.into(),
        }
    }
}

/// 一个输出意图字典的构建态（Table 365）。
#[derive(Debug, Clone, PartialEq)]
pub struct OutputIntent {
    /// `/S`：输出意图子类型。
    pub subtype: OutputIntentSubtype,
    /// `/OutputCondition`：人类可读输出条件。
    pub output_condition: Option<String>,
    /// `/OutputConditionIdentifier`：输出条件标识（必填，须非空）。
    pub identifier: String,
    /// `/RegistryName`：注册表 URI。
    pub registry_name: Option<String>,
    /// `/Info`：补充说明。
    pub info: Option<String>,
    /// `/DestOutputProfile`：ICC 配置文件流。
    pub profile: Option<OutputIntentProfile>,
}

impl OutputIntent {
    /// 新建输出意图；`identifier` 为必填输出条件标识。
    pub fn new(subtype: OutputIntentSubtype, identifier: impl Into<String>) -> Self {
        Self {
            subtype,
            output_condition: None,
            identifier: identifier.into(),
            registry_name: None,
            info: None,
            profile: None,
        }
    }

    /// 设置 `/OutputCondition`。
    pub fn with_output_condition(mut self, condition: impl Into<String>) -> Self {
        self.output_condition = Some(condition.into());
        self
    }

    /// 设置 `/RegistryName`。
    pub fn with_registry_name(mut self, registry: impl Into<String>) -> Self {
        self.registry_name = Some(registry.into());
        self
    }

    /// 设置 `/Info`。
    pub fn with_info(mut self, info: impl Into<String>) -> Self {
        self.info = Some(info.into());
        self
    }

    /// 设置 `/DestOutputProfile`。
    pub fn with_destination_profile(mut self, profile: OutputIntentProfile) -> Self {
        self.profile = Some(profile);
        self
    }
}
