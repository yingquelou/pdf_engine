//! 属性表的**间接引用操作数**形态（`/Span 5 0 R BDC`）：`num gen R` 折叠、
//! 解引用、`/ActualText` 替换与 `/OC` 可见性。
//!
//! 该形态不是 ISO 32000-1 授权的写法：Table 320（`pdftotext -raw
//! docs/PDF32000_2008.pdf -` 输出 L29075–29078）只授权 `BDC` 的属性操作数为
//! 内联字典或 `/Properties` 名称，14.6.2（同输出 L29087–29091）更要求「值中
//! 含间接引用的属性表字典 shall 定义为命名资源并按名称引用」。这里测的是对
//! 生产端实态（本 crate 写端 `ContentStream::begin_marked_content_reference`
//! 也产出该形态）的**互操作容错**，解引用口径与已落地的声明式读端
//! （`crate::core::reader::content::marked` 的 `section_from` `Value::Ref`
//! 分支）保持一致：解不到就降级为「无属性、可见」，绝不报错。

use crate::core::types::{Array, Dict, Document, Object, Value};

use super::support::{
    catalog_with_pages, document_from_objects, oc_properties_document, oc_properties_document_with,
    resources_with_font, winansi_type1_font,
};

/// 只带 `/Type /OCMD`、`/OCGs` 与 `/P` 策略的成员字典（8.11.2.2 Table 100）。
fn ocmd_object(object: i64, groups: &[i64], policy: &str) -> Object {
    let mut refs = Array::new();
    for group in groups {
        refs = refs.push(Value::Ref(*group, 0));
    }
    Object::new(
        object,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("OCMD".into()))
                .insert("OCGs", Value::Array(refs))
                .insert("P", Value::Name(policy.into())),
        ),
    )
}

fn dict_object(object: i64, entries: Vec<(&str, Value)>) -> Object {
    let mut dict = Dict::new();
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    Object::new(object, 0, Value::Dict(dict))
}

/// 对象 1 目录 + 对象 6 属性表（带 `/ActualText (unseen)`）的最小文档。
fn actual_text_document() -> Document {
    document_from_objects(vec![
        Object::new(1, 0, catalog_with_pages(Value::Ref(2, 0))),
        dict_object(6, vec![("ActualText", Value::String("unseen".into()))]),
    ])
}

/// 折叠不能让 `BDC` 的属性操作数停在栈上被丢掉：解引用后 `/ActualText` 与
/// 内联字典、`/Properties` 名称两条通路同样生效。
#[test]
fn reference_property_operand_replaces_shown_text_with_actual_text() {
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf /Span 6 0 R BDC (orig) Tj ET EMC \
                    BT /F1 12 Tf (tail) Tj ET";

    assert_eq!(
        actual_text_document().extract_text(content, &resources),
        "unseentail"
    );
}

/// 引用直接指向默认配置 OFF 的**裸 OCG 字典**（本 crate 写端产出的形态）：
/// 该区段文本不抽取，与 `/Properties` 名称指向 OCG 的既有分支同构。
#[test]
fn reference_to_bare_ocg_hidden_by_default_configuration_hides_section_text() {
    let document = oc_properties_document(&[], &[5]);
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"/Span 5 0 R BDC BT /F1 12 Tf (gone) Tj ET EMC \
                    BT /F1 12 Tf (kept) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "kept");
}

/// 同上但配置为 ON：引用命中的 OCG 可见时文本照常抽取。
#[test]
fn reference_to_bare_ocg_visible_by_default_configuration_keeps_section_text() {
    let document = oc_properties_document(&[5], &[]);
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"/Span 5 0 R BDC BT /F1 12 Tf (shown) Tj ET EMC";

    assert_eq!(document.extract_text(content, &resources), "shown");
}

/// 引用指向属性表字典 `<< /OC 5 0 R >>`（`/OC` 是 OFF 的 OCG 引用）：隐藏。
#[test]
fn reference_property_list_with_hidden_oc_group_hides_section_text() {
    let document = oc_properties_document_with(
        &[],
        &[5],
        vec![dict_object(6, vec![("OC", Value::Ref(5, 0))])],
    );
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"/Span 6 0 R BDC BT /F1 12 Tf (gone) Tj ET EMC \
                    BT /F1 12 Tf (kept) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "kept");
}

/// 引用指向属性表字典、其 `/OC` 是间接 OCMD（`/P /AllOff`）：走既有 OCMD
/// 策略通路，隐藏。
#[test]
fn reference_property_list_with_indirect_ocmd_policy_hides_section_text() {
    let document = oc_properties_document_with(
        &[5],
        &[],
        vec![
            ocmd_object(7, &[5], "AllOff"),
            dict_object(6, vec![("OC", Value::Ref(7, 0))]),
        ],
    );
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"/Span 6 0 R BDC BT /F1 12 Tf (gone) Tj ET EMC \
                    BT /F1 12 Tf (kept) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "kept");
}

/// 引用直接指向 OCMD 字典（不是属性表）：按成员策略判可见性。
#[test]
fn reference_to_indirect_ocmd_policy_hides_section_text() {
    let document = oc_properties_document_with(&[5], &[], vec![ocmd_object(7, &[5], "AllOff")]);
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"/Span 7 0 R BDC BT /F1 12 Tf (gone) Tj ET EMC \
                    BT /F1 12 Tf (kept) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "kept");
}

/// 悬空引用（对象不存在）、目标不是字典、以及 `/OC` 指到不存在对象：都只
/// 降级为「无属性、可见」，文本照出且不报错。
#[test]
fn unresolvable_reference_operands_degrade_to_visible_without_properties() {
    let document = document_from_objects(vec![
        Object::new(1, 0, catalog_with_pages(Value::Ref(2, 0))),
        Object::new(8, 0, Value::Integer(42)),
        dict_object(9, vec![("OC", Value::Ref(99, 0))]),
    ]);
    let resources = resources_with_font("F1", winansi_type1_font());
    let dangling = b"/Span 99 0 R BDC BT /F1 12 Tf (first) Tj ET EMC";
    let not_a_dict = b"/Span 8 0 R BDC BT /F1 12 Tf (second) Tj ET EMC";
    let oc_unresolvable = b"/Span 9 0 R BDC BT /F1 12 Tf (third) Tj ET EMC";

    assert_eq!(
        document.extract_text(dangling, &resources),
        "first",
        "对象不存在只丢属性表，不影响区段本身"
    );
    assert_eq!(
        document.extract_text(not_a_dict, &resources),
        "second",
        "目标不是字典同样降级"
    );
    assert_eq!(
        document.extract_text(oc_unresolvable, &resources),
        "third",
        "/OC 解不到对象时不隐藏：与声明式读端一样保持可见"
    );
}

/// 引用里的代号被忽略：按对象号取对象，非零代号照样解到属性表。
#[test]
fn reference_operand_ignores_the_generation_and_resolves_the_object() {
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf /Span 6 5 R BDC (orig) Tj ET EMC";

    assert_eq!(
        actual_text_document().extract_text(content, &resources),
        "unseen",
        "增量更新后引用可能写旧代号，解引用只看对象号"
    );
}

/// 畸形引用形态不得 panic，也不得吞掉同一区段前后的 `Tj` 文本：元数不足、
/// 非整数对象号、负对象号、数与 `R` 之间夹了别的算符，都按「不是引用」处理，
/// 行为与折叠前一致。
#[test]
fn malformed_reference_triplets_never_panic_or_swallow_shown_text() {
    let resources = resources_with_font("F1", winansi_type1_font());
    let cases: Vec<&[u8]> = vec![
        b"BT /F1 12 Tf (a) Tj 5 R (b) Tj ET",
        b"BT /F1 12 Tf (a) Tj 5.5 0 R (b) Tj ET",
        b"BT /F1 12 Tf (a) Tj -3 0 R (b) Tj ET",
        b"BT /F1 12 Tf (a) Tj 5 0 foo R (b) Tj ET",
        b"BT /F1 12 Tf (a) Tj R (b) Tj ET",
    ];
    for content in cases {
        assert_eq!(
            actual_text_document().extract_text(content, &resources),
            "ab",
            "非引用的 R 形态必须原样降级：{:?}",
            String::from_utf8_lossy(content)
        );
    }
}

/// 畸形形态落在 `BDC` 的属性操作数位置时，`R` 与折叠前一样把栈清空，于是
/// 该 `BDC` 根本没有标签也没有属性表：文本照出，不报错。
#[test]
fn malformed_property_operand_before_bdc_falls_back_to_unmarked_extraction() {
    let resources = resources_with_font("F1", winansi_type1_font());
    let cases: Vec<&[u8]> = vec![
        b"BT /F1 12 Tf /Span 5 R BDC (orig) Tj ET EMC BT /F1 12 Tf (tail) Tj ET",
        b"BT /F1 12 Tf /Span 5.5 0 R BDC (orig) Tj ET EMC BT /F1 12 Tf (tail) Tj ET",
        b"BT /F1 12 Tf /Span 6 0 0 R BDC (orig) Tj ET EMC BT /F1 12 Tf (tail) Tj ET",
    ];
    for content in cases {
        assert_eq!(
            actual_text_document().extract_text(content, &resources),
            "origtail",
            "折叠不能把畸形形态读成引用：{:?}",
            String::from_utf8_lossy(content)
        );
    }
}

/// 三个数加 `R`：折叠尾部两数、把多余的数留在栈上，与声明式读端的
/// `fold_reference` 同一口径；随后的 `Tj` 仍取到显示串。
#[test]
fn extra_number_before_reference_triplet_keeps_shown_text() {
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf (a) Tj 6 0 0 R (b) Tj ET";

    assert_eq!(
        actual_text_document().extract_text(content, &resources),
        "ab"
    );
}

/// 显示数组里的 `num gen R` **不**折叠（数组元素形态不在抽取器建模范围内），
/// 该分支与折叠前逐字相同：未折叠的 `R` 照旧清掉待关闭的数组，数组内文本
/// 丢失的既有行为不变，但不得 panic，也不得影响数组之后的 `Tj`。
#[test]
fn reference_triplet_inside_show_array_does_not_break_later_shows() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf [(a) 5 0 R (b)] TJ (c) Tj ET";

    assert_eq!(
        document.extract_text(content, &resources),
        "c",
        "数组内的 R 走的是折叠前的老路径（整个数组被 clear 丢掉），\
         本增量没有改变它，也没有让它更糟"
    );
}

/// 折叠后嵌套帧语义不变：内层 `BMC` 仍继承外层引用属性表推出的 `hidden`
/// （`state.rs` 的 `begin_marked` 父帧传播）。
#[test]
fn nested_frame_inherits_hidden_state_from_reference_property_operand() {
    let document = oc_properties_document(&[], &[5]);
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"/Outer 5 0 R BDC /Inner BMC BT /F1 12 Tf (gone) Tj ET EMC EMC";

    assert_eq!(document.extract_text(content, &resources), "");
}

/// 折叠后嵌套帧语义不变：外层引用属性表的 `/ActualText` 覆盖内层 `BMC`，
/// 整段只发出一次替换。
#[test]
fn nested_frame_uses_actual_text_from_reference_property_operand_once() {
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf /Outer 6 0 R BDC (a) Tj /Nested BMC (b) Tj EMC EMC \
                    (tail) Tj ET";

    assert_eq!(
        actual_text_document().extract_text(content, &resources),
        "unseentail"
    );
}

/// 引用命中的属性表同时带 `/OC`（默认 OFF）与 `/ActualText` 时**隐藏优先**：
/// 整段一个字都不出，替换文本不会把隐藏内容「救回来」——与内联字典、
/// `/Properties` 名称两条既有通路的次序一致（`state.begin_marked` 只看
/// `hidden`，`actual_text` 只在可见时发出）。
#[test]
fn hidden_reference_property_list_drops_text_even_with_actual_text() {
    let document = oc_properties_document_with(
        &[],
        &[5],
        vec![dict_object(
            6,
            vec![
                ("OC", Value::Ref(5, 0)),
                ("ActualText", Value::String("replacement".into())),
            ],
        )],
    );
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"/Span 6 0 R BDC BT /F1 12 Tf (orig) Tj ET EMC \
                    BT /F1 12 Tf (kept) Tj ET";

    assert_eq!(
        document.extract_text(content, &resources),
        "kept",
        "隐藏的引用区段既不输出原文也不输出替换文本"
    );
}

/// `/ReversedChars` 标签配引用属性表时，标签驱动的逐串反序（14.8.2.3.3）
/// 与 `hidden` 继承都不受影响。
#[test]
fn reversed_chars_tag_with_reference_property_operand_still_reverses_shown_string() {
    let document = document_from_objects(vec![
        Object::new(1, 0, catalog_with_pages(Value::Ref(2, 0))),
        dict_object(6, vec![("Lang", Value::Name("en-GB".into()))]),
    ]);
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf /ReversedChars 6 0 R BDC (ab) Tj ET EMC (cd) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "bacd");
}

/// `Artifact` 标签 + 引用属性表：标签的排除语义优先，与属性表能否解引用无关。
#[test]
fn artifact_tag_with_reference_property_operand_stays_excluded() {
    let document = oc_properties_document(&[5], &[]);
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"/Artifact 5 0 R BDC BT /F1 12 Tf (gone) Tj ET EMC \
                    BT /F1 12 Tf (kept) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "kept");
}
