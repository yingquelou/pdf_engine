//! PDF 流过滤器（`/Filter` + `/DecodeParms`）解码。
//!
//! 本模块按职责拆分为目录模块：
//! - `ascii`：ASCIIHex / ASCII85 / RunLength；
//! - `bitmap`：位图后端（JBIG2/JPX 像素解码）输出展平为 `Vec<u8>` 的
//!   薄适配器（`decode_bitmap_stream` / `decode_bitmap_stream_with`）；
//! - `flate`：FlateDecode 与第三方压缩后端适配器；
//! - `indirect`：7.4.1 / Table 5 的 `/Filter`、`/DecodeParms` **间接引用形态**
//!   规范化（解引用 + shall 个数核对），由 `Document` 侧提供解引用器；
//! - `jpeg`：DCTDecode 与第三方 JPEG 后端适配器；
//! - `lzw`：LZWDecode；
//! - `predictor`：LZW/Flate 共用的 Predictor；
//! - 本文件：`StreamFilter` 接口、内建注册表、`/Filter` 调度与公共辅助。
//!
//! **可扩展的过滤器接口**：每个过滤器实现 [`StreamFilter`]，[`decode_stream`]
//! 只按规范名调度，因此新增过滤器或替换实现无需改动调度逻辑。依赖第三方
//! 库的过滤器只在自身实现内部经后端适配器调用（如 `FlateDecode` 的
//! `InflateBackend`），对外只暴露 PDF 语义，不暴露任何第三方类型。

mod ascii;
mod bitmap;
mod flate;
mod indirect;
mod jpeg;
mod lzw;
mod lzw_backend;
mod predictor;

pub(crate) use indirect::{ObjectResolver, normalize_filter_entries};

use crate::core::types::{Dict, Value};
use std::fmt;

pub use bitmap::{BitmapStreamContext, decode_bitmap_stream, decode_bitmap_stream_with};

use ascii::{Ascii85Filter, AsciiHexFilter, RunLengthFilter};
use flate::FlateFilter;
use jpeg::DctFilter;
use lzw::LzwFilter;

/// 流解码失败原因（无法解码的过滤器、数据损坏、参数非法等）。
///
/// 携带人类可读说明；调用方将其并入 `Document::warnings` 后优雅失败。
#[derive(Debug, Clone, PartialEq)]
pub struct DecodeError(pub String);

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for DecodeError {}

impl DecodeError {
    fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

type Result<T> = std::result::Result<T, DecodeError>;

/// PDF 流过滤器（ISO 32000-1 7.4）的解码接口。
///
/// 每个过滤器独立实现本 trait，[`decode_stream`] 按 `/Filter` 的规范名调度。
/// 这样新增过滤器（如后续的 JPX/JBIG2）或替换某个过滤器的实现时，无需改动
/// 调度逻辑；依赖第三方库的过滤器只在实现内部经后端适配器调用，trait 本身
/// 不暴露任何第三方类型。
///
/// `Send + Sync`：过滤器实现作为无状态单例注册（见 [`builtin_filters`]），
/// 可安全地被多个解码线程共享。
pub trait StreamFilter: Send + Sync {
    /// 规范过滤器名（如 `FlateDecode`），与 `/Filter` 全名一致。
    fn name(&self) -> &'static str;

    /// 解码 `data`。
    ///
    /// `params` 为该过滤器对应的 `/DecodeParms` 字典（多过滤器时按数组下标
    /// 一一对应；缺失为 `None`）。
    fn decode(&self, data: &[u8], params: Option<&Dict>) -> Result<Vec<u8>>;
}

/// `CCITTFaxDecode`：Group 3/4 传真解码。
///
/// 具体位流/码表实现位于 `crate::core::ccitt`；本实现只负责把
/// `/DecodeParms` 交给其参数解析并完成错误包装。未来若替换为第三方
/// 传真解码后端，可在此适配器内部替换，不影响 `StreamFilter` 调度。
struct CcittFilter;

impl StreamFilter for CcittFilter {
    fn name(&self) -> &'static str {
        "CCITTFaxDecode"
    }

    fn decode(&self, data: &[u8], params: Option<&Dict>) -> Result<Vec<u8>> {
        let parsed =
            crate::core::ccitt::CcittParams::from_dict(params).map_err(DecodeError::new)?;
        crate::core::ccitt::decode(data, &parsed).map_err(DecodeError::new)
    }
}

/// 内建的标准过滤器实例（无状态单例）。
static DEFAULT_FILTERS: [&'static dyn StreamFilter; 7] = [
    &FlateFilter,
    &LzwFilter,
    &CcittFilter,
    &DctFilter,
    &AsciiHexFilter,
    &Ascii85Filter,
    &RunLengthFilter,
];

/// 返回内建标准过滤器的静态集合。
///
/// 可直接传给 [`decode_stream_with`]；调用方也可 `to_vec()` 后追加自定义
/// [`StreamFilter`] 实现，在保留内建能力的同时扩展解码器——例如为 JPX /
/// JBIG2 等接入独立实现或第三方解码后端。
pub fn builtin_filters() -> &'static [&'static dyn StreamFilter] {
    &DEFAULT_FILTERS
}

/// 按流字典的 `/Filter` + `/DecodeParms` 解码流字节，返回解码后的数据。
///
/// 使用 [`builtin_filters`] 中的内建实现；需要扩展或覆盖实现时用
/// [`decode_stream_with`]。
///
/// - 无 `/Filter`：原样返回（克隆）；
/// - 多过滤器：按数组顺序**依次**解码，前一步的输出作为后一步的输入；
/// - 任一过滤器不支持或解码失败：返回 [`DecodeError`]。
///
/// 注意：`/Filter` 与 `/DecodeParms` 按 7.3.10 允许写成间接引用。本函数只有
/// 字典、没有 `Document` 上下文，无法解引用，故此类字典须先经
/// [`normalize_filter_entries`]（`Document::decode_stream_bytes` 已自动调用）
/// 规范化；残留的引用仍按「不支持」报错，不会静默返回未解码字节。
pub fn decode_stream(dict: &Dict, data: &[u8]) -> Result<Vec<u8>> {
    decode_stream_with(builtin_filters(), dict, data)
}

/// 用指定的过滤器集合解码流字节。
///
/// 集合中**第一个** `name` 匹配的实现生效，因此自定义实现可用于覆盖内建
/// 过滤器（把自定义实现放在 [`builtin_filters`] 之前即可）。`/Filter` 的
/// 缩写（`/Fl` `/LZW` 等）会先规范化为全名再查找。
pub fn decode_stream_with(
    filters: &[&dyn StreamFilter],
    dict: &Dict,
    data: &[u8],
) -> Result<Vec<u8>> {
    let names = filter_names(dict)?;
    if names.is_empty() {
        return Ok(data.to_vec());
    }
    let mut buf = data.to_vec();
    for (i, raw) in names.iter().enumerate() {
        let name = canonical_filter(raw);
        // 每个过滤器对应一个 /DecodeParms 字典（单字典只作用于首个过滤器）。
        let params = decode_params(dict, i);
        let filter = filters
            .iter()
            .copied()
            .find(|f| f.name() == name)
            .ok_or_else(|| {
                DecodeError::new(format!("unsupported filter /{name} (cannot decode)"))
            })?;
        buf = filter.decode(&buf, params.as_ref())?;
    }
    Ok(buf)
}

/// 按内建过滤器解码，但 `skip` 判为真的规范名步骤**原样透传**。
///
/// `pub(crate)`：图像读端（§7.4.9 JPXDecode）需要拿到 JPX 步骤之前的
/// 原始字节做语法层解析，而本 crate 按设计不解码 JPX 像素；其余步骤
/// （Flate 等）照常解码，被跳过步骤的 `/DecodeParms` 一并忽略。
/// 不改变 [`decode_stream`] 对任何现有调用方的行为。
pub(crate) fn decode_stream_except(
    dict: &Dict,
    data: &[u8],
    skip: &dyn Fn(&str) -> bool,
) -> Result<Vec<u8>> {
    let names = filter_names(dict)?;
    if names.is_empty() {
        return Ok(data.to_vec());
    }
    let mut buf = data.to_vec();
    for (i, raw) in names.iter().enumerate() {
        let name = canonical_filter(raw);
        if skip(name) {
            continue;
        }
        let params = decode_params(dict, i);
        let filter = DEFAULT_FILTERS
            .iter()
            .copied()
            .find(|f| f.name() == name)
            .ok_or_else(|| {
                DecodeError::new(format!("unsupported filter /{name} (cannot decode)"))
            })?;
        buf = filter.decode(&buf, params.as_ref())?;
    }
    Ok(buf)
}

/// 解析 `/Filter`：无 → 空表；单个 Name → 单元素；
/// Name 数组 → 多元素。其它类型（含间接引用）报错。
fn filter_names(dict: &Dict) -> Result<Vec<String>> {
    match dict.get("Filter") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Name(n)) => Ok(vec![n.clone()]),
        Some(Value::Array(a)) => {
            let mut v = Vec::with_capacity(a.len());
            for item in a.iter() {
                match item {
                    Value::Name(n) => v.push(n.clone()),
                    other => {
                        return Err(DecodeError::new(format!(
                            "unsupported /Filter array element: {}",
                            other.type_name()
                        )));
                    }
                }
            }
            Ok(v)
        }
        Some(other) => Err(DecodeError::new(format!(
            "unsupported /Filter value: {}",
            other.type_name()
        ))),
    }
}

/// 取第 `index` 个过滤器对应的 `/DecodeParms`（或缩写 `/DP`）字典。
///
/// 单字典只作用于首个过滤器（规范：多过滤器时 /DecodeParms 应为数组）；
/// 数组则按下标一一对应。缺失或非字典返回 `None`（即使用预测器默认值）。
///
/// `pub(crate)`：图像读端（§7.4.7 JBIG2Decode）也需按过滤器下标取
/// `/DecodeParms` 来解析 `/JBIG2Globals`，与解码调度共用同一套下标
/// 对齐规则（数组早位的 `null` 占位天然返回 `None`）。
pub(crate) fn decode_params(dict: &Dict, index: usize) -> Option<Dict> {
    match dict.get("DecodeParms").or_else(|| dict.get("DP")) {
        Some(Value::Dict(d)) => {
            if index == 0 {
                Some(d.clone())
            } else {
                None
            }
        }
        Some(Value::Array(a)) => a.iter().nth(index).and_then(|v| v.as_dict().cloned()),
        _ => None,
    }
}

/// 过滤器名缩写 → 规范全名。
///
/// `pub(crate)`：writer 需要据此判断「读端是否会解压该过滤器」
/// （见 `writer::prepare_stream_dict_for_write`），避免两端各维护一份
/// 过滤器清单。
pub(crate) fn canonical_filter(name: &str) -> &str {
    match name {
        "Fl" => "FlateDecode",
        "AHx" => "ASCIIHexDecode",
        "A85" => "ASCII85Decode",
        "RL" => "RunLengthDecode",
        "LZW" => "LZWDecode",
        "CCF" => "CCITTFaxDecode",
        "DCT" => "DCTDecode",
        other => other,
    }
}

#[cfg(test)]
mod tests;
