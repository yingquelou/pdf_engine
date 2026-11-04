//! 外部文件流读端测试（ISO 32000-1 7.3.8 / 7.4.1 + Table 5，文件规格
//! 见 7.11.2–7.11.3 Table 44）。

use crate::core::reader::file_stream::{
    ExternalFileStream, ExternalFileStreamError, ExternalFileStreamProblem,
};
use crate::core::types::{Array, Dict, Document, Object, PdfVersion, StreamData, Value};

fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

fn string(value: &str) -> Value {
    Value::String(value.to_owned())
}

/// 只带给定条目的空正文流对象（外部文件流的正文按 Table 5 一律忽略）。
fn stream_dict(entries: &[(&str, Value)]) -> Dict {
    let mut dict = Dict::new();
    for (key, value) in entries {
        dict = dict.insert((*key).to_string(), value.clone());
    }
    dict
}

fn document_with(dict: Dict) -> Document {
    let document = Document::new();
    document.set_object(Object::new(
        1,
        0,
        Value::Stream(dict, StreamData::Owned(Vec::new())),
    ));
    document
}

/// Table 44 字典形态的最小文件规格。
fn filespec_dict(entries: &[(&str, Value)]) -> Value {
    Value::Dict(stream_dict(entries))
}

fn stream_value(doc: &Document) -> Value {
    doc.get_object(1).unwrap().unwrap().value
}

/// 带内联字节的流对象（`/F` 场景下这些字节按 Table 5 一律忽略）。
fn document_with_inline(dict: Dict, bytes: &[u8]) -> Document {
    let document = Document::new();
    document.set_object(Object::new(
        1,
        0,
        Value::Stream(dict, StreamData::Owned(bytes.to_vec())),
    ));
    document
}

/// 解析成功场景的公共流字典：`/F` 字典形态 + `/FFilter` 单名。
fn valid_stream_dict() -> Dict {
    stream_dict(&[
        ("F", filespec_dict(&[("F", string("data.bin"))])),
        ("FFilter", name("FlateDecode")),
        ("Length", Value::Integer(0)),
    ])
}

#[test]
fn plain_streams_and_non_streams_are_not_applicable() {
    // 普通压缩流：只有 /Filter，没有任何 Table 5 外部文件条目。
    let dict = stream_dict(&[
        ("Filter", name("FlateDecode")),
        ("Length", Value::Integer(4)),
    ]);
    let doc = document_with(dict);
    assert_eq!(doc.external_file_stream(&stream_value(&doc)).unwrap(), None);
    assert!(
        doc.warnings().is_empty(),
        "无关流不得被读红：{:?}",
        doc.warnings()
    );

    // null 值按 7.3.9 等同省略 → 同样不是外部文件流。
    let nullish = document_with(stream_dict(&[("F", Value::Null), ("FFilter", Value::Null)]));
    assert_eq!(
        nullish
            .external_file_stream(&stream_value(&nullish))
            .unwrap(),
        None
    );

    // 非流值（字典、整数）→ Ok(None)，不是错误。
    assert_eq!(doc.external_file_stream(&Value::Integer(7)).unwrap(), None);
    assert_eq!(doc.external_file_stream_object(404).unwrap(), None);
}

#[test]
fn reads_ffilter_name_parms_and_prefers_uf_for_display() {
    let dict = stream_dict(&[
        (
            "F",
            filespec_dict(&[("F", string("spec.pdf")), ("UF", string("sp\u{00e9}c.pdf"))]),
        ),
        (
            "FFilter",
            arr(vec![name("ASCII85Decode"), name("LZWDecode")]),
        ),
        (
            "FDecodeParms",
            arr(vec![
                Value::Null,
                Value::Dict(stream_dict(&[("Columns", Value::Integer(8))])),
            ]),
        ),
        ("Length", Value::Integer(0)),
    ]);
    let doc = document_with(dict);
    let model = doc
        .external_file_stream(&stream_value(&doc))
        .unwrap()
        .expect("应识别为外部文件流");
    assert_eq!(model.filters, vec!["ASCII85Decode", "LZWDecode"]);
    assert_eq!(model.decode_parms.len(), 2);
    assert_eq!(model.parms_for(0), None, "null 元素 → 无参数");
    assert_eq!(
        model
            .parms_for(1)
            .and_then(|params| params.get("Columns"))
            .cloned(),
        Some(Value::Integer(8))
    );
    assert_eq!(model.parms_for(2), None, "越界不 panic");
    // 展示名：/UF 优先回退 /F（7.11.3，与 Filespec::name 同口径）。
    assert_eq!(model.display_name(), "sp\u{00e9}c.pdf");
    assert_eq!(model.file.short_name.as_deref(), Some("spec.pdf"));
    assert_eq!(model.reference, None);
    assert!(model.has_filters());
    assert_eq!(model.dict.get("Length"), Some(&Value::Integer(0)));
}

#[test]
fn string_form_filespec_and_missing_parms_default_to_aligned_none() {
    let doc = document_with(valid_stream_dict());
    let model = doc
        .external_file_stream(&stream_value(&doc))
        .unwrap()
        .expect("/F 字符串形态在 Table 5 之外未建模，缺 /FDecodeParms 时对齐为 None");
    // 上面用的是字典形态；这里改验字符串形态与缺省参数量。
    assert_eq!(model.display_name(), "data.bin");

    let string_form = document_with(stream_dict(&[
        ("F", string("raw.dat")),
        ("FFilter", name("RunLengthDecode")),
    ]));
    let model = string_form
        .external_file_stream(&stream_value(&string_form))
        .unwrap()
        .expect("字符串形态文件规格（7.11.2）应支持");
    assert_eq!(model.file.short_name.as_deref(), Some("raw.dat"));
    assert_eq!(model.file.unicode_name, None);
    assert_eq!(model.filters, vec!["RunLengthDecode"]);
    assert_eq!(
        model.decode_parms,
        vec![None],
        "缺 /FDecodeParms 不物化猜测"
    );
}

#[test]
fn ffilter_or_parms_without_f_is_a_typed_problem() {
    // /FFilter 存在但没有任何 /F：外部文件本身不可指认。
    let doc = document_with(stream_dict(&[("FFilter", name("FlateDecode"))]));
    let error = doc
        .external_file_stream(&stream_value(&doc))
        .expect_err("缺 /F 必须报类型化错误而非默认");
    assert!(matches!(error, ExternalFileStreamError::Malformed(_)));
    assert!(
        error.to_string().contains("without /F"),
        "错误应指明缺 /F：{error}"
    );

    // /FDecodeParms 存在但 /FFilter 缺省（Table 5：参数属于 /FFilter 指定的过滤器）。
    let orphan = document_with(stream_dict(&[
        ("F", string("a.bin")),
        (
            "FDecodeParms",
            Value::Dict(stream_dict(&[("Columns", Value::Integer(8))])),
        ),
    ]));
    let error = orphan
        .external_file_stream(&stream_value(&orphan))
        .expect_err("孤儿 /FDecodeParms 必须报错");
    assert!(
        error
            .to_string()
            .contains("/FDecodeParms present without /FFilter"),
        "got {error}"
    );
}

#[test]
fn count_and_shape_mismatches_are_typed_problems() {
    // 两过滤器配单参数字典（Table 5 shall：一过滤器一项）。
    let single = document_with(stream_dict(&[
        ("F", string("a.bin")),
        ("FFilter", arr(vec![name("A"), name("B")])),
        ("FDecodeParms", Value::Dict(stream_dict(&[]))),
    ]));
    let error = single
        .external_file_stream(&stream_value(&single))
        .expect_err("单字典配多过滤器必须报错");
    assert!(
        error.to_string().contains("single dictionary"),
        "got {error}"
    );

    // 数组个数与过滤器个数不一致。
    let short = document_with(stream_dict(&[
        ("F", string("a.bin")),
        ("FFilter", arr(vec![name("A"), name("B")])),
        ("FDecodeParms", arr(vec![Value::Dict(stream_dict(&[]))])),
    ]));
    let error = short
        .external_file_stream(&stream_value(&short))
        .expect_err("个数不匹配必须报错");
    assert!(error.to_string().contains("has 1 entries"), "got {error}");

    // 参数数组元素既非字典也非 null。
    let bad_element = document_with(stream_dict(&[
        ("F", string("a.bin")),
        ("FFilter", name("A")),
        ("FDecodeParms", arr(vec![Value::Integer(3)])),
    ]));
    let error = bad_element
        .external_file_stream(&stream_value(&bad_element))
        .expect_err("非法参数元素必须报错");
    assert!(
        error.to_string().contains("array element is integer"),
        "got {error}"
    );

    // /FFilter 值形态非法（整数）。
    let bad_filter = document_with(stream_dict(&[
        ("F", string("a.bin")),
        ("FFilter", Value::Integer(1)),
    ]));
    let error = bad_filter
        .external_file_stream(&stream_value(&bad_filter))
        .expect_err("非法 /FFilter 形态必须报错");
    assert!(
        error.to_string().contains("/FFilter is integer (Table 5"),
        "got {error}"
    );
}

#[test]
fn zero_name_ffilter_array_is_valid_and_red_nothing() {
    let doc = document_with(stream_dict(&[
        ("F", string("a.bin")),
        ("FFilter", arr(vec![])),
        ("FDecodeParms", arr(vec![])),
    ]));
    let model = doc
        .external_file_stream(&stream_value(&doc))
        .unwrap()
        .expect("Table 5 允许零个过滤器名");
    assert!(model.filters.is_empty());
    assert!(model.decode_parms.is_empty());
    assert!(!model.has_filters());
    assert!(doc.warnings().is_empty(), "got {:?}", doc.warnings());
}

#[test]
fn indirect_reference_forms_use_the_shared_normalizer() {
    // 与 filters/indirect.rs、reader/image 同一形态：/FFilter 的值与
    // /FDecodeParms 的数组元素写成间接引用（7.3.10）。
    let document = Document::new();
    document.set_object(Object::new(1, 0, Value::Name("ASCIIHexDecode".to_owned())));
    document.set_object(Object::new(
        2,
        0,
        Value::Dict(stream_dict(&[("Columns", Value::Integer(8))])),
    ));
    document.set_object(Object::new(
        3,
        0,
        Value::Stream(
            stream_dict(&[
                (
                    "F",
                    filespec_dict(&[("F", string("hex.dat")), ("UF", string("hex.dat"))]),
                ),
                (
                    "FFilter",
                    arr(vec![Value::Ref(1, 0), name("RunLengthDecode")]),
                ),
                ("FDecodeParms", arr(vec![Value::Ref(2, 0), Value::Null])),
            ]),
            StreamData::Owned(Vec::new()),
        ),
    ));
    let value = document.get_object(3).unwrap().unwrap().value;
    let model = document
        .external_file_stream(&value)
        .unwrap()
        .expect("引用形态应与直接对象形态等价");
    assert_eq!(model.reference, None, "直接传入流值时引用为 None");
    let model = document
        .external_file_stream_object(3)
        .unwrap()
        .expect("对象号入口与流值入口等价");
    assert_eq!(model.filters, vec!["ASCIIHexDecode", "RunLengthDecode"]);
    assert_eq!(
        model
            .parms_for(0)
            .and_then(|params| params.get("Columns"))
            .cloned(),
        Some(Value::Integer(8))
    );
    assert_eq!(model.parms_for(1), None);
    assert_eq!(model.reference, Some((3, 0)));
    assert!(
        document.warnings().is_empty(),
        "合法引用形态不应产生诊断：{:?}",
        document.warnings()
    );
}

#[test]
fn dangling_ffilter_reference_is_a_typed_problem_not_a_silent_default() {
    let doc = document_with(stream_dict(&[
        ("F", string("a.bin")),
        ("FFilter", Value::Ref(9, 0)),
    ]));
    let error = doc
        .external_file_stream(&stream_value(&doc))
        .expect_err("引用指向缺失对象必须报错而非按无过滤器降级");
    assert!(
        error.to_string().contains("absent object"),
        "错误应带上规范化器诊断：{error}"
    );
    assert!(
        matches!(error, ExternalFileStreamError::Malformed(_)),
        "got {error:?}"
    );
}

/// 取模型（交叉核对不得影响构造），失败即 panic 便于定位。
fn model_of(doc: &Document) -> ExternalFileStream {
    doc.external_file_stream(&stream_value(doc))
        .expect("结构合法")
        .expect("应识别为外部文件流")
}

/// 只带 `/F`＋`/FFilter`＋给定 `/Length` 的流字典。
fn f_stream_dict(length: Value) -> Dict {
    stream_dict(&[
        ("F", string("a.bin")),
        ("FFilter", name("FlateDecode")),
        ("Length", length),
    ])
}

#[test]
fn clean_external_stream_carries_no_problems() {
    let doc = document_with(valid_stream_dict());
    let model = model_of(&doc);
    assert!(
        model.problems.is_empty(),
        "合法外部文件流不得被报不符：{:?}",
        model.problems
    );
    assert!(!model.has_problems());
}

#[test]
fn pdf_1_1_declaration_gates_the_pdf_1_2_entries_but_keeps_values() {
    let mut doc = document_with(stream_dict(&[
        ("F", string("a.bin")),
        ("FFilter", name("FlateDecode")),
        ("FDecodeParms", Value::Null),
        ("Length", Value::Integer(0)),
    ]));
    doc.source_version = PdfVersion::V1_1;
    let model = model_of(&doc);
    assert_eq!(model.display_name(), "a.bin");
    assert_eq!(model.filters, vec!["FlateDecode"], "门禁不拦读取");
    assert_eq!(
        model.problems,
        vec![ExternalFileStreamProblem::VersionGate {
            entries: vec!["F", "FFilter"],
            declared: PdfVersion::V1_1,
        }],
        "/FDecodeParms 为 null 按 7.3.9 等同省略，不进清单"
    );

    // 恰好 1.2 不报；非外部文件流也不报（Ok(None)）。
    let mut ok = document_with(valid_stream_dict());
    ok.source_version = PdfVersion::V1_2;
    assert!(model_of(&ok).problems.is_empty());
}

#[test]
fn crypt_ffilter_on_external_data_is_a_shall_problem_not_a_failure() {
    let doc = document_with(stream_dict(&[
        ("F", string("a.bin")),
        ("FFilter", arr(vec![name("ASCIIHexDecode"), name("Crypt")])),
        ("Length", Value::Integer(0)),
    ]));
    let model = model_of(&doc);
    assert_eq!(model.filters, vec!["ASCIIHexDecode", "Crypt"]);
    assert_eq!(
        model.problems,
        vec![ExternalFileStreamProblem::EncryptedExternalData {
            filter: "Crypt".to_owned()
        }],
        "数组里的 Crypt 同样命中"
    );
    assert!(!model.problems[0].is_advisory(), "shall 级不该被当提示");

    let plain = document_with(valid_stream_dict());
    assert!(model_of(&plain).problems.is_empty());
}

#[test]
fn inline_bytes_are_reported_ignored_and_length_should_match_them() {
    // 一致：只报「已忽略」（提示级），不报 should 违例。
    let consistent = document_with_inline(f_stream_dict(Value::Integer(5)), b"12345");
    let model = model_of(&consistent);
    assert_eq!(
        model.problems,
        vec![ExternalFileStreamProblem::InlineBytesIgnored { bytes: 5 }]
    );
    assert!(model.problems[0].is_advisory());

    // 少报：提示 + should 两条并存。
    let short = document_with_inline(f_stream_dict(Value::Integer(3)), b"12345");
    assert_eq!(
        model_of(&short).problems,
        vec![
            ExternalFileStreamProblem::InlineBytesIgnored { bytes: 5 },
            ExternalFileStreamProblem::LengthMismatch {
                declared: 3,
                actual: 5
            },
        ]
    );

    // 空内联 + /Length 0：两条都不报（Table 5「usually, there are no bytes」）。
    let empty = document_with_inline(f_stream_dict(Value::Integer(0)), b"");
    assert!(model_of(&empty).problems.is_empty());

    // 缺 /Length：不猜声明值，只报已忽略的字节。
    let no_length = document_with_inline(
        stream_dict(&[("F", string("a.bin")), ("FFilter", name("FlateDecode"))]),
        b"12345",
    );
    assert_eq!(
        model_of(&no_length).problems,
        vec![ExternalFileStreamProblem::InlineBytesIgnored { bytes: 5 }]
    );
}

#[test]
fn indirect_length_is_resolved_for_the_should_check() {
    let document = Document::new();
    document.set_object(Object::new(2, 0, Value::Integer(9)));
    document.set_object(Object::new(
        1,
        0,
        Value::Stream(
            f_stream_dict(Value::Ref(2, 0)),
            StreamData::Owned(b"12345".to_vec()),
        ),
    ));
    let model = model_of(&document);
    assert!(
        model
            .problems
            .contains(&ExternalFileStreamProblem::LengthMismatch {
                declared: 9,
                actual: 5
            }),
        "引用形态的 /Length 应按 7.3.10 解一层：{:?}",
        model.problems
    );
}

#[test]
fn problem_messages_carry_their_spec_anchors() {
    let cases = [
        (
            ExternalFileStreamProblem::VersionGate {
                entries: vec!["F", "FDecodeParms"],
                declared: PdfVersion::V1_0,
            },
            "L1095",
            false,
        ),
        (
            ExternalFileStreamProblem::EncryptedExternalData {
                filter: "Crypt".to_owned(),
            },
            "L2743",
            false,
        ),
        (
            ExternalFileStreamProblem::InlineBytesIgnored { bytes: 7 },
            "L1096",
            true,
        ),
        (
            ExternalFileStreamProblem::LengthMismatch {
                declared: 3,
                actual: 7,
            },
            "L1097",
            true,
        ),
    ];
    for (problem, anchor, advisory) in cases {
        let text = problem.to_string();
        assert!(text.contains(anchor), "{text} 应带 raw 行号 {anchor}");
        assert_eq!(problem.is_advisory(), advisory, "{text}");
        assert!(
            text.contains("/F") || text.contains("Length") || text.contains("Crypt"),
            "{text} 应点明条目"
        );
    }
}

#[test]
fn filespec_without_any_name_is_an_invalid_filespec_problem() {
    // Table 44：/F 在 DOS/Mac/Unix 全缺时必填；只有 /Desc 的字典没有
    // 任何可指认的文件。
    let doc = document_with(stream_dict(&[
        ("F", filespec_dict(&[("Desc", string("no name here"))])),
        ("FFilter", name("FlateDecode")),
    ]));
    let error = doc
        .external_file_stream(&stream_value(&doc))
        .expect_err("无名文件规格必须报错");
    assert!(matches!(error, ExternalFileStreamError::InvalidFilespec(_)));
    assert!(error.to_string().contains("names no file"), "got {error}");

    // /F 值根本不是文件规格（流）。
    let wrong = Document::new();
    wrong.set_object(Object::new(
        2,
        0,
        Value::Stream(
            stream_dict(&[("Length", Value::Integer(0))]),
            StreamData::Owned(Vec::new()),
        ),
    ));
    wrong.set_object(Object::new(
        1,
        0,
        Value::Stream(
            stream_dict(&[("F", Value::Ref(2, 0)), ("FFilter", name("FlateDecode"))]),
            StreamData::Owned(Vec::new()),
        ),
    ));
    let value = wrong.get_object(1).unwrap().unwrap().value;
    let error = wrong
        .external_file_stream(&value)
        .expect_err("/F 指向流不是文件规格");
    assert!(
        error
            .to_string()
            .contains("file specification string or dictionary"),
        "got {error}"
    );
}
