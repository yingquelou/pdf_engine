//! FDF 读端 `/JavaScript` 单元测试（Table 243 续表 raw L24933–L24934 →
//! Table 245 raw L24951–L24981）：字典形态的类型化解码、文本串/文本流
//! 两形态的分野（流与间接引用不确证、原值不丢）、非字典形态宽容保留
//! 与缺省不物化。

use super::*;
use crate::core::reader::fdf::FdfJavaScriptScript;
use crate::core::types::StreamData;

/// 用单个 `/JavaScript` 值装配最小 FDF 并读取。
fn fdf_with_javascript(javascript: Value) -> FdfDocument {
    let document = doc_from_objects(vec![
        catalog_obj(Value::Ref(2, 0)),
        fdf_dict_obj(Dict::new().insert("JavaScript", javascript), 2),
    ]);
    document
        .fdf()
        .expect("FDF 读端应成功")
        .expect("该文件是 FDF")
}

/// 文本流形态（Table 245 的第二个可选形态）。
fn script_stream() -> Value {
    Value::Stream(
        Dict::new().insert("Length", Value::Integer(5)),
        StreamData::Owned(b"f();".to_vec()),
    )
}

#[test]
fn javascript_dict_text_forms_decode_to_typed_scripts() {
    // Table 245 raw L24953–L24981：`/Before`、`/After`、
    // `/AfterPermsReady` 与 `/Doc` 的文本串形态都就地确证（字面串与
    // 十六进制串走同一条 `text_value` 解码路径）。
    let scripts = Dict::new()
        .insert("Before", Value::String("before f()".to_string()))
        .insert("After", Value::HexString("FEFF0041".to_string()))
        .insert("AfterPermsReady", Value::String("perms".to_string()))
        .insert(
            "Doc",
            field_array(vec![
                name_entry("init"),
                Value::String("var a = 1;".to_string()),
                name_entry("cleanup"),
                Value::HexString("FEFF005A".to_string()),
            ]),
        );
    let fdf = fdf_with_javascript(Value::Dict(scripts.clone()));
    let typed = fdf.fdf.javascript_scripts.expect("字典形态应被类型化");
    assert_eq!(typed.before.as_deref(), Some("before f()"));
    assert_eq!(typed.after.as_deref(), Some("A"));
    assert_eq!(typed.after_perms_ready.as_deref(), Some("perms"));
    assert_eq!(
        typed.doc_scripts,
        vec![
            FdfJavaScriptScript {
                name: "init".to_string(),
                script: "var a = 1;".to_string(),
            },
            FdfJavaScriptScript {
                name: "cleanup".to_string(),
                script: "Z".to_string(),
            },
        ]
    );
    assert_eq!(typed.doc_script("init"), Some("var a = 1;"));
    assert_eq!(typed.doc_script("absent"), None);
    // 既有 `javascript` 原值字段语义不变：仍然保留原值。
    assert_eq!(fdf.fdf.javascript, Some(Value::Dict(scripts.clone())));
    // 类型化视图的原始字典与 FDF 字典里的 `/JavaScript` 同源。
    assert_eq!(typed.dict, scripts);
}

#[test]
fn javascript_stream_and_reference_forms_stay_untyped_and_keep_raw() {
    // 规范给的是「text string **or** text stream」两形态：直接流与间接
    // 引用都无法就地确证 → 类型化保持 None，原值一条不丢（读端宽容）。
    let stream = script_stream();
    let scripts = Dict::new()
        .insert("Before", stream.clone())
        .insert("After", Value::Ref(4, 0))
        .insert(
            "Doc",
            field_array(vec![
                name_entry("full"),
                stream.clone(),
                name_entry("tail"),
                Value::String("t".to_string()),
            ]),
        );
    let fdf = fdf_with_javascript(Value::Dict(scripts));
    let typed = fdf.fdf.javascript_scripts.expect("字典形态仍应被类型化");
    assert_eq!(typed.before, None, "直接流形态保持未类型化");
    assert_eq!(typed.after, None, "间接引用形态保持未类型化");
    assert_eq!(
        typed.doc_scripts,
        vec![FdfJavaScriptScript {
            name: "tail".to_string(),
            script: "t".to_string(),
        }],
        "只有两端都可确证的对进入类型化列表"
    );
    assert_eq!(typed.dict.get("Before"), Some(&stream));
    assert_eq!(typed.dict.get("After"), Some(&Value::Ref(4, 0)));
    assert!(
        matches!(fdf.fdf.javascript, Some(Value::Dict(_))),
        "原值不丢弃：{:?}",
        fdf.fdf.javascript
    );
}

#[test]
fn javascript_odd_doc_array_is_lenient_on_read() {
    // Table 245 raw L24973「The array shall contain an even number of
    // elements」：读端宽容，完整前缀对可类型化，悬挂尾项保留原值
    // （写端在装配前拒绝，见 builder 侧测试）。
    let scripts = Dict::new().insert(
        "Doc",
        field_array(vec![
            name_entry("a"),
            Value::String("x".to_string()),
            name_entry("dangling"),
        ]),
    );
    let fdf = fdf_with_javascript(Value::Dict(scripts));
    let typed = fdf.fdf.javascript_scripts.expect("字典仍被类型化");
    assert_eq!(typed.doc_scripts.len(), 1);
    assert_eq!(typed.doc_scripts[0].name, "a");
    assert_eq!(typed.doc_scripts[0].script, "x");
}

#[test]
fn javascript_non_dict_form_is_tolerated_and_kept_raw() {
    // Table 243 声明类型是 dictionary；非字典形态（如直接写成串）读端
    // 不报错、类型化为 None、原值保留（写端装配前拒绝）。
    let fdf = fdf_with_javascript(Value::String("app.init();".to_string()));
    assert_eq!(fdf.fdf.javascript_scripts, None);
    assert_eq!(
        fdf.fdf.javascript,
        Some(Value::String("app.init();".to_string()))
    );
}

#[test]
fn javascript_absent_leaves_typed_and_raw_none() {
    // 缺省条目不物化：`/JavaScript` 缺失时类型化与原值都是 None。
    let document = doc_from_objects(vec![
        catalog_obj(Value::Ref(2, 0)),
        fdf_dict_obj(Dict::new(), 2),
    ]);
    let fdf = document.fdf().expect("ok").expect("FDF");
    assert_eq!(fdf.fdf.javascript, None);
    assert_eq!(fdf.fdf.javascript_scripts, None);
}
