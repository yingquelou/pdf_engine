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
    /// 几何类注解边框效果（`/BE`，Table 177/178），PDF 1.5 引入。
    AnnotationBorderEffect,
    /// Square/Circle 矩形差值（`/RD`，Table 177），PDF 1.5 引入。
    AnnotationRectDifferences,
    /// Polygon 云状意图（`/IT /PolygonCloud`，Table 178），PDF 1.6 引入。
    PolygonCloudIntent,
    /// Polygon/PolyLine 标注意图（`/IT /PolygonDimension`、`/PolyLineDimension`，
    /// Table 178），PDF 1.7 引入。
    PolygonDimensionIntent,
    /// Link 注解高亮模式（`/H`，Table 173），PDF 1.2 引入。
    LinkAnnotationHighlighting,
    /// Link 注解按下 URI 动作（`/PA`，Table 173），PDF 1.3 引入。
    LinkAnnotationPressedAction,
    /// Link 注解四边形激活区域（`/QuadPoints`，Table 173），PDF 1.6 引入。
    LinkAnnotationQuadPoints,
    /// Link 注解边框样式（`/BS`，Table 173），PDF 1.6 引入。
    LinkAnnotationBorderStyle,
    /// 注解 measure 字典（`/Measure`，Table 261–263；Line 与
    /// Polygon/PolyLine 自 Table 175/178），PDF 1.7 引入。
    AnnotationMeasure,
    /// Hide 动作（隐藏/显示注解，12.6.4.10 Table 210），PDF 1.2 引入。
    HideAction,
    /// Set-OCG-State 动作（切换可选内容组状态，12.6.4.12 Table 213），
    /// PDF 1.5 引入。
    SetOcgState,
    /// Transition 动作（全屏演示过渡特效，12.6.4.14 Table 215），PDF 1.5 引入。
    TransitionAction,
    /// Thread 动作（跳转到文章线上的 bead，12.6.4.6 Table 205），PDF 1.1 引入。
    ThreadAction,
    /// Embedded Go-To 动作（在嵌套嵌入文档之间跳转，12.6.4.4 Table 201），
    /// PDF 1.6 引入。
    EmbeddedGoToAction,
    /// 旧式注解边框数组的虚线项（`/Border` 第四项，Table 164），PDF 1.1 引入。
    AnnotationBorderDash,
    /// 表单字段附加动作（`/AA` 的 `/K` `/F` `/V` `/C` 触发事件，
    /// Table 196），PDF 1.3 引入。
    FormFieldAdditionalActions,
    /// 复选框与单选按钮字段的 `/Opt` 导出值数组（Table 227），PDF 1.4 引入。
    FormButtonOptions,
    /// 选择字段的多选索引数组 `/I`（Table 231），PDF 1.4 引入。
    FormChoiceIndices,
    /// AcroForm 计算顺序（`/CO`，Table 218），PDF 1.3 引入。
    FormCalculationOrder,
    /// 表单数据动作（SubmitForm/ResetForm/ImportData，12.7.5.2–12.7.5.4），
    /// PDF 1.2 引入。
    FormDataActions,
    /// 表单动作的 `/Fields` 字段全限定名条目（Table 236/238），PDF 1.3 引入。
    FormActionFieldNames,
    /// Submit-Form 动作的 `/URLType`（Table 236），PDF 1.7 引入。
    SubmitFormUrlType,
    /// 页面视口字典（页面 `/VP` 数组，12.9.2 Table 260），PDF 1.6 引入。
    PageViewports,
    /// 页面演示条目（页面 `/Dur` 与 `/Trans`，12.4.4 Table 30），PDF 1.1 引入。
    PagePresentation,
    /// 文章线（目录 `/Threads`、线程与 bead 字典，12.4.3 Table 160/161），
    /// PDF 1.1 引入。
    ArticleThreads,
    /// 便携集合（目录 `/Collection`，12.3.5 Table 155–158），PDF 1.7 引入。
    Collection,
    /// 集合项字典（文件规格 `/CI`，7.11.6 Table 48/49），PDF 1.7 引入。
    CollectionItem,
    /// 命名页面模板名称树（目录 `/Names /Templates`，12.7.6 与 Table 31），
    /// PDF 1.3 引入。
    PageTemplates,
    /// 备用演示（目录 `/Names /AlternatePresentations` 名称树，13.5 与
    /// Table 297），PDF 1.4 引入。
    AlternatePresentations,
    /// 声音对象（声音流字典，13.3 与 Table 294）、Sound 动作（12.6.4.8
    /// Table 208）与 Sound 注解（12.5.6.16 Table 185），PDF 1.2 引入。
    SoundObjects,
    /// 电影对象（电影字典 13.4 Table 295）、Movie 动作（12.6.4.9
    /// Table 209）与 Movie 注解（12.5.6.17 Table 186），PDF 1.2 引入。
    MovieObjects,
    /// 多媒体 rendition（13.2 Table 266–278）、Rendition 动作（12.6.4.13
    /// Table 214）与 Screen 注解（12.5.6.18 Table 187），PDF 1.5 引入。
    Renditions,
    /// 3D artwork 流（13.6 Table 300）与 3D 注解（12.5.6.19 Table 298），
    /// PDF 1.6 引入。
    ThreeDAnnotations,
    /// 3D 流字典的动画样式字典 `/AN`（13.6.3.2 Table 301/302），PDF 1.7 引入。
    ThreeDAnimationStyle,
    /// 3D 视图字典的 PDF 1.7 条目（`/RM` `/LS` `/SA` `/NA` `/NR`，
    /// 13.6.4.1 Table 304），PDF 1.7 引入。
    ThreeDViewExtensions,
    /// Go-To-3D-View 动作（12.6.4.15 Table 216），PDF 1.6 引入。
    GoTo3DViewAction,
    /// 3D 标记注解的外部数据字典 `/ExData`（13.6.6 Table 170/313），
    /// PDF 1.7 引入。
    ThreeDMarkup,
    /// 目录 `/OutputIntents` 输出意图数组（14.11.5 Table 365），PDF 1.4 引入。
    OutputIntents,
    /// 打印机标记（14.11.3）：PrinterMark 注解（Table 362）与表单条目标
    /// 记（Table 363），PDF 1.4 引入。
    PrinterMarks,
    /// Trap 网络（14.11.6）：TrapNet 注解（Table 366）与表单条目标记
    /// （Table 367），PDF 1.3 引入。
    TrapNetworks,
    /// Trap 网络的修改日期 `/LastModified`（14.11.6 Table 366），PDF 1.4 引入。
    TrapNetworkModificationDate,
    /// 页面处理框（`/BleedBox` `/TrimBox` `/ArtBox`，14.11.2 Table 30），
    /// PDF 1.3 引入。
    PageBoundaries,
    /// 页面 `/BoxColorInfo` 框颜色信息（14.11.2 Table 360/361），PDF 1.4 引入。
    PageBoxColorInfo,
    /// 分色字典（页面 `/SeparationInfo`，14.11.4 Table 364），PDF 1.3 引入。
    SeparationInfo,
    /// OPI 代理字典（图像与表单 XObject 的 `/OPI`，14.11.7 Table 368–370），
    /// PDF 1.2 引入。
    OpiDictionaries,
    /// 逻辑结构树（目录 `/StructTreeRoot` 与结构元素，14.7 Table 322/323），
    /// PDF 1.3 引入。
    LogicalStructure,
    /// 文档默认自然语言（目录 `/Lang`，14.9.2.1），PDF 1.4 引入。
    DocumentLanguage,
    /// 结构元素的语言与替换文本条目（`/Lang` `/ActualText`，14.7.2
    /// Table 323），PDF 1.4 引入。
    StructureLanguageText,
    /// 结构元素的缩写展开条目（`/E`，14.7.2 Table 323），PDF 1.5 引入。
    StructureExpandedText,
    /// Span 属性表的自然语言与缩写展开条目（`/Lang` `/E`，14.9.2.1、
    /// 14.9.5），PDF 1.4 引入。
    SpanPropertyLists,
    /// Span 属性表的替代描述与替换文本条目（`/Alt` `/ActualText`，
    /// 14.9.3、14.9.4），PDF 1.5 引入。
    SpanAlternateText,
    /// Artifact 标记内容属性表（14.8.2.2.2 Table 330），PDF 1.4 引入。
    ArtifactPropertyLists,
    /// 背景 Artifact（`/Type /Background`，14.8.2.2.2 Table 330），
    /// PDF 1.7 引入。
    ArtifactBackground,
    /// Artifact 的 `/Subtype` 条目（14.8.2.2.2 Table 330），PDF 1.7 引入。
    ArtifactSubtype,
    /// 结构属性对象的用户属性（`/O /UserProperties` 与 `/P`，14.7.5
    /// Table 328/329），PDF 1.6 引入。
    StructureUserProperties,
    /// 标准结构属性所有者（14.8.5.2 Table 341）的 Layout/List/PrintField/
    /// Table 条目（14.8.5.4–14.8.5.7 Table 342–349），PDF 1.4 引入。
    StandardStructureAttributes,
    /// Layout 标准属性中 PDF 1.5 引入的条目（14.8.5.4 Table 343/345 的
    /// 颜色、边框、内边距、文本装饰与 Ruby/竖排字形方向）。
    LayoutExtendedAttributes,
    /// Layout 标准属性的分栏条目（14.8.5.4 Table 346 的 `/ColumnCount`
    /// `/ColumnGap` `/ColumnWidths`），PDF 1.6 引入。
    LayoutColumnAttributes,
    /// Table 标准属性的表头引用与作用范围（`/Headers` `/Scope`，14.8.5.7
    /// Table 349），PDF 1.5 引入。
    TableHeaderAttributes,
    /// Table 标准属性的摘要条目（`/Summary`，14.8.5.7 Table 349），
    /// PDF 1.7 引入。
    TableSummary,
    /// PrintField 标准属性（`/O /PrintField`，14.8.5.6 Table 348），
    /// PDF 1.7 引入。
    PrintFieldAttributes,
    /// 目录标记信息字典（`/MarkInfo`，14.7.1 Table 321），PDF 1.4 引入。
    MarkInfo,
    /// 标记信息的用户属性条目（`/MarkInfo /UserProperties`，Table 321），
    /// PDF 1.6 引入。
    MarkInfoUserProperties,
    /// 标记信息的可疑内容条目（`/MarkInfo /Suspects`，Table 321），
    /// PDF 1.6 引入。
    MarkInfoSuspects,
    /// 标准结构类型的 PDF 1.5 扩展（14.8.4 的 THead/TBody/TFoot/Annot/
    /// Ruby/RB/RT/RP/Warichu/WT/WP），PDF 1.5 引入。
    StructureTypeExtensions,
}

impl PdfFeature {
    /// 该特性引入的最低 PDF 版本（按 ISO 32000-1）。
    pub fn min_version(self) -> PdfVersion {
        match self {
            PdfFeature::EncryptionRC4
            | PdfFeature::AnnotationBorderDash
            | PdfFeature::PagePresentation
            | PdfFeature::ArticleThreads
            | PdfFeature::ThreadAction => PdfVersion::V1_1,
            PdfFeature::TilingPattern
            | PdfFeature::LinkAnnotationHighlighting
            | PdfFeature::HideAction
            | PdfFeature::FormDataActions
            | PdfFeature::SoundObjects
            | PdfFeature::MovieObjects
            | PdfFeature::OpiDictionaries => PdfVersion::V1_2,
            PdfFeature::ImageMaskEntry
            | PdfFeature::AlternateImages
            | PdfFeature::Shading
            | PdfFeature::MarkupAnnotations
            | PdfFeature::DigitalSignatures
            | PdfFeature::AnnotationMarkupState
            | PdfFeature::LinkAnnotationPressedAction
            | PdfFeature::FormFieldAdditionalActions
            | PdfFeature::FormCalculationOrder
            | PdfFeature::FormActionFieldNames
            | PdfFeature::PageTemplates
            | PdfFeature::PageBoundaries
            | PdfFeature::SeparationInfo
            | PdfFeature::TrapNetworks
            | PdfFeature::LogicalStructure => PdfVersion::V1_3,
            PdfFeature::JBIG2Decode
            | PdfFeature::Transparency
            | PdfFeature::ImageIntent
            | PdfFeature::FormButtonOptions
            | PdfFeature::FormChoiceIndices
            | PdfFeature::AlternatePresentations
            | PdfFeature::OutputIntents
            | PdfFeature::PrinterMarks
            | PdfFeature::TrapNetworkModificationDate
            | PdfFeature::PageBoxColorInfo
            | PdfFeature::DocumentLanguage
            | PdfFeature::StructureLanguageText
            | PdfFeature::StandardStructureAttributes
            | PdfFeature::SpanPropertyLists
            | PdfFeature::ArtifactPropertyLists
            | PdfFeature::MarkInfo => PdfVersion::V1_4,
            PdfFeature::XRefStream
            | PdfFeature::ObjectStream
            | PdfFeature::JPXDecode
            | PdfFeature::OptionalContent
            | PdfFeature::AnnotationBorderEffect
            | PdfFeature::AnnotationRectDifferences
            | PdfFeature::SetOcgState
            | PdfFeature::TransitionAction
            | PdfFeature::Renditions
            | PdfFeature::TableHeaderAttributes
            | PdfFeature::LayoutExtendedAttributes
            | PdfFeature::StructureTypeExtensions
            | PdfFeature::StructureExpandedText
            | PdfFeature::SpanAlternateText => PdfVersion::V1_5,
            PdfFeature::SoftMaskMatte
            | PdfFeature::LineAnnotationLeader
            | PdfFeature::PolygonCloudIntent
            | PdfFeature::LinkAnnotationQuadPoints
            | PdfFeature::LinkAnnotationBorderStyle
            | PdfFeature::PageViewports
            | PdfFeature::EmbeddedGoToAction
            | PdfFeature::GoTo3DViewAction
            | PdfFeature::ThreeDAnnotations
            | PdfFeature::MarkInfoUserProperties
            | PdfFeature::MarkInfoSuspects
            | PdfFeature::LayoutColumnAttributes
            | PdfFeature::StructureUserProperties => PdfVersion::V1_6,
            PdfFeature::LineAnnotationArrowEndings => PdfVersion::V1_4,
            PdfFeature::LineAnnotationCaption
            | PdfFeature::PolygonDimensionIntent
            | PdfFeature::AnnotationMeasure
            | PdfFeature::SubmitFormUrlType
            | PdfFeature::Collection
            | PdfFeature::CollectionItem
            | PdfFeature::ThreeDAnimationStyle
            | PdfFeature::ThreeDViewExtensions
            | PdfFeature::TableSummary
            | PdfFeature::PrintFieldAttributes
            | PdfFeature::ArtifactBackground
            | PdfFeature::ArtifactSubtype
            | PdfFeature::ThreeDMarkup => PdfVersion::V1_7,
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
            PdfFeature::AnnotationBorderEffect => "annotation border effect",
            PdfFeature::AnnotationRectDifferences => "annotation rect differences",
            PdfFeature::PolygonCloudIntent => "polygon cloud intent",
            PdfFeature::PolygonDimensionIntent => "polygon dimension intent",
            PdfFeature::LinkAnnotationHighlighting => "link annotation highlighting",
            PdfFeature::LinkAnnotationPressedAction => "link annotation pressed action",
            PdfFeature::LinkAnnotationQuadPoints => "link annotation quad points",
            PdfFeature::LinkAnnotationBorderStyle => "link annotation border style",
            PdfFeature::AnnotationMeasure => "annotation measure dictionary",
            PdfFeature::AnnotationBorderDash => "annotation border dash array",
            PdfFeature::FormFieldAdditionalActions => "form field additional actions",
            PdfFeature::FormButtonOptions => "button form field options",
            PdfFeature::FormChoiceIndices => "choice field alternate indices",
            PdfFeature::FormCalculationOrder => "form calculation order",
            PdfFeature::FormDataActions => "form data actions",
            PdfFeature::HideAction => "hide action",
            PdfFeature::SetOcgState => "Set-OCG-State action",
            PdfFeature::TransitionAction => "transition action",
            PdfFeature::ThreadAction => "thread action",
            PdfFeature::EmbeddedGoToAction => "embedded go-to action",
            PdfFeature::FormActionFieldNames => "form action field names",
            PdfFeature::SubmitFormUrlType => "submit-form URL type",
            PdfFeature::PageViewports => "page viewport dictionaries",
            PdfFeature::PagePresentation => "page presentation entries",
            PdfFeature::ArticleThreads => "article threads",
            PdfFeature::Collection => "collections",
            PdfFeature::CollectionItem => "collection items",
            PdfFeature::PageTemplates => "page templates",
            PdfFeature::AlternatePresentations => "alternate presentations",
            PdfFeature::SoundObjects => "sound objects",
            PdfFeature::MovieObjects => "movie objects",
            PdfFeature::Renditions => "renditions",
            PdfFeature::ThreeDAnnotations => "3D annotations",
            PdfFeature::ThreeDAnimationStyle => "3D animation styles",
            PdfFeature::ThreeDViewExtensions => "3D view PDF 1.7 entries",
            PdfFeature::GoTo3DViewAction => "go-to-3D-view actions",
            PdfFeature::ThreeDMarkup => "3D markup annotations",
            PdfFeature::OutputIntents => "output intents",
            PdfFeature::PrinterMarks => "printer's marks",
            PdfFeature::TrapNetworks => "trap networks",
            PdfFeature::TrapNetworkModificationDate => "trap network modification dates",
            PdfFeature::PageBoundaries => "page boundaries",
            PdfFeature::PageBoxColorInfo => "page box color info",
            PdfFeature::SeparationInfo => "separation dictionaries",
            PdfFeature::OpiDictionaries => "OPI dictionaries",
            PdfFeature::LogicalStructure => "logical structure trees",
            PdfFeature::DocumentLanguage => "document language",
            PdfFeature::StructureLanguageText => "structure language and actual text",
            PdfFeature::StructureExpandedText => "structure expanded text",
            PdfFeature::SpanPropertyLists => "span property lists",
            PdfFeature::SpanAlternateText => "span alternate and replacement text",
            PdfFeature::ArtifactPropertyLists => "artifact property lists",
            PdfFeature::ArtifactBackground => "background artifacts",
            PdfFeature::ArtifactSubtype => "artifact subtype",
            PdfFeature::StructureTypeExtensions => "structure type extensions",
            PdfFeature::StructureUserProperties => "structure user properties",
            PdfFeature::StandardStructureAttributes => "standard structure attributes",
            PdfFeature::LayoutExtendedAttributes => "extended Layout attributes",
            PdfFeature::LayoutColumnAttributes => "Layout column attributes",
            PdfFeature::TableHeaderAttributes => "table header attributes",
            PdfFeature::TableSummary => "table summary",
            PdfFeature::PrintFieldAttributes => "PrintField attributes",
            PdfFeature::MarkInfo => "mark info",
            PdfFeature::MarkInfoUserProperties => "mark info user properties",
            PdfFeature::MarkInfoSuspects => "mark info suspects",
        };
        write!(f, "{name}")
    }
}
