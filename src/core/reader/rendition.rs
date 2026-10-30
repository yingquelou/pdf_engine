//! 多媒体 rendition 与 media clip 对象读取（13.2，PDF 1.5）。
//!
//! rendition 对象（Table 266–272）分媒体 rendition（`/S /MR`）与选择器
//! rendition（`/S /SR`）；媒体 rendition 的 `/C` 指向 media clip 对象
//! （Table 273–278），后者分 media clip data（`/S /MCD`）与 media clip
//! section（`/S /MCS`）。播放参数 `/P`（Table 279–281）与屏幕参数 `/SP`
//! （Table 282）当前保留原始值，不在此模块类型化。
//!
//! 递归结构（选择器嵌套、clip section 链）限深 32 并做间接引用环检测，
//! 避免恶意输入造成无限递归。

use std::fmt;

use crate::core::reader::content::text_value;
use crate::core::reader::navigation::{NavigationError, resolve};
use crate::core::types::{Dict, Document, Value};

/// 递归嵌套上限。
const MAX_DEPTH: usize = 32;

/// rendition 对象（Table 266）。
#[derive(Debug, Clone, PartialEq)]
pub enum Rendition {
    /// 媒体 rendition（Table 271）。
    Media(Box<MediaRendition>),
    /// 选择器 rendition（Table 272）。
    Selector(SelectorRendition),
    /// 未识别的 `/S` 名称；其余条目不解释。
    Other {
        /// `/S` 子类型名。
        subtype: String,
        /// `/N` 名称。
        name: Option<String>,
    },
}

/// 媒体 rendition（Table 271）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaRendition {
    /// `/N` 名称。
    pub name: Option<String>,
    /// `/C` media clip 对象；播放参数带 `/PL` 时可缺省。
    pub clip: Option<MediaClip>,
    /// `/P` media play parameters 字典（Table 279，保留原值）。
    pub play: Option<Value>,
    /// `/SP` media screen parameters 字典（Table 282，保留原值）。
    pub screen: Option<Value>,
    /// `/MH` 可行性条件字典（Table 267，保留原值）。
    pub mh: Option<Value>,
    /// `/BE` 最佳努力条件字典（Table 267，保留原值）。
    pub be: Option<Value>,
}

/// 选择器 rendition（Table 272）。
#[derive(Debug, Clone, PartialEq)]
pub struct SelectorRendition {
    /// `/N` 名称。
    pub name: Option<String>,
    /// `/R` 候选 rendition 数组（允许为空）。
    pub renditions: Vec<Rendition>,
    /// `/MH` 可行性条件字典（Table 267，保留原值）。
    pub mh: Option<Value>,
    /// `/BE` 最佳努力条件字典（Table 267，保留原值）。
    pub be: Option<Value>,
}

/// media clip 对象（Table 273）。
#[derive(Debug, Clone, PartialEq)]
pub enum MediaClip {
    /// 媒体剪辑数据（Table 274）。
    Data(MediaClipData),
    /// 媒体剪辑片段（Table 277）。
    Section(MediaClipSection),
    /// 未识别的 `/S` 名称。
    Other {
        /// `/S` 子类型名。
        subtype: String,
        /// `/N` 名称。
        name: Option<String>,
    },
}

/// media clip data（Table 274）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaClipData {
    /// `/N` 名称。
    pub name: Option<String>,
    /// `/D` 媒体数据来源：文件规格或流（form XObject）。
    pub data: MediaClipDataSource,
    /// `/CT` MIME 内容类型。
    pub content_type: Option<String>,
    /// `/P` media permissions 字典（Table 275）。
    pub permissions: Option<MediaPermissions>,
    /// `/Alt` 备用文本说明（14.9.2.4，保留原值）。
    pub alt: Option<Value>,
    /// `/PL` media players 字典（13.2.7.2，保留原值）。
    pub players: Option<Value>,
    /// `/MH` 必须满足的处理器条目（Table 276）。
    pub mh: Option<MediaClipHandler>,
    /// `/BE` 尽力满足的处理器条目（Table 276）。
    pub be: Option<MediaClipHandler>,
}

/// media clip data 的 `/D` 取值形态。
#[derive(Debug, Clone, PartialEq)]
pub enum MediaClipDataSource {
    /// 完整文件规格（7.11.3）。
    FileSpec(Box<crate::core::reader::embedded::Filespec>),
    /// 流（form XObject）。
    Stream(Value),
}

/// media clip data 的 MH/BE 字典（Table 276）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaClipHandler {
    /// `/BU` 基准 URL。
    pub base_url: Option<String>,
}

/// media clip section（Table 277）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaClipSection {
    /// `/N` 名称。
    pub name: Option<String>,
    /// `/D` 下一级 media clip 对象。
    pub next: Box<MediaClip>,
    /// `/Alt` 备用文本说明（保留原值）。
    pub alt: Option<Value>,
    /// `/MH` 必须满足的片段条目（Table 278）。
    pub mh: Option<MediaClipSectionHandler>,
    /// `/BE` 尽力满足的片段条目（Table 278）。
    pub be: Option<MediaClipSectionHandler>,
}

/// media clip section 的 MH/BE 字典（Table 278）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaClipSectionHandler {
    /// `/B` 起始偏移字典（13.2.6.2，保留原值）。
    pub begin: Option<Value>,
    /// `/E` 结束偏移字典（13.2.6.2，保留原值）。
    pub end: Option<Value>,
}

/// media permissions 字典（Table 275）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaPermissions {
    /// `/TF` 临时文件写入许可。
    pub temporary_file: TemporaryFilePermission,
}

/// `/TF` 临时文件许可（Table 275）。
#[derive(Debug, Clone, PartialEq)]
pub enum TemporaryFilePermission {
    /// `TEMPNEVER`（缺省）。
    Never,
    /// `TEMPEXTRACT`。
    Extract,
    /// `TEMPACCESS`。
    Access,
    /// `TEMPALWAYS`。
    Always,
    /// 未识别的名称（规范视为 `TEMPNEVER`，此处保留原值）。
    Other(String),
}

impl TemporaryFilePermission {
    /// 按 Table 275 映射名称；缺省与未识别名称的区别由 `Option` 表达。
    fn parse(name: &str) -> Self {
        match name {
            "TEMPNEVER" => Self::Never,
            "TEMPEXTRACT" => Self::Extract,
            "TEMPACCESS" => Self::Access,
            "TEMPALWAYS" => Self::Always,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// rendition / media clip 解析错误。
#[derive(Debug)]
pub enum RenditionError {
    /// 间接引用无法解析。
    Navigation(NavigationError),
    /// 条目缺失、类型或取值非法。
    Malformed(String),
}

impl fmt::Display for RenditionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Navigation(error) => write!(f, "{error}"),
            Self::Malformed(message) => write!(f, "invalid rendition object: {message}"),
        }
    }
}

impl std::error::Error for RenditionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Navigation(error) => Some(error),
            Self::Malformed(_) => None,
        }
    }
}

impl From<NavigationError> for RenditionError {
    fn from(error: NavigationError) -> Self {
        Self::Navigation(error)
    }
}

fn malformed(message: impl Into<String>) -> RenditionError {
    RenditionError::Malformed(message.into())
}

fn reference_of(value: &Value) -> Option<(i64, i64)> {
    match value {
        Value::Ref(number, generation) => Some((*number, *generation)),
        _ => None,
    }
}

fn optional_text(dict: &Dict, key: &str) -> Result<Option<String>, RenditionError> {
    match dict.get(key) {
        None => Ok(None),
        Some(value) => text_value(value)
            .map(Some)
            .ok_or_else(|| malformed(format!("/{key} is not a text string"))),
    }
}

impl Document {
    /// 解析 rendition 对象（Table 266）；`value` 可为间接引用。
    pub fn rendition(&self, value: &Value) -> Result<Rendition, RenditionError> {
        parse_rendition(self, value, 0, &mut Vec::new())
    }

    /// 解析 media clip 对象（Table 273）；`value` 可为间接引用。
    pub fn media_clip(&self, value: &Value) -> Result<MediaClip, RenditionError> {
        parse_media_clip(self, value, 0, &mut Vec::new())
    }
}

fn parse_rendition(
    document: &Document,
    value: &Value,
    depth: usize,
    path: &mut Vec<(i64, i64)>,
) -> Result<Rendition, RenditionError> {
    if depth >= MAX_DEPTH {
        return Err(malformed("rendition nesting exceeds the depth limit"));
    }
    let key = reference_of(value);
    if let Some(key) = key {
        if path.contains(&key) {
            return Err(malformed(format!(
                "rendition reference {} {} forms a cycle",
                key.0, key.1
            )));
        }
        path.push(key);
    }
    let result = parse_rendition_inner(document, value, depth, path);
    if key.is_some() {
        path.pop();
    }
    result
}

fn parse_rendition_inner(
    document: &Document,
    value: &Value,
    depth: usize,
    path: &mut Vec<(i64, i64)>,
) -> Result<Rendition, RenditionError> {
    let resolved = resolve(document, value)?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| malformed("rendition is not a dictionary"))?;
    check_type(dict, "Rendition", "rendition")?;
    let name = optional_text(dict, "N")?;
    let subtype = dict
        .get("S")
        .and_then(Value::as_name)
        .ok_or_else(|| malformed("rendition is missing a name /S"))?;
    match subtype {
        "MR" => {
            let clip = match dict.get("C") {
                None => None,
                Some(value) => Some(parse_media_clip(document, value, depth + 1, path)?),
            };
            Ok(Rendition::Media(Box::new(MediaRendition {
                name,
                clip,
                play: dict.get("P").cloned(),
                screen: dict.get("SP").cloned(),
                mh: dict.get("MH").cloned(),
                be: dict.get("BE").cloned(),
            })))
        }
        "SR" => {
            let entries = dict
                .get("R")
                .ok_or_else(|| malformed("selector rendition is missing /R"))?;
            let entries = resolve(document, entries)?;
            let entries = entries
                .as_array()
                .ok_or_else(|| malformed("selector rendition /R is not an array"))?;
            let mut renditions = Vec::with_capacity(entries.0.len());
            for entry in &entries.0 {
                renditions.push(parse_rendition(document, entry, depth + 1, path)?);
            }
            Ok(Rendition::Selector(SelectorRendition {
                name,
                renditions,
                mh: dict.get("MH").cloned(),
                be: dict.get("BE").cloned(),
            }))
        }
        other => Ok(Rendition::Other {
            subtype: other.to_owned(),
            name,
        }),
    }
}

fn parse_media_clip(
    document: &Document,
    value: &Value,
    depth: usize,
    path: &mut Vec<(i64, i64)>,
) -> Result<MediaClip, RenditionError> {
    if depth >= MAX_DEPTH {
        return Err(malformed("media clip nesting exceeds the depth limit"));
    }
    let key = reference_of(value);
    if let Some(key) = key {
        if path.contains(&key) {
            return Err(malformed(format!(
                "media clip reference {} {} forms a cycle",
                key.0, key.1
            )));
        }
        path.push(key);
    }
    let result = parse_media_clip_inner(document, value, depth, path);
    if key.is_some() {
        path.pop();
    }
    result
}

fn parse_media_clip_inner(
    document: &Document,
    value: &Value,
    depth: usize,
    path: &mut Vec<(i64, i64)>,
) -> Result<MediaClip, RenditionError> {
    let resolved = resolve(document, value)?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| malformed("media clip is not a dictionary"))?;
    check_type(dict, "MediaClip", "media clip")?;
    let name = optional_text(dict, "N")?;
    let subtype = dict
        .get("S")
        .and_then(Value::as_name)
        .ok_or_else(|| malformed("media clip is missing a name /S"))?;
    match subtype {
        "MCD" => {
            let data = dict
                .get("D")
                .ok_or_else(|| malformed("media clip data is missing /D"))?;
            let data = parse_media_data(document, data)?;
            let content_type = match dict.get("CT") {
                None => None,
                Some(value) => {
                    let text = text_value(value)
                        .ok_or_else(|| malformed("media clip data /CT is not a text string"))?;
                    if !text.is_ascii() {
                        return Err(malformed("media clip data /CT is not ASCII"));
                    }
                    Some(text)
                }
            };
            let permissions = match dict.get("P") {
                None => None,
                Some(value) => Some(parse_media_permissions(document, value)?),
            };
            let mh = match dict.get("MH") {
                None => None,
                Some(value) => Some(parse_clip_handler(document, value, "MH")?),
            };
            let be = match dict.get("BE") {
                None => None,
                Some(value) => Some(parse_clip_handler(document, value, "BE")?),
            };
            Ok(MediaClip::Data(MediaClipData {
                name,
                data,
                content_type,
                permissions,
                alt: dict.get("Alt").cloned(),
                players: dict.get("PL").cloned(),
                mh,
                be,
            }))
        }
        "MCS" => {
            let next = dict
                .get("D")
                .ok_or_else(|| malformed("media clip section is missing /D"))?;
            let next = parse_media_clip(document, next, depth + 1, path)?;
            let mh = match dict.get("MH") {
                None => None,
                Some(value) => Some(parse_section_handler(document, value, "MH")?),
            };
            let be = match dict.get("BE") {
                None => None,
                Some(value) => Some(parse_section_handler(document, value, "BE")?),
            };
            Ok(MediaClip::Section(MediaClipSection {
                name,
                next: Box::new(next),
                alt: dict.get("Alt").cloned(),
                mh,
                be,
            }))
        }
        other => Ok(MediaClip::Other {
            subtype: other.to_owned(),
            name,
        }),
    }
}

fn parse_media_data(
    document: &Document,
    value: &Value,
) -> Result<MediaClipDataSource, RenditionError> {
    let resolved = resolve(document, value)?;
    match &resolved {
        Value::Stream(..) => Ok(MediaClipDataSource::Stream(resolved)),
        Value::Dict(dict) => {
            let is_filespec = dict.get("Type").and_then(Value::as_name) == Some("Filespec")
                || dict.get("FS").is_some();
            if !is_filespec {
                return Err(malformed(
                    "media clip data /D is neither a filespec nor a stream",
                ));
            }
            document
                .filespec(&resolved)
                .map(|filespec| MediaClipDataSource::FileSpec(Box::new(filespec)))
                .map_err(|error| malformed(format!("media clip data /D: {error}")))
        }
        _ => Err(malformed(
            "media clip data /D is neither a filespec nor a stream",
        )),
    }
}

fn parse_media_permissions(
    document: &Document,
    value: &Value,
) -> Result<MediaPermissions, RenditionError> {
    let resolved = resolve(document, value)?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| malformed("media permissions is not a dictionary"))?;
    check_type(dict, "MediaPermissions", "media permissions")?;
    let temporary_file = match dict.get("TF") {
        None => TemporaryFilePermission::Never,
        Some(value) => {
            let name = text_value(value)
                .ok_or_else(|| malformed("media permissions /TF is not an ASCII string"))?;
            if !name.is_ascii() {
                return Err(malformed("media permissions /TF is not ASCII"));
            }
            TemporaryFilePermission::parse(&name)
        }
    };
    Ok(MediaPermissions { temporary_file })
}

fn parse_clip_handler(
    document: &Document,
    value: &Value,
    key: &str,
) -> Result<MediaClipHandler, RenditionError> {
    let resolved = resolve(document, value)?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| malformed(format!("media clip data /{key} is not a dictionary")))?;
    let base_url = match dict.get("BU") {
        None => None,
        Some(value) => {
            let text = text_value(value)
                .ok_or_else(|| malformed(format!("media clip data /{key} /BU is not a string")))?;
            if !text.is_ascii() || text.is_empty() {
                return Err(malformed(format!(
                    "media clip data /{key} /BU is not a non-empty ASCII URL"
                )));
            }
            Some(text)
        }
    };
    Ok(MediaClipHandler { base_url })
}

fn parse_section_handler(
    document: &Document,
    value: &Value,
    key: &str,
) -> Result<MediaClipSectionHandler, RenditionError> {
    let resolved = resolve(document, value)?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| malformed(format!("media clip section /{key} is not a dictionary")))?;
    for offset in ["B", "E"] {
        if let Some(value) = dict.get(offset)
            && !matches!(value, Value::Dict(_) | Value::Ref(..))
        {
            return Err(malformed(format!(
                "media clip section /{key} /{offset} is not a dictionary"
            )));
        }
    }
    Ok(MediaClipSectionHandler {
        begin: dict.get("B").cloned(),
        end: dict.get("E").cloned(),
    })
}

fn check_type(dict: &Dict, expected: &str, context: &str) -> Result<(), RenditionError> {
    match dict.get("Type").and_then(Value::as_name) {
        None => Ok(()),
        Some(name) if name == expected => Ok(()),
        Some(other) => Err(malformed(format!(
            "{context} /Type is {other}, expected {expected}"
        ))),
    }
}

#[cfg(test)]
mod tests;
