//! 附录 F 一致性诊断枚举（自 [`super::mod.rs`] 拆出以守 600 行/25600 字节
//! 文件上限；文档结构诊断与 hint 表诊断共用这一类型）。
//!
//! 每变体的 `Display` 携带对应条款文本，便于把「为什么违规」直接交给调用方；
//! 该 `impl` 本身也越过红线，故再拆到 [`display`]（变体与文案须同轮增删）。

mod display;

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
    /// F.3.6：hint 流字典除 `/Length` 外的值不得含间接引用。
    HintStreamIndirectValue { which: &'static str, key: String },
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
