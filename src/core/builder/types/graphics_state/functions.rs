use crate::core::types::Value;

use super::model::GraphicsState;

/// ExtGState 中函数类条目的已解析值（由组装阶段按句柄生成）。
#[derive(Debug, Default)]
pub(crate) struct GraphicsStateFunctions {
    pub transfer: Option<Value>,
    pub black_generation: Option<Value>,
    pub black_generation_2: Option<Value>,
    pub undercolor_removal: Option<Value>,
    pub undercolor_removal_2: Option<Value>,
    pub smask_transfer: Option<Value>,
    /// `/HT` 包装字典已解析出的值（函数句柄已替换为对象值）。
    pub halftone: Option<Value>,
}

impl GraphicsState {
    /// 函数类条目（名称与函数句柄），供组装阶段解析为字典值。
    pub(crate) fn function_handles(&self) -> impl Iterator<Item = (&'static str, usize)> + '_ {
        [
            ("TR", self.transfer_function),
            ("BG", self.black_generation),
            ("BG2", self.black_generation_2),
            ("UCR", self.undercolor_removal),
            ("UCR2", self.undercolor_removal_2),
        ]
        .into_iter()
        .filter_map(|(name, handle)| handle.map(|handle| (name, handle)))
    }

    /// 是否不依赖外部对象（`/Font`、`/SMask`、函数句柄或 `/HT`）。
    /// `/DR` 默认 ExtGState 只接受自包含状态。
    pub(crate) fn is_self_contained(&self) -> bool {
        self.font.is_none()
            && self.soft_mask.is_none()
            && self.halftone.is_none()
            && self.auxiliary_function_handles().is_empty()
    }

    /// 须为单输出的附加函数句柄：`/SMask /TR` 与 `/HT` 内函数。
    pub(crate) fn auxiliary_function_handles(&self) -> Vec<(&'static str, usize)> {
        let mut handles = Vec::new();
        if let Some(soft_mask) = &self.soft_mask
            && let Some(handle) = soft_mask.transfer_function
        {
            handles.push(("SMask /TR", handle));
        }
        if let Some(halftone) = &self.halftone {
            handles.extend(halftone.function_handles().map(|handle| ("HT", handle)));
        }
        handles
    }
}
