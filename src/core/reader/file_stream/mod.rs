//! 外部文件流（external file stream）的类型化**读端模型**（ISO 32000-1:2008
//! 7.3.8 / 7.4.1 + Table 5；本仓库矩阵与 [`crate::core::filters::indirect`]
//! 沿用旧版 Adobe PDF Reference 的「Table 7」编号，指同一组条目）。
//!
//! 规范依据（`docs/PDF32000_2008.pdf`，`pdftotext -raw` 实测抽取）：
//!
//! - **Table 5（Entries common to all stream dictionaries）**含三个
//!   PDF 1.2 外部文件流条目：
//!   - `/F`「file specification (Optional; PDF 1.2) The file containing the
//!     stream data. If this entry is present, the bytes between `stream` and
//!     `endstream` **shall be ignored**. However, the `/Length` entry should
//!     still specify the number of those bytes…」；
//!   - `/FFilter`「name or array (Optional; PDF 1.2) …**The same rules apply
//!     as for** `/Filter`」；
//!   - `/FDecodeParms`「dictionary or array (Optional; PDF 1.2) …used by the
//!     filters specified by `/FFilter`. The same rules apply as for
//!     `/DecodeParms`」。
//! - 编号说明：本地 ISO 32000-1 抽取里 Table 6 是 Standard filters、Table 7
//!   是 LZW 编码示例；「外部文件流附加条目」在 32000 中并入 Table 5，故本
//!   模块引用 Table 5，`indirect.rs` 注释中的「Table 7」为同一组条目的旧
//!   编号别名。
//! - 7.4.1 General：「The filter or filters for a stream shall be specified
//!   by the Filter entry … (**or the FFilter entry if the stream is
//!   external**)」「These optional parameters shall be specified by the
//!   DecodeParms entry … (**or the FDecodeParms entry if the stream is
//!   external**)」；
//! - 7.6.1：「When a PDF stream object refers to an external file, the
//!   stream's contents **shall not be encrypted**, since they are not part
//!   of the PDF file itself」——故本模型不涉密文处理；
//! - `/FFilter`/`/FDecodeParms` 的间接引用形态（值与数组元素均可为引用，
//!   7.3.10）由共享规范化器 [`crate::core::filters::normalize_filter_entries`]
//!   处理，与图像读端（`reader::image::entries`）、`Document::decode_stream_bytes`
//!   同一入口；本模块只补它自己 doc 指明的缺口——**读端类型化建模**，
//!   不重复实现解引用。
//!
//! `/F` 的值是**文件规格**（7.11.2–7.11.3）：字符串形态或字典形态，字典形态
//! 复用 [`crate::core::reader::embedded::Filespec`]（Table 44）。版本门禁按
//! 规范原文：`/F`、`/FFilter`、`/FDecodeParms` 为 PDF 1.2；`/UF` 为 PDF 1.7
//! 且「should be used in addition to `/F`」（`/F` 供向后兼容，非废弃——
//! 32000 原文只把 `/DOS`/`/Mac`/`/Unix` 标为 obsolescent；仓库惯称的
//! 「`/U`」在 Table 44 中不存在，对应旧条目即 `/F`）。
//!
//! # 立场
//!
//! - **只诊断，不猜测**：出现任一外部文件条目（非 `null` 值）即认定该流
//!   声称是外部文件流，缺 `/F`、`/FFilter`↔`/FDecodeParms` 个数违例、形态
//!   非法、文件规格没有任何文件名字条目，都返回 [`ExternalFileStreamError`]；
//!   完全没有这些条目的流只是「不是外部文件流」，返回 `Ok(None)`，绝不
//!   把无关流读红。
//! - **不读取、不抓取任何外部文件**：本切片只建模流字典语义；正文按
//!   Table 5 `/F` 的 shall 一律忽略，`decode_stream_bytes` 的既有行为不变。
//! - **写端不在本切片**（矩阵 Writer 列如实保持未完成）：不新增任何
//!   builder/writer 代码；条目缺失时的默认值不物化，`/FFilter` 缺省建模为
//!   空过滤器列表。
//! - 库政策（规范对读端拒收条款沉默）：低于条目门禁版本的文件里出现
//!   `/FFilter`／`/UF` 时照常读取，不做硬性拒绝。
//!
//! # 交叉核对（本切片新增：保留取值，只报不一致）
//!
//! [`ExternalFileStream::problems`] 给出 [`ExternalFileStreamProblem`] 列表；
//! 三项都不改 [`ExternalFileStreamError`] 的判定，已能解析的文件结果逐字不变。
//!
//! - `VersionGate`：Table 5 三条目皆「(Optional; PDF 1.2)」——raw L1095、
//!   L1102、L1106。声明版本低于 1.2 却用了任一条目即报；规范对读端拒收
//!   沉默，「照常读取、只报不符」属库政策（`PdfFeature` 无此特性项，不在本
//!   切片写集，阈值直取 [`PdfVersion::V1_2`]）。
//! - `EncryptedExternalData`：7.6.1「…refers to an external file, the
//!   stream's contents **shall not be encrypted**」raw L2743–L2745；`/F` 在
//!   且 `/FFilter` 含 `Crypt`（Table 6 L1246，PDF 1.5；`canonical_filter` 的
//!   缩写表无 `Crypt`，故按全名判定）→ shall 级。
//! - `InlineBytesIgnored`／`LengthMismatch`：Table 5 `/F`「the bytes between
//!   `stream` and `endstream` **shall be ignored**. However, the `Length`
//!   entry **should** still specify the number of those bytes…」raw
//!   L1096–L1099。前者提示级（该 shall 约束的是读端必须忽略，不是文件违例），
//!   后者 should 级。
//!
//! 内联字节数＝[`StreamData`] 记录长度 + 记录范围之后、`endstream` 之前未
//! 计入的字节（先扣 L1073–L1075 允许的那个未计入 EOL）。解析器直接把 `/Length`
//! 当记录长度（`Lexer::compute_stream_range`），报大即解析期失败，故此处只
//! 可能捕捉「报小」；无源文件或窗口内看不到 `endstream` 时按记录长度处理。

use std::fmt;

use crate::core::reader::content::text_value;
use crate::core::reader::embedded::Filespec;
use crate::core::reader::navigation::resolve;
use crate::core::types::{Dict, Document, PdfVersion, StreamData, Value};

#[cfg(test)]
mod tests;

mod problems;
pub use problems::ExternalFileStreamProblem;
use problems::{ENDSTREAM_KEYWORD, ENDSTREAM_PROBE_WINDOW, PDF_12_ENTRIES};

/// 外部文件流读端错误。
#[derive(Debug, Clone, PartialEq)]
pub enum ExternalFileStreamError {
    /// Table 5 外部文件条目形态非法：类型错、个数不匹配、缺 `/F` 等。
    Malformed(String),
    /// `/F` 文件规格无法解析，或按 Table 44 没有任何文件名字条目。
    InvalidFilespec(String),
}

impl fmt::Display for ExternalFileStreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(message) => write!(f, "{message}"),
            Self::InvalidFilespec(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ExternalFileStreamError {}

type Result<T> = std::result::Result<T, ExternalFileStreamError>;

fn malformed(message: impl Into<String>) -> ExternalFileStreamError {
    ExternalFileStreamError::Malformed(message.into())
}

fn invalid_filespec(message: impl Into<String>) -> ExternalFileStreamError {
    ExternalFileStreamError::InvalidFilespec(message.into())
}

/// 外部文件流的类型化读端视图（Table 5 的 `/F`、`/FFilter`、`/FDecodeParms`）。
#[derive(Debug, Clone, PartialEq)]
pub struct ExternalFileStream {
    /// 流对象的间接引用；直接嵌入时为 `None`。
    pub reference: Option<(i64, i64)>,
    /// `/F` 文件规格（7.11.3 Table 44；字符串形态按只含 `/F` 的规格建模）。
    pub file: Filespec,
    /// `/FFilter` 规范化后的过滤器名（引用形态已解引用；缺省为空列表）。
    pub filters: Vec<String>,
    /// 与 [`Self::filters`] 下标对齐的参数字典；`None` 表示该过滤器无参数
    /// （条目为 `null` 或按 Table 5 省略）。`/FDecodeParms` 整体缺省时为
    /// 与过滤器等长的全 `None` 列表。
    pub decode_parms: Vec<Option<Dict>>,
    /// 规范化（解引用后）的完整流字典；未建模条目（`/Length`、`/Subtype`
    /// 等）经此访问。
    pub dict: Dict,
    /// 交叉核对结果（版本门／外部数据加密／内联字节与 `/Length`），空列表即
    /// 三项全一致；只报告，不改其余字段，也不影响错误判定。
    pub problems: Vec<ExternalFileStreamProblem>,
}

impl ExternalFileStream {
    /// 展示名：`/UF` 优先、回退 `/F`（7.11.3「conforming readers should
    /// use the F and UF (beginning with PDF 1.7) entries」，与
    /// [`Filespec::name`] 同口径）。
    pub fn display_name(&self) -> &str {
        self.file.name()
    }

    /// 外部文件数据是否声明了需要按顺序应用的过滤器。
    pub fn has_filters(&self) -> bool {
        !self.filters.is_empty()
    }

    /// 第 `index` 个过滤器的参数字典；越界返回 `None`。
    pub fn parms_for(&self, index: usize) -> Option<&Dict> {
        self.decode_parms.get(index).and_then(Option::as_ref)
    }

    /// 是否带有 [`Self::problems`] 诊断（不影响其余字段的可用性）。
    pub fn has_problems(&self) -> bool {
        !self.problems.is_empty()
    }
}

impl Document {
    /// 判定并解析外部文件流（Table 5）。
    ///
    /// - 值不是流，或流字典没有 `/F`／`/FFilter`／`/FDecodeParms` 任何一条
    ///   （`null` 值按 7.3.9 等同省略）→ `Ok(None)`：普通流，不产生诊断；
    /// - 出现任一条目但结构违例 → 类型化错误，绝不静默给默认值；
    /// - 结构合法但版本门／加密规则／内联字节三项交叉核对不符 → 正常返回
    ///   模型，不符之处记在 [`ExternalFileStream::problems`]。
    ///
    /// 期望传入已解析的流值（[`Document::get_object`] 的 `value`）或引用型
    /// 流字典所在的值；对象号入口用 [`Self::external_file_stream_object`]。
    pub fn external_file_stream(&self, value: &Value) -> Result<Option<ExternalFileStream>> {
        let Value::Stream(dict, data) = value else {
            return Ok(None);
        };
        if !has_external_entries(dict) {
            return Ok(None);
        }
        // 与 decode_stream_bytes／图像读端同一规范化器解引用 /FFilter、
        // /FDecodeParms（值与数组元素均可为间接引用，7.3.10）。
        let resolver = |item: &Value| -> std::result::Result<Option<Value>, String> {
            let Value::Ref(num, _) = item else {
                return Err(format!(
                    "expected an indirect reference, got {}",
                    item.type_name()
                ));
            };
            // 按对象号取最新修订，忽略引用中的代号（与 document 侧同口径）。
            self.get_object(*num)
                .map(|object| object.map(|object| object.value))
                .map_err(|error| error.to_string())
        };
        let (normalized, issues) = crate::core::filters::normalize_filter_entries(dict, &resolver);
        // 只把 Table 5 外部文件条目自身的诊断并入类型化错误；`/Filter`
        // 方向的诊断留给解码路径，避免无关噪声。
        let file = self.external_stream_file(&normalized)?;
        let filters = extract_filters(&normalized, &issues)?;
        let decode_parms = extract_parms(&normalized, &filters, &issues)?;
        let problems = collect_problems(self, dict, &filters, data);
        Ok(Some(ExternalFileStream {
            reference: value.as_ref(),
            file,
            filters,
            decode_parms,
            dict: normalized,
            problems,
        }))
    }

    /// 按对象号取外部文件流视图；对象缺失或不是外部文件流均为 `Ok(None)`。
    ///
    /// 与流值入口的差别仅在 [`ExternalFileStream::reference`]：这里总能给出
    /// `(对象号, 代号)`，与 [`Document::filespec`] 对文件规格登记引用的口径
    /// 一致。
    pub fn external_file_stream_object(&self, num: i64) -> Result<Option<ExternalFileStream>> {
        let object = self
            .get_object(num)
            .map_err(|error| malformed(format!("cannot load object {num}: {error}")))?;
        let Some(object) = object else {
            return Ok(None);
        };
        let mut model = self.external_file_stream(&object.value)?;
        if let Some(model) = &mut model
            && model.reference.is_none()
        {
            model.reference = Some((object.num, object.generation));
        }
        Ok(model)
    }

    /// `/F` 文件规格（7.11.2–7.11.3）：字典/引用形态委托
    /// [`Document::filespec`]，字符串形态按只含文件名的规格建模。
    fn external_stream_file(&self, dict: &Dict) -> Result<Filespec> {
        let value = match dict.get("F") {
            None | Some(Value::Null) => {
                return Err(malformed(
                    "/FFilter or /FDecodeParms present without /F (Table 5: the file containing \
                     the stream data is specified by /F)",
                ));
            }
            Some(value) => value,
        };
        let resolved = resolve(self, value).map_err(|error| invalid_filespec(error.to_string()))?;
        let file = match &resolved {
            Value::Dict(_) => self
                .filespec(value)
                .map_err(|error| invalid_filespec(error.to_string()))?,
            Value::String(_) | Value::HexString(_) => Filespec {
                reference: value.as_ref(),
                short_name: text_value(&resolved),
                ..Filespec::default()
            },
            other => {
                return Err(invalid_filespec(format!(
                    "/F of an external file stream is {} (7.11.2–7.11.3: file specification \
                     string or dictionary)",
                    other.type_name()
                )));
            }
        };
        // Table 44：/F required if DOS/Mac/Unix all absent；全部名字条目
        // 皆缺时没有任何可指认的外部文件。
        if file.short_name.is_none()
            && file.unicode_name.is_none()
            && file.dos_name.is_none()
            && file.mac_name.is_none()
            && file.unix_name.is_none()
        {
            return Err(invalid_filespec(
                "the /F file specification names no file (Table 44: /F is required unless /DOS, \
                 /Mac or /Unix is present)",
            ));
        }
        Ok(file)
    }
}

/// 是否声称是外部文件流：`/F`／`/FFilter`／`/FDecodeParms` 任一存在且非
/// `null`（7.3.9：null 作字典条目值等同省略该条目）。
fn has_external_entries(dict: &Dict) -> bool {
    PDF_12_ENTRIES.iter().any(|key| entry_present(dict, key))
}

/// 条目存在且非 `null`（7.3.9 的「null 作字典条目值等同省略该条目」）。
fn entry_present(dict: &Dict, key: &str) -> bool {
    matches!(dict.get(key), Some(value) if !matches!(value, Value::Null))
}

/// `/FFilter` → 过滤器名列表；Table 5「The same rules apply as for
/// `/Filter`」的形态约束（Name 或 Name 数组，零、一、多个皆可）。
fn extract_filters(dict: &Dict, issues: &[String]) -> Result<Vec<String>> {
    let Some(value) = dict.get("FFilter") else {
        return Ok(Vec::new());
    };
    match value {
        // null 等同省略（7.3.9）。
        Value::Null => Ok(Vec::new()),
        Value::Name(name) => Ok(vec![name.clone()]),
        Value::Array(array) => array
            .0
            .iter()
            .map(|item| match item {
                Value::Name(name) => Ok(name.clone()),
                other => Err(malformed(format!(
                    "/FFilter array has a non-name element ({}) (Table 5: array of filter \
                     names){}",
                    other.type_name(),
                    issue_suffix(issues, "FFilter"),
                ))),
            })
            .collect(),
        other => Err(malformed(format!(
            "/FFilter is {} (Table 5: name or array of names, the same rules as for /Filter){}",
            other.type_name(),
            issue_suffix(issues, "FFilter"),
        ))),
    }
}

/// `/FDecodeParms` → 与过滤器下标对齐的参数字典列表；
/// Table 5「one entry for each filter」的 shall 个数约束在此类型化。
fn extract_parms(dict: &Dict, filters: &[String], issues: &[String]) -> Result<Vec<Option<Dict>>> {
    let Some(value) = dict.get("FDecodeParms") else {
        return Ok(vec![None; filters.len()]);
    };
    match value {
        Value::Null => Ok(vec![None; filters.len()]),
        Value::Dict(params) => {
            if filters.is_empty() {
                return Err(malformed(
                    "/FDecodeParms present without /FFilter (Table 5: the parameters are used \
                     by the filters specified by /FFilter)",
                ));
            }
            if filters.len() > 1 {
                return Err(malformed(format!(
                    "/FDecodeParms is a single dictionary but /FFilter applies {} filters \
                     (Table 5: DecodeParms shall be an array with one entry for each filter){}",
                    filters.len(),
                    issue_suffix(issues, "FDecodeParms"),
                )));
            }
            Ok(vec![Some(params.clone())])
        }
        Value::Array(array) => {
            if array.len() != filters.len() {
                return Err(malformed(format!(
                    "/FDecodeParms array has {} entries but /FFilter has {} (Table 5: one entry \
                     per filter, the null object where a filter has no parameters){}",
                    array.len(),
                    filters.len(),
                    issue_suffix(issues, "FDecodeParms"),
                )));
            }
            array
                .0
                .iter()
                .map(|item| match item {
                    Value::Dict(params) => Ok(Some(params.clone())),
                    Value::Null => Ok(None),
                    other => Err(malformed(format!(
                        "/FDecodeParms array element is {} (Table 5: dictionary or null){}",
                        other.type_name(),
                        issue_suffix(issues, "FDecodeParms"),
                    ))),
                })
                .collect()
        }
        other => Err(malformed(format!(
            "/FDecodeParms is {} (Table 5: dictionary or array of dictionaries, the same rules \
             as for DecodeParms){}",
            other.type_name(),
            issue_suffix(issues, "FDecodeParms"),
        ))),
    }
}

/// 规范化器针对该键产出的解引用诊断，并入类型化错误消息尾注。
fn issue_suffix(issues: &[String], key: &str) -> String {
    let relevant: Vec<&str> = issues
        .iter()
        .filter(|issue| issue.contains(key))
        .map(String::as_str)
        .collect();
    if relevant.is_empty() {
        String::new()
    } else {
        format!(
            " (filter-entry normalization: {}; original value kept)",
            relevant.join("; ")
        )
    }
}

/// 三项交叉核对：只记诊断，不改取值、不报错。
fn collect_problems(
    document: &Document,
    dict: &Dict,
    filters: &[String],
    data: &StreamData,
) -> Vec<ExternalFileStreamProblem> {
    let mut problems = Vec::new();
    let declared_version = document.source_version();
    if declared_version < PdfVersion::V1_2 {
        let entries: Vec<&'static str> = PDF_12_ENTRIES
            .into_iter()
            .filter(|key| entry_present(dict, key))
            .collect();
        if !entries.is_empty() {
            problems.push(ExternalFileStreamProblem::VersionGate {
                entries,
                declared: declared_version,
            });
        }
    }
    // `/F` 在此必然存在：缺 `/F`／`/F null` 已由 `external_stream_file` 返回
    // 硬错误，故加密与内联字节两条规则（都以「数据真在外部文件」为前提）
    // 不需要再判一次。
    if let Some(filter) = filters.iter().find(|name| name.as_str() == "Crypt") {
        problems.push(ExternalFileStreamProblem::EncryptedExternalData {
            filter: filter.clone(),
        });
    }
    let actual = observed_inline_bytes(document, data);
    if actual > 0 {
        problems.push(ExternalFileStreamProblem::InlineBytesIgnored { bytes: actual });
    }
    if let Some(declared) = declared_stream_length(document, dict)
        && declared as u64 != actual
    {
        problems.push(ExternalFileStreamProblem::LengthMismatch { declared, actual });
    }
    problems
}

/// 观察到的 `stream`…`endstream` 字节数：[`StreamData`] 记录长度，加上记录
/// 范围后仍未计入的字节（扣掉 L1073–L1075 允许的那个未计入 EOL）；无源文件
/// 或窗口内看不到 `endstream` 时只取记录长度。
fn observed_inline_bytes(document: &Document, data: &StreamData) -> u64 {
    let (offset, len) = match data {
        StreamData::Owned(bytes) => return bytes.len() as u64,
        StreamData::Ref { offset, len } | StreamData::EncryptedRef { offset, len, .. } => {
            (*offset, *len)
        }
    };
    let Some(window) = document.read_at(offset.saturating_add(len), ENDSTREAM_PROBE_WINDOW) else {
        return len;
    };
    // 解析器只允许空白跨过记录范围与关键字，故这里量到的正是 `/Length` 少报的段。
    let Some(position) = window
        .windows(ENDSTREAM_KEYWORD.len())
        .position(|chunk| chunk == ENDSTREAM_KEYWORD)
    else {
        return len;
    };
    let tail = &window[..position];
    let trimmed = tail
        .strip_suffix(b"\r\n" as &[u8])
        .or_else(|| tail.strip_suffix(b"\n" as &[u8]))
        .or_else(|| tail.strip_suffix(b"\r" as &[u8]))
        .unwrap_or(tail);
    len + trimmed.len() as u64
}

/// `/Length`（Table 5 L1071，Required）声明的字节数：间接引用按 7.3.10 解一层；
/// 缺失、非整数或负值一律 `None`（`/Length` 的形态本身由解析器判定）。
fn declared_stream_length(document: &Document, dict: &Dict) -> Option<i64> {
    let value = match dict.get("Length") {
        Some(Value::Ref(num, _)) => document.get_object(*num).ok().flatten()?.value,
        Some(value) => value.clone(),
        None => return None,
    };
    value.as_integer().filter(|length| *length >= 0)
}
