//! `DCTDecode`：JPEG（ISO/IEC 10918）解码，第三方实现经 `JpegBackend` 隔离。

use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_jpeg::JpegDecoder;

use crate::pdf::types::Dict;

use super::{DecodeError, Result, StreamFilter};

/// `DCTDecode`：JPEG 解码，输出 PDF 语义的像素分量序列。
pub(super) struct DctFilter;

impl StreamFilter for DctFilter {
    fn name(&self) -> &'static str {
        "DCTDecode"
    }

    fn decode(&self, data: &[u8], _params: Option<&Dict>) -> Result<Vec<u8>> {
        // /DecodeParms 的 Colors/Columns/Rows（ISO 32000-1 表 10）仅服务于
        // 不完整流的恢复场景；几何信息以 JPEG 帧头为准，这里不使用。
        dct_decode(data)
    }
}

/// JPEG 流的分量布局约定（决定 PDF 语义层的逆变换方式）。
///
/// 由后端按 JPEG 帧头与 Adobe APP14 `transform` 值判定：
/// - 灰度 / YCbCr→RGB：数据本身即 PDF 语义字节（[`JpegLayout::Direct`]）；
/// - `transform = 0`：Adobe 惯例把 CMYK 按取反形式（`255 − v`）存储，
///   输出前需整体取反（[`JpegLayout::CmykInverted`]）；
/// - `transform = 2`：前三分量是 CMY 的 YCbCr 变换形式、K 分量原样存储，
///   需按 libjpeg `ycck_cmyk_convert` 的逆过程还原（[`JpegLayout::Ycck`]）。
enum JpegLayout {
    Direct,
    CmykInverted,
    Ycck,
}

/// PDF 语义的解码结果：像素按分量顺序紧密排列（w×h×分量数）。
struct JpegImage {
    layout: JpegLayout,
    data: Vec<u8>,
}

/// JPEG 解码后端（第三方实现的隔离层）。
///
/// PDF 只规定 `DCTDecode` 的语义（JPEG 字节流 → DeviceGray/DeviceRGB/
/// DeviceCMYK 像素），不规定实现。`DctFilter` 只依赖本 trait，不直接依赖
/// 具体解码库；默认实现 [`ZuneJpegBackend`] 把 `zune-jpeg` 封装在唯一
/// 适配器中，后续可替换为其他后端而不影响过滤器调度与 PDF 语义。错误
/// 统一为人类可读 `String`，不外泄任何第三方类型。
trait JpegBackend {
    /// 解码 JPEG 数据；失败返回人类可读原因。
    fn decode(&self, data: &[u8]) -> std::result::Result<JpegImage, String>;
}

/// 默认 JPEG 后端：`zune-jpeg`（纯 Rust，支持基线与渐进式）。
struct ZuneJpegBackend;

impl JpegBackend for ZuneJpegBackend {
    fn decode(&self, data: &[u8]) -> std::result::Result<JpegImage, String> {
        let mut dec = JpegDecoder::new(ZCursor::new(data));
        dec.decode_headers()
            .map_err(|e| format!("DCTDecode failed: {e}"))?;
        let input = dec
            .input_colorspace()
            .ok_or_else(|| "DCTDecode: no input colorspace after headers".to_string())?;
        // 输出色彩空间尽量与输入一致：zune 对同色彩空间 3/4 分量为恒等
        // 复制，CMYK/YCCK 的存储字节因此原样到达 PDF 语义层，取反约定
        // 不丢失；灰度/YCbCr 则交由后端转成 Luma/RGB。
        let out = match input {
            ColorSpace::Luma | ColorSpace::CMYK | ColorSpace::YCCK => input,
            ColorSpace::YCbCr | ColorSpace::RGB => ColorSpace::RGB,
            other => {
                return Err(format!("DCTDecode: unsupported input colorspace {other:?}"));
            }
        };
        let options = dec.options().jpeg_set_out_colorspace(out);
        dec.set_options(options);
        let data = dec.decode().map_err(|e| format!("DCTDecode failed: {e}"))?;
        let layout = match input {
            ColorSpace::CMYK => JpegLayout::CmykInverted,
            ColorSpace::YCCK => JpegLayout::Ycck,
            _ => JpegLayout::Direct,
        };
        Ok(JpegImage { layout, data })
    }
}

/// `DCTDecode`：JPEG 解码 → PDF 语义像素。
///
/// Adobe APP14 约定（ISO 32000-1 7.4.8 注 3）：CMYK JPEG（`transform = 0`）
/// 按 `255 − v` 存储，须整体取反还原 DeviceCMYK；YCCK（`transform = 2`）
/// 按 libjpeg `ycck_cmyk_convert` 的逆过程还原：前三分量 YCbCr→RGB 后取补
/// （C = 255 − R 等），K 分量原样。数据损坏 / 不完整 → Err，绝不 panic。
fn dct_decode(data: &[u8]) -> Result<Vec<u8>> {
    let img = ZuneJpegBackend.decode(data).map_err(DecodeError::new)?;
    match img.layout {
        JpegLayout::Direct => Ok(img.data),
        JpegLayout::CmykInverted => Ok(img.data.iter().map(|v| 255 - v).collect()),
        JpegLayout::Ycck => Ok(ycck_to_cmyk(&img.data)),
    }
}

/// YCCK → DeviceCMYK：前三分量 YCbCr→RGB 后取补（C = 255 − R 等），K 原样。
fn ycck_to_cmyk(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for px in data.chunks_exact(4) {
        let (r, g, b) = ycbcr_to_rgb(px[0], px[1], px[2]);
        out.extend_from_slice(&[255 - r, 255 - g, 255 - b, px[3]]);
    }
    out
}

/// JFIF 标准 YCbCr → RGB（定点整数实现，系数按 2^16 缩放，结果截取到
/// 0..=255）：R = Y + 1.402(Cr−128)、G = Y − 0.344(Cb−128) − 0.714(Cr−128)、
/// B = Y + 1.772(Cb−128)。
fn ycbcr_to_rgb(y: u8, cb: u8, cr: u8) -> (u8, u8, u8) {
    let y = i32::from(y);
    let cb_off = i32::from(cb) - 128;
    let cr_off = i32::from(cr) - 128;
    // 1.402·65536 ≈ 91881，0.344136·65536 ≈ 22554，
    // 0.714136·65536 ≈ 46802，1.772·65536 ≈ 116130
    let r = y + ((91881 * cr_off + 0x8000) >> 16);
    let g = y - ((22554 * cb_off + 46802 * cr_off + 0x8000) >> 16);
    let b = y + ((116130 * cb_off + 0x8000) >> 16);
    (
        r.clamp(0, 255) as u8,
        g.clamp(0, 255) as u8,
        b.clamp(0, 255) as u8,
    )
}

#[cfg(test)]
mod tests {
    use jpeg_encoder::{ColorType, Encoder, rgb_to_ycbcr};

    use super::super::decode_stream;
    use crate::pdf::types::{Dict, Value};

    const W: u16 = 8;
    const H: u16 = 8;
    /// 有损解码的逐字节容差。quality 95 下测试梯度（相邻像素差 13/104 的
    /// 锯齿，属高频最坏情形）的 DCT 量化实测上界约 17，加 YCbCr 定点舍入
    /// 取 24；语义性错误（通道颠倒 / 漏取补）的误差远大于此，仍可区分。
    const TOL: i32 = 24;

    /// 用 jpeg-encoder 把 `data` 编码为 JPEG 测试夹具（quality 95，无色度
    /// 下采样）。`quality = Some(100)` 用于近似无损对照实验。
    fn encode(data: &[u8], ct: ColorType, progressive: bool) -> Vec<u8> {
        encode_q(data, ct, progressive, 95)
    }

    fn encode_q(data: &[u8], ct: ColorType, progressive: bool, quality: u8) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf, quality);
        enc.set_progressive(progressive);
        enc.encode(data, W, H, ct).expect("fixture encode");
        buf
    }

    /// 8×8 渐变像素：各通道取值随像素序号与通道号变化（非均匀，避免退化）。
    fn gradient(channels: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(W as usize * H as usize * channels);
        for i in 0..W as usize * H as usize {
            for c in 0..channels {
                out.push(((i * 13 + c * 61 + 29) % 256) as u8);
            }
        }
        out
    }

    /// 逐字节近似比较（有损）。失败时报告超差字节的首个位置，便于定位。
    fn assert_close(got: &[u8], want: &[u8], ctx: &str) {
        assert_eq!(got.len(), want.len(), "{ctx}: length mismatch");
        let max = got
            .iter()
            .zip(want)
            .map(|(g, w)| (i32::from(*g) - i32::from(*w)).abs())
            .max()
            .unwrap_or(0);
        let first_excess: Vec<(usize, u8, u8)> = got
            .iter()
            .zip(want)
            .enumerate()
            .filter(|(_, (g, w))| (i32::from(**g) - i32::from(**w)).abs() > TOL)
            .take(4)
            .map(|(i, (g, w))| (i, *g, *w))
            .collect();
        assert!(
            max <= TOL,
            "{ctx}: max diff {max} > {TOL}; first excess: {first_excess:?}"
        );
    }

    fn dct_dict() -> Dict {
        Dict::new().insert("Filter", Value::Name("DCTDecode".into()))
    }

    /// DeviceGray（1 分量）。
    #[test]
    fn dct_gray_roundtrip() {
        let px = gradient(1);
        let out = decode_stream(&dct_dict(), &encode(&px, ColorType::Luma, false)).unwrap();
        assert_close(&out, &px, "gray");
    }

    /// DeviceRGB（3 分量，YCbCr 变换存储 → RGB 输出）。
    #[test]
    fn dct_rgb_roundtrip() {
        let px = gradient(3);
        let out = decode_stream(&dct_dict(), &encode(&px, ColorType::Rgb, false)).unwrap();
        assert_close(&out, &px, "rgb");
    }

    /// DeviceCMYK（Adobe transform=0：按 255−v 存取，解码后应还原）。
    #[test]
    fn dct_cmyk_roundtrip() {
        let px = gradient(4);
        let out = decode_stream(&dct_dict(), &encode(&px, ColorType::Cmyk, false)).unwrap();
        assert_close(&out, &px, "cmyk");
    }

    /// YCCK（Adobe transform=2）：夹具按 libjpeg 约定自构——前三分量取
    /// `rgb_to_ycbcr(255−C, 255−M, 255−Y)`（jpeg-encoder 的 Ycck 为直通
    /// 存储），K 原样；解码应还原出原 CMYK。
    #[test]
    fn dct_ycck_roundtrip() {
        let px = gradient(4);
        let mut ycck = Vec::with_capacity(px.len());
        for p in px.chunks_exact(4) {
            let (y, cb, cr) = rgb_to_ycbcr(255 - p[0], 255 - p[1], 255 - p[2]);
            ycck.extend_from_slice(&[y, cb, cr, p[3]]);
        }
        let out = decode_stream(&dct_dict(), &encode(&ycck, ColorType::Ycck, false)).unwrap();
        assert_close(&out, &px, "ycck");
    }

    /// 渐进式 JPEG（多扫描）同样可解。
    #[test]
    fn dct_progressive_roundtrip() {
        let px = gradient(3);
        let out = decode_stream(&dct_dict(), &encode(&px, ColorType::Rgb, true)).unwrap();
        assert_close(&out, &px, "progressive");
    }

    /// 诊断对照：quality 100 时 RGB 路径应近似无损。
    #[test]
    fn dct_rgb_quality100_roundtrip() {
        let px = gradient(3);
        let out = decode_stream(&dct_dict(), &encode_q(&px, ColorType::Rgb, false, 100)).unwrap();
        assert_close(&out, &px, "rgb-q100");
    }

    /// 损坏 / 截断 / 空数据 → Err（不 panic），错误信息指明 DCTDecode。
    #[test]
    fn dct_corrupt_errors_without_panic() {
        let real = encode(&gradient(3), ColorType::Rgb, false);
        for (case, data) in [
            ("garbage", &b"\xff\xd8junk-junk-junk"[..]),
            ("truncated", &real[..real.len() / 2]),
            ("empty", &b""[..]),
        ] {
            let e = decode_stream(&dct_dict(), data).unwrap_err();
            assert!(e.to_string().contains("DCTDecode"), "{case}: got: {e}");
        }
    }
}
