//! 字体特征与字体选择器属性（ISO 32000-1 14.8.2.4.3，Table 331/332）的
//! 写 → 解析 → 推导往返：`PdfBuilder` 在 PDF 1.5 页面上写出标准 14 内置
//! 字体（无 `/FontDescriptor`，读端须改用众所周知值）与带描述符的自定义
//! Type 1 字体（`/Flags` + `/FontFamily` + `/FontWeight` + `/FontStretch`），
//! 还有只给 `/Flags`、三个选择器条目缺失的字体（读端按本库的名字扫描
//! 回退并逐项标记 `name_derived`）。
//! 读端 `Document::page_font_character_properties` 按 `Tf` 选中的资源名
//! 给出推导结果，特征画像登记 `PdfFeature::FontCharacteristics`（1.4）
//! 与 `PdfFeature::FontSelectorAttributes`（1.5）。
//! 第二个测试覆盖 Table 332 `FontSize`：写端产出的真实 `Tm` 操作符与
//! 表单 `/Matrix`、绘制 `cm` 在往返后的取舍。

use super::*;
use pdf_engine::core::builder::{Font, FontDescriptor, FormXObject, PageSize, PdfBuilder};
use pdf_engine::core::content::{ContentStream, Op};
use pdf_engine::core::reader::accessibility::{
    CharacterPropertiesError, FontStyle, FontVariant, GenericFontFamily, NameDerivedAttribute,
};
use pdf_engine::core::types::{PdfFeature, PdfVersion};

fn s(value: &str) -> String {
    value.to_owned()
}

/// 非标准 14 的 Type 1 字体：Table 111 要求 `/Widths`，故给出最小宽度表。
fn custom(name: &str, descriptor: FontDescriptor) -> Font {
    Font {
        name: s(name),
        ..Font::helvetica(12.0)
    }
    .with_widths(32, &[600.0; 96])
    .with_descriptor(descriptor)
}

#[test]
fn builder_font_characteristics_roundtrip_derives_from_descriptor_and_well_known() {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_5);
    let page = builder.page(PageSize::A4);
    // F1：标准 14 Helvetica，描述符可省略（9.6.2.2）。
    builder.text(
        page,
        &Font::helvetica(12.0),
        72.0,
        720.0,
        "Standard fourteen",
    );
    // F2：自定义字体，描述符带 Table 331/332 用到的全部条目。
    // Flags = Serif(位 2) | SmallCap(位 18) → Serifed/Smallcap、
    // GenericFontFamily Serif、FontVariant SmallCaps。
    let serif = custom(
        "WidgetSerif",
        FontDescriptor::new()
            .with_flags((1 << 1) | (1 << 17))
            .with_family("Widget")
            .with_weight(700.0)
            .with_stretch("Condensed"),
    );
    builder.text(page, &serif, 72.0, 700.0, "Descriptor driven");
    // F3：只有 /Flags 的描述符，三个选择器条目缺失 → 名字回退（库策略）。
    // Flags = FixedPitch(位 1) | Italic(位 7) → Proportional false、
    // GenericFontFamily Monospace、FontStyle Italic。
    let mono = custom(
        "GadgetMono",
        FontDescriptor::new().with_flags((1 << 0) | (1 << 6)),
    );
    builder.text(page, &mono, 72.0, 680.0, "Name fallback");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let raw = String::from_utf8_lossy(&buf).into_owned();
    assert!(raw.contains("/FontDescriptor"), "{raw}");
    assert!(raw.contains("/FontFamily (Widget)"), "{raw}");
    assert!(raw.contains("/FontStretch /Condensed"), "{raw}");

    let doc = parse_buf(&buf);

    let derived = doc
        .page_font_character_properties(0)
        .expect("页面字体特征推导应成功");
    assert_eq!(
        derived.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["F1", "F2", "F3"],
        "只收集 Tf 选中的字体资源: {derived:?}"
    );

    // F1：标准 14 无描述符 → 众所周知值（14.8.2.4.3 shall）。
    let helvetica = &derived["F1"];
    assert!(helvetica.from_standard14_well_known, "{helvetica:?}");
    assert_eq!(helvetica.serifed, Some(false));
    assert_eq!(
        helvetica.proportional,
        Some(true),
        "Proportional 是 FixedPitch 取反"
    );
    assert_eq!(helvetica.italic, Some(false));
    assert_eq!(helvetica.smallcap, Some(false));
    assert_eq!(helvetica.font_family.as_deref(), Some("Helvetica"));
    assert_eq!(
        helvetica.generic_font_family,
        Some(GenericFontFamily::SansSerif)
    );
    assert_eq!(helvetica.font_weight, Some(400.0));
    assert_eq!(helvetica.font_stretch.as_deref(), Some("Normal"));
    assert_eq!(helvetica.font_style, Some(FontStyle::Normal));
    assert_eq!(helvetica.font_variant, Some(FontVariant::Normal));
    assert!(helvetica.name_derived.is_empty(), "{helvetica:?}");

    // F2：Table 331 四项 + Table 332 三条目全部来自描述符。
    let widget = &derived["F2"];
    assert!(!widget.from_standard14_well_known);
    assert_eq!(widget.serifed, Some(true));
    assert_eq!(widget.proportional, Some(true));
    assert_eq!(widget.italic, Some(false));
    assert_eq!(widget.smallcap, Some(true));
    assert_eq!(widget.font_family.as_deref(), Some("Widget"));
    assert_eq!(widget.generic_font_family, Some(GenericFontFamily::Serif));
    assert_eq!(widget.font_weight, Some(700.0));
    assert_eq!(widget.font_stretch.as_deref(), Some("Condensed"));
    assert_eq!(widget.font_style, Some(FontStyle::Normal));
    assert_eq!(widget.font_variant, Some(FontVariant::SmallCaps));
    assert!(
        widget.name_derived.is_empty(),
        "描述符给了条目即非名字推导: {widget:?}"
    );

    // F3：标志位仍按描述符推导；族名/权重/拉伸缺失 → 名字扫描 + 标记。
    let gadget = &derived["F3"];
    assert_eq!(gadget.proportional, Some(false), "FixedPitch 置位取反");
    assert_eq!(gadget.serifed, Some(false));
    assert_eq!(gadget.italic, Some(true));
    assert_eq!(
        gadget.generic_font_family,
        Some(GenericFontFamily::Monospace)
    );
    assert_eq!(gadget.font_style, Some(FontStyle::Italic));
    assert_eq!(gadget.font_variant, Some(FontVariant::Normal));
    assert_eq!(gadget.font_family.as_deref(), Some("GadgetMono"));
    assert_eq!(
        gadget.font_weight,
        Some(400.0),
        "名字无权重词时按本库默认 400"
    );
    assert_eq!(gadget.font_stretch.as_deref(), Some("Normal"));
    assert_eq!(
        gadget.name_derived.iter().collect::<Vec<_>>(),
        vec![
            &NameDerivedAttribute::FontFamily,
            &NameDerivedAttribute::FontWeight,
            &NameDerivedAttribute::FontStretch,
        ],
        "缺失的选择器条目须逐项标记为名字推导（库策略）"
    );

    // 逐资源名查询与页面集合一致；未用的名字给类型化错误而非 panic。
    let single = doc
        .page_font_character_property(0, "F2")
        .expect("按资源名查询应成功");
    assert_eq!(single, widget.clone());
    let missing = doc.page_font_character_property(0, "F9");
    assert!(
        matches!(
            missing,
            Err(CharacterPropertiesError::MissingFontResource(ref name)) if name == "F9"
        ),
        "{missing:?}"
    );

    let features = doc.features.borrow();
    assert!(
        features.contains(PdfFeature::FontCharacteristics),
        "推导成功应登记 Table 331 的 1.4 特征"
    );
    assert!(
        features.contains(PdfFeature::FontSelectorAttributes),
        "解析含选择器条目的描述符应登记 Table 332 的 1.5 特征"
    );
}

/// 内含**真实** `Tm` 缩放操作的表单：
/// `BT 2 0 0 2 0 0 Tm /F1 6 Tf 5 5 Td (…) Tj ET`。
///
/// 同时挂 `/Matrix [3 0 0 3 0 0]`，绘制时 `cm` 用 `[1.5 0 0 1.5 …]`：
/// 两者都是设备/表单坐标空间变换，Table 332 的 `FontSize` 只说「取自当前
/// 文本矩阵的 a、b、c、d」，故都不该折算进去。
fn scaled_text_form(font_name: &str, size: f64) -> FormXObject {
    let mut content = ContentStream::new();
    content.begin_text();
    content.push(Op::SetTextMatrix([2.0, 0.0, 0.0, 2.0, 0.0, 0.0]));
    content.set_font(font_name, size);
    content.show_text_raw(5.0, 5.0, "Scaled inside form");
    content.end_text();
    FormXObject::new([0.0, 0.0, 160.0, 40.0], content).with_matrix([3.0, 0.0, 0.0, 3.0, 0.0, 0.0])
}

#[test]
fn builder_font_size_roundtrip_uses_text_matrix_and_ignores_form_and_ctm_scale() {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_5);
    // 页 0：`PdfBuilder::text` 产出 `BT /F1 11 Tf x y Td (…) Tj ET`，
    // Tm 为恒等 → FontSize 就是 Tf 字号。
    let plain = builder.page(PageSize::A4);
    builder.text(
        plain,
        &Font::helvetica(11.0),
        72.0,
        720.0,
        "Identity matrix",
    );

    // 页 1：页面上同一资源名以 Tf 6.0 显示（恒等 Tm → 6.0），表单内同一
    // Tf 字号被真实 Tm 放大 2 倍 → 12.0；取页内观测到的最大值。
    let scaled = builder.page(PageSize::A4);
    builder.text(scaled, &Font::helvetica(6.0), 72.0, 720.0, "Small on page");
    let form = builder.form_xobject(&scaled_text_form("F1", 6.0));
    builder.draw_form(scaled, form, [1.5, 0.0, 0.0, 1.5, 72.0, 400.0]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let raw = String::from_utf8_lossy(&buf).into_owned();
    assert!(
        raw.contains("2.0 0.0 0.0 2.0 0.0 0.0 Tm"),
        "写端应产出真实的文本矩阵操作符: {raw}"
    );
    assert!(
        raw.contains("/F1 6.0 Tf"),
        "表单内 Tf 字号须原样写出: {raw}"
    );

    let doc = parse_buf(&buf);

    let plain_props = doc
        .page_font_character_properties(0)
        .expect("页 0 字体特征推导");
    assert_eq!(
        plain_props.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["F1"]
    );
    assert_eq!(
        plain_props["F1"].font_size,
        Some(11.0),
        "恒等 Tm 下 FontSize = Tf 字号"
    );

    let scaled_props = doc
        .page_font_character_properties(1)
        .expect("页 1 字体特征推导");
    assert_eq!(
        scaled_props["F1"].font_size,
        Some(12.0),
        "Tf 6.0 × Tm 竖向缩放 2.0；表单 /Matrix(3) 与绘制 cm(1.5) 均不折算"
    );

    // 逐资源名查询与页面集合走同一推导，结果一致。
    let single = doc
        .page_font_character_property(1, "F1")
        .expect("按资源名查询字号");
    assert_eq!(single, scaled_props["F1"]);
    assert_eq!(single.font_size, Some(12.0));
}
