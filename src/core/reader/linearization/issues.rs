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
    /// F.4.1：页偏移 hint 表（Table F.3/F.4）位流损坏——截断或声明位宽超过
    /// 64 位原语上限。该表恒在主 hint 流偏移 0，故无键名可附。
    PageOffsetTableCorrupt { problem: String },
    /// F.4.1：Table F.3 声明「number of bits needed」的项越出原文范围
    /// 「have values in the range 0 through 32」（项 13 是分母值，不在此列）。
    PageOffsetWidthOutOfRange { item: &'static str, bits: u64 },
    /// Table F.4 项 3：「For the first page, this number shall be 0; the next
    /// two items start with the second page」——首页声明了共享对象引用数。
    PageOffsetFirstPageSharedRefs { count: u64 },
    /// Table F.4 项 1 与 F.3.1：首页首对象号（参数字典 `/O`）与「第二页首对象号
    /// 恒为 1」的累加链相撞——`/O` 不大于第二对象组用到的最大对象号。
    PageOffsetObjectNumberConflict {
        first_page_object: u64,
        second_group_max: u64,
    },
    /// Table F.4 项 4 ＋ F.4.2（35604）：共享对象标识是共享对象 hint 表的顺序
    /// 下标（自 0 起），越出该表实际条目数即无从定位。每页报一条，携带该页
    /// 越界标识符的最大值。
    PageOffsetSharedObjectIndexOutOfRange { page: u64, index: u64, entries: u64 },
    /// Table F.3 项 13（分母）为 0，而 Table F.4 项 3 声明了共享对象引用：分数
    /// 位置类别无从判定。读端**不做任何除法**，派生访问器退回头 `None`。
    PageOffsetSharedFractionDenominatorZero { references: u64 },
    /// Table F.2 键 `T` + F.4.3：字典用 `T` 声称有缩略图 hint 表，但 Table F.7
    /// 项 3「The number of pages that have thumbnail images」声明 0，与「the
    /// thumbnails section … shall include one or more per-page entries
    /// (Table F.8)」矛盾——条目整段缺失。
    ThumbnailHintTableMissing,
    /// F.4.3：缩略图 hint 表（Table F.7/F.8）位流损坏——截断、起点超出流长，
    /// 或声明位宽超过 64 位原语上限。本表由 Table F.2 键 `T` 定位，故不像
    /// 页偏移表那样恒在偏移 0。
    ThumbnailTableCorrupt { problem: String },
    /// F.4.3：Table F.7 声明「number of bits needed」的项（项 4/6/8）越出期望
    /// 范围 0—32。该范围的原文只写在 Table F.3 名下（**待核**类比外推，见
    /// `hints::thumbnail` 模块头）；声明值一律保留，表也按声明值读取。
    ThumbnailWidthOutOfRange { item: &'static str, bits: u64 },
    /// F.4.4：Table F.2 键 `O`/`A`/`E`/`I`/`L` 给出的 generic hint 表
    /// （Table F.9）位流损坏——自键给出的起点起读不满 4×32 位，或声明项宽越出
    /// 位数类条目的 0—32 范围（35524—35525）。缺键是**合法**形态（各类对象
    /// 「Present only if …」），不因缺键给诊断。
    GenericHintTableCorrupt { key: String, problem: String },
    /// F.4.5：Table F.2 键 `V`/`C`/`R` 给出的 extended generic hint 表
    /// （Table F.10）位流损坏——自键给出的起点起读不满项 1—6（4×32 + 32 + 16
    /// 位），项 7 的标识符列越过流尾（项 5 × 项 6 的位预算不足），或项 6 声明
    /// 位宽越出位数类条目的 0—32 范围（35524—35525 按 Table F.3 同一措辞
    /// 类比沿用）。缺键是**合法**形态（三类对象「Present only if …」），不因
    /// 缺键给诊断。
    ExtendedHintTableCorrupt { key: String, problem: String },
    /// F.4.5 项 6 与 F.4.1 Table F.3 项 11 的**派生**一致性核对（**不是**原文
    /// 的 shall，见 `hints::extended` 模块头的**待核**）：某个 extended 对象组
    /// 声明的共享对象标识符位宽超过页偏移 hint 表声明的文档级位宽。声明值一律
    /// 保留、表也按组内声明宽度读取完毕，这里只做类型化报告。
    ExtendedHintIdWidthExceedsDocument {
        key: String,
        group_bits: u64,
        document_bits: u64,
    },
    /// F.4.6：Table F.2 键 `B` 给出的内嵌文件流 hint 表（Table F.11/F.12）位流
    /// 损坏——读不满 160 位头部、项主序的某列（含项 5 的标识符列）越过流尾，或
    /// 头部项 4—7／文档级 Table F.3 项 11 的位宽越出 0—32（35524—35525）。缺键
    /// 是**合法**形态（35381「Present only if embedded file streams exist」）。
    EmbeddedFileHintTableCorrupt { problem: String },
    /// F.4.6：键 `B` 在场，但项 5 的唯一宽度来源（**文档级** Table F.3 项 11，
    /// 35847）不可得——页偏移表缺失或读坏。本表**不类型化**：以 0 宽冒充会把所有
    /// 标识符伪造成 0（取舍见 `hints::embedded_file` 模块头）。
    EmbeddedFileIdWidthUnavailable,
    /// F.4.6：Table F.11 项 3 声明 0 组，而键 `B` 的条件就是「有内嵌文件流」
    /// （35381），35795—35797 又规定每项按组数重复——自相矛盾（**派生**结论）。
    EmbeddedFileGroupCountZero,
    /// F.4.6：Table F.12 项 2/项 3 互斥 shall（35837—35844，同 35793—35794 的组
    /// 约定）：一项为 0 时另一项须为 0 且项 4 须大于 0。原值保留、表照常解析。
    EmbeddedFileSharedOnlyGroupViolation {
        group: u64,
        rule: &'static str,
        object_count: u64,
        group_length: u64,
        shared_reference_count: u64,
    },
    /// F.4.6：项 5 的标识符越出 F.4.2 共享对象 hint 表的条目数（35847—35850 的
    /// 「indices into the shared object hint table」＋35604 的自 0 起顺序位置）。
    EmbeddedFileSharedObjectIndexOutOfRange {
        group: u64,
        index: u64,
        entries: u64,
    },
    /// F.4.6 第一组的**派生**相邻性核对（由 35792 的号相邻约定＋Table F.11 项 1
    /// 的字面定义**推出**，非原文写成判定式的 shall）：第一组条目的项 1 落在
    /// 头部项 1 与项 1＋项 2−1 张成的号区间之外。
    EmbeddedFileFirstGroupAnchorMismatch {
        object_number: u64,
        run_start: u64,
        run_end: u64,
    },
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
            Self::PageOffsetTableCorrupt { problem } => write!(
                f,
                "F.4.1 (Table F.3/F.4): the page offset hint table at stream offset 0 is corrupt: {problem}"
            ),
            Self::PageOffsetWidthOutOfRange { item, bits } => write!(
                f,
                "F.4.1: the bit width declared by Table F.3 {item} shall be in the range 0 through 32, found {bits}; the raw value is kept and the table is read as declared"
            ),
            Self::PageOffsetFirstPageSharedRefs { count } => write!(
                f,
                "Table F.4 item 3: for the first page the number of shared objects referenced shall be 0 (the next two items start with the second page), found {count}"
            ),
            Self::PageOffsetObjectNumberConflict {
                first_page_object,
                second_group_max,
            } => write!(
                f,
                "Table F.4 item 1: the first object of the second page shall have an object number of 1 and subsequent pages accumulate, so the pages from the second one use object numbers up to {second_group_max}; /O {first_page_object} does not start after them (F.3.1)"
            ),
            Self::PageOffsetSharedObjectIndexOutOfRange {
                page,
                index,
                entries,
            } => write!(
                f,
                "Table F.4 item 4: page {page} refers to shared object hint table entry {index}, but F.4.2 requires an index that is a sequential position in that table counting from 0 and the table holds {entries} entry(ies)"
            ),
            Self::PageOffsetSharedFractionDenominatorZero { references } => write!(
                f,
                "Table F.3 item 13: the denominator of the fractional position is 0 while the pages reference {references} shared object(s), so Table F.4 item 5 gives no decidable position class; the reader performs no division and the typed position degrades to None"
            ),
            Self::ThumbnailHintTableMissing => write!(
                f,
                "F.4.3: the primary hint stream dictionary declares the thumbnail hint table under key T, but Table F.7 item 3 declares 0 pages that have thumbnail images although the thumbnails section shall include one or more per-page entries (Table F.8)"
            ),
            Self::ThumbnailTableCorrupt { problem } => write!(
                f,
                "F.4.3 (Table F.7/F.8): the thumbnail hint table is corrupt: {problem}"
            ),
            Self::ThumbnailWidthOutOfRange { item, bits } => write!(
                f,
                "F.4.3: the bit width declared by Table F.7 {item} is expected to be in the range 0 through 32 (the wording of F.4.1 for Table F.3, applied by analogy), found {bits}; the raw value is kept and the table is read as declared"
            ),
            Self::GenericHintTableCorrupt { key, problem } => write!(
                f,
                "F.4.4 (Table F.9): the generic hint table for key {key} is corrupt: {problem}"
            ),
            Self::ExtendedHintTableCorrupt { key, problem } => write!(
                f,
                "F.4.5 (Table F.10): the extended generic hint table for key {key} is corrupt: {problem}"
            ),
            Self::ExtendedHintIdWidthExceedsDocument {
                key,
                group_bits,
                document_bits,
            } => write!(
                f,
                "F.4.5 (Table F.10 item 6) / F.4.1 (Table F.3 item 11): the extended generic hint table for key {key} declares {group_bits} bit(s) for the greatest shared object identifier of its group while the page offset hint table declares {document_bits} bit(s) for the whole document; the table is read with its own group width and the raw values are kept (derived consistency rule, not a literal shall)"
            ),
            Self::EmbeddedFileHintTableCorrupt { problem } => write!(
                f,
                "F.4.6 (Table F.11/F.12): the embedded file stream hint table for key B is corrupt: {problem}"
            ),
            Self::EmbeddedFileIdWidthUnavailable => write!(
                f,
                "F.4.6 (Table F.12 item 5): key B declares an embedded file stream hint table, but the document-level bit width from Table F.3 item 11 is unavailable because the page offset hint table is missing or corrupt; the table is not typed because item 5's identifiers would otherwise be fabricated as zeros"
            ),
            Self::EmbeddedFileGroupCountZero => write!(
                f,
                "F.4.6 (Table F.11 item 3): key B declares an embedded file stream hint table whose header declares 0 groups, although F.4.6 repeats every item of Table F.12 once per group and key B is present only if embedded file streams exist"
            ),
            Self::EmbeddedFileSharedOnlyGroupViolation {
                group,
                rule,
                object_count,
                group_length,
                shared_reference_count,
            } => write!(
                f,
                "F.4.6 (Table F.12): group {group} violates the rule \"{rule}\": item 2 = {object_count}, item 3 = {group_length}, item 4 = {shared_reference_count}; the raw values are kept and the table is still read (derived from the group convention in F.4.6)"
            ),
            Self::EmbeddedFileSharedObjectIndexOutOfRange {
                group,
                index,
                entries,
            } => write!(
                f,
                "F.4.6 (Table F.12 item 5): group {group} refers to shared object hint table entry {index}, but F.4.2 requires an index that is a sequential position in that table counting from 0 and the table holds {entries} entry(ies)"
            ),
            Self::EmbeddedFileFirstGroupAnchorMismatch {
                object_number,
                run_start,
                run_end,
            } => write!(
                f,
                "F.4.6 (Table F.11 items 1/3 + Table F.12 item 1): the first embedded file stream group's object number {object_number} lies outside the adjacent object number range {run_start}..={run_end} derived from Table F.11 item 1 and the group's object count, although F.4.6 requires all objects of a group to have adjacent object numbers (derived consistency rule, not a literal shall)"
            ),
        }
    }
}

impl std::error::Error for LinearizationIssue {}
