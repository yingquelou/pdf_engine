//! 附录 F hint 表集成用例的共享脚手架：位写手、Table F.3/F.5 的位级装配、
//! 两轮收敛的分部布局迭代（F.3.1）与内容流/字典拼装。
//! 由 `mod.rs` 再导出给同目录的各家族测试模块与 [`thumbnail`] 复用。

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
pub(super) const PAGE_COUNT: usize = 2;

/// 高位在前、任意宽度的定宽整数写入器（F.4 前言的位流形态）。
#[derive(Default)]
pub(super) struct BitWriter {
    bytes: Vec<u8>,
    /// 当前字节里已占用的位数（0—7）。
    used: u32,
    /// 当前字节已写入的位（左对齐）。
    pending: u8,
}

impl BitWriter {
    pub(super) fn unsigned(&mut self, value: u64, bits: u32) {
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
    pub(super) fn finish(mut self) -> Vec<u8> {
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
pub(super) fn page_offset_table_bits(first_page_refs: u64) -> Vec<u8> {
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
pub(super) fn shared_object_table_bits() -> Vec<u8> {
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
pub(super) fn page_offset_bytes() -> usize {
    page_offset_table_bits(0).len()
}

/// Table F.2 键 `S` 的定位条目原文：紧跟页偏移表之后。
pub(super) fn hint_entries() -> String {
    format!(" /S {}", page_offset_bytes())
}

/// hint 流载荷：偏移 0 的页偏移表 + 紧随其后的共享对象表。
pub(super) fn hint_payload(first_page_refs: u64) -> Vec<u8> {
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

/// 按 F.3.1 的分部顺序写一轮，返回字节与实际布局。`version` 写进文件头
/// （F.3.2 的版本声明）：F.4.5 的版本门用例需要 1.4 的头，其余家族沿用 1.5。
fn assemble(version: &str, payload: &[u8], hint_entries: &str, g: Guess) -> (Vec<u8>, Guess) {
    let mut buf = Vec::new();
    // part 1：头部（F.3.2）。
    push(&mut buf, &format!("%PDF-{version}\n"));
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
pub(super) fn build(payload: &[u8], hint_entries: &str) -> Vec<u8> {
    build_at_version("1.5", payload, hint_entries)
}

/// 同 [`build`]，但文件头声明给定的 PDF 版本（F.4.5 版本门用例需要 1.4）。
pub(super) fn build_at_version(version: &str, payload: &[u8], hint_entries: &str) -> Vec<u8> {
    let mut guess = Guess::default();
    for _ in 0..8 {
        let (bytes, measured) = assemble(version, payload, hint_entries, guess);
        if measured == guess {
            return bytes;
        }
        guess = measured;
    }
    panic!("linearized layout did not converge");
}
