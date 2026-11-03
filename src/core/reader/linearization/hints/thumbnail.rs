//! 缩略图 hint 表（ISO 32000-1 F.4.3，Table F.7 头部 + Table F.8 逐页条目）。
//!
//! Table F.2 键 `T` 的定位：「(Present only if thumbnail images exist) Thumbnail
//! hint table (see F.4.3, "Thumbnail Hint Table")」（35324—35325）——键缺失是
//! **合法**形态（文档没有缩略图），故本家族不因缺键给诊断。
//!
//! 原文要点（行号＝`pdftotext -raw docs/PDF32000_2008.pdf` 的输出行）：
//! - 结构：「The thumbnail hint table shall consist of a header section (Table
//!   F.7) followed by the thumbnails section, which shall include one or more
//!   per-page entries (Table F.8), each of which describes the thumbnail image
//!   for a single page」（35693—35694）；条目序「The entries shall be in page
//!   number order starting with page 0, even if the document catalogue contains
//!   an OpenAction entry that specifies opening at some page other than page 0.
//!   Thumbnail images may exist for some pages and not for others」
//!   （35695—35697）。
//! - Table F.7 头部 12 项，定宽 32/32/32/16/32/16/32/16/32/32/32/32 = 336 位
//!   整（项 1—9 见 35717—35735，项 10—12 见 35760—35762；原文按页跨栏续排，
//!   故两张「Table F.7 … (continued)」块（35736—35737、35774—35775）之间夹着
//!   Table F.6 的续条目，抽取时须按项号而非行序归位）。
//! - Table F.8 逐页条目 3 项，宽度依次取 Table F.7 项 4/8/6（35765—35773），
//!   列序「The order of items in Table F.8 is as follows: a) Item 1 for all
//!   pages, in page order starting with the first page b) Item 2 … c) Item 3 …」
//!   （35740—35743）：三列各自成一整列，条目内部不连续（与 F.4.1 同一形态）。
//! - 列长取 Table F.7 项 3「The number of pages that have thumbnail images」
//!   （35720）：条目描述的是**有**缩略图的那些页，项 1 用「The number of
//!   preceding pages lacking a thumbnail image … lie between the previous
//!   entry's page and this page」（35765—35767）补回缺失的页。
//! - 增量语义：项 2 加头部项 7 得该页缩略图的对象数，项 3 加头部项 5 得该页
//!   缩略图的长度（字节）（35768—35773），见 [`ThumbnailEntry::object_count`]
//!   与 [`ThumbnailEntry::image_length`]（一律 saturating，绝不因不可信位流溢出）。
//! - 表内位置仍按 F.4 前言 NOTE 3 修正（[`super::LinearizationHints::adjust_position`]）；
//!   头部项 2「The location of the first thumbnail image」（35719）与项 10
//!   「The location of the first object in the thumbnail shared objects
//!   section」（35760）都是文件位置，大于主 hint 流起点者须补流长。
//! - 头部项 9—12 是**缩略图共享对象区**（part 9 的子段，见 F.3.10「Thumbnail
//!   shared objects … shall not be referenced from any other objects」
//!   35436—35437；表内定义 35729—35735、35760—35762）。F.4.3 没有规定这些值
//!   越界或自相矛盾时的读法，本切片**只原样保留声明值**、不做定位、不做交叉
//!   核对。
//!
//! **待核**（一律取保守行为：保留原值、不设版本门、不做派生除法）：
//! - 位宽范围：「All the items in Table F.3 that specify a number of bits
//!   needed, such as item 3, have values in the range 0 through 32. Although
//!   that range requires only 6 bits, 16-bit numbers shall be used」
//!   （35524—35525）**字面只约束 Table F.3**。Table F.7 项 4/6/8 用的是同一
//!   措辞（「The number of bits needed to represent …」35721—35728）与同一 16
//!   位存储约定，故按**类比**把 0—32 当作期望范围，由
//!   [`ThumbnailHintTable::width_problems`] 如实上报；越界不猜默认值、仍按声明
//!   宽度继续读取并原样保留。声明宽度超过 64 位原语上限时由 [`BitReader`] 直接
//!   给 [`HintBitError::InvalidWidth`]（读不动，交上层落类型化诊断）。
//! - Table F.8 项 1 标注「(Optional)」（35765）：原文未说明「省略」在位流里如何
//!   标示，而 35741 明确「Item 1 for all pages」且宽度由头部项 4 给出，故按
//!   **每项恒在、声明宽度为 0 即消费 0 位**处理（与 F.4.1 的 0 宽字段同口径）。
//! - 与页偏移表的关系：逐项核对 Table F.7 项 1—12 与 Table F.8 项 1—3 的引用
//!   目标，**只有本表自身**（35765—35773 全写「See Table F.7, item …」），F.4.3
//!   未出现 Table F.3 字样（附录 F 里引用 Table F.3 的是 F.4.2 项 1
//!   35678—35680 与 F.4.5 的 35815、35847）。故这里**不发明**任何「与页偏移
//!   头部交叉核对」的检查；两家只受 F.4 前言的位置规则相互约束：本表须以字节
//!   边界开始（35482—35483）、且起点不得低于页偏移表末字节，反向地，别的表位置
//!   落进本表体里同样自相矛盾——两条都由 [`super`] 的剔除逻辑给类型化诊断。

use super::bits::{BitReader, HintBitError};
use super::page_offset::{MAX_DECLARED_WIDTH, MAX_UNEVIDENCED_ITEMS, column, width};

/// Table F.7：缩略图 hint 表头部（12 项，定宽 9×32 + 3×16 = 336 位）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThumbnailHeader {
    /// 项 1（32 位）：第一张缩略图（＝缩略图区首条目对应的图）的对象号。
    pub first_thumbnail_object_number: u64,
    /// 项 2（32 位）：第一张缩略图的位置。
    pub first_thumbnail_object_location: u64,
    /// 项 3（32 位）：**有**缩略图的页数，即 Table F.8 的条目数。
    pub pages_with_thumbnails: u64,
    /// 项 4（16 位）：表示「连续无缩略图页数最大值」所需的位数（Table F.8 项 1 宽度）。
    pub gap_count_bits: u32,
    /// 项 5（32 位）：一张缩略图的最少长度（字节）。
    pub least_image_length: u64,
    /// 项 6（16 位）：表示缩略图长度最大差值所需的位数（Table F.8 项 3 宽度）。
    pub image_length_bits: u32,
    /// 项 7（32 位）：一张缩略图的最少对象数。
    pub least_object_count: u64,
    /// 项 8（16 位）：表示缩略图对象数最大差值所需的位数（Table F.8 项 2 宽度）。
    pub object_count_bits: u32,
    /// 项 9（32 位）：缩略图共享对象区（part 9 的子段）第一个对象的对象号。
    pub first_shared_object_number: u64,
    /// 项 10（32 位）：缩略图共享对象区第一个对象的位置。
    pub first_shared_object_location: u64,
    /// 项 11（32 位）：缩略图共享对象区里的对象数。
    pub shared_object_count: u64,
    /// 项 12（32 位）：缩略图共享对象区的长度（字节）。
    pub shared_section_length: u64,
}

impl ThumbnailHeader {
    /// Table F.7 里声明「number of bits needed」的 3 项（项 4/6/8）。
    fn bit_widths(&self) -> [(&'static str, u32); 3] {
        [
            ("item 4", self.gap_count_bits),
            ("item 6", self.image_length_bits),
            ("item 8", self.object_count_bits),
        ]
    }

    /// 每个条目至少消费的位数：Table F.8 项 1/2/3 的宽度之和。
    fn bits_per_entry(&self) -> u64 {
        (self.gap_count_bits as u64)
            .saturating_add(self.object_count_bits as u64)
            .saturating_add(self.image_length_bits as u64)
    }
}

/// Table F.8：一个逐页条目（3 项；宽度全部来自 Table F.7）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThumbnailEntry {
    /// 项 1：本条目前**没有**缩略图的页数（原文标 (Optional)，见模块头待核）。
    pub preceding_pages_without_thumbnail: u64,
    /// 项 2：对象数增量；该页缩略图对象数 = 本值 + 头部项 7。
    pub object_count_delta: u64,
    /// 项 3：长度增量；该页缩略图长度 = 本值 + 头部项 5（字节）。
    pub image_length_delta: u64,
}

impl ThumbnailEntry {
    /// Table F.8 项 2 + Table F.7 项 7：该页缩略图的对象数。
    pub fn object_count(&self, header: &ThumbnailHeader) -> u64 {
        header
            .least_object_count
            .saturating_add(self.object_count_delta)
    }

    /// Table F.8 项 3 + Table F.7 项 5：该页缩略图的长度（字节）。
    pub fn image_length(&self, header: &ThumbnailHeader) -> u64 {
        header
            .least_image_length
            .saturating_add(self.image_length_delta)
    }
}

/// F.4.3 缩略图 hint 表：Table F.7 头部 + Table F.8 的条目列（按页序）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbnailHintTable {
    /// Table F.7 头部（12 项原样保留，含未建模的缩略图共享对象区 4 项）。
    pub header: ThumbnailHeader,
    /// 本表在主 hint 流位流里的起始字节偏移，即 Table F.2 键 `T` 的值。
    pub start_byte: u64,
    /// 条目数＝头部项 3 的原值（保留便于溯源，包括它超出剩余位预算的形态）。
    pub declared_entries: u64,
    /// 逐页条目，按页号序（首条目对应**页号最小**的那张缩略图，35695—35697）。
    pub entries: Vec<ThumbnailEntry>,
    /// 本表消费掉的位数（自表首字节起）；F.4 前言的「表以字节边界开始且互不
    /// 重叠」核对据此折算后续表的最小位置。
    pub bits_consumed: u64,
}

impl ThumbnailHintTable {
    /// 从 `data`（主/溢出 hint 流解码拼接后的位流）的字节偏移 `start` 处解析；
    /// `start` 由主 hint 流字典的 Table F.2 键 `T` 给出（F.4.3 不钉死起点，
    /// 但 F.4 前言要求表以字节边界开始，35482—35483）。
    pub fn parse(data: &[u8], start: usize) -> Result<Self, HintBitError> {
        if start > data.len() {
            return Err(HintBitError::ShortBuffer {
                needed_bits: 8,
                remaining_bits: 0,
                position_bits: start as u64 * 8,
            });
        }
        let mut reader = BitReader::new(&data[start..]);
        let header = ThumbnailHeader {
            first_thumbnail_object_number: reader.unsigned(32)?,
            first_thumbnail_object_location: reader.unsigned(32)?,
            pages_with_thumbnails: reader.unsigned(32)?,
            gap_count_bits: width(reader.unsigned(16)?),
            least_image_length: reader.unsigned(32)?,
            image_length_bits: width(reader.unsigned(16)?),
            least_object_count: reader.unsigned(32)?,
            object_count_bits: width(reader.unsigned(16)?),
            first_shared_object_number: reader.unsigned(32)?,
            first_shared_object_location: reader.unsigned(32)?,
            shared_object_count: reader.unsigned(32)?,
            shared_section_length: reader.unsigned(32)?,
        };
        // 条目数是不可信的 32 位声明值：进入任何分配前先按位预算封顶。三项
        // 宽度全为 0 时条目不占位，位预算给不出页数证据，改用绝对上限。
        let declared = header.pages_with_thumbnails;
        let count = usize::try_from(declared)
            .ok()
            .filter(|count| fits(&header, *count, reader.remaining_bits()));
        let Some(count) = count else {
            return Err(HintBitError::ShortBuffer {
                needed_bits: header.bits_per_entry().saturating_mul(declared),
                remaining_bits: reader.remaining_bits(),
                position_bits: reader.position_bits(),
            });
        };
        // a) 项 1 → b) 项 2 → c) 项 3：三列都是「for all pages, in page order」。
        let gaps = column(&mut reader, count, header.gap_count_bits)?;
        let object_deltas = column(&mut reader, count, header.object_count_bits)?;
        let length_deltas = column(&mut reader, count, header.image_length_bits)?;
        let bits_consumed = reader.position_bits();
        let entries = gaps
            .into_iter()
            .zip(object_deltas)
            .zip(length_deltas)
            .map(|((gap, objects), length)| ThumbnailEntry {
                preceding_pages_without_thumbnail: gap,
                object_count_delta: objects,
                image_length_delta: length,
            })
            .collect();
        Ok(Self {
            header,
            start_byte: start as u64,
            declared_entries: declared,
            entries,
            bits_consumed,
        })
    }

    /// 本表末尾之后的第一个字节（流内绝对位置；表不重叠，故 Table F.2 其他键
    /// 的位置不得低于此值，也不得等于本表起点）。
    pub fn end_byte(&self) -> u64 {
        self.start_byte
            .saturating_add(self.bits_consumed.div_ceil(8))
    }

    /// Table F.7 声明位宽越出期望范围（0—32，按 Table F.3 类比，见模块头待核）
    /// 的项（项名, 声明值）。
    pub fn width_problems(&self) -> Vec<(&'static str, u64)> {
        self.header
            .bit_widths()
            .iter()
            .filter(|(_, bits)| *bits > MAX_DECLARED_WIDTH)
            .map(|(item, bits)| (*item, *bits as u64))
            .collect()
    }

    /// 条目 `index` 对应的**页号**（自 0 起）：按项 1 的定义累加前导的无缩略图
    /// 页（35765—35767 与 35695—35697 的序约定）。这是由项 1 定义**推出的**值，
    /// 非规范条款；越界索引返回 `None`，累加一律 saturating（项 1 的声明宽度可
    /// 达 64 位，绝不溢出 panic）。
    pub fn page_of(&self, index: usize) -> Option<u64> {
        let entry = self.entries.get(index)?;
        let leading = self.entries[..index].iter().fold(0u64, |sum, previous| {
            sum.saturating_add(previous.preceding_pages_without_thumbnail)
                .saturating_add(1)
        });
        Some(leading.saturating_add(entry.preceding_pages_without_thumbnail))
    }
}

/// 条目数是否落在剩余位预算内；宽度全为 0 时按 [`MAX_UNEVIDENCED_ITEMS`] 封顶。
fn fits(header: &ThumbnailHeader, entries: usize, remaining_bits: u64) -> bool {
    let each = header.bits_per_entry();
    let Ok(count) = u64::try_from(entries) else {
        return false;
    };
    if each == 0 {
        return count <= MAX_UNEVIDENCED_ITEMS;
    }
    each.saturating_mul(count) <= remaining_bits
}
