//! 从 [`Document`] 出发的 JBIG2/JPX 解码入口：把文档声明版本注入后端
//! 请求，使 [`super::check_codec_version`] 版本门禁在生产路径上端到端
//! 生效。
//!
//! [`ImageModel::decode_image`] 的便捷入口 [`ImageModel::decode_image_with`]
//! 不携带文档版本、恒传 `pdf_version: None`（保持既有语义不变）；需要版本
//! 门禁的调用方走本模块的 `Document` 入口，由 `Document` 读取
//! [`Document::source_version`] 注入请求。
//!
//! 版本来源沿用仓库既有口径 [`Document::source_version`]（读端在解析时
//! 填充的文档声明版本），与流内可能出现的 `/Version` 分歧时不引入新规则：
//! 一律以 `source_version()` 为准。本增量只做版本注入与分发通路，不含任何
//! 像素/码流解码。

use crate::core::types::{Document, Value};

use super::super::error::Result;
use super::super::model::ImageModel;
use super::{DecodedBitmap, ImageBackends};

impl Document {
    /// 用显式注册的后端集合解码一个图像 XObject，并把文档声明版本注入
    /// 版本门禁。
    ///
    /// `value` 与 [`Document::image`] 接受同一形态：图像流本身或指向它的
    /// 间接引用；本方法先经 [`Document::image`] 解析为 [`ImageModel`]，再
    /// 以 `self.source_version()` 分发到 [`super::decode_jbig2_with`] /
    /// [`super::decode_jpx_with`]。
    ///
    /// 语义与 [`ImageModel::decode_image_with`] 一致，唯一差别是这里注入
    /// 文档版本：声明版本低于规范门（JBIG2 需 PDF 1.4、JPX 需 PDF 1.5）时，
    /// 经 [`super::check_codec_version`] 返回类型化 `Unsupported`，后端
    /// **零调用**；版本达标或未注册后端时维持既有 `Unsupported` 口径。
    /// 非 JBIG2/JPX 图像提示改用 [`ImageModel::samples`]。不新增任何错误
    /// 枚举变体。
    pub fn decode_image_with(
        &self,
        value: &Value,
        backends: &ImageBackends<'_>,
    ) -> Result<DecodedBitmap> {
        let model = self.image(value)?;
        self.decode_image_model_with(&model, backends)
    }

    /// 已解析 [`ImageModel`] 的解码入口（[`Document::page_image`] 的结果
    /// 可直接接入），把文档声明版本注入版本门禁。语义同
    /// [`Document::decode_image_with`]。
    pub fn decode_image_model_with(
        &self,
        model: &ImageModel,
        backends: &ImageBackends<'_>,
    ) -> Result<DecodedBitmap> {
        model.decode_image_at(backends, Some(self.source_version()))
    }
}
