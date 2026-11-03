//! 附录 F 线性化读端测试：自足夹具与分组用例。
//!
//! 夹具严格按 F.3.1 的 11 个分部顺序装配（头部 → 线性化字典 → 首页
//! 交叉引用表 → hint 流 → 首页对象组 → 其余对象组 → 主交叉引用表 →
//! 文件尾 startxref），并按 F.3.4 的条目序（字典在表首、主 hint 流在表尾）
//! 与 Table F.1 的 `/T`「首条目之前的空白字符」措辞取值，因此默认形态
//! **零诊断**；每个负例只开一个旋钮，断言命中该规则。
//!
//! 夹具是**语法层字节串**，故用文件偏移引用对象（语法层无需对象流）。
//! 对象流的禁用规则（F.3.1「may not be contained in an object stream」）
//! 按 xref 条目种类判定，测试在解析后改写 pub 字段驱动，不改夹具形态。
//!
//! 文件分工：本模块放对象号常量、旋钮 [`Knobs`]、布局 [`Guess`]、收敛驱动
//! [`build`] 与打开/断言辅助；单轮装配在 [`fixture`]，分部写手在 [`parts`]。

use std::io::Write;
use std::process::id;
use std::time::{SystemTime, UNIX_EPOCH};

use self::fixture::build_once;
use super::{LinearizationInfo, LinearizationIssue};
use crate::core::reader::Parser;
use crate::core::types::Document;

mod conform;
mod fixture;
mod hints;
mod parts;
mod structure;
mod typing;

// ── 对象号（F.3.1 两对象组；F.3.6 hint 流取末尾号）─────────────────────

/// 第二组：其余页的对象与文档级对象，自 1 起顺序编号。
const PAGES: i64 = 1;
const PAGE2: i64 = 2;
const CONTENTS2: i64 = 3;
const INFO: i64 = 4;
/// 第一组：线性化字典（正文第一个对象）+ 目录 + 首页对象组。
const DICT: i64 = 5;
const CATALOGUE: i64 = 6;
const PAGE1: i64 = 7;
const CONTENTS1: i64 = 8;
const FONT: i64 = 9;
/// 主 hint 流取最后一个对象号（F.3.6）。
const HINT: i64 = 10;
/// 主交叉引用流的对象号（仅 `main_is_stream` 变体使用）。
const MAIN_STREAM: i64 = 11;

/// 文档页数真值（Table F.1 `/N`）。
const PAGE_TOTAL: i64 = 2;
/// `/Length` 为间接引用的对象号（F.3.3 反例与 hint 开销计算用）。
const INDIRECT_LENGTH: i64 = 12;

/// 主 hint 流的缺省载荷（`linearization()` 不解码它；hint 表用例经
/// `hint_payload` 旋钮换成位级手工装配的表字节）。
const HINT_PAYLOAD: &[u8] = b"hint-table-bytes-never-decoded";
/// 内容流载荷（未压缩，便于夹具长度可预测）。
const CONTENT_PAYLOAD: &[u8] = b"BT /F1 12 Tf 72 720 Td (Annex F) Tj ET";

/// 夹具旋钮：一次只破坏一条 shall，其余形态保持合规。
#[derive(Debug, Clone)]
struct Knobs<'a> {
    /// 头部声明版本（默认 1.4；`1.0` 驱动 F.3.2 的版本门禁）。
    version: &'static str,
    /// 头部行之后插入的注释填充字节数（驱动 F.3.3 的 1024 字节上限）。
    padding: usize,
    /// 正文第一个对象改为对象 1（驱动 F.3.3「first object in the body」）。
    decoy_first: bool,
    /// 线性化字典对象的代号（默认 0；非 0 驱动 F.3.1）。
    dict_generation: u32,
    /// 字典 xref 条目的代号（默认 0；非 0 驱动 F.3.1）。
    dict_entry_generation: u32,
    /// 字典的首页 xref 条目写成 free（驱动 F.3.3/F.3.4）。
    dict_entry_free: bool,
    /// 首页表内字体条目写成 free（驱动 F.3.4「no free entries」）。
    first_page_free_entry: bool,
    /// 首页表起始号前移到对象 4（字典条目不在表首，驱动 F.3.4）。
    dict_entry_not_first: bool,
    /// 交换首页表最后两个条目（主 hint 流条目不在表尾，驱动 F.3.4）。
    swap_hint_entry: bool,
    /// 主表多索引一个对象号（两组对象号交错，驱动 F.3.1）。
    main_extra_entry: bool,
    /// 主表首条目写成 in-use（驱动 F.3.11「first entry shall be free」）。
    main_first_entry_used: bool,
    /// 主 trailer 多出 `/Root`（驱动 F.3.11「no entries other than Size」）。
    main_trailer_extras: bool,
    /// 主交叉引用改为 XRef 流（Table F.1 的 `/T` 第二种形态）。
    main_is_stream: bool,
    /// 只写一份 xref 表（驱动 F.3.1「exactly two sections」）。
    single_table: bool,
    /// `/H` 指向字典对象而非流（驱动 F.3.6）。
    bad_hint_target: bool,
    /// `/H` 写成两个整数对（Table F.1 的溢出 hint 流形态）。
    overflow_hint: bool,
    /// `/T` 指向 `xref` 关键字（既非空白也非数字，驱动 Table F.1）。
    bad_main_target: bool,
    /// 目录多出指向字典的引用（驱动 F.3.3「no references」）。
    catalogue_refs_dict: bool,
    /// 首页 trailer 多出指向字典的引用（驱动 F.3.3）。
    first_page_trailer_refs_dict: bool,
    /// 首页页对象多出指向字典的引用（驱动 F.3.3）。
    first_page_refs_dict: bool,
    /// `/N` 比页面树实际页数多 1（驱动 Table F.1）。
    wrong_page_count: bool,
    /// 显式写出 `/P` 的值（缺省时条目不出现，默认 0）。
    first_page_number: Option<i64>,
    /// 首页页对象缺少的必需属性名（驱动 F.3.7 的不许继承）。
    missing_page_attribute: &'static str,
    /// 从字典中删除的条目名（驱动 F.3.3 的 Required）。
    omit: &'a [&'a str],
    /// 条目的字面量替换（类型化与越界诊断用）。
    replaced: &'a [(&'a str, &'a str)],
    /// 主 hint 流的载荷字节（缺省用 [`HINT_PAYLOAD`]；hint 表用例换成
    /// 位级手工装配的表字节）。
    hint_payload: Option<&'a [u8]>,
    /// 追加进主 hint 流字典的条目原文（如 `" /S 4"`，Table F.2 键位）。
    hint_entries: &'a str,
}

/// 默认形态即「附录 F 严格合规」的线性化文件：版本取一个 1.2 之后的常见值
/// （F.3.2 的门禁是 1.1，F.1 的可选特性自 1.2 起），其余旋钮全部关闭。
impl Default for Knobs<'_> {
    fn default() -> Self {
        Self {
            version: "1.4",
            padding: 0,
            decoy_first: false,
            dict_generation: 0,
            dict_entry_generation: 0,
            dict_entry_free: false,
            first_page_free_entry: false,
            dict_entry_not_first: false,
            swap_hint_entry: false,
            main_extra_entry: false,
            main_first_entry_used: false,
            main_trailer_extras: false,
            main_is_stream: false,
            single_table: false,
            bad_hint_target: false,
            overflow_hint: false,
            bad_main_target: false,
            catalogue_refs_dict: false,
            first_page_trailer_refs_dict: false,
            first_page_refs_dict: false,
            wrong_page_count: false,
            first_page_number: None,
            missing_page_attribute: "",
            omit: &[],
            replaced: &[],
            hint_payload: None,
            hint_entries: "",
        }
    }
}

/// 布局猜测/测量值：字典自身字节数影响其后所有偏移，故定点迭代收敛。
#[derive(Debug, Clone, Copy, PartialEq)]
struct Guess {
    /// `/L`：文件总长。
    l: u64,
    /// `/H[0]`：主 hint 流对象起始偏移。
    hint_off: u64,
    /// `/H[1]`：主 hint 流长度（含流对象开销）。
    hint_len: u64,
    /// `/E`：首页对象组末尾偏移。
    e: u64,
    /// `/T`：主交叉引用定位偏移。
    t: u64,
    /// `/Prev`：首页 trailer 指向的主表偏移。
    prev: u64,
    /// 文件尾 `startxref` 值（首页交叉引用表偏移）。
    first_xref: u64,
    /// 目录、首页页对象、首页内容流、字体对象的偏移（首页表条目用）。
    cat: u64,
    page: u64,
    contents: u64,
    font: u64,
    /// `/Length` 间接引用对象的偏移（首页表条目用）。
    length_obj: u64,
}

impl Default for Guess {
    fn default() -> Self {
        Self {
            l: 1400,
            hint_off: 700,
            hint_len: 100,
            e: 1100,
            t: 1250,
            prev: 1240,
            first_xref: 300,
            cat: 500,
            page: 560,
            contents: 620,
            font: 680,
            length_obj: 0,
        }
    }
}

/// 按旋钮构造附录 F 夹具字节串（收敛后的自洽布局）。
fn build(knobs: &Knobs) -> Vec<u8> {
    let mut guess = Guess {
        length_obj: if knobs.indirect_length() { 1160 } else { 0 },
        ..Guess::default()
    };
    for _ in 0..24 {
        let (bytes, measured) = build_once(knobs, guess);
        if measured == guess {
            return bytes;
        }
        guess = measured;
    }
    panic!("附录 F 夹具未能在 24 轮内收敛");
}

impl Knobs<'_> {
    /// `/Length` 是否写成间接引用（F.3.3「all values … direct objects」反例）。
    fn indirect_length(&self) -> bool {
        self.replaced.iter().any(|(k, _)| *k == "LengthRef")
    }
}

// ── 打开与断言辅助 ──────────────────────────────────────────────────────

/// 生成唯一临时文件路径（沿用 target/test-tmp/：沙箱禁止写系统 %TEMP%）。
fn temp_path(name: &str) -> std::path::PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|path| path.ancestors().nth(3).map(std::path::PathBuf::from))
        .filter(|path| path.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!(
        "demo-pdf-{}-{}-{}.pdf",
        id(),
        name,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ))
}

/// 写出夹具并解析成 `Document`（识别线性化要求真实文件源）。
fn open(bytes: &[u8], name: &str) -> Document {
    let path = temp_path(name);
    {
        let mut file = std::fs::File::create(&path).expect("create fixture");
        file.write_all(bytes).expect("write fixture");
        file.flush().expect("flush fixture");
    }
    let file = std::fs::File::open(&path).expect("open fixture");
    Parser::new(file)
        .expect("parser")
        .parse_document()
        .expect("parse fixture")
}

/// 按旋钮构造夹具并返回其线性化模型（未识别则 panic）。
fn info_of(knobs: &Knobs, name: &str) -> LinearizationInfo {
    let bytes = build(knobs);
    let document = open(&bytes, name);
    document
        .linearization()
        .expect("linearization must not hit I/O error")
        .unwrap_or_else(|| panic!("fixture must be recognized as linearized"))
}

/// 断言命中指定诊断（一个形态可能连带触发其他规则，故逐条命中判定）。
fn expect_issue(info: &LinearizationInfo, expected: &LinearizationIssue) {
    assert!(
        info.issues.contains(expected),
        "expected {expected:?}, got {:?}",
        info.issues
    );
}

/// 断言诊断集合恰为给定内容（用于只破坏一条规则的形态）。
fn expect_issues(info: &LinearizationInfo, expected: &[LinearizationIssue]) {
    assert_eq!(info.issues, expected, "issue set mismatch");
}

/// 定位字节串中某个间接对象的绝对偏移。
fn object_offset(bytes: &[u8], header: &str) -> u64 {
    let haystack = format!("\n{header} obj");
    let position = bytes
        .windows(haystack.len())
        .position(|window| window == haystack.as_bytes())
        .unwrap_or_else(|| panic!("object {header} not found in fixture"));
    (position + 1) as u64
}
