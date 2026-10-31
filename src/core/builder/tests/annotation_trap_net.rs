//! Trap 网络注解写端（14.11.6，Table 366/367）：`/F` 恒为 68、两种互斥
//! 跟踪形式、`/AP /N` 表单的 Table 367 条目、PDF 1.3/1.4 门禁、每页至多
//! 一个且位于 `/Annots` 末尾，以及失败时零字节契约。

use super::support::{annot_object, assert_text_entry};
use super::*;
use crate::core::builder::types::text_string_value;
use crate::core::content::ContentStream;
use crate::core::reader::annotation::ProcessColorModel;
use crate::core::types::{PdfDate, PdfFeature};

fn date() -> PdfDate {
    PdfDate::parse("D:20240101120000Z").expect("valid PDF date")
}

fn form() -> FormXObject {
    FormXObject::new([0.0, 0.0, 100.0, 100.0], ContentStream::new())
        .with_pcm(ProcessColorModel::DeviceCMYK)
}

fn trap_net(rect: [f64; 4]) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::TrapNet, rect)
}

/// 构建单页文档并按指定版本、可选外观写出。
fn build_one(
    annot: PdfAnnotation,
    form: Option<FormXObject>,
    version: PdfVersion,
) -> (Result<(), WriteError>, Vec<u8>) {
    let mut b = PdfBuilder::with_version(version);
    let p0 = b.page(PageSize::A4);
    let annot = match form {
        Some(form) => {
            let handle = b.form_xobject(&form);
            annot.with_appearance(handle)
        }
        None => annot,
    };
    b.add_annotation(p0, annot);
    let mut out = Vec::new();
    let result = b.build(&mut out);
    (result, out)
}

/// 断言构建失败、消息命中且未写出任何字节。
fn expect_rejected(annot: PdfAnnotation, fragment: &str) {
    let (result, out) = build_one(annot, None, PdfVersion::V1_4);
    let error = result.expect_err("trap network contract violation");
    let message = error.to_string();
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "expected InvalidObject, got {message}"
    );
    assert!(message.contains(fragment), "unexpected message: {message}");
    assert_eq!(out.len(), 0, "rejected build must not emit bytes");
}

/// 表单条目校验失败同样在写出字节前拒绝。
fn expect_form_rejected(form: FormXObject, annot: PdfAnnotation, fragment: &str) {
    let (result, out) = build_one(annot, Some(form), PdfVersion::V1_4);
    let error = result.expect_err("trap network form contract violation");
    let message = error.to_string();
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "expected InvalidObject, got {message}"
    );
    assert!(message.contains(fragment), "unexpected message: {message}");
    assert_eq!(out.len(), 0, "rejected build must not emit bytes");
}

#[test]
fn trap_net_writes_flags_tracking_and_form_entries() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p0 = b.page(PageSize::A4);
    let handle = b.form_xobject(
        &form()
            .with_separation_color_name("Spot")
            .with_trap_region(2)
            .with_trap_styles("Red:0.0,Blue:0.0")
            .with_font_fauxing(Dict::new().insert("Type", Value::Name("Font".into()))),
    );
    b.add_annotation(
        p0,
        trap_net([10.0, 10.0, 20.0, 20.0])
            .with_trap_network_last_modified(date())
            .with_appearance(handle),
    );
    let doc = b.assemble_document().expect("assemble");
    {
        let features = doc.features.borrow();
        assert!(
            features.contains(PdfFeature::TrapNetworks),
            "Trap 网络应写入特性画像"
        );
        assert!(
            features.contains(PdfFeature::TrapNetworkModificationDate),
            "/LastModified 为 PDF 1.4 条目"
        );
    }

    let annot = annot_object(&doc, 3, 0);
    assert_eq!(annot.get("Subtype"), Some(&Value::Name("TrapNet".into())));
    assert_eq!(
        annot.get("F"),
        Some(&Value::Integer(68)),
        "未显式设置标志位时仍须写出 Print | ReadOnly"
    );
    assert_text_entry(&annot, "LastModified");
    assert!(
        annot.get("Version").is_none(),
        "/LastModified 与 /Version 互斥"
    );
    assert!(
        annot.get("AnnotStates").is_none(),
        "/LastModified 与 /AnnotStates 互斥"
    );
    // `/AP /N` 直接是表单流，禁止 `/AS`。
    assert!(annot.get("AS").is_none());
    let normal = annot
        .get("AP")
        .and_then(Value::as_dict)
        .and_then(|ap| ap.get("N"))
        .and_then(Value::as_ref)
        .expect("/AP /N reference");
    let object = doc
        .get_object(normal.0)
        .expect("form object")
        .expect("form exists");
    let Value::Stream(form_dict, _) = &object.value else {
        panic!("appearance must be a form stream");
    };
    assert_eq!(form_dict.get("Subtype"), Some(&Value::Name("Form".into())));
    assert_eq!(
        form_dict.get("PCM"),
        Some(&Value::Name("DeviceCMYK".into())),
        "Table 367 的 /PCM 为必填 name"
    );
    let names = form_dict
        .get("SeparationColorNames")
        .and_then(Value::as_array)
        .expect("/SeparationColorNames array");
    assert_eq!(names.0, vec![Value::Name("Spot".into())]);
    let regions = form_dict
        .get("TrapRegions")
        .and_then(Value::as_array)
        .expect("/TrapRegions array");
    assert_eq!(regions.0, vec![Value::Ref(2, 0)]);
    assert_eq!(
        form_dict.get("TrapStyles"),
        Some(&text_string_value("Red:0.0,Blue:0.0")),
        "Table 367 的 /TrapStyles 为文本串"
    );
    let fonts = form_dict
        .get("FontFauxing")
        .and_then(Value::as_array)
        .expect("/FontFauxing array");
    assert_eq!(fonts.0.len(), 1);
    assert_eq!(
        fonts.0[0].as_dict().and_then(|dict| dict.get("Type")),
        Some(&Value::Name("Font".into()))
    );
}

#[test]
fn trap_net_writes_version_and_annot_states_at_1_3() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p0 = b.page(PageSize::A4);
    let handle = b.form_xobject(&form());
    // `/AnnotStates` 与页面其余注解一一对应：先放一个 Text 注解。
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0]),
    );
    b.add_annotation(
        p0,
        trap_net([10.0, 10.0, 20.0, 20.0])
            .with_trap_network_version(
                vec![Value::Ref(1, 0), Value::Ref(2, 0)],
                vec![Some("On".into()), None],
            )
            .with_appearance(handle),
    );
    let doc = b.assemble_document().expect("assemble");
    {
        let features = doc.features.borrow();
        assert!(features.contains(PdfFeature::TrapNetworks));
        assert!(
            !features.contains(PdfFeature::TrapNetworkModificationDate),
            "无 /LastModified 时不应登记 1.4 条目"
        );
    }

    // Trap 网络必须位于 `/Annots` 末尾，故索引为 1。
    let annot = annot_object(&doc, 3, 1);
    assert_eq!(annot.get("Subtype"), Some(&Value::Name("TrapNet".into())));
    assert_eq!(annot.get("F"), Some(&Value::Integer(68)));
    assert!(annot.get("LastModified").is_none());
    let version = annot
        .get("Version")
        .and_then(Value::as_array)
        .expect("/Version array");
    assert_eq!(version.0, vec![Value::Ref(1, 0), Value::Ref(2, 0)]);
    let states = annot
        .get("AnnotStates")
        .and_then(Value::as_array)
        .expect("/AnnotStates array");
    assert_eq!(
        states.0,
        vec![Value::Name("On".into()), Value::Null],
        "null 项须原样写出"
    );
}

#[test]
fn trap_net_last_modified_requires_pdf_1_4() {
    // `/LastModified` 自 PDF 1.4 引入（Table 366 注 2）。
    let (result, out) = build_one(
        trap_net([0.0, 0.0, 10.0, 10.0]).with_trap_network_last_modified(date()),
        Some(form()),
        PdfVersion::V1_3,
    );
    let error = result.expect_err("LastModified below PDF 1.4");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("TrapNet annotation /LastModified requires PDF 1.4")),
        "{error}"
    );
    assert_eq!(out.len(), 0);

    // 注解子类型自 PDF 1.3 引入（Table 169）。
    let (result, out) = build_one(
        trap_net([0.0, 0.0, 10.0, 10.0])
            .with_trap_network_version(vec![Value::Ref(1, 0)], vec![Some("On".into())]),
        Some(form()),
        PdfVersion::V1_2,
    );
    let error = result.expect_err("TrapNet below PDF 1.3");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("TrapNet annotations require PDF 1.3")),
        "{error}"
    );
    assert_eq!(out.len(), 0);

    // Table 367 表单条目独立门禁：1.2 的普通外观表单也不能带 /PCM。
    let (result, out) = build_one(
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0]),
        Some(form()),
        PdfVersion::V1_2,
    );
    let error = result.expect_err("trap form entries below PDF 1.3");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("trap network form entries require PDF 1.3")),
        "{error}"
    );
    assert_eq!(out.len(), 0);

    let (result, out) = build_one(
        trap_net([0.0, 0.0, 10.0, 10.0]).with_trap_network_last_modified(date()),
        Some(form()),
        PdfVersion::V1_4,
    );
    result.expect("TrapNet is writable from PDF 1.4");
    assert!(!out.is_empty());
}

#[test]
fn trap_net_rejects_invalid_tracking_and_form_entries() {
    // Table 366：两种跟踪形式恰有其一。
    expect_rejected(
        trap_net([0.0, 0.0, 10.0, 10.0]),
        "requires either /LastModified or /Version and /AnnotStates",
    );
    expect_rejected(
        trap_net([0.0, 0.0, 10.0, 10.0])
            .with_trap_network_last_modified(date())
            .with_flags(4),
        "/F must be 0 or 68",
    );
    // 跟踪条目只属于 TrapNet 注解。
    expect_rejected(
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0])
            .with_trap_network_last_modified(date()),
        "specific to TrapNet",
    );
    // `/Version` 每项须为间接引用。
    expect_rejected(
        trap_net([0.0, 0.0, 10.0, 10.0]).with_trap_network_version(vec![Value::Integer(1)], vec![]),
        "must be indirect references",
    );
    // `/AnnotStates` 的 name 项须为合法 name。
    expect_rejected(
        trap_net([0.0, 0.0, 10.0, 10.0])
            .with_trap_network_version(vec![Value::Ref(1, 0)], vec![Some("Bad Name".into())]),
        "/AnnotStates entry contains name delimiters",
    );
    // `/AP` 正常外观必填。
    expect_rejected(
        trap_net([0.0, 0.0, 10.0, 10.0]).with_trap_network_last_modified(date()),
        "requires a normal appearance",
    );
    // 多状态外观必须声明 `/AS`。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p0 = b.page(PageSize::A4);
    let on = b.form_xobject(&form());
    let off = b.form_xobject(&form());
    b.add_annotation(
        p0,
        trap_net([0.0, 0.0, 10.0, 10.0])
            .with_trap_network_last_modified(date())
            .with_appearance_state("On", on)
            .with_appearance_state("Off", off),
    );
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("multiple appearances without /AS");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("multiple appearances"),
        "{error}"
    );
    assert_eq!(out.len(), 0);

    // Table 367 表单条目：声明任一条目时 `/PCM` 必填。
    expect_form_rejected(
        FormXObject::new([0.0, 0.0, 100.0, 100.0], ContentStream::new()),
        trap_net([0.0, 0.0, 10.0, 10.0]).with_trap_network_last_modified(date()),
        "must declare /PCM",
    );
    // `/PCM` 只接受标准过程颜色模型。
    expect_form_rejected(
        form().with_pcm(ProcessColorModel::Other("DeviceLab".into())),
        trap_net([0.0, 0.0, 10.0, 10.0]).with_trap_network_last_modified(date()),
        "is not a standard process color model",
    );
    // 色料名须唯一且为合法 name。
    expect_form_rejected(
        form()
            .with_separation_color_name("Spot")
            .with_separation_color_name("Spot"),
        trap_net([0.0, 0.0, 10.0, 10.0]).with_trap_network_last_modified(date()),
        "registered twice",
    );
    expect_form_rejected(
        form().with_separation_color_name("Bad Name"),
        trap_net([0.0, 0.0, 10.0, 10.0]).with_trap_network_last_modified(date()),
        "trap network form separation color name",
    );
    // TrapRegion 对象号须为正。
    expect_form_rejected(
        form().with_trap_region(0),
        trap_net([0.0, 0.0, 10.0, 10.0]).with_trap_network_last_modified(date()),
        "object numbers must be positive",
    );
}

#[test]
fn trap_net_is_limited_to_one_per_page() {
    // 合规布局：Trap 网络即使先注册也须排在其余注解之后。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p0 = b.page(PageSize::A4);
    let p1 = b.page(PageSize::A4);
    let handle = b.form_xobject(&form());
    b.add_annotation(
        p0,
        trap_net([10.0, 10.0, 20.0, 20.0])
            .with_trap_network_last_modified(date())
            .with_appearance(handle),
    );
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0]),
    );
    b.add_annotation(
        p1,
        trap_net([30.0, 30.0, 40.0, 40.0])
            .with_trap_network_last_modified(date())
            .with_appearance(handle),
    );
    let doc = b.assemble_document().expect("one TrapNet per page");
    assert_eq!(
        annot_object(&doc, 3, 0).get("Subtype"),
        Some(&Value::Name("Text".into()))
    );
    assert_eq!(
        annot_object(&doc, 3, 1).get("Subtype"),
        Some(&Value::Name("TrapNet".into())),
        "Trap 网络必须是 /Annots 的最后一项"
    );

    // 同一页两个 Trap 网络在写出字节前被拒绝。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p0 = b.page(PageSize::A4);
    let handle = b.form_xobject(&form());
    for rect in [[0.0, 0.0, 10.0, 10.0], [10.0, 10.0, 20.0, 20.0]] {
        b.add_annotation(
            p0,
            trap_net(rect)
                .with_trap_network_last_modified(date())
                .with_appearance(handle),
        );
    }
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("two TrapNet on one page");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error
            .to_string()
            .contains("at most one trap network annotation per page"),
        "{error}"
    );
    assert_eq!(out.len(), 0);
}
