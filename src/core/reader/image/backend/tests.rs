//! 后端契约层单元测试：全部用 `#[cfg(test)]` 假后端验证分发、错误形态
//! 与版本门禁；不涉及任何真实 JBIG2/JPEG 2000 解码。

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::core::reader::image::error::ImageError;
use crate::core::reader::image::model::ImageModel;
use crate::core::types::PdfVersion;

use super::{
    DecodedBitmap, ImageBackends, ImageCodec, Jbig2Backend, Jbig2DecodeRequest, JpxBackend,
    JpxDecodeRequest, check_codec_version, decode_jbig2_with, decode_jpx_with,
};

/// 2×1、1 分量、1 bpp 的合法位图（row_bytes=1，height=2 → 2 字节）。
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

/// 契约层假后端共用的判决形态。
#[derive(Clone, Copy)]
enum Outcome {
    Good,
    Corrupt,
    NotSupported,
    /// 产出违反 DecodedBitmap 形状契约的位图（样本长度不符）。
    BadShape,
}

struct FakeJpx {
    name: &'static str,
    outcome: Outcome,
    calls: AtomicUsize,
    seen_len: AtomicUsize,
}

impl FakeJpx {
    fn new(name: &'static str, outcome: Outcome) -> Self {
        Self {
            name,
            outcome,
            calls: AtomicUsize::new(0),
            seen_len: AtomicUsize::new(0),
        }
    }
}

impl JpxBackend for FakeJpx {
    fn name(&self) -> &'static str {
        self.name
    }

    fn decode(&self, request: &JpxDecodeRequest<'_>) -> Result<DecodedBitmap, ImageError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.seen_len
            .store(request.codestream.len(), Ordering::Relaxed);
        dispatch(self.outcome)
    }
}

struct FakeJbig2 {
    outcome: Outcome,
    calls: AtomicUsize,
}

impl FakeJbig2 {
    fn new(outcome: Outcome) -> Self {
        Self {
            outcome,
            calls: AtomicUsize::new(0),
        }
    }
}

impl Jbig2Backend for FakeJbig2 {
    fn name(&self) -> &'static str {
        "fake-jbig2"
    }

    fn decode(&self, _request: &Jbig2DecodeRequest<'_>) -> Result<DecodedBitmap, ImageError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        dispatch(self.outcome)
    }
}

fn dispatch(outcome: Outcome) -> Result<DecodedBitmap, ImageError> {
    match outcome {
        Outcome::Good => Ok(ok_bitmap()),
        Outcome::Corrupt => Err(ImageError::Filter(
            "codestream truncated at segment header".to_owned(),
        )),
        Outcome::NotSupported => Err(ImageError::Unsupported(
            "fake backend covers none of this codestream".to_owned(),
        )),
        Outcome::BadShape => Ok(DecodedBitmap {
            width: 2,
            height: 2,
            components: 1,
            bits_per_component: 1,
            samples: vec![0xAA],
            icc_profile: None,
        }),
    }
}

fn image(filters: &[&str], data: Vec<u8>) -> ImageModel {
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
        data,
    }
}

fn jpx_request<'a>(codestream: &'a [u8], version: Option<PdfVersion>) -> JpxDecodeRequest<'a> {
    JpxDecodeRequest {
        codestream,
        header: None,
        smask_in_data: None,
        pdf_version: version,
    }
}

fn jbig2_request<'a>(codestream: &'a [u8], version: Option<PdfVersion>) -> Jbig2DecodeRequest<'a> {
    Jbig2DecodeRequest {
        codestream,
        globals_raw: None,
        page_information: None,
        pdf_version: version,
    }
}

/// 未注册任何后端时，JBIG2/JPX 图像维持类型化 `Unsupported`（`NONE`
/// 仍是显式空集合，不 panic）。
#[test]
fn unregistered_backends_keep_typed_unsupported_behavior() {
    for filters in [&["JPXDecode"][..], &["JBIG2Decode"][..]] {
        let model = image(filters, b"whatever".to_vec());
        let error = model
            .decode_image_with(&ImageBackends::NONE)
            .expect_err("no backend must error");
        assert!(
            matches!(error, ImageError::Unsupported(_)),
            "filters {filters:?}: got {error:?}"
        );
    }
}

/// 内建集合登记两个像素解码后端，且名字唯一标识适配器实现。
#[test]
fn builtin_backends_register_hayro_adapters() {
    let backends = super::builtin_image_backends();
    assert_eq!(backends.jbig2()[0].name(), "hayro-jbig2");
    assert_eq!(backends.jpx()[0].name(), "hayro-jpeg2000");
}

/// 非 JBIG2/JPX 图像走后端通路时提示改用 `samples()`，仍是 `Unsupported`。
#[test]
fn plain_image_directs_caller_back_to_samples() {
    let model = image(&["FlateDecode"], vec![0]);
    let error = model
        .decode_image_with(&ImageBackends::NONE)
        .expect_err("non-codec image must error");
    match error {
        ImageError::Unsupported(message) => assert!(message.contains("samples"), "got: {message}"),
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

/// 截断/损坏码流由后端报 `Filter`，分发层原样透传、不吞错、不回退。
#[test]
fn truncated_codestream_propagates_typed_filter_error() {
    let fake = FakeJpx::new("fake", Outcome::Corrupt);
    let backends: [&dyn JpxBackend; 1] = [&fake];
    let truncated = b"\xFF\x4F\xFF\x51\x00\x00\x10".to_vec();
    let request = jpx_request(&truncated, None);
    let error = decode_jpx_with(&backends, &request).expect_err("corrupt input must error");
    assert!(
        matches!(error, ImageError::Filter(_)),
        "corrupt stream must stay Filter, got {error:?}"
    );
    assert_eq!(fake.calls.load(Ordering::Relaxed), 1);
}

/// 首个后端声明 `Unsupported`（能力不覆盖）时回退到下一个匹配的后端。
#[test]
fn unsupported_backend_falls_through_to_next() {
    let first = FakeJpx::new("partial", Outcome::NotSupported);
    let second = FakeJpx::new("full", Outcome::Good);
    let backends: [&dyn JpxBackend; 2] = [&first, &second];
    let request = jpx_request(b"jp2", None);
    let bitmap = decode_jpx_with(&backends, &request).expect("second backend must serve");
    assert_eq!(bitmap, ok_bitmap());
    assert_eq!(first.calls.load(Ordering::Relaxed), 1);
    assert_eq!(second.calls.load(Ordering::Relaxed), 1);
}

/// 全部后端 `Unsupported` 时透传最后一个类型化错误（不静默成功）。
#[test]
fn all_backends_unsupported_propagates_typed_error() {
    let only = FakeJpx::new("none", Outcome::NotSupported);
    let backends: [&dyn JpxBackend; 1] = [&only];
    let request = jpx_request(b"jp2", None);
    let error = decode_jpx_with(&backends, &request).expect_err("must not swallow");
    match error {
        ImageError::Unsupported(message) => assert!(message.contains("fake backend"), "{message}"),
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

/// 空码流不被契约层拦截：原样送达后端，由后端给出类型化错误。
#[test]
fn empty_codestream_is_delivered_verbatim() {
    let fake = FakeJpx::new("watcher", Outcome::Corrupt);
    let backends: [&dyn JpxBackend; 1] = [&fake];
    let request = jpx_request(b"", None);
    let error = decode_jpx_with(&backends, &request).expect_err("empty input must error");
    assert!(matches!(error, ImageError::Filter(_)), "got {error:?}");
    assert_eq!(fake.seen_len.load(Ordering::Relaxed), 0);
}

/// 后端产出形状违约（样本长度不符）被归一化为 `Invalid`，点名后端。
#[test]
fn malformed_backend_output_becomes_invalid_contract_error() {
    let fake = FakeJpx::new("shoddy", Outcome::BadShape);
    let backends: [&dyn JpxBackend; 1] = [&fake];
    let request = jpx_request(b"jp2", None);
    let error = decode_jpx_with(&backends, &request).expect_err("bad shape must error");
    match error {
        ImageError::Invalid(message) => {
            assert!(message.contains("shoddy"), "must name backend: {message}");
            assert!(message.contains("contract"), "must say contract: {message}");
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

/// JBIG2 门禁：声明版本低于 1.4 拒绝且后端零调用。
#[test]
fn jbig2_gate_rejects_pre_1_4_and_skips_backends() {
    let fake = FakeJbig2::new(Outcome::Good);
    let backends: [&dyn Jbig2Backend; 1] = [&fake];
    let request = jbig2_request(b"segs", Some(PdfVersion::V1_3));
    let error = decode_jbig2_with(&backends, &request).expect_err("1.3 must be gated");
    match error {
        ImageError::Unsupported(message) => {
            assert!(
                message.contains("JBIG2Decode") && message.contains("1.4"),
                "{message}"
            );
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
    assert_eq!(fake.calls.load(Ordering::Relaxed), 0);
}

/// JPX 门禁：1.4 拒绝、1.5 放行；JBIG2 在 1.4 恰好放行。
#[test]
fn version_gate_boundaries() {
    let too_old = PdfVersion::V1_4;
    let error = check_codec_version(ImageCodec::Jpx, Some(too_old)).expect_err("JPX needs 1.5");
    assert!(
        matches!(error, ImageError::Unsupported(_)),
        "JPX@1.4 got {error:?}"
    );
    assert!(check_codec_version(ImageCodec::Jpx, Some(PdfVersion::V1_5)).is_ok());
    assert!(check_codec_version(ImageCodec::Jbig2, Some(PdfVersion::V1_4)).is_ok());
    assert!(check_codec_version(ImageCodec::Jbig2, None).is_ok());
    assert_eq!(ImageCodec::Jbig2.min_pdf_version(), PdfVersion::V1_4);
    assert_eq!(ImageCodec::Jpx.min_pdf_version(), PdfVersion::V1_5);
}

/// 门禁放行后，截断码流仍走类型化 `Filter` 错误（JBIG2 侧对称覆盖）。
#[test]
fn jbig2_corrupt_segments_yield_filter_error() {
    let fake = FakeJbig2::new(Outcome::Corrupt);
    let backends: [&dyn Jbig2Backend; 1] = [&fake];
    let request = jbig2_request(&[0x30, 0x00], Some(PdfVersion::V1_7));
    let error = decode_jbig2_with(&backends, &request).expect_err("corrupt must error");
    assert!(matches!(error, ImageError::Filter(_)), "got {error:?}");
    assert_eq!(fake.calls.load(Ordering::Relaxed), 1);
}

/// `ImageModel::decode_image_with` 把注册集合接通到后端（JPX 路径）。
#[test]
fn model_dispatch_reaches_registered_backend() {
    let fake = FakeJpx::new("plumbed", Outcome::Good);
    let backends: [&dyn JpxBackend; 1] = [&fake];
    let model = image(&["JPXDecode"], b"\xFF\x4F".to_vec());
    let bitmap = model
        .decode_image_with(&ImageBackends::new(&[], &backends))
        .expect("registered backend must serve");
    assert_eq!(bitmap.width, 2);
    assert_eq!(fake.seen_len.load(Ordering::Relaxed), 2);
}

/// `DecodedBitmap` 形状自检覆盖零维、非法位深与行字节计算。
#[test]
fn bitmap_contract_checks_shape_parameters() {
    assert!(ok_bitmap().contract_violation().is_none());
    let zero = DecodedBitmap {
        width: 0,
        height: 1,
        components: 1,
        bits_per_component: 1,
        samples: Vec::new(),
        icc_profile: None,
    };
    assert!(zero.contract_violation().is_some());
    let bad_depth = DecodedBitmap {
        width: 1,
        height: 1,
        components: 1,
        bits_per_component: 3,
        samples: vec![0],
        icc_profile: None,
    };
    assert!(bad_depth.contract_violation().is_some());
    let wide = DecodedBitmap {
        width: 3,
        height: 1,
        components: 1,
        bits_per_component: 2,
        samples: vec![0],
        icc_profile: None,
    };
    assert_eq!(wide.row_bytes(), 1);
    assert!(wide.contract_violation().is_none());
}
