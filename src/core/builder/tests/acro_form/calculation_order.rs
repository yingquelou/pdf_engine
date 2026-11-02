//! 计算顺序 `/CO`（12.7.2 Table 221）：显式声明按声明序写出、缺省按字段树
//! 前序只列带 `/C` 计算动作的字段，以及未知/遗漏/重复条目在构建期拒绝且
//! 零字节输出。

use super::super::*;
use super::support::acro_dict;
use crate::core::writer::WriteError;

/// 构造单页文档并按顺序加入字段；期望校验失败且不写出任何字节。
fn form_with_fields_error(form: AcroForm, fields: Vec<FormField>) -> WriteError {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let mut form = form;
    for field in fields {
        form.add_field(page, field);
    }
    builder.set_acro_form(form);
    let mut out = Vec::new();
    let error = builder
        .build(&mut out)
        .expect_err("invalid form must be rejected");
    assert!(out.is_empty(), "failed build must not write bytes");
    error
}

fn calculate_field(name: &str, rect: [f64; 4]) -> FormField {
    FormField::text(name, rect).with_action(
        FieldActionEvent::Calculate,
        Action::JavaScript("recalc()".into()),
    )
}

#[test]
fn calculation_order_explicit_names_are_written_in_declared_order() {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new().with_calculation_order(vec!["tax".into(), "total".into()]);
    form.add_field(page, calculate_field("total", [72.0, 700.0, 200.0, 720.0]));
    form.add_field(page, calculate_field("tax", [72.0, 670.0, 200.0, 690.0]));
    builder.set_acro_form(form);
    let doc = builder.assemble_document().expect("assemble");

    let co = acro_dict(&doc)
        .get("CO")
        .and_then(|value| value.as_array())
        .expect("/CO")
        .clone();
    let nums: Vec<i64> =
        co.0.iter()
            .map(|value| value.as_ref().expect("/CO entries are indirect").0)
            .collect();
    let parsed = doc.acro_form().expect("read").expect("form present");
    assert_eq!(parsed.calculation_order, ["tax", "total"]);
    let total_num = parsed.fields[0].reference.expect("total reference").0;
    let tax_num = parsed.fields[1].reference.expect("tax reference").0;
    assert_eq!(
        nums,
        vec![tax_num, total_num],
        "explicit /CO order wins over field tree order"
    );
}

#[test]
fn calculation_order_defaults_to_tree_preorder_for_calculate_fields() {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(page, FormField::text("plain", [72.0, 730.0, 200.0, 750.0]));
    form.add_field(page, calculate_field("total", [72.0, 700.0, 200.0, 720.0]));
    form.add_field(page, calculate_field("tax", [72.0, 670.0, 200.0, 690.0]));
    builder.set_acro_form(form);
    let doc = builder.assemble_document().expect("assemble");

    let co = acro_dict(&doc)
        .get("CO")
        .and_then(|value| value.as_array())
        .expect("/CO")
        .clone();
    assert_eq!(co.0.len(), 2, "only fields with /C actions are listed");
    let parsed = doc.acro_form().expect("read").expect("form present");
    assert_eq!(parsed.calculation_order, ["total", "tax"]);
    let plain = &parsed.fields[0];
    assert!(plain.actions.is_none(), "no /AA for a plain field");
}

#[test]
fn calculation_order_rejects_unknown_missing_and_duplicate_entries() {
    let rect = [72.0, 700.0, 200.0, 720.0];

    let no_calculate = FormField::text("amount", rect);
    let error = form_with_fields_error(
        AcroForm::new().with_calculation_order(vec!["amount".into()]),
        vec![no_calculate],
    );
    assert!(
        matches!(&error, WriteError::InvalidObject(message) if message.contains("is not a field with a /C calculation action")),
        "non-calculate entry: {error:?}"
    );

    let missing = form_with_fields_error(
        AcroForm::new().with_calculation_order(vec!["total".into()]),
        vec![
            calculate_field("total", rect),
            calculate_field("tax", [72.0, 670.0, 200.0, 690.0]),
        ],
    );
    assert!(
        matches!(&missing, WriteError::InvalidObject(message) if message.contains("must list every field")),
        "missing /CO entry: {missing:?}"
    );

    let duplicate = form_with_fields_error(
        AcroForm::new().with_calculation_order(vec!["total".into(), "total".into()]),
        vec![calculate_field("total", rect)],
    );
    assert!(
        matches!(&duplicate, WriteError::InvalidObject(message) if message.contains("lists field total twice")),
        "duplicate /CO entry: {duplicate:?}"
    );

    let unknown = form_with_fields_error(
        AcroForm::new().with_calculation_order(vec!["ghost".into()]),
        vec![calculate_field("total", rect)],
    );
    assert!(
        matches!(&unknown, WriteError::InvalidObject(message) if message.contains("ghost")),
        "unknown /CO entry: {unknown:?}"
    );
}
