//! 位图后端薄适配器的单元测试：全部用 `#[cfg(test)]` 假后端验证
//! 展平、上下文注入、版本门禁与错误形态透传；不含任何真实解码。

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::core::filters::{BitmapStreamContext, decode_bitmap_stream, decode_bitmap_stream_with};
use crate::core::reader::image::{
    DecodedBitmap, ImageBackends, ImageError, Jbig2Backend, Jbig2DecodeRequest,
    JpxBackend, JpxDecodeRequest,
};
use crate::core::types::{Array, Dict, PdfVersion, Value};

/// 3×2、1 分量、8bpp 的合法位图：row_bytes=3、samples 共 6 字节、行优先。
fn good_bitmap() -> DecodedBitmap {
    DecodedBitmap {
        width: 3,
        height: 2,
        components: 1,
        bits_per_component: 8,
        samples: vec![10, 20, 30, 40, 50, 60],
        icc_profile: None,
    }
}

/// 形状违约位图：声明 3×2@8bpp 却只带回 1 字节样本。
fn bad_shape_bitmap() -> DecodedBitmap {
    DecodedBitmap {
        width: 3,
        height: 2,
        components: 1,
        bits_per_component: 8,
        samples: vec![10],
        icc_profile: None,
    }
}

#[derive(Clone, Copy)]
enum Outcome {
    Good,
    Corrupt,
    NotSupported,
    BadShape,
}

fn outcome_bitmap(outcome: Outcome, name: &str) -> std::result::Result<DecodedBitmap, ImageError> {
    match outcome {
        Outcome::Good => Ok(good_bitmap()),
        Outcome::Corrupt => Err(ImageError::Filter(format!(
            "{name}: codestream truncated at segment header"
        ))),
        Outcome::NotSupported => Err(ImageError::Unsupported(
            "fake backend covers none of this codestream".to_owned(),
        )),
        Outcome::BadShape => Ok(bad_shape_bitmap()),
    }
}

/// 观察请求上下文（码流、globals、页信息段）的假 JBIG2 后端。
struct FakeJbig2 {
    outcome: Outcome,
    calls: AtomicUsize,
    seen_codestream: Mutex<Vec<u8>>,
    saw_globals: AtomicUsize,
    saw_page_information: AtomicUsize,
}

impl FakeJbig2 {
    fn new(outcome: Outcome) -> Self {
        Self {
            outcome,
            calls: AtomicUsize::new(0),
            seen_codestream: Mutex::new(Vec::new()),
            saw_globals: AtomicUsize::new(usize::MAX),
            saw_page_information: AtomicUsize::new(usize::MAX),
        }
    }
}

impl Jbig2Backend for FakeJbig2 {
    fn name(&self) -> &'static str {
        "fake-jbig2-filter"
    }

    fn decode(
        &self,
        request: &Jbig2DecodeRequest<'_>,
    ) -> std::result::Result<DecodedBitmap, ImageError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        *self.seen_codestream.lock().unwrap() = request.codestream.to_vec();
        self.saw_globals
            .store(usize::from(request.globals_raw.is_some()), Ordering::Relaxed);
        self.saw_page_information.store(
            usize::from(request.page_information.is_some()),
            Ordering::Relaxed,
        );
        outcome_bitmap(self.outcome, self.name())
    }
}

/// 观察请求上下文（码流、JPX 头部几何）的假 JPX 后端。
struct FakeJpx {
    outcome: Outcome,
    calls: AtomicUsize,
    seen_codestream: Mutex<Vec<u8>>,
    seen_header: Mutex<Option<(usize, usize, usize, usize)>>,
}

impl FakeJpx {
    fn new(outcome: Outcome) -> Self {
        Self {
            outcome,
            calls: AtomicUsize::new(0),
            seen_codestream: Mutex::new(Vec::new()),
            seen_header: Mutex::new(None),
        }
    }
}

impl JpxBackend for FakeJpx {
    fn name(&self) -> &'static str {
        "fake-jpx-filter"
    }

    fn decode(
        &self,
        request: &JpxDecodeRequest<'_>,
    ) -> std::result::Result<DecodedBitmap, ImageError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        *self.seen_codestream.lock().unwrap() = request.codestream.to_vec();
        *self.seen_header.lock().unwrap() = request.header.as_ref().map(|header| {
            (
                header.width as usize,
                header.height as usize,
                header.component_count(),
                header.bits_per_component().unwrap_or(0) as usize,
            )
        });
        outcome_bitmap(self.outcome, self.name())
    }
}

fn bitmap_dict(names: &[&str]) -> Dict {
    let filter = if names.len() == 1 {
        Value::Name(names[0].to_owned())
    } else {
        Value::Array(names.iter().fold(Array::new(), |array, name| {
            array.push(Value::Name((*name).to_owned()))
        }))
    };
    Dict::new().insert("Filter", filter)
}

/// 裸 JPEG 2000 码流：SOC + SIZ（2×1、1 分量、8bpp），供头部语法兜底。
fn jpx_codestream_2x1() -> Vec<u8> {
    let mut data = vec![0xFF, 0x4F]; // SOC
    data.extend_from_slice(&[0xFF, 0x51, 0x00, 0x29]); // SIZ，Lsiz=41
    data.extend_from_slice(&[0x00, 0x00]); // SizId / Rsiz
    for value in [2u32, 1, 0, 0, 2, 1, 0, 0] {
        // Xsiz Ysiz XOsiz YOsiz XTsiz YTsiz XTOsiz YTOsiz
        data.extend_from_slice(&value.to_be_bytes());
    }
    data.extend_from_slice(&1u16.to_be_bytes()); // Csiz=1
    data.extend_from_slice(&[0x07, 0x01, 0x01]); // SSiz(8bpp 无符号), XRsiz, YRsiz
    data
}

/// 注册假位图后端后，过滤器入口把 `DecodedBitmap.samples` 按行优先原样
/// 展平为 `Vec<u8>`（长度 = row_bytes × height，与宽高通道描述一致）。
#[test]
fn registered_jbig2_backend_flattens_row_major_bitmap_through_filters_entry() {
    let fake = FakeJbig2::new(Outcome::Good);
    let refs: [&dyn Jbig2Backend; 1] = [&fake];
    let backends = ImageBackends::new(&refs, &[]);
    let codestream = b"\x30\x00\x00\x00\x00\x00\x02\x00\x00\x00";
    let samples = decode_bitmap_stream(&bitmap_dict(&["JBIG2Decode"]), codestream, &backends)
        .expect("registered backend must serve");
    assert_eq!(samples, vec![10, 20, 30, 40, 50, 60], "展平必须逐行拼接");
    assert_eq!(fake.calls.load(Ordering::Relaxed), 1);
    assert_eq!(
        *fake.seen_codestream.lock().unwrap(),
        codestream,
        "后端必须收到原始码流字节"
    );
    assert_eq!(fake.saw_globals.load(Ordering::Relaxed), 0);
    assert_eq!(fake.saw_page_information.load(Ordering::Relaxed), 0);
}

/// JPX 路径：字典与码流足以给出几何时，适配器就地解析头部语法并把
/// `JpxHeader` 上下文送达后端。
#[test]
fn registered_jpx_backend_receives_parsed_header_geometry_from_codestream() {
    let fake = FakeJpx::new(Outcome::Good);
    let refs: [&dyn JpxBackend; 1] = [&fake];
    let backends = ImageBackends::new(&[], &refs);
    let codestream = jpx_codestream_2x1();
    let samples =
        decode_bitmap_stream(&bitmap_dict(&["JPXDecode"]), &codestream, &backends).expect("jpx ok");
    assert_eq!(samples, vec![10, 20, 30, 40, 50, 60]);
    assert_eq!(*fake.seen_codestream.lock().unwrap(), codestream);
    assert_eq!(
        *fake.seen_header.lock().unwrap(),
        Some((2, 1, 1, 8)),
        "头部上下文应为 2x1、1 分量、8bpp"
    );
}

/// 未注册任何后端（默认 `ImageBackends::NONE` 口径）时，两条路径都返回
/// 既有类型化 `Unsupported`，绝不塞入伪解码。
#[test]
fn unregistered_backends_return_typed_unsupported_for_both_codecs() {
    for (name, marker) in [("JBIG2Decode", "JBIG2Decode"), ("JPXDecode", "JPXDecode")] {
        let error = decode_bitmap_stream(&bitmap_dict(&[name]), b"payload", &ImageBackends::NONE)
            .expect_err("no backend must error");
        match error {
            ImageError::Unsupported(message) => {
                assert!(message.contains(marker), "{message}");
                assert!(
                    message.contains("pixel backend registered"),
                    "必须是「未注册后端」口径: {message}"
                );
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }
}

/// PDF 1.4/1.5 之前的旧名 `JBIG2`/`JPX` 同样命中位图分发（与图像读端
/// `is_jbig2_filter`/`is_jpx_filter` 同一口径）。
#[test]
fn legacy_filter_aliases_jbig2_and_jpx_reach_bitmap_backend() {
    let jbig2_fake = FakeJbig2::new(Outcome::Good);
    let jbig2_refs: [&dyn Jbig2Backend; 1] = [&jbig2_fake];
    let jbig2_backends = ImageBackends::new(&jbig2_refs, &[]);
    assert_eq!(
        decode_bitmap_stream(&bitmap_dict(&["JBIG2"]), b"segs", &jbig2_backends)
            .expect("legacy JBIG2 alias must dispatch"),
        vec![10, 20, 30, 40, 50, 60]
    );
    let jpx_fake = FakeJpx::new(Outcome::Good);
    let jpx_refs: [&dyn JpxBackend; 1] = [&jpx_fake];
    let jpx_backends = ImageBackends::new(&[], &jpx_refs);
    assert_eq!(
        decode_bitmap_stream(&bitmap_dict(&["JPX"]), b"jp2", &jpx_backends)
            .expect("legacy JPX alias must dispatch"),
        vec![10, 20, 30, 40, 50, 60]
    );
}

/// 版本门禁拦在任何后端被调用之前：1.3 文档上下文里的 JBIG2、1.4 文档
/// 上下文里的 JPX 都返回类型化 `Unsupported` 且后端零调用。
#[test]
fn version_gate_blocks_backend_before_any_call() {
    let jbig2_fake = FakeJbig2::new(Outcome::Good);
    let jbig2_refs: [&dyn Jbig2Backend; 1] = [&jbig2_fake];
    let context = BitmapStreamContext {
        pdf_version: Some(PdfVersion::V1_3),
        ..Default::default()
    };
    let error = decode_bitmap_stream_with(
        &bitmap_dict(&["JBIG2Decode"]),
        b"segs",
        &ImageBackends::new(&jbig2_refs, &[]),
        &context,
    )
    .expect_err("1.3 must gate JBIG2");
    match error {
        ImageError::Unsupported(message) => {
            assert!(
                message.contains("JBIG2Decode") && message.contains("1.4"),
                "{message}"
            );
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
    assert_eq!(jbig2_fake.calls.load(Ordering::Relaxed), 0);

    let jpx_fake = FakeJpx::new(Outcome::Good);
    let jpx_refs: [&dyn JpxBackend; 1] = [&jpx_fake];
    let context = BitmapStreamContext {
        pdf_version: Some(PdfVersion::V1_4),
        ..Default::default()
    };
    let error = decode_bitmap_stream_with(
        &bitmap_dict(&["JPXDecode"]),
        b"jp2",
        &ImageBackends::new(&[], &jpx_refs),
        &context,
    )
    .expect_err("1.4 must gate JPX");
    assert!(
        matches!(error, ImageError::Unsupported(_)),
        "JPX@1.4 got {error:?}"
    );
    assert_eq!(jpx_fake.calls.load(Ordering::Relaxed), 0);
}

/// 门禁放行后 `pdf_version` 注入照常解码并展平（1.5 上下文里的 JPX 成功）。
#[test]
fn admitted_version_flows_to_backend_and_flattens_samples() {
    let jpx_fake = FakeJpx::new(Outcome::Good);
    let jpx_refs: [&dyn JpxBackend; 1] = [&jpx_fake];
    let context = BitmapStreamContext {
        pdf_version: Some(PdfVersion::V1_5),
        ..Default::default()
    };
    let samples = decode_bitmap_stream_with(
        &bitmap_dict(&["JPXDecode"]),
        b"jp2",
        &ImageBackends::new(&[], &jpx_refs),
        &context,
    )
    .expect("1.5 admits JPX");
    assert_eq!(samples, vec![10, 20, 30, 40, 50, 60]);
    assert_eq!(jpx_fake.calls.load(Ordering::Relaxed), 1);
}

/// 后端交回违反 `DecodedBitmap` 契约的位图时，由分发层归一化为 `Invalid`
/// 并点名后端，适配器不 panic、不扩散脏数据。
#[test]
fn contract_violating_backend_output_becomes_normalized_invalid_error() {
    let fake = FakeJbig2::new(Outcome::BadShape);
    let refs: [&dyn Jbig2Backend; 1] = [&fake];
    let error = decode_bitmap_stream(
        &bitmap_dict(&["JBIG2Decode"]),
        b"segs",
        &ImageBackends::new(&refs, &[]),
    )
    .expect_err("bad shape must error");
    match error {
        ImageError::Invalid(message) => {
            assert!(
                message.contains("fake-jbig2-filter"),
                "必须点名后端: {message}"
            );
            assert!(message.contains("contract"), "必须说明契约违约: {message}");
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

/// 后端声明码流损坏（`Filter`）原样透传、不回退；声明能力不覆盖
/// （`Unsupported`）在无后继后端时同样透传最后类型化错误。
#[test]
fn backend_errors_pass_through_verbatim_without_retry() {
    let corrupt = FakeJbig2::new(Outcome::Corrupt);
    let refs: [&dyn Jbig2Backend; 1] = [&corrupt];
    let error = decode_bitmap_stream(
        &bitmap_dict(&["JBIG2Decode"]),
        b"segs",
        &ImageBackends::new(&refs, &[]),
    )
    .expect_err("corrupt codestream must error");
    match error {
        ImageError::Filter(message) => {
            assert!(
                message.contains("fake-jbig2-filter: codestream truncated"),
                "消息必须逐字透传: {message}"
            );
        }
        other => panic!("expected Filter, got {other:?}"),
    }
    assert_eq!(
        corrupt.calls.load(Ordering::Relaxed),
        1,
        "Filter 错误不得重试"
    );

    let partial = FakeJpx::new(Outcome::NotSupported);
    let jpx_refs: [&dyn JpxBackend; 1] = [&partial];
    let error = decode_bitmap_stream(
        &bitmap_dict(&["JPXDecode"]),
        b"jp2",
        &ImageBackends::new(&[], &jpx_refs),
    )
    .expect_err("unsupported must error");
    match error {
        ImageError::Unsupported(message) => {
            assert!(message.contains("fake backend"), "{message}");
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

/// `/Filter` 链里位图步骤之前的其余步骤照常用内建过滤器解码，位图步骤
/// 原样透传成码流（与图像读端 `decode_stream_except` 同一口径）。
#[test]
fn earlier_filter_stages_decode_before_bitmap_backend() {
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    let raw = b"raw-jbig2-segments";
    let mut encoder = ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(raw).unwrap();
    let compressed = encoder.finish().unwrap();

    let fake = FakeJbig2::new(Outcome::Good);
    let refs: [&dyn Jbig2Backend; 1] = [&fake];
    let dict = bitmap_dict(&["FlateDecode", "JBIG2Decode"]);
    let samples = decode_bitmap_stream(&dict, &compressed, &ImageBackends::new(&refs, &[]))
        .expect("flate step must run before bitmap dispatch");
    assert_eq!(samples, vec![10, 20, 30, 40, 50, 60]);
    assert_eq!(
        *fake.seen_codestream.lock().unwrap(),
        raw,
        "后端必须收到 flate 解开的码流"
    );
}

/// `/JBIG2Globals` 只有 `Document` 解引用才拿得到，经上下文字段注入后
/// 送达后端；缺省（`None`）时后端按契约自行从码流读取。
#[test]
fn jbig2_globals_injected_through_context_reach_backend() {
    let globals = b"\x00\x00\x00\x00\x00\x01\x00\x00\x00\x00\x00".to_vec();
    let fake = FakeJbig2::new(Outcome::Good);
    let refs: [&dyn Jbig2Backend; 1] = [&fake];
    let backends = ImageBackends::new(&refs, &[]);
    let context = BitmapStreamContext {
        pdf_version: None,
        jbig2_globals: Some(&globals),
    };
    decode_bitmap_stream_with(&bitmap_dict(&["JBIG2Decode"]), b"segs", &backends, &context)
        .expect("injected globals must ride the request");
    assert_eq!(fake.saw_globals.load(Ordering::Relaxed), 1);

    decode_bitmap_stream(&bitmap_dict(&["JBIG2Decode"]), b"segs", &backends)
        .expect("default context still dispatches");
    assert_eq!(
        fake.saw_globals.load(Ordering::Relaxed),
        0,
        "缺省上下文不得带 globals"
    );
}

/// 非位图过滤器流不是本适配器的对象：返回类型化 `Unsupported` 并指回
/// `filters::decode_stream`，后端零调用、行为与 `decode_stream` 互不侵犯。
#[test]
fn non_bitmap_stream_directs_caller_back_to_decode_stream() {
    let fake = FakeJbig2::new(Outcome::Good);
    let refs: [&dyn Jbig2Backend; 1] = [&fake];
    let error = decode_bitmap_stream(
        &bitmap_dict(&["FlateDecode"]),
        b"not-a-bitmap-step",
        &ImageBackends::new(&refs, &[]),
    )
    .expect_err("plain stream must not reach bitmap dispatch");
    match error {
        ImageError::Unsupported(message) => {
            assert!(message.contains("decode_stream"), "got: {message}");
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
    assert_eq!(fake.calls.load(Ordering::Relaxed), 0);
}
