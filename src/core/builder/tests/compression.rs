//! 写端流压缩（D4）测试：版本门控、失败零字节契约与默认明文。

use super::*;
use crate::core::writer::{DeflateBackend, ImagePredictor};
use std::sync::Arc;

/// 24×24 平滑渐变 RGB 图像：行内递增值让 PNG Up 预测器产生大量零差值，
/// 同时纯 Flate 也能显著缩小（避免小载荷被 zlib 头膨胀）。
fn compressible_image() -> ImageData {
    let mut data = Vec::with_capacity(24 * 24 * 3);
    for y in 0..24u8 {
        for x in 0..24u8 {
            data.extend_from_slice(&[x * 10, y * 10, (x + y) * 5]);
        }
    }
    ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 24,
        height: 24,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data,
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    }
}

fn build_with(
    version: PdfVersion,
    compression: Option<StreamCompression>,
) -> (Result<(), WriteError>, Vec<u8>) {
    let mut b = PdfBuilder::with_version(version);
    if let Some(comp) = compression {
        b.compress_streams(comp);
    }
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(12.0), 72.0, 720.0, "D4");
    let img = compressible_image();
    b.image(p, 72.0, 600.0, 120.0, 120.0, &img);
    let mut out = Vec::new();
    let result = b.build(&mut out);
    (result, out)
}

#[test]
fn flate_compression_requires_pdf_1_2() {
    let (error, out) = build_with(PdfVersion::V1_1, Some(StreamCompression::flate()));
    let error = error.expect_err("PDF 1.1 must reject flate compression");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "错误应为 UnsupportedVersion: {error}"
    );
    assert!(
        error
            .to_string()
            .contains("flate stream compression requires PDF 1.2"),
        "错误信息应说明版本要求: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");

    let (result, out) = build_with(PdfVersion::V1_2, Some(StreamCompression::flate()));
    result.expect("PDF 1.2 supports flate compression");
    assert!(
        String::from_utf8_lossy(&out).contains("/Filter /FlateDecode"),
        "PDF 1.2 启用后应写出 FlateDecode 流"
    );
}

#[test]
fn image_predictor_requires_pdf_1_3() {
    let compression = StreamCompression::flate().with_image_predictor(ImagePredictor::PngUp);
    let (error, out) = build_with(PdfVersion::V1_2, Some(compression));
    let error = error.expect_err("PDF 1.2 must reject PNG predictor");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "错误应为 UnsupportedVersion: {error}"
    );
    assert!(
        error
            .to_string()
            .contains("PNG image predictor requires PDF 1.3"),
        "错误信息应说明预测器版本要求: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");

    let compression = StreamCompression::flate().with_image_predictor(ImagePredictor::PngUp);
    let (result, out) = build_with(PdfVersion::V1_3, Some(compression));
    result.expect("PDF 1.3 supports PNG predictor");
    let text = String::from_utf8_lossy(&out).into_owned();
    assert!(text.contains("/Predictor 15"), "{text}");
    assert!(text.contains("/Colors 3"), "{text}");
    assert!(text.contains("/Columns 24"), "{text}");
}

#[test]
fn failing_backend_build_writes_no_bytes() {
    struct Broken;
    impl DeflateBackend for Broken {
        fn deflate(&self, _data: &[u8]) -> Result<Vec<u8>, String> {
            Err("backend exploded".into())
        }
    }
    let compression = StreamCompression::with_backend(Arc::new(Broken));
    let (error, out) = build_with(PdfVersion::V1_7, Some(compression));
    let error = error.expect_err("broken backend must fail the build");
    assert!(
        matches!(&error, WriteError::Compression(_)),
        "错误应为 Compression: {error}"
    );
    assert!(
        error.to_string().contains("backend exploded"),
        "错误信息应透传后端原因: {error}"
    );
    assert!(out.is_empty(), "压缩失败时不应静默回退明文写出任何字节");
}

#[test]
fn streams_stay_plain_by_default() {
    let (result, out) = build_with(PdfVersion::V1_7, None);
    result.expect("default build should succeed");
    let text = String::from_utf8_lossy(&out);
    assert!(
        !text.contains("/FlateDecode"),
        "未启用压缩策略时所有流应保持明文"
    );
    assert!(text.contains("/Im1 Do"), "{text}");
}
