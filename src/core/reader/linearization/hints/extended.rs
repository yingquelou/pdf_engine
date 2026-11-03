//! extended generic hint table（ISO 32000-1 F.4.5，Table F.10）——**只读侧**。
//!
//! 本家族是 F.4.4 `generic`（Table F.9）的**超集**：「An extended generic hint
//! table shall begin with the same entries as in a generic hint table, and shall
//! be followed by three additional entries, as shown in Table F.10. This table
//! provides hints for accessing objects that reference shared objects」（
//! 35779—35781）——项 1—4 与 Table F.9 逐字同形（35808—35811 对比 35802—35805），
//! 多出来的正是「本组引用了哪些共享对象」的项 5—7。
//!
//! 原文要点（行号＝`pdftotext -raw docs/PDF32000_2008.pdf` 的输出行）：
//! - 适用名册：「As of PDF 1.5, the following hint tables, if needed, shall use
//!   the extended generic format: Interactive form hint table / Logical
//!   structure hint table / Renditions name tree hint table」（35781—35785），
//!   即 Table F.2 键 `V`（35332—35333）、`C`（35336—35337）、`R`
//!   （35379—35380）。三键与 F.4.4 的 `O`/`A`/`E`/`I`/`L` **互不相交**，故本增量
//!   对既有 generic 路径是纯加法。
//! - 三键同样都是**条件存在**：`V`「Present only if an interactive form
//!   dictionary exists」、`C`「Present only if a logical structure hierarchy
//!   exists; PDF 1.3」、`R`「Present only if a renditions name tree exists;
//!   PDF 1.5」（同上三处）。缺键即该类对象没给 hint，属合法形态，**不给诊断**
//!   （与 `O`/`A`/`E`/`I`/`L`、`T` 同口径，与 Required 的 `S` 不同）。
//! - **不在本表覆盖范围**：「Embedded file streams shall not be referred to by
//!   this hint table, even if they are reachable from nodes in the renditions
//!   name tree; instead they shall use the hint table described in F.4.6」
//!   （35786—35788）——键 `B` 属 F.4.6，本切片不碰。
//! - Table F.10 项 5—7（35812—35817）：⑤ 32 位「The number of shared object
//!   references.」⑥ 16 位「The number of bits needed to represent the
//!   numerically greatest shared object identifier used by the objects in the
//!   group.」⑦ 起「Starting with item 7, each of the remaining items in this
//!   table shall be a shared object identifier—that is, an index into the
//!   shared object hint table」，其 size 列字面写「See Table F.3, item 11」。
//! - 表内项 2 的位置仍是文件偏移，按 F.4 前言 NOTE 3 修正（35500—35503）：
//!   `LinearizationHints::adjust_position`。
//!
//! **版本门**（本库策略，非条款）：35781 只说「as of PDF 1.5 … shall use the
//! extended generic format」，**没有**「1.5 之前的 `V`/`C`/`R` 须按 Table F.9 读」
//! 的 shall。但 extended 形态相对 generic 只多不少，1.5 之前出现这三键时唯一有
//! 据可循的形态就是 F.4.4 的 generic 表（项 5—7 在不在流里无从判定）。故
//! [`group_hint_keys`] 在声明版本 < 1.5 时把 `V`/`C`/`R` 一并交给 generic 路径，
//! ≥ 1.5 时才按 Table F.10 解析（[`ExtendedHintTable`]）。版本取自
//! [`Document::source_version`](crate::core::types::Document)，与 F.3.2 的线性化
//! 版本门同一来源，直接比较 [`PdfVersion`] 常量、不引入新的特性开关。
//!
//! **待核**（一律取有据的那一种读法，不猜第二种）：
//! - **项 7 的打包宽度歧义**：项 7 的 size 列指向**文档级**的 Table F.3 项 11
//!   「The number of bits needed to represent the numerically greatest shared
//!   object identifier used by the pages」（35546—35548，本库已解析为
//!   `PageOffsetHeader::shared_object_id_bits`），而项 6 声明的是**本组**的同一
//!   量（35813—35814）。两处都只是字面，没有任何一句裁定谁优先。本库**取项 6
//!   的组内宽度**解包项 7：①项 6 就装在本表里，取它才让本表自描述、可独立解出
//!   （页偏移表缺失或读坏时，文档级宽度根本拿不到）；②项 6 的描述限定在「used
//!   by the objects in the group」，与项 7 的逐组标识符同域。项 6 的原值同时
//!   暴露在 [`ExtendedHintTable::shared_object_id_bits`]，并在页偏移表也在场时
//!   给 [`LinearizationIssue::ExtendedHintIdWidthExceedsDocument`]——该交叉核对
//!   是由两处字段定义**推出**的一致性规则，**不是**原文的 shall，口径同
//!   [`GenericHintTable::object_range`](super::generic::GenericHintTable::object_range)
//!   对派生访问器的标注。除此之外不发明第二种线格式、不做「猜宽度」的退路。
//! - **项 6 为 0**：0 位只能表示值 0，故项 7 不消费任何位；本库仍按项 5 的条数
//!   解包，得到的标识符全为 0（＝共享对象 hint 表的第 0 条）。此时位预算给不出
//!   计数证据，沿用 [`MAX_UNEVIDENCED_ITEMS`] 绝对上限（页偏移表的
//!   `fits_shared` 同一手法，属库策略而非条款）——绝不做不可界分配，也不因宽度
//!   为 0 而除零。
//! - **项 6 的 0—32 范围**：「All the items in Table F.3 that specify a number
//!   of bits needed … have values in the range 0 through 32. Although that
//!   range requires only 6 bits, 16-bit numbers shall be used」（35524—35525）
//!   字面只约束 Table F.3；项 6 用的是同一措辞、同一 16 位存储，故按 `generic`
//!   模块头既有口径类比沿用该上限。越界一律 [`HintBitError::InvalidWidth`]
//!   类型化拒绝（保留原值毫无意义：项 7 的列长直接由它决定），绝不退化成
//!   未定义行为。

use super::super::issues::LinearizationIssue;
use super::bits::{BitReader, HintBitError};
use super::page_offset::{
    MAX_DECLARED_WIDTH, MAX_UNEVIDENCED_ITEMS, PageOffsetHintTable, column, width,
};
use super::{HintTablePosition, LinearizationHints};
use crate::core::types::PdfVersion;

/// Table F.10 项 1—5 的定宽（位；35808—35812 的「32」列）。
const TABLE_F10_FIXED_ITEM_WIDTH_BITS: u32 = 32;

/// Table F.10 项 6 的存储宽度（位；35813 的「16」列）。
const TABLE_F10_WIDTH_FIELD_BITS: u32 = 16;

/// F.4.5 用 extended 形态承载的 Table F.2 键（名册 35332—35333 的 `V`、
/// 35336—35337 的 `C`、35379—35380 的 `R`，按名册序）。三键都是**条件存在**，
/// 缺键不报错。
pub(super) const EXTENDED_HINT_KEYS: [&str; 3] = ["V", "C", "R"];

/// F.4.4 与 F.4.5 八键的**名册序**合并表（Table F.2 的 35326—35338 与
/// 35379—35380），只在声明版本 < 1.5 时使用：那时 `V`/`C`/`R` 也按 Table F.9
/// 形态解析（见模块头的**版本门**），故五键的相对次序与
/// [`super::generic::GENERIC_HINT_KEYS`] 完全一致，既有行为不变。
const GENERIC_SHAPE_KEYS_BEFORE_PDF_1_5: [&str; 8] = ["O", "A", "E", "V", "I", "C", "L", "R"];

/// F.4.5 的版本门（35781「As of PDF 1.5」）：文档声明版本是否 ≥ 1.5。
pub(super) fn uses_extended_format(version: PdfVersion) -> bool {
    version >= PdfVersion::V1_5
}

/// 分组表（一张表描述**一个**对象组）分发器该按哪些 Table F.2 键走 Table F.9
/// 形态：≥ 1.5 时只有 F.4.4 的五键，< 1.5 时把 `V`/`C`/`R` 也并进来（模块头的
/// **版本门**）。
pub(super) fn group_hint_keys(version: PdfVersion) -> &'static [&'static str] {
    if uses_extended_format(version) {
        &super::generic::GENERIC_HINT_KEYS
    } else {
        &GENERIC_SHAPE_KEYS_BEFORE_PDF_1_5
    }
}

/// F.4.5 / Table F.10：一个对象组的 extended generic hint 表（项 1—4 同
/// Table F.9，另加项 5 的 32 位、项 6 的 16 位与项 7 起的变长标识符列）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtendedHintTable {
    /// 本组来自哪个 Table F.2 键（[`EXTENDED_HINT_KEYS`] 之一）：`V` 交互表单、
    /// `C` 逻辑结构、`R` 影片名称树。同一形态服务三类对象，故身份须跟着解析
    /// 结果走，调用方才能按键取组。
    pub key: &'static str,
    /// 项 1（32 位，35808）：组内第一个对象的对象号。
    pub first_object_number: u64,
    /// 项 2（32 位，35809）：组内第一个对象的位置（文件偏移，须按 F.4 前言
    /// NOTE 3 用 `LinearizationHints::adjust_position` 修正）。
    pub first_object_location: u64,
    /// 项 3（32 位，35810）：组内对象数。
    pub object_count: u64,
    /// 项 4（32 位，35811）：对象组的长度（字节）。
    pub group_length: u64,
    /// 项 5（32 位，35812）：本组引用的共享对象数（＝项 7 列的声明长度）。
    pub shared_reference_count: u64,
    /// 项 6（16 位，35813—35814）：表示本组用到的最大共享对象标识符所需的位数。
    /// 原值保留（不折算、不用文档级值替换），项 7 即按本宽度解包（见模块头
    /// **待核**）。
    pub shared_object_id_bits: u32,
    /// 项 7 起（35815—35817）：逐个共享对象标识符，即 F.4.2 共享对象 hint 表
    /// 的顺序下标（自 0 起）。列位越过流尾时整表按 [`HintBitError`] 拒绝，
    /// 故成功解析的表其长度恒等于项 5（[`Self::shared_reference_count`]）——
    /// 不留「静默半截列」。
    pub shared_object_ids: Vec<u64>,
    /// 本表在主 hint 流位流里的起始字节偏移，即 Table F.2 该键的值。
    pub start_byte: u64,
    /// 本表消费掉的位数：项 1—6 恒 176 位，项 7 起按 [`Self::shared_object_ids`]
    /// 的长度追加。
    pub bits_consumed: u64,
}

impl ExtendedHintTable {
    /// 从 `data`（主/溢出 hint 流解码拼接后的位流）的字节偏移 `start` 处按
    /// Table F.10 解析键 `key` 的对象组。
    ///
    /// `start` 由主 hint 流字典的 Table F.2 键给出（F.3.6：「the dictionary of
    /// the primary hint stream shall contain entries giving the position of the
    /// beginning of each hint table in the stream」35313—35316，且**只**在主流
    /// 字典里），F.4 前言另要求表以字节边界开始（35482—35483）。
    pub fn parse(data: &[u8], start: usize, key: &'static str) -> Result<Self, HintBitError> {
        if start > data.len() {
            return Err(HintBitError::ShortBuffer {
                needed_bits: 8,
                remaining_bits: 0,
                position_bits: start as u64 * 8,
            });
        }
        let mut reader = BitReader::new(&data[start..]);
        // 项 1—4：与 Table F.9 同形（35779—35780 的 shall）。
        let first_object_number = reader.unsigned(TABLE_F10_FIXED_ITEM_WIDTH_BITS)?;
        let first_object_location = reader.unsigned(TABLE_F10_FIXED_ITEM_WIDTH_BITS)?;
        let object_count = reader.unsigned(TABLE_F10_FIXED_ITEM_WIDTH_BITS)?;
        let group_length = reader.unsigned(TABLE_F10_FIXED_ITEM_WIDTH_BITS)?;
        // 项 5（32 位计数）与项 6（16 位宽度）都是**不可信声明值**：先过 0—32
        // 位宽门，再按剩余位预算为 `项 5 × 项 6` 封顶，然后才进入任何分配。
        let shared_reference_count = reader.unsigned(TABLE_F10_FIXED_ITEM_WIDTH_BITS)?;
        let shared_object_id_bits = width(reader.unsigned(TABLE_F10_WIDTH_FIELD_BITS)?);
        if shared_object_id_bits > MAX_DECLARED_WIDTH {
            return Err(HintBitError::InvalidWidth {
                bits: u64::from(shared_object_id_bits),
            });
        }
        let declared = shared_reference_count;
        let needed_bits = u64::from(shared_object_id_bits).saturating_mul(declared);
        let Some(count) =
            bounded_identifier_count(declared, shared_object_id_bits, reader.remaining_bits())
        else {
            return Err(HintBitError::ShortBuffer {
                needed_bits,
                remaining_bits: reader.remaining_bits(),
                position_bits: reader.position_bits(),
            });
        };
        let shared_object_ids = column(&mut reader, count, shared_object_id_bits)?;
        Ok(Self {
            key,
            first_object_number,
            first_object_location,
            object_count,
            group_length,
            shared_reference_count,
            shared_object_id_bits,
            shared_object_ids,
            start_byte: start as u64,
            bits_consumed: reader.position_bits(),
        })
    }

    /// 本表末尾之后的第一个字节（流内绝对位置，起点是 Table F.2 键的值）。
    /// F.4 前言要求各表以字节边界开始且互不重叠，故其他键的位置落进
    /// `[start_byte, 本值)` 即自相矛盾；本表长度**不固定**（项 7 是变长列），
    /// 只能按 [`Self::bits_consumed`] 折算。
    pub fn end_byte(&self) -> u64 {
        self.start_byte
            .saturating_add(self.bits_consumed.div_ceil(8))
    }

    /// 项 1 + 项 3：组内对象号的**闭**区间 `(first, last)`。项 3 声明 0 个
    /// 对象时返回 `None`——与 Table F.9 同口径：F.4.5 没有「组须至少含一个
    /// 对象」的 shall（该措辞只在 F.4.6 的 35792—35793 出现），故空组按声明值
    /// 原样保留、不给诊断。由项 1/项 3 的字面定义**推出**的访问器，非规范条款；
    /// 累加一律 saturating，绝不因不可信位流溢出 panic。
    pub fn object_range(&self) -> Option<(u64, u64)> {
        if self.object_count == 0 {
            return None;
        }
        let last = self
            .first_object_number
            .saturating_add(self.object_count.saturating_sub(1));
        Some((self.first_object_number, last))
    }

    /// **派生**一致性核对（非原文 shall，见模块头**待核**）：本组项 6 声明的
    /// 标识符位宽是否超过文档级 Table F.3 项 11 的 `shared_object_id_bits`。
    /// 超过时返回 `(组内声明位宽, 文档声明位宽)`；文档没有页偏移 hint 表时
    /// 没有可比对象，一律 `None`。
    pub fn shared_id_width_problem(&self, document_bits: u32) -> Option<(u64, u64)> {
        (self.shared_object_id_bits > document_bits).then_some((
            u64::from(self.shared_object_id_bits),
            u64::from(document_bits),
        ))
    }
}

/// 项 7 列的元素数：先按位预算封顶，再进入任何分配。
///
/// 项 5 是 32 位不可信计数、项 6 是 16 位不可信宽度，二者相乘可达 ~2^48，
/// 故这里只允许 `宽度 × 条数 <= 剩余位` 的形态通过（乘法比较，避免除法与
/// 宽度 0 的除零）；宽度为 0 时列不占位，改用 [`MAX_UNEVIDENCED_ITEMS`]
/// 绝对上限（模块头的**待核**）。宁给类型化错误，绝不做不可界分配。
fn bounded_identifier_count(declared: u64, bits: u32, remaining_bits: u64) -> Option<usize> {
    let count = usize::try_from(declared).ok()?;
    if bits == 0 {
        return (count as u64 <= MAX_UNEVIDENCED_ITEMS).then_some(count);
    }
    (u64::from(bits).saturating_mul(count as u64) <= remaining_bits).then_some(count)
}

/// Table F.2 键 `V`/`C`/`R`：F.4.5 的 extended generic hint 表（Table F.10）。
///
/// 三键都是**条件存在**（模块头），缺键即该类对象没给 hint，属合法形态，
/// **不给诊断**，故返回列表可以少于三项；`V`/`C`/`R` 的位置若落进前一张已解析
/// 表（页偏移表、缩略图表、generic 表或前一张 extended 表）的表体内，已由位置
/// 剔除给出 [`LinearizationIssue::HintTablePositionInvalid`] 并从位置表里消失，
/// 这里直接跳过——起点都不可信，不猜。键在而形态不符（读不满项 1—6、项 7 的
/// 列越过流尾、项 6 越出 0—32）落 [`LinearizationIssue::ExtendedHintTableCorrupt`]。
///
/// 版本门：< 1.5 的文档里这三键已由 generic 路径消化（[`group_hint_keys`]），
/// 本函数返回空列表，否则同一张表会被读两遍、还可能被读成两种形态。
pub(super) fn parse_extended(
    positions: &mut Vec<HintTablePosition>,
    data: &[u8],
    version: PdfVersion,
    issues: &mut Vec<LinearizationIssue>,
) -> Vec<ExtendedHintTable> {
    if !uses_extended_format(version) {
        return Vec::new();
    }
    let mut tables = Vec::new();
    for key in EXTENDED_HINT_KEYS {
        let Some(position) = positions
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.position)
        else {
            continue;
        };
        // 位置已由装配层收敛到 `0..=stream_len`，故换算 `usize` 不会失败；
        // 失败时只跳过本键，不猜起点。
        let Ok(start) = usize::try_from(position) else {
            continue;
        };
        match ExtendedHintTable::parse(data, start, key) {
            Ok(table) => {
                drop_positions_inside_extended_table(&table, positions, issues);
                tables.push(table);
            }
            Err(problem) => issues.push(LinearizationIssue::ExtendedHintTableCorrupt {
                key: key.to_string(),
                problem: problem.to_string(),
            }),
        }
    }
    // 本键的位置被别人的表体吃掉时不列条目：位置都不再可信，解析结果也不该留。
    tables.retain(|table| positions.iter().any(|entry| entry.key == table.key));
    tables
}

/// F.4 前言：「each hint table shall begin at a byte boundary」（35482—35483）
/// 且各表按 NOTE 2 依次排布、互不重叠。extended 表长随项 5/项 6 变化，故按
/// [`ExtendedHintTable::end_byte`] 折算；其他 Table F.2 位置落进
/// `[本键, 本键末)` 即自相矛盾。本键自身的起点不在此列；两个键共用同一位置
/// 不视为矛盾——规范没有禁止同一张表兼任两类 hint，此处不发明 shall。
fn drop_positions_inside_extended_table(
    table: &ExtendedHintTable,
    positions: &mut Vec<HintTablePosition>,
    issues: &mut Vec<LinearizationIssue>,
) {
    let start = table.start_byte;
    let end = table.end_byte();
    let inside = positions
        .iter()
        .filter(|entry| entry.key != table.key && (start..end).contains(&entry.position))
        .cloned()
        .collect::<Vec<_>>();
    for entry in inside {
        issues.push(LinearizationIssue::HintTablePositionInvalid {
            key: entry.key,
            problem: format!(
                "position {} lies inside the extended generic hint table of key {}, which occupies bytes {start}..{end}",
                entry.position, table.key
            ),
        });
    }
    positions.retain(|entry| entry.key == table.key || !(start..end).contains(&entry.position));
}

/// F.4.5 项 6 与 F.4.1 Table F.3 项 11 的**派生**一致性核对（见模块头**待核**：
/// 这不是原文的 shall，只是两处同名字段的自洽性）。逐组一条诊断，携带键名与
/// 两个声明值；页偏移表不在场时无从比较，静默跳过。
pub(super) fn check_extended_shared_id_widths(
    page_offset: Option<&PageOffsetHintTable>,
    tables: &[ExtendedHintTable],
    issues: &mut Vec<LinearizationIssue>,
) {
    let Some(header) = page_offset.map(|table| &table.header) else {
        return;
    };
    for table in tables {
        if let Some((group_bits, document_bits)) =
            table.shared_id_width_problem(header.shared_object_id_bits)
        {
            issues.push(LinearizationIssue::ExtendedHintIdWidthExceedsDocument {
                key: table.key.to_string(),
                group_bits,
                document_bits,
            });
        }
    }
}

impl LinearizationHints {
    /// 按 Table F.2 键取 F.4.5 的 extended 对象组（`V`/`C`/`R`）：键缺失（该类
    /// 对象没给 hint，合法形态）与键在而解析失败都返回 `None`，**不是**空组；
    /// 声明版本 < 1.5 时该键走的是 F.4.4 形态，须用
    /// `LinearizationHints::generic_group_of` 取。
    pub fn extended_group_of(&self, key: &str) -> Option<&ExtendedHintTable> {
        self.extended.iter().find(|table| table.key == key)
    }

    /// 键 `key` 的 extended 表结束后的第一个字节（流内绝对位置，起点是该键的
    /// 值）；本键没有可用表时 `None`。F.4 前言要求各表以字节边界开始且互不
    /// 重叠，故其他键的位置落进 `[该键, 本值)` 即自相矛盾。
    pub fn extended_end_byte(&self, key: &str) -> Option<u64> {
        self.extended_group_of(key).map(ExtendedHintTable::end_byte)
    }
}
