//! 嵌入文件（7.11.3–7.11.4）构建态：文件规格条目 + 嵌入文件流字节。
//!
//! 一次 [`EmbeddedFile`] 注册在 `build` 时展开为一对间接对象：文件规格
//! 字典（Table 44，`/F` `/UF` `/Desc` `/EF`）与嵌入文件流（Table 45，
//! `/Subtype` MIME 名 + `/Params` 文件参数），并登记进目录
//! `/Names /EmbeddedFiles` 名称树；句柄可供 FileAttachment 注解的 `/FS`
//! 引用（12.5.6.14）。

use md5::{Digest, Md5};

/// 嵌入文件：名称、说明、MIME 子类型与文件字节。
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddedFile {
    name: String,
    short_name: Option<String>,
    description: Option<String>,
    subtype: Option<String>,
    creation_date: Option<String>,
    modification_date: Option<String>,
    data: Vec<u8>,
}

impl EmbeddedFile {
    /// 新建嵌入文件；`name` 为展示名（≥1.7 写出为 `/UF`，ASCII 时兼作 `/F`），
    /// `data` 为文件原始字节。
    pub fn new(name: impl Into<String>, data: impl Into<Vec<u8>>) -> Self {
        Self {
            name: name.into(),
            short_name: None,
            description: None,
            subtype: None,
            creation_date: None,
            modification_date: None,
            data: data.into(),
        }
    }

    /// 覆盖 `/F` 7 位 ASCII 短名；非 ASCII 展示名必须显式提供。
    pub fn with_short_name(mut self, short_name: impl Into<String>) -> Self {
        self.short_name = Some(short_name.into());
        self
    }

    /// `/Desc` 说明文本串（PDF 1.6）。
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// 嵌入文件流 `/Subtype` 的 MIME 类型名，如 `application/pdf`
    /// （`/` 由写出层转义为 `#2F`）。
    pub fn with_subtype(mut self, subtype: impl Into<String>) -> Self {
        self.subtype = Some(subtype.into());
        self
    }

    /// `/Params` `/CreationDate` 日期文本串（如 `D:20261028050000Z`）。
    pub fn with_creation_date(mut self, date: impl Into<String>) -> Self {
        self.creation_date = Some(date.into());
        self
    }

    /// `/Params` `/ModDate` 日期文本串。
    pub fn with_modification_date(mut self, date: impl Into<String>) -> Self {
        self.modification_date = Some(date.into());
        self
    }

    /// 展示名（名称树键与 `/UF`）。
    pub fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub(crate) fn subtype(&self) -> Option<&str> {
        self.subtype.as_deref()
    }

    pub(crate) fn creation_date(&self) -> Option<&str> {
        self.creation_date.as_deref()
    }

    pub(crate) fn modification_date(&self) -> Option<&str> {
        self.modification_date.as_deref()
    }

    pub(crate) fn data(&self) -> &[u8] {
        &self.data
    }

    /// 是否只有 ASCII 展示名（`/UF` 仅 ≥1.7 写出，低版本可省）。
    pub(crate) fn name_is_ascii(&self) -> bool {
        self.name.is_ascii()
    }

    /// `/F` 短名：显式覆盖优先，否则取 ASCII 展示名。
    pub(crate) fn short_name(&self) -> Option<&str> {
        self.short_name
            .as_deref()
            .or(self.name.is_ascii().then_some(self.name.as_str()))
    }

    /// 7.11.4.3：`/CheckSum` 为去掉数据中 ASCII 空白字符（NUL、TAB、LF、
    /// FF、CR、SPACE）后的 MD5 摘要。
    pub(crate) fn checksum(&self) -> [u8; 16] {
        let filtered: Vec<u8> = self
            .data
            .iter()
            .copied()
            .filter(|byte| !matches!(byte, 0 | 0o11 | 0o12 | 0o14 | 0o15 | b' '))
            .collect();
        Md5::digest(&filtered).into()
    }

    /// 校验名称与配置；错误串直接用于
    /// [`WriteError`](crate::core::writer::WriteError)。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("embedded file name must not be empty".into());
        }
        if self.name.contains('/') || self.name.contains('\\') {
            return Err(format!(
                "embedded file name must not contain path separators, got {:?}",
                self.name
            ));
        }
        if let Some(short) = &self.short_name {
            if short.is_empty() {
                return Err("embedded file /F short name must not be empty".into());
            }
            if !short.is_ascii() {
                return Err("embedded file /F short name must be pure ASCII".into());
            }
            if short.contains('/') || short.contains('\\') {
                return Err("embedded file /F short name must not contain path separators".into());
            }
        } else if !self.name.is_ascii() {
            return Err(format!(
                "non-ASCII embedded file name {:?} requires an explicit ASCII short name",
                self.name
            ));
        }
        if self.subtype.as_deref().is_some_and(str::is_empty) {
            return Err("embedded file /Subtype must not be empty".into());
        }
        for (key, date) in [
            ("/CreationDate", &self.creation_date),
            ("/ModDate", &self.modification_date),
        ] {
            if date.as_deref().is_some_and(str::is_empty) {
                return Err(format!("embedded file /Params {key} must not be empty"));
            }
        }
        Ok(())
    }
}
