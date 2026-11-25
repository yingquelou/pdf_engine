//! media clip section 写端（13.2.6 Table 277/278，PDF 1.5）。
//!
//! [`MediaClip`] 是 rendition `/C` 的递归模型，只建模 media clip data
//! （`/S /MCD`，Table 274）与 media clip section（`/S /MCS`，Table 277）。
//! section 目前只写 `/N` 与 `/D`；`/Alt`、`/MH`、`/BE`（Table 278）尚未
//! 建模，读端仍保留原值。
//!
//! `/D` 必须是间接引用，因此 section 链上每个下一级 clip 都由
//! `assemble::rendition` 分配独立对象号；最外层 clip 内联在 rendition
//! `/C` 中，不占用对象号。嵌套深度与读端同一计数：最外层 rendition 深度
//! 为 0、clip 为 1，section 的 `/D` 每级加一，最终 media clip data 也计入
//! 深度；达到 32 层在写出任何对象前拒绝，保证零字节输出契约。

use std::cell::Cell;

use super::super::{AlternateText, text_string_value};
use super::{MAX_DEPTH, MediaClipData};
use crate::core::types::{Dict, Value};
use crate::core::writer::WriteError;

/// media clip 对象（Table 273）。
#[derive(Debug, Clone, PartialEq)]
pub enum MediaClip {
    /// media clip data（Table 274）。
    Data(MediaClipData),
    /// media clip section（Table 277）。
    Section(MediaClipSection),
}

impl From<MediaClipData> for MediaClip {
    fn from(data: MediaClipData) -> Self {
        Self::Data(data)
    }
}

impl From<MediaClipSection> for MediaClip {
    fn from(section: MediaClipSection) -> Self {
        Self::Section(section)
    }
}

impl MediaClip {
    /// 按 clip 深度校验数据来源句柄、section 名称与嵌套深度。
    ///
    /// 深度计数与读端 `parse_media_clip` 一致：本节点的深度达到上限即
    /// 拒绝，section 的下一级 clip 深度加一。
    pub(crate) fn validate_at_depth(
        &self,
        depth: usize,
        form_count: usize,
        embedded_count: usize,
    ) -> Result<(), WriteError> {
        if depth >= MAX_DEPTH {
            return Err(WriteError::InvalidObject(
                "media clip nesting exceeds the depth limit".into(),
            ));
        }
        match self {
            Self::Data(data) => data.validate(form_count, embedded_count),
            Self::Section(section) => section.validate_at_depth(depth, form_count, embedded_count),
        }
    }

    /// `/D` 引用的 Form XObject 句柄（含 section 链），用于登记写出。
    pub(crate) fn form_handle(&self) -> Option<usize> {
        match self {
            Self::Data(data) => data.form_handle(),
            Self::Section(section) => section.next().form_handle(),
        }
    }

    /// 最外层 clip 是否为 section；决定内联 `/C` 是否消费对象号。
    pub(crate) fn is_section(&self) -> bool {
        matches!(self, Self::Section(_))
    }

    /// 写 rendition `/C` 的内联值。
    ///
    /// media clip data 直接写成字典；section 从共享游标取得其 `/D`
    /// 目标对象号，写成 `<< ... /D n 0 R >>`。
    pub(crate) fn inline_value(&self, refs: &ClipRefs<'_>) -> Result<Value, WriteError> {
        match self {
            Self::Data(data) => Ok(Value::Dict(
                data.to_dict(refs.form_nums, refs.embedded_nums)?,
            )),
            Self::Section(section) => {
                let num = refs.next_root()?;
                Ok(Value::Dict(section.to_dict(Value::Ref(num, 0))))
            }
        }
    }
}

/// media clip section 的 `/MH`/`/BE` 字典（Table 278）。
///
/// `/B` 与 `/E` 是 media offset 字典（13.2.6.2），本层按原值搬运，
/// 只校验其为字典；语义（时间/帧/标记）属播放端。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaClipSectionHandler {
    begin: Option<Value>,
    end: Option<Value>,
}

impl MediaClipSectionHandler {
    /// 空处理器字典。
    pub fn new() -> Self {
        Self::default()
    }

    /// `/B` 起始偏移字典（media offset dictionary）。
    pub fn with_begin(mut self, begin: Value) -> Self {
        self.begin = Some(begin);
        self
    }

    /// `/E` 结束偏移字典（media offset dictionary）。
    pub fn with_end(mut self, end: Value) -> Self {
        self.end = Some(end);
        self
    }

    fn validate(&self) -> Result<(), WriteError> {
        for (key, value) in [("/B", &self.begin), ("/E", &self.end)] {
            if let Some(value) = value
                && !matches!(value, Value::Dict(_))
            {
                return Err(WriteError::InvalidObject(format!(
                    "media clip section handler {key} must be a media offset dictionary"
                )));
            }
        }
        Ok(())
    }

    fn to_dict(&self) -> Dict {
        let mut dict = Dict::new();
        if let Some(begin) = &self.begin {
            dict = dict.insert("B", begin.clone());
        }
        if let Some(end) = &self.end {
            dict = dict.insert("E", end.clone());
        }
        dict
    }
}

/// media clip section（Table 277）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaClipSection {
    name: Option<String>,
    alt: Option<AlternateText>,
    must_honour: Option<MediaClipSectionHandler>,
    best_effort: Option<MediaClipSectionHandler>,
    next: Box<MediaClip>,
}

impl MediaClipSection {
    /// 以被切片的下一级 clip 构造；`/D` 必写。
    pub fn new(next: impl Into<MediaClip>) -> Self {
        Self {
            name: None,
            alt: None,
            must_honour: None,
            best_effort: None,
            next: Box::new(next.into()),
        }
    }

    /// `/N` 名称。
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// `/Alt` 替代描述（Table 277）：文本串或多语言文本数组。
    pub fn with_alt(mut self, alt: impl Into<AlternateText>) -> Self {
        self.alt = Some(alt.into());
        self
    }

    /// `/MH` 必须满足的偏移条目（Table 278）。
    pub fn with_must_honour(mut self, handler: MediaClipSectionHandler) -> Self {
        self.must_honour = Some(handler);
        self
    }

    /// `/BE` 尽力满足的偏移条目（Table 278）。
    pub fn with_best_effort(mut self, handler: MediaClipSectionHandler) -> Self {
        self.best_effort = Some(handler);
        self
    }

    /// `/D` 下一级 media clip。
    pub(crate) fn next(&self) -> &MediaClip {
        &self.next
    }

    /// 按 section 深度校验名称与下一级结构；`/D` 目标深度加一。
    fn validate_at_depth(
        &self,
        depth: usize,
        form_count: usize,
        embedded_count: usize,
    ) -> Result<(), WriteError> {
        if self.name.as_deref().is_some_and(str::is_empty) {
            return Err(WriteError::InvalidObject(
                "media clip section /N must not be empty".into(),
            ));
        }
        if let Some(alt) = &self.alt {
            alt.validate()?;
        }
        if let Some(handler) = &self.must_honour {
            handler.validate()?;
        }
        if let Some(handler) = &self.best_effort {
            handler.validate()?;
        }
        self.next
            .validate_at_depth(depth + 1, form_count, embedded_count)
    }

    /// 写出 media clip section 字典；`next_ref` 为 `/D` 的间接引用。
    ///
    /// section 自身不引用 Form XObject 或嵌入文件，这些句柄由下一级
    /// clip 对象负责解析。
    pub(crate) fn to_dict(&self, next_ref: Value) -> Dict {
        let mut dict = Dict::new()
            .insert("Type", Value::Name("MediaClip".into()))
            .insert("S", Value::Name("MCS".into()))
            .insert("D", next_ref);
        if let Some(name) = &self.name {
            dict = dict.insert("N", text_string_value(name));
        }
        if let Some(alt) = &self.alt {
            dict = dict.insert("Alt", alt.to_value());
        }
        if let Some(handler) = &self.must_honour {
            dict = dict.insert("MH", Value::Dict(handler.to_dict()));
        }
        if let Some(handler) = &self.best_effort {
            dict = dict.insert("BE", Value::Dict(handler.to_dict()));
        }
        dict
    }
}

/// 写出 rendition 时解析 media clip 的对象号映射。
///
/// `roots` 是每个内联 section `/C` 的 `/D` 目标对象号（按动作写出顺序
/// 消费），由注解与大纲共享的 `cursor` 推进；全部 section `/D` 目标由
/// `assemble::rendition::write_objects` 按同一分配顺序写出并逐级引用。
/// 内联阶段只消费首级目标，因此游标不会越过 `roots`。
#[derive(Debug, Clone, Copy)]
pub(crate) struct ClipRefs<'a> {
    /// Form XObject 句柄 → 表单流对象号。
    pub(crate) form_nums: &'a [i64],
    /// 嵌入文件句柄 → 文件规格对象号。
    pub(crate) embedded_nums: &'a [i64],
    /// 每个内联 section 的首级 `/D` 目标对象号。
    pub(crate) roots: &'a [i64],
    /// 内联 section 的消费游标；缺失时拒绝写出 section。
    pub(crate) cursor: Option<&'a Cell<usize>>,
}

impl ClipRefs<'_> {
    /// 取得下一个内联 section 的首级 `/D` 对象号并推进游标。
    fn next_root(&self) -> Result<i64, WriteError> {
        let cursor = self.cursor.ok_or_else(|| {
            WriteError::InvalidObject(
                "media clip section /C requires allocated object numbers".into(),
            )
        })?;
        let index = cursor.get();
        let num = self.roots.get(index).copied().ok_or_else(|| {
            WriteError::InvalidObject(format!(
                "media clip section /C has no allocated object number at index {index}"
            ))
        })?;
        cursor.set(index + 1);
        Ok(num)
    }
}
