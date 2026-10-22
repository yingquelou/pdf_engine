//! PDF 对象模型与文件结构类型。
//!
//! 目录模块：
//! - `version`：`PdfVersion` / `PdfFeature` 能力模型；
//! - `features`：`FeatureProfile` 与字典/值特性扫描；
//! - `value`：`Value` / `Dict` / `Array` / 流与 xref/trailer 结构；
//! - `document`：懒加载 `Document`。
//!
//! 类型保持纯数据，不带 I/O 生命周期，供 reader/writer 共享。

mod date;
mod document;
mod features;
#[cfg(test)]
mod tests;
mod value;
mod version;

pub use date::{PdfDate, PdfDateError, PdfDateTimezone};
pub use document::Document;
pub use features::FeatureProfile;
pub use value::{
    Array, Dict, Object, StartXref, StreamData, SubXref, Trailer, Value, Xref, XrefEntry, XrefKind,
};
pub use version::{PdfFeature, PdfVersion};

pub(crate) use document::ObjectSecurity;
pub(crate) use features::{scan_dict_features, scan_value_features};
