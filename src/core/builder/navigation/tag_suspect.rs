//! Tag suspects（ISO 32000-1 14.8.2.3.1，PDF 1.6 引入）的写端。
//!
//! 规范原文：
//!
//! > In some situations, conforming writer may be unable to determine correct
//! > page content order for part of a document's contents. In such cases, tag
//! > suspects (PDF 1.6) can be used. The conforming writer shall identify
//! > suspect content by using marked content (see 14.6, "Marked Content") with
//! > a tag of TagSuspect, as shown in next Example. The marked content shall
//! > have a properties dictionary with an entry whose name is TagSuspect and
//! > whose value is Ordering, which indicates that the ordering of the enclosed
//! > marked content does not meet Tagged PDF specifications.
//! > EXAMPLE  /TagSuspect <</TagSuspect /Ordering>> BDC .... % Problem page contents EMC
//! > Documents containing tag suspects shall contain a Suspects entry with a
//! > value of true in the mark information dictionary (see Table 321).
//!
//! [`PdfBuilder::begin_tag_suspect`] 写出的正是规范 EXAMPLE 的那一形态：属性表
//! 由本模块生成，调用方无从拼错，因此读端归属诊断的两条 tag suspect 规则
//! （`TagSuspectMissingProperty` 与 `TagSuspectSuspectsFlagNotSet`）不会在
//! builder 产物上报错。
//!
//! 构建期校验落在 `assemble_document` 里、任何对象号分配与写出之前，失败保持
//! 零字节契约：
//!
//! - 标签精确为 `TagSuspect` 的标记内容必须带 `/TagSuspect /Ordering` 内联
//!   属性表；`BMC`/`MP` 这类不带属性表的写法直接拒绝——规范只给出带属性表的
//!   一种写法。
//! - `BDC`/`DP` 以名字引用页 `/Properties` 资源时**不**拒绝：属性表在页面
//!   资源字典里，内容流扫描看不到其内容，只能按「本页含 tag suspects」处理，
//!   实际字典交由读端解析校验。
//! - 一旦出现 tag suspects，目录 `/MarkInfo` 必须有 `/Suspects true`
//!   （[`PdfBuilder::set_mark_info`]），且目标版本不低于 PDF 1.6。

use crate::core::builder::types::StructureMarkInfo;
use crate::core::builder::{PageBuild, PdfBuilder};
use crate::core::content::{MarkedContentProperties, Op};
use crate::core::types::{Dict, PdfFeature, PdfVersion, Value};
use crate::core::writer::WriteError;

/// 14.8.2.3.1 规定的 suspect 内容标签；只有精确匹配才算 tag suspect。
const TAG: &str = "TagSuspect";

/// 扫描全部页面内容流的 tag suspects，并做文档级门禁。
///
/// 返回文档是否含 tag suspects，调用方据此把 [`PdfFeature::TagSuspects`]
/// 写入特征画像供版本推导。属性表形态逐次校验：无属性表、缺 `/TagSuspect`
/// 条目、条目值非名字 `Ordering` 均返回 [`WriteError::InvalidObject`]；含
/// tag suspects 时要求 `/MarkInfo /Suspects true` 与 PDF 1.6，分别返回
/// [`WriteError::InvalidObject`] 与 [`WriteError::UnsupportedVersion`]。
/// 全部失败都发生在写出任何字节之前。
pub(in crate::core::builder) fn validate(
    pages: &[PageBuild],
    mark_info: Option<&StructureMarkInfo>,
    version: PdfVersion,
) -> Result<bool, WriteError> {
    let mut first_page = None;
    for (page, page_build) in pages.iter().enumerate() {
        for op in page_build.content.ops() {
            let properties = match op {
                Op::BeginMarkedContent { tag } | Op::MarkedContentPoint { tag } if tag == TAG => {
                    return Err(invalid(format!(
                        "page {page} TagSuspect marked content has no property list; 14.8.2.3.1 only allows /TagSuspect << /TagSuspect /Ordering >> BDC"
                    )));
                }
                Op::BeginMarkedContentProperties { tag, properties }
                | Op::MarkedContentPointProperties { tag, properties }
                    if tag == TAG =>
                {
                    properties
                }
                _ => continue,
            };
            validate_properties(page, properties)?;
            first_page.get_or_insert(page);
        }
    }
    let Some(page) = first_page else {
        return Ok(false);
    };
    if !version.supports(PdfFeature::TagSuspects) {
        return Err(WriteError::UnsupportedVersion(format!(
            "{} require PDF {}, got {}",
            PdfFeature::TagSuspects,
            PdfFeature::TagSuspects.min_version().as_str(),
            version.as_str()
        )));
    }
    if !mark_info.is_some_and(|info| info.suspects) {
        return Err(invalid(format!(
            "page {page} marks tag suspects but the catalog /MarkInfo does not set /Suspects true (14.8.2.3.1, Table 321)"
        )));
    }
    Ok(true)
}

/// 一次 `TagSuspect` 标记的属性表是否为规范要求的 `/TagSuspect /Ordering`。
fn validate_properties(
    page: usize,
    properties: &MarkedContentProperties,
) -> Result<(), WriteError> {
    // 引用 `/Properties` 资源名的形态：属性表内容不在内容流里，不拒绝。
    let MarkedContentProperties::Dict(dict) = properties else {
        return Ok(());
    };
    match dict.get("TagSuspect") {
        Some(Value::Name(name)) if name == "Ordering" => Ok(()),
        Some(value) => Err(invalid(format!(
            "page {page} TagSuspect property list must hold the name Ordering, got {}",
            value.type_name()
        ))),
        None => Err(invalid(format!(
            "page {page} TagSuspect property list has no /TagSuspect entry; it must be /TagSuspect /Ordering (14.8.2.3.1)"
        ))),
    }
}

impl PdfBuilder {
    /// 在指定页开始一段 tag suspect 标记内容：
    /// `/TagSuspect << /TagSuspect /Ordering >> BDC`
    /// （ISO 32000-1 14.8.2.3.1，PDF 1.6 引入）。
    ///
    /// 页序无法判定为合规范序时用它包住那段内容：属性表由本方法写成规范
    /// EXAMPLE 的形态，调用方无从拼错。读端把标签为 `TagSuspect` 的区段连同
    /// 整个子树从「真实内容未入逻辑结构」的 should 判定中豁免——规范承认这段
    /// 内容的页序本就不合 Tagged PDF；结构树也不需要为它挂元素。
    ///
    /// 文档因此必须声明 `/MarkInfo << /Suspects true >>`
    /// （[`PdfBuilder::set_mark_info`]）且目标版本不低于 1.6，否则构建分别返回
    /// [`WriteError::InvalidObject`] 与 [`WriteError::UnsupportedVersion`]，
    /// 两者均零字节输出。未知页索引被忽略。必须以
    /// [`PdfBuilder::end_tag_suspect`] 结束。
    pub fn begin_tag_suspect(&mut self, page: usize) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.content.begin_marked_content_properties(
                TAG,
                MarkedContentProperties::Dict(
                    Dict::new().insert("TagSuspect", Value::Name("Ordering".into())),
                ),
            );
        }
        self
    }

    /// 结束当前 tag suspect 标记内容段（`EMC`）。未知页索引被忽略。
    pub fn end_tag_suspect(&mut self, page: usize) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.content.end_marked_content();
        }
        self
    }
}

fn invalid(message: impl Into<String>) -> WriteError {
    WriteError::InvalidObject(format!("tag suspect: {}", message.into()))
}
