//! PDF 版本与特性能力模型。

use std::fmt;

/// PDF 版本号（`%PDF-x.y` 头部声明）。
///
/// 覆盖 1.0–1.7 与 2.0。reader 用它记录源文档声明的版本（版本中立
/// 元数据）并据此补宽读说明；writer **不**读取源版本——输出特性一律
/// 由 writer 自身的目标版本决定。
///
/// **版本→特性**判断统一走 [`PdfVersion::supports`]（唯一事实源），
/// 各特性引入的最低版本由 [`PdfFeature::min_version`] 给出。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PdfVersion {
    pub major: u8,
    pub minor: u8,
}

impl PdfVersion {
    pub const V1_0: Self = Self { major: 1, minor: 0 };
    pub const V1_1: Self = Self { major: 1, minor: 1 };
    pub const V1_2: Self = Self { major: 1, minor: 2 };
    pub const V1_3: Self = Self { major: 1, minor: 3 };
    pub const V1_4: Self = Self { major: 1, minor: 4 };
    pub const V1_5: Self = Self { major: 1, minor: 5 };
    pub const V1_6: Self = Self { major: 1, minor: 6 };
    pub const V1_7: Self = Self { major: 1, minor: 7 };
    /// PDF 2.0（ISO 32000-2）。字段顺序派生 `Ord`，故 `V1_7 < V2_0` 成立。
    pub const V2_0: Self = Self { major: 2, minor: 0 };

    /// 从文件头字节解析 `%PDF-x.y`。找不到或格式不符返回 `None`。
    pub fn parse(header: &[u8]) -> Option<Self> {
        let head = header.iter().take(1024).copied().collect::<Vec<_>>();
        let s = String::from_utf8_lossy(&head);
        let marker = s.find("%PDF-")?;
        Self::from_name(&s[marker + 5..])
    }

    /// 从版本名字符串解析版本，形如 `1.5`、`2.0`。
    ///
    /// 用于文件头 `%PDF-x.y` 之后的部分，以及根 Catalog 字典的
    /// `/Version` 覆盖值（PDF 1.4+）。接受 `1.0–1.7` 与 `2.0`；
    /// 无法解析或超出范围（如 `2.1`、`3.0`）返回 `None`，由调用方
    /// 忽略该覆盖。
    pub fn from_name(name: &str) -> Option<Self> {
        let mut parts = name.split(|c: char| !c.is_ascii_digit());
        let major: u8 = parts.next()?.parse().ok()?;
        let minor: u8 = parts.next()?.parse().ok()?;
        if (major == 1 && minor <= 7) || (major == 2 && minor == 0) {
            Some(Self { major, minor })
        } else {
            None
        }
    }

    pub fn as_str(&self) -> String {
        format!("{}.{}", self.major, self.minor)
    }

    /// 版本是否支持某特性，即 `self >= feature.min_version()`。
    ///
    /// 这是版本→特性判断的**唯一事实源**：reader 的 xref 流/对象流
    /// 版本门控、writer 的输出特性校验，均应经此方法而非自行比较
    /// 版本常量。[`PdfVersion::supports_xref_streams`] 等为此的便捷
    /// 包装。
    pub fn supports(self, feature: PdfFeature) -> bool {
        self >= feature.min_version()
    }

    /// 是否支持交叉引用流（XRef stream，PDF 1.5+ 引入）。
    /// 便捷包装，等价于 `self.supports(PdfFeature::XRefStream)`。
    pub fn supports_xref_streams(self) -> bool {
        self.supports(PdfFeature::XRefStream)
    }

    /// 是否支持对象流（ObjStm，PDF 1.5+ 引入）。
    /// 便捷包装，等价于 `self.supports(PdfFeature::ObjectStream)`。
    pub fn supports_object_streams(self) -> bool {
        self.supports(PdfFeature::ObjectStream)
    }
}

impl Default for PdfVersion {
    fn default() -> Self {
        Self::V1_7
    }
}

impl fmt::Display for PdfVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// PDF 特性：统一的「版本→特性」能力模型的载体。
///
/// 每个特性对应 ISO 32000-1/32000-2 中某个自特定版本引入的语法/语义
/// 能力。判定「某版本是否支持某特性」统一用
/// [`PdfVersion::supports`]，其内部即比较本枚举给出的最低版本
/// [`PdfFeature::min_version`]。这样特性阈值集中一处，避免版本常量
/// 散落在 reader/writer/builder 各处比较。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PdfFeature {
    /// 交叉引用流（XRef stream，`/Type /XRef`），PDF 1.5 引入。
    XRefStream,
    /// 对象流（ObjStm，`/Type /ObjStm`），PDF 1.5 引入。
    ObjectStream,
    /// 标准加密中的 40-bit RC4（标准安全处理器自 PDF 1.1 引入）。
    EncryptionRC4,
    /// JBIG2 图像压缩过滤器（`/JBIG2Decode`），PDF 1.4 引入。
    JBIG2Decode,
    /// JPEG 2000 图像压缩过滤器（`/JPXDecode`），PDF 1.5 引入。
    JPXDecode,
    /// 透明度（`/ca` `/CA` `/BM`、透明组），PDF 1.4 引入。
    Transparency,
    /// 图像 `/Mask` 条目（显式蒙版或颜色键蒙版），PDF 1.3 引入。
    ImageMaskEntry,
    /// 图像 `/Alternates` 替代图像数组（Table 90），PDF 1.3 引入。
    AlternateImages,
    /// 图像 `/Intent` 渲染意图，PDF 1.4 引入。
    ImageIntent,
    /// 平铺图案（`/PatternType 1` 图案流），PDF 1.2 引入。
    TilingPattern,
    /// 平滑渐变（`/ShadingType 1–7`、`sh` 操作符与着色图案），PDF 1.3 引入。
    Shading,
    /// 可选内容（`/OCProperties`），PDF 1.5 引入。
    OptionalContent,
    /// 软蒙版 `/Matte` 衬底颜色（PDF 1.6 引入）。
    SoftMaskMatte,
    /// 标记类注解（Highlight/Underline/StrikeOut/Caret/Ink 等，Table 169），
    /// PDF 1.3 引入。
    MarkupAnnotations,
    /// 数字签名（`/SigFlags`、`/FT /Sig` 域、签名字典 `/ByteRange` +
    /// `/Contents`），PDF 1.3 引入。
    DigitalSignatures,
    /// 文本注解标记状态条目（`/State`、`/StateModel`），PDF 1.3 引入。
    AnnotationMarkupState,
    /// Line 注解端点样式与填充色条目（`/LE`、`/IC`，Table 175），PDF 1.4 引入。
    LineAnnotationArrowEndings,
    /// Line 注解引线条目（`/LL`、`/LLE`、`/Cap`、`/IT`，Table 175），PDF 1.6 引入。
    LineAnnotationLeader,
    /// Line 注解题注设置（`/LLO`、`/CP`、`/CO`，Table 175），PDF 1.7 引入。
    LineAnnotationCaption,
    /// FreeText 对齐方式条目（`/Q`，Table 174），PDF 1.4 引入。
    FreeTextQuadding,
    /// FreeText 富文本条目（`/RC`、`/DS`，Table 174），PDF 1.5 引入。
    FreeTextRichContent,
    /// FreeText 标注框条目（`/CL`、`/IT`、`/BE`、`/RD`、`/LE`，Table 174），
    /// PDF 1.6 引入。
    FreeTextCallout,
}

impl PdfFeature {
    /// 该特性引入的最低 PDF 版本（按 ISO 32000-1）。
    pub fn min_version(self) -> PdfVersion {
        match self {
            PdfFeature::EncryptionRC4 => PdfVersion::V1_1,
            PdfFeature::TilingPattern => PdfVersion::V1_2,
            PdfFeature::ImageMaskEntry
            | PdfFeature::AlternateImages
            | PdfFeature::Shading
            | PdfFeature::MarkupAnnotations
            | PdfFeature::DigitalSignatures
            | PdfFeature::AnnotationMarkupState => PdfVersion::V1_3,
            PdfFeature::JBIG2Decode | PdfFeature::Transparency | PdfFeature::ImageIntent => {
                PdfVersion::V1_4
            }
            PdfFeature::XRefStream
            | PdfFeature::ObjectStream
            | PdfFeature::JPXDecode
            | PdfFeature::OptionalContent => PdfVersion::V1_5,
            PdfFeature::SoftMaskMatte | PdfFeature::LineAnnotationLeader => PdfVersion::V1_6,
            PdfFeature::LineAnnotationArrowEndings => PdfVersion::V1_4,
            PdfFeature::LineAnnotationCaption => PdfVersion::V1_7,
            PdfFeature::FreeTextQuadding => PdfVersion::V1_4,
            PdfFeature::FreeTextRichContent => PdfVersion::V1_5,
            PdfFeature::FreeTextCallout => PdfVersion::V1_6,
        }
    }
}

impl fmt::Display for PdfFeature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            PdfFeature::XRefStream => "xref stream",
            PdfFeature::ObjectStream => "object stream",
            PdfFeature::EncryptionRC4 => "40-bit RC4 encryption",
            PdfFeature::JBIG2Decode => "JBIG2Decode filter",
            PdfFeature::JPXDecode => "JPXDecode filter",
            PdfFeature::Transparency => "transparency",
            PdfFeature::ImageMaskEntry => "image Mask entry",
            PdfFeature::AlternateImages => "alternate images",
            PdfFeature::ImageIntent => "image rendering intent",
            PdfFeature::TilingPattern => "tiling pattern",
            PdfFeature::Shading => "shading",
            PdfFeature::OptionalContent => "optional content",
            PdfFeature::SoftMaskMatte => "soft mask matte color",
            PdfFeature::MarkupAnnotations => "markup annotations",
            PdfFeature::DigitalSignatures => "digital signature field",
            PdfFeature::AnnotationMarkupState => "text annotation markup state",
            PdfFeature::LineAnnotationArrowEndings => "line annotation arrow endings",
            PdfFeature::LineAnnotationLeader => "line annotation leader settings",
            PdfFeature::LineAnnotationCaption => "line annotation caption settings",
            PdfFeature::FreeTextQuadding => "free text quadding",
            PdfFeature::FreeTextRichContent => "free text rich content",
            PdfFeature::FreeTextCallout => "free text callout",
        };
        write!(f, "{name}")
    }
}
