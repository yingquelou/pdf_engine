//! 图像 XObject 与内联图像的类型化读端模型（ISO 32000-1 §8.9.5–8.9.7）。
//!
//! [`Document::image`] 把图像 XObject（流或间接引用）解析为 [`ImageModel`]：
//! `/Width` `/Height` `/ColorSpace` `/BitsPerComponent` `/Decode`
//! `/Interpolate` `/Intent` `/ImageMask` `/Mask` `/SMask` `/SMaskInData`
//! `/Alternates` 全部建模，其余条目保留在 `other`。`/Mask` 区分显式蒙版
//! （图像蒙版 XObject）与颜色键范围；`/Alternates` 递归解析为图像列表。
//! [`Document::inline_image`] 接受内容流里的 [`InlineImage`]，按 Table 94
//! 把缩写键（`/W` `/H` `/BPC` `/CS` `/D` `/F` `/DP` `/IM` `/I`）还原为
//! 标准键，`/CS` 的颜色空间名按 Table 95 还原（`/G` `/RGB` `/CMYK` `/I`），
//! 并先经过滤器解码数据；[`Document::page_inline_image`] 额外用页面
//! `/Resources` 解析非内建的命名颜色空间。
//!
//! [`ImageModel::samples`] 是样本读取 API（不做合成/渲染）：按行对齐解码
//! 位流、校验数据长度（截断/多余字节报错），对设备/CIE/ICCBased 空间应用
//! `/Decode`（缺省时 Lab 用其 `/Range`，其余用 [0 1]），Indexed 展开为
//! 基空间分量，Separation/DeviceN 经 tint 函数求值。对 Indexed /
//! Separation / DeviceN 的显式 `/Decode`（需在查色板/tint 之前映射）
//! 暂不支持，返回类型化错误而不是猜测。
//!
//! **JPXDecode（§7.4.9）只建模到语法层**：`/Filter` 含 JPX 时过滤链的
//! JPX 步骤原样透传（其余步骤照常解码），原始 JPX 字节交给
//! [`parse_jpx_header`] 解析头部语法，结果挂在 [`ImageModel::jpx`]；
//! 据此按 Table 89 忽略声明的 `/BitsPerComponent`、在 `/ColorSpace`
//! 缺失时给出设备空间提示。按设计**不解码 JPX 像素**（无小波/熵解码，
//! ICC 剖面只记长度），[`ImageModel::samples`] 对 JPX 图像返回类型化
//! `Unsupported`。边界详见 [`jpx`] 模块文档。
//!
//! **JBIG2Decode（§7.4.7）同样只建模到段语法层**：`/Filter` 含 JBIG2
//! 步骤时该步骤原样透传（其余步骤照常解码），原始段字节交给
//! [`parse_jbig2_segments`] 遍历段链；`/JBIG2Globals` 解码参数按 JBIG2
//! 步骤下标从 `/DecodeParms` 解析（数组早位可用 `null` 占位）并递归
//! 解引用，结果连同页关联、文件头/页尾/文件尾段等「存在但禁用」的
//! 类型化一致性发现挂在 [`ImageModel::jbig2`]；内联图像使用
//! JBIG2Decode 依规范（shall not）直接拒绝。按设计**不解码 JBIG2
//! 像素**（无算术/Huffman 熵解码、无符号字典重建、无区域合成），
//! [`ImageModel::samples`] 对 JBIG2 图像返回类型化 `Unsupported`。
//! 边界详见 [`jbig2`] 模块文档。

mod entries;
mod error;
mod model;
mod parse;

#[cfg(test)]
mod tests;

mod jbig2;
mod jpx;

pub use error::ImageError;
pub use jbig2::{
    Jbig2Error, Jbig2Finding, Jbig2ImageInfo, Jbig2PageInfo, Jbig2Segment, Jbig2SegmentKind,
    Jbig2Stream, parse_jbig2_segments,
};
pub use jpx::{JpxColorSpace, JpxComponent, JpxContainer, JpxHeader, parse_jpx_header};
pub use model::{ImageMaskModel, ImageModel, ImageSamples};
