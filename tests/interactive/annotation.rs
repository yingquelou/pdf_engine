//! E2 注解：通用条目、子类型专有条目、`/Dest` 目标模型与 `/AP` 外观流的写出→解析往返。

use super::*;

/// 写入 → 重新解析：注解应经页面 `/Annots` 由读端注解模型完整提取，
/// 含文本条目、标志、`/QuadPoints`、`/Popup` 引用与 `/A` 动作。
#[test]
fn builder_annotations_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        Action, AnnotationDestination, AnnotationKind, PageSize, PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::reader::action::Action as ReaderAction;
    use pdf_engine::core::reader::annotation::AnnotationSubtype;
    use pdf_engine::core::reader::navigation::{Destination, OutlineDestination};
    use pdf_engine::core::types::{PdfTextString, Value};

    fn decode_text(value: &Value) -> String {
        match value {
            Value::String(text) => text.clone(),
            Value::HexString(hex) => {
                let bytes: Vec<u8> = (0..hex.len() / 2)
                    .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
                    .collect();
                PdfTextString::decode(&bytes).unwrap().into_string()
            }
            other => panic!("expected text string, got {other:?}"),
        }
    }

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let p1 = builder.page(PageSize::A4);
    let popup = builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Popup, [96.0, 690.0, 240.0, 750.0]),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 700.0, 92.0, 720.0])
            .with_contents("Review note")
            .with_title("Ann")
            .with_flags(4)
            .with_popup(popup),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Highlight, [72.0, 650.0, 220.0, 662.0])
            .with_quad_points(vec![72.0, 662.0, 220.0, 662.0, 72.0, 650.0, 220.0, 650.0])
            .with_color(vec![1.0, 1.0, 0.0]),
    );
    builder.add_annotation(
        p1,
        PdfAnnotation::new(AnnotationKind::Link, [0.0, 0.0, 100.0, 20.0])
            .with_action(Action::Uri("https://qoder.com".into())),
    );
    builder.add_annotation(
        p1,
        PdfAnnotation::new(AnnotationKind::Link, [0.0, 30.0, 100.0, 50.0]).with_destination(
            AnnotationDestination::Explicit(Destination::Fit { page: 0 }),
        ),
    );
    builder.add_annotation(
        p1,
        PdfAnnotation::new(AnnotationKind::Link, [0.0, 60.0, 100.0, 80.0])
            .with_action(Action::GoTo(Destination::Fit { page: 1 })),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let first = doc.page_annotations(0).expect("page 0 annotations");
    assert_eq!(first.len(), 3);
    let text = &first[1];
    assert_eq!(text.subtype, AnnotationSubtype::Text);
    assert_eq!(text.rect, [72.0, 700.0, 92.0, 720.0]);
    assert!(text.flags.print());
    assert_eq!(
        decode_text(text.contents.as_ref().expect("/Contents")),
        "Review note"
    );
    assert_eq!(decode_text(text.title.as_ref().expect("/T")), "Ann");
    let popup_ref = text.popup.expect("/Popup reference");
    let popup_obj = doc
        .get_object(popup_ref.0)
        .expect("popup object")
        .expect("exist");
    let popup_dict = popup_obj.value.as_dict().expect("popup dict");
    assert_eq!(
        popup_dict.get("Subtype"),
        Some(&Value::Name("Popup".into()))
    );
    let highlight = &first[2];
    assert_eq!(highlight.subtype, AnnotationSubtype::Highlight);
    assert_eq!(
        highlight.quad_points.as_ref().expect("/QuadPoints").len(),
        8
    );
    assert_eq!(highlight.color.as_deref(), Some([1.0, 1.0, 0.0].as_slice()));

    let second = doc.page_annotations(1).expect("page 1 annotations");
    assert_eq!(second.len(), 3);
    let uri = doc
        .action(second[0].action.as_ref().expect("/A"))
        .expect("uri action parses");
    assert_eq!(uri, ReaderAction::Uri("https://qoder.com".into()));
    let dest = second[1].destination.as_ref().expect("/Dest");
    let array = dest.as_array().expect("explicit destination array");
    let (page_num, _) = array.0[0].as_ref().expect("page reference");
    let page_obj = doc
        .get_object(page_num)
        .expect("page object")
        .expect("exist");
    assert_eq!(
        page_obj.value.as_dict().expect("dict").get("Type"),
        Some(&Value::Name("Page".into()))
    );
    assert_eq!(
        page_num, 3,
        "explicit /Dest must reference the first Page object"
    );
    let goto = doc
        .action(second[2].action.as_ref().expect("/A GoTo"))
        .expect("goto action parses");
    assert_eq!(
        goto,
        ReaderAction::GoTo(OutlineDestination::Explicit(Destination::Fit { page: 1 }))
    );
}

/// 写入 → 重新解析：Text `/Name` `/Open` `/State` `/StateModel`、Caret `/Symbol`
/// 与 Ink `/InkList` 等子类型专有条目应回读为读端的类型化字段，非 ASCII
/// 文本串（`/State`）与名字型条目（`/Name`、`/Symbol`）各自保持原值（12.5.6）。
#[test]
fn builder_annotation_subtype_entries_roundtrip_through_parser() {
    use pdf_engine::core::builder::{AnnotationKind, PageSize, PdfAnnotation, PdfBuilder};
    use pdf_engine::core::reader::annotation::AnnotationSubtype;

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 700.0, 92.0, 720.0])
            .with_icon_name("Comment")
            .with_open(true)
            .with_state("已标记")
            .with_state_model("Marked"),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Caret, [100.0, 660.0, 112.0, 672.0])
            .with_symbol("Cross"),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Ink, [120.0, 600.0, 220.0, 660.0])
            .with_ink_path(vec![120.0, 660.0, 170.0, 630.0, 220.0, 600.0])
            .with_ink_path(vec![130.0, 610.0, 200.0, 650.0]),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let annots = doc.page_annotations(0).expect("page 0 annotations");
    assert_eq!(annots.len(), 3);

    let text = &annots[0];
    assert_eq!(text.subtype, AnnotationSubtype::Text);
    assert_eq!(text.icon_name, Some("Comment".into()));
    assert_eq!(text.is_open, Some(true));
    assert_eq!(text.state, Some("已标记".into()));
    assert_eq!(text.state_model, Some("Marked".into()));
    assert_eq!(text.symbol, None, "/Symbol belongs to Caret");
    assert!(text.ink_paths.is_empty(), "/InkList belongs to Ink");

    let caret = &annots[1];
    assert_eq!(caret.subtype, AnnotationSubtype::Caret);
    assert_eq!(caret.symbol, Some("Cross".into()));
    assert_eq!(caret.icon_name, None);

    let ink = &annots[2];
    assert_eq!(ink.subtype, AnnotationSubtype::Ink);
    assert_eq!(
        ink.ink_paths,
        vec![
            vec![120.0, 660.0, 170.0, 630.0, 220.0, 600.0],
            vec![130.0, 610.0, 200.0, 650.0],
        ],
        "/InkList 每条路径的坐标序列应逐值还原"
    );
}

/// 写入 → 重新解析：Line 注解的端点 `/L`、端点样式 `/LE`、引线 `/LL` `/LLE`、
/// 题注开关 `/Cap`、意图 `/IT` 与题注条目 `/LLO` `/CP` `/CO`（Table 175/176）
/// 应回读为读端的类型化字段，`/IC` 填充色经通用条目回读。
#[test]
fn builder_line_annotation_entries_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, CaptionPosition as BuilderCaptionPosition,
        LineEndStyle as BuilderLineEndStyle, LineIntent as BuilderLineIntent, PageSize,
        PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::reader::annotation::{
        AnnotationSubtype, CaptionPosition, LineEndStyle, LineIntent,
    };

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Line, [72.0, 640.0, 300.0, 700.0])
            .with_line_endpoints([72.0, 700.5, 300.0, 650.0])
            .with_line_endings(
                BuilderLineEndStyle::ClosedArrow,
                BuilderLineEndStyle::ROpenArrow,
            )
            .with_interior_color(vec![1.0, 0.0, 0.0])
            .with_color(vec![0.0, 0.0, 1.0])
            .with_leader_length(12.5)
            .with_leader_extension(3.0)
            .with_caption(true)
            .with_line_intent(BuilderLineIntent::LineDimension)
            .with_leader_line_offset(6.0)
            .with_caption_position(BuilderCaptionPosition::Top)
            .with_caption_offset([-1.5, 20.0]),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let annots = doc.page_annotations(0).expect("page 0 annotations");
    assert_eq!(annots.len(), 1);
    let line = &annots[0];
    assert_eq!(line.subtype, AnnotationSubtype::Line);
    assert_eq!(line.line_endpoints, Some([72.0, 700.5, 300.0, 650.0]));
    assert_eq!(
        line.line_endings,
        Some([LineEndStyle::ClosedArrow, LineEndStyle::ROpenArrow]),
        "/LE 两端样式各自还原"
    );
    assert_eq!(line.interior_color, Some(vec![1.0, 0.0, 0.0]));
    assert_eq!(line.color, Some(vec![0.0, 0.0, 1.0]));
    assert_eq!(line.leader_length, Some(12.5));
    assert_eq!(line.leader_extension, Some(3.0));
    assert_eq!(line.caption, Some(true));
    assert_eq!(line.line_intent, Some(LineIntent::LineDimension));
    assert_eq!(line.leader_line_offset, Some(6.0));
    assert_eq!(line.caption_position, Some(CaptionPosition::Top));
    assert_eq!(line.caption_offset, Some([-1.5, 20.0]));
}

/// 写入 → 重新解析：注解 `/Dest` 的显式数组与命名目标应由
/// `Document::annotation_destination` 解析为 12.3.2 目标模型，命名键继续经
/// `Document::named_destination` 在目录 `/Names /Dests` 树中查得实际目标。
#[test]
fn builder_annotation_destination_resolves_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationDestination, AnnotationKind, PageSize, PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::reader::navigation::{Destination, OutlineDestination};

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let _p1 = builder.page(PageSize::A4);
    builder.named_destination(
        "intro",
        Destination::Xyz {
            page: 1,
            left: Some(0.0),
            top: Some(700.0),
            zoom: None,
        },
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [0.0, 0.0, 100.0, 20.0])
            .with_destination(AnnotationDestination::Named("intro".into())),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [0.0, 30.0, 100.0, 50.0]).with_destination(
            AnnotationDestination::Explicit(Destination::Fit { page: 1 }),
        ),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 700.0, 92.0, 720.0])
            .with_contents("no destination"),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let annots = doc.page_annotations(0).expect("page 0 annotations");
    assert_eq!(annots.len(), 3);

    assert_eq!(
        doc.annotation_destination(&annots[0])
            .expect("named /Dest parses"),
        Some(OutlineDestination::Named("intro".into())),
        "命名目标先返回名称键"
    );
    assert_eq!(
        doc.named_destination("intro").expect("name tree"),
        Some(Destination::Xyz {
            page: 1,
            left: Some(0.0),
            top: Some(700.0),
            zoom: None,
        }),
        "名称键经 `/Dests` 树解析到第 2 页"
    );
    assert_eq!(
        doc.annotation_destination(&annots[1])
            .expect("explicit /Dest parses"),
        Some(OutlineDestination::Explicit(Destination::Fit { page: 1 })),
        "显式数组的页引用解析为 0 基页号"
    );
    assert_eq!(
        doc.annotation_destination(&annots[2])
            .expect("annotation without /Dest"),
        None
    );
}

/// 写入 → 重新解析：注解 `/AP` 外观流应回读为 Form XObject 流引用，
/// `/N` 状态、流字典与内容绘制算子完整保留（12.5.5）。
#[test]
fn builder_annotation_appearance_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, FormXObject, PageSize, PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::types::Value;

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let mut content = ContentStream::new();
    content
        .save_state()
        .set_fill_gray(0.5)
        .rect(0.0, 0.0, 20.0, 20.0)
        .fill()
        .restore_state();
    let normal = builder.form_xobject(&FormXObject::new([0.0, 0.0, 20.0, 20.0], content));
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Square, [10.0, 10.0, 30.0, 30.0])
            .with_appearance(normal),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let annots = doc.page_annotations(0).expect("page 0 annotations");
    assert_eq!(annots.len(), 1);
    let appearance = annots[0].appearance.as_ref().expect("/AP dict");
    let normal_value = appearance.normal.as_ref().expect("/AP /N");
    let (form_num, _) = normal_value.as_ref().expect("/N should be a reference");
    let form = doc
        .get_object(form_num)
        .expect("form object")
        .expect("exist");
    let form_dict = match &form.value {
        Value::Stream(dict, _) => dict.clone(),
        other => panic!("expected appearance Form XObject, got {other:?}"),
    };
    assert_eq!(form_dict.get("Type"), Some(&Value::Name("XObject".into())));
    assert_eq!(form_dict.get("Subtype"), Some(&Value::Name("Form".into())));
    assert!(form_dict.get("BBox").is_some());
    let content = doc
        .decode_stream_bytes(&form.value)
        .expect("appearance content decodes");
    let text = String::from_utf8_lossy(&content);
    assert!(text.contains(" re"), "appearance content: {text}");
    assert!(
        text.lines().any(|line| line.trim() == "f"),
        "appearance content: {text}"
    );
}
/// E2：写端支持的 17 种子类型逐个写出后，读端解析的 `AnnotationSubtype`
/// 与写端 `AnnotationKind::subtype()` 一致，且名称、引入版本与标记类归属
/// 全部落在同一张 Table 169 上（读写共用单点表）。
#[test]
fn builder_annotation_subtype_table_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, EmbeddedFile, PageSize, PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::reader::annotation::TABLE_169;

    const KINDS: [AnnotationKind; 17] = [
        AnnotationKind::Text,
        AnnotationKind::Link,
        AnnotationKind::FreeText,
        AnnotationKind::Line,
        AnnotationKind::Square,
        AnnotationKind::Circle,
        AnnotationKind::Polygon,
        AnnotationKind::PolyLine,
        AnnotationKind::Highlight,
        AnnotationKind::Underline,
        AnnotationKind::Squiggly,
        AnnotationKind::StrikeOut,
        AnnotationKind::Caret,
        AnnotationKind::Ink,
        AnnotationKind::Popup,
        AnnotationKind::FileAttachment,
        AnnotationKind::Stamp,
    ];

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    // `FileAttachment` 注解必须带 `/FS` 文件规格句柄（E6 构建期校验）。
    let filespec = builder.embed_file(EmbeddedFile::new("note.txt", b"subtype table".to_vec()));
    for (index, kind) in KINDS.iter().enumerate() {
        let top = 740.0 - (index as f64) * 20.0;
        let rect = [72.0, top - 10.0, 200.0, top];
        let mut annot = PdfAnnotation::new(*kind, rect);
        if *kind == AnnotationKind::Line {
            annot = annot.with_line_endpoints(rect);
        }
        if *kind == AnnotationKind::FreeText {
            // `/DA` 是 Table 174 的必填条目。
            annot = annot.with_default_appearance("/Helv 12 Tf 0 g");
        }
        if *kind == AnnotationKind::FileAttachment {
            annot = annot.with_filespec(filespec);
        }
        if matches!(kind, AnnotationKind::Polygon | AnnotationKind::PolyLine) {
            // `/Vertices` 是 Table 178 的必填条目。
            annot = annot.with_vertices(vec![rect[0], rect[1], rect[2], rect[1], rect[2], rect[3]]);
        }
        builder.add_annotation(p0, annot);
    }
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build 17 subtypes");

    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), KINDS.len());
    for (kind, annotation) in KINDS.iter().zip(&annotations) {
        let row = TABLE_169
            .iter()
            .find(|(name, _, _)| *name == kind.name())
            .unwrap_or_else(|| panic!("Table 169 无 {} 行", kind.name()));
        assert_eq!(annotation.subtype, kind.subtype(), "{}", kind.name());
        assert_eq!(annotation.subtype.name(), kind.name());
        assert_eq!(
            annotation.subtype.min_version(),
            Some(row.1),
            "{}",
            kind.name()
        );
        assert_eq!(annotation.subtype.is_markup(), row.2, "{}", kind.name());
    }
}

/// E2：FreeText 注解的 Table 174 专有条目写出后，读端从页面 `/Annots`
/// 重新解析出同一组值（`/DA` `/Q` `/RC` `/DS` `/CL` `/IT` `/BE` `/RD` `/LE`）。
#[test]
fn builder_free_text_annotation_entries_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, BorderEffect, CalloutLine, FreeTextIntent, LineEndStyle, PageSize,
        PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::reader::annotation::{
        BorderEffect as ReaderBorderEffect, BorderEffectStyle as ReaderBorderEffectStyle,
        CalloutLine as ReaderCalloutLine, FreeTextIntent as ReaderFreeTextIntent,
        LineEndStyle as ReaderLineEndStyle,
    };
    use pdf_engine::core::types::{PdfTextString, Value};

    fn decode_text(value: &Value) -> String {
        match value {
            Value::String(text) => text.clone(),
            Value::HexString(hex) => {
                let bytes: Vec<u8> = (0..hex.len() / 2)
                    .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
                    .collect();
                PdfTextString::decode(&bytes).unwrap().into_string()
            }
            other => panic!("expected text string, got {other:?}"),
        }
    }

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::FreeText, [72.0, 640.0, 300.0, 700.0])
            .with_default_appearance("/F1 12 Tf 0 g")
            .with_rich_content("<body>note</body>")
            .with_quadding(2)
            .with_default_style("font: ArialMT 12pt;")
            .with_callout_line(CalloutLine::Knee([20.0, 100.0, 60.5, 140.0, 120.0, 180.0]))
            .with_free_text_intent(FreeTextIntent::FreeTextCallout)
            .with_border_effect(BorderEffect::cloudy(1.5))
            .with_text_rectangle([0.0, 2.5, 0.0, 2.5])
            .with_callout_ending(LineEndStyle::ClosedArrow),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 1);
    let annotation = &annotations[0];
    assert_eq!(
        annotation.default_appearance.as_deref(),
        Some("/F1 12 Tf 0 g")
    );
    assert_eq!(annotation.quadding, Some(2));
    let rich = annotation.rich_contents.as_ref().expect("/RC");
    assert_eq!(decode_text(rich), "<body>note</body>");
    assert_eq!(
        annotation.default_style.as_deref(),
        Some("font: ArialMT 12pt;")
    );
    assert_eq!(
        annotation.callout_line,
        Some(ReaderCalloutLine::Knee([
            20.0, 100.0, 60.5, 140.0, 120.0, 180.0
        ]))
    );
    assert_eq!(
        annotation.free_text_intent,
        Some(ReaderFreeTextIntent::FreeTextCallout)
    );
    assert_eq!(
        annotation.border_effect,
        Some(ReaderBorderEffect {
            style: ReaderBorderEffectStyle::Cloudy,
            intensity: Some(1.5),
        })
    );
    assert_eq!(annotation.text_rectangle, Some([0.0, 2.5, 0.0, 2.5]));
    assert_eq!(
        annotation.callout_ending,
        Some(ReaderLineEndStyle::ClosedArrow)
    );
    assert!(
        annotation.line_endings.is_none(),
        "单名字 /LE 不写进 Line 的双端点数组"
    );
}
