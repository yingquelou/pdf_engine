//! 字段类型、`/Ff` 标志位与 Widget 状态解码测试（Table 221/226/228/230）。

use super::*;

#[test]
fn widget_node_keeps_appearance_state_and_page_reference() {
    let widget_dict = widget(arr(vec![
        Value::Integer(0),
        Value::Integer(0),
        Value::Integer(20),
        Value::Integer(20),
    ]))
    .insert("T", text("ack"))
    .insert("FT", name("Btn"))
    .insert("Ff", Value::Integer(1 << 16))
    .insert(
        "AP",
        Value::Dict(Dict::new().insert("N", Value::Ref(30, 0))),
    )
    .insert("AS", name("On"))
    .insert("P", Value::Ref(3, 0));
    let doc = doc_with_acro_form(Value::Dict(
        Dict::new().insert("Fields", arr(vec![Value::Dict(widget_dict)])),
    ));
    let form = doc.acro_form().expect("parse").expect("present");
    let field = &form.fields[0];
    assert!(field.is_widget);
    assert_eq!(field.field_type, Some(FieldType::Button));
    assert_eq!(field.appearance_state.as_deref(), Some("On"));
    assert_eq!(field.page, Some((3, 0)));
    assert_eq!(field.flags.bits(), 1 << 16);
    assert!(field.flags.pushbutton());
    assert_eq!(
        field
            .appearance
            .as_ref()
            .and_then(Value::as_dict)
            .and_then(|dict| dict.get("N")),
        Some(&Value::Ref(30, 0))
    );
}

#[test]
fn choice_button_and_signature_types() {
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![
            Value::Dict(
                Dict::new()
                    .insert("FT", name("Btn"))
                    .insert("Ff", Value::Integer((1 << 15) | (1 << 16))),
            ),
            Value::Dict(
                Dict::new()
                    .insert("FT", name("Ch"))
                    .insert("Opt", arr(vec![text("A"), text("B")]))
                    .insert("V", name("B"))
                    .insert("Ff", Value::Integer((1 << 17) | (1 << 21))),
            ),
            Value::Dict(Dict::new().insert("FT", name("Sig"))),
        ]),
    )));
    let form = doc.acro_form().expect("parse").expect("present");
    assert_eq!(form.fields[0].field_type, Some(FieldType::Button));
    assert!(form.fields[0].flags.radio());
    assert!(form.fields[0].flags.pushbutton());
    assert_eq!(form.fields[1].field_type, Some(FieldType::Choice));
    assert_eq!(
        form.fields[1].options.as_ref().map(|options| options.len()),
        Some(2)
    );
    assert_eq!(form.fields[1].value, Some(name("B")));
    assert!(form.fields[1].flags.combo());
    assert!(form.fields[1].flags.multi_select());
    assert_eq!(form.fields[2].field_type, Some(FieldType::Signature));
}

#[test]
fn field_flags_decode_every_standard_bit() {
    // Table 221：通用位。
    let common = FieldFlags::from_bits((1 << 0) | (1 << 1) | (1 << 2));
    assert!(common.read_only());
    assert!(common.required());
    assert!(common.no_export());

    // Table 226：15 NoToggleToOff、16 Radio、17 Pushbutton、26 RadiosInUnison。
    let button = FieldFlags::from_bits((1 << 14) | (1 << 15) | (1 << 16) | (1 << 25));
    assert!(button.no_toggle_to_off());
    assert!(button.radio());
    assert!(button.pushbutton());
    assert!(button.radios_in_unison());

    // Table 228：13 Multiline、14 Password、21 FileSelect、23 DoNotSpellCheck、
    // 24 DoNotScroll、25 Comb、26 RichText。
    let text = FieldFlags::from_bits(
        (1 << 12) | (1 << 13) | (1 << 20) | (1 << 22) | (1 << 23) | (1 << 24) | (1 << 25),
    );
    assert!(text.multiline());
    assert!(text.password());
    assert!(text.file_select());
    assert!(text.do_not_spell_check());
    assert!(text.do_not_scroll());
    assert!(text.comb());
    assert!(text.rich_text());

    // Table 230：18 Combo、19 Edit、20 Sort、22 MultiSelect、
    // 23 DoNotSpellCheck、27 CommitOnSelChange。
    let choice = FieldFlags::from_bits(
        (1 << 17) | (1 << 18) | (1 << 19) | (1 << 21) | (1 << 22) | (1 << 26),
    );
    assert!(choice.combo());
    assert!(choice.edit());
    assert!(choice.sort());
    assert!(choice.multi_select());
    assert!(choice.do_not_spell_check());
    assert!(choice.commit_on_sel_change());
}

#[test]
fn field_flags_accessors_are_bit_exact() {
    let single = |index: u32| FieldFlags::from_bits(1 << index);
    // FileSelect(21) 与 MultiSelect(22) 相邻不串扰。
    assert!(single(20).file_select() && !single(20).multi_select());
    assert!(single(21).multi_select() && !single(21).file_select());
    // Comb(25) 与 RichText/RadiosInUnison(26) 相邻不串扰。
    assert!(single(24).comb() && !single(24).rich_text() && !single(24).radios_in_unison());
    assert!(single(25).rich_text() && !single(25).comb());
    assert!(
        single(25).radios_in_unison() && single(25).rich_text(),
        "bit 26 对按钮与文本字段分别命名"
    );
    assert!(single(26).commit_on_sel_change() && !single(26).rich_text());
    assert!(single(18).edit() && !single(18).combo());
    // 未建模位原样保留。
    let unknown = FieldFlags::from_bits(1 << 31);
    assert_eq!(unknown.bits(), 1 << 31);
    assert!(!unknown.read_only() && !unknown.combo());
}

#[test]
fn parsed_text_flags_surface_file_select_scroll_comb_and_rich_text() {
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new().insert("FT", name("Tx")).insert(
                "Ff",
                Value::Integer((1 << 20) | (1 << 23) | (1 << 24) | (1 << 25)),
            ),
        )]),
    )));
    let form = doc.acro_form().expect("parse").expect("present");
    let flags = form.fields[0].flags;
    assert!(flags.file_select());
    assert!(flags.do_not_scroll());
    assert!(flags.comb());
    assert!(flags.rich_text());
    assert!(!flags.multiline());
    assert!(!flags.multi_select());
}
