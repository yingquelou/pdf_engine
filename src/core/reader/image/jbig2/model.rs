//! JBIG2 语法层数据模型：段、页面信息、类型化发现、错误与段链视图。

use std::fmt;

use super::kinds::Jbig2SegmentKind;

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
