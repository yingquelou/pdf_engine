//! 外部文件流读端集成（7.3.8 / 7.4.1 + Table 5 的 `/F`、`/FFilter`、
//! `/FDecodeParms`；文件规格见 7.11.2–7.11.3 Table 44）。
//!
//! 按 `filters_indirect` 主题的风格内联构造 PDF 字节：
//! - 合法形态：`/F` 引用字典形态文件规格（`/UF` 用 UTF-16BE 十六进制串）、
//!   `/FFilter` 数组元素与 `/FDecodeParms` 数组元素写成间接引用——与解码
//!   路径共享同一规范化器（`filters::normalize_filter_entries`）；
//! - 违例形态：有 `/FFilter` 缺 `/F`、参数个数与过滤器个数不匹配，均返回
//!   类型化错误；同文档里的普通流仍 `Ok(None)`，不被读红；
//! - 交叉核对形态（版本门／`Crypt` 加密／内联字节与 `/Length`）：真实字节
//!   里照常解析，不符之处只落在 `ExternalFileStream::problems`。

use super::*;

use pdf_engine::core::reader::file_stream::{ExternalFileStreamError, ExternalFileStreamProblem};
use pdf_engine::core::types::PdfVersion;

/// 上一条带主题的流对象：正文为空（Table 5：`/F` 存在时 `stream` 与
/// `endstream` 之间的字节一律忽略），`/Length 0`。
fn external_stream(num: i64, entries: &str) -> Vec<u8> {
    format!("{num} 0 obj\n<< {entries} /Length 0 >>\nstream\nendstream\nendobj\n").into_bytes()
}

/// 带 `/Length` 与内联正文的流对象（`{body}` 原样写在 `stream` 之后）。
fn external_stream_with_body(num: i64, entries: &str, length: i64, body: &str) -> Vec<u8> {
    format!("{num} 0 obj\n<< {entries} /Length {length} >>\nstream\n{body}endstream\nendobj\n")
        .into_bytes()
}

/// 按顺序写出对象并生成经典 xref 表 + trailer（偏移实测，`/Root 1 0 R`）。
fn assemble_with_header(header: &str, objects: &[Vec<u8>]) -> Vec<u8> {
    let mut buf = header.as_bytes().to_vec();
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

fn assemble(objects: &[Vec<u8>]) -> Vec<u8> {
    assemble_with_header("%PDF-1.4\n", objects)
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

/// 版本门：`%PDF-1.1` 声明里用了 Table 5 的 PDF 1.2 条目（raw L1095/L1102/
/// L1106）——照常解析成模型，另给出 `VersionGate`。
#[test]
fn pdf_1_1_file_using_the_pdf_1_2_entries_is_read_and_gated() {
    let objects: Vec<Vec<u8>> = vec![
        b"1 0 obj\n<< /Type /Catalog >>\nendobj\n".to_vec(),
        external_stream(2, "/F (raster.dat) /FFilter /FlateDecode"),
    ];
    let doc = parse_buf(&assemble_with_header("%PDF-1.1\n", &objects));
    let model = doc
        .external_file_stream_object(2)
        .expect("门禁不拦读取")
        .expect("对象 2 是外部文件流");
    assert_eq!(model.filters, vec!["FlateDecode"]);
    assert_eq!(model.display_name(), "raster.dat");
    assert_eq!(
        model.problems,
        vec![ExternalFileStreamProblem::VersionGate {
            entries: vec!["F", "FFilter"],
            declared: PdfVersion::V1_1,
        }]
    );

    // 同一份对象体换个 1.2 头部即无门禁诊断（阈值取条目自身的 PDF 1.2 标注）。
    let newer = parse_buf(&assemble_with_header("%PDF-1.2\n", &objects));
    assert!(
        newer
            .external_file_stream_object(2)
            .expect("1.2 文档")
            .expect("外部文件流")
            .problems
            .is_empty()
    );
}

/// 加密规则：`/F` 在而 `/FFilter` 写 `Crypt`——7.6.1 的 shall（raw
/// L2743–L2745），照常给出模型值并记一条非提示级诊断。
#[test]
fn crypt_ffilter_on_external_file_data_is_reported_not_fatal() {
    let objects: Vec<Vec<u8>> = vec![
        b"1 0 obj\n<< /Type /Catalog >>\nendobj\n".to_vec(),
        external_stream(2, "/F (raster.dat) /FFilter [ /ASCIIHexDecode /Crypt ]"),
    ];
    let doc = parse_buf(&assemble(&objects));
    let model = doc
        .external_file_stream_object(2)
        .expect("Crypt 不阻断读取")
        .expect("对象 2 是外部文件流");
    assert_eq!(
        model.filters,
        vec!["ASCIIHexDecode", "Crypt"],
        "过滤器清单原样保留"
    );
    assert_eq!(
        model.problems,
        vec![ExternalFileStreamProblem::EncryptedExternalData {
            filter: "Crypt".to_owned()
        }]
    );
    assert!(
        !model.problems[0].is_advisory(),
        "shall 违例不该被归入提示级"
    );
}

/// 忽略的内联字节：`/F` 在、`stream`…`endstream` 之间真写了 5 个字节
/// （Table 5 raw L1096–L1097）——模型不读它们，只报「已忽略」；`/Length`
/// 报小 1 个字节时另记一条 should 级不符（raw L1097–L1099）。
#[test]
fn inline_bytes_and_under_declared_length_with_f_are_reported() {
    let objects: Vec<Vec<u8>> = vec![
        b"1 0 obj\n<< /Type /Catalog >>\nendobj\n".to_vec(),
        // /Length 与内联字节一致：只有「已忽略」一条。
        external_stream_with_body(2, "/F (raster.dat) /FFilter /FlateDecode", 5, "12345"),
        // 内联 `AB\t \n`：记录范围 3 字节，`endstream` 前还剩 1 个空格 +
        // 一个未计入 EOL，故实际内联字节是 4 而 /Length 报 3。
        external_stream_with_body(3, "/F (raster.dat) /FFilter /FlateDecode", 3, "AB\t \n"),
    ];
    let doc = parse_buf(&assemble(&objects));
    let model = doc
        .external_file_stream_object(2)
        .expect("带正文的外部文件流")
        .expect("对象 2 是外部文件流");
    assert_eq!(
        model.problems,
        vec![ExternalFileStreamProblem::InlineBytesIgnored { bytes: 5 }]
    );
    assert!(model.problems[0].is_advisory());

    let mismatched = doc
        .external_file_stream_object(3)
        .expect("少报 /Length 的外部文件流")
        .expect("对象 3 是外部文件流");
    assert_eq!(
        mismatched.problems,
        vec![
            ExternalFileStreamProblem::InlineBytesIgnored { bytes: 4 },
            ExternalFileStreamProblem::LengthMismatch {
                declared: 3,
                actual: 4
            },
        ],
        "记录范围后的空格要计入实际字节数，允许的未计入 EOL 则不算"
    );
    assert!(
        mismatched
            .problems
            .iter()
            .all(ExternalFileStreamProblem::is_advisory),
        "已忽略字节与 /Length 都只到提示/should 级"
    );
}

/// 交叉核对不改既有口径：落地时 Accepted 的样本依旧零诊断，且
/// `external_file_stream*` 不往 `Document::warnings` 里写任何东西。
#[test]
fn cross_checks_leave_accepted_files_and_document_warnings_untouched() {
    let doc = parse_buf(&external_file_stream_pdf());
    let before = doc.warnings().len();
    let model = doc
        .external_file_stream_object(5)
        .expect("合法外部文件流")
        .expect("对象 5 是外部文件流");
    assert!(model.problems.is_empty(), "got {:?}", model.problems);
    // 违例样本仍是硬错误（不是降级为诊断）。
    assert!(doc.external_file_stream_object(7).is_err());
    assert!(doc.external_file_stream_object(8).is_err());
    // 普通流仍 Ok(None)，且整条路径不改文档警告。
    let plain = doc.get_object(6).expect("object").expect("object 6").value;
    assert_eq!(doc.external_file_stream(&plain).expect("普通流"), None);
    assert_eq!(
        doc.warnings().len(),
        before,
        "交叉核对不得写入文档警告：{:?}",
        doc.warnings()
    );
}
