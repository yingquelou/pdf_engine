//! 渐变（Shading）与着色图案（Shading Pattern）：ISO 32000-1 §8.7.4.2、
//! §8.7.4.3 与 §8.7.2。
//!
//! 支持 Type 1（函数式）、Type 2（轴向）、Type 3（径向）、Type 4（自由
//! 三角网格）、Type 5（栅格三角网格）、Type 6（Coons 网格）与 Type 7
//! （张量积网格）渐变字典，以及引用渐变的 PatternType 2 图案字典。
//! 所有类型共有的可选条目 `/Background`、`/BBox` 与 `/AntiAlias` 由
//! [`ShadingOptions`] 建模；网格渐变（Type 4–7）可经 `with_function`
//! 配置颜色映射函数，此时每个顶点只携带一个参数变量而非完整颜色分量。
//!
//! 按主题目录化拆分（逐行搬运自原单文件）：
//! - `validate`：共享的数值有限性、参数域、颜色函数与网格位深校验；
//! - `options`：[`ShadingOptions`]（Table 78 所有类型共有的可选条目）；
//! - `gradient`：Type 1–3 函数式/轴向/径向渐变；
//! - `mesh`：网格顶点、颜色映射函数与 `/Decode` 解码范围；
//! - `mesh_grid`：Type 4–5 三角网格渐变；
//! - `patch`：Coons 与张量积网格片及其 Type 6–7 渐变；
//! - `dict`：`Shading` 联合字典（Type 1–7）；
//! - `pattern`：PatternType 2 着色图案字典。

mod dict;
mod gradient;
mod mesh;
mod mesh_grid;
mod options;
mod patch;
mod pattern;
mod validate;

pub use dict::Shading;
pub use gradient::{AxialShading, FunctionShading, RadialShading};
pub use mesh::{MeshFunction, MeshVertex};
pub use mesh_grid::{FreeFormMeshShading, LatticeMeshShading};
pub use options::ShadingOptions;
pub use patch::{CoonsPatch, CoonsPatchShading, TensorProductPatch, TensorProductPatchShading};
pub use pattern::ShadingPattern;

pub(crate) use mesh::mesh_component_ranges;
