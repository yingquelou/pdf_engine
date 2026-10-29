//! Link 注解专有条目：高亮模式 `/H` 与按下 URI 动作 `/PA` 的构建态与校验
//! （12.5.6.5，Table 173）。
//!
//! `/H` PDF 1.2（缺省 `Invert`），`/PA` PDF 1.3。Table 173 把 `/PA` 定义为
//! URI 动作，因此写端只接受 [`Action::Uri`]；两个条目都只许挂在 Link 上。

use super::{Action, AnnotationKind, PdfAnnotation};

/// 链接注解高亮模式（`/H`，Table 173，PDF 1.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkHighlightMode {
    /// 不高亮（`/N`）。
    None,
    /// 反色注解矩形内容（`/I`，缺省）。
    Invert,
    /// 反色注解边框（`/O`）。
    Outline,
    /// 按下立体效果（`/P`）。
    Push,
}

impl LinkHighlightMode {
    /// 模式名（`/H` 值）。
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "N",
            Self::Invert => "I",
            Self::Outline => "O",
            Self::Push => "P",
        }
    }
}

/// Link 注解专有条目（12.5.6.5，Table 173）的构建态。
#[derive(Debug, Clone, Default)]
pub(crate) struct LinkEntries {
    /// `/H`：高亮模式（PDF 1.2）。
    highlight: Option<LinkHighlightMode>,
    /// `/PA`：按下 URI 动作（PDF 1.3）。
    pressed_action: Option<Action>,
}

impl LinkEntries {
    /// 高亮模式 `/H`。
    pub(crate) fn highlight(&self) -> Option<LinkHighlightMode> {
        self.highlight
    }

    /// 按下 URI 动作 `/PA`。
    pub(crate) fn pressed_action(&self) -> Option<&Action> {
        self.pressed_action.as_ref()
    }

    /// 是否声明了任一条目。
    fn is_declared(&self) -> bool {
        self.highlight.is_some() || self.pressed_action.is_some()
    }

    /// 校验取值；`/PA` 只承载 URI 动作（Table 173）。
    fn validate(&self) -> Result<(), String> {
        if let Some(action) = &self.pressed_action
            && !matches!(action, Action::Uri(_))
        {
            return Err("Link annotation /PA must be a URI action".into());
        }
        Ok(())
    }
}

impl PdfAnnotation {
    /// Link 高亮模式 `/H`（PDF 1.2；缺省 `Invert`）。
    pub fn with_link_highlight(mut self, highlight: LinkHighlightMode) -> Self {
        self.link.highlight = Some(highlight);
        self
    }

    /// Link 按下 URI 动作 `/PA`（PDF 1.3；仅接受 [`Action::Uri`]）。
    pub fn with_pressed_action(mut self, action: Action) -> Self {
        self.link.pressed_action = Some(action);
        self
    }
}

impl PdfAnnotation {
    /// Link 注解专有条目（Table 173）。
    pub(crate) fn link(&self) -> &LinkEntries {
        &self.link
    }

    /// Link 专有条目的子类型归属与取值校验；由 [`PdfAnnotation::validate`] 调用。
    pub(super) fn validate_link_entries(&self) -> Result<(), String> {
        if self.link.is_declared() {
            self.require_kind(AnnotationKind::Link, "Link annotation /H and /PA")?;
        }
        self.link.validate()
    }
}
