//! Shading 与 Pattern 读端模型（ISO 32000-1 §8.7）。
//!
//! 从 [`Document`] 解析 Shading 字典/流（Type 1–7，含网格位流的
//! `BitsPerCoordinate`/`BitsPerComponent`/`BitsPerFlag`/`Decode` 解码）、
//! PatternType 1 平铺图案（流 + 资源 + 无色基色空间）与 PatternType 2
//! 着色图案（内联字典、间接引用或命名 `/Shading` 资源）。函数维数、
//! 坐标与定义域按规范校验，颜色空间复用
//! [`crate::core::reader::color_space`] 模型。
//!
//! Type 4 网格顶点按规范保留原始记录（标志 + 坐标 + 分量），不做
//! “复用前序顶点”的三角形展开——展开属几何合成，由调用方按标志
//! 语义完成。非法结构返回类型化 [`ShadingError`]。
//!
//! 按主题目录化拆分（形态与 `builder/tests/structure/`、`builder/assemble/shading/`
//! 等已目录化模块一致）：
//! - `error`：类型化 [`ShadingError`]、错误构造辅助与资源护栏上限；
//! - `model`：Shading Type 1–7 与 Pattern 的类型化数据模型；
//! - `parse`：Shading 解析入口与 Type 1–3/4–7 渐变解析；
//! - `pattern`：Pattern 解析入口与 PatternType 1/2 图案解析；
//! - `fields`：字典字段抽取、校验与值形态强转辅助；
//! - `bitstream`：网格片记录分组与位流解码。

mod bitstream;
mod error;
mod fields;
mod model;
mod parse;
mod pattern;

pub use error::ShadingError;
pub use model::{
    AxialShading, CoonsPatchShading, FreeFormMeshShading, FunctionShading, LatticeMeshShading,
    MeshFunction, MeshVertex, PaintType, PatternModel, RadialShading, ShadingModel, ShadingOptions,
    ShadingPattern, TensorProductPatchShading, TilingPattern, TilingType,
};

#[cfg(test)]
mod tests;
