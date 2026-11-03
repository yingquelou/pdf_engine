//! 附录 F 线性化 PDF 的读端识别与线性化参数字典类型化（只读端、只语法层）。
//!
//! 覆盖 ISO 32000-1 附录 F 中**文件结构识别**所需的部分；原文关键 shall 摘录：
//!
//! - F.1：「Linearization of PDF is an optional feature available beginning in
//!   PDF 1.2 … A conforming reader that does not support this optional feature
//!   can still successfully process linearized files although not as
//!   efficiently」——本模块只做识别与诊断，不改变既有普通路径读取；
//! - F.3.2：「The Linearized PDF file shall begin with the standard header line
//!   … Linearization is independent of PDF version number and may be applied to
//!   any PDF file of version 1.1 or greater」；
//! - F.3.3：「Following the header, the first object in the body of the file
//!   (part 2) shall be an indirect dictionary object, the linearization
//!   parameter dictionary, which shall contain the parameters listed in Table
//!   F.1. All values in this dictionary shall be direct objects. There shall be
//!   no references to this dictionary anywhere in the document; however, the
//!   first-page cross-reference table (part 3) shall contain a normal entry for
//!   it. The linearization parameter dictionary shall be entirely contained
//!   within the first 1024 bytes of the PDF file」；
//! - Table F.1：`Linearized`（number，Required）、`L`（integer，Required，
//!   「shall be exactly equal to the actual length of the PDF file. A mismatch
//!   indicates that the file is not linearized and shall be treated as ordinary
//!   PDF, ignoring linearization information」）、`H`（array，Required，
//!   「An array of two or four integers」；`offset1` 是**流对象**起始而非流数据
//!   起始，`length1` **含流对象开销**）、`O`/`E`/`N`/`T`（integer，Required）、
//!   `P`（integer，Optional，「Default value: 0」）。字典里**没有** `/V`，也
//!   **没有** hint 流对象号条目（`V` 属 Table F.2 的 hint 表键）；
//! - F.3.1：两对象组（第一组为目录、文档级对象与首页全部对象，号「starting at
//!   the first object number after the last number of the second group」；第二组
//!   其余对象自 1 起顺序编号）、「These groups of objects shall be indexed by
//!   exactly two cross-reference table sections」、「All objects shall have a
//!   generation number of 0」，以及 PDF 1.5 起「These additional objects may not
//!   be contained in an object stream: the linearization dictionary, the
//!   document catalogue, and page objects」；
//! - F.3.4：首页 xref 表「shall contain entries for the linearization parameter
//!   dictionary (at the beginning) and the primary hint stream (at the end)」，
//!   且「shall consist of a single cross-reference subsection that has no free
//!   entries」；其 trailer 的 `Prev` 给出主 xref 表偏移，并须含有效 `Size` 与
//!   `Root`；表后可选 `startxref`/整数/`%%EOF`，「This information shall be
//!   ignored」；
//! - F.3.6：hint 表「shall be enclosed within a stream object」，且不被任何对象
//!   引用；主 hint 流必有，溢出 hint 流可选；
//! - F.3.7：首页 page 对象「shall explicitly specify all required attributes,
//!   such as Resources and MediaBox; the attributes may not be inherited from
//!   ancestor page tree nodes」；
//! - F.3.11：主 xref 表「consists of a single cross-reference subsection,
//!   beginning at object number 0」，首条目须为 free 条目，其余在用且自 1 起连续
//!   编号；「The startxref line shall give the offset of the first-page
//!   cross-reference table. The Prev entry of the first-page trailer shall give
//!   the offset of the main cross-reference table. The main trailer has no Prev
//!   entry and shall not contain any entries other than Size」。
//!
//! **范围**：只做「是否线性化」的识别、Table F.1 条目的类型化，以及上述 shall
//! 的一致性诊断。F.4 hint 表的**二进制解码**与写端线性化输出**不在本模块**
//! （见 `docs/ROADMAP.md` F5 剩余缺口）。
//!
//! **尚未落成诊断的 shall**（本模块判形态与可读范围，不判以下**值**与**排列**）：
//! F.3.4 首页 trailer 的 `Size`「shall be the combined number of entries in both」
//! 两表条目数之和（只判其存在与整数形态）；F.3.6 hint 流「there shall be no
//! references to the stream anywhere in the document」（只判 `H` 目标是流对象
//! 且范围不越界）；F.3.11 主表「the remaining entries … numbered consecutively,
//! starting at 1」（只判首条目是 free 条目）与首页 trailer `Prev` 的**值**同主表
//! 实际偏移的互校；以及 F.3.5/F.3.7–F.3.10 规定的段内对象类别与排列。
//!
//! 诊断只进入 [`LinearizationInfo::issues`]，不写入 `Document::warnings`——
//! 非线性化告警通道，保持既有「线性化文件按普通路径读取且无告警」的行为不变。

use std::fmt;

use crate::core::types::{Dict, Document, PdfVersion, Value};

use super::DEFAULT_WINDOW;
use super::lexer::Lexer;
use super::object::parse_indirect_object;
use super::{Result, err};

mod checks;
mod dict;
#[cfg(test)]
mod tests;

pub use dict::{HintRange, HintStreams};

/// F.3.3：判定线性化所需读取的上限字节数。
pub(crate) const FIRST_BYTES_LIMIT: u64 = 1024;
/// 识别窗口的回退档：字典超出前 1024 字节时仍按「宽进」识别并给出诊断。
const HEAD_SCAN_RETRY_WINDOW: usize = 4096;
/// F.3.3：主 xref 表定位偏移与表尾 trailer 的有界读取窗口。
pub(crate) const MAIN_TABLE_WINDOW: usize = DEFAULT_WINDOW;

/// 线性化参数字典的类型化结果与附录 F 一致性诊断。
///
/// 条目按 Table F.1 类型化；类型不符、缺失必填项或跨结构违规都以
/// [`LinearizationIssue`] 呈现，字段值取 `None` 表示该条目不可用。
#[derive(Debug, Clone, PartialEq)]
pub struct LinearizationInfo {
    /// 字典对象号（F.3.3：应为文件正文的第一个间接对象）。
    pub object_number: i64,
    /// 字典对象的代号；F.3.1 要求所有对象代号为 0。
    pub generation: i64,
    /// 字典对象 `num gen obj` 的绝对起始偏移。
    pub offset: u64,
    /// 字典对象解析结束处的绝对偏移（用于 1024 字节判定）。
    pub end_offset: u64,
    /// 文件正文第一个间接对象的对象号（F.3.3 判定用）。
    pub first_body_object: i64,
    /// 实际文件长度；元数据不可读时为 `None`，此时跳过长度相关检查。
    pub file_size: Option<u64>,
    /// `/Linearized`：线性化格式的版本标识（Table F.1，number）。
    pub version: Option<f64>,
    /// `/L`：整个文件的长度，须与实际长度精确相等（Table F.1）。
    pub declared_file_length: Option<u64>,
    /// `/H`：主 hint 流与可选溢出 hint 流的 `(偏移, 长度)`。
    pub hint_streams: Option<HintStreams>,
    /// `/O`：首页 page 对象的对象号。
    pub first_page_object: Option<i64>,
    /// `/E`：首页末尾相对文件开头的偏移。
    pub first_page_end: Option<u64>,
    /// `/N`：文档页数。
    pub page_count: Option<i64>,
    /// `/T`：主引用表定位偏移——传统表为首条目之前的那个空白字符偏移，
    /// 纯交叉引用流文档为主交叉引用流对象的偏移（Table F.1）。
    pub main_xref_offset: Option<u64>,
    /// `/P`：首页的页码（Optional，Default value: 0）。
    pub first_page_number: i64,
    /// 附录 F 一致性诊断；为空表示结构完全符合规范的 shall 集合。
    pub issues: Vec<LinearizationIssue>,
    /// 原样保留的字典，便于读取 Table F.1 之外的扩展条目。
    pub dict: Dict,
}

impl LinearizationInfo {
    /// 结构是否与附录 F 的 shall 集合一致（无任何诊断）。
    pub fn is_consistent(&self) -> bool {
        self.issues.is_empty()
    }

    /// Table F.1 `/L`：长度不符时「shall be treated as ordinary PDF, ignoring
    /// linearization information」。`/L` 缺失或类型错误同样不可信。
    pub fn treated_as_ordinary(&self) -> bool {
        match (self.declared_file_length, self.file_size) {
            (Some(declared), Some(actual)) => declared != actual,
            (None, _) => true,
            (Some(_), None) => false,
        }
    }
}

/// 附录 F 的线性化一致性诊断（每条对应一个可判定的 shall）。
#[derive(Debug, Clone, PartialEq)]
pub enum LinearizationIssue {
    /// F.3.3：字典须含 Table F.1 标注 Required 的全部条目。
    MissingRequired(&'static str),
    /// Table F.1：条目类型不符。
    WrongType {
        key: &'static str,
        expected: &'static str,
        found: String,
    },
    /// F.3.3：「All values in this dictionary shall be direct objects」。
    IndirectValue(String),
    /// Table F.1：`H` 须为两个或四个整数。
    HintArrayLength { len: usize },
    /// Table F.1：`L` 与实际文件长度不符。
    FileLengthMismatch { declared: u64, actual: u64 },
    /// F.3.3：字典须完全落在文件前 1024 字节内。
    DictionaryBeyondFirst1024 { end_offset: u64, limit: u64 },
    /// F.3.3：文件正文第一个对象须是线性化参数字典。
    NotFirstBodyObject { object_number: i64 },
    /// F.3.3：文档中不得存在对该字典的引用。
    ReferencedFrom { location: &'static str, key: String },
    /// F.3.3/F.3.4：首页 xref 表须含字典的普通（normal）条目。
    NotNormalEntry {
        role: &'static str,
        object_number: i64,
    },
    /// F.3.4：首页 xref 表须是单段且无 free 条目。
    FirstPageTableFreeEntries { count: usize },
    /// F.3.4：首页 trailer 须含有效的 `Prev`（主表偏移）、`Size` 与 `Root`。
    FirstPageTrailerEntry { key: &'static str, problem: String },
    /// F.3.4：首页 xref 表须把字典列在表首、主 hint 流列在表尾。
    TableEntryOrder {
        role: &'static str,
        object_number: i64,
    },
    /// F.3.1：「All objects shall have a generation number of 0」。
    NonZeroGeneration { role: &'static str, generation: i64 },
    /// F.3.1：线性化字典、目录与页对象不得置于对象流。
    ForbiddenInObjectStream { role: &'static str },
    /// F.3.1：两组对象须由恰好两个交叉引用段索引。
    CrossReferenceSections { count: usize },
    /// F.3.1：第一组对象号须紧接第二组末尾之后。
    ObjectNumberGroups {
        first_group_min: i64,
        second_group_max: i64,
    },
    /// Table F.1 `/O` 与 F.3.7：首页对象缺失或不是页对象。
    FirstPageObject { object_number: i64, problem: String },
    /// F.3.7：首页对象须显式给出必需属性，不得从祖先节点继承。
    FirstPageMissingAttribute {
        object_number: i64,
        key: &'static str,
    },
    /// Table F.1 `/N`：与目录页面树的实际页数不符。
    PageCountMismatch { declared: i64, actual: usize },
    /// Table F.1 `/P`：页码越界（须满足 `0 <= P < N`）。
    FirstPageNumberOutOfRange { page_number: i64, page_count: i64 },
    /// Table F.1：`/E` 或 `/T` 越过文件末尾。
    OffsetBeyondFile {
        key: &'static str,
        offset: u64,
        file_length: u64,
    },
    /// Table F.1：hint 流范围越过文件末尾。
    HintRangeBeyondFile {
        which: &'static str,
        end: u64,
        file_length: u64,
    },
    /// F.3.6：`/H` 指向的对象须是流（只看目标形态，不解码 hint 内容）。
    HintTargetNotStream { which: &'static str, offset: u64 },
    /// Table F.1 `/T`：主交叉引用表/流的定位不符。
    MainCrossReferenceOffset { offset: u64, problem: String },
    /// F.3.11：主表结构（自对象 0 起的单段、首条目 free、其余在用连续）。
    MainCrossReferenceTable { problem: String },
    /// F.3.11：主 trailer「shall not contain any entries other than Size」。
    MainTrailerEntries { keys: Vec<String> },
    /// F.3.2：线性化适用于 PDF 1.1 及以上版本。
    VersionBelowGate { version: String },
}

impl fmt::Display for LinearizationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRequired(key) => {
                write!(f, "F.3.3 Table F.1: required entry /{key} is missing")
            }
            Self::WrongType {
                key,
                expected,
                found,
            } => write!(
                f,
                "Table F.1: entry /{key} shall be {expected}, found {found}"
            ),
            Self::IndirectValue(key) => write!(
                f,
                "F.3.3: all values in the linearization parameter dictionary shall be direct objects, /{key} is an indirect reference"
            ),
            Self::HintArrayLength { len } => {
                write!(
                    f,
                    "Table F.1: /H shall be an array of two or four integers, found {len} element(s)"
                )
            }
            Self::FileLengthMismatch { declared, actual } => write!(
                f,
                "Table F.1: /L {declared} is not exactly equal to the actual file length {actual}; the file shall be treated as ordinary PDF"
            ),
            Self::DictionaryBeyondFirst1024 { end_offset, limit } => write!(
                f,
                "F.3.3: the linearization parameter dictionary shall be entirely contained within the first {limit} bytes, it ends at offset {end_offset}"
            ),
            Self::NotFirstBodyObject { object_number } => write!(
                f,
                "F.3.3: the first object in the body of the file shall be the linearization parameter dictionary, object {object_number} precedes it"
            ),
            Self::ReferencedFrom { location, key } => write!(
                f,
                "F.3.3: there shall be no references to the linearization parameter dictionary, found one from /{key} of the {location}"
            ),
            Self::NotNormalEntry {
                role,
                object_number,
            } => write!(
                f,
                "F.3.3/F.3.4: the first-page cross-reference table shall contain a normal entry for the {role} (object {object_number})"
            ),
            Self::FirstPageTableFreeEntries { count } => write!(
                f,
                "F.3.4: the first-page cross-reference table shall consist of a single subsection with no free entries, found {count} free entries"
            ),
            Self::FirstPageTrailerEntry { key, problem } => write!(
                f,
                "F.3.4: the first-page trailer shall contain a valid /{key} entry, but {problem}"
            ),
            Self::TableEntryOrder {
                role,
                object_number,
            } => write!(
                f,
                "F.3.4: the first-page cross-reference table shall list the {role} (object {object_number}) at the expected position"
            ),
            Self::NonZeroGeneration { role, generation } => write!(
                f,
                "F.3.1: all objects shall have a generation number of 0, the {role} has {generation}"
            ),
            Self::ForbiddenInObjectStream { role } => write!(
                f,
                "F.3.1: the {role} may not be contained in an object stream"
            ),
            Self::CrossReferenceSections { count } => write!(
                f,
                "F.3.1: the two object groups shall be indexed by exactly two cross-reference table sections, found {count}"
            ),
            Self::ObjectNumberGroups {
                first_group_min,
                second_group_max,
            } => write!(
                f,
                "F.3.1: the first object group shall start at the first object number after the last number of the second group, found {first_group_min} after {second_group_max}"
            ),
            Self::FirstPageObject {
                object_number,
                problem,
            } => write!(
                f,
                "F.3.7: the first page's page object {object_number} is unusable: {problem}"
            ),
            Self::FirstPageMissingAttribute { object_number, key } => write!(
                f,
                "F.3.7: page object {object_number} shall explicitly specify /{key} instead of inheriting it"
            ),
            Self::PageCountMismatch { declared, actual } => write!(
                f,
                "Table F.1: /N {declared} does not match the {actual} page(s) of the document catalogue"
            ),
            Self::FirstPageNumberOutOfRange {
                page_number,
                page_count,
            } => write!(
                f,
                "Table F.1: /P {page_number} is out of range for a document of {page_count} page(s)"
            ),
            Self::OffsetBeyondFile {
                key,
                offset,
                file_length,
            } => write!(
                f,
                "Table F.1: /{key} offset {offset} is beyond the end of the file ({file_length} bytes)"
            ),
            Self::HintRangeBeyondFile {
                which,
                end,
                file_length,
            } => write!(
                f,
                "Table F.1: the {which} hint stream range ends at {end}, beyond the end of the file ({file_length} bytes)"
            ),
            Self::HintTargetNotStream { which, offset } => write!(
                f,
                "F.3.6: the {which} hint table shall be enclosed within a stream object, the object at offset {offset} is not a stream"
            ),
            Self::MainCrossReferenceOffset { offset, problem } => {
                write!(f, "Table F.1: /T offset {offset} is invalid: {problem}")
            }
            Self::MainCrossReferenceTable { problem } => {
                write!(
                    f,
                    "F.3.11: main cross-reference table is invalid: {problem}"
                )
            }
            Self::MainTrailerEntries { keys } => write!(
                f,
                "F.3.11: the main trailer shall not contain entries other than Size, found {}",
                keys.join(", ")
            ),
            Self::VersionBelowGate { version } => write!(
                f,
                "F.3.2: linearization may be applied to any PDF file of version 1.1 or greater, the file declares {version}"
            ),
        }
    }
}

impl std::error::Error for LinearizationIssue {}

/// 文件头窗口中定位线性化参数字典的结果（F.3.3）。
struct HeadHit {
    object_number: i64,
    generation: i64,
    offset: u64,
    end_offset: u64,
    first_body_object: i64,
    dict: Dict,
}

impl Document {
    /// 识别线性化 PDF 并类型化其线性化参数字典（附录 F）。
    ///
    /// 返回 `Ok(None)` 表示**非线性化**：文档没有文件源（写入端构造的内存
    /// 文档）、头部无有效 `%PDF-x.y` 行，或正文第一个对象不含 `/Linearized`；
    /// 此时不产生任何告警。返回的 [`LinearizationInfo::issues`] 是对照
    /// F.2/F.3 一致性 shall 的诊断，调用方据此决定是否把文件当普通 PDF 处理
    /// （见 [`LinearizationInfo::treated_as_ordinary`]）。
    ///
    /// `Err` 只表示读取文件头失败（I/O），不表示文件非线性化。
    pub fn linearization(&self) -> Result<Option<LinearizationInfo>> {
        // 写入端构造的文档没有文件源，无从按 F.3.2/F.3.3 判定头部与首对象。
        if self.source.is_none() {
            return Ok(None);
        }
        let file_size = self.source_len();
        // F.3.3 的理由是「limits the amount of data a conforming reader must
        // read before deciding」：先读规范的前 1024 字节，未命中再放宽窗口
        // （宽进严出：识别到越界的字典照样返回模型，并给出诊断）。
        let mut hit: Option<HeadHit> = None;
        for window in [
            FIRST_BYTES_LIMIT as usize,
            HEAD_SCAN_RETRY_WINDOW,
            MAIN_TABLE_WINDOW,
        ] {
            let Some(bytes) = self.read_at(0, window) else {
                return Err(err(
                    0,
                    "failed to read the PDF header for the linearization check",
                ));
            };
            // F.3.2：文件须以标准头部行开始；非 PDF 直接判为非线性化。
            if PdfVersion::parse(&bytes).is_none() {
                return Ok(None);
            }
            hit = scan_head(&bytes);
            if hit.is_some() {
                break;
            }
        }
        let Some(hit) = hit else {
            return Ok(None);
        };

        let mut info = LinearizationInfo {
            object_number: hit.object_number,
            generation: hit.generation,
            offset: hit.offset,
            end_offset: hit.end_offset,
            first_body_object: hit.first_body_object,
            file_size,
            version: None,
            declared_file_length: None,
            hint_streams: None,
            first_page_object: None,
            first_page_end: None,
            main_xref_offset: None,
            page_count: None,
            first_page_number: 0,
            issues: Vec::new(),
            dict: hit.dict,
        };
        info.type_entries();
        let found = checks::collect(self, &info);
        info.issues.extend(found);
        Ok(Some(info))
    }

    /// 文件是否带线性化参数字典（F.3.3）。
    ///
    /// 读取失败按「非线性化」处理，便于调用方把它交给普通路径。
    pub fn is_linearized(&self) -> bool {
        matches!(self.linearization(), Ok(Some(_)))
    }
}

/// 在头部窗口内按文件顺序解析间接对象，直到找到含 `/Linearized` 的字典。
///
/// 头部行与其后的二进制注释行都是 `%` 注释，[`Lexer::skip_ws`] 一并跳过
/// （7.5.2）。窗口被截断、语法断裂或未命中都返回 `None`，由调用方放宽窗口
/// 重试；`first_body_object` 记录窗口内解析到的第一个对象号。
fn scan_head(bytes: &[u8]) -> Option<HeadHit> {
    let mut lex = Lexer::at(bytes, 0);
    lex.skip_ws();
    let mut first_body_object: Option<i64> = None;
    loop {
        let offset = lex.pos();
        let object = parse_indirect_object(&mut lex).ok()?;
        let end_offset = lex.pos();
        if first_body_object.is_none() {
            first_body_object = Some(object.num);
        }
        if let Value::Dict(dict) = &object.value
            && dict.get("Linearized").is_some()
        {
            return Some(HeadHit {
                object_number: object.num,
                generation: object.generation,
                offset,
                end_offset,
                first_body_object: first_body_object.unwrap_or(object.num),
                dict: dict.clone(),
            });
        }
        lex.skip_ws();
        // 下一个对象必须以对象号（数字）开始，否则认定头部窗口已耗尽。
        if !matches!(lex.peek(), Some(b) if b.is_ascii_digit()) {
            return None;
        }
    }
}
