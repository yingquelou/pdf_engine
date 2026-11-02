//! 文档特性画像与扫描逻辑（reader/writer 共用）。

mod inspect;
mod profile;
mod scan;

pub use profile::FeatureProfile;
pub(crate) use scan::{scan_dict_features, scan_value_features};
