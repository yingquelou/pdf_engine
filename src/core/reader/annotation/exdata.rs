//! 3D 标记注解的外部数据字典 `/ExData`（13.6.6，Table 170/313，PDF 1.7）。
//!
//! `/ExData` 是标记类注解的附加条目（Table 170），值为外部数据字典
//! （Table 313）。`/Subtype /Markup3D` 时经 `/3DA` 指向一条 3D 注解
//! （间接引用，或按 `/NM` 命名的文本串）、`/3DV` 给出 3D 视图字典、
//! 可选 `/MD5` 是 3D 流数据的 16 字节摘要。`/3DV` 复用 13.6.4 的视图
//! 解析器（[`crate::core::reader::three_d`]）。
//!
//! `/Type` 在 Table 313 中规定为 `ExData`（可选），但 13.6.6 的示例写作
//! `/Type /Markup3D`；读端两种都接受，写端按表写出 `ExData`。未识别的
//! `/Subtype` 名字按仓库惯例保留为 [`ExDataSubtype::Other`]，不阻塞注解
//! 解析。

use crate::core::reader::navigation::resolve;
use crate::core::reader::three_d::{ThreeDError, ThreeDView, parse_view};
use crate::core::types::{Dict, Document, Value};

use super::{AnnotationError, Result, malformed};

/// 外部数据字典的 `/Subtype`（Table 313）：当前只定义了 `Markup3D`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExDataSubtype {
    /// 3D 标记注解（`/Subtype /Markup3D`）。
    Markup3D,
    /// 未识别的子类型名（原样保留）。
    Other(String),
}

impl ExDataSubtype {
    /// 由 `/Subtype` 名字解析；未知名返回 [`ExDataSubtype::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "Markup3D" => Self::Markup3D,
            other => Self::Other(other.to_string()),
        }
    }

    /// 标准名称；[`ExDataSubtype::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::Markup3D => "Markup3D",
            Self::Other(other) => other,
        }
    }
}

/// `/3DA` 的目标：3D 注解字典的间接引用，或命名其 `/NM` 的文本串。
#[derive(Debug, Clone, PartialEq)]
pub enum Markup3DAnnotation {
    /// 目标 3D 注解的间接引用 `num gen R`。
    Reference { number: i64, generation: i64 },
    /// 文本串（字面串或十六进制串），按 3D 注解的 `/NM` 命名；保留原值
    /// 以便调用方沿用原编码。
    Name(Value),
}

/// 3D 标记注解载荷（Table 313 的 `/3DA` `/3DV` `/MD5`）。
#[derive(Debug, Clone, PartialEq)]
pub struct Markup3D {
    /// `/3DA` 目标 3D 注解。
    pub annotation: Markup3DAnnotation,
    /// `/3DV` 3D 视图字典（Table 304）。
    pub view: ThreeDView,
    /// `/MD5` 3D 流数据的 MD5 摘要（恰 16 字节）。
    pub md5: Option<[u8; 16]>,
}

/// 注解的外部数据字典 `/ExData`（Table 313）。
#[derive(Debug, Clone, PartialEq)]
pub struct ExternalData {
    /// `/Subtype`：目前只有 `Markup3D`，未知名保留原值。
    pub subtype: ExDataSubtype,
    /// `Markup3D` 子类型的类型化载荷；未识别子类型为 `None`。
    pub markup_3d: Option<Markup3D>,
    /// 完整外部数据字典，未建模条目经此访问。
    pub dict: Dict,
}

/// 解析注解 `/ExData`：缺失返回 `None`。
///
/// 值可为间接引用；必须解析为字典。`/Subtype` 必填且须为名字，
/// `/Type` 若存在须为 `ExData` 或 `Markup3D`（Table 313 与 13.6.6 示例
/// 不一致，两种都接受）。未识别的 `/Subtype` 只保留原名与原始字典。
pub(super) fn parse_external_data(
    document: &Document,
    value: Option<&Value>,
    context: &str,
) -> Result<Option<ExternalData>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let resolved =
        resolve(document, value).map_err(|error| AnnotationError::Malformed(error.to_string()))?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| malformed(format!("{context} is not a dictionary")))?;

    match dict.get("Type") {
        None => {}
        // Table 313 写 `ExData`，13.6.6 的示例写 `Markup3D`；语义相同。
        Some(Value::Name(name)) if name == "ExData" || name == "Markup3D" => {}
        Some(Value::Name(other)) => {
            return Err(malformed(format!(
                "{context} /Type is {other}, expected ExData or Markup3D"
            )));
        }
        Some(_) => return Err(malformed(format!("{context} /Type must be a name"))),
    }

    let subtype = dict
        .get("Subtype")
        .ok_or_else(|| malformed(format!("{context} is missing required /Subtype")))?
        .as_name()
        .ok_or_else(|| malformed(format!("{context} /Subtype must be a name")))?;
    let subtype = ExDataSubtype::from_name(subtype);
    let markup_3d = match &subtype {
        ExDataSubtype::Markup3D => Some(parse_markup_3d(document, dict, context)?),
        ExDataSubtype::Other(_) => None,
    };
    Ok(Some(ExternalData {
        subtype,
        markup_3d,
        dict: dict.clone(),
    }))
}

/// 解析 `Markup3D` 载荷：`/3DA` 与 `/3DV` 必填，`/MD5` 可选。
fn parse_markup_3d(document: &Document, dict: &Dict, context: &str) -> Result<Markup3D> {
    // `/3DA` 不解引用：引用形式只记录目标对象号，文本串形式即注解名。
    let annotation = match dict.get("3DA") {
        None => {
            return Err(malformed(format!(
                "{context} /Subtype /Markup3D is missing required /3DA"
            )));
        }
        Some(Value::Ref(number, generation)) => Markup3DAnnotation::Reference {
            number: *number,
            generation: *generation,
        },
        Some(value @ (Value::String(_) | Value::HexString(_))) => {
            Markup3DAnnotation::Name(value.clone())
        }
        Some(other) => {
            return Err(malformed(format!(
                "{context} /3DA must be an indirect reference or a text string, found {}",
                other.type_name()
            )));
        }
    };

    let view_value = dict.get("3DV").ok_or_else(|| {
        malformed(format!(
            "{context} /Subtype /Markup3D is missing required /3DV"
        ))
    })?;
    let resolved = resolve(document, view_value)
        .map_err(|error| AnnotationError::Malformed(error.to_string()))?;
    let view_dict = resolved
        .as_dict()
        .ok_or_else(|| malformed(format!("{context} /3DV is not a 3D view dictionary")))?;
    let view = parse_view(document, view_dict, "markup 3D view").map_err(|error| match error {
        ThreeDError::Malformed(message) => malformed(format!("{context} /3DV: {message}")),
        ThreeDError::LimitExceeded(what) => AnnotationError::LimitExceeded(what),
    })?;

    let md5 = match dict.get("MD5") {
        None => None,
        Some(value) => Some(parse_md5(value, context)?),
    };

    Ok(Markup3D {
        annotation,
        view,
        md5,
    })
}

/// `/MD5`：字面串或十六进制串，解码后须恰为 16 字节。
fn parse_md5(value: &Value, context: &str) -> Result<[u8; 16]> {
    let bytes = byte_string(value).ok_or_else(|| {
        malformed(format!(
            "{context} /MD5 must be a literal or hexadecimal byte string"
        ))
    })?;
    let Ok(md5) = <[u8; 16]>::try_from(bytes.as_slice()) else {
        return Err(malformed(format!(
            "{context} /MD5 must decode to exactly 16 bytes, got {}",
            bytes.len()
        )));
    };
    Ok(md5)
}

/// 字面串/十六进制串解码为字节；其他值类型返回 `None`。
///
/// 十六进制串忽略 ASCII 空白，奇数位按 PDF 惯例在尾部补 `0`。
fn byte_string(value: &Value) -> Option<Vec<u8>> {
    match value {
        Value::String(text) => Some(text.as_bytes().to_vec()),
        Value::HexString(hex) => {
            let digits: Vec<u8> = hex
                .bytes()
                .filter(|byte| !byte.is_ascii_whitespace())
                .collect();
            let mut padded = digits.clone();
            if padded.len() % 2 == 1 {
                padded.push(b'0');
            }
            let mut out = Vec::with_capacity(padded.len() / 2);
            for pair in padded.chunks(2) {
                let high = (pair[0] as char).to_digit(16)?;
                let low = (pair[1] as char).to_digit(16)?;
                out.push((high * 16 + low) as u8);
            }
            Some(out)
        }
        _ => None,
    }
}
