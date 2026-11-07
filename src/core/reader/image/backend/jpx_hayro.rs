//! JPEG 2000（JPX）像素解码后端：`hayro-jpeg2000` 的唯一适配点（D3 契约）。
//!
//! 本文件是 `hayro_jpeg2000` 类型在 `src/` 中唯一允许出现的位置（由
//! `scripts/deps_check.ps1` 的 D3 静态扫描核对，文件自身的测试纳入
//! `test_paths`）。对外只暴露项目自有类型：[`HayroJpxBackend`] 实现
//! [`JpxBackend`]，错误一律归一化为
//! [`crate::core::reader::image::error::ImageError`] 并带 `JPXDecode: `
//! 前缀，与其它过滤器同口径。
//!
//! # 输出约定
//!
//! - 样本恒为 **8 位交错**行优先（上游把 1/2/4/16 位源缩放到 8 位）；
//!   `components` 取颜色空间分量数，`bits_per_component` 恒为 8。
//! - 混合子采样（如 4:2:0，各分量 `XRsiz`/`YRsiz` 不一致）由上游按最近邻
//!   上采样到参考网格；**所有分量采样步长相同且 > 1** 时上游会直接把输出
//!   收缩到分量网格，适配层据 [`JpxDecodeRequest::header`] 的参考网格几何
//!   按整数因子复制像素补回全分辨率（否则返回尺寸与 PDF `/Width` 不一致）。
//! - 内嵌 ICC 剖面只取原始字节交给调用方的颜色管线（适配层不解释、不做
//!   颜色转换），见 [`DecodedBitmap::icc_profile`]；本 crate 因此不必启用
//!   上游的 `image`/`moxcms` 特性。
//! - `/Decode` 对 JPX 图像按 §7.4.9 忽略：适配层不反转样本，CMYK 等分量
//!   按码流原值输出。
//! - 码流含不透明（opacity）通道时：`/SMaskInData` 缺省或为 0 则按规范
//!   忽略该通道；非 0 时无法用 [`DecodedBitmap`] 表达软蒙版，返回
//!   `ImageError::Unsupported`，不静默丢数据。
//!
//! # 资源护栏
//!
//! 上游在 `decode()` 内按几何分配整幅缓冲；适配层在调用前按头部几何与
//! 通道数检查 [`MAX_DIMENSION`] 与 [`MAX_OUTPUT_BYTES`]，超限直接返回
//! `ImageError::Filter`（文案含 `exceeds`），避免畸形码流触发 OOM。

use super::super::error::{ImageError, Result};
use super::{DecodedBitmap, JpxBackend, JpxDecodeRequest};

/// 单边像素上限（JP2 几何为 u32，这里按 2^16 收紧）。
const MAX_DIMENSION: u32 = 1 << 16;

/// 解码输出字节上限，与 CCITT/JBIG2 适配层同一口径（256 MB）。
const MAX_OUTPUT_BYTES: usize = 256 * 1024 * 1024;

/// 基于 `hayro-jpeg2000` 的 JPX 解码后端（无状态，可共享）。
#[derive(Debug, Clone, Copy, Default)]
pub struct HayroJpxBackend;

impl JpxBackend for HayroJpxBackend {
    fn name(&self) -> &'static str {
        "hayro-jpeg2000"
    }

    fn decode(&self, request: &JpxDecodeRequest<'_>) -> Result<DecodedBitmap> {
        let settings = hayro_jpeg2000::DecodeSettings::default();
        let image = hayro_jpeg2000::Image::new(request.codestream, &settings)
            .map_err(|error| failure(format!("codestream structure: {error}")))?;

        let color_space = image.color_space();
        let components = color_space.num_channels();
        let has_alpha = image.has_alpha();
        if has_alpha && request.smask_in_data.unwrap_or(0) != 0 {
            return Err(ImageError::Unsupported(format!(
                "JPXDecode: /SMaskInData {} requires the opacity channel, which \
                 DecodedBitmap cannot represent",
                request.smask_in_data.unwrap_or(0)
            )));
        }
        let icc_profile = match color_space {
            hayro_jpeg2000::ColorSpace::Icc { profile, .. } => Some(profile.clone()),
            _ => None,
        };
        let channels = usize::from(components) + usize::from(has_alpha);
        let source_width = image.width();
        let source_height = image.height();
        // 上游在「全部分量步长相同且 > 1」时把输出收缩到分量网格；按
        // 头部参考网格几何做整数倍最近邻上采样，保证尺寸与 PDF 一致。
        let (width, height, step_x, step_y) = match request.header {
            Some(header) => (
                header.width,
                header.height,
                upsample_factor(header.width, source_width),
                upsample_factor(header.height, source_height),
            ),
            None => (source_width, source_height, 1, 1),
        };
        check_limits(width, height, channels)?;

        let decoded = image
            .decode()
            .map_err(|error| failure(format!("pixel data: {error}")))?;
        let expected = (source_width as usize)
            .saturating_mul(source_height as usize)
            .saturating_mul(channels);
        if decoded.len() != expected {
            return Err(failure(format!(
                "decoded {} bytes, expected {expected} for {source_width}x{source_height} \
                 x {channels} channel(s)",
                decoded.len()
            )));
        }
        let samples = if has_alpha || step_x > 1 || step_y > 1 {
            resample(&decoded, source_width, usize::from(components), channels, step_x, step_y)
        } else {
            decoded
        };

        Ok(DecodedBitmap {
            width,
            height,
            components,
            bits_per_component: 8,
            samples,
            icc_profile,
        })
    }
}

/// 参考网格边长为 `reference`、解码网格边长为 `decoded` 时，整数倍上采样
/// 因子；不构成整数倍（上游未收缩或几何异常）时按 1 处理。
fn upsample_factor(reference: u32, decoded: u32) -> u32 {
    if decoded == 0 || reference <= decoded || !reference.is_multiple_of(decoded) {
        return 1;
    }
    reference / decoded
}

/// 去不透明通道并按 `(step_x, step_y)` 做最近邻复制，输出全分辨率交错样本。
fn resample(
    decoded: &[u8],
    source_width: u32,
    components: usize,
    channels: usize,
    step_x: u32,
    step_y: u32,
) -> Vec<u8> {
    let source_width = source_width as usize;
    let mut out = Vec::with_capacity(
        decoded
            .len()
            .saturating_div(channels)
            .saturating_mul(components)
            .saturating_mul(step_x as usize)
            .saturating_mul(step_y as usize),
    );
    for row in decoded.chunks_exact(source_width * channels) {
        for _ in 0..step_y {
            for x in 0..source_width {
                let base = x * channels;
                let pixel = &row[base..base + components];
                for _ in 0..step_x {
                    out.extend_from_slice(pixel);
                }
            }
        }
    }
    out
}

fn failure(message: impl Into<String>) -> ImageError {
    ImageError::Filter(format!("JPXDecode: {}", message.into()))
}

fn check_limits(width: u32, height: u32, channels: usize) -> Result<()> {
    if width == 0 || height == 0 {
        return Err(failure(format!("empty image geometry {width}x{height}")));
    }
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(failure(format!(
            "image geometry {width}x{height} exceeds the {MAX_DIMENSION}-pixel dimension limit"
        )));
    }
    let bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(channels));
    match bytes {
        Some(bytes) if bytes <= MAX_OUTPUT_BYTES => Ok(()),
        _ => Err(failure(format!(
            "decoded size for {width}x{height} x {channels} channel(s) exceeds the \
             {MAX_OUTPUT_BYTES}-byte output limit"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reader::image::backend::decode_jpx_with;
    use crate::core::reader::image::error::ImageError;
    use crate::core::types::PdfVersion;

    /// 真实码流取证：以下五个码流均由 OpenJPEG 2.5.4 的 `opj_compress`
    /// 在本机生成（无损可逆 5/3），再把 JP2 容器里的 `jp2c` 载荷取出；
    /// 已用 `opj_dump` 核对几何/分量/精度。测试只嵌入码流本身，容器由
    /// [`jp2_file`] 按需合成。
    ///
    /// 8×4 灰度，样本 `v(x,y) = 8x + 8y`（每行逐样本递增 8）。
    const GRAY8: &str = concat!(
        "ff4fff5100290000000000080000000400000000000000000000000800000004",
        "00000000000000000001070101ff52000c00000001000004040001ff5c000440",
        "40ff640025000143726561746564206279204f70656e4a504547207665727369",
        "6f6e20322e352e34ff90000a0000000000240001ff93df8098121dd847ce86d5",
        "c70d2883d2d71d215557e88fffd9",
    );

    /// 8×4 RGB，分量 `R=8x`、`G=8y`、`B=64+32(x mod 4)`。
    const RGB8: &str = concat!(
        "ff4fff51002f0000000000080000000400000000000000000000000800000004",
        "00000000000000000003070101070101070101ff52000c000000010100040400",
        "01ff5c00044040ff640025000143726561746564206279204f70656e4a504547",
        "2076657273696f6e20322e352e34ff90000a0000000000590001ff93cfb46811",
        "50530c62db5d4fa3daa05c8cf28b5c5b926d3255dc1b0d88f3df80b01d30dad8",
        "3422c4d3080a950fab1f8154d92120005100c7d424277566d64eca144d6b38dd",
        "49d9a43802949fffd9",
    );

    /// 8×4 灰度、**16 位**，样本 `v(x,y) = 2000(8y + x)`（PGM 大端写入）。
    const GRAY16: &str = concat!(
        "ff4fff5100290000000000080000000400000000000000000000000800000004",
        "000000000000000000010f0101ff52000c00000001000004040001ff5c000440",
        "80ff640025000143726561746564206279204f70656e4a504547207665727369",
        "6f6e20322e352e34ff90000a0000000000430001ff93dff89188121fb122337d",
        "c745e99f350bc79e152fa235294ed1d2ef8e265c3708beddcabf2f8c22d886e1",
        "bb6035ec363f2f2f32965fffd9",
    );

    /// 8×4、4 分量、**16 位灰度源之外的另一种取证**：由 OpenJPEG 2.5.4
    /// 读入 32 位 RGBA PNG（GDI+ 保存后先剥掉 `sRGB`/`gAMA` 等会触发
    /// 读端伽马校正的辅助块），`-mct 0` 无损编码后取 `jp2c` 载荷；
    /// `opj_decompress -OutFor RAW` 逐平面核对得 `C=8x`、`M=8y`、
    /// `Y=128`、`K=255-8x`。同一码流既作 CMYK 用，也作 RGB+alpha 用
    /// （见 [`rgb_with_opacity`]）。
    const CMYK8: &str = concat!(
        "ff4fff5100320000000000080000000400000000000000000000000800000004",
        "00000000000000000004070101070101070101070101ff52000c000000010000",
        "04040001ff5c00044040ff640025000143726561746564206279204f70656e4a",
        "5045472076657273696f6e20322e352e34ff90000a0000000000540001ff93df",
        "80a011669096f641509512f700fd6d2f0e1b0d88f4dfdf80a8128cd0bdfd9347",
        "6b76654779cdbbd6b2824000a20180cfb44c14005ba10e20dbdd86cd075b2f2f",
        "2f6856ff7fffd9",
    );

    /// 4:2:0 的 YCbCr：参考网格 16×8；Y 全分辨率 `16x + y`，Cb/Cr 为
    /// `XRsiz=YRsiz=2` 的 8×4 平面（`8i + 8j` / `255 - 8i - 8j`）。
    const YCBCR420: &str = concat!(
        "ff4fff51002f0000000000100000000800000000000000000000001000000008",
        "00000000000000000003070101070202070202ff52000c000000010000040400",
        "01ff5c00044040ff640025000143726561746564206279204f70656e4a504547",
        "2076657273696f6e20322e352e34ff90000a0000000000a30001ff93df832012",
        "3352d4e6f9c1c76f998ccc8ff909a51e6c3f361349e81a14f35be8dd723c37ab",
        "4bab6371895d0ade96fdfb7ca611bc927d48038d705472b1dd93e239cd6bcc82",
        "902fb4c8d4a4ea208caa8f0a59555d31bbe671b5680003340a00000000000000",
        "000007df8098121dd847ce86d5c70d2883d2d71d215557e88fcfb4541400578e",
        "ed45a670d97a988db11bfeb3ab9646ff7fffd9",
    );

    /// 单分量、且**唯一分量的步长也是 2**：上游会把输出收缩到分量网格
    /// 8×4（参考网格 16×8），适配层需按头部补回 16×8。
    const UNIFORM_SUBSAMPLED: &str = concat!(
        "ff4fff5100290000000000100000000800000000000000000000001000000008",
        "00000000000000000001070202ff52000c00000001000004040001ff5c000440",
        "40ff640025000143726561746564206279204f70656e4a504547207665727369",
        "6f6e20322e352e34ff90000a0000000000240001ff93df8098121dd847ce86d5",
        "c70d2883d2d71d215557e88fffd9",
    );

    fn hex_bytes(hex: &str) -> Vec<u8> {
        assert_eq!(hex.len() % 2, 0, "odd hex length");
        (0..hex.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).expect("valid hex"))
            .collect()
    }

    /// 与生产路径一致：头部由 [`crate::core::reader::image::parse_jpx_header`]
    /// 解析后注入请求。
    fn decode(codestream: &[u8], smask_in_data: Option<u8>) -> Result<DecodedBitmap> {
        let header = crate::core::reader::image::parse_jpx_header(codestream);
        let request = JpxDecodeRequest {
            codestream,
            header: header.as_ref(),
            smask_in_data,
            pdf_version: Some(PdfVersion::V1_5),
        };
        decode_jpx_with(&[&HayroJpxBackend], &request)
    }

    /// 合成最小 JP2 容器（signature + ftyp + jp2h + jp2c）。
    ///
    /// `colour` 是 `colr` box 的载荷（枚举颜色空间用 `0x01, 0, 0, 取值`；
    /// ICC 用 `0x02, 0, 0, 剖面字节`）。`cdef` 非空时追加到 `jp2h`。
    fn jp2_file(
        width: u32,
        height: u32,
        components: u16,
        bpc: u8,
        colour: &[u8],
        cdef: Option<&[u8]>,
        codestream: &[u8],
    ) -> Vec<u8> {
        fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
            let mut out = Vec::with_capacity(8 + body.len());
            out.extend_from_slice(&((body.len() + 8) as u32).to_be_bytes());
            out.extend_from_slice(kind);
            out.extend_from_slice(body);
            out
        }

        // T.800 Annex A 的签名框：长度 12 + `jP  ` + 0D0A870A。
        let mut file = vec![0x00, 0x00, 0x00, 0x0C, b'j', b'P', b' ', b' ', 0x0D, 0x0A, 0x87, 0x0A];
        file.extend_from_slice(&boxed(b"ftyp", b"jp2 \0\0\0\0jp2 "));

        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&components.to_be_bytes());
        ihdr.push(bpc);
        ihdr.extend_from_slice(&[7, 0, 0]); // C / UnkC / IPR（与 OpenJPEG 一致）
        let mut header = boxed(b"ihdr", &ihdr);
        header.extend_from_slice(&boxed(b"colr", colour));
        if let Some(cdef) = cdef {
            header.extend_from_slice(&boxed(b"cdef", cdef));
        }
        file.extend_from_slice(&boxed(b"jp2h", &header));
        file.extend_from_slice(&boxed(b"jp2c", codestream));
        file
    }

    fn enumerated(colour_space: u32) -> Vec<u8> {
        let mut colour = vec![1, 0, 0];
        colour.extend_from_slice(&colour_space.to_be_bytes());
        colour
    }

    #[test]
    fn gray8_codestream_decodes_exact_samples() {
        let codestream = hex_bytes(GRAY8);
        let bitmap = decode(&codestream, None).expect("gray 8-bit codestream decodes");
        assert_eq!((bitmap.width, bitmap.height), (8, 4));
        assert_eq!(bitmap.components, 1);
        assert_eq!(bitmap.bits_per_component, 8);
        assert_eq!(bitmap.icc_profile, None);
        for y in 0..4u8 {
            for x in 0..8u8 {
                assert_eq!(
                    bitmap.samples[usize::from(y) * 8 + usize::from(x)],
                    8 * x + 8 * y,
                    "pixel ({x},{y})"
                );
            }
        }
    }

    #[test]
    fn rgb_jp2_decodes_interleaved_samples() {
        let codestream = hex_bytes(RGB8);
        let file = jp2_file(8, 4, 3, 7, &enumerated(16), None, &codestream);
        let bitmap = decode(&file, None).expect("sRGB JP2 decodes");
        assert_eq!((bitmap.width, bitmap.height), (8, 4));
        assert_eq!(bitmap.components, 3);
        assert_eq!(bitmap.bits_per_component, 8);
        assert_eq!(bitmap.samples.len(), 8 * 4 * 3);
        for y in 0..4usize {
            for x in 0..8usize {
                let base = (y * 8 + x) * 3;
                assert_eq!(bitmap.samples[base], (8 * x) as u8, "R ({x},{y})");
                assert_eq!(bitmap.samples[base + 1], (8 * y) as u8, "G ({x},{y})");
                assert_eq!(bitmap.samples[base + 2], (64 + 32 * (x % 4)) as u8, "B");
            }
        }
    }

    #[test]
    fn subsampled_chroma_is_upsampled_to_reference_grid() {
        let codestream = hex_bytes(YCBCR420);
        let bitmap = decode(&codestream, None).expect("4:2:0 codestream decodes");
        assert_eq!((bitmap.width, bitmap.height), (16, 8), "reference grid");
        assert_eq!(bitmap.components, 3);
        assert_eq!(bitmap.samples.len(), 16 * 8 * 3);
        for y in 0..8usize {
            for x in 0..16usize {
                let base = (y * 16 + x) * 3;
                assert_eq!(bitmap.samples[base], (16 * x + y) as u8, "Y ({x},{y})");
                let chroma = (8 * (x / 2) + 8 * (y / 2)) as u8;
                assert_eq!(bitmap.samples[base + 1], chroma, "Cb ({x},{y})");
                assert_eq!(bitmap.samples[base + 2], 255 - chroma, "Cr ({x},{y})");
            }
        }
    }

    #[test]
    fn uniformly_subsampled_codestream_is_upsampled_from_header() {
        let codestream = hex_bytes(UNIFORM_SUBSAMPLED);
        let bitmap = decode(&codestream, None).expect("uniform subsampling decodes");
        assert_eq!((bitmap.width, bitmap.height), (16, 8), "参考网格补回全分辨率");
        for y in 0..8usize {
            for x in 0..16usize {
                let expected = (8 * (x / 2) + 8 * (y / 2)) as u8;
                assert_eq!(bitmap.samples[y * 16 + x], expected, "pixel ({x},{y})");
            }
        }
    }

    #[test]
    fn cmyk_jp2_keeps_four_components_without_inversion() {
        let codestream = hex_bytes(CMYK8);
        // EnumCS 12 = CMYK（T.800 Annex I）：真实 4 分量 CMYK 的 colr 形态。
        let file = jp2_file(8, 4, 4, 7, &enumerated(12), None, &codestream);
        let bitmap = decode(&file, None).expect("4-component JP2 decodes");
        assert_eq!((bitmap.width, bitmap.height), (8, 4));
        assert_eq!(bitmap.components, 4);
        assert_eq!(bitmap.samples.len(), 8 * 4 * 4);
        for y in 0..4usize {
            for x in 0..8usize {
                let base = (y * 8 + x) * 4;
                assert_eq!(bitmap.samples[base], (8 * x) as u8, "C ({x},{y})");
                assert_eq!(bitmap.samples[base + 1], (8 * y) as u8, "M ({x},{y})");
                assert_eq!(bitmap.samples[base + 2], 128, "Y ({x},{y})");
                assert_eq!(bitmap.samples[base + 3], (255 - 8 * x) as u8, "K");
            }
        }
    }

    #[test]
    fn sixteen_bit_source_is_scaled_to_eight_bit() {
        let codestream = hex_bytes(GRAY16);
        let bitmap = decode(&codestream, None).expect("16-bit codestream decodes");
        assert_eq!((bitmap.width, bitmap.height), (8, 4));
        assert_eq!(bitmap.bits_per_component, 8, "upstream scales to 8 bits");
        for index in 0..32usize {
            let source = 2000.0 * index as f32;
            let expected = (source / 65535.0 * 255.0).round() as u8;
            assert_eq!(bitmap.samples[index], expected, "sample {index}");
        }
    }

    #[test]
    fn embedded_icc_profile_is_forwarded_verbatim() {
        let codestream = hex_bytes(RGB8);
        let mut profile = vec![0u8; 24];
        profile[16..20].copy_from_slice(b"RGB ");
        let mut colour = vec![2, 0, 0]; // METH = 2（受限 ICC）
        colour.extend_from_slice(&profile);
        let file = jp2_file(8, 4, 3, 7, &colour, None, &codestream);
        let bitmap = decode(&file, None).expect("ICC JP2 decodes");
        assert_eq!(bitmap.icc_profile.as_deref(), Some(profile.as_slice()));
        assert_eq!(bitmap.components, 3, "样本仍按码流分量输出");
    }

    /// 4 分量码流 + `cdef` 把通道 3 标为 opacity：上游按 RGB + alpha 处理。
    fn rgb_with_opacity() -> Vec<u8> {
        let codestream = hex_bytes(CMYK8);
        let mut cdef = Vec::new();
        cdef.extend_from_slice(&4u16.to_be_bytes());
        for (index, kind) in [(0u16, 0u16), (1, 0), (2, 0), (3, 1)] {
            cdef.extend_from_slice(&index.to_be_bytes());
            cdef.extend_from_slice(&kind.to_be_bytes());
            cdef.extend_from_slice(&0u16.to_be_bytes());
        }
        jp2_file(8, 4, 4, 7, &enumerated(16), Some(&cdef), &codestream)
    }

    #[test]
    fn opacity_channel_is_ignored_by_default() {
        let file = rgb_with_opacity();
        let bitmap = decode(&file, None).expect("default SMaskInData ignores opacity");
        assert_eq!(bitmap.components, 3);
        assert_eq!(bitmap.samples.len(), 8 * 4 * 3);
        assert_eq!(&bitmap.samples[..3], &[0, 0, 128], "颜色分量原样保留");
        // 显式 0 与缺省同义。
        assert!(decode(&file, Some(0)).is_ok());
    }

    #[test]
    fn requested_soft_mask_is_reported_unsupported() {
        let file = rgb_with_opacity();
        let error = decode(&file, Some(1)).expect_err("opacity channel cannot be represented");
        assert!(
            matches!(&error, ImageError::Unsupported(message) if message.contains("SMaskInData 1")),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn truncated_codestream_reports_filter_error() {
        let codestream = hex_bytes(GRAY8);
        let error = decode(&codestream[..codestream.len() / 2], None)
            .expect_err("truncated codestream must fail");
        assert!(
            matches!(&error, ImageError::Filter(message) if message.starts_with("JPXDecode: ")),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn garbage_bytes_report_filter_error() {
        let error =
            decode(&[0xDE, 0xAD, 0xBE, 0xEF], None).expect_err("garbage codestream must fail");
        assert!(
            matches!(&error, ImageError::Filter(message) if message.starts_with("JPXDecode: ")),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn version_gate_rejects_pdf_1_4() {
        let codestream = hex_bytes(GRAY8);
        let request = JpxDecodeRequest {
            codestream: &codestream,
            header: None,
            smask_in_data: None,
            pdf_version: Some(PdfVersion::V1_4),
        };
        let error = decode_jpx_with(&[&HayroJpxBackend], &request)
            .expect_err("PDF 1.4 cannot declare JPXDecode");
        assert!(
            matches!(&error, ImageError::Unsupported(message) if message.contains("requires PDF 1.5")),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn resource_limits_reject_oversized_geometry() {
        let error = check_limits(MAX_DIMENSION + 1, 1, 1).expect_err("dimension limit");
        assert!(
            matches!(&error, ImageError::Filter(message) if message.contains("exceeds")),
            "unexpected error: {error}"
        );
        let error = check_limits(MAX_DIMENSION, MAX_DIMENSION, 4).expect_err("byte limit");
        assert!(
            matches!(&error, ImageError::Filter(message) if message.contains("exceeds")),
            "unexpected error: {error}"
        );
        check_limits(1024, 1024, 4).expect("1 MiB x 4 fits the limit");
    }
}
