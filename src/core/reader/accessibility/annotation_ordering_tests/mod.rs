//! 注解页面内容序列（ISO 32000-1 14.8.2.3.2）的读端单元测试。
//!
//! 手工夹具覆盖规范的四句话：`Annot`（PDF 1.5）/`Link`/`Form` 元素的显式
//! 关联、邻接在前、邻接在后，以及「位置无从判定」；再加 NOTE 的空标记内容
//! 序列锚点、跨页声称、没有结构树与畸形/环状写法的降级。
//!
//! 约定 5（单文件不越 600 行/25 KiB）：夹具在 `support`，用例按主题分
//! `association`（显式关联）、`adjacency`（邻接与锚点）、`degradation`
//! （跨页、无结构与类型化降级）三个子模块。

mod adjacency;
mod association;
mod degradation;
mod support;
