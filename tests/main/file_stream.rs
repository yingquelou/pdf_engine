//! 外部文件流读端集成（7.3.8 / 7.4.1 + Table 5 的 `/F`、`/FFilter`、
//! `/FDecodeParms`；文件规格见 7.11.2–7.11.3 Table 44）。
//!
//! 按 `filters_indirect` 主题的风格内联构造 PDF 字节：
//! - 合法形态：`/F` 引用字典形态文件规格（`/UF` 用 UTF-16BE 十六进制串）、
//!   `/FFilter` 数组元素与 `/FDecodeParms` 数组元素写成间接引用——与解码
//!   路径共享同一规范化器（`filters::normalize_filter_entries`）；
//! - 违例形态：有 `/FFilter` 缺 `/F`、参数个数与过滤器个数不匹配，均返回
//!   类型化错误；同文档里的普通流仍 `Ok(None)`，不被读红。

use super::*;

use pdf_engine::core::reader::file_stream::ExternalFileStreamError;

/// 上一条带主题的流对象：正文为空（Table 5：`/F` 存在时 `stream` 与
/// `endstream` 之间的字节一律忽略），`/Length 0`。
fn external_stream(num: i64, entries: &str) -> Vec<u8> {
    format!("{num} 0 obj\n<< {entries} /Length 0 >>\nstream\nendstream\nendobj\n").into_bytes()
}

/// 按顺序写出对象并生成经典 xref 表 + trailer（偏移实测，`/Root 1 0 R`）。
fn assemble(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut buf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for body in objects {
        offsets.push(buf.len());
        buf.extend_from_slice(body);
    }
    let xref_offset = buf.len();
    buf.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    buf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets {
        buf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    buf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    buf
}

/// 合法 + 违例混合的样本文件：
/// - 对象 2：文件规格字典 `<< /F (raster.dat) /UF <UTF-16BE résumé.pdf> >>`；
/// - 对象 3：`/FDecodeParms` 数组元素引用的参数字典；
/// - 对象 4：`/FFilter` 数组元素引用的过滤器名；
/// - 对象 5：外部文件流 `/F 2 0 R`、`/FFilter [ /ASCIIHexDecode 4 0 R ]`、
///   `/FDecodeParms [ null 3 0 R ]`；
/// - 对象 6：普通 Flate 流（无 Table 5 外部文件条目）；
/// - 对象 7：`/FFilter` 存在但缺 `/F`（违例）；
/// - 对象 8：两个过滤器配单参数字典（shall 违例）。
fn external_file_stream_pdf() -> Vec<u8> {
    // "résumé.pdf" 的 UTF-16BE（带 BOM）十六进制串形态，验证 /UF 解码。
    let objects: Vec<Vec<u8>> = vec![
        b"1 0 obj\n<< /Type /Catalog >>\nendobj\n".to_vec(),
        b"2 0 obj\n<< /F (raster.dat) /UF <FEFF007200E900730075006D00E9002E007000640066> >>\nendobj\n".to_vec(),
        b"3 0 obj\n<< /Columns 8 >>\nendobj\n".to_vec(),
        b"4 0 obj\n/FlateDecode\nendobj\n".to_vec(),
        external_stream(
            5,
            "/F 2 0 R /FFilter [ /ASCIIHexDecode 4 0 R ] /FDecodeParms [ null 3 0 R ]",
        ),
        external_stream(6, "/Filter /FlateDecode"),
        external_stream(7, "/FFilter /FlateDecode"),
        external_stream(
            8,
            "/F (x.dat) /FFilter [ /ASCIIHexDecode /RunLengthDecode ] /FDecodeParms 3 0 R",
        ),
    ];
    assemble(&objects)
}

#[test]
fn external_file_stream_reads_typed_model_from_parsed_bytes() {
    let doc = parse_buf(&external_file_stream_pdf());
    let model = doc
        .external_file_stream_object(5)
        .expect("合法外部文件流")
        .expect("对象 5 是外部文件流");
    assert_eq!(model.reference, Some((5, 0)));
    assert_eq!(model.filters, vec!["ASCIIHexDecode", "FlateDecode"]);
    assert_eq!(model.decode_parms.len(), 2);
    assert_eq!(
        model
            .parms_for(1)
            .and_then(|params| params.get("Columns"))
            .cloned(),
        Some(Value::Integer(8)),
        "引用形态的 /FDecodeParms 元素应与解码路径同一规范化器解引用"
    );
    // /UF 优先展示（7.11.3），/F 原样保留向后兼容。
    assert_eq!(model.display_name(), "résumé.pdf");
    assert_eq!(model.file.short_name.as_deref(), Some("raster.dat"));

    // 普通流与非流对象：Ok(None)，不产生任何诊断。
    let plain = doc.get_object(6).expect("object").expect("object 6").value;
    assert_eq!(
        doc.external_file_stream(&plain).expect("普通流不得报错"),
        None
    );
    assert_eq!(
        doc.external_file_stream_object(1)
            .expect("目录不是流")
            .as_ref(),
        None
    );
    assert_eq!(
        doc.external_file_stream_object(404).expect("缺失对象"),
        None
    );
}

#[test]
fn broken_external_streams_surface_typed_problems_without_redding_others() {
    let doc = parse_buf(&external_file_stream_pdf());

    // 有 /FFilter 缺 /F：外部文件不可指认。
    let error = doc
        .external_file_stream_object(7)
        .expect_err("/FFilter 缺 /F 必须是类型化错误");
    assert!(matches!(error, ExternalFileStreamError::Malformed(_)));
    assert!(error.to_string().contains("without /F"), "got {error}");

    // 两过滤器配单参数字典：Table 5 shall 违例。
    let error = doc
        .external_file_stream_object(8)
        .expect_err("个数不匹配必须是类型化错误");
    assert!(
        error.to_string().contains("single dictionary"),
        "got {error}"
    );

    // 同文档普通流依旧是 Ok(None)；违例只落在被询问的对象上。
    let plain = doc.get_object(6).expect("object").expect("object 6").value;
    assert_eq!(
        doc.external_file_stream(&plain).expect("普通流不得报错"),
        None
    );
}

#[test]
fn string_form_f_and_direct_forms_read_like_references() {
    // 同一语义的直接对象写法（/F 字符串形态 + 直接名/字典），与引用形态
    // 的读取结果对齐（7.3.10 等价性，库策略：两形态一律类型化）。
    let objects: Vec<Vec<u8>> = vec![
        b"1 0 obj\n<< /Type /Catalog >>\nendobj\n".to_vec(),
        external_stream(2, "/F (raster.dat) /FFilter /ASCIIHexDecode"),
    ];
    let doc = parse_buf(&assemble(&objects));
    let model = doc
        .external_file_stream_object(2)
        .expect("直接形态")
        .expect("对象 2 是外部文件流");
    assert_eq!(model.filters, vec!["ASCIIHexDecode"]);
    assert_eq!(
        model.decode_parms,
        vec![None],
        "缺 /FDecodeParms 对齐为 None"
    );
    assert_eq!(model.display_name(), "raster.dat");
    assert_eq!(model.file.unicode_name, None);
}
