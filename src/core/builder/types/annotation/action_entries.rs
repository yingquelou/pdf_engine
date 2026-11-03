//! 交互条目的构造方法与读端：触发动作 `/A` 与跳转目标 `/Dest`
//! （12.5.3、Table 173）；二者互斥的校验见 `validation`。

use super::{Action, AnnotationDestination, PdfAnnotation};

impl PdfAnnotation {
    /// 触发动作 `/A`（内联动作字典；`GoTo` 的页号在 assemble 时解析）。
    pub fn with_action(mut self, action: Action) -> Self {
        self.action = Some(action);
        self
    }

    /// 跳转目标 `/Dest`；与 `/A` 互斥（Table 173 禁止 `/A` 存在时再写 `/Dest`）。
    pub fn with_destination(mut self, destination: AnnotationDestination) -> Self {
        self.destination = Some(destination);
        self
    }

    pub(crate) fn action(&self) -> Option<&Action> {
        self.action.as_ref()
    }

    pub(crate) fn destination(&self) -> Option<&AnnotationDestination> {
        self.destination.as_ref()
    }
}
