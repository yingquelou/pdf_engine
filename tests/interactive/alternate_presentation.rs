//! 备用演示写出 → 解析 → 回读往返（ISO 32000-1 13.5、Table 297）：
//! `/Names /AlternatePresentations` 名称树、幻灯片字典、`/StartResource`
//! 与 `/Resources` 指向目录 `/Names /EmbeddedFiles` 中的文件规格。

use super::*;

/// 构建器写出幻灯片，解析后经读端 API 回读名称、起始资源与文件规格。
#[test]
fn builder_alternate_presentation_roundtrips_through_parser() {
    use pdf_engine::core::builder::{EmbeddedFile, PageSize, PdfBuilder, Slideshow};
    use pdf_engine::core::types::{PdfFeature, PdfVersion, Value};

    let payload = b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>".to_vec();

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::Letter);
    builder.embed_file(EmbeddedFile::new("slides.svg", payload.clone()));
    let show =
        Slideshow::new("MySlideShow", "slides.svg").with_resource("slides.svg", "slides.svg");
    builder.add_alternate_presentation(show);

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build alternate presentation");
    let doc = parse_buf(&buf);

    let files = doc.embedded_files().expect("embedded files");
    assert_eq!(files.len(), 1, "嵌入文件名称树应有一条");
    assert!(files.contains_key("slides.svg"));

    let shows = doc
        .alternate_presentations()
        .expect("alternate presentations");
    assert_eq!(shows.len(), 1, "应回读单个幻灯片");
    let show = &shows["MySlideShow"];
    assert_eq!(show.start_resource, "slides.svg");
    assert_eq!(
        show.dict.get("Type").and_then(Value::as_name),
        Some("SlideShow")
    );
    assert_eq!(
        show.dict.get("Subtype").and_then(Value::as_name),
        Some("Embedded")
    );
    assert_eq!(show.resources.len(), 1, "资源名称树应有一个键");

    let resource = &show.resources["slides.svg"];
    let filespec_dict = resource.as_dict().expect("资源值应解析为文件规格字典");
    assert_eq!(
        filespec_dict.get("Type").and_then(Value::as_name),
        Some("Filespec")
    );
    assert!(
        filespec_dict.get("EF").and_then(Value::as_dict).is_some(),
        "文件规格应带 /EF"
    );
    let filespec = doc.filespec(resource).expect("parse filespec");
    assert_eq!(
        filespec.primary().expect("embedded stream").data,
        payload,
        "嵌入文件字节应原样往返"
    );

    doc.load_all_objects().expect("load all objects");
    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(
        features.contains(&PdfFeature::AlternatePresentations),
        "特性画像应登记 AlternatePresentations"
    );
}
