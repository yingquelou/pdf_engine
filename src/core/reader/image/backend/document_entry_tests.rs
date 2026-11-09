//! `Document` 层解码入口的单元测试：用记录型假后端断言文档声明版本经
//! `source_version()` 注入请求、版本门禁在 Document 路径上端到端生效，并
//! 回归 `ImageModel::samples()` 与便捷路径 `decode_image_with` 的既有语义。
//! 不涉及任何真实 JBIG2/JPEG 2000 解码。

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::core::reader::image::error::ImageError;
use crate::core::reader::image::model::ImageModel;
use crate::core::types::{Document, PdfVersion};

use super::{
    DecodedBitmap, ImageBackends, Jbig2Backend, Jbig2DecodeRequest, JpxBackend, JpxDecodeRequest,
};

/// 2×2、1 分量、1 bpp 的合法位图（row_bytes=1、height=2 → 2 字节）。
fn ok_bitmap() -> DecodedBitmap {
    DecodedBitmap {
        width: 2,
        height: 2,
        components: 1,
        bits_per_component: 1,
        samples: vec![0xAA, 0x55],
        icc_profile: None,
    }
}

/// 记录收到的 `pdf_version` 与调用次数的 JPX 假后端（恒成功）。
struct RecordingJpx {
    calls: AtomicUsize,
    seen: Mutex<Option<Option<PdfVersion>>>,
}

impl RecordingJpx {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            seen: Mutex::new(None),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }

    fn seen_version(&self) -> Option<Option<PdfVersion>> {
        *self.seen.lock().unwrap()
    }
}

impl JpxBackend for RecordingJpx {
    fn name(&self) -> &'static str {
        "recording-jpx"
    }

    fn decode(&self, request: &JpxDecodeRequest<'_>) -> Result<DecodedBitmap, ImageError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        *self.seen.lock().unwrap() = Some(request.pdf_version);
        Ok(ok_bitmap())
    }
}

/// 记录收到的 `pdf_version` 与调用次数的 JBIG2 假后端（恒成功）。
struct RecordingJbig2 {
    calls: AtomicUsize,
    seen: Mutex<Option<Option<PdfVersion>>>,
}

impl RecordingJbig2 {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            seen: Mutex::new(None),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }

    fn seen_version(&self) -> Option<Option<PdfVersion>> {
        *self.seen.lock().unwrap()
    }
}

impl Jbig2Backend for RecordingJbig2 {
    fn name(&self) -> &'static str {
        "recording-jbig2"
    }

    fn decode(&self, request: &Jbig2DecodeRequest<'_>) -> Result<DecodedBitmap, ImageError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        *self.seen.lock().unwrap() = Some(request.pdf_version);
        Ok(ok_bitmap())
    }
}

fn model(filters: &[&str]) -> ImageModel {
    ImageModel {
        width: 2,
        height: 2,
        color_space: None,
        bits_per_component: 1,
        decode: None,
        interpolate: false,
        intent: None,
        matte: None,
        image_mask: false,
        mask: None,
        smask: None,
        smask_in_data: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        opi: None,
        jpx: None,
        jbig2: None,
        filters: filters.iter().map(|name| (*name).to_owned()).collect(),
        other: BTreeMap::new(),
        data: b"payload".to_vec(),
    }
}

fn doc_with_version(version: PdfVersion) -> Document {
    let mut doc = Document::new();
    doc.source_version = version;
    doc
}

/// Document 入口把 `source_version()` 注入 JPX 后端请求（JPX 恰好 1.5 放行）。
#[test]
fn document_entry_injects_source_version_into_jpx_backend() {
    let fake = RecordingJpx::new();
    let backends: [&dyn JpxBackend; 1] = [&fake];
    let doc = doc_with_version(PdfVersion::V1_5);
    let model = model(&["JPXDecode"]);
    let bitmap = doc
        .decode_image_model_with(&model, &ImageBackends::new(&[], &backends))
        .expect("1.5 must pass the JPX gate");
    assert_eq!(bitmap, ok_bitmap());
    assert_eq!(fake.calls(), 1);
    assert_eq!(
        fake.seen_version(),
        Some(Some(PdfVersion::V1_5)),
        "后端收到的 pdf_version 必须就是文档 source_version"
    );
}

/// Document 入口把 `source_version()` 注入 JBIG2 后端请求（1.4 恰好放行）。
#[test]
fn document_entry_injects_source_version_into_jbig2_backend() {
    let fake = RecordingJbig2::new();
    let backends: [&dyn Jbig2Backend; 1] = [&fake];
    let doc = doc_with_version(PdfVersion::V1_4);
    let model = model(&["JBIG2Decode"]);
    doc.decode_image_model_with(&model, &ImageBackends::new(&backends, &[]))
        .expect("1.4 must pass the JBIG2 gate");
    assert_eq!(fake.calls(), 1);
    assert_eq!(fake.seen_version(), Some(Some(PdfVersion::V1_4)));
}

/// 低版本（1.3）经 Document 入口拒绝 JBIG2，后端零调用。
#[test]
fn document_entry_jbig2_gate_rejects_pre_1_4_without_backend_call() {
    let fake = RecordingJbig2::new();
    let backends: [&dyn Jbig2Backend; 1] = [&fake];
    let doc = doc_with_version(PdfVersion::V1_3);
    let model = model(&["JBIG2Decode"]);
    let error = doc
        .decode_image_model_with(&model, &ImageBackends::new(&backends, &[]))
        .expect_err("1.3 must gate out JBIG2 even with a working backend");
    match error {
        ImageError::Unsupported(message) => {
            assert!(
                message.contains("JBIG2Decode") && message.contains("1.4"),
                "{message}"
            );
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
    assert_eq!(fake.calls(), 0, "门禁必须先于后端分发拦截");
}

/// 低版本（1.4）经 Document 入口拒绝 JPX，后端零调用；1.3 同样拒绝。
#[test]
fn document_entry_jpx_gate_rejects_pre_1_5_without_backend_call() {
    for version in [PdfVersion::V1_3, PdfVersion::V1_4] {
        let fake = RecordingJpx::new();
        let backends: [&dyn JpxBackend; 1] = [&fake];
        let doc = doc_with_version(version);
        let model = model(&["JPXDecode"]);
        let error = doc
            .decode_image_model_with(&model, &ImageBackends::new(&[], &backends))
            .expect_err("pre-1.5 must gate out JPX");
        match error {
            ImageError::Unsupported(message) => {
                assert!(
                    message.contains("JPXDecode") && message.contains("1.5"),
                    "{message}"
                );
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
        assert_eq!(fake.calls(), 0, "{version:?} must not reach the backend");
    }
}

/// 恰好 1.4/1.5 边界经 Document 入口放行、更高版本同样放行。
#[test]
fn document_entry_boundary_versions_pass_gate() {
    let jbig2_cases = [PdfVersion::V1_4, PdfVersion::V1_7];
    for version in jbig2_cases {
        let fake = RecordingJbig2::new();
        let backends: [&dyn Jbig2Backend; 1] = [&fake];
        let doc = doc_with_version(version);
        doc.decode_image_model_with(
            &model(&["JBIG2Decode"]),
            &ImageBackends::new(&backends, &[]),
        )
        .unwrap_or_else(|error| panic!("JBIG2 @{version:?} must pass, got {error:?}"));
        assert_eq!(fake.calls(), 1);
    }
    let jpx_cases = [PdfVersion::V1_5, PdfVersion::V1_6, PdfVersion::V1_7];
    for version in jpx_cases {
        let fake = RecordingJpx::new();
        let backends: [&dyn JpxBackend; 1] = [&fake];
        let doc = doc_with_version(version);
        doc.decode_image_model_with(&model(&["JPXDecode"]), &ImageBackends::new(&[], &backends))
            .unwrap_or_else(|error| panic!("JPX @{version:?} must pass, got {error:?}"));
        assert_eq!(fake.calls(), 1);
    }
}

/// 便捷路径 `ImageModel::decode_image_with` 仍注入 `pdf_version: None`
/// （保持既有宽容语义，公共 API 行为零变化）。
#[test]
fn convenience_path_still_injects_none_version() {
    let fake = RecordingJpx::new();
    let backends: [&dyn JpxBackend; 1] = [&fake];
    let model = model(&["JPXDecode"]);
    model
        .decode_image_with(&ImageBackends::new(&[], &backends))
        .expect("convenience path ignores the gate (None is permissive)");
    assert_eq!(fake.calls(), 1);
    assert_eq!(
        fake.seen_version(),
        Some(None),
        "便捷路径必须仍传 None（与文档版本无关）"
    );
}

/// `ImageModel::samples()` 对 JBIG2/JPX 继续返回类型化 `Unsupported`
/// （本增量对既有行为零改动）。
#[test]
fn samples_behavior_unchanged_for_codecs() {
    for filters in [&["JBIG2Decode"][..], &["JPXDecode"][..]] {
        let error = model(filters)
            .samples()
            .expect_err("codec images must stay Unsupported in samples()");
        assert!(
            matches!(error, ImageError::Unsupported(_)),
            "filters {filters:?}: got {error:?}"
        );
    }
}

/// 非 JBIG2/JPX 图像经 Document 入口仍提示改用 `samples()`（`Unsupported`）。
#[test]
fn document_entry_non_codec_image_directs_to_samples() {
    let doc = doc_with_version(PdfVersion::V1_7);
    let model = model(&["FlateDecode"]);
    let error = doc
        .decode_image_model_with(&model, &ImageBackends::NONE)
        .expect_err("plain image must not dispatch to codec backends");
    match error {
        ImageError::Unsupported(message) => assert!(message.contains("samples"), "{message}"),
        other => panic!("expected Unsupported, got {other:?}"),
    }
}
