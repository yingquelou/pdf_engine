//! 附录 F.4 hint 表读端的集成验证：自己按 F.3 分部装配一个**真正线性化**的
//! 最小文件，把位级手工组装的页偏移 hint 表（F.4.1，Table F.3/F.4）与共享
//! 对象 hint 表（F.4.2，Table F.5/F.6）注入主 hint 流，再从公开 API
//! （`Document::linearization` / `Document::linearization_hints`）解析回
//! 类型化数据。
//!
//! 页偏移表按 F.4 前言恒在流偏移 0、且没有 Table F.2 键，故它的字节长度就是
//! 后面那张表 `/S` 的最小合法位置：本文件把两表按真实字节相邻装配，再用
//! `hints.page_offset_end_byte()` 与 `position_of("S")` 相互印证。
//!
//! 本库写端尚不产出线性化布局（`docs/ROADMAP.md` 的 F5 缺口），`PdfBuilder`
//! 也就无从直接造出 F.3.1 要求的两段交叉引用表形态，因此这里按分部顺序
//! 逐字节写出；所有可变数字一律定宽，两次装配即可让偏移收敛到自洽。

use pdf_engine::core::reader::linearization::LinearizationIssue;
use pdf_engine::core::reader::linearization::hints::SharedObjectHintTable;

use super::*;

/// F.4.3 缩略图 hint 表的集成用例（同目录子模块，复用本文件的位写手与装配器；
/// 本文件已逼近 600 行/25600 字节的上限，故新家族独立成文件）。
mod thumbnail;

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
/// 内容流载荷（不压缩，长度可预测）。
const CONTENT: &[u8] = b"BT /F1 12 Tf 72 720 Td (Annex F hints) Tj ET";
/// 字典 `/N` 与页偏移表的列长（两处须一致：读端按 `/N` 成列）。
const PAGE_COUNT: usize = 2;

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

/// 位级装配 Table F.3 头部（13 项，5×32 + 8×16 = 288 位）+ Table F.4 的
/// `PAGE_COUNT` 个逐页条目，严格按 F.4.1 a)—g) 的项交错序（项 1—3、6—7 各
/// 是「all pages, in page order」的一整列；项 4/5 自第二页起逐页成列）。
///
/// `first_page_refs` 驱动 Table F.4 项 3 的首页 shall（合规值 0）：非零时
/// 项 4/5 的列里**不含**首页元素，原值只保留在类型化条目里。
fn page_offset_table_bits(first_page_refs: u64) -> Vec<u8> {
    let mut w = BitWriter::default();
    // Table F.3：32/32/16/32/16/32/16/32/16/16/16/16/16。
    w.unsigned(3, 32); // 项 1：一页的最少对象数
    w.unsigned(700, 32); // 项 2：首页页对象的位置
    w.unsigned(3, 16); // 项 3：对象数增量的位宽
    w.unsigned(300, 32); // 项 4：一页的最小长度
    w.unsigned(6, 16); // 项 5：页长增量的位宽
    w.unsigned(20, 32); // 项 6：内容流起点偏移的最小值
    w.unsigned(5, 16); // 项 7：内容流起点偏移增量的位宽
    w.unsigned(50, 32); // 项 8：内容流长度的最小值
    w.unsigned(8, 16); // 项 9：内容流长度增量的位宽
    w.unsigned(2, 16); // 项 10：共享对象引用数的位宽
    w.unsigned(4, 16); // 项 11：共享对象标识的位宽
    w.unsigned(3, 16); // 项 12：分数位置分子位宽
    w.unsigned(4, 16); // 项 13：分母（值，非位宽）
    // a) 项 1 列（3 位）。
    w.unsigned(0, 3);
    w.unsigned(1, 3);
    // b) 项 2 列（6 位）。
    w.unsigned(10, 6);
    w.unsigned(25, 6);
    // c) 项 3 列（2 位）：首页 0（shall），第二页 2 个引用。
    w.unsigned(first_page_refs, 2);
    w.unsigned(2, 2);
    // d) 项 4 列（4 位）：第二页的两个共享对象标识。
    w.unsigned(1, 4);
    w.unsigned(5, 4);
    // e) 项 5 列（3 位）：与项 4 同序的分子。
    w.unsigned(2, 3);
    w.unsigned(0, 3);
    // f) 项 6 列（5 位）。
    w.unsigned(4, 5);
    w.unsigned(7, 5);
    // g) 项 7 列（8 位）。
    w.unsigned(11, 8);
    w.unsigned(13, 8);
    w.finish()
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

/// 页偏移表占用的字节数（350 位 → 44 字节）：F.4 前言把本表钉在流偏移 0，
/// 故它后面的第一张表的最小合法位置就是本值。
fn page_offset_bytes() -> usize {
    page_offset_table_bits(0).len()
}

/// Table F.2 键 `S` 的定位条目原文：紧跟页偏移表之后。
fn hint_entries() -> String {
    format!(" /S {}", page_offset_bytes())
}

/// hint 流载荷：偏移 0 的页偏移表 + 紧随其后的共享对象表。
fn hint_payload(first_page_refs: u64) -> Vec<u8> {
    let mut payload = page_offset_table_bits(first_page_refs);
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
fn handbuilt_linearized_file_exposes_page_offset_hint_table() {
    let payload = hint_payload(0);
    let entries = hint_entries();
    let bytes = build(&payload, &entries);
    let document = parse_buf(&bytes);
    let info = document
        .linearization()
        .expect("no I/O error")
        .expect("recognized as linearized");
    assert!(info.is_consistent(), "structure issues: {:?}", info.issues);

    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(hints.is_consistent(), "hint issues: {:?}", hints.issues);
    let table = hints
        .page_offset
        .as_ref()
        .expect("the page offset table sits at stream offset 0");

    // Table F.3 头部 13 项逐项归位；本表没有 Table F.2 键，纯按前言定位。
    assert_eq!(table.declared_pages, PAGE_COUNT as u64);
    assert_eq!(table.header.least_objects_per_page, 3);
    assert_eq!(table.header.first_page_object_location, 700);
    assert_eq!(table.header.object_count_bits, 3);
    assert_eq!(table.header.least_page_length, 300);
    assert_eq!(table.header.page_length_bits, 6);
    assert_eq!(table.header.least_content_offset, 20);
    assert_eq!(table.header.content_offset_bits, 5);
    assert_eq!(table.header.least_content_length, 50);
    assert_eq!(table.header.content_length_bits, 8);
    assert_eq!(table.header.shared_ref_count_bits, 2);
    assert_eq!(table.header.shared_object_id_bits, 4);
    assert_eq!(table.header.shared_position_numerator_bits, 3);
    assert_eq!(table.header.shared_position_denominator, 4);
    assert!(table.width_problems().is_empty());

    // Table F.4 项 1/2/6/7 存的是**增量**：类型化访问器须加回头部最小值。
    assert_eq!(table.pages.len(), PAGE_COUNT);
    assert_eq!(table.pages[0].object_count_delta, 0);
    assert_eq!(table.pages[0].page_length_delta, 10);
    assert_eq!(table.pages[0].content_offset_delta, 4);
    assert_eq!(table.pages[0].content_length_delta, 11);
    assert_eq!(table.pages[0].object_count(&table.header), 3);
    assert_eq!(table.pages[0].page_length(&table.header), 310);
    assert_eq!(table.pages[0].content_offset(&table.header), 24);
    assert_eq!(table.pages[0].content_length(&table.header), 61);
    assert_eq!(table.pages[1].object_count(&table.header), 4);
    assert_eq!(table.pages[1].page_length(&table.header), 325);
    assert_eq!(table.pages[1].content_offset(&table.header), 27);
    assert_eq!(table.pages[1].content_length(&table.header), 63);

    // F.4.1 d)/e)：共享对象两列自第二页起，按页归位、不跨页混排。
    assert_eq!(table.pages[0].shared_reference_count, 0);
    assert!(table.pages[0].shared_object_ids.is_empty());
    assert!(table.pages[0].shared_position_numerators.is_empty());
    assert_eq!(table.pages[1].shared_reference_count, 2);
    assert_eq!(table.pages[1].shared_object_ids, vec![1, 5]);
    assert_eq!(table.pages[1].shared_position_numerators, vec![2, 0]);

    // 表体长度即下一张表的下界：350 位补齐到 44 字节，`/S` 恰好落在其上。
    assert_eq!(table.bits_consumed, 350);
    assert_eq!(table.end_byte(), page_offset_bytes() as u64);
    assert_eq!(hints.page_offset_end_byte(), Some(table.end_byte()));
    assert_eq!(hints.position_of("S"), Some(table.end_byte()));
    assert_eq!(hints.stream_bytes, payload.len());

    // F.4.1 是 F.4.2 的前置：本表解析不挤掉后面的共享对象表。
    assert!(hints.shared_object.is_some());

    // 识别与解码都不改变普通路径：两页可读、零告警。
    assert_eq!(document.page_count().expect("page count"), 2);
    assert!(document.warnings().is_empty());
}

#[test]
fn malformed_page_offset_table_is_a_typed_issue_not_a_panic() {
    let full = hint_payload(0);
    // 截在页偏移表体内（含空表）：一律类型化诊断，页偏移家族整体退回 `None`。
    for prefix in [0usize, 1, 13, 43] {
        let bytes = build(&full[..prefix], " /S 9999");
        let document = parse_buf(&bytes);
        let hints = document
            .linearization_hints()
            .expect("no I/O error")
            .expect("hint stream present");
        assert!(
            hints
                .issues
                .iter()
                .any(|issue| matches!(issue, LinearizationIssue::PageOffsetTableCorrupt { .. })),
            "prefix {prefix}/{} must yield a typed page offset issue: {:#?}",
            full.len(),
            hints.issues
        );
        assert!(hints.page_offset.is_none());
        // 解码失败不影响文档按普通路径读取。
        assert_eq!(document.page_count().expect("page count"), 2);
    }

    // Table F.4 项 3 的首页 shall：非零值原样保留、列不错位读、给类型化诊断。
    let payload = hint_payload(3);
    let entries = hint_entries();
    let bytes = build(&payload, &entries);
    let document = parse_buf(&bytes);
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints
            .issues
            .contains(&LinearizationIssue::PageOffsetFirstPageSharedRefs { count: 3 }),
        "{:#?}",
        hints.issues
    );
    let table = hints.page_offset.as_ref().expect("violated value kept");
    assert_eq!(table.pages[0].shared_reference_count, 3);
    assert!(table.pages[0].shared_object_ids.is_empty());
    assert_eq!(table.pages[1].shared_object_ids, vec![1, 5]);
    // 违规值不影响后续家族的可读性。
    assert!(hints.shared_object.is_some());
}

#[test]
fn handbuilt_linearized_file_exposes_shared_object_hint_table() {
    let payload = hint_payload(0);
    let entries = hint_entries();
    let bytes = build(&payload, &entries);
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
    assert_eq!(hints.position_of("S"), Some(page_offset_bytes() as u64));
    assert!(hints.page_offset.is_some());

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
    let shared_at = page_offset_bytes() as u64;
    assert_eq!(hints.adjust_position(shared_at), shared_at);
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
    // 表位仍在流内（页偏移表之后），但 Table F.5 头部只写出一半。
    let full = hint_payload(0);
    let payload = &full[..page_offset_bytes() + 6];
    let entries = hint_entries();
    let bytes = build(payload, &entries);
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
    // 前一家族的完整字节不受后一表截断的影响。
    assert!(hints.page_offset.is_some());
    assert_eq!(
        hints.page_offset_end_byte(),
        Some(page_offset_bytes() as u64)
    );
    // 解码失败不影响文档按普通路径读取。
    assert_eq!(document.page_count().expect("page count"), 2);
}

#[test]
fn missing_required_shared_object_table_key_is_reported() {
    // Table F.2 的 `S` 是 Required：主 hint 流字典缺它即给类型化诊断。
    let payload = hint_payload(0);
    let bytes = build(&payload, "");
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
    // 页偏移表不靠 Table F.2 键定位（前言钉死在偏移 0），故缺 `S` 也照常解析。
    assert!(hints.page_offset.is_some());
}
