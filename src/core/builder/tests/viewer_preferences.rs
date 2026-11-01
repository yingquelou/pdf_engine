//! 查看器首选项写端测试（ISO 32000-1 12.2，Table 150）：目录
//! `/ViewerPreferences` 物化、缺省省略、取值契约（库策略边界零字节拒绝）、
//! 逐档位版本门禁与特征画像登记。

use super::*;
use crate::core::types::PdfFeature;

fn viewer_preferences_object(doc: &Document) -> Dict {
    let (root, _generation) = doc
        .trailer
        .dict
        .get("Root")
        .and_then(Value::as_ref)
        .expect("/Root");
    let catalog = doc
        .get_object(root)
        .expect("catalog lookup")
        .expect("catalog object")
        .value
        .as_dict()
        .expect("catalog dict")
        .clone();
    catalog
        .get("ViewerPreferences")
        .and_then(Value::as_dict)
        .expect("catalog /ViewerPreferences dict")
        .clone()
}

/// 写出后重新解析，检查对象图与特征画像。
fn build_and_parse(b: &mut PdfBuilder) -> Document {
    let mut out = Vec::new();
    b.build(&mut out).expect("build");
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    std::fs::create_dir_all(&dir).expect("create test-tmp");
    let path = dir.join(format!(
        "pdf-engine-viewer-preferences-{}-{}.pdf",
        std::process::id(),
        out.len()
    ));
    std::fs::write(&path, &out).expect("write temp pdf");
    let file = std::fs::File::open(&path).expect("open temp pdf");
    let mut parser = crate::core::reader::Parser::new(file).expect("parser");
    let doc = parser.parse_document().expect("parse");
    let _ = std::fs::remove_file(&path);
    doc
}

fn features(doc: &Document) -> Vec<PdfFeature> {
    doc.features.borrow().iter().copied().collect()
}

#[test]
fn viewer_preferences_writes_only_declared_entries() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.set_viewer_preferences(
        ViewerPreferences::new()
            .with_fit_window(true)
            .with_center_window(true)
            .with_non_full_screen_page_mode(NonFullScreenPageMode::UseOutlines)
            .with_direction(ReadingDirection::RightToLeft)
            .with_display_doc_title(true)
            .with_print_area(PageBoundaryName::BleedBox)
            .with_print_scaling(PrintScaling::None)
            .with_duplex(Duplex::DuplexFlipLongEdge)
            .with_pick_tray_by_pdf_size(true)
            .with_print_page_range(vec![(1, 4), (9, 12)])
            .with_num_copies(2),
    );
    let s = render(&mut b);
    assert!(
        s.contains("/ViewerPreferences"),
        "目录应挂 /ViewerPreferences"
    );
    assert!(s.contains("/FitWindow true"));
    assert!(s.contains("/CenterWindow true"));
    assert!(s.contains("/NonFullScreenPageMode /UseOutlines"));
    assert!(s.contains("/Direction /R2L"));
    assert!(s.contains("/DisplayDocTitle true"));
    assert!(s.contains("/PrintArea /BleedBox"));
    assert!(s.contains("/PrintScaling /None"));
    assert!(s.contains("/Duplex /DuplexFlipLongEdge"));
    assert!(s.contains("/PickTrayByPDFSize true"));
    assert!(s.contains("/PrintPageRange [1 4 9 12]"));
    assert!(s.contains("/NumCopies 2"));
    // 未声明（缺省）条目不物化。
    assert!(!s.contains("/HideToolbar"));
    assert!(!s.contains("/ViewArea"));
    assert!(!s.contains("/Enforce"));
}

#[test]
fn viewer_preferences_default_preferences_still_write_dictionary() {
    // 全部缺省：写出空字典（表达「声明了首选项」），回读等价于缺省模型。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.set_viewer_preferences(ViewerPreferences::new());
    let s = render(&mut b);
    assert!(s.contains("/ViewerPreferences"));
    let doc = build_and_parse(&mut b);
    let prefs = doc.viewer_preferences().expect("读取").expect("字典存在");
    assert_eq!(prefs, ViewerPreferences::default());
    assert!(features(&doc).contains(&PdfFeature::ViewerPreferences));
}

#[test]
fn viewer_preferences_rejects_reversed_print_page_range_with_zero_bytes() {
    // 倒序子区间：库策略拒绝（规范未禁止），零字节输出。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.set_viewer_preferences(ViewerPreferences::new().with_print_page_range(vec![(9, 2)]));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("倒序子区间应被拒绝");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn viewer_preferences_rejects_out_of_bounds_print_page_range_with_zero_bytes() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.set_viewer_preferences(ViewerPreferences::new().with_print_page_range(vec![(0, 3)]));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("页码从 1 起");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty());
}

#[test]
fn viewer_preferences_rejects_non_positive_num_copies_with_zero_bytes() {
    // /NumCopies ≥1 是库策略（规范把范围留给阅读器）。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.set_viewer_preferences(ViewerPreferences::new().with_num_copies(0));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("份数必须为正");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn viewer_preferences_rejects_empty_enforce_name_with_zero_bytes() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.set_viewer_preferences(ViewerPreferences::new().with_enforce(vec![String::new()]));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("空名应被拒绝");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty());
}

#[test]
fn viewer_preferences_dictionary_on_pdf_1_1_target_writes_zero_bytes() {
    // 字典本身 PDF 1.2 引入。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_1);
    b.page(PageSize::A4);
    b.set_viewer_preferences(ViewerPreferences::new().with_hide_toolbar(true));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("1.2 条目在 1.1 目标应被拒绝");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn viewer_preferences_direction_on_pdf_1_2_target_writes_zero_bytes() {
    // /Direction PDF 1.3 引入。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    b.page(PageSize::A4);
    b.set_viewer_preferences(
        ViewerPreferences::new().with_direction(ReadingDirection::RightToLeft),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("1.3 条目在 1.2 目标应被拒绝");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(out.is_empty());
}

#[test]
fn viewer_preferences_1_4_entries_on_pdf_1_3_target_write_zero_bytes() {
    // /DisplayDocTitle 与四个页面边界条目 PDF 1.4 引入。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    b.page(PageSize::A4);
    b.set_viewer_preferences(ViewerPreferences::new().with_view_clip(PageBoundaryName::ArtBox));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("1.4 条目在 1.3 目标应被拒绝");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(out.is_empty());
}

#[test]
fn viewer_preferences_print_scaling_on_pdf_1_4_target_writes_zero_bytes() {
    // /PrintScaling PDF 1.6 引入。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    b.page(PageSize::A4);
    b.set_viewer_preferences(ViewerPreferences::new().with_print_scaling(PrintScaling::None));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("1.6 条目在 1.4 目标应被拒绝");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(out.is_empty());
}

#[test]
fn viewer_preferences_1_7_only_entry_on_pdf_1_4_target_writes_zero_bytes() {
    // 1.7-only 条目（/Duplex 等）在 1.4 目标下零字节。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    b.page(PageSize::A4);
    b.set_viewer_preferences(
        ViewerPreferences::new()
            .with_print_page_range(vec![(1, 2)])
            .with_num_copies(3),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("1.7 条目在 1.4 目标应被拒绝");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn viewer_preferences_enforce_rejected_on_all_supported_targets() {
    // /Enforce 系 PDF 2.0 扩展；writer 输出区间 1.0–1.7，一切目标均拒绝。
    for version in [PdfVersion::V1_4, PdfVersion::V1_7] {
        let mut b = PdfBuilder::with_version(version);
        b.page(PageSize::A4);
        b.set_viewer_preferences(ViewerPreferences::new().with_enforce(vec!["CropBox".to_owned()]));
        let mut out = Vec::new();
        let error = b
            .build(&mut out)
            .expect_err("/Enforce 在任何 1.x 目标应被拒绝");
        assert!(
            matches!(error, WriteError::UnsupportedVersion(_)),
            "{error}"
        );
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}

#[test]
fn viewer_preferences_feature_profile_registers_used_tiers() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.set_viewer_preferences(
        ViewerPreferences::new()
            .with_direction(ReadingDirection::RightToLeft)
            .with_display_doc_title(true)
            .with_print_scaling(PrintScaling::None)
            .with_num_copies(2),
    );
    let doc = build_and_parse(&mut b);
    // 读端扫描：解析并读取类型化首选项时，目录对象顺带进入特征画像。
    doc.viewer_preferences()
        .expect("读取首选项")
        .expect("字典存在");
    let used = features(&doc);
    for expected in [
        PdfFeature::ViewerPreferences,
        PdfFeature::ViewerReadingDirection,
        PdfFeature::ViewerDisplayDocTitle,
        PdfFeature::ViewerPrintScaling,
        PdfFeature::ViewerPrintDialogPreferences,
    ] {
        assert!(used.contains(&expected), "特征画像应登记 {expected}");
    }
    assert!(
        !used.contains(&PdfFeature::ViewerPageBoundaryNames),
        "未声明的档位不应登记"
    );
}

#[test]
fn viewer_preferences_roundtrip_through_parser_pdf_1_4() {
    // 1.4 文档：只用 ≤1.4 档位条目，写出 → 解析 → 类型化回读一致。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    b.page(PageSize::A4);
    b.set_viewer_preferences(
        ViewerPreferences::new()
            .with_hide_window_ui(true)
            .with_non_full_screen_page_mode(NonFullScreenPageMode::UseThumbs)
            .with_center_window(true)
            .with_display_doc_title(true)
            .with_print_clip(PageBoundaryName::TrimBox),
    );
    let doc = build_and_parse(&mut b);
    let prefs = doc.viewer_preferences().expect("读取").expect("字典存在");
    assert!(prefs.hide_window_ui);
    assert!(prefs.center_window);
    assert_eq!(
        prefs.non_full_screen_page_mode,
        NonFullScreenPageMode::UseThumbs
    );
    assert_eq!(prefs.direction, ReadingDirection::LeftToRight, "缺省 L2R");
    assert!(prefs.display_doc_title);
    assert_eq!(prefs.print_clip, PageBoundaryName::TrimBox);
    assert_eq!(prefs.view_area, PageBoundaryName::CropBox, "缺省 CropBox");
    let dict = viewer_preferences_object(&doc);
    assert_eq!(dict.get("DisplayDocTitle"), Some(&Value::Bool(true)));
}

#[test]
fn viewer_preferences_roundtrip_through_parser_pdf_1_7() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.set_viewer_preferences(
        ViewerPreferences::new()
            .with_fit_window(true)
            .with_duplex(Duplex::Simplex)
            .with_pick_tray_by_pdf_size(false)
            .with_print_page_range(vec![(2, 5)])
            .with_num_copies(3),
    );
    let doc = build_and_parse(&mut b);
    let prefs = doc.viewer_preferences().expect("读取").expect("字典存在");
    assert!(prefs.fit_window);
    assert_eq!(prefs.duplex, Some(Duplex::Simplex));
    assert_eq!(prefs.pick_tray_by_pdf_size, Some(false));
    assert_eq!(prefs.print_page_range, vec![(2, 5)]);
    assert_eq!(prefs.num_copies, Some(3));
    assert_eq!(
        prefs.print_scaling,
        PrintScaling::AppDefault,
        "缺省 AppDefault"
    );
}
