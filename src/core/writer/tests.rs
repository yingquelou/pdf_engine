//! writer 模块测试。

use super::*;
use crate::core::types::{Dict, Document, Object, StreamData, Value};

#[test]
fn serialize_simple_values() {
    let mut s = String::new();
    write_value(&Value::Integer(42), &mut s);
    assert_eq!(s, "42");
    s.clear();
    write_value(&Value::Name("Type".into()), &mut s);
    assert_eq!(s, "/Type");
    s.clear();
    write_value(&Value::Ref(3, 0), &mut s);
    assert_eq!(s, "3 0 R");
}

#[test]
fn serialize_dict_and_array() {
    let mut s = String::new();
    let d = Dict::new()
        .insert("Name", Value::Name("Catalog".into()))
        .insert("Pages", Value::Ref(2, 0));
    write_value(&Value::Dict(d), &mut s);
    assert!(s.contains("/Name /Catalog"));
    assert!(s.contains("/Pages 2 0 R"));

    s.clear();
    let a = crate::core::types::Array::new()
        .push(Value::Integer(1))
        .push(Value::Integer(2));
    write_value(&Value::Array(a), &mut s);
    assert_eq!(s, "[1 2]");
}

#[test]
fn write_object_stream() {
    let obj = Object::new(
        1,
        0,
        Value::Stream(
            Dict::new().insert("Length", Value::Integer(5)),
            StreamData::Owned(b"hello".to_vec()),
        ),
    );
    let doc = Document::from_objects(
        vec![obj],
        crate::core::types::Xref::new(),
        crate::core::types::Trailer::new(),
        crate::core::types::StartXref(0),
    );
    let obj = doc.get_object(1).unwrap().unwrap();
    let bytes = write_object(&obj, &doc).unwrap();
    let s = String::from_utf8_lossy(&bytes);
    assert!(s.contains("1 0 obj"));
    assert!(s.contains("stream"));
    assert!(s.contains("hello"));
    assert!(s.contains("endstream"));
    assert!(s.contains("endobj"));
}

#[test]
fn roundtrip_minimal_document() {
    use crate::core::reader::Parser;
    use crate::core::types::{Document, Object, StartXref, Trailer, Value, Xref};

    // 构造一个最小 PDF：1 个 Catalog 对象
    let mut doc = Document::new();
    let catalog = Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    );
    doc.set_object(catalog);
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc.xref = Xref::new();
    doc.startxref = StartXref(0);

    let mut buf = Vec::new();
    write_document(&doc, &mut buf).unwrap();
    let out = String::from_utf8_lossy(&buf);
    // 默认 1.7 ≥1.5：输出 XRef 流 + 对象流（无经典 xref 表/trailer）。
    assert!(out.contains("%PDF-1.7"));
    assert!(out.contains("/Type /XRef"));
    assert!(out.contains("/Type /ObjStm"));
    assert!(out.contains("/Root 1 0 R"));
    assert!(!out.contains("\nxref\n"), "不应再有经典 xref 表");
    assert!(out.contains("startxref"));
    assert!(out.contains("%%EOF"));

    // 回读验证（懒加载：get_object 触发解析）。
    // Parser 现为文件式：把 buf 写临时文件再打开。
    use std::io::Write as _;
    // 沙箱策略按镜像路径拦截：target 目录内的二进制禁止写系统 %TEMP%，
    // 临时文件统一落在构建目录 target/test-tmp/ 下；无法定位 target 时
    // 回退系统临时目录。
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    let _ = std::fs::create_dir_all(&dir);
    let tmp = dir.join(format!(
        "demo-writer-rt-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    {
        let mut f = std::fs::File::create(&tmp).unwrap();
        f.write_all(&buf).unwrap();
        f.flush().unwrap();
    }
    let file = std::fs::File::open(&tmp).unwrap();
    let mut p = Parser::new(file).unwrap();
    let doc2 = p.parse_document().unwrap();
    let cat = doc2.get_object(1).unwrap().expect("catalog");
    assert_eq!(
        cat.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Catalog")
    );
}

// ── ASCII85 串（`<~ ... ~>`）body 门禁 ────────────────────────────────
//
// 口径来源 ISO 32000-1 §7.4.3（`pdftotext -raw` raw L1311-1313：字母表
// `!`(21h)–`u`(75h) 加 `z`(7Ah)、EOD 是两字符 `~>`、白空格一律忽略、
// 其他字符报错）与 §7.2.2 Table 1（raw L738-746 的六个白空格）。
// §7.3.5（raw L911）是 Name Objects，与字符串无关；§7.3.4（raw L815-822）
// 只定义 `( )` 与 `< >` 两种串形态，故 `<~ ~>` 的 body 合法性只能按
// ASCII85 过滤器的字符口径判定。

/// 带一个对象的最小文档（trailer 为空，逐对象门禁只受该对象影响）。
fn one_object_doc(obj: Object) -> Document {
    Document::from_objects(
        vec![obj],
        crate::core::types::Xref::new(),
        crate::core::types::Trailer::new(),
        crate::core::types::StartXref(0),
    )
}

/// 写出对象并取出 `InvalidObject` 消息；其他错误形态直接 panic。
fn reject_message(obj: Object) -> String {
    let doc = one_object_doc(obj);
    let stored = doc.get_object(1).unwrap().expect("object 1 cached");
    let error = write_object(&stored, &doc).expect_err("illegal ascii85 body must be refused");
    match error {
        WriteError::InvalidObject(message) => message,
        other => panic!("expected InvalidObject, got {other}"),
    }
}

/// 合法 body 必须能通过可失败的写入口（`write_object`）。
fn accept_body(body: &str) -> Vec<u8> {
    let doc = one_object_doc(Object::new(1, 0, Value::Ascii85String(body.into())));
    let stored = doc.get_object(1).unwrap().expect("object 1 cached");
    write_object(&stored, &doc)
        .unwrap_or_else(|error| panic!("body {body:?} 应可写出，却报 {error}"))
}

/// (a) 合法 body：写出与旧口径**逐字节相同**的 `<~ body ~>`，门禁不误伤。
#[test]
fn ascii85_string_legal_bodies_write_byte_identical_output() {
    for body in [
        "9jqo^",
        "",
        "z",
        "9jqo^z9jqo^",
        "9jqo^\n 9jqo^\t\r\u{0c}\u{0} ",
        "a<b>c",
        "m!nu",
    ] {
        let mut out = String::new();
        write_value(&Value::Ascii85String(body.into()), &mut out);
        assert_eq!(out, format!("<~{body}~>"), "body {body:?} 须原样包裹");
        let bytes = accept_body(body);
        let text = String::from_utf8_lossy(&bytes).to_string();
        assert!(
            text.contains(&format!("<~{body}~>")),
            "合法 body {body:?} 不得被改写：{text}"
        );
    }

    // 字典条目形态同样原样写出。
    let obj = Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Payload", Value::Ascii85String("9jqo^".into())),
        ),
    );
    let doc = one_object_doc(obj);
    let stored = doc.get_object(1).unwrap().expect("object 1 cached");
    let bytes = write_object(&stored, &doc).expect("legal body");
    assert!(
        String::from_utf8_lossy(&bytes).contains("<~9jqo^~>"),
        "合法 body 不得被改写：{}",
        String::from_utf8_lossy(&bytes)
    );
}

/// (b) body 含终止符 `~>`：类型化拒绝，既不转义也不剥离。
#[test]
fn ascii85_string_body_containing_the_terminator_is_rejected() {
    let message = reject_message(Object::new(1, 0, Value::Ascii85String("a~>b".into())));
    assert!(
        message.contains("~>") && message.contains("truncated"),
        "拒绝理由须点明 EOD 截断风险：{message}"
    );
    assert!(message.contains("byte offset 1"), "须给出偏移：{message}");
}

/// body 含单独的 `~`（7Eh 不在字母表内，且是 `ASCII85Decode` 的提前 EOD）：
/// 同样拒绝——读回虽字节对称，解码出的语义字节却被截短。
#[test]
fn ascii85_string_body_with_a_stray_tilde_is_rejected() {
    for body in ["9jqo^~", "9~jqo^", "9jqo^<~"] {
        let message = reject_message(Object::new(1, 0, Value::Ascii85String(body.into())));
        assert!(
            message.contains("TILDE") || message.contains("~>"),
            "body {body:?}: 须报告 ~ 违规：{message}"
        );
    }
}

/// (c) body 含字母表外字符：`v`–`y`/`{`/`|`/`}`、DEL 与非 ASCII 都拒绝。
#[test]
fn ascii85_string_body_with_an_illegal_character_is_rejected() {
    for (body, offset) in [("9jqo*v", 5usize), ("9jqo\u{7f}", 4), ("9jqo\u{ff5e}", 4)] {
        let message = reject_message(Object::new(1, 0, Value::Ascii85String(body.into())));
        assert!(
            message.contains("outside the ASCII85 alphabet"),
            "body {body:?}: {message}"
        );
        assert!(
            message.contains(&format!("byte offset {offset}")),
            "body {body:?}: 须给出偏移：{message}"
        );
    }
    // `<`（3Ch）与 `>`（3Eh）本身在字母表内，不因字符集被拒。
    accept_body("a<b>c");
}

/// (d) `z` 短写只在组边界合法（§7.4.3 raw L1320-1322 与 raw L1330-1332：
/// 五字节全 0 才写作 `z`，组中间的 `z` 是永不应出现的组合）；白空格
/// （Table 1 六项）不计入组长度。
#[test]
fn ascii85_string_z_shorthand_only_at_a_group_boundary() {
    for body in ["z", "zzz", "9jqo^z", "z9jqo^", "9jqo^\nz 9jqo^z"] {
        accept_body(body);
    }
    for body in ["9jqoz", "9jqo^!z", "9jqo^z9joz"] {
        let message = reject_message(Object::new(1, 0, Value::Ascii85String(body.into())));
        assert!(
            message.contains("partial group"),
            "body {body:?}: 组中间的 z 须被拒：{message}"
        );
    }
}

/// 门禁走完整值树：字典条目、数组元素与流字典里的 body 都要检查。
#[test]
fn ascii85_string_gate_walks_nested_dicts_arrays_and_stream_dicts() {
    let nested = Object::new(
        1,
        0,
        Value::Dict(Dict::new().insert(
            "Outer",
            Value::Array(crate::core::types::Array::new().push(Value::Dict(
                Dict::new().insert("Inner", Value::Ascii85String("9jqo^~>z".into())),
            ))),
        )),
    );
    let message = reject_message(nested);
    assert!(
        message.contains("~>") && message.contains("byte offset 5"),
        "嵌套数组里的违规须被找到：{message}"
    );

    let stream_dict = Object::new(
        1,
        0,
        Value::Stream(
            Dict::new().insert("Sig", Value::Ascii85String("9jqo*v".into())),
            crate::core::types::StreamData::Owned(b"payload".to_vec()),
        ),
    );
    let message = reject_message(stream_dict);
    assert!(
        message.contains("outside the ASCII85 alphabet"),
        "流字典条目的违规须被找到：{message}"
    );
}

/// 文档级预检（装配期通道的同一实现）：覆盖打包进对象流的值与 trailer，
/// 且与逐对象门禁给出同一理由。
#[test]
fn ascii85_string_document_preflight_covers_objects_and_trailer() {
    let doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Payload", Value::Ascii85String("9jqo^".into())),
        ),
    ));
    validate_ascii85_payloads(&doc).expect("合法 body 不得被文档级预检拒绝");

    doc.set_object(Object::new(2, 0, Value::Ascii85String("a~>b".into())));
    let error = validate_ascii85_payloads(&doc).expect_err("对象里的违规须被预检拒绝");
    match error {
        WriteError::InvalidObject(message) => {
            assert!(
                message.contains("object 2") && message.contains("~>"),
                "{message}"
            );
        }
        other => panic!("expected InvalidObject, got {other}"),
    }

    let mut doc = Document::new();
    doc.trailer = crate::core::types::Trailer {
        dict: Dict::new().insert("ID", Value::Ascii85String("9jqo\u{7f}".into())),
    };
    let message = match validate_ascii85_payloads(&doc).expect_err("trailer 违规须被拒绝") {
        WriteError::InvalidObject(message) => message,
        other => panic!("expected InvalidObject, got {other}"),
    };
    assert!(
        message.contains("trailer /ID") && message.contains("outside the ASCII85 alphabet"),
        "{message}"
    );
}

/// 写出 `~>` 违规 body 的对象流容器：预检在容器落进缓冲前触发，
/// 整文档写出保持零字节（打包值只经无错 channel 的 `write_value`）。
#[test]
fn ascii85_string_object_stream_container_write_fails_with_zero_bytes() {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Payload", Value::Ascii85String("a~>b".into())),
        ),
    ));
    doc.trailer = crate::core::types::Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc.xref = crate::core::types::Xref::new();
    doc.startxref = crate::core::types::StartXref(0);

    let mut buf = Vec::new();
    let error = write_document(&doc, &mut buf).expect_err("打包进对象流的非法 body 必须被拒");
    match error {
        WriteError::InvalidObject(message) => assert!(
            message.contains("object 1") && message.contains("~>"),
            "同一理由须从容器咽喉点报出：{message}"
        ),
        other => panic!("expected InvalidObject, got {other}"),
    }
    assert!(buf.is_empty(), "拒绝必须零字节");
}
