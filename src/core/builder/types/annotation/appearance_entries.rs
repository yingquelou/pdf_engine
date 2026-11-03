//! 外观流条目的构造方法与读端：`/AP` 字典的常态 `/N`、按下态 `/D`、悬停态
//! `/R` 流与状态子字典，以及选中状态 `/AS`（12.5.5、Table 165/167）。

use super::PdfAnnotation;

impl PdfAnnotation {
    /// 常态外观流（`/AP` 字典 `/N`）；参数为 [`FormXObject`](super::super::FormXObject) 句柄。
    pub fn with_appearance(mut self, handle: usize) -> Self {
        self.appearance_normal = Some(handle);
        self
    }

    /// 按下态外观流（`/AP` 字典 `/D`）；参数为 Form XObject 句柄。
    pub fn with_appearance_down(mut self, handle: usize) -> Self {
        self.appearance_down = Some(handle);
        self
    }

    /// 悬停态外观流（`/AP` 字典 `/R`）；参数为 Form XObject 句柄。
    pub fn with_appearance_rollover(mut self, handle: usize) -> Self {
        self.appearance_rollover = Some(handle);
        self
    }

    /// 常态外观状态子字典 `/AP /N << /State … >>` 的一项（PDF 1.2）；
    /// 参数为状态名与该状态的 Form XObject 句柄。与 [`Self::with_appearance`]
    /// 互斥（`/N` 不能同时是流与字典）。
    pub fn with_appearance_state(mut self, state: impl Into<String>, handle: usize) -> Self {
        self.appearance_states.push((state.into(), handle));
        self
    }

    /// 选中的外观状态 `/AS`；须已在 `/AP /N` 状态子字典注册。
    pub fn with_selected_appearance_state(mut self, state: impl Into<String>) -> Self {
        self.appearance_state = Some(state.into());
        self
    }

    pub(crate) fn appearance_normal(&self) -> Option<usize> {
        self.appearance_normal
    }

    pub(crate) fn appearance_down(&self) -> Option<usize> {
        self.appearance_down
    }

    pub(crate) fn appearance_rollover(&self) -> Option<usize> {
        self.appearance_rollover
    }

    pub(crate) fn appearance_states(&self) -> &[(String, usize)] {
        &self.appearance_states
    }

    pub(crate) fn appearance_state(&self) -> Option<&str> {
        self.appearance_state.as_deref()
    }

    /// 是否声明了任意 `/AP` 外观流。
    pub(crate) fn has_appearance(&self) -> bool {
        self.appearance_normal.is_some()
            || self.appearance_down.is_some()
            || self.appearance_rollover.is_some()
            || !self.appearance_states.is_empty()
    }
}
