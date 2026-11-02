//! PostScript Type 1 font program (`/FontFile`) parsing.
//!
//! 按主题目录化拆分（形态与 `reader/shading/` 等已目录化模块一致）：
//! - `error`：Type 1 程序解析失败的共享错误构造辅助；
//! - `scanner`：PostScript 词法扫描器（`Token` 与 `Scanner`）；
//! - `header`：明文头解析与类型化 `Type1Header`/`Type1BuiltinEncoding`；
//! - `crypto`：Type 1 解密原语、`/lenIV` 段解密与 `eexec` 段边界；
//! - `private_dict`：Private 字典解析（`/CharStrings` 与 `/Subrs` 的 `RD` 段）；
//! - `program`：[`Type1FontProgram`] 解析入口与 `/FontFile` 三段长度切分；
//! - `charstring`：CharString 算子解释（宽度和路径）；
//! - `subset`：`/FontFile` 程序子集化；
//! - `tests`：解析入口、长度切分与明文头单元测试。

mod charstring;
mod crypto;
mod error;
mod header;
mod private_dict;
mod program;
mod scanner;
mod subset;

pub use charstring::{Type1CharString, Type1PathSegment, Type1Seac};
pub use header::{Type1BuiltinEncoding, Type1Header};
pub use program::Type1FontProgram;
pub(crate) use program::font_file_lengths;
pub use subset::subset_type1;

#[cfg(test)]
mod tests;
