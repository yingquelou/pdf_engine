//! Web Capture（14.10）：SpiderInfo 信息字典、内容集与来源信息（Table 350–356）。
//!
//! 本层只做类型化读取：内容集是 URLS/IDS 名称树条目指向的字典（或字典数组），
//! 按 Table 352/353/354 解析；来源信息按 Table 355/356 解析。数字标识符
//! （MD5）与 URL 规范化算法（14.10.3.2/14.10.3.3）不在此实现，由捕获端负责。

use std::fmt;

use crate::core::reader::navigation::{catalog, resolve};
use crate::core::types::{Dict, Document, Value};

#[cfg(test)]
mod tests;

/// Web Capture 读取中的类型化错误。
#[derive(Debug, Clone, PartialEq)]
pub enum WebCaptureError {
    /// 目录或名称树不可解析。
    Navigation(String),
    /// 条目结构损坏。
    Malformed(String),
}

impl fmt::Display for WebCaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Navigation(message) | Self::Malformed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for WebCaptureError {}

fn fail(message: impl Into<String>) -> WebCaptureError {
    WebCaptureError::Malformed(message.into())
}

fn navigation(error: crate::core::reader::navigation::NavigationError) -> WebCaptureError {
    WebCaptureError::Navigation(error.to_string())
}

/// 目录 `/SpiderInfo` Web Capture 信息字典（Table 350）。
#[derive(Debug, Clone, PartialEq)]
pub struct SpiderInfo {
    /// `/V` Web Capture 版本号（符合文件为 1.0）。
    pub version: f64,
    /// `/C` 命令字典引用数组（原样保留）。
    pub commands: Vec<Value>,
    /// 完整字典。
    pub dict: Dict,
}

/// 内容集（Table 352）。
#[derive(Debug, Clone, PartialEq)]
pub enum ContentSet {
    /// 页面集（`/S /SPS`，Table 353）。
    Page(PageSet),
    /// 图像集（`/S /SIS`，Table 354）。
    Image(ImageSet),
}

impl ContentSet {
    /// 数字标识符 `/ID`。
    pub fn id(&self) -> &[u8] {
        match self {
            Self::Page(set) => &set.id,
            Self::Image(set) => &set.id,
        }
    }

    /// 内容集成员对象（`/O`）。
    pub fn objects(&self) -> &[Value] {
        match self {
            Self::Page(set) => &set.objects,
            Self::Image(set) => &set.objects,
        }
    }

    /// 来源信息列表（`/SI`）。
    pub fn sources(&self) -> &[SourceInfo] {
        match self {
            Self::Page(set) => &set.sources,
            Self::Image(set) => &set.sources,
        }
    }
}

/// 页面集（Table 353）。
#[derive(Debug, Clone, PartialEq)]
pub struct PageSet {
    pub id: Vec<u8>,
    pub objects: Vec<Value>,
    pub sources: Vec<SourceInfo>,
    pub content_type: Option<String>,
    pub timestamp: Option<Value>,
    pub title: Option<String>,
    pub text_id: Option<Vec<u8>>,
    pub dict: Dict,
}

/// 图像集（Table 354）。
#[derive(Debug, Clone, PartialEq)]
pub struct ImageSet {
    pub id: Vec<u8>,
    pub objects: Vec<Value>,
    pub sources: Vec<SourceInfo>,
    pub content_type: Option<String>,
    pub timestamp: Option<Value>,
    /// `/R` 引用计数：单个整数或与 `/O` 平行的数组。
    pub reference_counts: Vec<i64>,
    pub dict: Dict,
}

/// 来源信息字典（Table 355）。
#[derive(Debug, Clone, PartialEq)]
pub struct SourceInfo {
    /// `/AU`：单个 URL 字符串或多条重定向链（Table 356）。
    pub urls: UrlAliases,
    pub timestamp: Option<Value>,
    pub expiration: Option<Value>,
    /// `/S`：0 非表单、1 GET、2 POST。
    pub submission: Option<i64>,
    /// `/C` 命令字典引用（原样保留）。
    pub command: Option<Value>,
    pub dict: Dict,
}

/// Web Capture 命令字典（Table 357）。
#[derive(Debug, Clone, PartialEq)]
pub struct WebCaptureCommand {
    /// `/URL` 初始请求 URL（ASCII）。
    pub url: String,
    /// `/L` 跟随的层级数，缺省 1。
    pub levels: i64,
    /// `/F` 标志位（Table 358：bit1 SameSite、bit2 SamePath、bit3 Submit）。
    pub flags: i64,
    /// `/P` 提交的数据（字符串或流，原样保留）。
    pub posted: Option<Value>,
    /// `/CT` POST 内容类型。
    pub content_type: Option<String>,
    /// `/H` 附加 HTTP 请求头。
    pub headers: Option<String>,
    /// `/S` 命令设置字典引用（原样保留）。
    pub settings: Option<Value>,
    pub dict: Dict,
}

impl WebCaptureCommand {
    /// bit 1：仅从初始 URL 的主机取出。
    pub fn same_site(&self) -> bool {
        self.flags & 0b001 != 0
    }

    /// bit 2：仅从初始 URL 的路径取出。
    pub fn same_path(&self) -> bool {
        self.flags & 0b010 != 0
    }

    /// bit 3：命令表示表单提交。
    pub fn submit(&self) -> bool {
        self.flags & 0b100 != 0
    }
}

/// Web Capture 命令设置字典（Table 359）。
#[derive(Debug, Clone, PartialEq)]
pub struct WebCaptureCommandSettings {
    /// `/G` 所有转换引擎共享的全局设置（原样保留）。
    pub global: Option<Dict>,
    /// `/C` 各转换引擎设置：引擎名 → 设置字典。
    pub engines: std::collections::BTreeMap<String, Dict>,
    pub dict: Dict,
}

/// `/AU` 取值：单 URL 或 URL 别名字典中的多条链。
#[derive(Debug, Clone, PartialEq)]
pub enum UrlAliases {
    /// 单个 ASCII URL。
    Single(String),
    /// 每条链按重定向顺序排列的 URL。
    Chains(Vec<Vec<String>>),
}

/// 解码字节串（字面/十六进制）为原始字节。
fn byte_string(value: &Value) -> Result<Vec<u8>, WebCaptureError> {
    match value {
        Value::String(text) => Ok(text.as_bytes().to_vec()),
        Value::HexString(hex) => {
            let digits: Vec<u8> = hex
                .bytes()
                .filter(|byte| !byte.is_ascii_whitespace())
                .collect();
            let mut out = Vec::with_capacity(digits.len() / 2);
            for pair in digits.as_chunks::<2>().0 {
                let hi = (pair[0] as char).to_digit(16).unwrap_or(0) as u8;
                let lo = (pair[1] as char).to_digit(16).unwrap_or(0) as u8;
                out.push(hi << 4 | lo);
            }
            Ok(out)
        }
        other => Err(fail(format!(
            "expected byte string, found {}",
            other.type_name()
        ))),
    }
}

/// 可解码为文本的字符串（ASCII/PDFDocEncoding 近似：直接取字面）。
fn text_string(value: &Value) -> Result<String, WebCaptureError> {
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::HexString(_) => Ok(String::from_utf8_lossy(&byte_string(value)?).into_owned()),
        other => Err(fail(format!(
            "expected text string, found {}",
            other.type_name()
        ))),
    }
}

impl Document {
    /// 目录 `/SpiderInfo`（Table 350）；缺失返回 `Ok(None)`。
    pub fn web_capture_info(&self) -> Result<Option<SpiderInfo>, WebCaptureError> {
        let catalog = catalog(self).map_err(navigation)?;
        let Some(value) = catalog.get("SpiderInfo") else {
            return Ok(None);
        };
        let resolved = resolve(self, value).map_err(navigation)?;
        let dict = resolved
            .as_dict()
            .ok_or_else(|| fail("/SpiderInfo is not a dictionary"))?;
        let version = match dict.get("V") {
            Some(Value::Float(value)) => *value,
            Some(Value::Integer(value)) => *value as f64,
            _ => return Err(fail("/SpiderInfo /V must be a number")),
        };
        let commands = match dict.get("C") {
            Some(value) => {
                let resolved = resolve(self, value).map_err(navigation)?;
                let array = resolved
                    .as_array()
                    .ok_or_else(|| fail("/SpiderInfo /C must be an array"))?;
                array.0.clone()
            }
            None => Vec::new(),
        };
        Ok(Some(SpiderInfo {
            version,
            commands,
            dict: dict.clone(),
        }))
    }

    /// 解析 Web Capture 命令设置字典（Table 359）。
    pub fn web_capture_command_settings(
        &self,
        value: &Value,
    ) -> Result<Option<WebCaptureCommandSettings>, WebCaptureError> {
        let resolved = resolve(self, value).map_err(navigation)?;
        let dict = resolved
            .as_dict()
            .ok_or_else(|| fail("web capture command settings is not a dictionary"))?;
        let global = match dict.get("G") {
            Some(value) => {
                let resolved = resolve(self, value).map_err(navigation)?;
                Some(
                    resolved
                        .as_dict()
                        .ok_or_else(|| fail("command settings /G is not a dictionary"))?
                        .clone(),
                )
            }
            None => None,
        };
        let mut engines = std::collections::BTreeMap::new();
        if let Some(value) = dict.get("C") {
            let resolved = resolve(self, value).map_err(navigation)?;
            let engines_dict = resolved
                .as_dict()
                .ok_or_else(|| fail("command settings /C is not a dictionary"))?;
            for (name, raw) in &engines_dict.0 {
                let resolved = resolve(self, raw).map_err(navigation)?;
                let settings = resolved.as_dict().ok_or_else(|| {
                    fail(format!(
                        "command settings engine /{name} is not a dictionary"
                    ))
                })?;
                engines.insert(name.clone(), settings.clone());
            }
        }
        Ok(Some(WebCaptureCommandSettings {
            global,
            engines,
            dict: dict.clone(),
        }))
    }

    /// 页面首选缩放（Table 30 `/PZ`，14.10.6，PDF 1.3）；须为正有限数。
    pub fn page_preferred_zoom(&self, page_index: usize) -> Result<Option<f64>, WebCaptureError> {
        let page = self
            .locate_page_node(page_index)
            .map_err(|error| WebCaptureError::Navigation(error.to_string()))?;
        let dict = page
            .as_dict()
            .ok_or_else(|| fail("page node is not a dictionary"))?;
        let value = match dict.get("PZ") {
            Some(Value::Float(value)) => *value,
            Some(Value::Integer(value)) => *value as f64,
            Some(other) => {
                return Err(fail(format!(
                    "page /PZ must be a number, found {}",
                    other.type_name()
                )));
            }
            None => return Ok(None),
        };
        if !value.is_finite() || value <= 0.0 {
            return Err(fail("page /PZ must be a positive finite number"));
        }
        Ok(Some(value))
    }

    /// 页面的 Web Capture 数字标识符（Table 30 `/ID`，14.10.6）；原始字节。
    pub fn page_web_capture_id(
        &self,
        page_index: usize,
    ) -> Result<Option<Vec<u8>>, WebCaptureError> {
        let page = self
            .locate_page_node(page_index)
            .map_err(|error| WebCaptureError::Navigation(error.to_string()))?;
        let dict = page
            .as_dict()
            .ok_or_else(|| fail("page node is not a dictionary"))?;
        match dict.get("ID") {
            Some(value) => byte_string(value).map(Some),
            None => Ok(None),
        }
    }

    /// 解析一个 Web Capture 命令字典（Table 357）。
    pub fn web_capture_command(
        &self,
        value: &Value,
    ) -> Result<Option<WebCaptureCommand>, WebCaptureError> {
        let resolved = resolve(self, value).map_err(navigation)?;
        let dict = resolved
            .as_dict()
            .ok_or_else(|| fail("web capture command is not a dictionary"))?;
        let url = match dict.get("URL") {
            Some(value) => text_string(value)?,
            None => return Err(fail("web capture command /URL is required")),
        };
        if url.is_empty() || !url.is_ascii() {
            return Err(fail("web capture command /URL must be non-empty ASCII"));
        }
        let levels = match dict.get("L") {
            Some(value) => value
                .as_integer()
                .ok_or_else(|| fail("web capture command /L must be an integer"))?,
            None => 1,
        };
        if levels < 0 {
            return Err(fail("web capture command /L must be non-negative"));
        }
        let flags = match dict.get("F") {
            Some(value) => value
                .as_integer()
                .ok_or_else(|| fail("web capture command /F must be an integer"))?,
            None => 0,
        };
        if flags & !0b111 != 0 {
            return Err(fail(format!(
                "web capture command /F has reserved bits set ({flags:#b})"
            )));
        }
        let content_type = match dict.get("CT") {
            Some(value) => {
                let text = text_string(value)?;
                if !text.is_ascii() {
                    return Err(fail("web capture command /CT must be ASCII"));
                }
                Some(text)
            }
            None => None,
        };
        let headers = match dict.get("H") {
            Some(value) => Some(text_string(value)?),
            None => None,
        };
        Ok(Some(WebCaptureCommand {
            url,
            levels,
            flags,
            posted: dict.get("P").cloned(),
            content_type,
            headers,
            settings: dict.get("S").cloned(),
            dict: dict.clone(),
        }))
    }

    /// 把 `/SpiderInfo /C` 的命令引用数组解析为类型化命令。
    pub fn web_capture_commands(
        &self,
        info: &SpiderInfo,
    ) -> Result<Vec<WebCaptureCommand>, WebCaptureError> {
        let mut out = Vec::with_capacity(info.commands.len());
        for value in &info.commands {
            if let Some(command) = self.web_capture_command(value)? {
                out.push(command);
            }
        }
        Ok(out)
    }

    /// 解析一个内容集字典（Table 352）。
    pub fn web_capture_content_set(&self, value: &Value) -> Result<ContentSet, WebCaptureError> {
        let resolved = resolve(self, value).map_err(navigation)?;
        let dict = resolved
            .as_dict()
            .ok_or_else(|| fail("content set is not a dictionary"))?;
        if let Some(kind) = dict.get("Type")
            && kind.as_name() != Some("SpiderContentSet")
        {
            return Err(fail(format!(
                "content set /Type must be /SpiderContentSet, got {kind:?}"
            )));
        }
        let subtype = dict
            .get("S")
            .and_then(|value| value.as_name())
            .ok_or_else(|| fail("content set /S is required"))?;
        let id = dict
            .get("ID")
            .ok_or_else(|| fail("content set /ID is required"))
            .and_then(|value| byte_string(value))?;
        let objects = match dict.get("O") {
            Some(value) => {
                let resolved = resolve(self, value).map_err(navigation)?;
                resolved
                    .as_array()
                    .ok_or_else(|| fail("content set /O must be an array"))?
                    .0
                    .clone()
            }
            None => return Err(fail("content set /O is required")),
        };
        let sources = self.parse_source_list(
            dict.get("SI")
                .ok_or_else(|| fail("content set /SI is required"))?,
        )?;
        let content_type = match dict.get("CT") {
            Some(value) => Some(text_string(value)?),
            None => None,
        };
        let timestamp = dict.get("TS").cloned();
        match subtype {
            "SPS" => Ok(ContentSet::Page(PageSet {
                id,
                objects,
                sources,
                content_type,
                timestamp,
                title: match dict.get("T") {
                    Some(value) => Some(text_string(value)?),
                    None => None,
                },
                text_id: match dict.get("TID") {
                    Some(value) => Some(byte_string(value)?),
                    None => None,
                },
                dict: dict.clone(),
            })),
            "SIS" => {
                let reference_counts = self.parse_reference_counts(
                    dict.get("R")
                        .ok_or_else(|| fail("image set /R is required"))?,
                    objects.len(),
                )?;
                Ok(ContentSet::Image(ImageSet {
                    id,
                    objects,
                    sources,
                    content_type,
                    timestamp,
                    reference_counts,
                    dict: dict.clone(),
                }))
            }
            other => Err(fail(format!(
                "content set /S must be /SPS or /SIS, got /{other}"
            ))),
        }
    }

    /// `/SI`：单个来源信息字典或字典数组。
    fn parse_source_list(&self, value: &Value) -> Result<Vec<SourceInfo>, WebCaptureError> {
        let resolved = resolve(self, value).map_err(navigation)?;
        match &resolved {
            Value::Array(items) => items
                .0
                .iter()
                .map(|item| self.parse_source_info(item))
                .collect(),
            _ => Ok(vec![self.parse_source_info(&resolved)?]),
        }
    }

    fn parse_source_info(&self, value: &Value) -> Result<SourceInfo, WebCaptureError> {
        let resolved = resolve(self, value).map_err(navigation)?;
        let dict = resolved
            .as_dict()
            .ok_or_else(|| fail("source information is not a dictionary"))?;
        let urls = self.parse_url_aliases(
            dict.get("AU")
                .ok_or_else(|| fail("source information /AU is required"))?,
        )?;
        let submission = match dict.get("S") {
            Some(value) => {
                let code = value
                    .as_integer()
                    .ok_or_else(|| fail("source information /S must be an integer"))?;
                if !(0..=2).contains(&code) {
                    return Err(fail(format!(
                        "source information /S must be 0, 1 or 2, got {code}"
                    )));
                }
                Some(code)
            }
            None => None,
        };
        Ok(SourceInfo {
            urls,
            timestamp: dict.get("TS").cloned(),
            expiration: dict.get("E").cloned(),
            submission,
            command: dict.get("C").cloned(),
            dict: dict.clone(),
        })
    }

    fn parse_url_aliases(&self, value: &Value) -> Result<UrlAliases, WebCaptureError> {
        let resolved = resolve(self, value).map_err(navigation)?;
        match &resolved {
            Value::String(_) | Value::HexString(_) => {
                Ok(UrlAliases::Single(text_string(&resolved)?))
            }
            Value::Dict(dict) => {
                let chains = dict
                    .get("C")
                    .ok_or_else(|| fail("URL alias dictionary /C is required"))?;
                let resolved = resolve(self, chains).map_err(navigation)?;
                let chains = resolved
                    .as_array()
                    .ok_or_else(|| fail("URL alias dictionary /C must be an array"))?;
                let mut out = Vec::with_capacity(chains.0.len());
                for chain in &chains.0 {
                    let resolved = resolve(self, chain).map_err(navigation)?;
                    let chain = resolved
                        .as_array()
                        .ok_or_else(|| fail("URL alias chain must be an array"))?;
                    let mut urls = Vec::with_capacity(chain.0.len());
                    for url in &chain.0 {
                        if !matches!(url, Value::String(_) | Value::HexString(_)) {
                            return Err(fail(format!(
                                "URL alias entry must be a string, found {}",
                                url.type_name()
                            )));
                        }
                        urls.push(text_string(url)?);
                    }
                    out.push(urls);
                }
                Ok(UrlAliases::Chains(out))
            }
            other => Err(fail(format!(
                "source information /AU must be a string or dictionary, found {}",
                other.type_name()
            ))),
        }
    }

    fn parse_reference_counts(
        &self,
        value: &Value,
        object_count: usize,
    ) -> Result<Vec<i64>, WebCaptureError> {
        let resolved = resolve(self, value).map_err(navigation)?;
        match &resolved {
            Value::Array(items) => {
                let mut out = Vec::with_capacity(items.0.len());
                for item in &items.0 {
                    out.push(
                        item.as_integer()
                            .ok_or_else(|| fail("image set /R entry must be an integer"))?,
                    );
                }
                if out.len() != object_count {
                    return Err(fail(format!(
                        "image set /R has {} entries for {} objects",
                        out.len(),
                        object_count
                    )));
                }
                Ok(out)
            }
            other => {
                let count = other
                    .as_integer()
                    .ok_or_else(|| fail("image set /R must be an integer or array"))?;
                if object_count != 1 {
                    return Err(fail(format!(
                        "image set /R is a single integer but /O has {object_count} entries"
                    )));
                }
                Ok(vec![count])
            }
        }
    }

    /// 按 URL 查 URLS 名称树得到内容集（可能多个）；未命中返回空。
    pub fn web_capture_sets_by_url(&self, url: &str) -> Result<Vec<ContentSet>, WebCaptureError> {
        let tree = self.name_tree("URLS").map_err(navigation)?;
        let Some(value) = tree.get(url) else {
            return Ok(Vec::new());
        };
        self.web_capture_sets(value)
    }

    /// 按数字标识符（MD5 原始字节）查 IDS 名称树；未命中返回空。
    pub fn web_capture_sets_by_id(&self, id: &[u8]) -> Result<Vec<ContentSet>, WebCaptureError> {
        let key: String = id.iter().map(|byte| *byte as char).collect();
        let tree = self.name_tree("IDS").map_err(navigation)?;
        let Some(value) = tree.get(&key) else {
            return Ok(Vec::new());
        };
        self.web_capture_sets(value)
    }

    fn web_capture_sets(&self, value: &Value) -> Result<Vec<ContentSet>, WebCaptureError> {
        let resolved = resolve(self, value).map_err(navigation)?;
        match &resolved {
            Value::Array(items) => items
                .0
                .iter()
                .map(|item| self.web_capture_content_set(item))
                .collect(),
            _ => Ok(vec![self.web_capture_content_set(&resolved)?]),
        }
    }
}
