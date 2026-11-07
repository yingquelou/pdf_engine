//! 经解析器的 D3 编解码版本门禁集成测试：解析一个含 JBIG2/JPX 图像 XObject
//! 的最小 PDF（就地构造字节串，不新增二进制夹具），从 `Document` 入口调用
//! 解码，断言按 `source_version()` 判定的版本门禁与「后端是否被调用」的差别。

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use pdf_engine::core::reader::image::{
    DecodedBitmap, ImageBackends, ImageError, Jbig2Backend, Jbig2DecodeRequest, JpxBackend,
    JpxDecodeRequest,
};
use pdf_engine::core::types::{Document, PdfVersion, Value};

/// 记录调用次数与收到的 `pdf_version` 的 JBIG2 假后端（恒成功）。
struct CountingJbig2 {
    calls: AtomicUsize,
    seen: Mutex<Option<Option<PdfVersion>>>,
}

impl CountingJbig2 {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            seen: Mutex::new(None),
        }
    }
}

impl Jbig2Backend for CountingJbig2 {
    fn name(&self) -> &'static str {
        "counting-jbig2"
    }
    fn decode(&self, request: &Jbig2DecodeRequest<'_>) -> Result<DecodedBitmap, ImageError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        *self.seen.lock().unwrap() = Some(request.pdf_version);
        Ok(bitmap())
    }
}

/// 记录调用次数与收到的 `pdf_version` 的 JPX 假后端（恒成功）。
struct CountingJpx {
    calls: AtomicUsize,
    seen: Mutex<Option<Option<PdfVersion>>>,
}

impl CountingJpx {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            seen: Mutex::new(None),
        }
    }
}

impl JpxBackend for CountingJpx {
    fn name(&self) -> &'static str {
        "counting-jpx"
    }
    fn decode(&self, request: &JpxDecodeRequest<'_>) -> Result<DecodedBitmap, ImageError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        *self.seen.lock().unwrap() = Some(request.pdf_version);
        Ok(bitmap())
    }
}

fn bitmap() -> DecodedBitmap {
    DecodedBitmap {
        width: 2,
        height: 2,
        components: 1,
        bits_per_component: 1,
        samples: vec![0xAA, 0x55],
        icc_profile: None,
    }
}

/// 就地拼装一个含单个图像 XObject 的最小 PDF，头部版本 `1.{minor}`、
/// `/Filter` 为 `filter`，xref 偏移按字节位置精确计算。
fn build_pdf(minor: u8, filter: &str, data: &[u8]) -> Vec<u8> {
    let obj1 = b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";
    let obj2 = b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n";
    let obj3 = b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
                /Resources << /XObject << /Im1 4 0 R >> >> >>\nendobj\n";
    let mut obj4 = Vec::new();
    let img_head = format!(
        "4 0 obj\n<< /Type /XObject /Subtype /Image /Width 2 /Height 2 \
         /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /{filter} /Length {} >>\nstream\n",
        data.len()
    );
    obj4.extend_from_slice(img_head.as_bytes());
    obj4.extend_from_slice(data);
    obj4.extend_from_slice(b"\nendstream\nendobj\n");

    let mut buf = Vec::new();
    buf.extend_from_slice(format!("%PDF-1.{minor}\n").as_bytes());
    let off1 = buf.len();
    buf.extend_from_slice(obj1);
    let off2 = buf.len();
    buf.extend_from_slice(obj2);
    let off3 = buf.len();
    buf.extend_from_slice(obj3);
    let off4 = buf.len();
    buf.extend_from_slice(&obj4);
    let xref_off = buf.len();
    let xref = format!(
        "xref\n0 5\n0000000000 65535 f \n{off1:010} 00000 n \n{off2:010} 00000 n \
         \n{off3:010} 00000 n \n{off4:010} 00000 n \ntrailer\n<< /Size 5 /Root 1 0 R >>\n\
         startxref\n{xref_off}\n%%EOF\n"
    );
    buf.extend_from_slice(xref.as_bytes());
    buf
}

/// 取页面 `/Resources` 里命名图像 XObject 的引用值。
fn image_value(doc: &Document) -> Value {
    doc.page_resource(0, "XObject", "Im1")
        .expect("page resource lookup")
        .expect("Im1 present")
}

/// JBIG2：声明 1.3 经 Document 入口被门禁拒绝、后端零调用；
/// 恰好 1.4 放行、后端收到文档版本并成功。对比便捷路径（`None` 宽容）
/// 在 1.3 文档上仍放行，凸显版本注入的差异。
#[test]
fn jbig2_document_entry_gates_by_source_version() {
    // 1.3 —— 门禁拦截。
    let doc = crate::parse_buf(&build_pdf(3, "JBIG2Decode", b"seg-bytes"));
    assert_eq!(doc.source_version(), PdfVersion::V1_3);
    let fake = CountingJbig2::new();
    let backends: [&dyn Jbig2Backend; 1] = [&fake];
    let value = image_value(&doc);
    let error = doc
        .decode_image_with(&value, &ImageBackends::new(&backends, &[]))
        .expect_err("1.3 must gate out JBIG2 through the Document entry");
    match error {
        ImageError::Unsupported(message) => {
            assert!(
                message.contains("JBIG2Decode") && message.contains("1.4"),
                "{message}"
            );
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
    assert_eq!(
        fake.calls.load(Ordering::Relaxed),
        0,
        "gate must precede dispatch"
    );

    // 同一 1.3 文档上的便捷路径不注入版本 → 门禁放行、后端被调用。
    let convenience = CountingJbig2::new();
    let convenience_backends: [&dyn Jbig2Backend; 1] = [&convenience];
    let model = doc
        .page_image(0, "Im1")
        .expect("page image")
        .expect("Im1 image");
    model
        .decode_image_with(&ImageBackends::new(&convenience_backends, &[]))
        .expect("convenience path ignores the version gate (None is permissive)");
    assert_eq!(convenience.calls.load(Ordering::Relaxed), 1);
    assert_eq!(convenience.seen.lock().unwrap().unwrap(), None);

    // 1.4 —— 恰好边界放行，后端收到文档声明版本。
    let doc = crate::parse_buf(&build_pdf(4, "JBIG2Decode", b"seg-bytes"));
    assert_eq!(doc.source_version(), PdfVersion::V1_4);
    let ok_fake = CountingJbig2::new();
    let ok_backends: [&dyn Jbig2Backend; 1] = [&ok_fake];
    let value = image_value(&doc);
    let served = doc
        .decode_image_with(&value, &ImageBackends::new(&ok_backends, &[]))
        .expect("1.4 must pass the JBIG2 gate");
    assert_eq!(served, bitmap());
    assert_eq!(ok_fake.calls.load(Ordering::Relaxed), 1);
    assert_eq!(
        ok_fake.seen.lock().unwrap().unwrap(),
        Some(PdfVersion::V1_4),
        "后端必须收到文档 source_version"
    );
}

/// JPX：声明 1.4 经 Document 入口被门禁拒绝、后端零调用；
/// 恰好 1.5 放行、后端收到文档版本并成功。
#[test]
fn jpx_document_entry_gates_by_source_version() {
    let doc = crate::parse_buf(&build_pdf(4, "JPXDecode", b"JP2-payload"));
    assert_eq!(doc.source_version(), PdfVersion::V1_4);
    let fake = CountingJpx::new();
    let backends: [&dyn JpxBackend; 1] = [&fake];
    let value = image_value(&doc);
    let error = doc
        .decode_image_with(&value, &ImageBackends::new(&[], &backends))
        .expect_err("1.4 must gate out JPX through the Document entry");
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

    let doc = crate::parse_buf(&build_pdf(5, "JPXDecode", b"JP2-payload"));
    assert_eq!(doc.source_version(), PdfVersion::V1_5);
    let ok_fake = CountingJpx::new();
    let ok_backends: [&dyn JpxBackend; 1] = [&ok_fake];
    let value = image_value(&doc);
    let served = doc
        .decode_image_with(&value, &ImageBackends::new(&[], &ok_backends))
        .expect("1.5 must pass the JPX gate");
    assert_eq!(served, bitmap());
    assert_eq!(ok_fake.calls.load(Ordering::Relaxed), 1);
    assert_eq!(
        ok_fake.seen.lock().unwrap().unwrap(),
        Some(PdfVersion::V1_5)
    );
}
