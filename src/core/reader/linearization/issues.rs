//! 附录 F 一致性诊断枚举（自 [`super::mod.rs`] 拆出以守 600 行/25600 字节
//! 文件上限；文档结构诊断与 hint 表诊断共用这一类型）。
//!
//! 每变体的 `Display` 携带对应条款文本，便于把「为什么违规」直接交给调用方。

use std::fmt;

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
    /// F.3.6：hint 流对象读不到、不是流或 `/Filter` 解码失败，位流无从取起。
    HintStreamUnreadable {
        which: &'static str,
        problem: String,
    },
    /// Table F.2：`S`（Required）——主 hint 流字典缺少共享对象 hint 表位置。
    SharedHintTableMissing,
    /// F.3.6：主 hint 流字典的表位置条目形态不符（非整数、负值或越出解码后的流）。
    HintTablePositionInvalid { key: String, problem: String },
    /// F.4：hint 表位流损坏（截断、非法位宽或 Table F.5 项 3/4 自相矛盾）。
    HintTableCorrupt { key: String, problem: String },
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
            Self::HintStreamUnreadable { which, problem } => write!(
                f,
                "F.3.6: the {which} hint stream cannot supply the decoded bit stream for the F.4 tables: {problem}"
            ),
            Self::SharedHintTableMissing => write!(
                f,
                "F.3.6 Table F.2: the key S is Required, the primary hint stream dictionary shall give the position of the shared object hint table (F.4.2)"
            ),
            Self::HintTablePositionInvalid { key, problem } => write!(
                f,
                "F.3.6: the hint stream dictionary entry /{key} gives a table position that is unusable: {problem}"
            ),
            Self::HintTableCorrupt { key, problem } => write!(
                f,
                "F.4.2 (Table F.5/F.6): the hint table {key} is corrupt: {problem}"
            ),
        }
    }
}

impl std::error::Error for LinearizationIssue {}
