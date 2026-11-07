//! JBIG2 像素解码后端：`hayro-jbig2` 的唯一适配点（D3 契约）。
//!
//! 本文件是 `hayro_jbig2` 类型在 `src/` 中唯一允许出现的位置（由
//! `scripts/deps_check.ps1` 的 D3 静态扫描核对，文件自身的测试纳入
//! `test_paths`）。对外只暴露项目自有类型：[`HayroJbig2Backend`] 实现
//! [`Jbig2Backend`]，错误一律归一化为
//! [`crate::core::reader::image::error::ImageError`] 并带 `JBIG2Decode: `
//! 前缀，与其它过滤器（`CCITTFaxDecode:` / `FlateDecode:` 等）同口径。
//!
//! # 位图约定
//!
//! 输出遵循 PDF 1 位 DeviceGray 样本约定：**1 = 白、0 = 黑**（与默认
//! `/Decode [0 1]`、CCITT 适配层同一口径）。T.88 的页面位图以 1 = 黑，
//! 适配层因此在 [sink](RowSink) 内逐像素反转；每行按位紧排，行末不足
//! 8 位的填充位补 1（白），与 CCITT 适配层的补白一致。
//!
//! # 码流边界与损坏处置
//!
//! - PDF 图像流应为嵌入组织（T.88 Annex D.3）：文件头、页尾段、文件尾段
//!   「存在但禁用」。上游遇到页尾/文件尾段会跳过（jbig2dec 同口径的宽容
//!   度），不视为损坏。
//! - 若码流仍带 JBIG2 文件签名（部分生成器违规写入 `/Filter` 前），改用
//!   文件组织解析，避免把合法文件判为损坏。
//! - `/JBIG2Globals` 的**原始段字节**经
//!   [`Jbig2DecodeRequest::globals_raw`] 注入，与图像流段合并后按段号排序
//!   解码（T.88 §7.2.3 的引用顺序由上游保证）。
//! - 段头截断、段长越界、熵解码失败一律返回 `ImageError::Filter`
//!   （`JBIG2Decode: …`），不 panic、不返回部分结果。
//!
//! # 资源护栏
//!
//! 分配像素缓冲前，先按页面信息段几何检查 [`MAX_DIMENSION`] 与
//! [`MAX_OUTPUT_BYTES`]；超限直接返回 `ImageError::Filter`（文案含
//! `exceeds`），不做任何大块分配，避免畸形页几何触发 OOM。

use super::super::error::{ImageError, Result};
use super::{DecodedBitmap, Jbig2Backend, Jbig2DecodeRequest};

/// 单边像素上限（T.88 页几何为 u32，这里按 2^16 收紧）。
const MAX_DIMENSION: u32 = 1 << 16;

/// 解码输出字节上限，与 CCITT 适配层同一口径（256 MB）。
const MAX_OUTPUT_BYTES: usize = 256 * 1024 * 1024;

/// JBIG2 文件签名（T.88 §6.2）；PDF 图像流不应带它，但需容忍违规写入。
const FILE_SIGNATURE: [u8; 8] = [0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A];

/// 基于 `hayro-jbig2` 的 JBIG2 解码后端（无状态，可共享）。
#[derive(Debug, Clone, Copy, Default)]
pub struct HayroJbig2Backend;

impl Jbig2Backend for HayroJbig2Backend {
    fn name(&self) -> &'static str {
        "hayro-jbig2"
    }

    fn decode(&self, request: &Jbig2DecodeRequest<'_>) -> Result<DecodedBitmap> {
        let image = if request.codestream.starts_with(&FILE_SIGNATURE) {
            hayro_jbig2::Image::new(request.codestream)
        } else {
            hayro_jbig2::Image::new_embedded(request.codestream, request.globals_raw)
        }
        .map_err(|error| failure(format!("segment chain: {error}")))?;

        let width = image.width();
        let height = image.height();
        check_limits(width, height)?;

        let mut sink = RowSink::new(width, height);
        image
            .decode(&mut sink)
            .map_err(|error| failure(format!("pixel data: {error}")))?;
        sink.finish()
    }
}

fn failure(message: impl Into<String>) -> ImageError {
    ImageError::Filter(format!("JBIG2Decode: {}", message.into()))
}

fn check_limits(width: u32, height: u32) -> Result<()> {
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(failure(format!(
            "page geometry {width}x{height} exceeds the {MAX_DIMENSION}-pixel dimension limit"
        )));
    }
    let bytes = (width as usize).div_ceil(8).saturating_mul(height as usize);
    if bytes > MAX_OUTPUT_BYTES {
        return Err(failure(format!(
            "decoded size {bytes} bytes exceeds the {MAX_OUTPUT_BYTES}-byte output limit"
        )));
    }
    Ok(())
}

/// 行优先、按位紧排的 PDF 样本汇：初始化为全白（1），黑像素清零该位。
struct RowSink {
    width: u32,
    row: Vec<u8>,
    out: Vec<u8>,
    x: u32,
    rows: u32,
    height: u32,
    overflow: bool,
}

impl RowSink {
    fn new(width: u32, height: u32) -> Self {
        let row_bytes = (width as usize).div_ceil(8);
        Self {
            width,
            row: vec![0xFF; row_bytes],
            out: Vec::with_capacity(row_bytes.saturating_mul(height as usize)),
            x: 0,
            rows: 0,
            height,
            overflow: false,
        }
    }

    fn finish(mut self) -> Result<DecodedBitmap> {
        if self.overflow {
            return Err(failure(
                "decoded pixels exceed the page geometry declared in the page information segment",
            ));
        }
        if self.rows != self.height {
            return Err(failure(format!(
                "decoded {} of {} rows",
                self.rows, self.height
            )));
        }
        Ok(DecodedBitmap {
            width: self.width,
            height: self.height,
            components: 1,
            bits_per_component: 1,
            samples: std::mem::take(&mut self.out),
            icc_profile: None,
        })
    }

    fn flush_row(&mut self) {
        self.out.extend_from_slice(&self.row);
        self.row.fill(0xFF);
        self.x = 0;
        self.rows += 1;
    }
}

impl hayro_jbig2::Decoder for RowSink {
    fn push_pixel(&mut self, black: bool) {
        if self.overflow {
            return;
        }
        if self.x >= self.width {
            self.overflow = true;
            return;
        }
        if black {
            self.row[(self.x / 8) as usize] &= !(0x80 >> (self.x % 8));
        }
        self.x += 1;
    }

    fn push_pixel_chunk(&mut self, black: bool, chunk_count: u32) {
        if self.overflow {
            return;
        }
        if !self.x.is_multiple_of(8) || u64::from(self.x) + u64::from(chunk_count) * 8 > u64::from(self.width) {
            self.overflow = true;
            return;
        }
        if black {
            let start = (self.x / 8) as usize;
            self.row[start..start + chunk_count as usize].fill(0x00);
        }
        self.x += chunk_count * 8;
    }

    fn next_line(&mut self) {
        if self.overflow {
            return;
        }
        if self.rows >= self.height || self.x != self.width {
            self.overflow = true;
            return;
        }
        self.flush_row();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reader::image::error::ImageError;
    use crate::core::types::PdfVersion;
    use crate::core::reader::image::backend::decode_jbig2_with;

    /// 经分发层调用内建后端（与 `ImageModel::decode_image_with` 同一条路径）。
    fn decode(request: &Jbig2DecodeRequest<'_>) -> Result<DecodedBitmap> {
        decode_jbig2_with(&[&HayroJbig2Backend], request)
    }

    /// PDF 32000-1:2008 §7.4.7 EXAMPLE 1 的 `/JBIG2Globals` 流（符号字典
    /// 段，61 字节）。逐字节取自规范 EXAMPLE 2 的段清单（页面 32–33），
    /// 与本机 `jbig2dec` 解码结果一致。
    const GLOBALS_HEX: &str = concat!(
        "0000000000010000000032",
        "000003fffdff02fefefe0000000100000001",
        "2ae225aea9a5a538b4d9999c5c8e56ef0f8727f2b53d4e37ef795cc5506dffac",
    );

    /// 参考位图：`jbig2dec` 对上述码流（含 13 字节文件头的完整文件）输出
    /// 的 PBM P4 载荷，52×66、每行 7 字节、1 = 黑，共 462 字节。
    const GOLDEN_PBM_HEX: &str = "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000003f000000000000ffe00000000001c1e0000000000380e000000000078060000000000f0060000000000f0000000000000f0000000000000f0000000000000f0000000000000f0000000000000f0000000000000f0000000000000f0000000000000f001000000000070070000000000380700000000003c1e00000000001ffc000000000007f0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001f8000000000007ff00000000000e0f00000000001c0700000000003c03000000000078030000000000780000000000007800000000000078000000000000780000000000007800000000000078000000000000780000000000007800000000000078008000000000380380000000001c0380000000001e0f00000000000ffe000000000003f8000000000000000000000000000000000000000000000000";

    fn hex_bytes(hex: &str) -> Vec<u8> {
        assert_eq!(hex.len() % 2, 0, "odd hex length");
        (0..hex.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).expect("valid hex"))
            .collect()
    }

    /// 页面信息段（T.88 §7.4.7，19 字节载荷），页关联固定为 1。
    fn page_info_segment(width: u32, height: u32, flags: u8) -> Vec<u8> {
        let mut segment = vec![0, 0, 0, 1, 0x30, 0x00, 0x01, 0, 0, 0, 19];
        segment.extend_from_slice(&width.to_be_bytes());
        segment.extend_from_slice(&height.to_be_bytes());
        segment.extend_from_slice(&[0; 8]); // XResolution / YResolution
        segment.push(flags);
        segment.extend_from_slice(&[0, 0]); // striping
        segment
    }

    /// 立即文本区域段（T.88 §7.2.4 段头 + §7.4.4 载荷，30 字节），引用
    /// 全局符号字典段 0；熵编码数据逐字节取自规范的 EXAMPLE。
    fn text_region_segment() -> Vec<u8> {
        let mut segment = vec![0, 0, 0, 2, 0x06, 0x20, 0x00, 0x01, 0, 0, 0, 30];
        segment.extend_from_slice(&52u32.to_be_bytes()); // region width
        segment.extend_from_slice(&66u32.to_be_bytes()); // region height
        segment.extend_from_slice(&[0; 8]); // X / Y offsets
        segment.extend_from_slice(&[0x02, 0x00, 0x10, 0x00, 0x00, 0x00, 0x02]);
        segment.extend_from_slice(&[0x31, 0xDB, 0x51, 0xCE, 0x51, 0xFF, 0xAC]);
        segment
    }

    /// PDF 32000-1 §7.4.7 EXAMPLE 的图像流：页面信息段 + 立即文本区域段，
    /// 共 72 字节（页尾/文件尾段按 §7.4.7 丢弃）。
    fn spec_example_stream() -> Vec<u8> {
        let mut stream = page_info_segment(52, 66, 0x40);
        stream.extend_from_slice(&text_region_segment());
        stream
    }

    /// 构造「只有页面信息段」的段链：无区域段时页面位图取默认像素。
    fn page_information_only(width: u32, height: u32, default_pixel: bool) -> Vec<u8> {
        page_info_segment(width, height, if default_pixel { 0x04 } else { 0x00 })
    }

    fn request<'a>(
        codestream: &'a [u8],
        globals: Option<&'a [u8]>,
        version: Option<PdfVersion>,
    ) -> Jbig2DecodeRequest<'a> {
        Jbig2DecodeRequest {
            codestream,
            globals_raw: globals,
            page_information: None,
            pdf_version: version,
        }
    }

    #[test]
    fn spec_example_matches_reference_bitmap() {
        let globals = hex_bytes(GLOBALS_HEX);
        let stream = spec_example_stream();
        let reference = hex_bytes(GOLDEN_PBM_HEX);
        assert_eq!(globals.len(), 61, "globals fixture");
        assert_eq!(stream.len(), 72, "image stream fixture");
        assert_eq!(reference.len(), 7 * 66, "reference bitmap fixture");

        let backend = HayroJbig2Backend;
        let bitmap = decode_jbig2_with(
            &[&backend],
            &request(&stream, Some(&globals), Some(PdfVersion::V1_4)),
        )
        .expect("PDF 32000-1 §7.4.7 example decodes");

        assert_eq!((bitmap.width, bitmap.height), (52, 66));
        assert_eq!(bitmap.components, 1);
        assert_eq!(bitmap.bits_per_component, 1);
        assert_eq!(bitmap.samples.len(), 7 * 66, "packed bit rows");

        let mut mismatches = 0usize;
        let mut black = 0usize;
        for y in 0..66usize {
            for x in 0..52usize {
                let shift = 7 - (x % 8);
                let expected_black = (reference[y * 7 + x / 8] >> shift) & 1;
                let ours = (bitmap.samples[y * 7 + x / 8] >> shift) & 1;
                if expected_black == 1 {
                    black += 1;
                }
                // 参考位图 1 = 黑；本项目样本 0 = 黑。
                if ours != 1 - expected_black {
                    mismatches += 1;
                }
            }
        }
        assert_eq!(mismatches, 0, "pixel-for-pixel match with jbig2dec");
        assert_eq!(black, 234, "reference black pixel count");
        // 行末填充位补白（1），避免下游把填充位读成黑。
        for y in 0..66usize {
            for x in 52..56usize {
                let shift = 7 - (x % 8);
                assert_eq!(
                    (bitmap.samples[y * 7 + x / 8] >> shift) & 1,
                    1,
                    "row {y} padding bit must be white"
                );
            }
        }
    }

    #[test]
    fn default_black_page_decodes_through_chunk_path() {
        let stream = page_information_only(16, 16, true);
        let bitmap = decode(&request(&stream, None, Some(PdfVersion::V1_4)))
            .expect("page information only");
        assert_eq!((bitmap.width, bitmap.height), (16, 16));
        assert_eq!(bitmap.samples.len(), 2 * 16);
        assert!(
            bitmap.samples.iter().all(|byte| *byte == 0x00),
            "default pixel 1 means every sample bit is black (0)"
        );
    }

    #[test]
    fn default_white_page_is_all_white() {
        let stream = page_information_only(8, 4, false);
        let bitmap =
            decode(&request(&stream, None, Some(PdfVersion::V1_4))).expect("all white");
        assert_eq!(bitmap.samples, vec![0xFF; 4]);
    }

    #[test]
    fn oversized_geometry_is_rejected_before_allocation() {
        let stream = page_information_only(65_535, 65_535, false);
        let error = decode(&request(&stream, None, Some(PdfVersion::V1_4)))
            .expect_err("oversized page must be rejected");
        match error {
            ImageError::Filter(message) => {
                assert!(message.starts_with("JBIG2Decode: "), "{message}");
                assert!(message.contains("exceeds"), "{message}");
            }
            other => panic!("expected filter error, got {other:?}"),
        }
    }

    #[test]
    fn truncated_segment_header_reports_filter_error() {
        let stream = spec_example_stream();
        let error = decode(&request(&stream[..6], None, Some(PdfVersion::V1_4)))
            .expect_err("truncated header");
        match error {
            ImageError::Filter(message) => assert!(message.starts_with("JBIG2Decode: "), "{message}"),
            other => panic!("expected filter error, got {other:?}"),
        }
    }

    #[test]
    fn truncated_segment_payload_reports_filter_error() {
        // 页面信息段声明 19 字节载荷，只给 19-6 字节。
        let full = page_information_only(16, 16, false);
        let error = decode(&request(&full[..full.len() - 6], None, Some(PdfVersion::V1_4)))
            .expect_err("truncated payload");
        match error {
            ImageError::Filter(message) => assert!(message.starts_with("JBIG2Decode: "), "{message}"),
            other => panic!("expected filter error, got {other:?}"),
        }
    }

    #[test]
    fn garbage_bytes_report_filter_error() {
        let data = b"not a jbig2 segment chain".to_vec();
        let error = decode(&request(&data, None, Some(PdfVersion::V1_4)))
            .expect_err("garbage");
        match error {
            ImageError::Filter(message) => assert!(message.starts_with("JBIG2Decode: "), "{message}"),
            other => panic!("expected filter error, got {other:?}"),
        }
    }

    #[test]
    fn version_gate_rejects_pdf_1_3() {
        let stream = page_information_only(8, 8, false);
        let error = decode(&request(&stream, None, Some(PdfVersion::V1_3)))
            .expect_err("JBIG2Decode requires PDF 1.4");
        match error {
            ImageError::Unsupported(message) => {
                assert!(message.contains("requires PDF 1.4"), "{message}");
            }
            other => panic!("expected unsupported, got {other:?}"),
        }
    }

    #[test]
    fn file_header_prefixed_stream_is_accepted() {
        // 违规但常见的形态：嵌入段链前多了 13 字节文件头。
        let mut stream = FILE_SIGNATURE.to_vec();
        stream.extend_from_slice(&[0x01, 0, 0, 0, 1]);
        stream.extend_from_slice(&page_information_only(8, 8, true));
        let bitmap =
            decode(&request(&stream, None, Some(PdfVersion::V1_4))).expect("file header");
        assert_eq!((bitmap.width, bitmap.height), (8, 8));
        assert_eq!(bitmap.samples, vec![0x00; 8]);
    }
}
