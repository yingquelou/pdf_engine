//! 分色字典写端（ISO 32000-1 14.11.4 Table 364）。
//!
//! 一组分色页共享同一个分色字典：`/Pages` 列出组内各页对象引用，
//! `/DeviceColorant` 为设备色料名，可选 `/ColorSpace` 为 Separation 或
//! DeviceN 颜色空间（8.6.6）。组由 [`PdfBuilder::add_separation_group`]
//! 注册，`build` 阶段统一校验并写出。
//!
//! [`PdfBuilder::add_separation_group`]: super::super::PdfBuilder::add_separation_group

use crate::core::builder::types::ColorSpace;

pub use crate::core::reader::separation::DeviceColorant;

/// 一组分色页共享的分色字典内容（Table 364）。
///
/// `/Pages` 由组内页号在 `build` 时生成；本类型只保存色料与可选颜色
/// 空间。`/ColorSpace` 只接受 Separation 与 DeviceN。
#[derive(Debug, Clone, PartialEq)]
pub struct SeparationInfo {
    /// `/DeviceColorant`：设备色料名（名字或文本串）。
    pub colorant: DeviceColorant,
    /// `/ColorSpace`：Separation 或 DeviceN 颜色空间。
    pub color_space: Option<ColorSpace>,
}

impl SeparationInfo {
    /// 以设备色料名构造（无 `/ColorSpace`）。
    pub fn new(colorant: DeviceColorant) -> Self {
        Self {
            colorant,
            color_space: None,
        }
    }

    /// 设置 `/ColorSpace`。
    pub fn with_color_space(mut self, space: ColorSpace) -> Self {
        self.color_space = Some(space);
        self
    }
}

/// 构建器内部的分色页组：页号（0 基）与共享字典内容。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SeparationGroup {
    /// 组内页号（0 基，注册顺序）。
    pub pages: Vec<usize>,
    /// 共享的分色字典内容。
    pub info: SeparationInfo,
}
