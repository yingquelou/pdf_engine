//! 字体特征与字体选择器属性读端测试（ISO 32000-1 14.8.2.4.3，
//! Table 331/Table 332）。

use super::character_properties_derive::standard14_well_known;
use crate::core::reader::accessibility::{
    CharacterProperties, CharacterPropertiesError, FontStyle, FontVariant, GenericFontFamily,
    NameDerivedAttribute,
};
use crate::core::types::{Array, Dict, Document, Object, PdfFeature, StreamData, Trailer, Value};

const CATALOG: i64 = 1;
const PAGES: i64 = 2;
const PAGE: i64 = 3;
const FONT: i64 = 4;
const DESCRIPTOR: i64 = 5;

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

fn font(subtype: &str, base_font: Option<&str>, descriptor: Option<Value>) -> Value {
    let mut dict = Dict::new()
        .insert("Type", name("Font"))
        .insert("Subtype", name(subtype));
    if let Some(base_font) = base_font {
        dict = dict.insert("BaseFont", name(base_font));
    }
    if let Some(descriptor) = descriptor {
        dict = dict.insert("FontDescriptor", descriptor);
    }
    Value::Dict(dict)
}

fn descriptor(flags: Option<u32>, entries: Vec<(&str, Value)>) -> Value {
    let mut dict = Dict::new().insert("Type", name("FontDescriptor"));
    if let Some(flags) = flags {
        dict = dict.insert("Flags", Value::Integer(i64::from(flags)));
    }
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    Value::Dict(dict)
}

fn derive(font: &Value) -> CharacterProperties {
    let document = Document::new();
    document
        .font_character_properties(font)
        .unwrap_or_else(|error| panic!("特征推导应成功: {error}"))
}

/// 单页文档：资源 /Font 为给定条目、内容流为给定字节。
fn page_document(resources: Dict, content: &[u8]) -> Document {
    let mut document = Document::new();
    document.set_object(Object::new(
        PAGES,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", Value::Array(Array(vec![Value::Ref(PAGE, 0)])))
                .insert("Count", Value::Integer(1)),
        ),
    ));
    document.set_object(Object::new(
        PAGE,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Page"))
                .insert("Parent", Value::Ref(PAGES, 0))
                .insert("Resources", Value::Dict(resources))
                .insert(
                    "Contents",
                    Value::Stream(Dict::new(), StreamData::Owned(content.to_vec())),
                ),
        ),
    ));
    document.set_object(Object::new(
        CATALOG,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", Value::Ref(PAGES, 0)),
        ),
    ));
    document.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(CATALOG, 0)),
    };
    document
}

#[test]
fn derives_serifed_from_serif_flag_in_font_descriptor() {
    // Table 331（shall）：Serifed ＝ /Flags 的 Serif 位（位 2）。
    let serif = derive(&font(
        "Type1",
        Some("CustomSerif"),
        Some(descriptor(Some(2), vec![])),
    ));
    assert_eq!(serif.serifed, Some(true));
    let sans = derive(&font(
        "Type1",
        Some("CustomSans"),
        Some(descriptor(Some(32), vec![])),
    ));
    assert_eq!(sans.serifed, Some(false));
}

#[test]
fn derives_proportional_as_complement_of_fixed_pitch_flag() {
    // Table 331（shall）：Proportional ＝ FixedPitch 位的**取反**。
    let fixed = derive(&font(
        "Type1",
        Some("CustomMono"),
        Some(descriptor(Some(1), vec![])),
    ));
    assert_eq!(fixed.proportional, Some(false));
    assert_eq!(fixed.serifed, Some(false));
    let flowing = derive(&font(
        "Type1",
        Some("CustomProportional"),
        Some(descriptor(Some(2), vec![])),
    ));
    assert_eq!(flowing.proportional, Some(true));
}

#[test]
fn derives_italic_and_smallcap_flags_into_table331_and_selector_rules() {
    // Table 331：Italic（位 7）与 SmallCap（位 18）；Table 332 的
    // FontStyle/FontVariant 是同两个位的 shall 规则。
    let properties = derive(&font(
        "Type1",
        Some("CustomSmall"),
        Some(descriptor(Some((1 << 6) | (1 << 17)), vec![])),
    ));
    assert_eq!(properties.italic, Some(true));
    assert_eq!(properties.smallcap, Some(true));
    assert_eq!(properties.font_style, Some(FontStyle::Italic));
    assert_eq!(properties.font_variant, Some(FontVariant::SmallCaps));
    assert_eq!(properties.serifed, Some(false));
    assert_eq!(properties.proportional, Some(true));
}

#[test]
fn generic_font_family_branches_follow_table332_conditions() {
    // Table 332（shall）四分支：Monospace 压过一切 FixedPitch；Script 且非
    // FixedPitch 归 Cursive（即使 Serif 同置）；Serif 行要求 FixedPitch 与
    // Script 均清；三者全清为 SansSerif。
    let cases = [
        (2u32, GenericFontFamily::Serif),
        (0, GenericFontFamily::SansSerif),
        (32, GenericFontFamily::SansSerif),
        (8, GenericFontFamily::Cursive),
        (2 | 8, GenericFontFamily::Cursive),
        (1, GenericFontFamily::Monospace),
        (1 | 2, GenericFontFamily::Monospace),
        (1 | 8, GenericFontFamily::Monospace),
    ];
    for (flags, expected) in cases {
        let properties = derive(&font(
            "Type1",
            Some("CustomBranch"),
            Some(descriptor(Some(flags), vec![])),
        ));
        assert_eq!(
            properties.generic_font_family,
            Some(expected),
            "flags={flags}"
        );
    }
}

#[test]
fn uses_well_known_values_for_standard_14_font_without_descriptor() {
    // 14.8.2.4.3（shall）：标准 14 描述符可缺省，"the well-known values
    // for those fonts shall be used"。Helvetica：无 Serif/FixedPitch →
    // SansSerif；族名 Helvetica、权重 400、拉伸 Normal。
    let document = Document::new();
    let properties = document
        .font_character_properties(&font("Type1", Some("Helvetica"), None))
        .expect("Helvetica 应可推导");
    assert_eq!(
        properties,
        CharacterProperties {
            serifed: Some(false),
            proportional: Some(true),
            italic: Some(false),
            smallcap: Some(false),
            font_family: Some("Helvetica".to_owned()),
            generic_font_family: Some(GenericFontFamily::SansSerif),
            // 字体级入口没有内容流上下文，Table 332 `FontSize` 不物化。
            font_size: None,
            font_weight: Some(400.0),
            font_stretch: Some("Normal".to_owned()),
            font_style: Some(FontStyle::Normal),
            font_variant: Some(FontVariant::Normal),
            from_standard14_well_known: true,
            name_derived: Default::default(),
        }
    );
    assert!(
        document
            .features
            .borrow()
            .contains(PdfFeature::FontCharacteristics),
        "成功推导应登记 1.4 的字体特征"
    );
    assert!(
        standard14_well_known("Nonexistent").is_none(),
        "非标准 14 名字不命中众所周知表"
    );
}

#[test]
fn standard_14_well_known_serif_italic_and_weight_variants() {
    let times = derive(&font("Type1", Some("Times-BoldItalic"), None));
    assert_eq!(times.serifed, Some(true));
    assert_eq!(times.italic, Some(true));
    assert_eq!(times.font_weight, Some(700.0));
    assert_eq!(times.font_family.as_deref(), Some("Times"));
    assert_eq!(times.generic_font_family, Some(GenericFontFamily::Serif));

    let courier = derive(&font("Type1", Some("Courier"), None));
    assert_eq!(courier.proportional, Some(false));
    assert_eq!(
        courier.generic_font_family,
        Some(GenericFontFamily::Monospace)
    );
    assert_eq!(courier.font_style, Some(FontStyle::Normal));

    let courier_oblique = derive(&font("Type1", Some("Courier-Oblique"), None));
    assert_eq!(courier_oblique.italic, Some(true));
    assert_eq!(courier_oblique.font_style, Some(FontStyle::Italic));

    let dingbats = derive(&font("Type1", Some("ZapfDingbats"), None));
    assert_eq!(dingbats.smallcap, Some(false));
    assert_eq!(dingbats.font_family.as_deref(), Some("ZapfDingbats"));
}

#[test]
fn subset_tagged_standard_14_font_still_uses_well_known_values() {
    // 子集标签 `ABCDEF+` 剥除后仍按标准 14 众所周知值处理（9.6.2.2 名字）。
    let properties = derive(&font("Type1", Some("ABCDEF+Times-Bold"), None));
    assert_eq!(properties.serifed, Some(true));
    assert_eq!(properties.font_weight, Some(700.0));
    assert_eq!(properties.font_family.as_deref(), Some("Times"));
    assert!(properties.from_standard14_well_known);
    assert!(properties.name_derived.is_empty());
}

#[test]
fn overridden_standard_14_descriptor_wins_over_well_known_table() {
    // 9.6.2 说明标准 14 的描述符「specifying them enables a standard font
    // to be overridden」：此时按描述符标志推导，缺失的族名/权重/拉伸走
    // 名字回退（记录 name_derived，属库策略）。
    let properties = derive(&font(
        "Type1",
        Some("Helvetica"),
        Some(descriptor(Some(2), vec![])),
    ));
    assert_eq!(
        properties.serifed,
        Some(true),
        "描述符 Serif 位压过众所周知值"
    );
    assert!(!properties.from_standard14_well_known);
    assert_eq!(properties.font_family.as_deref(), Some("Helvetica"));
    assert_eq!(properties.font_weight, Some(400.0));
    assert_eq!(
        properties.name_derived.iter().collect::<Vec<_>>(),
        vec![
            &NameDerivedAttribute::FontFamily,
            &NameDerivedAttribute::FontWeight,
            &NameDerivedAttribute::FontStretch,
        ]
    );
}

#[test]
fn selector_attributes_derived_from_font_name_when_absent_from_descriptor() {
    // Table 332 前言：FontFamily/FontWeight/FontStretch 不在描述符中时
    // "derived from the font name in a manner of the conforming reader's
    // choosing"——本库的名字扫描，逐项记入 name_derived。
    let properties = derive(&font(
        "Type1",
        Some("MySans-BoldItalic"),
        Some(descriptor(Some(2), vec![])),
    ));
    assert_eq!(properties.serifed, Some(true));
    assert_eq!(
        properties.italic,
        Some(false),
        "Table 331 只看描述符标志，不采信名字里的 Italic"
    );
    assert_eq!(properties.font_family.as_deref(), Some("MySans"));
    assert_eq!(properties.font_weight, Some(700.0));
    assert_eq!(properties.font_stretch.as_deref(), Some("Normal"));
    assert_eq!(properties.name_derived.len(), 3);
}

#[test]
fn descriptor_selector_entries_are_not_marked_name_derived() {
    let properties = derive(&font(
        "Type1",
        Some("Zeta-Bold"),
        Some(descriptor(
            Some(2),
            vec![
                ("FontFamily", Value::String("Zeta".to_owned())),
                ("FontWeight", Value::Integer(300)),
                ("FontStretch", name("Condensed")),
            ],
        )),
    ));
    assert_eq!(properties.font_family.as_deref(), Some("Zeta"));
    assert_eq!(properties.font_weight, Some(300.0));
    assert_eq!(properties.font_stretch.as_deref(), Some("Condensed"));
    assert!(properties.name_derived.is_empty());
    assert!(!properties.from_standard14_well_known);
}

#[test]
fn truetype_without_descriptor_leaves_flag_derivation_absent() {
    // 9.6.4：TrueType 可省略描述符（默认值在字体程序里，本库不解析程序）。
    // 标志位规则项一律 None——不臆造规范留给读者的默认；三个选择器条目
    // 走名字回退并标记。
    let properties = derive(&font("TrueType", Some("FooSans"), None));
    assert_eq!(properties.serifed, None);
    assert_eq!(properties.proportional, None);
    assert_eq!(properties.italic, None);
    assert_eq!(properties.smallcap, None);
    assert_eq!(properties.generic_font_family, None);
    assert_eq!(properties.font_style, None);
    assert_eq!(properties.font_variant, None);
    assert_eq!(properties.font_family.as_deref(), Some("FooSans"));
    assert!(properties.name_derived.len() == 3);
    assert!(!properties.from_standard14_well_known);
}

#[test]
fn type3_font_without_descriptor_derives_no_flag_characteristics() {
    // 9.6.5：Type 3 字体字典没有 /BaseFont；规范也未给 Type 3 特征来源，
    // 全部条目落 None，不报错也不猜测。
    let properties = derive(&font("Type3", None, None));
    assert_eq!(properties.serifed, None);
    assert_eq!(properties.generic_font_family, None);
    assert_eq!(properties.font_family, None);
    assert!(properties.name_derived.is_empty());
}

#[test]
fn non_standard_type1_without_descriptor_returns_typed_error() {
    // Table 111：标准 14 之外 /FontDescriptor 为 Required——畸形输入给
    // 类型化错误而不是静默的 None（原则 6）。
    let document = Document::new();
    let error = document
        .font_character_properties(&font("Type1", Some("FooBar"), None))
        .expect_err("非标准 Type 1 缺描述符必须报错");
    assert!(
        matches!(error, CharacterPropertiesError::Invalid(_)),
        "{error}"
    );
    assert!(error.to_string().contains("/FontDescriptor"), "{error}");
}

#[test]
fn unknown_font_subtype_and_malformed_entries_return_typed_errors() {
    let document = Document::new();
    let cases = vec![
        // 未知子类型。
        font("Type99", Some("Whatever"), None),
        // Type 1 缺 /BaseFont。
        font("Type1", None, None),
        // /BaseFont 不是名字。
        font("Type1", None, Some(descriptor(Some(2), vec![]))),
    ];
    for case in cases {
        assert!(
            document.font_character_properties(&case).is_err(),
            "畸形字体应返回类型化错误: {case:?}"
        );
    }
    // /BaseFont 名字为假的字典形态。
    let bad_base = Value::Dict(
        Dict::new()
            .insert("Subtype", name("Type1"))
            .insert("BaseFont", Value::Integer(42)),
    );
    let error = document
        .font_character_properties(&bad_base)
        .expect_err("/BaseFont 非名字应报错");
    assert!(matches!(error, CharacterPropertiesError::Invalid(_)));
}

#[test]
fn malformed_descriptor_entries_return_typed_errors() {
    let document = Document::new();
    let cases = vec![
        // /Flags 非整数。
        descriptor(None, vec![("Flags", name("bold"))]),
        // /Flags 负数（Table 123 是无符号 32 位）。
        descriptor(None, vec![("Flags", Value::Integer(-1))]),
        // /FontWeight 非数字。
        descriptor(Some(2), vec![("FontWeight", name("Bold"))]),
        // /FontFamily 非文本串。
        descriptor(Some(2), vec![("FontFamily", Value::Integer(3))]),
    ];
    for descriptor_value in cases {
        let error = document
            .font_character_properties(&font("Type1", Some("Custom"), Some(descriptor_value)))
            .expect_err("描述符条目形态错误应报错");
        assert!(
            matches!(error, CharacterPropertiesError::Invalid(_)),
            "{error}"
        );
    }
    // 描述符缺 /Type FontDescriptor。
    let error = document
        .font_character_properties(&font(
            "Type1",
            Some("Custom"),
            Some(Value::Dict(Dict::new().insert("Flags", Value::Integer(2)))),
        ))
        .expect_err("描述符 /Type 缺失应报错");
    assert!(matches!(error, CharacterPropertiesError::Invalid(_)));
}

#[test]
fn indirect_font_and_dangling_reference_return_typed_results() {
    let document = Document::new();
    document.set_object(Object::new(FONT, 0, font("Type1", Some("Helvetica"), None)));
    let properties = document
        .font_character_properties(&Value::Ref(FONT, 0))
        .expect("间接引用应解引用");
    assert_eq!(properties.font_family.as_deref(), Some("Helvetica"));
    let error = document
        .font_character_properties(&Value::Ref(99, 0))
        .expect_err("悬空引用应报错");
    assert!(
        matches!(error, CharacterPropertiesError::Invalid(_)),
        "{error}"
    );
}

#[test]
fn page_collection_returns_only_fonts_selected_by_text_state() {
    // 14.8.2.4.3（shall）：特征只作用于显示串、来自「字符显示时刻文本状态
    // 的字体」。只收集 Tf 选中的资源；/Resources 里登记但从未被 Tf 选中的
    // 字体（此处甚至是畸形字典）不参与推导。
    let resources = Dict::new().insert(
        "Font",
        Value::Dict(
            Dict::new()
                .insert("F1", Value::Ref(FONT, 0))
                .insert("F2", Value::Integer(7)),
        ),
    );
    let document = page_document(resources, b"BT /F1 12 Tf 72 700 Td (Hi) Tj ET");
    document.set_object(Object::new(
        FONT,
        0,
        font("Type1", Some("Times-Roman"), None),
    ));
    let collected = document
        .page_font_character_properties(0)
        .expect("页面收集应成功");
    assert_eq!(collected.keys().collect::<Vec<_>>(), vec!["F1"]);
    let times = &collected["F1"];
    assert_eq!(times.serifed, Some(true));
    assert_eq!(times.font_family.as_deref(), Some("Times"));
    assert!(times.from_standard14_well_known);
}

#[test]
fn page_collection_of_unresolved_text_state_font_errors() {
    // Tf 引用不在 /Resources /Font 的名字 → 类型化错误而不是 panic。
    let resources = Dict::new().insert(
        "Font",
        Value::Dict(Dict::new().insert("F1", Value::Ref(FONT, 0))),
    );
    let document = page_document(resources, b"BT /F9 12 Tf (Hi) Tj ET");
    document.set_object(Object::new(FONT, 0, font("Type1", Some("Helvetica"), None)));
    let error = document
        .page_font_character_properties(0)
        .expect_err("缺失字体资源应报错");
    assert_eq!(
        error,
        CharacterPropertiesError::MissingFontResource("F9".to_owned())
    );
}

#[test]
fn named_page_font_property_resolves_resource_and_errors_on_absence() {
    let resources = Dict::new().insert(
        "Font",
        Value::Dict(Dict::new().insert("F1", Value::Ref(FONT, 0)).insert(
            "F3",
            font(
                "Type1",
                Some("Custom"),
                Some(descriptor(Some(1 | 64), vec![])),
            ),
        )),
    );
    let document = page_document(resources, b"BT /F1 12 Tf (Hi) Tj ET");
    document.set_object(Object::new(
        FONT,
        0,
        descriptor(
            Some(2),
            vec![("FontFamily", Value::String("Courier".to_owned()))],
        ),
    ));
    // 按资源名查询：即使字体没被 Tf 选中也能推导（14.8.2.4.3 之外提供
    // 发现入口，供调用方预取）。
    let properties = document
        .page_font_character_property(0, "F3")
        .expect("按名查询应成功");
    assert_eq!(properties.proportional, Some(false));
    assert_eq!(properties.italic, Some(true));
    assert_eq!(properties.font_family.as_deref(), Some("Custom"));
    let error = document
        .page_font_character_property(0, "F404")
        .expect_err("资源名不存在应报错");
    assert_eq!(
        error,
        CharacterPropertiesError::MissingFontResource("F404".to_owned())
    );
}

#[test]
fn type0_font_derives_from_first_descendant_font_descriptor() {
    // 9.7.4.1：复合字体的描述符挂在 CIDFont 后代上。
    let type0 = Value::Dict(
        Dict::new()
            .insert("Type", name("Font"))
            .insert("Subtype", name("Type0"))
            .insert("BaseFont", name("CJK-Pro"))
            .insert("Encoding", name("Identity-H"))
            .insert(
                "DescendantFonts",
                Value::Array(Array(vec![Value::Ref(FONT, 0)])),
            ),
    );
    let document = Document::new();
    document.set_object(Object::new(
        FONT,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Font"))
                .insert("Subtype", name("CIDFontType2"))
                .insert("FontDescriptor", Value::Ref(DESCRIPTOR, 0)),
        ),
    ));
    document.set_object(Object::new(
        DESCRIPTOR,
        0,
        descriptor(
            Some(2),
            vec![
                ("FontFamily", Value::String("CJK".to_owned())),
                ("FontWeight", Value::Integer(500)),
                ("FontStretch", name("SemiExpanded")),
            ],
        ),
    ));
    let properties = document
        .font_character_properties(&type0)
        .expect("Type 0 应经后代描述符推导");
    assert_eq!(properties.serifed, Some(true));
    assert_eq!(properties.font_family.as_deref(), Some("CJK"));
    assert_eq!(properties.font_weight, Some(500.0));
    assert_eq!(properties.font_stretch.as_deref(), Some("SemiExpanded"));
    assert!(properties.name_derived.is_empty());
}

#[test]
fn type0_font_without_descendant_fonts_errors_typedly() {
    let type0 = Value::Dict(
        Dict::new()
            .insert("Subtype", name("Type0"))
            .insert("BaseFont", name("CJK-Pro"))
            .insert("Encoding", name("Identity-H")),
    );
    let error = Document::new()
        .font_character_properties(&type0)
        .expect_err("Type 0 缺 /DescendantFonts 应报错");
    assert!(
        matches!(error, CharacterPropertiesError::Invalid(_)),
        "{error}"
    );
}
