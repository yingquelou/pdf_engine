//! 文件规格与嵌入文件流读取（7.11.2–7.11.4）。
//!
//! - 目录 `/Names /EmbeddedFiles` 名称树（复用 E1 的名称字典语义）→
//!   [`Document::embedded_files`]，键按 PDF 文本串解码。
//! - 文件规格字典（Table 44）：`/FS`、`/F`、`/UF`、`/Desc`、平台条目
//!   `/DOS`/`/Mac`/`/Unix`、`/ID`、`/V`、`/EF`、`/RF`、`/CI`（7.11.6
//!   集合项，须为间接引用）。
//! - `/EF` 的三种载体（直接流、嵌套文件规格、双元素数组）统一展平为
//!   [`Filespec::embedded`]，数组按声明顺序保留低版本与 Unicode 名版本。
//! - `/RF` 关联文件数组（7.11.4.2）按载体键展开为
//!   [`RelatedFile`] 对（名字 + 嵌入文件流）。
//! - 嵌入文件流（Table 45）：`/Subtype` MIME 名、`/Params` 的 `/Size`、
//!   `/CheckSum`、`/CreationDate`、`/ModDate`；`/CheckSum` 校验按
//!   7.11.4.3 忽略数据中的 ASCII 空白字符。
//! - 防护：文件规格嵌套深度 8、引用环检测、单个文件规格流数量上限。

use std::collections::{BTreeMap, HashSet};
use std::fmt;

use crate::core::reader::content::text_value;
use crate::core::reader::navigation::{NavigationError, resolve};
use crate::core::reader::security::{CryptoBackend, RustCryptoBackend};
use crate::core::types::{Dict, Document, Value};

mod collection_item;

pub use collection_item::{
    CollectionItem, CollectionItemValue, CollectionSubitem, CollectionSubitemValue,
};

/// 文件规格与 `/EF` 载体的最大嵌套深度。
const MAX_FILESPEC_DEPTH: usize = 8;
/// 单个文件规格可展开的嵌入文件流上限（含 `/RF` 关联文件）。
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

/// 文件规格字典（7.11.3 Table 44）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Filespec {
    /// 文件规格对象的间接引用；直接嵌入时为 `None`。
    pub reference: Option<(i64, i64)>,
    /// `/FS` 文件系统名（不含斜杠；标准仅 `URL`，其余为注册名）。
    pub file_system: Option<String>,
    /// `/F` 7 位 ASCII 文件名。
    pub short_name: Option<String>,
    /// `/UF` Unicode 文件名（PDF 1.7）。
    pub unicode_name: Option<String>,
    /// `/Desc` 说明文本串（PDF 1.6）。
    pub description: Option<String>,
    /// `/DOS` 平台文件名字节串（规范已弃用，仅读取）。
    pub dos_name: Option<Vec<u8>>,
    /// `/Mac` 平台文件名字节串（规范已弃用，仅读取）。
    pub mac_name: Option<Vec<u8>>,
    /// `/Unix` 平台文件名字节串（规范已弃用，仅读取）。
    pub unix_name: Option<Vec<u8>>,
    /// `/ID` 被引用文件的标识符（两个字节串，7.11.4）。
    pub file_id: Option<[Vec<u8>; 2]>,
    /// `/V` 易变标记（PDF 1.2，缺省 `false`）；缺失时为 `None`。
    pub volatile: Option<bool>,
    /// `/EF` 展平后的嵌入文件流；空表示引用型/外部文件规格。
    pub embedded: Vec<EmbeddedFile>,
    /// `/RF` 关联文件数组（PDF 1.3）：载体键（`F`/`UF`/`DOS`/`Mac`/`Unix`）
    /// → 名字与嵌入文件流对。
    pub related_files: BTreeMap<String, Vec<RelatedFile>>,
    /// `/CI` 集合项字典（PDF 1.7，7.11.6）。
    pub collection_item: Option<CollectionItem>,
    /// 完整文件规格字典，未建模条目经此访问。
    pub dict: Dict,
}

/// `/RF` 关联文件对（7.11.4.2）：文件名字与对应的嵌入文件流。
#[derive(Debug, Clone, PartialEq)]
pub struct RelatedFile {
    /// 关联文件名字（文本串解码结果）。
    pub name: String,
    /// 关联文件内容。
    pub file: EmbeddedFile,
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
        RustCryptoBackend.md5(&filtered)
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
        validate_optional_byte_string(dict, "DOS")?;
        validate_optional_byte_string(dict, "Mac")?;
        validate_optional_byte_string(dict, "Unix")?;
        let file_system = match dict.get("FS") {
            None => None,
            Some(value) => Some(
                value
                    .as_name()
                    .ok_or_else(|| malformed("/Filespec /FS is not a name"))?
                    .to_string(),
            ),
        };
        let volatile = match dict.get("V") {
            None => None,
            Some(Value::Bool(flag)) => Some(*flag),
            Some(_) => return Err(malformed("/Filespec /V is not a boolean")),
        };
        let mut embedded = Vec::new();
        if let Some(ef) = dict.get("EF") {
            self.collect_embedded(ef, depth + 1, visited, &mut embedded)?;
        }
        let related_files = match dict.get("RF") {
            None => BTreeMap::new(),
            Some(rf) => {
                let ef = dict
                    .get("EF")
                    .ok_or_else(|| malformed("/RF requires an /EF dictionary"))?;
                let resolved_ef =
                    resolve(self, ef).map_err(|error| malformed(error.to_string()))?;
                let ef_dict = resolved_ef
                    .as_dict()
                    .ok_or_else(|| malformed("/EF must be a dictionary when /RF is present"))?;
                self.parse_related_files(rf, ef_dict)?
            }
        };
        let related_count: usize = related_files.values().map(Vec::len).sum();
        if embedded.len() + related_count > MAX_EMBEDDED_STREAMS {
            return Err(EmbeddedFileError::LimitExceeded(
                "embedded file stream count",
            ));
        }
        let collection_item = match dict.get("CI") {
            None => None,
            Some(value) => Some(collection_item::parse_collection_item(self, value)?),
        };
        Ok(Filespec {
            reference,
            file_system,
            short_name: dict.get("F").and_then(text_value),
            unicode_name: dict.get("UF").and_then(text_value),
            description: dict.get("Desc").and_then(text_value),
            dos_name: dict.get("DOS").and_then(string_bytes),
            mac_name: dict.get("Mac").and_then(string_bytes),
            unix_name: dict.get("Unix").and_then(string_bytes),
            file_id: parse_file_id(dict.get("ID"))?,
            volatile,
            embedded,
            related_files,
            collection_item,
            dict: dict.clone(),
        })
    }

    /// `/RF` 关联文件数组（7.11.4.2）：每个载体键都须出现在 `/EF` 中，
    /// 值为 `[文件名 流 文件名 流 ...]`（2n 个元素）。
    fn parse_related_files(
        &self,
        value: &Value,
        ef_dict: &Dict,
    ) -> Result<BTreeMap<String, Vec<RelatedFile>>> {
        let resolved = resolve(self, value).map_err(|error| malformed(error.to_string()))?;
        let rf_dict = resolved
            .as_dict()
            .ok_or_else(|| malformed("/RF is not a dictionary"))?;
        let mut out = BTreeMap::new();
        for (carrier, entry) in &rf_dict.0 {
            if !ef_dict.0.contains_key(carrier) {
                return Err(malformed(format!("/RF key /{carrier} is absent from /EF")));
            }
            let resolved_entry =
                resolve(self, entry).map_err(|error| malformed(error.to_string()))?;
            let array = resolved_entry
                .as_array()
                .ok_or_else(|| malformed(format!("/RF /{carrier} is not an array")))?;
            if array.len() < 2 || !array.len().is_multiple_of(2) {
                return Err(malformed(format!(
                    "/RF /{carrier} must hold an even number of name/stream pairs"
                )));
            }
            let mut pairs = Vec::with_capacity(array.len() / 2);
            for pair in array.0.chunks(2) {
                let name = text_value(&pair[0]).ok_or_else(|| {
                    malformed(format!("/RF /{carrier} file name is not a text string"))
                })?;
                let reference = pair[1].as_ref();
                let resolved_file =
                    resolve(self, &pair[1]).map_err(|error| malformed(error.to_string()))?;
                let file = self.parse_embedded_stream(&resolved_file, reference)?;
                pairs.push(RelatedFile { name, file });
            }
            out.insert(carrier.clone(), pairs);
        }
        Ok(out)
    }

    /// `/EF` 载体：流、文件规格字典、载体字典（`/F` `/UF` `/DOS` `/Mac` `/Unix`）
    /// 或前三者组成的数组。
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
            Value::Dict(dict) if is_ef_carrier_dict(dict) => {
                for entry in dict.0.values() {
                    self.collect_embedded(entry, depth, visited, out)?;
                }
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
                "/EF value must be a stream, file specification, carrier dictionary or array, got {other:?}"
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

/// `/EF` 载体字典（Table 44 `/EF`）：键仅取五个载体名，值均为流。
fn is_ef_carrier_dict(dict: &Dict) -> bool {
    const CARRIERS: [&str; 5] = ["F", "UF", "DOS", "Mac", "Unix"];
    !dict.0.is_empty()
        && dict.0.keys().all(|key| CARRIERS.contains(&key.as_str()))
        && dict
            .0
            .values()
            .all(|value| !matches!(value, Value::String(_) | Value::HexString(_)))
}

/// `/F` 与 `/UF` 缺失可以，但存在时必须是文本串。
fn validate_optional_text(dict: &Dict, key: &str) -> Result<()> {
    match dict.get(key) {
        None => Ok(()),
        Some(value) if text_value(value).is_some() => Ok(()),
        Some(_) => Err(malformed(format!("/Filespec /{key} is not a text string"))),
    }
}

/// `/DOS` `/Mac` `/Unix` 缺失可以，但存在时必须是字节串。
fn validate_optional_byte_string(dict: &Dict, key: &str) -> Result<()> {
    match dict.get(key) {
        None => Ok(()),
        Some(value) if string_bytes(value).is_some() => Ok(()),
        Some(_) => Err(malformed(format!("/Filespec /{key} is not a byte string"))),
    }
}

/// `/ID`：两个字节串组成的文件标识符数组（7.11.4）。
fn parse_file_id(value: Option<&Value>) -> Result<Option<[Vec<u8>; 2]>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let array = value
        .as_array()
        .ok_or_else(|| malformed("/Filespec /ID is not an array"))?;
    if array.len() != 2 {
        return Err(malformed("/Filespec /ID must hold two byte strings"));
    }
    let first = string_bytes(&array.0[0])
        .ok_or_else(|| malformed("/Filespec /ID entry is not a byte string"))?;
    let second = string_bytes(&array.0[1])
        .ok_or_else(|| malformed("/Filespec /ID entry is not a byte string"))?;
    Ok(Some([first, second]))
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
