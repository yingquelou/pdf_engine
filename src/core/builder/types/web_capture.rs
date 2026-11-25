//! Web Capture 写端模型（14.10 Table 350–359）。
//!
//! 只建模页面集（`/S /SPS`）与来源信息；图像集、数字标识符计算（MD5）与
//! URL 规范化由调用方负责（本库只按给定 ID/URL 写名称树）。内容集与
//! URLS/IDS 名称树在导航后处理阶段分配对象号并写出。

use super::text_string_value;
use crate::core::types::{Dict, Value};

/// Web Capture 命令设置字典（Table 359）构建态。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WebCaptureCommandSettings {
    global: Option<Dict>,
    engines: Vec<(String, Dict)>,
}

impl WebCaptureCommandSettings {
    /// 空设置字典。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置 `/G` 全局设置。
    pub fn with_global(mut self, global: Dict) -> Self {
        self.global = Some(global);
        self
    }

    /// 追加一个转换引擎的设置 `/C /<engine>`。
    pub fn with_engine(mut self, engine: impl Into<String>, settings: Dict) -> Self {
        self.engines.push((engine.into(), settings));
        self
    }

    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new();
        if let Some(global) = &self.global {
            dict = dict.insert("G", Value::Dict(global.clone()));
        }
        if !self.engines.is_empty() {
            let mut engines = Dict::new();
            for (name, settings) in &self.engines {
                engines = engines.insert(name.clone(), Value::Dict(settings.clone()));
            }
            dict = dict.insert("C", Value::Dict(engines));
        }
        dict
    }
}

/// Web Capture 命令字典（Table 357）构建态。
#[derive(Debug, Clone, PartialEq)]
pub struct WebCaptureCommand {
    url: String,
    levels: i64,
    flags: i64,
    posted: Option<Value>,
    content_type: Option<String>,
    headers: Option<String>,
    settings: Option<WebCaptureCommandSettings>,
}

impl WebCaptureCommand {
    /// 以初始 URL 构造；`/URL` 必填。
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            levels: 1,
            flags: 0,
            posted: None,
            content_type: None,
            headers: None,
            settings: None,
        }
    }

    /// `/L` 跟随层级数。
    pub fn with_levels(mut self, levels: i64) -> Self {
        self.levels = levels;
        self
    }

    /// `/F` 标志位（Table 358，仅 bit1–3 可用）。
    pub fn with_flags(mut self, flags: i64) -> Self {
        self.flags = flags;
        self
    }

    /// `/P` 提交的数据（原样写出）。
    pub fn with_posted(mut self, posted: Value) -> Self {
        self.posted = Some(posted);
        self
    }

    /// `/CT` POST 内容类型。
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// `/H` 附加 HTTP 请求头。
    pub fn with_headers(mut self, headers: impl Into<String>) -> Self {
        self.headers = Some(headers.into());
        self
    }

    /// `/S` 命令设置字典。
    pub fn with_settings(mut self, settings: WebCaptureCommandSettings) -> Self {
        self.settings = Some(settings);
        self
    }

    pub(crate) fn validate(&self) -> Result<(), crate::core::writer::WriteError> {
        if self.url.is_empty() || !self.url.is_ascii() {
            return Err(crate::core::writer::WriteError::InvalidObject(
                "web capture command /URL must be non-empty ASCII".into(),
            ));
        }
        if self.levels < 0 {
            return Err(crate::core::writer::WriteError::InvalidObject(
                "web capture command /L must be non-negative".into(),
            ));
        }
        if self.flags & !0b111 != 0 {
            return Err(crate::core::writer::WriteError::InvalidObject(format!(
                "web capture command /F has reserved bits set ({:#b})",
                self.flags
            )));
        }
        if let Some(content_type) = &self.content_type
            && (content_type.is_empty() || !content_type.is_ascii())
        {
            return Err(crate::core::writer::WriteError::InvalidObject(
                "web capture command /CT must be non-empty ASCII".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new().insert("URL", text_string_value(&self.url));
        if self.levels != 1 {
            dict = dict.insert("L", Value::Integer(self.levels));
        }
        if self.flags != 0 {
            dict = dict.insert("F", Value::Integer(self.flags));
        }
        if let Some(posted) = &self.posted {
            dict = dict.insert("P", posted.clone());
        }
        if let Some(content_type) = &self.content_type {
            dict = dict.insert("CT", text_string_value(content_type));
        }
        if let Some(headers) = &self.headers {
            dict = dict.insert("H", text_string_value(headers));
        }
        if let Some(settings) = &self.settings {
            dict = dict.insert("S", Value::Dict(settings.to_dict()));
        }
        dict
    }
}

/// 来源信息字典（Table 355）的构建态。
#[derive(Debug, Clone, PartialEq)]
pub struct WebCaptureSource {
    /// URL 重定向链；单链单元素写出 `/AU (url)`，否则写 URL 别名字典。
    pub(crate) chains: Vec<Vec<String>>,
    /// `/S` 表单提交方式：0 非表单、1 GET、2 POST。
    pub(crate) submission: Option<i64>,
    /// `/TS` 时间戳（PDF 日期串）。
    pub(crate) timestamp: Option<String>,
    /// `/E` 过期时间（PDF 日期串）。
    pub(crate) expiration: Option<String>,
}

impl WebCaptureSource {
    /// 以单个来源 URL 构造。
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            chains: vec![vec![url.into()]],
            submission: None,
            timestamp: None,
            expiration: None,
        }
    }

    /// 追加一条 URL 重定向链（按重定向顺序）。所有链须指向同一目的地。
    pub fn with_alias_chain(mut self, chain: impl IntoIterator<Item = String>) -> Self {
        self.chains.push(chain.into_iter().collect());
        self
    }

    /// 设置 `/S`（须为 0/1/2）。
    pub fn with_submission(mut self, code: i64) -> Self {
        self.submission = Some(code);
        self
    }

    /// 设置 `/TS` 时间戳（PDF 日期串）。
    pub fn with_timestamp(mut self, date: impl Into<String>) -> Self {
        self.timestamp = Some(date.into());
        self
    }

    /// 设置 `/E` 过期时间（PDF 日期串）。
    pub fn with_expiration(mut self, date: impl Into<String>) -> Self {
        self.expiration = Some(date.into());
        self
    }
}

/// Web Capture 图像集（Table 352/354）。
///
/// 图像以「页面索引 + 该页图像资源序号」定位（与结构树 OBJR 的图像寻址一致），
/// 装配期映射为图像 XObject 的对象号。
#[derive(Debug, Clone, PartialEq)]
pub struct WebCaptureImageSet {
    pub(crate) id: Vec<u8>,
    pub(crate) images: Vec<(usize, usize)>,
    pub(crate) sources: Vec<WebCaptureSource>,
    pub(crate) content_type: Option<String>,
    pub(crate) timestamp: Option<String>,
    /// `/R` 引用计数，与 `images` 平行；缺省每个图像为 1。
    pub(crate) reference_counts: Vec<i64>,
}

impl WebCaptureImageSet {
    /// 以数字标识符与图像位置（页面索引 + 该页图像序号）构造。
    pub fn new(id: impl Into<Vec<u8>>, images: impl IntoIterator<Item = (usize, usize)>) -> Self {
        let images: Vec<(usize, usize)> = images.into_iter().collect();
        let reference_counts = vec![1; images.len()];
        Self {
            id: id.into(),
            images,
            sources: Vec::new(),
            content_type: None,
            timestamp: None,
            reference_counts,
        }
    }

    /// 追加来源信息。
    pub fn with_source(mut self, source: WebCaptureSource) -> Self {
        self.sources.push(source);
        self
    }

    /// 设置 `/CT` MIME 内容类型。
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// 设置 `/TS` 内容集创建时间。
    pub fn with_timestamp(mut self, date: impl Into<String>) -> Self {
        self.timestamp = Some(date.into());
        self
    }

    /// 设置 `/R` 引用计数（须与图像数等长）。
    pub fn with_reference_counts(mut self, counts: impl IntoIterator<Item = i64>) -> Self {
        self.reference_counts = counts.into_iter().collect();
        self
    }
}

/// Web Capture 页面集（Table 352/353）。
#[derive(Debug, Clone, PartialEq)]
pub struct WebCapturePageSet {
    pub(crate) id: Vec<u8>,
    pub(crate) pages: Vec<usize>,
    pub(crate) sources: Vec<WebCaptureSource>,
    pub(crate) content_type: Option<String>,
    pub(crate) timestamp: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) text_id: Option<Vec<u8>>,
}

impl WebCapturePageSet {
    /// 以数字标识符（原始字节）与页面索引列表构造。
    pub fn new(id: impl Into<Vec<u8>>, pages: impl IntoIterator<Item = usize>) -> Self {
        Self {
            id: id.into(),
            pages: pages.into_iter().collect(),
            sources: Vec::new(),
            content_type: None,
            timestamp: None,
            title: None,
            text_id: None,
        }
    }

    /// 追加来源信息。
    pub fn with_source(mut self, source: WebCaptureSource) -> Self {
        self.sources.push(source);
        self
    }

    /// 设置 `/CT` MIME 内容类型。
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// 设置 `/TS` 内容集创建时间。
    pub fn with_timestamp(mut self, date: impl Into<String>) -> Self {
        self.timestamp = Some(date.into());
        self
    }

    /// 设置 `/T` 人类可读标题。
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// 设置 `/TID` 文本标识符（原始字节）。
    pub fn with_text_id(mut self, text_id: impl Into<Vec<u8>>) -> Self {
        self.text_id = Some(text_id.into());
        self
    }
}
