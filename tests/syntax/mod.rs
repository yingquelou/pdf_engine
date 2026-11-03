//! 集成测试：第 7 章语法与文件结构主题。
//!
//! 覆盖词法/对象头、经典交叉引用表与 XRef 流、对象流、历史 `/Prev` 链、
//! 流过滤器解码、写出布局（xref/对象流降级）与 Name 转义、版本与特性门控的
//! 解析→写出→再解析往返；共享辅助见 `tests/support/mod.rs`。

use pdf_engine::core;
use pdf_engine::core::types::{
    Dict, Document, Object, PdfFeature, PdfVersion, StreamData, Trailer, Value, XrefKind,
};
use std::io::Write;

#[path = "../support/mod.rs"]
mod support;

use support::*;

mod ascii85_string;
mod classic_xref;
mod filters_and_names;
mod incremental_read;
mod object_stream;
mod reader_lexical;
mod writer_version_gates;
mod xref_stream_read;

/// 跳过文件头部的 `%PDF-x.y\n%<4 bytes>\n`（共 14 字节左右）。
fn strip_pdf_header(buf: &[u8]) -> Vec<u8> {
    // 找到第二个换行后的位置
    let mut nl_count = 0;
    for (i, &b) in buf.iter().enumerate() {
        if b == b'\n' {
            nl_count += 1;
            if nl_count == 2 {
                return buf[i + 1..].to_vec();
            }
        }
    }
    buf.to_vec()
}

/// 生成一条 XRef 流条目（`/W [1 2 1]`：1 字节 type + 2 字节字段 + 1 字节字段）。
fn xref_row(t: u8, f2: u16, f3: u8) -> Vec<u8> {
    let mut v = vec![t];
    v.extend_from_slice(&f2.to_be_bytes());
    v.push(f3);
    v
}

/// 追加一个 XRef 流对象并以 `startxref` + `%%EOF` 收尾（`/W [1 2 1]`、
/// `/Index [0 index_size]`、`/Size index_size`、`/Root 1 0 R`）。
///
/// `num` 为 XRef 流自身对象号；`off` 为其文件偏移——调用方须在构造条目
/// **之前**以 `buf.len()` 快照（条目可能引用该偏移）；`dict_extra` 追加到
/// 流字典（如 `/Filter`、`/DecodeParms`）。
fn append_xref_stream(
    buf: &mut Vec<u8>,
    num: i64,
    off: usize,
    index_size: usize,
    dict_extra: &str,
    entries: &[u8],
) {
    let hdr = format!(
        "{num} 0 obj\n<< /Type /XRef /Size {index_size} /Index [0 {index_size}] \
         /W [1 2 1]{dict_extra} /Length {} /Root 1 0 R >>\nstream\n",
        entries.len()
    );
    buf.extend_from_slice(hdr.as_bytes());
    buf.extend_from_slice(entries);
    buf.extend_from_slice(b"\nendstream\nendobj\nstartxref\n");
    buf.extend_from_slice(format!("{off}\n").as_bytes());
    buf.extend_from_slice(b"%%EOF\n");
}

/// 构造一个含 XRef 流（type 1 顶层对象 + type 2 指向对象流）的 PDF：
/// 对象 1 为 Catalog（引用对象 5）；对象 6 为对象流（ObjStm，内含对象
/// 5）；对象 7 为 XRef 流。
///
/// - `filter`：对象流的 `/Filter` 名（空串 = 未压缩）；
/// - `compress`：为真时用 zlib 真压缩对象流字节（需配 `/FlateDecode`）。
fn obj_stm_pdf(filter: &str, compress: bool) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.5\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Marker 5 0 R >>\nendobj\n");
    // 对象 6：对象流，头部为「对象号 相对偏移」对，随后是对象本体。
    let inner = b"<< /Marker /InObjStm >>";
    let mut stm: Vec<u8> = Vec::new();
    stm.extend_from_slice(b"5 0 "); // 对象 5，相对偏移 0
    stm.extend_from_slice(inner);
    let first = 4usize; // stm 中对象本体的起始偏移
    let payload = if compress { zlib(&stm) } else { stm };
    let off6 = buf.len();
    let filter_hdr = if filter.is_empty() {
        String::new()
    } else {
        format!(" /Filter /{filter}")
    };
    let hdr6 = format!(
        "6 0 obj\n<< /Type /ObjStm /N 1 /First {first}{filter_hdr} /Length {} >>\nstream\n",
        payload.len()
    );
    buf.extend_from_slice(hdr6.as_bytes());
    buf.extend_from_slice(&payload);
    buf.extend_from_slice(b"\nendstream\nendobj\n");
    // 对象 7：XRef 流，/W [1 2 1]，/Index [0 8]（条目覆盖对象 0..=7）。
    let off7 = buf.len();
    let mut xs: Vec<u8> = Vec::new();
    xs.extend_from_slice(&xref_row(0, 0, 255)); // 0：自由条目
    xs.extend_from_slice(&xref_row(1, off1 as u16, 0)); // 1：Catalog
    xs.extend_from_slice(&xref_row(0, 0, 0)); // 2
    xs.extend_from_slice(&xref_row(0, 0, 0)); // 3
    xs.extend_from_slice(&xref_row(0, 0, 0)); // 4
    xs.extend_from_slice(&xref_row(2, 6, 0)); // 5：位于对象流 6 内，索引 0（type 2）
    xs.extend_from_slice(&xref_row(1, off6 as u16, 0)); // 6：对象流本身
    xs.extend_from_slice(&xref_row(1, off7 as u16, 0)); // 7：XRef 流本身
    append_xref_stream(&mut buf, 7, off7, 8, "", &xs);
    buf
}

/// 按 `(码值, 码宽)` 序列做 MSB-first 位打包（测试用 LZW 码流构造）。
fn pack_lzw_codes(codes: &[(u32, u32)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut pending = 0u32;
    for &(code, width) in codes {
        acc = (acc << width) | code;
        pending += width;
        while pending >= 8 {
            pending -= 8;
            out.push((acc >> pending) as u8);
        }
    }
    if pending > 0 {
        out.push(((acc << (8 - pending)) & 0xff) as u8);
    }
    out
}

/// 用「CLEAR + 9 位字面量 + EOD」编码小段数据（不依赖字典）。
///
/// 仅用于测试构造 LZW 压缩流；数据须足够短（码数不足以触发 9→10 位切换），
/// 因此不覆盖码宽增长（由 `filters.rs` 单元测试覆盖）。
fn lzw_encode_literals(data: &[u8]) -> Vec<u8> {
    let mut codes: Vec<(u32, u32)> = vec![(256, 9)];
    codes.extend(data.iter().map(|&b| (b as u32, 9)));
    codes.push((257, 9));
    pack_lzw_codes(&codes)
}

/// 构造「头部声明 `header` + 实际用 XRef 流（对象 3）」的 PDF：
/// 对象 1=Catalog、2=Pages，对象 3=XRef 流 `/W [1 2 1] /Index [0 3]`。
///
/// - `filter`：XRef 流的 `/Filter` 名（空串 = 未压缩）；
/// - `png_up`：为真时按 PNG Up（Predictor 12，Columns=4）预编码条目后再压缩。
fn xref_stream_pdf(header: &str, filter: &str, png_up: bool) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(header.as_bytes());
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    // 原始条目：每行 type(1)+offset(2)+gen(1) = 4 字节
    let mut xs: Vec<u8> = Vec::new();
    xs.extend_from_slice(&xref_row(0, 0, 255)); // 0：自由条目
    xs.extend_from_slice(&xref_row(1, off1 as u16, 0)); // 1：Catalog
    xs.extend_from_slice(&xref_row(1, off2 as u16, 0)); // 2：Pages
    // PNG Up 预编码（按行做 enc[i] = raw[i] - raw_prev[i]）
    let columns = 4usize;
    let encoded = if png_up {
        let mut e = Vec::new();
        for r in 0..(xs.len() / columns) {
            for c in 0..columns {
                let cur = xs[r * columns + c];
                let up = if r == 0 { 0 } else { xs[(r - 1) * columns + c] };
                e.push(cur.wrapping_sub(up));
            }
        }
        e
    } else {
        xs.clone()
    };
    let stream_bytes = match filter {
        "" => encoded,
        "LZWDecode" => lzw_encode_literals(&encoded),
        _ => zlib(&encoded),
    };
    let off_stm = buf.len();
    let filter_hdr = if filter.is_empty() {
        String::new()
    } else {
        format!(" /Filter /{filter}")
    };
    let parms_hdr = if png_up {
        format!(" /DecodeParms << /Predictor 12 /Columns {columns} >>")
    } else {
        String::new()
    };
    append_xref_stream(
        &mut buf,
        3,
        off_stm,
        3,
        &format!("{filter_hdr}{parms_hdr}"),
        &stream_bytes,
    );
    buf
}

/// 断言文档能取出 Catalog（对象 1）与 Pages（对象 2）。
fn assert_catalog_and_pages(doc: &Document) {
    let cat = doc.get_object(1).expect("parse").expect("obj 1 present");
    assert_eq!(
        cat.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Catalog")
    );
    let pages = doc.get_object(2).expect("parse").expect("obj 2 present");
    assert_eq!(
        pages
            .value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Pages")
    );
}

// ── 问题 2：写端内容-版本一致性校验（严出） ──────────────────────

/// 构造含单个指定 `/Filter` 流的文档：对象 1 = Catalog，对象 2 = 该过滤器流。
fn filter_stream_doc(filter: &str) -> Document {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(Dict::new().insert("Type", Value::Name("Catalog".into()))),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Stream(
            Dict::new().insert("Filter", Value::Name(filter.into())),
            StreamData::Owned(b"\x00\x01".to_vec()),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc
}
