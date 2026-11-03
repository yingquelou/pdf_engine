//! 通用文本条目（Table 165/166）的构造方法与读端：`/F` 标志、`/Contents`
//! 正文、`/T` 弹出窗口标题、`/NM` 注解名、`/M` 修改日期与 `/CreationDate`
//! 创建日期。

use super::PdfAnnotation;

impl PdfAnnotation {
    /// 注解标志位（Table 165；如 1=无缩放、4=打印）。
    pub fn with_flags(mut self, flags: u32) -> Self {
        self.flags = flags;
        self
    }

    /// 正文内容 `/Contents`。
    pub fn with_contents(mut self, contents: impl Into<String>) -> Self {
        self.contents = Some(contents.into());
        self
    }

    /// 弹出窗口标题 `/T`。
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// 注解名 `/NM`。
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// 修改日期 `/M`（日期字符串，如 `D:20261027120000`）。
    pub fn with_modified_date(mut self, date: impl Into<String>) -> Self {
        self.modified_date = Some(date.into());
        self
    }

    /// 创建日期 `/CreationDate`（扩展条目；Widget/标注工具使用）。
    pub fn with_creation_date(mut self, date: impl Into<String>) -> Self {
        self.creation_date = Some(date.into());
        self
    }

    /// 标志位。
    pub(crate) fn flags(&self) -> u32 {
        self.flags
    }

    pub(crate) fn contents(&self) -> Option<&str> {
        self.contents.as_deref()
    }

    pub(crate) fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub(crate) fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub(crate) fn modified_date(&self) -> Option<&str> {
        self.modified_date.as_deref()
    }

    pub(crate) fn creation_date(&self) -> Option<&str> {
        self.creation_date.as_deref()
    }
}
