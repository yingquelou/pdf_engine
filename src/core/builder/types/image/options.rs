//! [`ImageData`] 的链式可选条目设置：`/Decode`、`/Interpolate`、`/Intent`、
//! `/Alternates`、`/Matte`、`/SMask`、`/ImageMask`、`/Mask` 与 `/OPI`。

use crate::core::builder::types::OpiProxy;

use super::image_data::{ImageData, ImageIntent, ImageMask};

impl ImageData {
    /// 设置 `/Decode` 数组。
    pub fn with_decode(mut self, decode: impl Into<Vec<f64>>) -> Self {
        self.decode = Some(decode.into());
        self
    }

    /// 设置 `/Interpolate`。
    pub fn with_interpolate(mut self, interpolate: bool) -> Self {
        self.interpolate = interpolate;
        self
    }

    /// 设置 `/Intent`。
    pub fn with_intent(mut self, intent: ImageIntent) -> Self {
        self.intent = Some(intent);
        self
    }

    /// 追加替代图像（`/Alternates`）。
    pub fn with_alternates(mut self, alternates: impl Into<Vec<ImageData>>) -> Self {
        self.alternates = alternates.into();
        self
    }

    /// 设置 `/Matte` 衬底颜色。
    pub fn with_matte(mut self, matte: impl Into<Vec<f64>>) -> Self {
        self.matte = Some(matte.into());
        self
    }

    /// 附加显式软蒙版图像。
    pub fn with_smask(mut self, smask: ImageData) -> Self {
        self.smask = Some(Box::new(smask));
        self
    }

    /// 把本图标记为图像蒙版（`/ImageMask true`）。
    ///
    /// 要求 `bits == 1` 且 `color_space` 为 `DeviceGray`；`/ColorSpace`
    /// 与 `/Mask` 不会写出，`/Decode` 只允许 `[0 1]` 或 `[1 0]`。
    pub fn with_image_mask(mut self) -> Self {
        self.image_mask = true;
        self
    }

    /// 附加 `/Mask` 条目（显式蒙版或颜色键蒙版）。
    pub fn with_mask(mut self, mask: ImageMask) -> Self {
        self.mask = Some(mask);
        self
    }

    /// 设置 `/OPI` 代理字典（§14.11.7，PDF 1.2）。
    ///
    /// 目标版本低于 1.2 时构建失败（`WriteError::UnsupportedVersion`）。
    pub fn with_opi(mut self, opi: OpiProxy) -> Self {
        self.opi = Some(opi);
        self
    }
}
