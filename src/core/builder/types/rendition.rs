//! Rendition 与 media clip 对象写端（13.2，PDF 1.5）。
//!
//! 覆盖媒体 rendition（`/S /MR`，Table 271）与选择器 rendition
//! （`/S /SR`，Table 272）。media clip 只建模 media clip data
//! （`/S /MCD`，Table 274）：`/D` 的媒体数据写成嵌入文件规格或
//! Form XObject 的间接引用，也可写成 URL 文件规格；clip section
//! （`/S /MCS`，Table 277）目前仅有读端能力。播放参数 `/P`
//! （Table 279–281）与屏幕参数 `/SP`（Table 282–284）在 `play`/`screen`
//! 子模块建模，两处 `/PL` media players（13.2.7 Table 290–293）在
//! `players` 子模块建模。
//!
//! 嵌入文件与 Form XObject 句柄由 [`MediaClipData::to_dict`] 按对象号
//! 表解析；句柄上界在 `action_requirements` 于任何对象写出前拒绝。

use super::text_string_value;
use crate::core::types::{Array, Dict, Value};
use crate::core::writer::WriteError;

mod play;
mod players;
mod screen;

pub use play::{MediaDuration, MediaFit, MediaPlayParameters, MediaPlaySettings, MediaTimespan};
pub use players::{MediaPlayerInfo, MediaPlayers, SoftwareIdentifier};
pub use screen::{MediaFloatingWindow, MediaScreenParameters, MediaScreenSettings, MediaWindow};

/// media clip data 的 `/D` 媒体数据来源（Table 274）。
#[derive(Debug, Clone, PartialEq)]
pub enum MediaClipSource {
    /// 嵌入文件句柄；写出为该文件规格的间接引用。
    EmbeddedFile(usize),
    /// URL 文件规格（`/FS /URL` + `/F`）。
    Url(String),
    /// Form XObject 句柄；写出为该表单流的间接引用。
    FormXObject(usize),
}

/// `/TF` 临时文件写入许可（Table 275）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemporaryFilePermission {
    /// `TEMPNEVER`（缺省）：不允许写临时文件。
    Never,
    /// `TEMPEXTRACT`：只允许提取媒体数据。
    Extract,
    /// `TEMPACCESS`：允许临时访问媒体数据。
    Access,
    /// `TEMPALWAYS`：不限制临时文件。
    Always,
}

impl TemporaryFilePermission {
    fn name(self) -> &'static str {
        match self {
            Self::Never => "TEMPNEVER",
            Self::Extract => "TEMPEXTRACT",
            Self::Access => "TEMPACCESS",
            Self::Always => "TEMPALWAYS",
        }
    }
}

/// media permissions 字典（Table 275）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaPermissions {
    temporary_file: TemporaryFilePermission,
}

impl MediaPermissions {
    /// 按 `/TF` 许可构造。
    pub fn new(temporary_file: TemporaryFilePermission) -> Self {
        Self { temporary_file }
    }

    /// `/TF` 许可。
    pub fn temporary_file(&self) -> TemporaryFilePermission {
        self.temporary_file
    }

    fn to_dict(&self) -> Dict {
        Dict::new()
            .insert("Type", Value::Name("MediaPermissions".into()))
            .insert("TF", Value::Name(self.temporary_file.name().into()))
    }
}

/// media clip data（Table 274）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaClipData {
    name: Option<String>,
    source: MediaClipSource,
    content_type: Option<String>,
    permissions: Option<MediaPermissions>,
    players: Option<MediaPlayers>,
    base_url: Option<String>,
}

impl MediaClipData {
    /// 以媒体数据来源构造；`/D` 必写。
    pub fn new(source: MediaClipSource) -> Self {
        Self {
            name: None,
            source,
            content_type: None,
            permissions: None,
            players: None,
            base_url: None,
        }
    }

    /// `/N` 名称。
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// `/CT` MIME 内容类型（ASCII）。
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// `/P` media permissions 字典（Table 275）。
    pub fn with_permissions(mut self, permissions: MediaPermissions) -> Self {
        self.permissions = Some(permissions);
        self
    }

    /// `/PL` media players 字典（Table 290）。
    pub fn with_players(mut self, players: MediaPlayers) -> Self {
        self.players = Some(players);
        self
    }

    /// `/MH` 的 `/BU` 基准 URL（Table 276）。
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// `/D` 引用的 Form XObject 句柄（用于登记写出）。
    pub(crate) fn form_handle(&self) -> Option<usize> {
        match &self.source {
            MediaClipSource::FormXObject(handle) => Some(*handle),
            _ => None,
        }
    }

    /// 校验数据来源句柄与 ASCII 条目。
    pub(crate) fn validate(
        &self,
        form_count: usize,
        embedded_count: usize,
    ) -> Result<(), WriteError> {
        match &self.source {
            MediaClipSource::EmbeddedFile(handle) if *handle >= embedded_count => {
                return Err(WriteError::InvalidObject(format!(
                    "media clip data /D references unknown embedded file handle {handle}"
                )));
            }
            MediaClipSource::FormXObject(handle) if *handle >= form_count => {
                return Err(WriteError::InvalidObject(format!(
                    "media clip data /D references unknown form handle {handle}"
                )));
            }
            MediaClipSource::Url(url) if !is_printable_ascii(url) => {
                return Err(WriteError::InvalidObject(
                    "media clip data /D URL must be non-empty printable ASCII".into(),
                ));
            }
            _ => {}
        }
        if let Some(name) = &self.name
            && name.is_empty()
        {
            return Err(WriteError::InvalidObject(
                "media clip data /N must not be empty".into(),
            ));
        }
        if let Some(content_type) = &self.content_type
            && (content_type.is_empty() || !content_type.is_ascii())
        {
            return Err(WriteError::InvalidObject(
                "media clip data /CT must be non-empty ASCII".into(),
            ));
        }
        if let Some(base_url) = &self.base_url
            && !is_printable_ascii(base_url)
        {
            return Err(WriteError::InvalidObject(
                "media clip data /MH /BU must be non-empty printable ASCII".into(),
            ));
        }
        if let Some(players) = &self.players {
            players.validate()?;
        }
        Ok(())
    }

    /// 写出 media clip data 字典；`form_nums`/`embedded_nums` 为句柄到
    /// 对象号的映射（下标即句柄）。
    pub(crate) fn to_dict(
        &self,
        form_nums: &[i64],
        embedded_nums: &[i64],
    ) -> Result<Dict, WriteError> {
        self.validate(form_nums.len(), embedded_nums.len())?;
        let data = match &self.source {
            MediaClipSource::EmbeddedFile(handle) => {
                let num = embedded_nums.get(*handle).copied().filter(|num| *num > 0).ok_or_else(
                    || {
                        WriteError::InvalidObject(format!(
                            "media clip data /D references embedded file handle {handle} with no allocated object number"
                        ))
                    },
                )?;
                Value::Ref(num, 0)
            }
            MediaClipSource::Url(url) => Value::Dict(
                Dict::new()
                    .insert("FS", Value::Name("URL".into()))
                    .insert("F", text_string_value(url)),
            ),
            MediaClipSource::FormXObject(handle) => {
                let num = form_nums.get(*handle).copied().filter(|num| *num > 0).ok_or_else(
                    || {
                        WriteError::InvalidObject(format!(
                            "media clip data /D references form handle {handle} with no allocated object number"
                        ))
                    },
                )?;
                Value::Ref(num, 0)
            }
        };
        let mut dict = Dict::new()
            .insert("Type", Value::Name("MediaClip".into()))
            .insert("S", Value::Name("MCD".into()))
            .insert("D", data);
        if let Some(name) = &self.name {
            dict = dict.insert("N", text_string_value(name));
        }
        if let Some(content_type) = &self.content_type {
            dict = dict.insert("CT", text_string_value(content_type));
        }
        if let Some(permissions) = &self.permissions {
            dict = dict.insert("P", Value::Dict(permissions.to_dict()));
        }
        if let Some(players) = &self.players {
            dict = dict.insert("PL", Value::Dict(players.to_dict()));
        }
        if let Some(base_url) = &self.base_url {
            let handler = Dict::new().insert("BU", text_string_value(base_url));
            dict = dict.insert("MH", Value::Dict(handler));
        }
        Ok(dict)
    }
}

/// 媒体 rendition（`/S /MR`，Table 271）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaRendition {
    name: Option<String>,
    clip: MediaClipData,
    play: Option<MediaPlayParameters>,
    screen: Option<MediaScreenParameters>,
}

impl MediaRendition {
    /// 以 media clip 对象构造；`/C` 必写。
    pub fn new(clip: MediaClipData) -> Self {
        Self {
            name: None,
            clip,
            play: None,
            screen: None,
        }
    }

    /// `/N` 名称。
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// `/P` media play parameters（Table 279）。
    pub fn with_play(mut self, play: MediaPlayParameters) -> Self {
        self.play = Some(play);
        self
    }

    /// `/SP` media screen parameters（Table 282）。
    pub fn with_screen(mut self, screen: MediaScreenParameters) -> Self {
        self.screen = Some(screen);
        self
    }

    pub(crate) fn validate(
        &self,
        form_count: usize,
        embedded_count: usize,
    ) -> Result<(), WriteError> {
        if self.name.as_deref().is_some_and(str::is_empty) {
            return Err(WriteError::InvalidObject(
                "rendition /N must not be empty".into(),
            ));
        }
        self.clip.validate(form_count, embedded_count)?;
        if let Some(play) = &self.play {
            play.validate()?;
        }
        if let Some(screen) = &self.screen {
            screen.validate()?;
        }
        Ok(())
    }

    pub(crate) fn form_handle(&self) -> Option<usize> {
        self.clip.form_handle()
    }

    pub(crate) fn to_dict(
        &self,
        form_nums: &[i64],
        embedded_nums: &[i64],
    ) -> Result<Dict, WriteError> {
        let mut dict = Dict::new()
            .insert("Type", Value::Name("Rendition".into()))
            .insert("S", Value::Name("MR".into()))
            .insert(
                "C",
                Value::Dict(self.clip.to_dict(form_nums, embedded_nums)?),
            );
        if let Some(name) = &self.name {
            dict = dict.insert("N", text_string_value(name));
        }
        if let Some(play) = &self.play {
            dict = dict.insert("P", Value::Dict(play.to_dict()));
        }
        if let Some(screen) = &self.screen {
            dict = dict.insert("SP", Value::Dict(screen.to_dict()));
        }
        Ok(dict)
    }
}

/// 选择器 rendition（`/S /SR`，Table 272）。
#[derive(Debug, Clone, PartialEq)]
pub struct SelectorRendition {
    name: Option<String>,
    renditions: Vec<Rendition>,
}

impl SelectorRendition {
    /// 以候选 rendition 列表构造；`/R` 必写（允许为空）。
    pub fn new(renditions: Vec<Rendition>) -> Self {
        Self {
            name: None,
            renditions,
        }
    }

    /// `/N` 名称。
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub(crate) fn validate(
        &self,
        form_count: usize,
        embedded_count: usize,
    ) -> Result<(), WriteError> {
        if self.name.as_deref().is_some_and(str::is_empty) {
            return Err(WriteError::InvalidObject(
                "rendition /N must not be empty".into(),
            ));
        }
        for rendition in &self.renditions {
            rendition.validate(form_count, embedded_count)?;
        }
        Ok(())
    }

    pub(crate) fn collect_form_handles(&self, handles: &mut Vec<usize>) {
        for rendition in &self.renditions {
            rendition.collect_form_handles(handles);
        }
    }

    pub(crate) fn to_dict(
        &self,
        form_nums: &[i64],
        embedded_nums: &[i64],
    ) -> Result<Dict, WriteError> {
        let mut entries = Vec::with_capacity(self.renditions.len());
        for rendition in &self.renditions {
            entries.push(Value::Dict(rendition.to_dict(form_nums, embedded_nums)?));
        }
        let mut dict = Dict::new()
            .insert("Type", Value::Name("Rendition".into()))
            .insert("S", Value::Name("SR".into()))
            .insert("R", Value::Array(Array(entries)));
        if let Some(name) = &self.name {
            dict = dict.insert("N", text_string_value(name));
        }
        Ok(dict)
    }
}

/// rendition 对象（Table 266）。
///
/// 媒体 rendition 装箱，避免播放/屏幕参数放大 rendition 与动作枚举的尺寸。
#[derive(Debug, Clone, PartialEq)]
pub enum Rendition {
    /// 媒体 rendition（Table 271）。
    Media(Box<MediaRendition>),
    /// 选择器 rendition（Table 272）。
    Selector(SelectorRendition),
}

impl Rendition {
    /// 以 media clip 构造媒体 rendition。
    pub fn media(clip: MediaClipData) -> Self {
        Self::Media(Box::new(MediaRendition::new(clip)))
    }

    /// 以候选列表构造选择器 rendition。
    pub fn selector(renditions: Vec<Rendition>) -> Self {
        Self::Selector(SelectorRendition::new(renditions))
    }

    /// `/N` 名称。
    pub fn with_name(self, name: impl Into<String>) -> Self {
        let name = name.into();
        match self {
            Self::Media(media) => Self::Media(Box::new(media.with_name(name))),
            Self::Selector(selector) => Self::Selector(selector.with_name(name)),
        }
    }

    /// 校验嵌套结构与数据来源句柄。
    pub(crate) fn validate(
        &self,
        form_count: usize,
        embedded_count: usize,
    ) -> Result<(), WriteError> {
        match self {
            Self::Media(media) => media.validate(form_count, embedded_count),
            Self::Selector(selector) => selector.validate(form_count, embedded_count),
        }
    }

    /// 收集 `/D` 引用的 Form XObject 句柄，供写出前分配对象号。
    pub(crate) fn collect_form_handles(&self, handles: &mut Vec<usize>) {
        match self {
            Self::Media(media) => {
                if let Some(handle) = media.form_handle() {
                    handles.push(handle);
                }
            }
            Self::Selector(selector) => selector.collect_form_handles(handles),
        }
    }

    /// 写出 rendition 字典。
    pub(crate) fn to_dict(
        &self,
        form_nums: &[i64],
        embedded_nums: &[i64],
    ) -> Result<Dict, WriteError> {
        match self {
            Self::Media(media) => media.to_dict(form_nums, embedded_nums),
            Self::Selector(selector) => selector.to_dict(form_nums, embedded_nums),
        }
    }
}

fn is_printable_ascii(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| (0x21..=0x7E).contains(&byte))
}
