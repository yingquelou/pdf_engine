//! 正例与非线性化：严格按附录 F 装配的夹具零诊断；非线性化文件返回 `None`。

use super::super::FIRST_BYTES_LIMIT;
use super::*;
use crate::core::reader::linearization::{HintRange, HintStreams};

/// 默认夹具即严格合规形态：识别成功、Table F.1 条目全部类型化、零诊断。
#[test]
fn annex_f_conformant_fixture_is_consistent() {
    let bytes = build(&Knobs::default());
    let document = open(&bytes, "lin-conform");
    let info = document
        .linearization()
        .expect("no I/O error")
        .expect("recognized as linearized");

    assert!(
        info.is_consistent(),
        "conformant fixture must have no issue: {:?}",
        info.issues
    );
    assert!(!info.treated_as_ordinary());
    assert!(document.is_linearized());

    // F.3.3：字典是正文第一个间接对象，且整个落在前 1024 字节内。
    assert_eq!(info.object_number, DICT);
    assert_eq!(info.first_body_object, DICT);
    assert_eq!(info.generation, 0);
    assert_eq!(info.offset, object_offset(&bytes, "5 0"));
    assert!(info.end_offset <= FIRST_BYTES_LIMIT);

    // Table F.1 类型化。
    assert_eq!(info.version, Some(1.0));
    assert_eq!(info.declared_file_length, Some(bytes.len() as u64));
    assert_eq!(info.file_size, Some(bytes.len() as u64));
    assert_eq!(info.first_page_object, Some(PAGE1));
    assert_eq!(info.page_count, Some(PAGE_TOTAL));
    // `/P` 缺失 → Table F.1「Default value: 0」。
    assert_eq!(info.first_page_number, 0);
    assert!(info.dict.get("P").is_none());

    // Table F.1 `/H`：offset1 是**流对象**起始（而非流数据起始），
    // length1 含流对象开销——即对象 10 的完整字节数。
    let hint_offset = object_offset(&bytes, "10 0");
    assert_eq!(
        info.hint_streams,
        Some(HintStreams {
            primary: HintRange {
                offset: hint_offset,
                length: object_offset(&bytes, "6 0") - hint_offset,
            },
            overflow: None,
        })
    );

    // Table F.1 `/E`：首页对象组末尾（其后紧接第二组的第一个对象）。
    assert_eq!(info.first_page_end, Some(object_offset(&bytes, "1 0")));

    // Table F.1 `/T`：传统主表首条目**之前**的那个空白字符。
    let main_target = info.main_xref_offset.expect("/T present");
    assert_eq!(bytes[main_target as usize], b'\n');

    // 识别不改变普通路径读取：两页均可按页面树取到，且无告警。
    assert_eq!(document.page_count().expect("page count"), 2);
    assert!(document.warnings().is_empty());
}

/// 纯交叉引用流形态的主表：`/T` 取主 XRef 流对象偏移（Table F.1 的
/// PDF 1.5 段），F.3.11 的主 trailer 语义由流字典承担。
#[test]
fn annex_f_xref_stream_main_table_is_consistent() {
    let knobs = Knobs {
        version: "1.5",
        main_is_stream: true,
        ..Default::default()
    };
    let bytes = build(&knobs);
    let document = open(&bytes, "lin-xref-stream-main");
    let info = document
        .linearization()
        .expect("no I/O error")
        .expect("recognized");
    assert!(info.is_consistent(), "issues: {:?}", info.issues);
    assert_eq!(
        info.main_xref_offset,
        Some(object_offset(&bytes, "11 0")),
        "/T 应指向主交叉引用流对象本身"
    );
    // 首页表 + 主 XRef 流段：F.3.1 要求的恰好两段。
    assert_eq!(document.xref.0.len(), 2);
}

/// Table F.1 `/H` 的四整数形态：溢出 hint 流一并类型化。
#[test]
fn overflow_hint_stream_pair_is_typed() {
    let knobs = Knobs {
        overflow_hint: true,
        ..Default::default()
    };
    let bytes = build(&knobs);
    let info = open(&bytes, "lin-overflow-hint")
        .linearization()
        .expect("no I/O error")
        .expect("recognized");
    let streams = info.hint_streams.expect("/H typed");
    assert!(streams.primary.length > 0);
    assert_eq!(
        streams.overflow,
        Some(HintRange {
            offset: object_offset(&bytes, "8 0"),
            length: 20,
        })
    );
    assert!(info.is_consistent(), "issues: {:?}", info.issues);
}

/// `/Length` 写成间接引用的 hint 流：`length1` 须含该长度对象的开销
/// （Table F.1），而字典条目本身仍全为直接对象，故零诊断。
#[test]
fn indirect_stream_length_object_overhead_counts_in_h() {
    let knobs = Knobs {
        replaced: &[("LengthRef", "1")],
        ..Default::default()
    };
    let bytes = build(&knobs);
    let info = open(&bytes, "lin-indirect-length")
        .linearization()
        .expect("no I/O error")
        .expect("recognized");
    let streams = info.hint_streams.expect("/H typed");
    // 实测长度覆盖到 `12 0 obj` 之后（首页对象组从目录对象开始）。
    assert_eq!(
        streams.primary.offset + streams.primary.length,
        object_offset(&bytes, "6 0"),
        "length1 应含流对象开销与其间接 /Length 对象"
    );
    assert!(info.is_consistent(), "issues: {:?}", info.issues);
}

/// 非线性化：正文第一个对象没有 `/Linearized` → `Ok(None)`、无告警，
/// 普通路径照常读取（F.1「can still successfully process linearized
/// files」的反向要求：识别不得影响既有读取）。
#[test]
fn ordinary_pdf_returns_none_without_warning() {
    let knobs = Knobs {
        omit: &["Linearized"],
        ..Default::default()
    };
    let bytes = build(&knobs);
    let document = open(&bytes, "lin-ordinary");
    assert!(document.linearization().expect("no I/O error").is_none());
    assert!(!document.is_linearized());
    assert!(document.warnings().is_empty());
    assert_eq!(document.page_count().expect("page count"), 2);
}

/// 写入端构造的内存文档没有文件源，无从按 F.3.2/F.3.3 判定头部与首对象
/// → `Ok(None)`。
#[test]
fn in_memory_document_returns_none() {
    let document = Document::new();
    assert!(document.linearization().expect("no I/O error").is_none());
    assert!(!document.is_linearized());
}
