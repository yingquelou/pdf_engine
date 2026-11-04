//! 7.4.1 / Table 5 集成：`/Filter` 与 `/DecodeParms` 写成**间接引用**的流对象，
//! 走「写出 → 重新解析 → 解码取内容」全链路。
//!
//! 覆盖四种合法形态（引用→Name、引用→Name 数组、引用→参数字典、引用→参数
//! 字典数组且数组元素本身也是引用）与两种失败形态（引用指向字典、引用成环）：
//! 合法形态的解码结果与 Table 5 例子的直接对象写法一致；失败形态保留原值、
//! 产出 `7.4.1` 诊断且**原始字节不丢**（解码明确失败）。
//!
//! 图像读端（切片 149）复用同一规范化器：`/Filter [ /AHx 2 0 R ]` 形态的
//! 图像 XObject 经写出→重解析后，元数据、解码数据与 `samples()` 与全直接
//! 写法一致，见 `image_xobject_with_indirect_filter_array_roundtrips_like_inline_form`。

use super::*;

/// 全白 + 两行 4 白 4 黑的 Group 4 条带（`filters` 主题同一份样本）。
const CCITT_DATA: &[u8] = &[0x9B, 0x78, 0x00, 0x80, 0x08];

/// 上一条带的期望解码结果（8 列 × 3 行，位展平为 3 字节）。
const CCITT_PLAIN: [u8; 3] = [0xFF, 0xF0, 0xF0];

/// 一个声明给定条目、正文为 [`CCITT_DATA`] 的流对象本体。
fn ccitt_stream(num: i64, entries: &str) -> Vec<u8> {
    let mut body = format!(
        "{num} 0 obj\n<< {entries} /Length {} >>\nstream\n",
        CCITT_DATA.len()
    )
    .into_bytes();
    body.extend_from_slice(CCITT_DATA);
    body.extend_from_slice(b"\nendstream\nendobj\n");
    body
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

/// 混合六种形态的 PDF：
/// - 对象 4：`/Filter 2 0 R`（指向 Name）+ `/DecodeParms 3 0 R`（指向字典）；
/// - 对象 6：`/Filter 5 0 R`（指向 Name 数组）；
/// - 对象 8：`/DecodeParms 7 0 R`（指向字典数组，元素 `[ 3 0 R ]` 仍是引用）；
/// - 对象 10：`/Filter 9 0 R`（指向字典——非法形态）；
/// - 对象 12：`/Filter 11 0 R`（对象 11 自身即 `11 0 R`——环）。
fn indirect_filter_pdf() -> Vec<u8> {
    let objects: Vec<Vec<u8>> = vec![
        b"1 0 obj\n<< /Type /Catalog >>\nendobj\n".to_vec(),
        b"2 0 obj\n/CCITTFaxDecode\nendobj\n".to_vec(),
        b"3 0 obj\n<< /K -1 /Columns 8 /Rows 3 >>\nendobj\n".to_vec(),
        ccitt_stream(4, "/Filter 2 0 R /DecodeParms 3 0 R"),
        b"5 0 obj\n[ /CCITTFaxDecode ]\nendobj\n".to_vec(),
        ccitt_stream(6, "/Filter 5 0 R /DecodeParms 3 0 R"),
        b"7 0 obj\n[ 3 0 R ]\nendobj\n".to_vec(),
        ccitt_stream(8, "/Filter /CCITTFaxDecode /DecodeParms 7 0 R"),
        b"9 0 obj\n<< /Columns 8 >>\nendobj\n".to_vec(),
        ccitt_stream(10, "/Filter 9 0 R"),
        b"11 0 obj\n11 0 R\nendobj\n".to_vec(),
        ccitt_stream(12, "/Filter 11 0 R"),
    ];
    assemble(&objects)
}

/// 取指定对象的流值。
fn stream_value(doc: &Document, num: i64) -> Value {
    doc.get_object(num)
        .unwrap_or_else(|error| panic!("get_object({num}) 失败：{error}"))
        .unwrap_or_else(|| panic!("对象 {num} 缺失"))
        .value
}

/// 断言三个合法形态的流都能解出同一份明文。
fn assert_valid_forms_decode(doc: &Document, stage: &str) {
    for num in [4, 6, 8] {
        let value = stream_value(doc, num);
        assert!(
            matches!(value, Value::Stream(_, _)),
            "{stage}: 对象 {num} 不是流"
        );
        assert_eq!(
            doc.decode_stream_bytes(&value)
                .unwrap_or_else(|error| panic!("{stage}: 对象 {num} 解码失败：{error}")),
            CCITT_PLAIN,
            "{stage}: 对象 {num} 的间接引用形态解码结果不符"
        );
    }
    assert!(
        !doc.warnings()
            .iter()
            .any(|warning| warning.contains("7.4.1")),
        "{stage}: 合法形态不应产生诊断：{:?}",
        doc.warnings()
    );
}

/// 解析（写出前）与重解析（写出后）都走一遍全链路。
#[test]
fn indirect_filter_and_parms_forms_roundtrip() {
    let bytes = indirect_filter_pdf();
    let doc = parse_buf(&bytes);
    assert_valid_forms_decode(&doc, "解析后");

    let mut out = Vec::new();
    core::writer::DocumentWriter::new()
        .build(&doc, &mut out)
        .expect("写出应成功（间接引用形态的流对象不得被丢弃）");
    let reparsed = parse_buf(&out);
    assert_valid_forms_decode(&reparsed, "写出重读后");
}

/// 失败形态（引用指向字典）：保留原值 + 诊断 + 解码失败，原始字节不丢。
#[test]
fn indirect_filter_to_illegal_target_is_diagnosed_and_preserved() {
    for (stage, doc) in [
        ("解析后", parse_buf(&indirect_filter_pdf())),
        ("写出重读后", {
            let doc = parse_buf(&indirect_filter_pdf());
            let mut out = Vec::new();
            core::writer::DocumentWriter::new()
                .build(&doc, &mut out)
                .expect("违例文档也要能写出");
            parse_buf(&out)
        }),
    ] {
        let value = stream_value(&doc, 10);
        let error = match doc.decode_stream_bytes(&value) {
            Err(error) => error,
            Ok(bytes) => panic!("{stage}: 引用指向字典必须解码失败，却得到 {bytes:?}"),
        };
        assert!(error.contains("/Filter"), "{stage}: got {error}");
        assert_eq!(
            doc.stream_bytes(&value).as_deref(),
            Some(CCITT_DATA),
            "{stage}: 原始流字节不得丢失"
        );
        assert!(
            doc.warnings()
                .iter()
                .any(|warning| warning.contains("7.4.1") && warning.contains("resolves to dict")),
            "{stage}: 诊断缺失：{:?}",
            doc.warnings()
        );
        // 原值仍是引用（未被静默改写）。
        assert_eq!(
            value.as_dict().expect("stream dict").get("Filter"),
            Some(&Value::Ref(9, 0)),
            "{stage}: 非法形态必须保留原值"
        );
    }
}

/// 失败形态（引用成环）：诊断 + 解码失败，且绝不无限递归。
#[test]
fn cyclic_indirect_filter_is_diagnosed() {
    let doc = parse_buf(&indirect_filter_pdf());
    let value = stream_value(&doc, 12);
    assert!(
        doc.decode_stream_bytes(&value).is_err(),
        "环引用不得解码成功"
    );
    assert!(
        doc.warnings()
            .iter()
            .any(|warning| warning.contains("7.4.1") && warning.contains("cyclic")),
        "环诊断缺失：{:?}",
        doc.warnings()
    );
    assert_eq!(doc.stream_bytes(&value).as_deref(), Some(CCITT_DATA));
}

/// Table 5 的 shall 个数核对同样落到 `warnings`（多过滤器配单参数字典）。
#[test]
fn decode_parms_count_mismatch_is_reported_while_parsing_stream() {
    let objects: Vec<Vec<u8>> = vec![
        b"1 0 obj\n<< /Type /Catalog >>\nendobj\n".to_vec(),
        b"2 0 obj\n<< /K -1 /Columns 8 /Rows 3 >>\nendobj\n".to_vec(),
        ccitt_stream(
            3,
            "/Filter [ /ASCIIHexDecode /CCITTFaxDecode ] /DecodeParms 2 0 R",
        ),
    ];
    let doc = parse_buf(&assemble(&objects));
    let value = stream_value(&doc, 3);
    assert!(doc.decode_stream_bytes(&value).is_err());
    assert!(
        doc.warnings()
            .iter()
            .any(|warning| warning.contains("one entry for each filter")),
        "shall 诊断缺失：{:?}",
        doc.warnings()
    );
}

/// 2×1 DeviceGray 8-bit 图像流：数据为 ASCIIHex→RunLength 两级编码
/// （`0100FF80>` → 字节 01 00 FF 80 → 字面段 [00 FF] + EOD），
/// 期望样本 [0.0, 1.0]。`/Filter` 取调用方给定的字面形态。
fn image_stream(num: i64, filter: &str) -> Vec<u8> {
    let data: &[u8] = b"0100FF80>";
    let mut body = format!(
        "{num} 0 obj\n<< /Type /XObject /Subtype /Image /Width 2 /Height 1 \
         /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter {filter} /Length {} >>\nstream\n",
        data.len()
    )
    .into_bytes();
    body.extend_from_slice(data);
    body.extend_from_slice(b"\nendstream\nendobj\n");
    body
}

/// 图像读端复用 7.4.1 规范化器（切片 149）：`/Filter [ /AHx 2 0 R ]` 这类
/// **数组元素为间接引用**的图像 XObject，写出→重解析后，规范名列表、解码
/// 数据与 `samples()` 检索与全直接写法完全一致；引用指向缺失对象则诊断 +
/// 明确失败（不静默误解码）。
#[test]
fn image_xobject_with_indirect_filter_array_roundtrips_like_inline_form() {
    let objects: Vec<Vec<u8>> = vec![
        b"1 0 obj\n<< /Type /Catalog >>\nendobj\n".to_vec(),
        b"2 0 obj\n/RunLengthDecode\nendobj\n".to_vec(),
        image_stream(3, "[ /AHx 2 0 R ]"), // 缺口形态：数组元素是引用
        image_stream(4, "[ /AHx /RunLengthDecode ]"), // 全直接基准
        image_stream(5, "[ /AHx 99 0 R ]"), // 引用指向缺失对象
    ];
    let bytes = assemble(&objects);
    let stages = [
        ("解析后", parse_buf(&bytes)),
        ("写出重读后", {
            let doc = parse_buf(&bytes);
            let mut out = Vec::new();
            core::writer::DocumentWriter::new()
                .build(&doc, &mut out)
                .expect("间接引用 /Filter 数组的图像流不得被写出丢弃");
            parse_buf(&out)
        }),
    ];
    for (stage, doc) in stages {
        let indirect = doc.image(&stream_value(&doc, 3)).unwrap_or_else(|error| {
            panic!("{stage}: 元素引用形态的 /Filter 数组应可解析：{error}")
        });
        let inline = doc
            .image(&stream_value(&doc, 4))
            .expect("全直接基准应可解析");
        assert_eq!(
            indirect.filters,
            vec!["ASCIIHexDecode".to_string(), "RunLengthDecode".to_string()],
            "{stage}: 规范名列表"
        );
        assert_eq!(indirect.filters, inline.filters, "{stage}: 过滤器列表");
        assert_eq!(indirect.width, 2, "{stage}: 元数据 /Width");
        assert_eq!(indirect.height, 1, "{stage}: 元数据 /Height");
        assert_eq!(indirect.data, inline.data, "{stage}: 解码数据");
        let samples = indirect
            .samples()
            .unwrap_or_else(|error| panic!("{stage}: 样本读取失败：{error}"));
        assert_eq!(samples.values, vec![0.0, 1.0], "{stage}: 像素样本");
        assert_eq!(
            samples.values,
            inline.samples().expect("基准样本").values,
            "{stage}: 与全直接写法样本不一致"
        );
        assert!(
            !doc.warnings()
                .iter()
                .any(|warning| warning.contains("7.4.1")),
            "{stage}: 合法形态不应产生诊断：{:?}",
            doc.warnings()
        );

        // 负面：引用指向缺失对象 → 诊断 + 失败，绝不静默按剩余过滤器解码。
        let error = doc
            .image(&stream_value(&doc, 5))
            .expect_err("{stage}: 缺失对象的引用元素必须失败而非误解码");
        assert!(
            error
                .to_string()
                .contains("/Filter array has a non-name element"),
            "{stage}: got {error}"
        );
        assert!(
            doc.warnings()
                .iter()
                .any(|warning| { warning.contains("7.4.1") && warning.contains("absent object") }),
            "{stage}: absent 诊断缺失：{:?}",
            doc.warnings()
        );
    }
}
