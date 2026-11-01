//! 查看器首选项集成测试（ISO 32000-1 12.2，Table 150）：builder 写出目录
//! `/ViewerPreferences` → `parse_buf` 重解析 → `Document::viewer_preferences()`
//! 类型化回读一致；页面文本提取与特征画像不受干扰；原始字节含声明的条目；
//! 目标版本过低时零字节输出。

use super::*;

#[test]
fn builder_viewer_preferences_roundtrip_through_parser_pdf_1_7() {
    use pdf_engine::core::builder::{
        Duplex, NonFullScreenPageMode, PageSize, PdfBuilder, PrintScaling, ReadingDirection,
        ViewerPreferences,
    };
    use pdf_engine::core::types::{PdfFeature, PdfVersion};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::Letter);
    builder.text(
        page,
        &pdf_engine::core::builder::Font::helvetica(12.0),
        72.0,
        700.0,
        "ViewerPrefs",
    );
    builder.set_viewer_preferences(
        ViewerPreferences::new()
            .with_hide_toolbar(true)
            .with_non_full_screen_page_mode(NonFullScreenPageMode::UseOC)
            .with_direction(ReadingDirection::RightToLeft)
            .with_display_doc_title(true)
            .with_view_area(pdf_engine::core::builder::PageBoundaryName::TrimBox)
            .with_print_scaling(PrintScaling::None)
            .with_duplex(Duplex::DuplexFlipShortEdge)
            .with_print_page_range(vec![(1, 2), (5, 8)])
            .with_num_copies(2),
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build viewer preferences");

    // 原始字节应含字典与已声明条目。
    let raw = String::from_utf8_lossy(&buf);
    assert!(
        raw.contains("/ViewerPreferences"),
        "字节中应有 /ViewerPreferences"
    );
    assert!(raw.contains("/HideToolbar true"));
    assert!(raw.contains("/Direction /R2L"));
    assert!(raw.contains("/PrintPageRange [1 2 5 8]"));

    let doc = parse_buf(&buf);
    doc.load_all_objects().expect("load all objects");
    let prefs = doc
        .viewer_preferences()
        .expect("读取类型化")
        .expect("字典存在");
    assert!(prefs.hide_toolbar);
    assert!(!prefs.fit_window, "未声明布尔缺省 false");
    assert_eq!(
        prefs.non_full_screen_page_mode,
        NonFullScreenPageMode::UseOC
    );
    assert_eq!(prefs.direction, ReadingDirection::RightToLeft);
    assert!(prefs.display_doc_title);
    assert_eq!(
        prefs.view_area,
        pdf_engine::core::builder::PageBoundaryName::TrimBox
    );
    assert_eq!(prefs.print_scaling, PrintScaling::None);
    assert_eq!(prefs.duplex, Some(Duplex::DuplexFlipShortEdge));
    assert_eq!(prefs.print_page_range, vec![(1, 2), (5, 8)]);
    assert_eq!(prefs.num_copies, Some(2));

    // 页面文本提取与特征画像不受首选项影响。
    assert_eq!(doc.extract_page_text(0).as_deref(), Some("ViewerPrefs"));
    let used: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(used.contains(&PdfFeature::ViewerPreferences));
    assert!(used.contains(&PdfFeature::ViewerPrintDialogPreferences));
}

#[test]
fn builder_viewer_preferences_roundtrip_through_parser_pdf_1_4() {
    use pdf_engine::core::builder::{
        PageBoundaryName, PageSize, PdfBuilder, ReadingDirection, ViewerPreferences,
    };
    use pdf_engine::core::types::{PdfFeature, PdfVersion};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_4);
    builder.page(PageSize::A4);
    builder.set_viewer_preferences(
        ViewerPreferences::new()
            .with_center_window(true)
            .with_direction(ReadingDirection::LeftToRight)
            .with_display_doc_title(true)
            .with_print_clip(PageBoundaryName::BleedBox),
    );

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build 1.4 viewer preferences");
    let doc = parse_buf(&buf);
    doc.load_all_objects().expect("load all objects");
    let prefs = doc
        .viewer_preferences()
        .expect("读取类型化")
        .expect("字典存在");
    assert!(prefs.center_window);
    assert!(prefs.display_doc_title);
    assert_eq!(prefs.direction, ReadingDirection::LeftToRight);
    assert_eq!(prefs.print_clip, PageBoundaryName::BleedBox);
    assert_eq!(prefs.view_clip, PageBoundaryName::CropBox, "缺省 CropBox");
    let used: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(used.contains(&PdfFeature::ViewerDisplayDocTitle));
    assert!(used.contains(&PdfFeature::ViewerPageBoundaryNames));
    assert!(
        !used.contains(&PdfFeature::ViewerPrintScaling),
        "未声明档位不登记"
    );
}

#[test]
fn builder_viewer_preferences_too_old_target_writes_zero_bytes() {
    use pdf_engine::core::builder::{PageSize, PdfBuilder, ViewerPreferences};
    use pdf_engine::core::types::PdfVersion;

    // 1.7-only 条目（/NumCopies）在 1.4 目标下：UnsupportedVersion + 零字节。
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_4);
    builder.page(PageSize::A4);
    builder.set_viewer_preferences(ViewerPreferences::new().with_num_copies(4));
    let mut buf = Vec::new();
    let error = builder
        .build(&mut buf)
        .expect_err("1.7 条目在 1.4 目标应被拒绝");
    assert!(
        matches!(
            error,
            pdf_engine::core::writer::WriteError::UnsupportedVersion(_)
        ),
        "{error}"
    );
    assert!(buf.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn builder_viewer_preferences_defaults_keep_page_text_and_features_unaffected() {
    use pdf_engine::core::builder::{PageSize, PdfBuilder, ViewerPreferences};
    use pdf_engine::core::types::{PdfFeature, PdfVersion};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::Letter);
    builder.text(
        page,
        &pdf_engine::core::builder::Font::helvetica(10.0),
        40.0,
        40.0,
        "Plain",
    );
    builder.set_viewer_preferences(ViewerPreferences::new().with_fit_window(true));
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    doc.load_all_objects().expect("load all objects");
    assert_eq!(doc.extract_page_text(0).as_deref(), Some("Plain"));
    assert_eq!(doc.page_count().expect("page count"), 1);
    let used: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(used.contains(&PdfFeature::ViewerPreferences));
    assert!(!used.contains(&PdfFeature::ViewerReadingDirection));
}
