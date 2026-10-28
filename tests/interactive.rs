//! 集成测试：阶段 E/F 交互特性主题。
//!
//! 覆盖导航（E1）、注解（E2）、动作（E3）、AcroForm（E4）、元数据与文件
//! 标识（E5）、嵌入文件（E6）、增量更新（E7）与数字签名（F1）的
//! 写入 → 解析 → 回读往返；共享辅助见 `tests/support/mod.rs`。

mod support;

use pdf_engine::core::types::{Array, Dict, Object, StreamData, Value};
use support::parse_buf;

/// E1：builder 写出的页面标签、命名目标、命名页面与大纲经解析器还原
/// （含非 ASCII 文本字符串、全部 8 种显式目标与折叠 Count）。
#[test]
fn builder_navigation_roundtrips_through_parser() {
    use pdf_engine::core::builder::{OutlineEntry, PageSize, PdfBuilder};
    use pdf_engine::core::reader::navigation::{
        Destination, OutlineDestination, PageLabelRange, PageLabelStyle,
    };

    let labels = vec![
        PageLabelRange {
            start: 0,
            style: None,
            prefix: "Cover".into(),
            first_number: 1,
        },
        PageLabelRange {
            start: 1,
            style: Some(PageLabelStyle::Decimal),
            prefix: "p.".into(),
            first_number: 1,
        },
        PageLabelRange {
            start: 2,
            style: Some(PageLabelStyle::UppercaseRoman),
            prefix: String::new(),
            first_number: 1,
        },
        PageLabelRange {
            start: 3,
            style: Some(PageLabelStyle::LowercaseLetters),
            prefix: "第 ".into(),
            first_number: 3,
        },
    ];

    let mut builder = PdfBuilder::new();
    for _ in 0..4 {
        builder.page(PageSize::A4);
    }
    builder.page_labels(&labels);

    let destinations = [
        ("fit", Destination::Fit { page: 0 }),
        (
            "fit-h",
            Destination::FitH {
                page: 1,
                top: Some(700.0),
            },
        ),
        (
            "fit-v",
            Destination::FitV {
                page: 2,
                left: Some(10.0),
            },
        ),
        (
            "fit-r",
            Destination::FitR {
                page: 3,
                rect: [0.0, 0.0, 100.0, 200.0],
            },
        ),
        ("fit-b", Destination::FitB { page: 0 }),
        ("fit-bh", Destination::FitBH { page: 1, top: None }),
        (
            "fit-bv",
            Destination::FitBV {
                page: 2,
                left: Some(5.0),
            },
        ),
        (
            "xyz",
            Destination::Xyz {
                page: 3,
                left: Some(1.0),
                top: Some(2.0),
                zoom: Some(1.5),
            },
        ),
    ];
    for (name, destination) in &destinations {
        builder.named_destination(*name, destination.clone());
    }
    builder.named_page("appendix", 3);
    builder.named_page("start", 0);

    let chapter = OutlineEntry {
        title: "Chapter 1".into(),
        destination: Some(OutlineDestination::Explicit(Destination::Fit { page: 0 })),
        action: None,
        color: Some([1.0, 0.0, 0.0]),
        flags: 3,
        expanded: true,
        children: vec![
            OutlineEntry {
                title: "Section 1.1".into(),
                destination: Some(OutlineDestination::Explicit(Destination::FitH {
                    page: 1,
                    top: Some(700.0),
                })),
                action: None,
                color: None,
                flags: 0,
                expanded: false,
                children: Vec::new(),
            },
            OutlineEntry {
                title: "Section 1.2".into(),
                destination: Some(OutlineDestination::Named("fit".into())),
                action: None,
                color: None,
                flags: 0,
                expanded: false,
                children: Vec::new(),
            },
        ],
    };
    let appendix = OutlineEntry {
        title: "附录 A".into(),
        destination: Some(OutlineDestination::Explicit(Destination::FitR {
            page: 3,
            rect: [0.0, 0.0, 100.0, 200.0],
        })),
        action: None,
        color: None,
        flags: 0,
        expanded: false,
        children: vec![OutlineEntry {
            title: "Details".into(),
            destination: Some(OutlineDestination::Explicit(Destination::FitB { page: 0 })),
            action: None,
            color: None,
            flags: 0,
            expanded: false,
            children: Vec::new(),
        }],
    };
    builder.outline(&[chapter, appendix]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    assert_eq!(reparsed.page_count().expect("page count"), 4);
    let parsed_labels = reparsed.page_labels().expect("labels").expect("present");
    assert_eq!(parsed_labels.ranges(), labels.as_slice());
    assert_eq!(
        reparsed.page_label(0).expect("label 0").as_deref(),
        Some("Cover")
    );
    assert_eq!(
        reparsed.page_label(1).expect("label 1").as_deref(),
        Some("p.1")
    );
    assert_eq!(
        reparsed.page_label(2).expect("label 2").as_deref(),
        Some("I")
    );
    assert_eq!(
        reparsed.page_label(3).expect("label 3").as_deref(),
        Some("第 c")
    );

    for (name, destination) in &destinations {
        assert_eq!(
            reparsed.named_destination(name).expect("named destination"),
            Some(destination.clone()),
            "{name}"
        );
    }

    assert_eq!(
        reparsed.named_page("appendix").expect("named page"),
        Some(3)
    );
    assert_eq!(reparsed.named_page("start").expect("named page"), Some(0));

    let outline = reparsed.outline().expect("outline").expect("present");
    assert_eq!(outline.items.len(), 2);
    let first = &outline.items[0];
    assert_eq!(first.title, "Chapter 1");
    assert_eq!(
        first.destination,
        Some(OutlineDestination::Explicit(Destination::Fit { page: 0 }))
    );
    assert_eq!(first.color, Some([1.0, 0.0, 0.0]));
    assert_eq!(first.flags, 3);
    assert_eq!(first.count, 2);
    assert_eq!(first.children.len(), 2);
    assert_eq!(
        first.children[1].destination,
        Some(OutlineDestination::Named("fit".into()))
    );
    assert_eq!(first.children[1].count, 0);
    let second = &outline.items[1];
    assert_eq!(second.title, "附录 A");
    assert_eq!(second.count, -1);
    assert_eq!(second.children.len(), 1);
    assert_eq!(second.children[0].title, "Details");
}

/// 写入 → 重新解析：trailer `/Info` 与 `/ID` 应经 XRef 流路径完整回读。
#[test]
fn builder_metadata_roundtrips_through_xref_stream_parser() {
    use pdf_engine::core::builder::{DocumentInfo, FileIdentifier, PageSize, PdfBuilder};
    use pdf_engine::core::types::Value;

    let mut builder = PdfBuilder::new();
    builder.page(PageSize::A4);
    builder.set_document_info(
        DocumentInfo::new()
            .title("报告")
            .author("Qoder")
            .producer("pdf_engine"),
    );
    builder.set_file_identifier(FileIdentifier::new([0x11; 16], [0x22; 16]));
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let info = doc.document_info().expect("/Info should roundtrip");
    match info.title {
        Some(Value::HexString(hex)) => {
            assert!(hex.starts_with("FEFF"), "UTF-16BE with BOM: {hex}")
        }
        other => panic!("expected hex string title, got {other:?}"),
    }
    let (perm, upd) = doc.file_identifier().expect("/ID should roundtrip");
    assert_eq!(perm, [0x11; 16]);
    assert_eq!(upd, [0x22; 16]);
}

/// 写入 → 重新解析：经典 xref 表路径（1.4）也须透传 trailer `/Info` 与 `/ID`。
#[test]
fn builder_metadata_roundtrips_through_classic_table_parser() {
    use pdf_engine::core::builder::{DocumentInfo, FileIdentifier, PageSize, PdfBuilder};
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_4);
    builder.page(PageSize::A4);
    builder.set_document_info(DocumentInfo::new().title("Plain"));
    builder.set_file_identifier(FileIdentifier::generate(b"material".as_slice()));
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let info = doc.document_info().expect("/Info via classic trailer");
    assert!(info.title.is_some());
    let (perm, upd) = doc.file_identifier().expect("/ID via classic trailer");
    assert_eq!(perm, upd, "generated identifier keeps IDs equal");
}

/// 写入 → 重新解析：XMP `/Metadata` 流应可经读端解回原文字节。
#[test]
fn builder_xmp_metadata_roundtrips_through_parser() {
    use pdf_engine::core::builder::{PageSize, PdfBuilder};

    let mut builder = PdfBuilder::new();
    builder.page(PageSize::A4);
    let xml = br##"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?><x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"/></x:xmpmeta><?xpacket end="w"?>"##.to_vec();
    builder.set_xmp_metadata(xml.clone());
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    assert_eq!(doc.xmp_metadata().as_deref(), Some(xml.as_slice()));
}

/// 读端：无 `/Metadata` 的文档返回 `None`。
#[test]
fn xmp_metadata_absent_returns_none() {
    use pdf_engine::core::builder::{PageSize, PdfBuilder};

    let mut builder = PdfBuilder::new();
    builder.page(PageSize::A4);
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    assert!(doc.xmp_metadata().is_none());
}

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

/// 写入 → 重新解析：AcroForm 字段树、Widget `/Annots`、`/Opt` 与 `/AP`
/// 外观流应经 `Document::acro_form` 完整回读（12.7.2–12.7.4）。
#[test]
fn builder_acro_form_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        AcroForm, FormField, FormOption, FormXObject, PageSize, PdfBuilder,
    };
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::reader::annotation::AnnotationSubtype;
    use pdf_engine::core::reader::form::FieldType;

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let mut appearance_content = ContentStream::new();
    appearance_content
        .set_fill_gray(0.0)
        .rect(0.0, 0.0, 10.0, 10.0)
        .fill();
    let appearance = builder.form_xobject(&FormXObject::new(
        [0.0, 0.0, 10.0, 10.0],
        appearance_content,
    ));

    let mut form = AcroForm::new()
        .with_need_appearances(true)
        .with_default_appearance("/Helv 0 Tf 0 g");
    let person = FormField::group("person")
        .with_child(FormField::text("name", [72.0, 700.0, 300.0, 720.0]).with_value("Alice"))
        .with_child(FormField::text("email", [72.0, 670.0, 300.0, 690.0]).with_value("a@b.c"));
    form.add_field(p0, person);
    form.add_field(
        p0,
        FormField::choice("color", [72.0, 620.0, 200.0, 640.0])
            .with_flags(FormField::COMBO)
            .with_value("R")
            .with_option(FormOption::new("R").with_display("Red"))
            .with_option(FormOption::new("G")),
    );
    form.add_field(
        p0,
        FormField::checkbox("subscribe", [72.0, 590.0, 92.0, 610.0])
            .with_appearance(appearance)
            .with_appearance_state("Off"),
    );
    builder.set_acro_form(form);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    let form = doc.acro_form().expect("parse form").expect("form present");
    assert!(form.need_appearances);
    assert!(form.default_appearance.is_some(), "/DA should be present");
    assert_eq!(form.fields.len(), 3);

    let person = &form.fields[0];
    assert_eq!(person.fully_qualified_name, "person");
    assert_eq!(person.kids.len(), 2);
    assert_eq!(person.kids[0].fully_qualified_name, "person.name");
    assert_eq!(person.kids[1].fully_qualified_name, "person.email");
    assert!(person.kids.iter().all(|kid| kid.is_widget));

    let choice = &form.fields[1];
    assert_eq!(choice.field_type, Some(FieldType::Choice));
    assert!(choice.flags.combo());
    assert_eq!(choice.options.as_ref().map(Vec::len), Some(2));

    let checkbox = &form.fields[2];
    assert_eq!(checkbox.field_type, Some(FieldType::Button));
    assert_eq!(checkbox.appearance_state.as_deref(), Some("Off"));
    let appearance = checkbox
        .appearance
        .as_ref()
        .and_then(|value| value.as_dict())
        .expect("/AP dictionary");
    let (form_num, _) = appearance
        .get("N")
        .and_then(|value| value.as_ref())
        .expect("/AP /N");
    let object = doc
        .get_object(form_num)
        .expect("form object")
        .expect("exists");
    match object.value {
        Value::Stream(ref dict, _) => {
            assert_eq!(dict.get("Subtype"), Some(&Value::Name("Form".into())));
        }
        other => panic!("expected Form XObject, got {other:?}"),
    }

    let widgets = doc.page_annotations(0).expect("page annotations");
    assert_eq!(
        widgets.len(),
        4,
        "two child widgets, the choice field and the checkbox"
    );
    assert!(
        widgets
            .iter()
            .all(|annot| annot.subtype == AnnotationSubtype::Widget)
    );
}

/// 写入 → 重新解析：`/AcroForm /DR /Font` 默认资源应回读为字体间接引用，
/// 且与页面 `/Resources` 中的同名字体共用同一对象（12.7.2 Table 218）。
#[test]
fn builder_acro_form_default_resources_roundtrip_through_parser() {
    use pdf_engine::core::builder::{AcroForm, Font, FormField, PageSize, PdfBuilder};
    use pdf_engine::core::types::Value;

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let font = Font::helvetica(12.0);
    builder.text(page, &font, 72.0, 750.0, "Form");
    let mut form = AcroForm::new()
        .with_default_appearance("/Helv 12 Tf 0 g")
        .with_default_font("Helv", font.clone());
    form.add_field(
        page,
        FormField::text("name", [72.0, 700.0, 300.0, 720.0]).with_value("Alice"),
    );
    builder.set_acro_form(form);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    let parsed = doc.acro_form().expect("parse form").expect("form present");
    assert_eq!(parsed.fields.len(), 1, "one top-level field");

    let dr = parsed
        .default_resources
        .as_ref()
        .and_then(|value| value.as_dict())
        .expect("/DR dictionary")
        .clone();
    let fonts = dr
        .get("Font")
        .and_then(|value| value.as_dict())
        .expect("/DR /Font dictionary")
        .clone();
    assert_eq!(fonts.0.len(), 1, "one default font resource");
    let (font_num, _) = fonts
        .get("Helv")
        .and_then(|value| value.as_ref())
        .expect("/Helv is an indirect reference");
    let font_dict = doc
        .get_object(font_num)
        .expect("font object")
        .expect("font exists")
        .value
        .as_dict()
        .expect("font dictionary")
        .clone();
    assert_eq!(
        font_dict.get("BaseFont"),
        Some(&Value::Name("Helvetica".into())),
        "default resource resolves to a real font"
    );

    let mut helvetica_objects = Vec::new();
    for num in 1..30 {
        if let Some(object) = doc.get_object(num).expect("probe object")
            && let Some(dict) = object.value.as_dict()
            && dict.get("Type").and_then(|value| value.as_name()) == Some("Font")
            && dict.get("BaseFont").and_then(|value| value.as_name()) == Some("Helvetica")
        {
            helvetica_objects.push(num);
        }
    }
    assert_eq!(
        helvetica_objects,
        vec![font_num],
        "page resources and /DR share one font object"
    );
    assert_eq!(
        doc.extract_page_text(0).expect("page text").trim(),
        "Form",
        "page text still extracts after adding default resources"
    );
}

/// 写入 → 重新解析：`/EmbeddedFiles` 名称树、文件规格（`/F` `/UF` `/Desc`
/// `/EF`）、`/Params`（`/Size` `/CheckSum`）与 FileAttachment 注解 `/FS`
/// 应经 `Document::embedded_files` 完整回读（7.11.2–7.11.4、12.5.6.14）。
#[test]
fn builder_embedded_files_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, EmbeddedFile, PageSize, PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::reader::annotation::AnnotationSubtype;

    let notes_payload = b"line one \t line two\n".to_vec();
    let report_payload = b"%PDF-1.4 fake report bytes".to_vec();

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let notes_handle = builder.embed_file(
        EmbeddedFile::new("notes.txt", notes_payload.clone())
            .with_subtype("text/plain")
            .with_creation_date("D:20261028050000Z"),
    );
    let report_handle = builder.embed_file(
        EmbeddedFile::new("报告.pdf", report_payload.clone())
            .with_short_name("report.pdf")
            .with_subtype("application/pdf")
            .with_description("季度报告"),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::FileAttachment, [72.0, 700.0, 92.0, 720.0])
            .with_filespec(report_handle),
    );
    let _ = notes_handle;

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);

    let files = doc.embedded_files().expect("embedded files");
    assert_eq!(files.len(), 2, "两个注册项应各占一个名称树条目");

    let notes = files
        .get("notes.txt")
        .expect("ASCII 名按解码键命中")
        .clone();
    assert_eq!(notes.short_name.as_deref(), Some("notes.txt"));
    assert_eq!(notes.unicode_name.as_deref(), Some("notes.txt"));
    assert!(notes.description.is_none());
    let notes_stream = notes.primary().expect("notes 嵌入流");
    assert_eq!(notes_stream.subtype.as_deref(), Some("text/plain"));
    assert_eq!(notes_stream.data, notes_payload, "字节应原样往返");
    assert_eq!(notes_stream.size_matches(), Some(true));
    assert_eq!(notes_stream.checksum_matches(), Some(true));
    assert!(notes_stream.params.creation_date.is_some());

    let report = files
        .get("报告.pdf")
        .expect("UTF-16BE 键应解码命中")
        .clone();
    assert_eq!(report.unicode_name.as_deref(), Some("报告.pdf"));
    assert_eq!(report.short_name.as_deref(), Some("report.pdf"));
    assert_eq!(report.description.as_deref(), Some("季度报告"));
    let report_stream = report.primary().expect("report 嵌入流");
    assert_eq!(report_stream.subtype.as_deref(), Some("application/pdf"));
    assert_eq!(&report_stream.data, &b"%PDF-1.4 fake report bytes"[..]);
    assert_eq!(report_stream.size_matches(), Some(true));
    assert_eq!(report_stream.checksum_matches(), Some(true));

    // `/F` 短名回退查找。
    let by_short = doc
        .embedded_file("report.pdf")
        .expect("lookup")
        .expect("短名应命中");
    assert_eq!(by_short.name(), "报告.pdf");

    // FileAttachment 注解 `/FS` → 同一文件规格。
    let annotations = doc.page_annotations(0).expect("page annotations");
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].subtype, AnnotationSubtype::FileAttachment);
    let filespec = doc
        .annotation_filespec(&annotations[0])
        .expect("filespec")
        .expect("附件注解有 /FS");
    assert_eq!(filespec.name(), "报告.pdf");
    assert_eq!(
        filespec.primary().expect("stream").data,
        b"%PDF-1.4 fake report bytes".to_vec()
    );
}

/// 增量更新追加页面（E7 / 7.5.6）：builder 写单页文档 → 解析 →
/// 手工追加第二页并改写 Pages 节点 → `build_incremental` → 回读，
/// 两页均可解析且新页文本可提取；原文件字节为输出前缀。
#[test]
fn builder_incremental_update_appends_second_page() {
    use pdf_engine::core::builder::{Font, PageSize, PdfBuilder};
    use pdf_engine::core::writer::DocumentWriter;

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    builder.text(p0, &Font::helvetica(24.0), 72.0, 700.0, "First");
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    assert_eq!(doc.page_count().expect("page count"), 1);

    // 在已解析文档上做对象手术：复制第 1 页字典、新建内容流，
    // 追加进 Pages /Kids 并递增 /Count。
    let next_num = doc.xref.max_size();
    let page_num = next_num;
    let content_num = next_num + 1;
    let pages = doc.get_object(2).expect("pages").expect("pages node").value;
    let pages_dict = pages.as_dict().expect("pages dict");
    let kids = pages_dict
        .get("Kids")
        .and_then(|v| v.as_array())
        .expect("/Kids array");
    let first_page = doc
        .get_object(3)
        .expect("first page")
        .expect("page object")
        .value;
    let first_page_dict = first_page.as_dict().expect("page dict");

    let content_bytes = b"BT /F1 24 Tf 72 650 Td (Second) Tj ET\n".to_vec();
    let content_dict = Dict::new().insert("Length", Value::Integer(content_bytes.len() as i64));
    doc.set_object(Object::new(
        content_num,
        0,
        Value::Stream(content_dict, StreamData::Owned(content_bytes)),
    ));
    let mut new_page = Dict::new()
        .insert("Type", Value::Name("Page".into()))
        .insert("Parent", Value::Ref(2, 0))
        .insert("Contents", Value::Ref(content_num, 0));
    if let Some(v) = first_page_dict.get("MediaBox") {
        new_page = new_page.insert("MediaBox", v.clone());
    }
    if let Some(v) = first_page_dict.get("Resources") {
        new_page = new_page.insert("Resources", v.clone());
    }
    doc.set_object(Object::new(page_num, 0, Value::Dict(new_page)));
    let updated_pages = Value::Dict(
        pages_dict
            .clone()
            .insert(
                "Kids",
                Value::Array(kids.clone().push(Value::Ref(page_num, 0))),
            )
            .insert("Count", Value::Integer(kids.len() as i64 + 1)),
    );
    doc.set_object(Object::new(2, 0, updated_pages));

    let original_start = doc.startxref.0;
    let mut buf2 = Vec::new();
    DocumentWriter::new()
        .build_incremental(&doc, &mut buf2)
        .expect("incremental build");
    assert_eq!(&buf2[..buf.len()], &buf[..], "增量输出应以原文件为前缀");

    let revised = parse_buf(&buf2);
    assert_eq!(revised.page_count().expect("page count after update"), 2);
    assert_eq!(
        revised.trailer().dict.get("Prev"),
        Some(&Value::Integer(original_start)),
        "trailer /Prev 应指向原 startxref"
    );
    assert_eq!(revised.extract_page_text(0).as_deref(), Some("First"));
    assert_eq!(revised.extract_page_text(1).as_deref(), Some("Second"));
}

/// 增量更新删除与遮蔽（E7）：注解对象被删除后回读应消失，
/// 其余注解保留；输出可再次被完整重写（全量 rewrite 兼容增量文档）。
#[test]
fn builder_incremental_update_deletes_annotation_and_supports_full_rewrite() {
    use pdf_engine::core::builder::{AnnotationKind, Font, PageSize, PdfAnnotation, PdfBuilder};
    use pdf_engine::core::writer::DocumentWriter;

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    builder.text(p0, &Font::helvetica(18.0), 72.0, 700.0, "Notes");
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 600.0, 92.0, 620.0])
            .with_contents("keep me"),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 500.0, 92.0, 520.0])
            .with_contents("drop me"),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    let before = doc.page_annotations(0).expect("annotations before");
    assert_eq!(before.len(), 2);

    // 删除第二条注解：更新页面 /Annots 并把注解对象标记为删除。
    let dropped = match before[1].reference {
        Some((num, _)) => num,
        None => panic!("second annotation should be an indirect object"),
    };
    let kept_ref = match before[0].reference {
        Some((num, _)) => Value::Ref(num, 0),
        None => panic!("first annotation should be an indirect object"),
    };
    doc.delete_object(dropped);
    let page = doc.get_object(3).expect("page").expect("page object").value;
    let page_dict = page.as_dict().expect("page dict");
    let updated_page = Value::Dict(
        page_dict
            .clone()
            .insert("Annots", Value::Array(Array(vec![kept_ref]))),
    );
    doc.set_object(Object::new(3, 0, updated_page));

    let mut buf2 = Vec::new();
    DocumentWriter::new()
        .build_incremental(&doc, &mut buf2)
        .expect("incremental delete");
    let revised = parse_buf(&buf2);
    let after = revised.page_annotations(0).expect("annotations after");
    assert_eq!(after.len(), 1, "删除后仅剩一条注解");
    fn decode_text(value: &Value) -> String {
        use pdf_engine::core::types::PdfTextString;
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

    assert_eq!(
        after[0].contents.as_ref().map(decode_text),
        Some("keep me".to_string()),
        "保留的是第一条注解"
    );
    assert!(
        revised.get_object(dropped).expect("lookup").is_none(),
        "被删对象应被自由条目遮蔽"
    );

    // 全量重写增量文档：读端沿 /Prev 链加载全部对象后重新序列化。
    let mut flat = Vec::new();
    DocumentWriter::new()
        .build(&revised, &mut flat)
        .expect("full rewrite of incremental document");
    let rewritten = parse_buf(&flat);
    assert!(
        rewritten.trailer().dict.get("Prev").is_none(),
        "全量重写输出自包含，不应保留 /Prev 链"
    );
    assert!(
        rewritten.warnings().is_empty(),
        "重写文档回读不应产生告警: {:?}",
        rewritten.warnings()
    );
    let final_annots = rewritten.page_annotations(0).expect("annotations final");
    assert_eq!(final_annots.len(), 1);
    assert_eq!(rewritten.extract_page_text(0).as_deref(), Some("Notes"));
}

/// 写端签名增量更新（F1 / 12.8.1）：builder 单页文档 → 解析 →
/// `sign_incremental`（恒等签名器，Contents = 覆盖段 SHA-256 摘要）→
/// 回读验证 ByteRange/摘要一致、页面文本仍可读；篡改覆盖字节后
/// 摘要不再匹配。
#[test]
fn builder_signature_incremental_protects_and_detects_tampering() {
    use pdf_engine::core::builder::{Font, PageSize, PdfBuilder};
    use pdf_engine::core::reader::signature::{DigestAlgorithm, RustCryptoSignatureBackend};
    use pdf_engine::core::writer::{DocumentWriter, SigningSpec};

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    builder.text(p0, &Font::helvetica(24.0), 72.0, 700.0, "Contract");
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let spec = SigningSpec::new(DigestAlgorithm::Sha256, 32)
        .with_field_name("Approval")
        .with_name("Acme");
    let signed = DocumentWriter::new()
        .sign_incremental(&doc, &spec, &RustCryptoSignatureBackend)
        .expect("sign_incremental");
    assert!(
        signed.bytes.starts_with(&buf),
        "签名增量输出应保留原文件字节"
    );

    let revised = parse_buf(&signed.bytes);
    assert_eq!(revised.extract_page_text(0).as_deref(), Some("Contract"));
    let sigs = revised.document_signatures().expect("signatures");
    assert_eq!(sigs.len(), 1);
    let sig = &sigs[0];
    assert_eq!(sig.name.as_deref(), Some("Acme"));
    revised
        .validate_signature_byte_range(sig)
        .expect("byte range covers the signed file");
    let Value::HexString(hex) = sig.contents.as_ref().expect("contents") else {
        panic!("contents shape");
    };
    let stored: Vec<u8> = (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
        .collect();
    let digest = revised
        .signature_digest(sig, DigestAlgorithm::Sha256, &RustCryptoSignatureBackend)
        .expect("digest");
    assert_eq!(stored, digest, "Contents 应等于覆盖段摘要（恒等签名器）");

    // 篡改覆盖段内的文本字节（等长替换），重算摘要应不再匹配。
    let at = signed
        .bytes
        .windows(b"Contract".len())
        .position(|w| w == b"Contract")
        .expect("literal in content stream");
    assert!(
        (at as i64) < sig.byte_range[1],
        "被篡改字节位于第一段覆盖区内"
    );
    let mut tampered = signed.bytes.clone();
    tampered[at + 3] = b'X';
    let tampered_doc = parse_buf(&tampered);
    let tampered_sigs = tampered_doc.document_signatures().expect("tampered sigs");
    let recomputed = tampered_doc
        .signature_digest(
            &tampered_sigs[0],
            DigestAlgorithm::Sha256,
            &RustCryptoSignatureBackend,
        )
        .expect("tampered digest");
    assert_ne!(stored, recomputed, "篡改覆盖字节后摘要必须改变");
}
