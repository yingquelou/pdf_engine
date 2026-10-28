//! E6 读端嵌入文件测试（7.11.2–7.11.4）。

use super::*;
use crate::core::types::{Array, Object, StreamData, Trailer, Xref};
use md5::{Digest, Md5};

fn name(value: &str) -> Value {
    Value::Name(value.into())
}

/// 文本串按 UTF-16BE 十六进制写出，验证读端解码路径。
fn text(value: &str) -> Value {
    let bytes = crate::core::types::PdfTextString::from(value).encode();
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<String>();
    Value::HexString(hex)
}

fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

fn rect() -> Value {
    arr(vec![
        Value::Integer(10),
        Value::Integer(20),
        Value::Integer(30),
        Value::Integer(40),
    ])
}

fn stream_object(num: i64, dict: Dict, data: &[u8]) -> Object {
    let dict = dict.insert("Length", Value::Integer(data.len() as i64));
    Object::new(
        num,
        0,
        Value::Stream(dict, StreamData::Owned(data.to_vec())),
    )
}

fn embedded_stream(num: i64, extra: Dict, data: &[u8]) -> Object {
    let dict = extra.insert("Type", name("EmbeddedFile"));
    stream_object(num, dict, data)
}

fn filespec_dict(entries: Vec<(&str, Value)>) -> Value {
    let mut dict = Dict::new().insert("Type", name("Filespec"));
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    Value::Dict(dict)
}

/// `names` 为 `Some` 时写入目录 `/Names` 字典。
fn doc_with_names_dict(names: Option<Dict>) -> Document {
    let mut catalog = Dict::new()
        .insert("Type", name("Catalog"))
        .insert("Pages", Value::Ref(2, 0));
    if let Some(names) = names {
        catalog = catalog.insert("Names", Value::Dict(names));
    }
    let objects = vec![
        Object::new(1, 0, Value::Dict(catalog)),
        Object::new(
            2,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", name("Pages"))
                    .insert("Kids", arr(vec![Value::Ref(3, 0)])),
            ),
        ),
        Object::new(
            3,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", name("Page"))
                    .insert("Parent", Value::Ref(2, 0)),
            ),
        ),
    ];
    let mut trailer = Trailer::new();
    trailer.dict = trailer.dict.clone().insert("Root", Value::Ref(1, 0));
    Document::from_objects(objects, Xref::new(), trailer, Default::default())
}

/// 目录 `/Names /EmbeddedFiles` 指向 `node` 的文档。
fn doc_with_embedded_node(node: Value) -> Document {
    doc_with_names_dict(Some(Dict::new().insert("EmbeddedFiles", node)))
}

fn doc_without_names() -> Document {
    doc_with_names_dict(None)
}

/// 单层 `/Names` 叶子：`[key value key value ...]`。
fn leaf(pairs: Vec<(Value, Value)>) -> Value {
    let mut flat = Vec::new();
    for (key, value) in pairs {
        flat.push(key);
        flat.push(value);
    }
    Value::Dict(Dict::new().insert("Names", arr(flat)))
}

fn md5_hex(data: &[u8]) -> Value {
    let digest = Md5::digest(data);
    let hex = digest.iter().map(|byte| format!("{byte:02X}")).collect();
    Value::HexString(hex)
}

#[test]
fn embedded_files_decodes_unicode_key_and_filespec_entries() {
    let payload = b"report body".to_vec();
    let doc = doc_with_embedded_node(leaf(vec![(text("报告.pdf"), Value::Ref(20, 0))]));
    for object in filespec_with_stream(20, &payload) {
        doc.set_object(object);
    }

    let files = doc.embedded_files().expect("embedded files");
    assert_eq!(files.len(), 1);
    let (key, filespec) = files.iter().next().expect("one entry");
    assert_eq!(key, "报告.pdf");
    assert_eq!(filespec.unicode_name.as_deref(), Some("报告.pdf"));
    assert_eq!(filespec.short_name.as_deref(), Some("report.pdf"));
    assert_eq!(filespec.description.as_deref(), Some("Quarterly report"));
    assert_eq!(filespec.name(), "报告.pdf");
    assert_eq!(filespec.reference, Some((20, 0)));
    let embedded = filespec.primary().expect("one embedded stream");
    assert_eq!(embedded.reference, Some((21, 0)));
    assert_eq!(embedded.subtype.as_deref(), Some("application/pdf"));
    assert_eq!(embedded.data, payload);

    // 名称树键未命中时回退比较 `/UF` 与 `/F`。
    let by_short = doc
        .embedded_file("report.pdf")
        .expect("lookup")
        .expect("short-name fallback match");
    assert_eq!(by_short.unicode_name.as_deref(), Some("报告.pdf"));
    assert_eq!(doc.embedded_file("nope.pdf").expect("lookup"), None);
}

/// 文件规格对象 `filespec_num` 与其嵌入流 `filespec_num + 1`。
fn filespec_with_stream(filespec_num: i64, payload: &[u8]) -> Vec<Object> {
    let filespec = Dict::new()
        .insert("Type", name("Filespec"))
        .insert("F", Value::String("report.pdf".into()))
        .insert("UF", text("报告.pdf"))
        .insert("Desc", text("Quarterly report"))
        .insert("EF", Value::Ref(filespec_num + 1, 0));
    let stream = embedded_stream(
        filespec_num + 1,
        Dict::new().insert("Subtype", name("application/pdf")),
        payload,
    );
    vec![Object::new(filespec_num, 0, Value::Dict(filespec)), stream]
}

#[test]
fn file_parameters_expose_size_and_declared_checksum() {
    let payload = b"attached-payload".to_vec();
    let params = Dict::new()
        .insert("Size", Value::Integer(payload.len() as i64))
        .insert("CreationDate", text("D:20261028050000Z"))
        .insert("CheckSum", md5_hex(&whitespace_stripped(&payload)));
    let doc = doc_with_embedded_node(leaf(vec![(
        Value::String("data.bin".into()),
        filespec_dict(vec![
            ("F", Value::String("data.bin".into())),
            ("EF", Value::Ref(30, 0)),
        ]),
    )]));
    doc.set_object(embedded_stream(
        30,
        Dict::new().insert("Params", Value::Dict(params)),
        &payload,
    ));

    let filespec = doc
        .embedded_file("data.bin")
        .expect("lookup")
        .expect("present");
    let embedded = filespec.primary().expect("stream");
    assert_eq!(embedded.params.size, Some(payload.len() as i64));
    assert_eq!(embedded.params.checksum, Some(md5_bytes(&payload)));
    assert_eq!(embedded.size_matches(), Some(true));
    assert_eq!(embedded.checksum_matches(), Some(true));
    assert!(embedded.params.creation_date.is_some());
    assert!(
        embedded
            .params
            .dict
            .as_ref()
            .expect("/Params")
            .get("Size")
            .is_some()
    );
}

/// 7.11.4.3：`/CheckSum` 由去掉 ASCII 空白的数据计算。
fn whitespace_stripped(data: &[u8]) -> Vec<u8> {
    data.iter()
        .copied()
        .filter(|byte| !matches!(byte, 0 | 9 | 10 | 12 | 13 | b' '))
        .collect()
}

fn md5_bytes(data: &[u8]) -> [u8; 16] {
    Md5::digest(whitespace_stripped(data)).into()
}

#[test]
fn checksum_ignores_ascii_whitespace_in_stream_data() {
    let payload = b"lines \t with\n spaces \r\n".to_vec();
    let params = Dict::new().insert("CheckSum", md5_hex(&whitespace_stripped(&payload)));
    let doc = doc_with_embedded_node(leaf(vec![(
        Value::String("notes.txt".into()),
        filespec_dict(vec![
            ("F", Value::String("notes.txt".into())),
            ("EF", Value::Ref(30, 0)),
        ]),
    )]));
    doc.set_object(embedded_stream(
        30,
        Dict::new().insert("Params", Value::Dict(params)),
        &payload,
    ));

    let embedded = doc
        .embedded_file("notes.txt")
        .expect("lookup")
        .expect("present")
        .primary()
        .expect("stream")
        .clone();
    assert_eq!(embedded.checksum_matches(), Some(true));
    assert_ne!(embedded.data, whitespace_stripped(&payload));

    // 声明值与实际数据不符时报 false（不报错，交由调用方决策）。
    let mismatched = Dict::new().insert("CheckSum", md5_hex(b"other bytes"));
    let doc = doc_with_embedded_node(leaf(vec![(
        Value::String("notes.txt".into()),
        filespec_dict(vec![
            ("F", Value::String("notes.txt".into())),
            ("EF", Value::Ref(31, 0)),
        ]),
    )]));
    doc.set_object(embedded_stream(
        31,
        Dict::new().insert("Params", Value::Dict(mismatched)),
        &payload,
    ));
    let embedded = doc
        .embedded_file("notes.txt")
        .expect("lookup")
        .expect("present")
        .primary()
        .expect("stream")
        .clone();
    assert_eq!(embedded.checksum_matches(), Some(false));
    assert_eq!(embedded.size_matches(), None);
}

#[test]
fn ef_accepts_direct_nested_and_array_carriers() {
    let doc = doc_with_embedded_node(leaf(vec![
        (
            Value::String("direct.txt".into()),
            // `/EF` 直接是流对象引用。
            filespec_dict(vec![
                ("F", Value::String("direct.txt".into())),
                ("EF", Value::Ref(40, 0)),
            ]),
        ),
        (
            Value::String("nested.txt".into()),
            // `/EF` 是嵌套文件规格，其 `/EF` 才是流。
            filespec_dict(vec![
                ("F", Value::String("nested.txt".into())),
                (
                    "EF",
                    filespec_dict(vec![
                        ("F", Value::String("nested.txt".into())),
                        ("EF", Value::Ref(41, 0)),
                    ]),
                ),
            ]),
        ),
        (
            Value::String("pair.txt".into()),
            // `/EF` 数组：低版本流 + Unicode 名版本流。
            filespec_dict(vec![
                ("F", Value::String("pair.txt".into())),
                ("EF", arr(vec![Value::Ref(42, 0), Value::Ref(43, 0)])),
            ]),
        ),
    ]));
    doc.set_object(embedded_stream(40, Dict::new(), b"direct"));
    doc.set_object(embedded_stream(41, Dict::new(), b"nested"));
    doc.set_object(embedded_stream(42, Dict::new(), b"ascii-version"));
    doc.set_object(embedded_stream(43, Dict::new(), b"unicode-version"));

    let files = doc.embedded_files().expect("embedded files");
    let direct = &files["direct.txt"];
    assert_eq!(direct.embedded.len(), 1);
    assert_eq!(direct.embedded[0].data, b"direct");
    assert_eq!(direct.embedded[0].reference, Some((40, 0)));

    let nested = &files["nested.txt"];
    assert_eq!(nested.embedded.len(), 1);
    assert_eq!(nested.embedded[0].data, b"nested");

    let pair = &files["pair.txt"];
    assert_eq!(pair.embedded.len(), 2);
    assert_eq!(pair.embedded[0].data, b"ascii-version");
    assert_eq!(pair.embedded[1].data, b"unicode-version");
    // 未声明 `/Params` 时不报错，仅无校验信息。
    assert_eq!(pair.embedded[0].checksum_matches(), None);
}

#[test]
fn annotation_filespec_resolves_file_attachment_target() {
    let payload = b"chart.csv,1,2,3".to_vec();
    let annotation = Value::Dict(
        Dict::new()
            .insert("Type", name("Annot"))
            .insert("Subtype", name("FileAttachment"))
            .insert("Rect", rect())
            .insert("FS", Value::Ref(50, 0)),
    );
    let doc = doc_without_names();
    doc.set_object(Object::new(4, 0, annotation));
    doc.set_object(Object::new(
        50,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Filespec"))
                .insert("F", Value::String("chart.csv".into()))
                .insert("EF", Value::Ref(51, 0)),
        ),
    ));
    doc.set_object(embedded_stream(51, Dict::new(), &payload));
    // 页面 `/Annots` 指向该注解。
    doc.set_object(Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Page"))
                .insert("Parent", Value::Ref(2, 0))
                .insert("Annots", arr(vec![Value::Ref(4, 0)])),
        ),
    ));

    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].subtype.name(), "FileAttachment");
    let filespec = doc
        .annotation_filespec(&annotations[0])
        .expect("filespec")
        .expect("attachment has /FS");
    assert_eq!(filespec.short_name.as_deref(), Some("chart.csv"));
    assert_eq!(filespec.primary().expect("stream").data, payload);
}

#[test]
fn absent_or_unknown_embedded_files_are_empty() {
    let doc = doc_without_names();
    assert!(doc.embedded_files().expect("no /Names is empty").is_empty());
    assert_eq!(doc.embedded_file("missing.pdf").expect("lookup"), None);

    let other_category_doc = doc_with_names_dict(Some(Dict::new().insert("Dests", leaf(vec![]))));
    assert!(
        other_category_doc
            .embedded_files()
            .expect("missing category")
            .is_empty()
    );

    let empty_tree = doc_with_embedded_node(leaf(vec![]));
    assert!(empty_tree.embedded_files().expect("empty leaf").is_empty());
}

#[test]
fn malformed_embedded_entries_are_typed_errors() {
    let cases: Vec<(&str, Value, Option<Object>)> = vec![
        (
            "tree value is not a file specification",
            filespec_dict(vec![("F", Value::Integer(7))]),
            None,
        ),
        ("filespec is not a dictionary", Value::Integer(12), None),
        (
            "/EF is not a stream or file specification",
            filespec_dict(vec![("EF", arr(vec![Value::Integer(3)]))]),
            None,
        ),
        (
            "/EF stream params are not a dictionary",
            filespec_dict(vec![("EF", Value::Ref(60, 0))]),
            Some(stream_object(
                60,
                Dict::new().insert("Params", Value::Integer(1)),
                b"x",
            )),
        ),
        (
            "/CheckSum must be 16 bytes",
            filespec_dict(vec![("EF", Value::Ref(61, 0))]),
            Some(stream_object(
                61,
                Dict::new().insert(
                    "Params",
                    Value::Dict(Dict::new().insert("CheckSum", Value::String("short".into()))),
                ),
                b"x",
            )),
        ),
        (
            "/Size must be a non-negative integer",
            filespec_dict(vec![("EF", Value::Ref(62, 0))]),
            Some(stream_object(
                62,
                Dict::new().insert(
                    "Params",
                    Value::Dict(Dict::new().insert("Size", Value::Integer(-4))),
                ),
                b"x",
            )),
        ),
        (
            "/Subtype must be a name",
            filespec_dict(vec![("EF", Value::Ref(63, 0))]),
            Some(stream_object(
                63,
                Dict::new().insert("Subtype", Value::String("pdf".into())),
                b"x",
            )),
        ),
        (
            "/UF must be a text string",
            filespec_dict(vec![("UF", arr(vec![Value::Integer(1)]))]),
            None,
        ),
    ];
    for (label, value, extra) in cases {
        let doc = doc_with_embedded_node(leaf(vec![(Value::String("a.bin".into()), value)]));
        if let Some(object) = extra {
            doc.set_object(object);
        }
        let error = doc
            .embedded_files()
            .expect_err(&format!("{label} must be rejected"));
        assert!(
            matches!(error, EmbeddedFileError::Malformed(_)),
            "{label}: got {error:?}"
        );
    }
}

#[test]
fn filespec_cycles_and_depth_are_typed_errors() {
    // 自环：文件规格的 `/EF` 指回自身。
    let doc = doc_with_embedded_node(leaf(vec![(
        Value::String("loop.bin".into()),
        filespec_dict(vec![
            ("F", Value::String("loop.bin".into())),
            ("EF", Value::Ref(70, 0)),
        ]),
    )]));

    doc.set_object(Object::new(
        70,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Filespec"))
                .insert("EF", Value::Ref(70, 0)),
        ),
    ));
    assert_eq!(
        doc.embedded_files().expect_err("cycle must be rejected"),
        EmbeddedFileError::Malformed("file specification reference cycle".into())
    );

    // 深度链：每层文件规格的 `/EF` 指向下一层，超过深度上限。
    let chain_len = 12usize;
    let doc = doc_with_embedded_node(leaf(vec![(
        Value::String("deep.bin".into()),
        Value::Ref(100, 0),
    )]));
    for step in 0..chain_len {
        let num = 100 + step as i64;
        let next = Value::Ref(num + 1, 0);
        doc.set_object(Object::new(
            num,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", name("Filespec"))
                    .insert("EF", next),
            ),
        ));
    }
    assert_eq!(
        doc.embedded_files()
            .expect_err("depth overflow must be rejected"),
        EmbeddedFileError::LimitExceeded("file specification depth")
    );
}

#[test]
fn relative_and_unmodelled_entries_are_preserved() {
    let payload = b"texture.png".to_vec();
    let doc = doc_with_embedded_node(leaf(vec![(
        Value::String("texture.png".into()),
        filespec_dict(vec![
            ("F", Value::String("texture.png".into())),
            ("EF", Value::Ref(80, 0)),
            ("RF", Value::Ref(81, 0)),
            ("DOS", Value::String("TEXTUR~1.PNG".into())),
        ]),
    )]));
    doc.set_object(embedded_stream(80, Dict::new(), &payload));
    doc.set_object(Object::new(
        81,
        0,
        filespec_dict(vec![("F", Value::String("../texture.png".into()))]),
    ));

    let filespec = doc
        .embedded_file("texture.png")
        .expect("lookup")
        .expect("present");
    // `/RF` 原样保留引用，不解析相对路径。
    assert_eq!(filespec.relative, Some(Value::Ref(81, 0)));
    assert_eq!(
        filespec.dict.get("DOS").and_then(text_value).as_deref(),
        Some("TEXTUR~1.PNG")
    );
    assert_eq!(filespec.primary().expect("stream").data, payload);
}
