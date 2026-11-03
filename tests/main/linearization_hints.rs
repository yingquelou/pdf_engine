//! 附录 F.4 hint 表读端的集成验证：自己按 F.3 分部装配一个**真正线性化**的
//! 最小文件，把位级手工组装的共享对象 hint 表（F.4.2，Table F.5/F.6）注入
//! 主 hint 流，再从公开 API（`Document::linearization` / `Document::
//! linearization_hints`）解析回类型化数据。
//!
//! 本库写端尚不产出线性化布局（`docs/ROADMAP.md` 的 F5 缺口），`PdfBuilder`
//! 也就无从直接造出 F.3.1 要求的两段交叉引用表形态，因此这里按分部顺序
//! 逐字节写出；所有可变数字一律定宽，两次装配即可让偏移收敛到自洽。

use pdf_engine::core::reader::linearization::LinearizationIssue;
use pdf_engine::core::reader::linearization::hints::SharedObjectHintTable;

use super::*;

/// 第二组对象号（F.3.1：其余对象自 1 起顺序编号）。
const PAGES: i64 = 1;
const PAGE2: i64 = 2;
const CONTENTS2: i64 = 3;
const INFO: i64 = 4;
/// 第一组对象号（正文第一个对象即线性化参数字典，主 hint 流取末尾号）。
const DICT: i64 = 5;
const CATALOGUE: i64 = 6;
const PAGE1: i64 = 7;
const CONTENTS1: i64 = 8;
const FONT: i64 = 9;
const HINT: i64 = 10;
/// 两段表的条目总数（首页表 6 条 + 主表 5 条）。
const SIZE: i64 = 11;
/// 首页表前的占位字节数：F.4.1 规定 page offset hint 表恒在流偏移 0，
/// 本切片不解析它，用零字节占位，共享对象表由字典 `/S 4` 定位。
const PAGE_OFFSET_PLACEHOLDER: usize = 4;
/// 内容流载荷（不压缩，长度可预测）。
const CONTENT: &[u8] = b"BT /F1 12 Tf 72 720 Td (Annex F hints) Tj ET";

/// 高位在前、任意宽度的定宽整数写入器（F.4 前言的位流形态）。
#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    /// 当前字节里已占用的位数（0—7）。
    used: u32,
    /// 当前字节已写入的位（左对齐）。
    pending: u8,
}

impl BitWriter {
    fn unsigned(&mut self, value: u64, bits: u32) {
        for index in (0..bits).rev() {
            let bit = ((value >> index) & 1) as u8;
            self.pending = (self.pending << 1) | bit;
            self.used += 1;
            if self.used == 8 {
                self.bytes.push(self.pending);
                self.pending = 0;
                self.used = 0;
            }
        }
    }

    fn flag(&mut self, set: bool) {
        self.unsigned(u64::from(set), 1);
    }

    /// 收尾：不足一字节的尾巴按 F.4 的位流形态零填充（读端只看前面的位）。
    fn finish(mut self) -> Vec<u8> {
        if self.used > 0 {
            self.bytes.push(self.pending << (8 - self.used));
        }
        self.bytes
    }
}

/// 位级装配 Table F.5 头部 + Table F.6 组条目（首页 2 组，首组带 128 位
/// MD5 签名；共享区 1 组）。
fn shared_object_table_bits() -> Vec<u8> {
    let mut w = BitWriter::default();
    // Table F.5：32/32/32/32/16/32/16。
    w.unsigned(20, 32); // 项 1：共享对象区首对象号
    w.unsigned(5000, 32); // 项 2：共享对象区首对象位置
    w.unsigned(2, 32); // 项 3：首页条目数
    w.unsigned(3, 32); // 项 4：总条目数（含项 3）
    w.unsigned(8, 16); // 项 5：组对象数位宽
    w.unsigned(100, 32); // 项 6：组最小长度
    w.unsigned(12, 16); // 项 7：组长度位宽
    // F.4.2：交错按序列各自进行（原文「The order of items in each
    // sequence shall be as follows」a)—d)）。
    // 序列一（首页 2 组）：项 1 → 项 2 → 项 3 → 项 4。
    w.unsigned(5, 12);
    w.unsigned(7, 12);
    w.flag(true);
    w.flag(false);
    for byte in 0x00u8..=0x0F {
        w.unsigned(byte as u64, 8);
    }
    w.unsigned(3, 8);
    w.unsigned(0, 8);
    // 序列二（共享区 1 组）：无签名。
    w.unsigned(9, 12);
    w.flag(false);
    w.unsigned(1, 8);
    w.finish()
}

/// hint 流载荷：page offset 表占位 + 共享对象 hint 表位流。
fn hint_payload() -> Vec<u8> {
    let mut payload = vec![0u8; PAGE_OFFSET_PLACEHOLDER];
    payload.extend(shared_object_table_bits());
    payload
}

/// 一轮装配所需的布局猜测；定宽字段保证两次装配后收敛。
#[derive(Clone, Copy, Default, PartialEq)]
struct Guess {
    l: u64,
    hint_off: u64,
    hint_len: u64,
    e: u64,
    t: u64,
    prev: u64,
    first_xref: u64,
    cat: u64,
    page: u64,
    contents: u64,
    font: u64,
    pages: u64,
    page2: u64,
    contents2: u64,
    info: u64,
}

fn push(buf: &mut Vec<u8>, text: &str) {
    buf.extend_from_slice(text.as_bytes());
}

fn content_stream(number: i64) -> String {
    let body = std::str::from_utf8(CONTENT).expect("ascii content");
    format!(
        "{number} 0 obj\n<< /Length {} >>\nstream\n{body}\nendstream\nendobj\n",
        CONTENT.len()
    )
}

/// 按 F.3.1 的分部顺序写一轮，返回字节与实际布局。
fn assemble(payload: &[u8], hint_entries: &str, g: Guess) -> (Vec<u8>, Guess) {
    let mut buf = Vec::new();
    // part 1：头部（F.3.2）。
    push(&mut buf, "%PDF-1.5\n");
    // part 2：线性化参数字典（F.3.3；Table F.1 条目）。
    let dict_at = buf.len() as u64;
    push(
        &mut buf,
        &format!(
            "{DICT} 0 obj\n<< /Linearized 1.0 /L {:>10} /H [{:>10} {:>10}] /O {PAGE1} /E {:>10} /N 2 /T {:>10} >>\nendobj\n",
            g.l, g.hint_off, g.hint_len, g.e, g.t
        ),
    );
    // part 3：首页交叉引用表 + trailer（F.3.4：字典在表首、主 hint 流在表尾）。
    let first_xref = buf.len() as u64;
    push(&mut buf, &format!("xref\n{DICT} 6\n"));
    for offset in [dict_at, g.cat, g.page, g.contents, g.font, g.hint_off] {
        push(&mut buf, &format!("{offset:010} 00000 n \n"));
    }
    push(
        &mut buf,
        &format!(
            "trailer\n<< /Size {SIZE} /Prev {:>10} /Root {CATALOGUE} 0 R >>\nstartxref\n0\n%%EOF\n",
            g.prev
        ),
    );
    // part 5：主 hint 流（F.3.6；Table F.2 键位由 hint_entries 注入）。
    let hint_at = buf.len() as u64;
    push(
        &mut buf,
        &format!(
            "{HINT} 0 obj\n<< /Length {:>10}{hint_entries} >>\nstream\n",
            payload.len()
        ),
    );
    buf.extend_from_slice(payload);
    push(&mut buf, "\nendstream\nendobj\n");
    let hint_len = buf.len() as u64 - hint_at;
    // part 6：首页对象组（F.3.5/F.3.7：页对象自带 MediaBox 与 Resources）。
    let cat_at = buf.len() as u64;
    push(
        &mut buf,
        &format!("{CATALOGUE} 0 obj\n<< /Type /Catalog /Pages {PAGES} 0 R >>\nendobj\n"),
    );
    let page_at = buf.len() as u64;
    push(
        &mut buf,
        &format!(
            "{PAGE1} 0 obj\n<< /Type /Page /Parent {PAGES} 0 R /MediaBox [0 0 612 792] /Contents {CONTENTS1} 0 R /Resources << /Font << /F1 {FONT} 0 R >> >> >>\nendobj\n"
        ),
    );
    let contents_at = buf.len() as u64;
    push(&mut buf, &content_stream(CONTENTS1));
    let font_at = buf.len() as u64;
    push(
        &mut buf,
        &format!("{FONT} 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n"),
    );
    // part 7：其余对象的第二组。
    let pages_at = buf.len() as u64;
    push(
        &mut buf,
        &format!(
            "{PAGES} 0 obj\n<< /Type /Pages /Kids [{PAGE1} 0 R {PAGE2} 0 R] /Count 2 >>\nendobj\n"
        ),
    );
    let page2_at = buf.len() as u64;
    push(
        &mut buf,
        &format!(
            "{PAGE2} 0 obj\n<< /Type /Page /Parent {PAGES} 0 R /MediaBox [0 0 612 792] /Contents {CONTENTS2} 0 R /Resources << /Font << /F1 {FONT} 0 R >> >> >>\nendobj\n"
        ),
    );
    let contents2_at = buf.len() as u64;
    push(&mut buf, &content_stream(CONTENTS2));
    let info_at = buf.len() as u64;
    push(
        &mut buf,
        &format!("{INFO} 0 obj\n<< /Producer (pdf_engine Annex F integration) >>\nendobj\n"),
    );
    // part 9/10：主交叉引用表与其 trailer（F.3.11：主 trailer 只有 Size）。
    let main_at = buf.len() as u64;
    push(&mut buf, "xref\n0 5\n");
    // Table F.1 `/T`：首条目**之前**的那个空白字符。
    let t_val = buf.len() as u64;
    push(&mut buf, "\n0000000000 65535 f \n");
    for offset in [pages_at, page2_at, contents2_at, info_at] {
        push(&mut buf, &format!("{offset:010} 00000 n \n"));
    }
    push(&mut buf, &format!("trailer\n<< /Size {SIZE} >>\n"));
    // 文件尾：F.3.11「The startxref line shall give the offset of the
    // first-page cross-reference table」。
    push(&mut buf, &format!("startxref\n{first_xref}\n%%EOF\n"));
    let measured = Guess {
        l: buf.len() as u64,
        hint_off: hint_at,
        hint_len,
        e: pages_at,
        t: t_val,
        prev: main_at,
        first_xref,
        cat: cat_at,
        page: page_at,
        contents: contents_at,
        font: font_at,
        pages: pages_at,
        page2: page2_at,
        contents2: contents2_at,
        info: info_at,
    };
    (buf, measured)
}

/// 定点迭代到布局自洽（定宽字段保证收敛）。
fn build(payload: &[u8], hint_entries: &str) -> Vec<u8> {
    let mut guess = Guess::default();
    for _ in 0..8 {
        let (bytes, measured) = assemble(payload, hint_entries, guess);
        if measured == guess {
            return bytes;
        }
        guess = measured;
    }
    panic!("linearized layout did not converge");
}

#[test]
fn handbuilt_linearized_file_exposes_shared_object_hint_table() {
    let payload = hint_payload();
    let bytes = build(&payload, " /S 4");
    let document = parse_buf(&bytes);
    let info = document
        .linearization()
        .expect("no I/O error")
        .expect("recognized as linearized");
    assert!(info.is_consistent(), "structure issues: {:?}", info.issues);
    assert!(!info.treated_as_ordinary());

    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(hints.is_consistent(), "hint issues: {:?}", hints.issues);
    assert_eq!(hints.stream_bytes, payload.len());
    assert_eq!(hints.position_of("S"), Some(4));

    let table = hints.shared_object.as_ref().expect("parsed table");
    assert_eq!(table.header.first_object_number, 20);
    assert_eq!(table.header.first_object_location, 5000);
    assert_eq!(table.header.first_page_entries, 2);
    assert_eq!(table.header.total_entries, 3);
    assert_eq!(table.header.group_object_bits, 8);
    assert_eq!(table.header.least_group_length, 100);
    assert_eq!(table.header.group_length_bits, 12);
    assert_eq!(table.first_page_groups.len(), 2);
    assert_eq!(table.first_page_groups[0].length_delta, 5);
    assert_eq!(table.first_page_groups[0].object_count_minus_one, 3);
    let signature: Vec<u8> = (0x00u8..=0x0F).collect();
    assert_eq!(
        table.first_page_groups[0].signature,
        signature.as_slice().try_into().ok()
    );
    assert_eq!(table.first_page_groups[1].length_delta, 7);
    assert_eq!(table.first_page_groups[1].signature, None);
    assert_eq!(table.shared_section_groups.len(), 1);
    assert_eq!(table.shared_section_groups[0].length_delta, 9);
    assert_eq!(table.shared_section_groups[0].object_count_minus_one, 1);

    // F.4 前言 NOTE 1/NOTE 3 的分工：Table F.2 的 `/S` 是**流内**位置，
    // 原样使用；表内的对象位置（大于主流起点者）须补上主流长度。
    assert_eq!(hints.adjust_position(4), 4);
    assert_eq!(
        hints.adjust_position(hints.primary.offset),
        hints.primary.offset
    );
    let after_stream = hints.primary.offset + 1;
    assert_eq!(
        hints.adjust_position(after_stream),
        after_stream + hints.primary.length
    );
    assert_eq!(
        hints.adjust_position(table.header.first_object_location),
        table.header.first_object_location + hints.primary.length,
        "表内位置 5000 大于主流起点，须按 NOTE 3 补上主流长度"
    );

    // 识别与解码都不改变普通路径：两页可读、零告警。
    assert_eq!(document.page_count().expect("page count"), 2);
    assert!(document.warnings().is_empty());
}

#[test]
fn truncated_hint_bytes_are_a_typed_issue_not_a_panic() {
    // 表位仍在流内（偏移 4，流长 10），但 Table F.5 头部只写出一半。
    let full = hint_payload();
    let payload = &full[..PAGE_OFFSET_PLACEHOLDER + 6];
    let bytes = build(payload, " /S 4");
    let document = parse_buf(&bytes);
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::HintTableCorrupt { key, .. } if key == "S"
        )),
        "{:#?}",
        hints.issues
    );
    assert!(hints.shared_object.is_none());
    // 解码失败不影响文档按普通路径读取。
    assert_eq!(document.page_count().expect("page count"), 2);
}

#[test]
fn missing_required_shared_object_table_key_is_reported() {
    // Table F.2 的 `S` 是 Required：主 hint 流字典缺它即给类型化诊断。
    let bytes = build(&hint_payload(), "");
    let document = parse_buf(&bytes);
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints
            .issues
            .contains(&LinearizationIssue::SharedHintTableMissing),
        "{:#?}",
        hints.issues
    );
    assert!(hints.table_positions.is_empty());
    assert_eq!(hints.shared_object, None::<SharedObjectHintTable>);
}
