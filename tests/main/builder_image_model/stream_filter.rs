//! 位图后端薄适配器的集成测试：手工构造含 `/JBIG2Decode`/`/JPXDecode`
//! 图像流的最小 PDF → 真实 `Parser` 回读 → 经 `filters` 层入口解码并
//! 断言展平字节；版本门禁取解析器填充的 `source_version`，未注册后端
//! 维持类型化 `Unsupported`。全部用假后端，不含真实像素解码。

use super::*;
use flate2::write::ZlibEncoder;
use pdf_engine::core::filters::{
    BitmapStreamContext, decode_bitmap_stream, decode_bitmap_stream_with,
};
use pdf_engine::core::reader::image::{
    DecodedBitmap, ImageBackends, ImageError, Jbig2Backend, Jbig2DecodeRequest, JpxBackend,
    JpxDecodeRequest,
};
use pdf_engine::core::types::{Dict, PdfVersion, Value};
use std::io::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 3×2、1 分量、8bpp 的合法位图展平结果（6 字节、行优先）。
fn expected_samples() -> Vec<u8> {
    vec![111, 112, 113, 121, 122, 123]
}

struct FakeJbig2 {
    calls: AtomicUsize,
    seen_codestream: Mutex<Vec<u8>>,
}

impl FakeJbig2 {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            seen_codestream: Mutex::new(Vec::new()),
        }
    }
}

impl Jbig2Backend for FakeJbig2 {
    fn name(&self) -> &'static str {
        "fake-jbig2-integration"
    }

    fn decode(
        &self,
        request: &Jbig2DecodeRequest<'_>,
    ) -> std::result::Result<DecodedBitmap, ImageError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        *self.seen_codestream.lock().unwrap() = request.codestream.to_vec();
        Ok(DecodedBitmap {
            width: 3,
            height: 2,
            components: 1,
            bits_per_component: 8,
            samples: expected_samples(),
        })
    }
}

struct FakeJpx {
    calls: AtomicUsize,
    seen_codestream: Mutex<Vec<u8>>,
}

impl FakeJpx {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            seen_codestream: Mutex::new(Vec::new()),
        }
    }
}

impl JpxBackend for FakeJpx {
    fn name(&self) -> &'static str {
        "fake-jpx-integration"
    }

    fn decode(
        &self,
        request: &JpxDecodeRequest<'_>,
    ) -> std::result::Result<DecodedBitmap, ImageError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        *self.seen_codestream.lock().unwrap() = request.codestream.to_vec();
        Ok(DecodedBitmap {
            width: 3,
            height: 2,
            components: 1,
            bits_per_component: 8,
            samples: expected_samples(),
        })
    }
}

/// 经典交叉引用表的最小 PDF：对象 1 Catalog、2 Pages、3 Page（资源
/// `/Im1`）、4 图像流；`/Filter` 取给定写法，载荷原样写入流。
fn pdf_with_bitmap_image(version: &str, filter: &str, payload: &[u8]) -> Vec<u8> {
    fn push(buf: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]) {
        offsets.push(buf.len());
        buf.extend_from_slice(body);
        buf.extend_from_slice(b"\n");
    }
    let mut buf = Vec::new();
    let mut offsets: Vec<usize> = Vec::new();
    buf.extend_from_slice(format!("%PDF-{version}\n").as_bytes());
    push(
        &mut buf,
        &mut offsets,
        b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj",
    );
    push(
        &mut buf,
        &mut offsets,
        b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj",
    );
    push(
        &mut buf,
        &mut offsets,
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
          /Resources << /XObject << /Im1 4 0 R >> >> >>\nendobj",
    );
    let mut image = format!(
        "4 0 obj\n<< /Type /XObject /Subtype /Image /Width 3 /Height 2 \
         /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter {filter} \
         /Length {} >>\nstream\n",
        payload.len()
    )
    .into_bytes();
    image.extend_from_slice(payload);
    image.extend_from_slice(b"\nendstream\nendobj");
    push(&mut buf, &mut offsets, &image);
    let xref_offset = buf.len();
    let mut xref = b"xref\n0 5\n0000000000 65535 f \n".to_vec();
    for offset in &offsets {
        xref.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    xref.extend_from_slice(
        format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes(),
    );
    buf.extend_from_slice(&xref);
    buf
}

/// 取对象 4（图像流）的流字典与未经过滤器处理的原始字节。
fn image_stream(doc: &Document) -> (Dict, Vec<u8>) {
    let object = doc
        .get_object(4)
        .expect("get object")
        .expect("image object exists");
    let Value::Stream(dict, _) = &object.value else {
        panic!("object 4 must be a stream");
    };
    let raw = doc
        .stream_bytes(&object.value)
        .expect("raw stream bytes readable");
    (dict.clone(), raw)
}

/// 解析器真实回读的 1.4 文档里，注册假 JBIG2 后端后经过滤器入口拿到
/// 展平样本；便捷入口（无版本注入）结果一致、后端收到原始码流字节。
#[test]
fn jbig2_bitmap_stream_from_parsed_document_flattens_registered_backend_samples() {
    let pdf = pdf_with_bitmap_image("1.4", "/JBIG2Decode", b"jbig2-segment-payload");
    let doc = parse_buf(&pdf);
    assert_eq!(doc.source_version(), PdfVersion::V1_4);
    let (dict, raw) = image_stream(&doc);

    let fake = FakeJbig2::new();
    let refs: [&dyn Jbig2Backend; 1] = [&fake];
    let backends = ImageBackends::new(&refs, &[]);
    let context = BitmapStreamContext {
        pdf_version: Some(doc.source_version()),
        ..Default::default()
    };
    let samples = decode_bitmap_stream_with(&dict, &raw, &backends, &context)
        .expect("PDF 1.4 admits JBIG2Decode");
    assert_eq!(samples, expected_samples());
    assert_eq!(
        *fake.seen_codestream.lock().unwrap(),
        b"jbig2-segment-payload",
        "后端必须收到解析器读出的原始段字节"
    );

    assert_eq!(
        decode_bitmap_stream(&dict, &raw, &backends).expect("convenience entry agrees"),
        expected_samples()
    );
    assert_eq!(fake.calls.load(Ordering::Relaxed), 2);
}

/// 解析器填充的 1.3 文档版本经上下文注入后，门禁拦在假后端之前：
/// 类型化 `Unsupported`、后端零调用。
#[test]
fn jbig2_bitmap_stream_is_gated_by_parsed_document_source_version() {
    let pdf = pdf_with_bitmap_image("1.3", "/JBIG2Decode", b"jbig2-segment-payload");
    let doc = parse_buf(&pdf);
    assert_eq!(doc.source_version(), PdfVersion::V1_3);
    let (dict, raw) = image_stream(&doc);

    let fake = FakeJbig2::new();
    let refs: [&dyn Jbig2Backend; 1] = [&fake];
    let backends = ImageBackends::new(&refs, &[]);
    let context = BitmapStreamContext {
        pdf_version: Some(doc.source_version()),
        ..Default::default()
    };
    let error = decode_bitmap_stream_with(&dict, &raw, &backends, &context)
        .expect_err("1.3 must gate JBIG2Decode");
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

/// 1.5 文档、`/Filter [/FlateDecode /JPXDecode]` 的真实图像流：前序
/// flate 步骤在过滤器层照常解码，JPX 步骤原样透传给注册的假后端。
#[test]
fn jpx_bitmap_stream_with_flate_prefix_from_parsed_document_flattens_samples() {
    let raw_codestream = b"jp2-codestream-bytes";
    let mut encoder = ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(raw_codestream).unwrap();
    let compressed = encoder.finish().unwrap();

    let pdf = pdf_with_bitmap_image("1.5", "[/FlateDecode /JPXDecode]", &compressed);
    let doc = parse_buf(&pdf);
    assert_eq!(doc.source_version(), PdfVersion::V1_5);
    let (dict, stored) = image_stream(&doc);
    assert_eq!(stored, compressed, "流字典按 /Length 读出的应是压缩字节");

    let fake = FakeJpx::new();
    let refs: [&dyn JpxBackend; 1] = [&fake];
    let backends = ImageBackends::new(&[], &refs);
    let context = BitmapStreamContext {
        pdf_version: Some(doc.source_version()),
        ..Default::default()
    };
    let samples =
        decode_bitmap_stream_with(&dict, &stored, &backends, &context).expect("1.5 admits JPX");
    assert_eq!(samples, expected_samples());
    assert_eq!(
        *fake.seen_codestream.lock().unwrap(),
        raw_codestream,
        "flate 解开的码流必须逐字送达后端"
    );
    assert_eq!(fake.calls.load(Ordering::Relaxed), 1);
}

/// 解析器回读的 1.4 文档里 `/JPXDecode` 被 1.5 版本门拦截、后端零调用；
/// 同一文档在无后端注册时维持「未注册」类型化 `Unsupported`。
#[test]
fn jpx_bitmap_stream_is_gated_by_parsed_1_4_document_version() {
    let pdf = pdf_with_bitmap_image("1.4", "/JPXDecode", b"jp2-bytes");
    let doc = parse_buf(&pdf);
    assert_eq!(doc.source_version(), PdfVersion::V1_4);
    let (dict, raw) = image_stream(&doc);

    let fake = FakeJpx::new();
    let refs: [&dyn JpxBackend; 1] = [&fake];
    let backends = ImageBackends::new(&[], &refs);
    let context = BitmapStreamContext {
        pdf_version: Some(doc.source_version()),
        ..Default::default()
    };
    let error = decode_bitmap_stream_with(&dict, &raw, &backends, &context)
        .expect_err("1.4 must gate JPXDecode");
    match error {
        ImageError::Unsupported(message) => {
            assert!(
                message.contains("JPXDecode") && message.contains("1.5"),
                "{message}"
            );
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
    assert_eq!(fake.calls.load(Ordering::Relaxed), 0);

    let error = decode_bitmap_stream(&dict, &raw, &ImageBackends::NONE)
        .expect_err("default registry must not decode pixels");
    assert!(
        matches!(error, ImageError::Unsupported(_)),
        "未注册后端必须保持类型化 Unsupported, got {error:?}"
    );
}

/// 解析器回读的 1.4 文档、空注册表：`/JBIG2Decode` 走过滤器入口返回
/// 「no JBIG2Decode pixel backend registered」的类型化 `Unsupported`，
/// 不为测试塞入伪解码。
#[test]
fn unregistered_backends_on_parsed_bitmap_document_return_typed_unsupported() {
    let pdf = pdf_with_bitmap_image("1.4", "/JBIG2Decode", b"jbig2-segment-payload");
    let doc = parse_buf(&pdf);
    let (dict, raw) = image_stream(&doc);
    let error = decode_bitmap_stream(&dict, &raw, &ImageBackends::NONE)
        .expect_err("ImageBackends::NONE must stay unsupported");
    match error {
        ImageError::Unsupported(message) => {
            assert!(
                message.contains("no JBIG2Decode pixel backend registered"),
                "{message}"
            );
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
}
