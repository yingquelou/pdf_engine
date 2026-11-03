//! 附录 F.4 hint 表读端解码（已完整解析**页偏移**、**共享对象**与**缩略图**
//! 三家）。
//!
//! 消费上层已识别的 [`LinearizationInfo`]（Table F.1 `/H` 给出的 hint 流
//! 范围由 [`HintStreams`]/[`HintRange`] 承载，本模块不重复定位，只做：
//! 读流 → 过滤解码 → 溢出拼接 → 按主 hint 流字典的 Table F.2 键取表位置
//! → 位流解析）。
//!
//! **已实现**：
//! - [`bits`]：F.4 前言的位流原语（任意宽度、高位在前；带符号形式与链式
//!   增量表按 PDF Reference 1.4 F.2.2/F.2.3 字面实现，ISO 32000-1 的 F.4
//!   各表未引用，属**待核**预留）；
//! - [`page_offset`]：F.4.1 页偏移 hint 表（Table F.3 头部 13 项 + Table F.4
//!   逐页条目 7 项，含 a)—g) 的项交错序）；该表没有 Table F.2 键，按前言
//!   「shall be the first table in the stream and shall start at offset 0」
//!   恒从流偏移 0 起读，页数取 Table F.1 `/N`；
//! - [`shared`]：F.4.2 共享对象 hint 表（Table F.5 头部 + Table F.6 组
//!   条目，两条序列、项交错序与 128 位 MD5 签名）；
//! - [`thumbnail`]：F.4.3 缩略图 hint 表（Table F.7 头部 12 项 + Table F.8
//!   逐页条目 3 项，三列各自按页序）；本表由 Table F.2 键 `T` 定位，键
//!   「Present only if thumbnail images exist」，**缺失是合法形态**，故不
//!   因缺键给诊断。
//!
//! **未解析（诚实缺口）**：F.4 其余家族——generic hint 表（F.4.4，Table F.9，
//! outline/thread-information/named-destination/information-dictionary/
//! page-label 共用）、extended generic hint 表（F.4.5，Table F.10，
//! interactive form/logical structure/renditions name tree 共用）、
//! embedded file stream hint 表（F.4.6，Table F.11/F.12）。这些表的位置
//! 仍按 Table F.2 键在 [`LinearizationHints::table_positions`] 里如实
//! 列出，但内容为原样字节，未类型化。
//!
//! 诊断进入 [`LinearizationHints::issues`]（复用 [`LinearizationIssue`]
//! 的 hint 家族新增变体），不写 `Document::warnings`；[`Document::linearization`]
//! 的行为完全不受本模块影响。

use crate::core::filters::decode_stream;
use crate::core::types::{Dict, Document, Value};

use super::LinearizationInfo;
use super::checks::read_object_at;
use super::dict::HintRange;
use super::issues::LinearizationIssue;

pub mod bits;
mod page_offset;
mod shared;
mod thumbnail;

#[cfg(test)]
mod tests;

pub use page_offset::{PageOffsetEntry, PageOffsetHeader, PageOffsetHintTable};
pub use shared::{SharedObjectGroupEntry, SharedObjectHeader, SharedObjectHintTable};
pub use thumbnail::{ThumbnailEntry, ThumbnailHeader, ThumbnailHintTable};

/// Table F.2 列出的标准 hint 表键（`S` 为 Required，其余按存在条件）。
/// 注册键（Annex E）不在标准集合内，按未知键跳过、不报错。
const STANDARD_HINT_KEYS: [(&str, &str); 11] = [
    ("S", "shared object hint table (F.4.2, Table F.5/F.6)"),
    ("T", "thumbnail hint table (F.4.3, Table F.7/F.8)"),
    ("O", "outline hint table (F.4.4 generic)"),
    ("A", "thread information hint table (F.4.4 generic)"),
    ("E", "named destination hint table (F.4.4 generic)"),
    ("V", "interactive form hint table (F.4.5 extended generic)"),
    ("I", "information dictionary hint table (F.4.4 generic)"),
    (
        "C",
        "logical structure hint table (F.4.5 extended generic, PDF 1.3)",
    ),
    ("L", "page label hint table (F.4.4 generic, PDF 1.3)"),
    (
        "R",
        "renditions name tree hint table (F.4.5 extended generic, PDF 1.5)",
    ),
    ("B", "embedded file stream hint table (F.4.6, PDF 1.5)"),
];

/// F.4 前言：页偏移 hint 表「shall be the first table in the stream and shall
/// start at offset 0」，没有 Table F.2 键，故位置常量而非字典条目。
pub const PAGE_OFFSET_TABLE_POSITION: u64 = 0;

/// 主 hint 流字典给出的一张表的位置条目（F.3.6：字节偏移，相对解码后的
/// 流数据起点，溢出流已拼接）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintTablePosition {
    /// Table F.2 键名。
    pub key: String,
    /// 表在（解码拼接后的）hint 流位流中的字节偏移。
    pub position: u64,
}

/// 类型化后的 hint 表数据与一致性诊断。
#[derive(Debug, Clone, PartialEq)]
pub struct LinearizationHints {
    /// 主 hint 流的对象范围（Table F.1 `/H` 第一对；复制自
    /// [`LinearizationInfo::hint_streams`]，供位置修正与溯源）。
    pub primary: HintRange,
    /// 解码并拼接溢出流后的位流总字节数。
    pub stream_bytes: usize,
    /// 按 Table F.2 标准键列出的表位置（缺失键不列）。
    pub table_positions: Vec<HintTablePosition>,
    /// 已完整解析的页偏移 hint 表（F.4.1）；该表无 Table F.2 键，恒在流偏移
    /// 0（[`PAGE_OFFSET_TABLE_POSITION`]）。Table F.1 `/N` 不可用时为 `None`
    /// 且不报错（结构层已就 `/N` 给诊断）；解析失败亦为 `None` 并在
    /// [`Self::issues`] 给出类型化诊断。
    pub page_offset: Option<PageOffsetHintTable>,
    /// 已完整解析的共享对象 hint 表（F.4.2）；解析失败为 `None` 并在
    /// [`Self::issues`] 给出类型化诊断。
    pub shared_object: Option<SharedObjectHintTable>,
    /// 已完整解析的缩略图 hint 表（F.4.3）；Table F.2 键 `T` 缺失（文档没有
    /// 缩略图）是合法形态，此时为 `None` 且**不给诊断**；键在而解析失败亦为
    /// `None`，并在 [`Self::issues`] 给出类型化诊断。
    pub thumbnail: Option<ThumbnailHintTable>,
    /// hint 层一致性诊断；`LinearizationInfo::issues` 的文档结构诊断
    /// 不在此列（由 [`Document::linearization`] 报告）。
    pub issues: Vec<LinearizationIssue>,
}

impl LinearizationHints {
    /// hint 层是否一致（无诊断）。
    pub fn is_consistent(&self) -> bool {
        self.issues.is_empty()
    }

    /// 按 Table F.2 键查表位置。
    pub fn position_of(&self, key: &str) -> Option<u64> {
        self.table_positions
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.position)
    }

    /// F.4 前言 NOTE 3：hint 表内给出的位置「shall be interpreted as if
    /// the primary hint stream itself were not present」——大于主 hint 流
    /// 起点的位置须补上主 hint 流长度才是文件绝对偏移；溢出流的偏移与
    /// 长度按 F.4 前言末段不计。
    pub fn adjust_position(&self, position: u64) -> u64 {
        if position > self.primary.offset {
            position.saturating_add(self.primary.length)
        } else {
            position
        }
    }

    /// F.4.1 页偏移 hint 表结束后的第一个字节，即后续 Table F.2 各表位置的
    /// 下界；未解析成功时 `None`（F.4 前言：各表以字节边界开始且互不重叠）。
    pub fn page_offset_end_byte(&self) -> Option<u64> {
        self.page_offset.as_ref().map(PageOffsetHintTable::end_byte)
    }

    /// F.4.3 缩略图 hint 表结束后的第一个字节（流内绝对位置，起点是 Table
    /// F.2 键 `T` 的值）；本表未解析成功时 `None`。F.4 前言要求各表以字节边界
    /// 开始且互不重叠，故其他键的位置落进 `[T, 本值)` 即自相矛盾。
    pub fn thumbnail_end_byte(&self) -> Option<u64> {
        self.thumbnail.as_ref().map(ThumbnailHintTable::end_byte)
    }
}

impl Document {
    /// 类型化解析附录 F.4 的 hint 表（读端；当前完整解析 F.4.1 页偏移表、
    /// F.4.2 共享对象 hint 表与 F.4.3 缩略图 hint 表，其余家族见模块头的诚实
    /// 缺口）。
    ///
    /// 返回 `Ok(None)` 表示**无 hint 可解**：文档非线性化、缺 `/H`，或
    /// 主 hint 流对象完全不可读（此时不给诊断——[`Document::linearization`]
    /// 已在结构层报告缺 `/H` 等条目问题）。`Ok(Some)` 的
    /// [`LinearizationHints::issues`] 只含 hint 层诊断（Table F.2 键缺失、
    /// 位置越界、位流截断等）。
    pub fn linearization_hints(&self) -> super::Result<Option<LinearizationHints>> {
        let Some(info) = self.linearization()? else {
            return Ok(None);
        };
        Ok(self.hint_data(&info))
    }

    /// 从已识别的线性化模型出发解码 hint 表；`/H` 缺失返回 `None`。
    fn hint_data(&self, info: &LinearizationInfo) -> Option<LinearizationHints> {
        let streams = info.hint_streams?;
        let mut issues = Vec::new();
        let (dict, mut data) = match self.read_hint_stream(streams.primary) {
            Ok(part) => part,
            Err(problem) => {
                issues.push(LinearizationIssue::HintStreamUnreadable {
                    which: "primary",
                    problem,
                });
                return Some(LinearizationHints {
                    primary: streams.primary,
                    stream_bytes: 0,
                    table_positions: Vec::new(),
                    page_offset: None,
                    shared_object: None,
                    thumbnail: None,
                    issues,
                });
            }
        };
        // F.3.6：两 hint 流的（解码后）内容「shall be concatenated and
        // treated as if they were a single unbroken stream」；表位置只存在
        // 于主流字典。溢出流读失败给诊断并退回主流继续。
        if let Some(overflow) = streams.overflow {
            match self.read_hint_stream(overflow) {
                Ok((_, extra)) => data.extend_from_slice(&extra),
                Err(problem) => issues.push(LinearizationIssue::HintStreamUnreadable {
                    which: "overflow",
                    problem,
                }),
            }
        }
        let mut table_positions = table_positions(&dict, data.len(), &mut issues);
        // F.4 前言：页偏移 hint 表恒为主流的第一张表、自偏移 0 开始，页数取
        // Table F.1 `/N`（缺失/非正/超出 `usize` 即跳过，结构层已就 `/N` 报告）。
        let page_offset =
            parse_page_offset(info.page_count, &data, &mut table_positions, &mut issues);
        // F.4.3 缩略图表排在 F.4.2 之前：本表体的字节范围同样参与「表互不
        // 重叠」的位置剔除，须在其余 Table F.2 键被用作解析起点之前完成。
        let thumbnail = parse_thumbnail(&mut table_positions, &data, &mut issues);
        check_thumbnail_semantics(thumbnail.as_ref(), &mut issues);
        // 键存在但形态不符时不再叠加「缺 S」诊断。
        let s_declared = dict.get("S").is_some();
        let shared_object = parse_shared(&table_positions, s_declared, &data, &mut issues);
        check_header_consistency(shared_object.as_ref(), &mut issues);
        check_page_offset_semantics(page_offset.as_ref(), &mut issues);
        Some(LinearizationHints {
            primary: streams.primary,
            stream_bytes: data.len(),
            table_positions,
            page_offset,
            shared_object,
            thumbnail,
            issues,
        })
    }

    /// 读一个 hint 流对象：语法解析 → 取流字节（含解密）→ 按 `/Filter`
    /// 解码（F.3.6：表位置以「after decoding filters」后的字节计）。
    fn read_hint_stream(&self, range: HintRange) -> Result<(Dict, Vec<u8>), String> {
        let value = read_object_at(self, range.offset)
            .ok_or_else(|| "the object at the given offset does not parse".to_string())?;
        let Value::Stream(dict, _) = value.clone() else {
            return Err(format!(
                "the object at offset {} is {} rather than a stream",
                range.offset,
                value.type_name()
            ));
        };
        let raw = self
            .try_stream_bytes(&value)
            .map_err(|problem| format!("stream bytes unreadable: {problem}"))?;
        let decoded = decode_stream(&dict, &raw).map_err(|problem| {
            format!("stream bytes failed to decode through /Filter: {problem}")
        })?;
        Ok((dict, decoded))
    }
}

/// 收集主 hint 流字典里的 Table F.2 标准键位置；非整数/负值/越界各落
/// [`LinearizationIssue::HintTablePositionInvalid`]。
fn table_positions(
    dict: &Dict,
    stream_len: usize,
    issues: &mut Vec<LinearizationIssue>,
) -> Vec<HintTablePosition> {
    let mut positions = Vec::new();
    for (key, _) in STANDARD_HINT_KEYS {
        let Some(value) = dict.get(key) else {
            continue;
        };
        // F.3.6：除 `Length` 外 hint 流字典的值须是直接对象；引用值
        // 无法在字典层解引用，按类型不符给诊断。
        let position = match value {
            Value::Integer(number) => match u64::try_from(*number) {
                Ok(position) if position <= stream_len as u64 => position,
                Ok(position) => {
                    issues.push(LinearizationIssue::HintTablePositionInvalid {
                        key: key.to_string(),
                        problem: format!(
                            "position {position} is beyond the decoded hint stream ({stream_len} bytes)"
                        ),
                    });
                    continue;
                }
                Err(_) => {
                    issues.push(LinearizationIssue::HintTablePositionInvalid {
                        key: key.to_string(),
                        problem: format!("position {number} is negative"),
                    });
                    continue;
                }
            },
            other => {
                issues.push(LinearizationIssue::HintTablePositionInvalid {
                    key: key.to_string(),
                    problem: format!(
                        "the entry is {} rather than a non-negative integer",
                        other.type_name()
                    ),
                });
                continue;
            }
        };
        positions.push(HintTablePosition {
            key: key.to_string(),
            position,
        });
    }
    positions
}

/// F.4.1 页偏移 hint 表：主流第一张表、恒自偏移 0 起，页数取 Table F.1 `/N`。
///
/// 该表没有 Table F.2 键（位置由 [`PAGE_OFFSET_TABLE_POSITION`] 固定），故不走
/// 字典条目定位；但解析成功后其长度参与位置核对——F.4 前言规定「each hint
/// table shall begin at a byte boundary」且 NOTE 2 要求按序读取，落进表体里的
/// Table F.2 位置自相矛盾，由 [`drop_positions_inside_page_offset_table`] 剔除
/// 并逐键给 [`LinearizationIssue::HintTablePositionInvalid`]。
/// `/N` 缺失、非正或超出 `usize` 时整体跳过——结构层已就 `/N` 给出诊断，
/// 这里不叠加假阳性。
fn parse_page_offset(
    page_count: Option<i64>,
    data: &[u8],
    positions: &mut Vec<HintTablePosition>,
    issues: &mut Vec<LinearizationIssue>,
) -> Option<PageOffsetHintTable> {
    let pages = u64::try_from(page_count.filter(|count| *count > 0)?).ok()?;
    let table = match PageOffsetHintTable::parse(data, PAGE_OFFSET_TABLE_POSITION as usize, pages) {
        Ok(table) => table,
        Err(problem) => {
            issues.push(LinearizationIssue::PageOffsetTableCorrupt {
                problem: problem.to_string(),
            });
            return None;
        }
    };
    drop_positions_inside_page_offset_table(&table, positions, issues);
    Some(table)
}

/// 剔除并报告落在页偏移表体内的 Table F.2 位置：表互不重叠，故这样的位置一旦
/// 出现，对应键就没有可信的解析起点（[`parse_shared`] 因此只会给出
/// [`LinearizationIssue::HintTablePositionInvalid`]，不再叠加解析失败诊断）。
fn drop_positions_inside_page_offset_table(
    table: &PageOffsetHintTable,
    positions: &mut Vec<HintTablePosition>,
    issues: &mut Vec<LinearizationIssue>,
) {
    let end = table.end_byte();
    let inside = positions
        .iter()
        .filter(|entry| entry.position < end)
        .cloned()
        .collect::<Vec<_>>();
    for entry in inside {
        issues.push(LinearizationIssue::HintTablePositionInvalid {
            key: entry.key,
            problem: format!(
                "position {} lies inside the page offset hint table, which occupies the first {end} byte(s)",
                entry.position
            ),
        });
    }
    positions.retain(|entry| entry.position >= end);
}

/// F.4.1 声明的表内 shall：位宽项的 0—32 范围与「首页的共享对象引用数须为 0」。
///
/// 两者都不影响已读出的位（原值一律保留在 [`PageOffsetHintTable`] 里），只做
/// 类型化报告。
fn check_page_offset_semantics(
    table: Option<&PageOffsetHintTable>,
    issues: &mut Vec<LinearizationIssue>,
) {
    let Some(table) = table else { return };
    for (item, bits) in table.width_problems() {
        issues.push(LinearizationIssue::PageOffsetWidthOutOfRange { item, bits });
    }
    if let Some(entry) = table.pages.first()
        && entry.shared_reference_count > 0
    {
        issues.push(LinearizationIssue::PageOffsetFirstPageSharedRefs {
            count: entry.shared_reference_count,
        });
    }
}

/// Table F.2 键 `T`：F.4.3 缩略图 hint 表。
///
/// 键「Present only if thumbnail images exist」（35324）是**条件存在**，缺失即
/// 文档没有缩略图，属合法形态，不给诊断（与 Required 的 `S` 不同）；键存在但
/// 形态不符的已由 [`table_positions`] 报 [`LinearizationIssue::HintTablePositionInvalid`]
/// 并从位置表里剔除，这里不再叠加诊断。
///
/// 读出后仍要核对两条 shall：F.4.3「shall include one or more per-page entries
/// (Table F.8)」（35693—35694）——头部项 3 声明 0 条即「`T` 声称有表、表里却
/// 没有任何条目」，落 [`LinearizationIssue::ThumbnailHintTableMissing`]；位流
/// 截断/非法位宽落 [`LinearizationIssue::ThumbnailTableCorrupt`]。
fn parse_thumbnail(
    positions: &mut Vec<HintTablePosition>,
    data: &[u8],
    issues: &mut Vec<LinearizationIssue>,
) -> Option<ThumbnailHintTable> {
    let entry = positions.iter().find(|entry| entry.key == "T")?;
    let start = usize::try_from(entry.position).ok()?;
    let table = match ThumbnailHintTable::parse(data, start) {
        Ok(table) => table,
        Err(problem) => {
            issues.push(LinearizationIssue::ThumbnailTableCorrupt {
                problem: problem.to_string(),
            });
            return None;
        }
    };
    if table.entries.is_empty() {
        issues.push(LinearizationIssue::ThumbnailHintTableMissing);
    }
    drop_positions_inside_thumbnail_table(&table, positions, issues);
    Some(table)
}

/// F.4 前言：「each hint table shall begin at a byte boundary」（35482—35483）
/// 且各表按 [`ThumbnailHintTable::bits_consumed`] 依次排布，互不重叠。落进
/// 缩略图表体里的 Table F.2 位置自相矛盾（键 `T` 自身给的就是本表起点，不在
/// 此列），剔除后对应表就没有可信的解析起点。
fn drop_positions_inside_thumbnail_table(
    table: &ThumbnailHintTable,
    positions: &mut Vec<HintTablePosition>,
    issues: &mut Vec<LinearizationIssue>,
) {
    let start = table.start_byte;
    let end = table.end_byte();
    let inside = positions
        .iter()
        .filter(|entry| entry.key != "T" && (start..end).contains(&entry.position))
        .cloned()
        .collect::<Vec<_>>();
    for entry in inside {
        issues.push(LinearizationIssue::HintTablePositionInvalid {
            key: entry.key,
            problem: format!(
                "position {} lies inside the thumbnail hint table, which occupies bytes {start}..{end}",
                entry.position
            ),
        });
    }
    positions.retain(|entry| entry.key == "T" || !(start..end).contains(&entry.position));
}

/// F.4.3 声明的表内 shall：Table F.7 位宽项的期望范围（0—32，由 Table F.3 的
/// 35524—35525 按同一措辞类比外推，属**待核**，见 [`thumbnail`] 模块头）。
///
/// 越界的声明值一律保留在 [`ThumbnailHintTable`] 里、表也按声明值读取完毕，
/// 这里只做类型化报告；不猜默认值、不设版本门。
fn check_thumbnail_semantics(
    table: Option<&ThumbnailHintTable>,
    issues: &mut Vec<LinearizationIssue>,
) {
    let Some(table) = table else { return };
    for (item, bits) in table.width_problems() {
        issues.push(LinearizationIssue::ThumbnailWidthOutOfRange { item, bits });
    }
}

/// Table F.2：`S` Required；缺失或解析失败按类型化诊断归入 issues，
/// 并返回可用的解析结果。
fn parse_shared(
    positions: &[HintTablePosition],
    s_declared: bool,
    data: &[u8],
    issues: &mut Vec<LinearizationIssue>,
) -> Option<SharedObjectHintTable> {
    let Some(entry) = positions.iter().find(|entry| entry.key == "S") else {
        if !s_declared {
            issues.push(LinearizationIssue::SharedHintTableMissing);
        }
        return None;
    };
    let start = usize::try_from(entry.position).ok()?;
    match SharedObjectHintTable::parse(data, start) {
        Ok(table) => Some(table),
        Err(problem) => {
            issues.push(LinearizationIssue::HintTableCorrupt {
                key: "S".to_string(),
                problem: problem.to_string(),
            });
            None
        }
    }
}

/// Table F.5 项 4「including the number of shared object entries for the
/// first page (that is, the value of item 3)」：项 3 大于项 4 即自相矛盾。
fn check_header_consistency(
    table: Option<&SharedObjectHintTable>,
    issues: &mut Vec<LinearizationIssue>,
) {
    let Some(table) = table else { return };
    if table.header.first_page_entries > table.header.total_entries {
        issues.push(LinearizationIssue::HintTableCorrupt {
            key: "S".to_string(),
            problem: format!(
                "Table F.5 item 4 ({}) is smaller than item 3 ({}) although it shall include it",
                table.header.total_entries, table.header.first_page_entries
            ),
        });
    }
}
