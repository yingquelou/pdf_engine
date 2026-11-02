//! 图形状态参数字典（`/ExtGState`）的逻辑配置。
//!
//! 对应 PDF 32000-1 §8.4.5 Table 58。写出端在页面资源字典中登记
//! `/ExtGState` 子字典，内容流用 `/GS<n> gs` 应用；[`GraphicsState`]
//! 覆盖不依赖外部对象的标量参数、`/D`、`/BM`、`/Font`、`/SMask`
//! 软蒙版字典与 `/TR`、`/BG`、`/BG2`、`/UCR`、`/UCR2` 函数条目、
//! `/TR2` 原始字典与 `/HT` 半色调包装字典（见 `types::halftone`）。
//!
//! `/SMask` 的 `/G` 引用由构建器按文档级 Form XObject 句柄解析为对象号，
//! 被引用的表单须是透明组（带 `/Group`），并在构建阶段连同表单对象写出。

mod dictionary;
mod functions;
mod model;
mod options;

pub(crate) use functions::GraphicsStateFunctions;
pub use model::{DashPattern, GraphicsState, GraphicsStateFont, SoftMask, SoftMaskSubtype};
