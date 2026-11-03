//! 字段 `/TU` 替代名读取测试（Table 220、14.9.3）。

use super::*;

/// Table 220 `/TU`：literal 字符串按 PDFDocEncoding 解码为类型化替代名。
#[test]
fn field_alternate_name_decodes_pdfdocencoding_literal_string() {
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new()
                .insert("T", text("name"))
                .insert("FT", name("Tx"))
                .insert("TU", Value::String("Full name".into())),
        )]),
    )));
    let form = doc.acro_form().expect("parse").expect("present");
    assert_eq!(
        form.fields[0].alternate_name.as_deref(),
        Some("Full name"),
        "/TU literal string decodes via PDFDocEncoding"
    );
}

/// Table 220 `/TU`：带 `FE FF` BOM 的十六进制串按 UTF-16BE 解码。
#[test]
fn field_alternate_name_decodes_utf16be_hex_string() {
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new()
                .insert("T", text("name"))
                .insert("FT", name("Tx"))
                .insert("TU", text("姓名 Signatur✓")),
        )]),
    )));
    let form = doc.acro_form().expect("parse").expect("present");
    assert_eq!(
        form.fields[0].alternate_name.as_deref(),
        Some("姓名 Signatur✓"),
        "/TU with non-PDFDocEncoding characters round-trips via UTF-16BE"
    );
}

/// Table 220 `/TU` 未标注 inheritable：缺省必须是 `None`（不得伪造空串），
/// 祖先声明的 `/TU` 不下传给子节点。
#[test]
fn field_alternate_name_defaults_to_none_and_is_not_inherited() {
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new()
                .insert("T", text("person"))
                .insert("TU", text("Personal data"))
                .insert("Kids", arr(vec![Value::Dict(
                    Dict::new().insert("T", text("name")).insert("FT", name("Tx")),
                )])),
        )]),
    )));
    let form = doc.acro_form().expect("parse").expect("present");
    assert_eq!(
        form.fields[0].alternate_name.as_deref(),
        Some("Personal data")
    );
    assert_eq!(
        form.fields[0].kids[0].alternate_name, None,
        "/TU is not an inheritable entry (Table 220)"
    );

    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new().insert("T", text("plain")).insert("FT", name("Tx")),
        )]),
    )));
    let form = doc.acro_form().expect("parse").expect("present");
    assert_eq!(
        form.fields[0].alternate_name, None,
        "absent /TU must not materialize as an empty string"
    );
}

/// `/TU` 存在但非文本串：跟随本模块 `/T` 先例（`parse_partial_name`），
/// 报类型化 `Malformed` 而非静默降级。
#[test]
fn malformed_alternate_name_is_typed_error() {
    let error = form_error(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new().insert("T", text("name")).insert("TU", Value::Integer(3)),
        )]),
    )));
    assert!(
        matches!(&error, FormError::Malformed(message) if message.contains("/TU")),
        "non-text /TU must be a typed malformed error, got {error:?}"
    );
}
