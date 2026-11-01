//! 查看器首选项写端（ISO 32000-1 12.2，Table 150）。
//!
//! [`PdfBuilder::set_viewer_preferences`] 接收读端共用模型
//! [`ViewerPreferences`]（类型化条目与逐条目版本溯源见
//! [`crate::core::reader::navigation::ViewerPreferences`] 的文档），
//! assemble 时把**已声明（非缺省）条目**物化为目录 `/ViewerPreferences`
//! 直接字典：布尔缺省 `false`、`/NonFullScreenPageMode` 缺省 `UseNone`、
//! `/Direction` 缺省 `L2R`、四个页面边界条目缺省 `CropBox`、
//! `/PrintScaling` 缺省 `AppDefault` 一律省略，与规范缺省等价。
//!
//! 构建期校验落在 `assemble_preflight` 里、任何对象号分配与写出之前，
//! 失败保持零字节契约：
//!
//! - `/PrintPageRange` 页码须 ≥1（规范：首页 denoted by 1）且每对
//!   start ≤ end——**后者是本库策略而非规范 shall**，规范未禁止倒序子区间，
//!   读端对倒序保持宽松；
//! - `/NumCopies` 须 ≥1——规范原文「Values outside this range shall be
//!   ignored」未给出可执行的边界，正整数是本库策略；
//! - `/Enforce` 名数组的非空名字校验；
//! - 逐档位版本门禁按 [`ViewerPreferences::declared_features`] 实际用到的
//!   [`PdfFeature`] 判定：`/Direction` 需 PDF 1.3、`/DisplayDocTitle` 与
//!   `/ViewArea` 等需 1.4、`/PrintScaling` 需 1.6、`/Duplex` 等打印对话框
//!   条目需 1.7；`/Enforce` 系 PDF 2.0 扩展（32000-1 Table 150 无此条目），
//!   而本 writer 输出区间为 1.0–1.7，故含 `/Enforce` 的写出在所有目标
//!   版本下均返回 [`WriteError::UnsupportedVersion`]。
//!   版本不足返回 [`WriteError::UnsupportedVersion`]，取值非法返回
//!   [`WriteError::InvalidObject`]，两者均零字节输出。

use crate::core::builder::PdfBuilder;
use crate::core::types::{Array, Dict, PdfFeature, PdfVersion, Value};
use crate::core::writer::WriteError;

// 读端共用模型经写端模块再导出，便于 `pdf_engine::core::builder::*` 取用。
pub use crate::core::reader::navigation::{
    Duplex, NonFullScreenPageMode, PageBoundaryName, PrintScaling, ReadingDirection,
    ViewerPreferences,
};

/// 写出 `/ViewerPreferences` 字典；只物化已声明（非缺省）条目。
pub(crate) fn to_dict(prefs: &ViewerPreferences) -> Dict {
    let default = ViewerPreferences::default();
    let mut dict = Dict::new();
    if prefs.hide_toolbar {
        dict = dict.insert("HideToolbar", Value::Bool(true));
    }
    if prefs.hide_menubar {
        dict = dict.insert("HideMenubar", Value::Bool(true));
    }
    if prefs.hide_window_ui {
        dict = dict.insert("HideWindowUI", Value::Bool(true));
    }
    if prefs.fit_window {
        dict = dict.insert("FitWindow", Value::Bool(true));
    }
    if prefs.center_window {
        dict = dict.insert("CenterWindow", Value::Bool(true));
    }
    if prefs.non_full_screen_page_mode != default.non_full_screen_page_mode {
        dict = dict.insert(
            "NonFullScreenPageMode",
            Value::Name(prefs.non_full_screen_page_mode.pdf_name().into()),
        );
    }
    if prefs.direction != default.direction {
        dict = dict.insert("Direction", Value::Name(prefs.direction.pdf_name().into()));
    }
    if prefs.display_doc_title {
        dict = dict.insert("DisplayDocTitle", Value::Bool(true));
    }
    for (key, value) in [
        ("ViewArea", prefs.view_area),
        ("ViewClip", prefs.view_clip),
        ("PrintArea", prefs.print_area),
        ("PrintClip", prefs.print_clip),
    ] {
        if value != PageBoundaryName::CropBox {
            dict = dict.insert(key, Value::Name(value.pdf_name().into()));
        }
    }
    if prefs.print_scaling != PrintScaling::AppDefault {
        dict = dict.insert(
            "PrintScaling",
            Value::Name(prefs.print_scaling.pdf_name().into()),
        );
    }
    if let Some(duplex) = prefs.duplex {
        dict = dict.insert("Duplex", Value::Name(duplex.pdf_name().into()));
    }
    if let Some(value) = prefs.pick_tray_by_pdf_size {
        dict = dict.insert("PickTrayByPDFSize", Value::Bool(value));
    }
    if !prefs.print_page_range.is_empty() {
        let numbers = prefs
            .print_page_range
            .iter()
            .flat_map(|(first, last)| [*first, *last])
            .map(Value::Integer)
            .collect();
        dict = dict.insert("PrintPageRange", Value::Array(Array(numbers)));
    }
    if let Some(copies) = prefs.num_copies {
        dict = dict.insert("NumCopies", Value::Integer(copies));
    }
    if !prefs.enforce.is_empty() {
        let names = prefs
            .enforce
            .iter()
            .map(|name| Value::Name(name.clone()))
            .collect();
        dict = dict.insert("Enforce", Value::Array(Array(names)));
    }
    // 空字典保留：表达「声明了首选项但全部条目为规范缺省」。
    dict
}

/// 取值契约与逐档位版本门禁；返回要登记进特征画像的 [`PdfFeature`] 集合。
///
/// 全部违规发生在任何对象号分配与写出之前（零字节契约）。
pub(crate) fn validate(
    prefs: &ViewerPreferences,
    version: PdfVersion,
) -> Result<Vec<PdfFeature>, WriteError> {
    for (index, (first, last)) in prefs.print_page_range.iter().enumerate() {
        if *first < 1 || *last < 1 {
            return Err(invalid(format!(
                "viewer preferences: /PrintPageRange sub-range {index} starts at {first}/{last}; page numbers are 1-based (12.2 Table 150)"
            )));
        }
        if first > last {
            return Err(invalid(format!(
                "viewer preferences: /PrintPageRange sub-range {index} has first page {first} after last page {last}; rejecting reversed ranges is library policy, not a 12.2 shall"
            )));
        }
    }
    if let Some(copies) = prefs.num_copies
        && copies < 1
    {
        return Err(invalid(format!(
            "viewer preferences: /NumCopies must be at least 1, got {copies}; the positive bound is library policy (12.2 leaves the range to the reader)"
        )));
    }
    if prefs.enforce.iter().any(String::is_empty) {
        return Err(invalid(
            "viewer preferences: /Enforce names must not be empty",
        ));
    }
    let features = prefs.declared_features();
    for feature in &features {
        if !version.supports(*feature) {
            return Err(WriteError::UnsupportedVersion(format!(
                "{} require PDF {}, got {}",
                feature,
                feature.min_version().as_str(),
                version.as_str()
            )));
        }
    }
    Ok(features)
}

impl PdfBuilder {
    /// 设置查看器首选项（ISO 32000-1 12.2，Table 150，PDF 1.2 起），
    /// assemble 时写为目录 `/ViewerPreferences` 字典；仅声明过的（非缺省）
    /// 条目被物化。
    ///
    /// 构建期在任何对象号分配之前执行取值契约与逐档位版本门禁：非法取值
    /// 返回 [`WriteError::InvalidObject`]、目标版本低于所声明条目的引入
    /// 版本返回 [`WriteError::UnsupportedVersion`]，两者均零字节输出。
    /// `/Enforce` 系 PDF 2.0 扩展条目，在本 writer 支持的 1.0–1.7 输出
    /// 区间下均被版本门禁拒绝。
    pub fn set_viewer_preferences(&mut self, preferences: ViewerPreferences) -> &mut Self {
        self.viewer_preferences = Some(preferences);
        self
    }
}

fn invalid(message: impl Into<String>) -> WriteError {
    WriteError::InvalidObject(message.into())
}
