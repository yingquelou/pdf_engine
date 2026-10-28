//! 注解子类型专有条目：构造方法、取值访问与构建期校验（12.5.6）。
//!
//! 字段本身声明在 [`PdfAnnotation`]（父模块）；子类型归属与取值面的
//! 约束集中在本文件，非法一律 `InvalidObject`，且全部校验发生在分配对象号之前。

use super::{AnnotationKind, PdfAnnotation};

impl PdfAnnotation {
    /// Text 注解图标名 `/Name`（仅 Text 子类型；取值由查看器定义，写成名字）。
    pub fn with_icon_name(mut self, name: impl Into<String>) -> Self {
        self.icon_name = Some(name.into());
        self
    }

    /// Text 注解气泡展开状态 `/Open`（仅 Text 子类型）。
    pub fn with_open(mut self, open: bool) -> Self {
        self.is_open = Some(open);
        self
    }

    /// Text 注解标记状态 `/State`（文本串；标准值随 `/StateModel` 而变，PDF 1.3）。
    pub fn with_state(mut self, state: impl Into<String>) -> Self {
        self.state = Some(state.into());
        self
    }

    /// Text 注解状态模型 `/StateModel`（文本串，缺省 `Marked`，PDF 1.3）。
    pub fn with_state_model(mut self, model: impl Into<String>) -> Self {
        self.state_model = Some(model.into());
        self
    }

    /// Caret 注解符号 `/Symbol`（名字；标准值 `NewLine`、`Cross`）。
    pub fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = Some(symbol.into());
        self
    }

    /// Ink 注解路径 `/InkList`；每次调用追加一条 x/y 交替的路径（PDF 1.4）。
    pub fn with_ink_path(mut self, path: Vec<f64>) -> Self {
        self.ink_paths.push(path);
        self
    }
}

impl PdfAnnotation {
    pub(crate) fn icon_name(&self) -> Option<&str> {
        self.icon_name.as_deref()
    }

    pub(crate) fn is_open(&self) -> Option<bool> {
        self.is_open
    }

    pub(crate) fn state(&self) -> Option<&str> {
        self.state.as_deref()
    }

    pub(crate) fn state_model(&self) -> Option<&str> {
        self.state_model.as_deref()
    }

    pub(crate) fn symbol(&self) -> Option<&str> {
        self.symbol.as_deref()
    }

    pub(crate) fn ink_paths(&self) -> &[Vec<f64>] {
        &self.ink_paths
    }

    /// 是否声明了 Text 标记状态条目（`/State`、`/StateModel`，PDF 1.3）。
    pub(crate) fn has_markup_state(&self) -> bool {
        self.state.is_some() || self.state_model.is_some()
    }

    /// 校验子类型专有条目的归属与取值；由 [`PdfAnnotation::validate`] 调用。
    pub(super) fn validate_subtype_entries(&self) -> Result<(), String> {
        if self.icon_name.is_some() || self.is_open.is_some() {
            self.require_kind(AnnotationKind::Text, "/Name and /Open")?;
        }
        if self.has_markup_state() {
            self.require_kind(AnnotationKind::Text, "/State and /StateModel")?;
        }
        if self.symbol.is_some() {
            self.require_kind(AnnotationKind::Caret, "/Symbol")?;
        }
        if !self.ink_paths.is_empty() {
            self.require_kind(AnnotationKind::Ink, "/InkList")?;
        }
        if let Some(icon) = &self.icon_name {
            validate_name_value(icon, "annotation /Name")?;
        }
        if let Some(symbol) = &self.symbol {
            validate_name_value(symbol, "annotation /Symbol")?;
        }
        for value in [&self.state, &self.state_model] {
            if let Some(text) = value
                && text.is_empty()
            {
                return Err("annotation /State and /StateModel must not be empty".into());
            }
        }
        for path in &self.ink_paths {
            if path.len() % 2 != 0 || path.is_empty() {
                return Err(
                    "annotation /InkList path must hold an even, nonzero number of values".into(),
                );
            }
            if path.iter().any(|v| !v.is_finite()) {
                return Err("annotation /InkList entries must be finite".into());
            }
        }
        Ok(())
    }
}

/// 名字型条目取值：非空且不含空白与 PDF 名字定界符。
fn validate_name_value(value: &str, context: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("{context} must not be empty"));
    }
    if value
        .chars()
        .any(|c| c.is_whitespace() || "/[]()<>{}".contains(c) || c == '%')
    {
        return Err(format!("{context} contains name delimiters: {value}"));
    }
    Ok(())
}
