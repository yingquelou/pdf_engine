//! `FlateDecode`：zlib/DEFLATE 解码，第三方实现经 `InflateBackend` 隔离。

use std::io::Read;

use crate::pdf::types::Dict;

use super::predictor::apply_predictor;
use super::{DecodeError, Result, StreamFilter};

/// `FlateDecode`：zlib/DEFLATE 解码，随后应用 Predictor。
pub(super) struct FlateFilter;

impl StreamFilter for FlateFilter {
    fn name(&self) -> &'static str {
        "FlateDecode"
    }

    fn decode(&self, data: &[u8], params: Option<&Dict>) -> Result<Vec<u8>> {
        let decoded = flate_decode(data)?;
        apply_predictor(decoded, params)
    }
}

/// zlib/DEFLATE 解压后端（第三方实现的隔离层）。
///
/// PDF 只规定 `FlateDecode` 的字节格式（RFC 1950 zlib），不规定实现。
/// `FlateFilter` 只依赖本 trait，不直接依赖具体压缩库；默认实现
/// [`Flate2Backend`] 把 `flate2` 封装在唯一适配器中，后续可替换为其他
/// 纯 Rust/平台后端而不影响过滤器调度与 PDF 语义。
trait InflateBackend {
    /// 解压 zlib 数据；失败返回人类可读原因。
    fn inflate(&self, data: &[u8]) -> std::result::Result<Vec<u8>, String>;
}

/// 默认 DEFLATE 后端：`flate2`（纯 Rust miniz_oxide）。
struct Flate2Backend;

impl InflateBackend for Flate2Backend {
    fn inflate(&self, data: &[u8]) -> std::result::Result<Vec<u8>, String> {
        let mut out = Vec::new();
        let mut dec = flate2::read::ZlibDecoder::new(data);
        dec.read_to_end(&mut out)
            .map_err(|e| format!("FlateDecode failed: {e}"))?;
        Ok(out)
    }
}

/// `FlateDecode`：zlib（RFC 1950，含 zlib 头 + DEFLATE + Adler-32）解码。
/// 数据损坏 / CRC 不符 → Err。
fn flate_decode(data: &[u8]) -> Result<Vec<u8>> {
    Flate2Backend.inflate(data).map_err(DecodeError::new)
}

#[cfg(test)]
mod tests {
    use crate::pdf::types::{Dict, Value};

    use super::super::decode_stream;

    /// 损坏的 flate 数据返回 Err（不 panic）。
    #[test]
    fn corrupt_flate_errors() {
        let d = Dict::new().insert("Filter", Value::Name("FlateDecode".into()));
        let e = decode_stream(&d, b"not-a-zlib-stream").unwrap_err();
        assert!(e.to_string().contains("FlateDecode"), "got: {e}");
    }
}
