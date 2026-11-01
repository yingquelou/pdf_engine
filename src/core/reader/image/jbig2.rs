//! JBIG2Decode 过滤器（ISO 32000-1 §7.4.7 / ITU-T T.88）的段语法层解析。
//!
//! 本模块**只遍历 JBIG2 段链**读取元数据，不解码任何像素：
//! - 段头（T.88 §7.2）：段号、段旗标（类型位 `& 0x3F`、4 字节页关联位
//!   `0x40`、延迟非保留位 `0x80`）、被引用段表（短形式 TP=0–6；TP=7 长
//!   形式以本字节起始的 u32 低 29 位为个数，随后 `ceil(count/8)` 字节
//!   保留位图）、段页关联（1 或 4 字节，字段宽度取决于当前段号：
//!   ≤256 为 1 字节、≤65536 为 2 字节、否则 4 字节）、未压缩数据长度；
//! - 页面信息段（T.88 §7.4.8）：宽高、分辨率、页面合成旗标（BPC 字节）
//!   与条带信息（BPIFlags/BPIExtension 组成 u16，bit15 条带编码）；
//! - PDF 规则（§7.4.7）：JBIG2 文件头、end-of-page 段与 end-of-file 段
//!   shall not be used，图像流内段页关联应为 1，全局段（页关联 0）应
//!   另置于 `/JBIG2Globals` 流。遇到违例一律记为类型化一致性发现
//!   [`Jbig2Finding`]（「存在但禁用」），不静默丢弃任何段。
//!
//! **实现边界（刻意的设计约束，与 JPX 同一立场）**：
//! - 不做像素解码：无算术/MR/MMR/Huffman 熵解码、无符号字典重建、无
//!   区域合成，[`super::ImageModel::samples`] 对 JBIG2 图像返回类型化
//!   `Unsupported`；
//! - 除页面信息段外，其余段类型的载荷按原始字节保留，不解释内部语法
//!   （文本区域 NBIL、半调网格等字段均未建模）；
//! - `0xFFFFFFFF` 未指定长度的终止符搜索与 TP=5/6 预留长形式不支持：
//!   前者记录 [`Jbig2Finding::SegmentLengthUnspecified`] 后干净停止；
//! - 多页共享/随机访问文件组织不属于 PDF 的嵌入式组织范围：文件头按
//!   签名与旗标字节识别、跳过并记录发现，不校验组织记录。
//!
//! 截断处理：段头不完整返回类型化 [`Jbig2Error::TruncatedSegmentHeader`]
//! （绝不 panic）；段数据不足记录 [`Jbig2Finding::SegmentDataTruncated`]
//! 并干净停止，已解析的段照常返回。

use std::fmt;

/// JBIG2 文件头签名（T.88 §6.2：`97 4A 42 32 0D 0A 1A 0A`）。
const FILE_SIGNATURE: [u8; 8] = [0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A];

/// 页面信息段载荷的核心长度（4×u32 + BPC + BPIFlags = 18 字节）。
const PAGE_INFORMATION_MIN_LEN: usize = 18;

/// 页面信息段载荷的规范完整长度（再加 BPIExtension = 19 字节）。
const PAGE_INFORMATION_FULL_LEN: usize = 19;

/// 段类型（段旗标低 6 位，T.88 §7.2.3）；未登记取值保留在
/// [`Jbig2SegmentKind::Unknown`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Jbig2SegmentKind {
    /// 0：符号字典段。
    SymbolDictionary,
    /// 4：中间文本区域段。
    IntermediateTextRegion,
    /// 6：立即文本区域段。
    ImmediateTextRegion,
    /// 7：立即无损文本区域段。
    ImmediateLosslessTextRegion,
    /// 16：图案字典段。
    PatternDictionary,
    /// 20：中间半调区域段。
    IntermediateHalftoneRegion,
    /// 22：立即半调区域段。
    ImmediateHalftoneRegion,
    /// 23：立即无损半调区域段。
    ImmediateLosslessHalftoneRegion,
    /// 36：中间通用区域段。
    IntermediateGenericRegion,
    /// 38：立即通用区域段。
    ImmediateGenericRegion,
    /// 39：立即无损通用区域段。
    ImmediateLosslessGenericRegion,
    /// 40：中间通用调精区域段。
    IntermediateGenericRefinementRegion,
    /// 42：立即通用调精区域段。
    ImmediateGenericRefinementRegion,
    /// 43：立即无损通用调精区域段。
    ImmediateLosslessGenericRefinementRegion,
    /// 48：页面信息段（§7.4.8，本层唯一解释载荷的段类型）。
    PageInformation,
    /// 49：页结束段（PDF 禁用）。
    EndOfPage,
    /// 50：条带结束段。
    EndOfStripe,
    /// 51：文件结束段（PDF 禁用）。
    EndOfFile,
    /// 52：条纹剖面段（JBIG1 模板）。
    Profiles,
    /// 53：用户提供 Huffman 表段。
    Tables,
    /// 62：扩展段。
    Extension,
    /// 规范未登记（或本模块未命名）的原始 6 位取值。
    Unknown(u8),
}

impl Jbig2SegmentKind {
    /// 按段旗标低 6 位映射段类型；未知取值原样保留。
    pub fn from_segment_type(value: u8) -> Self {
        match value {
            0 => Self::SymbolDictionary,
            4 => Self::IntermediateTextRegion,
            6 => Self::ImmediateTextRegion,
            7 => Self::ImmediateLosslessTextRegion,
            16 => Self::PatternDictionary,
            20 => Self::IntermediateHalftoneRegion,
            22 => Self::ImmediateHalftoneRegion,
            23 => Self::ImmediateLosslessHalftoneRegion,
            36 => Self::IntermediateGenericRegion,
            38 => Self::ImmediateGenericRegion,
            39 => Self::ImmediateLosslessGenericRegion,
            40 => Self::IntermediateGenericRefinementRegion,
            42 => Self::ImmediateGenericRefinementRegion,
            43 => Self::ImmediateLosslessGenericRefinementRegion,
            48 => Self::PageInformation,
            49 => Self::EndOfPage,
            50 => Self::EndOfStripe,
            51 => Self::EndOfFile,
            52 => Self::Profiles,
            53 => Self::Tables,
            62 => Self::Extension,
            other => Self::Unknown(other),
        }
    }

    /// 原始 6 位段旗标取值（`Unknown` 保留其自身值）。
    pub fn segment_type(&self) -> u8 {
        match self {
            Self::SymbolDictionary => 0,
            Self::IntermediateTextRegion => 4,
            Self::ImmediateTextRegion => 6,
            Self::ImmediateLosslessTextRegion => 7,
            Self::PatternDictionary => 16,
            Self::IntermediateHalftoneRegion => 20,
            Self::ImmediateHalftoneRegion => 22,
            Self::ImmediateLosslessHalftoneRegion => 23,
            Self::IntermediateGenericRegion => 36,
            Self::ImmediateGenericRegion => 38,
            Self::ImmediateLosslessGenericRegion => 39,
            Self::IntermediateGenericRefinementRegion => 40,
            Self::ImmediateGenericRefinementRegion => 42,
            Self::ImmediateLosslessGenericRefinementRegion => 43,
            Self::PageInformation => 48,
            Self::EndOfPage => 49,
            Self::EndOfStripe => 50,
            Self::EndOfFile => 51,
            Self::Profiles => 52,
            Self::Tables => 53,
            Self::Extension => 62,
            Self::Unknown(other) => *other,
        }
    }

    /// ISO/IEC 14495-1 的段类型名（与 pdf.js `SegmentTypes` 同一口径）。
    pub fn name(&self) -> &'static str {
        match self {
            Self::SymbolDictionary => "SymbolDictionary",
            Self::IntermediateTextRegion => "IntermediateTextRegion",
            Self::ImmediateTextRegion => "ImmediateTextRegion",
            Self::ImmediateLosslessTextRegion => "ImmediateLosslessTextRegion",
            Self::PatternDictionary => "PatternDictionary",
            Self::IntermediateHalftoneRegion => "IntermediateHalftoneRegion",
            Self::ImmediateHalftoneRegion => "ImmediateHalftoneRegion",
            Self::ImmediateLosslessHalftoneRegion => "ImmediateLosslessHalftoneRegion",
            Self::IntermediateGenericRegion => "IntermediateGenericRegion",
            Self::ImmediateGenericRegion => "ImmediateGenericRegion",
            Self::ImmediateLosslessGenericRegion => "ImmediateLosslessGenericRegion",
            Self::IntermediateGenericRefinementRegion => "IntermediateGenericRefinementRegion",
            Self::ImmediateGenericRefinementRegion => "ImmediateGenericRefinementRegion",
            Self::ImmediateLosslessGenericRefinementRegion => {
                "ImmediateLosslessGenericRefinementRegion"
            }
            Self::PageInformation => "PageInformation",
            Self::EndOfPage => "EndOfPage",
            Self::EndOfStripe => "EndOfStripe",
            Self::EndOfFile => "EndOfFile",
            Self::Profiles => "Profiles",
            Self::Tables => "Tables",
            Self::Extension => "Extension",
            Self::Unknown(_) => "Unknown",
        }
    }

    /// §7.4.7：这三类「文件级」段（连同 [`Jbig2Finding::FileHeaderPresent`]
    /// 对应的文件头）不得出现在 PDF 里。
    pub fn is_forbidden_in_pdf(&self) -> bool {
        matches!(self, Self::EndOfPage | Self::EndOfFile)
    }
}

/// 单个 JBIG2 段：完整段头字段 + 原始载荷字节（不解释）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jbig2Segment {
    /// 段号（u32）。
    pub number: u32,
    /// 映射后的段类型。
    pub kind: Jbig2SegmentKind,
    /// 段旗标低 6 位原始值（`kind` 为 `Unknown` 时保留）。
    pub segment_type: u8,
    /// 段旗标字节原值（T.88 §7.2.3）。
    pub flags: u8,
    /// 延迟非保留旗标（bit 7）。
    pub deferred_non_retain: bool,
    /// 被引用段号表（字段宽度依当前段号而定）。
    pub referred_to_segments: Vec<u32>,
    /// 被引用个数是否取自 TP=7 长形式（32 位个数字段）。
    pub referred_count_extended: bool,
    /// 保留位图指示：短形式为本字节低 5 位；长形式为首个保留字节。
    pub retained_bits_indicator: u8,
    /// 长形式的保留位图字节；短形式为空。
    pub retained_bytes: Vec<u8>,
    /// 段页关联（0 = 全局段）。
    pub page_association: u32,
    /// 页关联字段是否按旗标 `0x40` 取 4 字节。
    pub page_association_long: bool,
    /// 段头声明的未压缩数据长度（0xFFFFFFFF 表示未指定）。
    pub data_size: u32,
    /// 实际读取到的载荷字节（截断时少于 `data_size`）。
    pub data: Vec<u8>,
    /// 段头自身长度（最小 11，随引用表与页关联宽度增长）。
    pub header_len: usize,
}

impl Jbig2Segment {
    /// 是否全局段（页关联 0，§7.4.7 要求置于 `/JBIG2Globals` 流）。
    pub fn is_global(&self) -> bool {
        self.page_association == 0
    }
}

/// 页面信息段（T.88 §7.4.8）载荷解析结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Jbig2PageInfo {
    /// 页宽（像素）。
    pub width: u32,
    /// 页高（像素）。
    pub height: u32,
    /// 水平分辨率（像素/米；0 表示未指定）。
    pub x_resolution: u32,
    /// 垂直分辨率（像素/米；0 表示未指定）。
    pub y_resolution: u32,
    /// BPC 页合成旗标字节原值。
    pub flags: u8,
    /// bit 0：无损编码。
    pub lossless: bool,
    /// bit 1：使用调精。
    pub refinement: bool,
    /// bit 2：页边框默认像素值。
    pub default_pixel: u8,
    /// bit 4–3：区域合成算子（0=OR、1=XNOR、2=XOR、3=AND）。
    pub combination_operator: u8,
    /// bit 5：需要组合缓冲。
    pub requires_buffer: bool,
    /// bit 6：合成算子覆盖段级取值。
    pub combination_operator_override: bool,
    /// bit 7：彩色页（T.88 Amendment 3）。
    pub colored: bool,
    /// BPIFlags 字节原值（条带信息高字节）。
    pub bpi_flags: u8,
    /// BPIExtension 字节；载荷不足 19 字节时为 `None`。
    pub bpi_extension: Option<u8>,
    /// 条带编码（BPIFlags bit 7）。
    pub striped: bool,
    /// 条带高度（`(BPIFlags & 0x7F) << 8 | BPIExtension`）；仅条带编码
    /// 且 BPIExtension 可读时非 `None`。
    pub stripe_height: Option<u16>,
}

impl Jbig2PageInfo {
    /// 合成算子名（T.88 Table 16 的取值 0–3）。
    pub fn combination_operator_name(&self) -> &'static str {
        match self.combination_operator {
            0 => "OR",
            1 => "XNOR",
            2 => "XOR",
            3 => "AND",
            _ => "Unknown",
        }
    }
}

/// 语法/一致性违例的类型化发现（诊断数据，不改变解析结果）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Jbig2Finding {
    /// §7.4.7：JBIG2 文件头在 PDF 中不得使用；识别并跳过后记录。
    /// `flags`/`random_access`/`declared_pages` 在文件头自身截断时为 `None`。
    FileHeaderPresent {
        position: usize,
        flags: Option<u8>,
        random_access: Option<bool>,
        declared_pages: Option<u32>,
    },
    /// §7.4.7：end-of-page 段（49）在 PDF 中不得使用，但确实存在。
    EndOfPageSegmentPresent { segment_number: u32 },
    /// §7.4.7：end-of-file 段（51）在 PDF 中不得使用，但确实存在。
    EndOfFileSegmentPresent { segment_number: u32 },
    /// §7.4.7：段声明长度 `0xFFFFFFFF`（未指定），本层不做终止符搜索。
    SegmentLengthUnspecified { segment_number: u32 },
    /// 段载荷短于声明长度：保留可读部分并干净停止。
    SegmentDataTruncated {
        segment_number: u32,
        declared: u32,
        available: usize,
    },
    /// 页面信息段载荷不足规范 19 字节。
    PageInformationSegmentTooShort {
        segment_number: u32,
        available: usize,
    },
    /// 图像流内出现页关联 0 的全局段（应另置于 `/JBIG2Globals` 流）。
    GlobalSegmentInImageStream { segment_number: u32 },
    /// 图像流内段页关联大于 1（§7.4.7：图像 XObject 段应恒为 1）。
    UnexpectedPageAssociation {
        segment_number: u32,
        page_association: u32,
    },
    /// 图像流段引用了本流不存在的段号且没有 `/JBIG2Globals`。
    Jbig2GlobalsMissing { dangling_reference_count: usize },
    /// `/JBIG2Globals` 取值不是可解析的流。
    Jbig2GlobalsNotAStream { value_type: String },
    /// `/JBIG2Globals` 流无法过滤解码或段链无法解析。
    Jbig2GlobalsUnparsable(Jbig2Error),
    /// 图像流段链在段头处截断：段列表只保留可解析部分之前的结果。
    UnparsableSegmentChain(Jbig2Error),
}

/// JBIG2 段链解析的失败原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Jbig2Error {
    /// 段头不完整：`position` 处只剩 `remaining` 字节，至少需要 `required`
    /// 个字节才能读完该段头。
    TruncatedSegmentHeader {
        position: usize,
        remaining: usize,
        required: usize,
    },
    /// `/JBIG2Globals` 流的过滤器解码失败（消息透传）。
    GlobalsStreamDecode(String),
}

impl fmt::Display for Jbig2Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TruncatedSegmentHeader {
                position,
                remaining,
                required,
            } => write!(
                f,
                "truncated JBIG2 segment header at byte {position}: {remaining} byte(s) \
                 available, at least {required} required"
            ),
            Self::GlobalsStreamDecode(message) => {
                write!(f, "/JBIG2Globals stream decode failed: {message}")
            }
        }
    }
}

impl std::error::Error for Jbig2Error {}

/// 一段 JBIG2 段链（图像流或全局段流）的解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jbig2Stream {
    /// 依序解析出的段（「存在但禁用」的段同样保留，不静默丢弃）。
    pub segments: Vec<Jbig2Segment>,
    /// 解析过程中记录的类型化发现。
    pub findings: Vec<Jbig2Finding>,
    /// 链上首个可解析的页面信息段载荷（§7.4.8）。
    pub page_information: Option<Jbig2PageInfo>,
}

impl Jbig2Stream {
    /// 页面信息段元数据。
    pub fn page_info(&self) -> Option<&Jbig2PageInfo> {
        self.page_information.as_ref()
    }

    /// 全局段（页关联 0）个数。
    pub fn global_segment_count(&self) -> usize {
        self.segments
            .iter()
            .filter(|segment| segment.is_global())
            .count()
    }

    /// 页段（页关联非 0）个数。
    pub fn page_segment_count(&self) -> usize {
        self.segments
            .iter()
            .filter(|segment| !segment.is_global())
            .count()
    }

    /// 页关联大于 1 的段（(段号, 页关联) 列表）；为空表示只出现 0/1。
    pub fn unexpected_page_associations(&self) -> Vec<(u32, u32)> {
        self.segments
            .iter()
            .filter(|segment| segment.page_association > 1)
            .map(|segment| (segment.number, segment.page_association))
            .collect()
    }
}

/// 单个图像 XObject 的 JBIG2 元数据视图：图像流段链、`/JBIG2Globals`
/// 全局段流与图像级一致性发现（§7.4.7）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jbig2ImageInfo {
    /// 图像流自身的段链（含文件头/页尾/文件尾段等「存在但禁用」项）。
    pub page: Jbig2Stream,
    /// `/JBIG2Globals` 全局段流的段链；参数缺失或不可解析时为 `None`。
    pub globals: Option<Jbig2Stream>,
    /// `/JBIG2Globals` 若为间接引用，记录其 (对象号, 代数)。
    pub globals_reference: Option<(i64, i64)>,
    /// 图像级一致性发现（页关联、全局段放置、`/JBIG2Globals` 解析等）。
    pub findings: Vec<Jbig2Finding>,
}

impl Jbig2ImageInfo {
    /// 图像流页面信息段元数据（§7.4.8）。
    pub fn page_info(&self) -> Option<&Jbig2PageInfo> {
        self.page.page_info()
    }

    /// 全局段个数：`/JBIG2Globals` 流的段数，加上混置在图像流中的
    /// 页关联 0 段。
    pub fn global_segment_count(&self) -> usize {
        self.globals
            .as_ref()
            .map_or(0, |stream| stream.segments.len())
            + self.page.global_segment_count()
    }

    /// 图像流页段（页关联非 0）个数。
    pub fn page_segment_count(&self) -> usize {
        self.page.page_segment_count()
    }

    /// 图像流内页关联大于 1 的段列表；为空表示只出现 0/1。
    pub fn unexpected_page_associations(&self) -> Vec<(u32, u32)> {
        self.page.unexpected_page_associations()
    }
}

/// 判断规范化的 `/Filter` 名列表中是否含 JBIG2Decode（含旧名 `JBIG2`，
/// 与 `types::features` 的映射同一口径）。
pub(crate) fn is_jbig2_filter(filters: &[String]) -> bool {
    filters
        .iter()
        .any(|name| name == "JBIG2Decode" || name == "JBIG2")
}

/// 遍历 JBIG2 段链，读取段头与页面信息段元数据（见模块文档的边界声明）。
///
/// 只解析语法层，不解码任何段载荷的内部结构；段头不完整返回类型化
/// [`Jbig2Error::TruncatedSegmentHeader`]（绝不 panic），其余畸形以
/// [`Jbig2Finding`] 记录后干净停止，已解析的段照常返回。
pub fn parse_jbig2_segments(data: &[u8]) -> Result<Jbig2Stream, Jbig2Error> {
    let mut segments = Vec::new();
    let mut findings = Vec::new();
    let mut page_information: Option<Jbig2PageInfo> = None;
    let mut position = 0usize;

    if data.len() >= 8 && data[..8] == FILE_SIGNATURE {
        // T.88 §6.2：旗标 bit0=0 为随机访问（无页数），bit1=0 时跟随
        // 4 字节页数。§7.4.7 禁止文件头进入 PDF：识别、跳过并记录。
        let flags = data.get(8).copied();
        let random_access = flags.map(|flags| flags & 1 == 0);
        let has_page_count = flags.is_some_and(|flags| flags & 2 == 0);
        let declared_pages = if has_page_count && data.len() >= 13 {
            Some(u32_at(data, 9))
        } else {
            None
        };
        findings.push(Jbig2Finding::FileHeaderPresent {
            position: 0,
            flags,
            random_access,
            declared_pages,
        });
        let header_len = if has_page_count { 13 } else { 9 };
        if data.len() < header_len {
            // 文件头自身截断：无从定位段起点，干净停止。
            return Ok(Jbig2Stream {
                segments,
                findings,
                page_information,
            });
        }
        position = header_len;
    }

    while position < data.len() {
        let (segment, advance, stop) =
            parse_one_segment(data, position, &mut findings, &mut page_information)?;
        segments.push(segment);
        if stop {
            break;
        }
        position += advance;
    }
    Ok(Jbig2Stream {
        segments,
        findings,
        page_information,
    })
}

/// 解析 `position` 处的单个段。返回（段, 应前进字节数, 是否干净停止）。
fn parse_one_segment(
    data: &[u8],
    position: usize,
    findings: &mut Vec<Jbig2Finding>,
    page_information: &mut Option<Jbig2PageInfo>,
) -> Result<(Jbig2Segment, usize, bool), Jbig2Error> {
    let remaining = data.len() - position;
    if remaining < 11 {
        // jbig2dec 同口径：11 字节是段头下界。
        return Err(Jbig2Error::TruncatedSegmentHeader {
            position,
            remaining,
            required: 11,
        });
    }
    let truncated = |required: usize| -> Result<(Jbig2Segment, usize, bool), Jbig2Error> {
        Err(Jbig2Error::TruncatedSegmentHeader {
            position,
            remaining,
            required,
        })
    };

    let number = u32_at(data, position);
    let flags = data[position + 4];
    let segment_type = flags & 0x3F;
    let kind = Jbig2SegmentKind::from_segment_type(segment_type);
    let deferred_non_retain = flags & 0x80 != 0;
    let page_association_long = flags & 0x40 != 0;
    let pa_len = if page_association_long { 4 } else { 1 };
    let rtscarf = data[position + 5];
    let mut offset = position + 6;

    let (referred_count, referred_count_extended, retained_bytes) = if rtscarf & 0xE0 == 0xE0 {
        // T.88 §7.2.4 长形式：32 位字段自本旗标字节起重叠读取（低 29 位
        // 为被引用段个数），随后 ceil(count/8) 字节保留位图。
        let count = u32_at(data, position + 5) & 0x1FFF_FFFF;
        offset = position + 9;
        let retained_len = (count as usize).div_ceil(8);
        let end = match offset.checked_add(retained_len) {
            Some(end) if end <= data.len() => end,
            _ => return truncated(offset - position + retained_len),
        };
        let bytes = data[offset..end].to_vec();
        offset = end;
        (count, true, bytes)
    } else {
        // 短形式：高 3 位直接是被引用段个数（0–6；TP=5/6 预留形式不
        // 支持，按个数处理，畸形输入会退化为后续截断发现）。
        ((rtscarf >> 5) as u32, false, Vec::new())
    };
    let retained_bits_indicator = if referred_count_extended {
        retained_bytes.first().copied().unwrap_or(0)
    } else {
        rtscarf & 0x1F
    };

    // 被引用段号字段宽度（T.88 §7.2.4，jbig2dec 同口径）：按当前段号。
    let field_size = if number <= 256 {
        1usize
    } else if number <= 65536 {
        2
    } else {
        4
    };
    let refs_len = match (referred_count as usize).checked_mul(field_size) {
        Some(len) => len,
        None => return truncated(remaining.saturating_add(1)),
    };
    let needed = (offset - position)
        .saturating_add(refs_len)
        .saturating_add(pa_len)
        .saturating_add(4);
    if needed > remaining {
        return truncated(needed);
    }
    let mut referred_to_segments = Vec::with_capacity(referred_count as usize);
    for _ in 0..referred_count {
        let value = match field_size {
            1 => u32::from(data[offset]),
            2 => u32::from(u16_at(data, offset)),
            _ => u32_at(data, offset),
        };
        offset += field_size;
        referred_to_segments.push(value);
    }
    let page_association = if page_association_long {
        u32_at(data, offset)
    } else {
        u32::from(data[offset])
    };
    offset += pa_len;
    let data_size = u32_at(data, offset);
    offset += 4;
    let header_len = offset - position;

    let mut payload = Vec::new();
    let mut advance = header_len;
    let mut stop = false;
    if data_size == u32::MAX {
        // §7.2.7 未指定长度：终止符搜索超出本层范围，记录并干净停止。
        findings.push(Jbig2Finding::SegmentLengthUnspecified {
            segment_number: number,
        });
        stop = true;
    } else {
        let declared = data_size as usize;
        let available = remaining - header_len;
        if declared > available {
            findings.push(Jbig2Finding::SegmentDataTruncated {
                segment_number: number,
                declared: data_size,
                available,
            });
            payload = data[offset..offset + available].to_vec();
            advance += available;
            stop = true;
        } else {
            payload = data[offset..offset + declared].to_vec();
            advance += declared;
        }
    }

    match kind {
        Jbig2SegmentKind::EndOfPage => findings.push(Jbig2Finding::EndOfPageSegmentPresent {
            segment_number: number,
        }),
        Jbig2SegmentKind::EndOfFile => findings.push(Jbig2Finding::EndOfFileSegmentPresent {
            segment_number: number,
        }),
        Jbig2SegmentKind::PageInformation => {
            if payload.len() < PAGE_INFORMATION_FULL_LEN {
                findings.push(Jbig2Finding::PageInformationSegmentTooShort {
                    segment_number: number,
                    available: payload.len(),
                });
            }
            if page_information.is_none() {
                *page_information = parse_page_information(&payload);
            }
        }
        _ => {}
    }

    Ok((
        Jbig2Segment {
            number,
            kind,
            segment_type,
            flags,
            deferred_non_retain,
            referred_to_segments,
            referred_count_extended,
            retained_bits_indicator,
            retained_bytes,
            page_association,
            page_association_long,
            data_size,
            data: payload,
            header_len,
        },
        advance,
        stop,
    ))
}

/// 页面信息段载荷（§7.4.8）：4×u32 几何 + BPC + BPIFlags (+ BPIExtension)。
fn parse_page_information(payload: &[u8]) -> Option<Jbig2PageInfo> {
    if payload.len() < PAGE_INFORMATION_MIN_LEN {
        return None;
    }
    let flags = payload[16];
    let bpi_flags = payload[17];
    let bpi_extension = (payload.len() >= PAGE_INFORMATION_FULL_LEN).then(|| payload[18]);
    let striped = bpi_flags & 0x80 != 0;
    let stripe_height = match (striped, bpi_extension) {
        (true, Some(extension)) => Some((u16::from(bpi_flags & 0x7F) << 8) | u16::from(extension)),
        _ => None,
    };
    Some(Jbig2PageInfo {
        width: u32_at(payload, 0),
        height: u32_at(payload, 4),
        x_resolution: u32_at(payload, 8),
        y_resolution: u32_at(payload, 12),
        flags,
        lossless: flags & 0x01 != 0,
        refinement: flags & 0x02 != 0,
        default_pixel: (flags >> 2) & 1,
        combination_operator: (flags >> 3) & 3,
        requires_buffer: flags & 0x20 != 0,
        combination_operator_override: flags & 0x40 != 0,
        colored: flags & 0x80 != 0,
        bpi_flags,
        bpi_extension,
        striped,
        stripe_height,
    })
}

fn u16_at(data: &[u8], offset: usize) -> u16 {
    let Some(bytes) = data.get(offset..offset + 2) else {
        return 0;
    };
    u16::from_be_bytes([bytes[0], bytes[1]])
}

fn u32_at(data: &[u8], offset: usize) -> u32 {
    let Some(bytes) = data.get(offset..offset + 4) else {
        return 0;
    };
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

#[cfg(test)]
mod tests;
