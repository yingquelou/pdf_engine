//! Shading/Pattern reader model tests.
//!
//! 按主题目录化拆分（形态与 `builder/tests/structure/`、`builder/tests/shading/`
//! 的目录化测试一致）：
//! - `support`：跨主题共享夹具（文档/对象值构造与渐变字典样本）；
//! - `simple`：Type 1–3 简单渐变的解析与校验；
//! - `mesh_decode`：Type 4–7 网格/网格片渐变的位流解码与分组；
//! - `pattern`：PatternType 1/2 图案的解析与校验；
//! - `page_resources`：页面 `/Shading`、`/Pattern` 命名资源解析；
//! - `errors`：类型化错误、访问器与色空间分量语义。

mod errors;
mod mesh_decode;
mod page_resources;
mod pattern;
mod simple;
mod support;
