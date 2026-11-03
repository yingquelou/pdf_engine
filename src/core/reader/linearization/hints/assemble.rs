//! 附录 F.4 hint 表读端的**装配层**：自 [`LinearizationInfo`] 出发，读主/溢出
//! hint 流 → 按 `/Filter` 解码 → 无缝拼接（F.3.6）→ 收集主 hint 流字典里的
//! Table F.2 键位置 → 按家族逐个解析 → 汇总类型化诊断。
//!
//! 本文件是 [`super`] 的**零行为变更**搬运拆分：`impl Document` 的三个方法与
//! 全部按 Table F.2 键分发的私有函数逐行移到这里，公开入口
//! `Document::linearization_hints` 的签名、语义与诊断集合完全不变。拆出的理由
//! 是文件红线（600 行 / 25600 字节）：F.4 的家族还在逐个补齐，分发逻辑须先有
//! 自己的文件，否则 `hints/mod.rs` 无法再容纳新表接线。

use crate::core::filters::decode_stream;
use crate::core::types::{Dict, Document, PdfVersion, Value};

use super::super::LinearizationInfo;
use super::super::checks::read_object_at;
use super::super::dict::HintRange;
use super::super::issues::LinearizationIssue;
use super::extended::{check_extended_shared_id_widths, group_hint_keys, parse_extended};
use super::{
    GenericHintTable, HintTablePosition, LinearizationHints, PAGE_OFFSET_TABLE_POSITION,
    PageOffsetHintTable, STANDARD_HINT_KEYS, SharedObjectHintTable, ThumbnailHintTable,
    page_offset_derive,
};

impl Document {
    /// 类型化解析附录 F.4 的 hint 表（读端；当前完整解析 F.4.1 页偏移表、
    /// F.4.2 共享对象 hint 表、F.4.3 缩略图 hint 表、F.4.4 generic hint 表与
    /// F.4.5 extended generic hint 表，其余家族见模块头的诚实缺口）。
    ///
    /// 返回 `Ok(None)` 表示**无 hint 可解**：文档非线性化、缺 `/H`，或
    /// 主 hint 流对象完全不可读（此时不给诊断——[`Document::linearization`]
    /// 已在结构层报告缺 `/H` 等条目问题）。`Ok(Some)` 的
    /// [`LinearizationHints::issues`] 只含 hint 层诊断（Table F.2 键缺失、
    /// 位置越界、位流截断等）。
    pub fn linearization_hints(&self) -> super::super::Result<Option<LinearizationHints>> {
        let Some(info) = self.linearization()? else {
            return Ok(None);
        };
        Ok(self.hint_data(&info))
    }

    /// 从已识别的线性化模型出发解码 hint 表；`/H` 缺失返回 `None`。
    fn hint_data(&self, info: &LinearizationInfo) -> Option<LinearizationHints> {
        let streams = info.hint_streams?;
        let mut issues = Vec::new();
        // Table F.4 项 1 的锚点：参数字典已解析的 `/O`，这里只做类型收敛。
        let first_page_object = info
            .first_page_object
            .and_then(|number| u64::try_from(number).ok());
        let (dict, mut data) = match self.read_hint_stream(streams.primary) {
            Ok(part) => part,
            Err(problem) => {
                issues.push(LinearizationIssue::HintStreamUnreadable {
                    which: "primary",
                    problem,
                });
                return Some(LinearizationHints {
                    primary: streams.primary,
                    first_page_object,
                    stream_bytes: 0,
                    table_positions: Vec::new(),
                    page_offset: None,
                    shared_object: None,
                    thumbnail: None,
                    generic: Vec::new(),
                    extended: Vec::new(),
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
        // F.4.4：键 `O`/`A`/`E`/`I`/`L` 各描述一个对象组，表体的字节范围同样
        // 参与「表互不重叠」的位置剔除，故须排在 F.4.2 之前——`S` 的位置一旦
        // 落进 generic 表体就没有可信的解析起点。声明版本 < 1.5 时 `V`/`C`/`R`
        // 也并进来按 Table F.9 读（F.4.5 的版本门，见 [`group_hint_keys`]）。
        let version = self.source_version();
        let generic = parse_generic(&mut table_positions, &data, version, &mut issues);
        // F.4.5：`V`/`C`/`R` 的 extended 表（Table F.10）长度随项 5/项 6 变化，
        // 同样参与位置剔除，故排在 `S` 之前。
        let extended = parse_extended(&mut table_positions, &data, version, &mut issues);
        // 键存在但形态不符时不再叠加「缺 S」诊断。
        let s_declared = dict.get("S").is_some();
        let shared_object = parse_shared(&table_positions, s_declared, &data, &mut issues);
        check_header_consistency(shared_object.as_ref(), &mut issues);
        check_page_offset_semantics(page_offset.as_ref(), &mut issues);
        check_extended_shared_id_widths(page_offset.as_ref(), &extended, &mut issues);
        page_offset_derive::check_page_offset_derivations(
            page_offset.as_ref(),
            first_page_object,
            &mut issues,
        );
        Some(LinearizationHints {
            primary: streams.primary,
            first_page_object,
            stream_bytes: data.len(),
            table_positions,
            page_offset,
            shared_object,
            thumbnail,
            generic,
            extended,
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

/// Table F.2 键 `O`/`A`/`E`/`I`/`L`：F.4.4 的 generic hint 表（Table F.9）。
///
/// 五键都描述「单个对象组」（35749—35751），且都是**条件存在**——`O`「Present
/// only if a document outline exists」、`A`「… article threads exist」、`E`「…
/// named destinations exist」、`I`「… document information dictionary exist」
/// （35326—35336），`L` 标 PDF 1.3（35338）。缺键即该类对象没给 hint，属合法
/// 形态，**不给诊断**（与 `T` 同口径，与 Required 的 `S` 不同），故返回值可以
/// 少于五项；[`super::LinearizationHints::generic_group_of`] 对缺键退回
/// `None` 而非空组。
///
/// 键在而表读不满 4×32 位（截断、起点到流尾不够）落
/// [`LinearizationIssue::GenericHintTableCorrupt`]，该键不列条目；键在但形态
/// 不符（非整数/负值/越出解码后的流）已由 [`table_positions`] 报
/// [`LinearizationIssue::HintTablePositionInvalid`] 并从位置表里剔除，这里
/// 不再叠加诊断。产品专有表（F.4.4「may be used for product-specific objects」
/// 35757）须走 Annex E 注册键，本库不猜未知键的形态，故只按标准名单分发。
///
/// 逐键「先解析、随即用本表体去剔除后续键的位置」：generic 一家有五个（声明
/// 版本 < 1.5 时连 `V`/`C`/`R` 一起是八个）键，若先把它们全解析完再剔除，落进
/// 前一张表体里的位置也会先被当成解析起点读一遍
/// （读到的是别人的表体字节，既假阳性也假阴性）。故本函数按
/// [`group_hint_keys`] 给出的键序遍历（即 [`super::generic::GENERIC_HINT_KEYS`]
/// 的名册序，< 1.5 时再并入 `V`/`C`/`R`），每读成一张表就调用
/// [`drop_positions_inside_generic_table`]，末尾只保留位置仍然存活的键——
/// [`super::LinearizationHints::generic_group_of`] 与
/// [`super::LinearizationHints::position_of`] 因此始终同进退。
fn parse_generic(
    positions: &mut Vec<HintTablePosition>,
    data: &[u8],
    version: PdfVersion,
    issues: &mut Vec<LinearizationIssue>,
) -> Vec<GenericHintTable> {
    let mut tables = Vec::new();
    for key in group_hint_keys(version).iter().copied() {
        let Some(position) = positions
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.position)
        else {
            continue;
        };
        // 位置已由 [`table_positions`] 收敛到 `0..=stream_len`，故换算 `usize`
        // 不会失败；失败时只跳过本键，不猜起点。
        let Ok(start) = usize::try_from(position) else {
            continue;
        };
        match GenericHintTable::parse(data, start, key) {
            Ok(table) => {
                drop_positions_inside_generic_table(&table, positions, issues);
                tables.push(table);
            }
            Err(problem) => issues.push(LinearizationIssue::GenericHintTableCorrupt {
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
/// 且各表按 NOTE 2 依次排布、互不重叠。generic 表长恒 16 字节（Table F.9 四项
/// 定宽 32 位），故其他 Table F.2 位置落进 `[本键, 本键 + 16)` 即自相矛盾；
/// 本键自身的起点不在此列。两个 generic 键共用同一位置不视为矛盾——规范没有
/// 禁止同一张表兼任两类 hint，此处不发明 shall。
fn drop_positions_inside_generic_table(
    table: &GenericHintTable,
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
                "position {} lies inside the generic hint table of key {}, which occupies bytes {start}..{end}",
                entry.position, table.key
            ),
        });
    }
    positions.retain(|entry| entry.key == table.key || !(start..end).contains(&entry.position));
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
