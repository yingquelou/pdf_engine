//! 文件规格与嵌入文件流读取（7.11.2–7.11.4）。
//!
//! - 目录 `/Names /EmbeddedFiles` 名称树（复用 E1 的名称字典语义）→
//!   [`Document::embedded_files`]，键按 PDF 文本串解码。
//! - 文件规格字典（Table 42）：`/F`、`/UF`、`/Desc`、`/EF`、`/RF`
//!   （相对引用仅原样保留，不解析外部文件系统引用）。
//! - `/EF` 的三种载体（直接流、嵌套文件规格、双元素数组）统一展平为
//!   [`Filespec::embedded`]，数组按声明顺序保留低版本与 Unicode 名版本。
//! - 嵌入文件流（Table 40）：`/Subtype` MIME 名、`/Params` 的 `/Size`、
//!   `/CheckSum`、`/CreationDate`、`/ModDate`；`/CheckSum` 校验按
//!   7.11.4.3 忽略数据中的 ASCII 空白字符。
//! - 防护：文件规格嵌套深度 8、引用环检测、单个文件规格流数量上限。

use std::collections::{BTreeMap, HashSet};
use std::fmt;

use md5::{Digest, Md5};

use crate::core::reader::content::text_value;
use crate::core::reader::navigation::{NavigationError, resolve};
use crate::core::types::{Dict, Document, Value};

/// 文件规格与 `/EF` 载体的最大嵌套深度。
const MAX_FILESPEC_DEPTH: usize = 8;
/// 单个文件规格可展开的嵌入文件流上限。
const MAX_EMBEDDED_STREAMS: usize = 64;

/// 嵌入文件读取中的类型化错误。
#[derive(Debug, Clone, PartialEq)]
pub enum EmbeddedFileError {
    /// 文档目录缺失或无法解析为字典。
    CatalogMissing,
    /// 名称树、文件规格或嵌入文件流条目结构损坏。
    Malformed(String),
    /// 超出嵌套深度或流数量上限。
    LimitExceeded(&'static str),
}

impl fmt::Display for EmbeddedFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CatalogMissing => write!(f, "document catalog is missing or malformed"),
            Self::Malformed(message) => write!(f, "{message}"),
            Self::LimitExceeded(what) => write!(f, "embedded file limit exceeded: {what}"),
        }
    }
}

impl std::error::Error for EmbeddedFileError {}

type Result<T> = std::result::Result<T, EmbeddedFileError>;

fn malformed(message: impl Into<String>) -> EmbeddedFileError {
    EmbeddedFileError::Malformed(message.into())
}

fn from_navigation(error: NavigationError) -> EmbeddedFileError {
    match error {
        NavigationError::CatalogMissing => EmbeddedFileError::CatalogMissing,
        NavigationError::LimitExceeded(what) => EmbeddedFileError::LimitExceeded(what),
        other => EmbeddedFileError::Malformed(other.to_string()),
    }
}

/// 文件规格字典（7.11.4 Table 42）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Filespec {
    /// 文件规格对象的间接引用；直接嵌入时为 `None`。
    pub reference: Option<(i64, i64)>,
    /// `/F` 7 位 ASCII 文件名。
    pub short_name: Option<String>,
    /// `/UF` Unicode 文件名（PDF 1.4+）。
    pub unicode_name: Option<String>,
    /// `/Desc` 说明文本串（PDF 1.4+）。
    pub description: Option<String>,
    /// `/EF` 展平后的嵌入文件流；空表示引用型/外部文件规格。
    pub embedded: Vec<EmbeddedFile>,
    /// `/RF` 相对文件规格（PDF 1.6）原样保留，不解析。
    pub relative: Option<Value>,
    /// 完整文件规格字典，未建模条目（`/DOS`、`/Mac`、`/Unix`、`/ID` 等）经此访问。
    pub dict: Dict,
}

impl Filespec {
    /// 展示名：`/UF` 优先，回退 `/F`；两者皆缺时为空串。
    pub fn name(&self) -> &str {
        self.unicode_name
            .as_deref()
            .or(self.short_name.as_deref())
            .unwrap_or("")
    }

    /// 第一个嵌入文件流（常规单文件场景）。
    pub fn primary(&self) -> Option<&EmbeddedFile> {
        self.embedded.first()
    }
}

/// 嵌入文件流（7.11.3 Table 40）。
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddedFile {
    /// 流对象引用；直接嵌入时为 `None`。
    pub reference: Option<(i64, i64)>,
    /// `/Subtype` 的 MIME 类型名（不含斜杠，如 `application/pdf`）。
    pub subtype: Option<String>,
    /// `/Params` 文件参数。
    pub params: FileParameters,
    /// 解码后的文件字节（过滤器与解密已应用）。
    pub data: Vec<u8>,
    /// 完整流字典。
    pub dict: Dict,
}

impl EmbeddedFile {
    /// 按 7.11.4.3 计算校验值：数据去除 ASCII 空白（NUL、TAB、LF、FF、CR、
    /// SPACE）后的 MD5 摘要。
    pub fn computed_checksum(&self) -> [u8; 16] {
        let filtered: Vec<u8> = self
            .data
            .iter()
            .copied()
            .filter(|byte| !matches!(byte, 0 | 0o11 | 0o12 | 0o14 | 0o15 | b' '))
            .collect();
        Md5::digest(&filtered).into()
    }

    /// 声明的 `/CheckSum` 与计算值是否一致；未声明时返回 `None`。
    pub fn checksum_matches(&self) -> Option<bool> {
        self.params
            .checksum
            .map(|declared| declared == self.computed_checksum())
    }

    /// 声明的 `/Size` 是否等于解码后的字节数；未声明时返回 `None`。
    pub fn size_matches(&self) -> Option<bool> {
        self.params.size.map(|size| size == self.data.len() as i64)
    }
}

/// 文件参数字典（7.11.4.1 Table 39）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FileParameters {
    /// `/Size` 原始字节数。
    pub size: Option<i64>,
    /// `/CheckSum` 声明的 16 字节 MD5 摘要。
    pub checksum: Option<[u8; 16]>,
    /// `/CreationDate` 文本串（原样保留）。
    pub creation_date: Option<Value>,
    /// `/ModDate` 文本串（原样保留）。
    pub modification_date: Option<Value>,
    /// 完整 `/Params` 字典（`/MacEncoding` 等未建模条目经此访问）。
    pub dict: Option<Dict>,
}

impl Document {
    /// 读取目录 `/Names /EmbeddedFiles` 名称树（7.11.4）。
    ///
    /// 键按 PDF 文本串解码（UTF-16BE 或 PDFDocEncoding）；类别缺失返回
    /// 空映射。
    pub fn embedded_files(&self) -> Result<BTreeMap<String, Filespec>> {
        let tree = self.name_tree("EmbeddedFiles").map_err(from_navigation)?;
        let mut files = BTreeMap::new();
        for (raw_key, value) in tree {
            let name = decode_tree_key(&raw_key);
            let filespec = self.filespec(&value)?;
            files.insert(name, filespec);
        }
        Ok(files)
    }

    /// 按名解析单个嵌入文件。
    ///
    /// 先按名称树键精确匹配，再回退比较文件规格的 `/UF` 与 `/F` 名；
    /// 未命中返回 `None`（名称匹配为逐字符比较，不做大小写折叠）。
    pub fn embedded_file(&self, name: &str) -> Result<Option<Filespec>> {
        let files = self.embedded_files()?;
        if let Some(found) = files.get(name) {
            return Ok(Some(found.clone()));
        }
        Ok(files
            .values()
            .find(|filespec| {
                filespec.unicode_name.as_deref() == Some(name)
                    || filespec.short_name.as_deref() == Some(name)
            })
            .cloned())
    }

    /// 解析任意文件规格值（名称树条目、注解 `/FS`、`/EF` 嵌套载体）。
    ///
    /// 支持间接引用；值为嵌入文件流时按只含一个流的文件规格处理。
    pub fn filespec(&self, value: &Value) -> Result<Filespec> {
        let mut visited = HashSet::new();
        self.parse_filespec(value, 0, &mut visited)
    }

    /// 解析注解的 `/FS` 文件规格（12.5.6.14 FileAttachment 注解）。
    pub fn annotation_filespec(
        &self,
        annotation: &crate::core::reader::annotation::Annotation,
    ) -> Result<Option<Filespec>> {
        let Some(value) = annotation.dict.get("FS") else {
            return Ok(None);
        };
        self.filespec(value).map(Some)
    }

    fn parse_filespec(
        &self,
        value: &Value,
        depth: usize,
        visited: &mut HashSet<(i64, i64)>,
    ) -> Result<Filespec> {
        if depth > MAX_FILESPEC_DEPTH {
            return Err(EmbeddedFileError::LimitExceeded("file specification depth"));
        }
        let reference = value.as_ref();
        if let Some(key) = reference
            && !visited.insert(key)
        {
            return Err(malformed("file specification reference cycle"));
        }
        let resolved = resolve(self, value).map_err(|error| malformed(error.to_string()))?;
        // 宽容：名称树或 `/EF` 直接指向嵌入文件流而非文件规格。
        if matches!(resolved, Value::Stream(_, _)) {
            let embedded = self.parse_embedded_stream(&resolved, reference)?;
            return Ok(Filespec {
                reference,
                embedded: vec![embedded],
                ..Filespec::default()
            });
        }
        let dict = resolved
            .as_dict()
            .ok_or_else(|| malformed("file specification is not a dictionary"))?;
        validate_optional_text(dict, "F")?;
        validate_optional_text(dict, "UF")?;
        let mut embedded = Vec::new();
        if let Some(ef) = dict.get("EF") {
            self.collect_embedded(ef, depth + 1, visited, &mut embedded)?;
        }
        if embedded.len() > MAX_EMBEDDED_STREAMS {
            return Err(EmbeddedFileError::LimitExceeded(
                "embedded file stream count",
            ));
        }
        Ok(Filespec {
            reference,
            short_name: dict.get("F").and_then(text_value),
            unicode_name: dict.get("UF").and_then(text_value),
            description: dict.get("Desc").and_then(text_value),
            embedded,
            relative: dict.get("RF").cloned(),
            dict: dict.clone(),
        })
    }

    /// `/EF` 三种载体：流、嵌套文件规格、二者组成的数组。
    fn collect_embedded(
        &self,
        value: &Value,
        depth: usize,
        visited: &mut HashSet<(i64, i64)>,
        out: &mut Vec<EmbeddedFile>,
    ) -> Result<()> {
        let resolved = resolve(self, value).map_err(|error| malformed(error.to_string()))?;
        match &resolved {
            Value::Stream(_, _) => {
                let reference = value.as_ref().or(resolved.as_ref());
                out.push(self.parse_embedded_stream(&resolved, reference)?);
                Ok(())
            }
            Value::Dict(_) => {
                // 传入原值（可能是间接引用）以便环检测按引用登记。
                let nested = self.parse_filespec(value, depth, visited)?;
                out.extend(nested.embedded);
                Ok(())
            }
            Value::Array(items) => {
                for item in &items.0 {
                    self.collect_embedded(item, depth, visited, out)?;
                }
                Ok(())
            }
            other => Err(malformed(format!(
                "/EF value must be a file stream, file specification or array, got {other:?}"
            ))),
        }
    }

    fn parse_embedded_stream(
        &self,
        value: &Value,
        reference: Option<(i64, i64)>,
    ) -> Result<EmbeddedFile> {
        let Value::Stream(dict, _) = value else {
            return Err(malformed("embedded file is not a stream"));
        };
        let subtype = match dict.get("Subtype") {
            None => None,
            Some(item) => Some(
                item.as_name()
                    .ok_or_else(|| malformed("/Subtype of an embedded file stream is not a name"))?
                    .to_string(),
            ),
        };
        let params = match dict.get("Params") {
            None => FileParameters::default(),
            Some(item) => {
                let resolved = resolve(self, item).map_err(|error| malformed(error.to_string()))?;
                let params_dict = resolved
                    .as_dict()
                    .ok_or_else(|| malformed("/Params is not a dictionary"))?;
                self.parse_file_parameters(params_dict)?
            }
        };
        let data = self.decode_stream_bytes(value).map_err(|error| {
            malformed(format!("embedded file stream cannot be decoded: {error}"))
        })?;
        Ok(EmbeddedFile {
            reference,
            subtype,
            params,
            data,
            dict: dict.clone(),
        })
    }

    fn parse_file_parameters(&self, dict: &Dict) -> Result<FileParameters> {
        let size = match dict.get("Size") {
            None => None,
            Some(value) => {
                let resolved =
                    resolve(self, value).map_err(|error| malformed(error.to_string()))?;
                let size = resolved
                    .as_integer()
                    .ok_or_else(|| malformed("/Params /Size is not an integer"))?;
                if size < 0 {
                    return Err(malformed("/Params /Size is negative"));
                }
                Some(size)
            }
        };
        let checksum = match dict.get("CheckSum") {
            None => None,
            Some(value) => {
                let bytes = string_bytes(value)
                    .ok_or_else(|| malformed("/Params /CheckSum is not a string"))?;
                if bytes.len() != 16 {
                    return Err(malformed(format!(
                        "/Params /CheckSum must be 16 bytes, got {}",
                        bytes.len()
                    )));
                }
                let mut digest = [0u8; 16];
                digest.copy_from_slice(&bytes);
                Some(digest)
            }
        };
        Ok(FileParameters {
            size,
            checksum,
            creation_date: dict.get("CreationDate").cloned(),
            modification_date: dict.get("ModDate").cloned(),
            dict: Some(dict.clone()),
        })
    }
}

/// 名称树键：原始字节串（每字节一个 `char`）→ 解码文本串。
fn decode_tree_key(raw: &str) -> String {
    let bytes: Vec<u8> = raw.chars().map(|character| character as u8).collect();
    crate::core::types::PdfTextString::decode(&bytes)
        .map(crate::core::types::PdfTextString::into_string)
        .unwrap_or_else(|_| raw.to_string())
}

/// `/F` 与 `/UF` 缺失可以，但存在时必须是文本串。
fn validate_optional_text(dict: &Dict, key: &str) -> Result<()> {
    match dict.get(key) {
        None => Ok(()),
        Some(value) if text_value(value).is_some() => Ok(()),
        Some(_) => Err(malformed(format!("/Filespec /{key} is not a text string"))),
    }
}

fn string_bytes(value: &Value) -> Option<Vec<u8>> {
    match value {
        Value::String(text) => Some(text.chars().map(|character| character as u8).collect()),
        Value::HexString(hex) => {
            let digits: Vec<u8> = hex
                .bytes()
                .filter(|byte| !byte.is_ascii_whitespace())
                .collect();
            let padded;
            let pairs = if digits.len() % 2 == 1 {
                padded = {
                    let mut value = digits.clone();
                    value.push(b'0');
                    value
                };
                &padded[..]
            } else {
                &digits[..]
            };
            let mut bytes = Vec::with_capacity(pairs.len() / 2);
            for pair in pairs.chunks(2) {
                let high = (pair[0] as char).to_digit(16)?;
                let low = (pair[1] as char).to_digit(16)?;
                bytes.push((high * 16 + low) as u8);
            }
            Some(bytes)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
