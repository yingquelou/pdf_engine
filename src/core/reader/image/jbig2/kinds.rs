//! 段类型枚举 [`Jbig2SegmentKind`]：段旗标低 6 位与规范段号的映射。

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
