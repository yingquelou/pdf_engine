//! 便携集合写出 → 解析 → 回读往返（12.3.5 Collections、Table 155–158）。

use super::*;

/// 写入 → 重新解析：模式（文本/文件名/大小字段，含 `/O` `/V` `/E`）、
/// `/D` 初始文档、`/View` 平铺与 `/Sort` 降序由读端 `collection` 还原，
/// 特性画像登记集合。
#[test]
fn builder_collection_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        Collection, CollectionField, CollectionFieldType, CollectionSort, CollectionView, PageSize,
        PdfBuilder,
    };
    use pdf_engine::core::reader::navigation::{
        CollectionFieldType as ReadFieldType, CollectionView as ReadView,
    };
    use pdf_engine::core::types::{PdfFeature, PdfVersion};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::A4);
    builder.collection(
        Collection::new()
            .with_schema(vec![
                CollectionField::new("title", CollectionFieldType::Text, "Title").with_order(0),
                CollectionField::new("file", CollectionFieldType::FileName, "File")
                    .with_visible(true)
                    .with_editable(true),
                CollectionField::new("size", CollectionFieldType::Size, "File size"),
            ])
            .with_initial_document("report.pdf")
            .with_view(CollectionView::Tiles)
            .with_sort(CollectionSort::new(vec!["size".into()]).with_ascending(vec![false])),
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build collection");

    let doc = parse_buf(&buf);
    let collection = doc
        .collection()
        .expect("read collection")
        .expect("collection present");
    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(features.contains(&PdfFeature::Collection));

    assert_eq!(collection.view, ReadView::Tiles);
    assert_eq!(
        collection.initial_document.as_deref(),
        Some(b"report.pdf".as_slice())
    );

    let schema = collection.schema.expect("schema");
    assert_eq!(schema.len(), 3);
    let title = &schema["title"];
    assert_eq!(title.field_type, ReadFieldType::Text);
    assert_eq!(title.name, "Title");
    assert_eq!(title.order, Some(0));
    assert_eq!(title.visible, None);
    assert_eq!(title.editable, None);

    let file = &schema["file"];
    assert_eq!(file.field_type, ReadFieldType::FileName);
    assert_eq!(file.name, "File");
    assert_eq!(file.order, None);
    assert_eq!(file.visible, Some(true));
    assert_eq!(file.editable, Some(true));

    let size = &schema["size"];
    assert_eq!(size.field_type, ReadFieldType::Size);
    assert_eq!(size.name, "File size");

    let sort = collection.sort.expect("sort");
    assert_eq!(sort.fields, vec!["size".to_string()]);
    assert_eq!(sort.ascending, vec![false]);
}
