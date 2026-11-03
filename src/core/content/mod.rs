//! PDF 内容流操作符引擎。
//!
//! 目录模块：
//! - `types`：`Op` 与参数类型（`TextArrayItem` / `LineCap` / `LineJoin` /
//!   `MarkedContentProperties` / `OptionalContent`）；
//! - `serialize`：操作符 → 内容流字节（含字面量转义）与序列化错误类型；
//! - `stream`：`ContentStream` 容器与链式高层 API。
//!
//! 与库其它部分解耦：不持有资源字典映射，不负责状态机校验（BT/ET 配对
//! 等由调用方保证）。

mod serialize;
mod stream;
#[cfg(test)]
mod tests;
mod types;

pub use serialize::ContentWriteError;
pub use stream::ContentStream;
pub use types::{LineCap, LineJoin, MarkedContentProperties, Op, OptionalContent, TextArrayItem};
