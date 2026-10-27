//! 写端流压缩（D4：`FlateDecode` 编码路径）。
//!
//! 与读端 `filters::flate::InflateBackend` 对称，压缩实现经
//! [`DeflateBackend`] trait 隔离，默认 [`Flate2DeflateBackend`]（纯 Rust
//! miniz_oxide）始终可用。压缩在序列化咽喉点（`write_object`）应用，
//! 内存中的 `StreamData::Owned` **恒为明文**——「宽进严出」不变量不受影响。
//!
//! **失败单一契约**：调用方注入的后端失败时报类型化
//! [`WriteError::Compression`]；[`DocumentWriter`](super::DocumentWriter)
//! 先缓冲全部字节、成功后才写出，因此失败时输出端**零字节**，
//! 绝不静默回退明文。

use std::fmt;
use std::io::Write as _;
use std::sync::Arc;

use crate::core::types::{Dict, Value};

use super::WriteError;

/// DEFLATE 压缩后端（第三方实现的隔离层）。
///
/// PDF 只规定 `FlateDecode` 的字节格式（RFC 1950 zlib），不规定实现。
/// 写端只依赖本 trait；默认实现 [`Flate2DeflateBackend`] 把 `flate2`
/// 封装在唯一适配器中，可替换为其他纯 Rust/平台后端而不影响写出流程。
pub trait DeflateBackend: Send + Sync {
    /// 把数据压缩为 zlib（RFC 1950）字节串；失败返回人类可读原因。
    fn deflate(&self, data: &[u8]) -> Result<Vec<u8>, String>;
}

/// 默认压缩后端：`flate2`（纯 Rust miniz_oxide），始终可用。
#[derive(Debug, Clone, Copy)]
pub struct Flate2DeflateBackend;

impl DeflateBackend for Flate2DeflateBackend {
    fn deflate(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(data)
            .map_err(|e| format!("FlateEncode failed: {e}"))?;
        enc.finish().map_err(|e| format!("FlateEncode failed: {e}"))
    }
}

/// 图像预测器选择（ISO 32000-1 7.4.4.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImagePredictor {
    /// 不使用预测器（默认）：纯 Flate。
    None,
    /// PNG Up（每行前置过滤字节 `2`，即 Predictor 15）：对 8-bit 设备色
    /// 图像样本做逐行差分。要求目标版本 ≥1.3。
    PngUp,
}

/// 写端流压缩策略。
///
/// 生效范围：序列化时所有**未声明 `/Filter`** 的流（Owned 明文流；含页面
/// 内容流、图像、字体程序与 writer 生成的对象流/XRef 流）。已声明过滤器的
/// 流（如 `DCTDecode`/`JPXDecode` 透传流）原样保留，不二次编码。
/// 空流不压缩。
pub struct StreamCompression {
    backend: Arc<dyn DeflateBackend>,
    predictor: ImagePredictor,
}

impl Clone for StreamCompression {
    fn clone(&self) -> Self {
        Self {
            backend: Arc::clone(&self.backend),
            predictor: self.predictor,
        }
    }
}

impl fmt::Debug for StreamCompression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamCompression")
            .field("predictor", &self.predictor)
            .field("backend", &"<dyn DeflateBackend>")
            .finish()
    }
}

impl StreamCompression {
    /// 默认 FlateDecode 压缩：`flate2` 后端、无预测器。
    pub fn flate() -> Self {
        Self {
            backend: Arc::new(Flate2DeflateBackend),
            predictor: ImagePredictor::None,
        }
    }

    /// 使用调用方注入的压缩后端（替代默认 `flate2`）。
    pub fn with_backend(backend: Arc<dyn DeflateBackend>) -> Self {
        Self {
            backend,
            predictor: ImagePredictor::None,
        }
    }

    /// 设定图像预测器。
    pub fn with_image_predictor(mut self, predictor: ImagePredictor) -> Self {
        self.predictor = predictor;
        self
    }

    /// 是否用到了需要更高版本门控的选项（PNG 预测器，PDF 1.3+）。
    pub(crate) fn uses_image_predictor(&self) -> bool {
        self.predictor == ImagePredictor::PngUp
    }

    /// 把「未声明 Filter 的明文流」压缩为（新字典，压缩字节）。
    ///
    /// 返回 `Ok(None)` 表示保持原样（已有 `/Filter` 或 `/F`、空字节、
    /// 或压缩后反而更大）。后端失败报 [`WriteError::Compression`]。
    pub(crate) fn apply(
        &self,
        dict: &Dict,
        plain: &[u8],
    ) -> Result<Option<(Dict, Vec<u8>)>, WriteError> {
        if plain.is_empty() {
            return Ok(None);
        }
        if dict.get("Filter").or_else(|| dict.get("F")).is_some() {
            // 已声明过滤器（JPX/DCT/BZip2 等透传流）：不二次编码。
            return Ok(None);
        }
        let mut d = dict.clone();
        let payload: Vec<u8>;
        let mut source = plain;
        if let Some((parms, filtered)) = self.predictor_image_parms(dict, plain) {
            d.0.insert("DecodeParms".into(), Value::Dict(parms));
            payload = filtered;
            source = &payload;
        }
        let compressed = self
            .backend
            .deflate(source)
            .map_err(|e| WriteError::Compression(format!("{e} (object dict: {d:?})")))?;
        if compressed.len() >= source.len() {
            // 压缩不减体积（随机数据）：保持明文，避免无收益膨胀。
            return Ok(None);
        }
        d.0.insert("Filter".into(), Value::Name("FlateDecode".into()));
        d.0.insert("Length".into(), Value::Integer(compressed.len() as i64));
        Ok(Some((d, compressed)))
    }

    /// 图像预测器前置：`/Subtype /Image` 且几何可判定（8-bit、设备色
    /// 命名空间、样本长度与行几何一致）时做 PNG Up 逐行差分（每行前缀
    /// 过滤字节 2），并给出与之配套的 `/DecodeParms`。
    fn predictor_image_parms(&self, dict: &Dict, plain: &[u8]) -> Option<(Dict, Vec<u8>)> {
        if self.predictor != ImagePredictor::PngUp {
            return None;
        }
        if dict.get("Subtype").and_then(Value::as_name) != Some("Image") {
            return None;
        }
        if dict
            .get("ImageMask")
            .is_some_and(|v| matches!(v, Value::Bool(true)))
        {
            return None;
        }
        if dict.get("BitsPerComponent").and_then(Value::as_integer) != Some(8) {
            return None;
        }
        let colors = match dict.get("ColorSpace").and_then(Value::as_name) {
            Some("DeviceGray") => 1usize,
            Some("DeviceRGB") => 3,
            Some("DeviceCMYK") => 4,
            _ => return None,
        };
        let width = dict.get("Width").and_then(Value::as_integer)?;
        if width <= 0 {
            return None;
        }
        let row_bytes = width as usize * colors;
        if !plain.len().is_multiple_of(row_bytes) {
            return None;
        }
        let parms = Dict::new()
            .insert("Predictor", Value::Integer(15))
            .insert("Colors", Value::Integer(colors as i64))
            .insert("BitsPerComponent", Value::Integer(8))
            .insert("Columns", Value::Integer(width));
        Some((parms, png_up_encode(plain, row_bytes)))
    }
}

/// PNG Up 过滤编码：每行前置过滤字节 `2`，行内 `enc[i] = raw[i] - prev[i]`
/// （首行 prev 视为全 0），按字节模 256 回绕。与读端 Predictor 15 解码
/// （`filters::predictor::png_predictor`）互逆。
fn png_up_encode(raw: &[u8], row_bytes: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() + raw.len() / row_bytes);
    let zero_row = vec![0u8; row_bytes];
    let mut prev: &[u8] = &zero_row;
    for cur in raw.chunks(row_bytes) {
        out.push(2u8);
        for (c, p) in cur.iter().zip(prev) {
            out.push(c.wrapping_sub(*p));
        }
        prev = cur;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::filters::decode_stream;
    use crate::core::writer::WriteError;

    fn dict() -> Dict {
        Dict::new()
    }

    fn image_dict(width: i64, colors: &str) -> Dict {
        Dict::new()
            .insert("Subtype", Value::Name("Image".into()))
            .insert("Width", Value::Integer(width))
            .insert("Height", Value::Integer(2))
            .insert("BitsPerComponent", Value::Integer(8))
            .insert("ColorSpace", Value::Name(colors.into()))
    }

    /// 默认后端压缩输出可被读端 `FlateDecode` 过滤器解回原字节。
    #[test]
    fn default_backend_output_decodes_through_reader_filter() {
        let plain: Vec<u8> = b"aaaaaaaaaa".repeat(40);
        let comp = StreamCompression::flate();
        let (d, bytes) = comp
            .apply(&dict(), &plain)
            .expect("apply")
            .expect("should compress");
        assert_eq!(
            d.get("Filter").and_then(Value::as_name),
            Some("FlateDecode")
        );
        assert_eq!(
            d.get("Length").and_then(Value::as_integer),
            Some(bytes.len() as i64)
        );
        assert_eq!(decode_stream(&d, &bytes).expect("decode"), plain);
    }

    /// 空流与已声明 `/Filter` 的流（如 JPX 透传）保持原样。
    #[test]
    fn empty_and_filtered_streams_are_left_untouched() {
        let comp = StreamCompression::flate();
        assert!(
            comp.apply(&dict(), b"").expect("apply").is_none(),
            "empty stream must stay plain"
        );
        let jpx = dict().insert("Filter", Value::Name("JPXDecode".into()));
        assert!(
            comp.apply(&jpx, b"abc").expect("apply").is_none(),
            "declared filter must not be recompressed"
        );
    }

    /// 压缩不减体积时保持明文（无收益不膨胀），且不残留 /DecodeParms。
    #[test]
    fn inflating_backend_result_keeps_stream_plain() {
        struct AlwaysBigger;
        impl DeflateBackend for AlwaysBigger {
            fn deflate(&self, data: &[u8]) -> Result<Vec<u8>, String> {
                let mut out = data.to_vec();
                out.push(0);
                Ok(out)
            }
        }
        let comp = StreamCompression::with_backend(Arc::new(AlwaysBigger))
            .with_image_predictor(ImagePredictor::PngUp);
        // 图像几何满足预测器条件，但压缩不减体积 → 整体保持明文。
        let samples: Vec<u8> = (0..12).collect();
        assert!(
            comp.apply(&image_dict(2, "DeviceRGB"), &samples)
                .expect("apply")
                .is_none(),
            "non-shrinking compression must keep the stream plain"
        );
    }

    /// PNG Up（Predictor 15）图像预测器编码可被读端过滤器解回原样本：
    /// 行间差分使平滑图像显著缩小。
    #[test]
    fn png_up_image_predictor_roundtrips_through_reader() {
        let width = 16i64;
        let row_bytes = width as usize * 3;
        let samples: Vec<u8> = (0..4u8)
            .flat_map(|row| std::iter::repeat_n(row.wrapping_mul(10), row_bytes))
            .collect();
        let comp = StreamCompression::flate().with_image_predictor(ImagePredictor::PngUp);
        let (d, bytes) = comp
            .apply(&image_dict(width, "DeviceRGB"), &samples)
            .expect("apply")
            .expect("should compress");
        let parms = d
            .get("DecodeParms")
            .and_then(Value::as_dict)
            .expect("parms");
        assert_eq!(parms.get("Predictor").and_then(Value::as_integer), Some(15));
        assert_eq!(parms.get("Colors").and_then(Value::as_integer), Some(3));
        assert_eq!(
            parms.get("Columns").and_then(Value::as_integer),
            Some(width)
        );
        assert_eq!(decode_stream(&d, &bytes).expect("decode"), samples);
    }

    /// 颜色空间不可判定（数组形式，如 Indexed）时跳过预测器、只做纯 Flate。
    #[test]
    fn predictor_skipped_when_colorspace_not_named_device() {
        let dict = Dict::new()
            .insert("Subtype", Value::Name("Image".into()))
            .insert("Width", Value::Integer(2))
            .insert("BitsPerComponent", Value::Integer(8))
            .insert(
                "ColorSpace",
                Value::Array(crate::core::types::Array(vec![
                    Value::Name("Indexed".into()),
                    Value::Integer(1),
                ])),
            );
        let plain: Vec<u8> = b"0123456789".repeat(40);
        let comp = StreamCompression::flate().with_image_predictor(ImagePredictor::PngUp);
        let (d, bytes) = comp.apply(&dict, &plain).expect("apply").expect("flate");
        assert!(d.get("DecodeParms").is_none(), "no predictor for Indexed");
        assert_eq!(decode_stream(&d, &bytes).expect("decode"), plain);
    }

    /// 注入后端失败时报类型化 `WriteError::Compression`。
    #[test]
    fn failing_backend_reports_compression_write_error() {
        struct Broken;
        impl DeflateBackend for Broken {
            fn deflate(&self, _data: &[u8]) -> Result<Vec<u8>, String> {
                Err("backend exploded".into())
            }
        }
        let comp = StreamCompression::with_backend(Arc::new(Broken));
        let error = comp
            .apply(&dict(), b"hello hello hello")
            .expect_err("backend failure must surface");
        assert!(
            matches!(&error, WriteError::Compression(_)),
            "expected Compression: {error}"
        );
        assert!(error.to_string().contains("backend exploded"), "{error}");
    }
}
