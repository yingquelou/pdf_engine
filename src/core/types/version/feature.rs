//! PDF 特性枚举与版本门控。

use super::PdfVersion;

/// PDF 特性：统一的「版本→特性」能力模型的载体。
///
/// 每个特性对应 ISO 32000-1/32000-2 中某个自特定版本引入的语法/语义
/// 能力。判定「某版本是否支持某特性」统一用
/// [`PdfVersion::supports`]，其内部即比较本枚举给出的最低版本
/// [`PdfFeature::min_version`]。这样特性阈值集中一处，避免版本常量
/// 散落在 reader/writer/builder 各处比较。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PdfFeature {
    /// 混合引用文件（hybrid-reference file，7.5.8.4，trailer `/XRefStm`
    /// Table 19），PDF 1.5 引入（依赖对象流与交叉引用流）。
    HybridReferenceFiles,
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
    /// 标记类注解的主题条目 `/Subj`（12.5.6.2 Table 170），PDF 1.5 引入。
    AnnotationSubject,
    /// 标记类注解的回复目标 `/IRT`（12.5.6.2 Table 170），PDF 1.5 引入。
    AnnotationInReplyTo,
    /// 标记类注解的回复类型名 `/RT`（12.5.6.2 Table 170），PDF 1.6 引入。
    AnnotationReplyType,
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
    /// 表单字段的替代字段名（字段字典 `/TU`，12.7.2 Table 220；
    /// 14.9.3 用于界面标识与无障碍朗读），PDF 1.3 引入。
    FormFieldAlternateName,
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
    /// 页内容中的 tag suspects（标签为 `TagSuspect` 的标记内容，
    /// 14.8.2.3.1），PDF 1.6 引入。
    TagSuspects,
    /// 反序显示串（标签为 `ReversedChars` 的标记内容，14.8.2.3.3），
    /// PDF 1.4 引入。
    ReversedCharacters,
    /// 标准结构类型的 PDF 1.5 扩展（14.8.4 的 THead/TBody/TFoot/Annot/
    /// Ruby/RB/RT/RP/Warichu/WT/WP），PDF 1.5 引入。
    StructureTypeExtensions,
    /// 扩展字典（目录 `/Extensions` 与各前缀开发者扩展字典，7.12 与
    /// Table 50），随 ISO 32000-1（PDF 1.7）引入。
    ExtensionsDictionary,
    /// 查看器首选项字典（目录 `/ViewerPreferences`，12.2 Table 150 的
    /// 1.2 档位条目：五个界面开关布尔与 `/NonFullScreenPageMode`），
    /// PDF 1.2 引入。
    ViewerPreferences,
    /// 查看器首选项的阅读顺序条目（`/Direction`，12.2 Table 150），
    /// PDF 1.3 引入。
    ViewerReadingDirection,
    /// 查看器首选项的标题栏条目（`/DisplayDocTitle`，12.2 Table 150），
    /// PDF 1.4 引入。
    ViewerDisplayDocTitle,
    /// 查看器首选项的页面边界条目（`/ViewArea` `/ViewClip` `/PrintArea`
    /// `/PrintClip`，12.2 Table 150），PDF 1.4 引入。
    ViewerPageBoundaryNames,
    /// 查看器首选项的打印缩放条目（`/PrintScaling`，12.2 Table 150），
    /// PDF 1.6 引入。
    ViewerPrintScaling,
    /// 查看器首选项的打印对话框条目（`/Duplex` `/PickTrayByPDFSize`
    /// `/PrintPageRange` `/NumCopies`，12.2 Table 150），PDF 1.7 引入。
    ViewerPrintDialogPreferences,
    /// 查看器首选项的强制执行条目（`/Enforce`，名数组）。ISO 32000-1
    /// 的 Table 150 无此条目，系 PDF 2.0（ISO 32000-2）追加；最低版本
    /// 记为 2.0。
    ViewerEnforcement,
    /// 字体特征推导（14.8.2.4.3 Table 331 的 Serifed/Proportional/Italic/
    /// Smallcap，由 FontDescriptor `/Flags` 派生），随 Tagged PDF 自
    /// PDF 1.4 起可用。
    FontCharacteristics,
    /// 字体选择器属性（14.8.2.4.3 Table 332 的 FontFamily/GenericFontFamily/
    /// FontWeight/FontStretch 等；描述符条目见 Table 122），PDF 1.5 引入。
    FontSelectorAttributes,
    /// 旧式命名目标字典（目录 `/Dests`，12.3.2.3 的 PDF 1.1 形态；
    /// Table 28 要求该字典**是间接引用**），PDF 1.1 引入。
    LegacyNamedDestinations,
}

impl PdfFeature {
    /// 该特性引入的最低 PDF 版本（按 ISO 32000-1）。
    pub fn min_version(self) -> PdfVersion {
        match self {
            PdfFeature::EncryptionRC4
            | PdfFeature::AnnotationBorderDash
            | PdfFeature::PagePresentation
            | PdfFeature::ArticleThreads
            | PdfFeature::ThreadAction
            | PdfFeature::LegacyNamedDestinations => PdfVersion::V1_1,
            PdfFeature::TilingPattern
            | PdfFeature::LinkAnnotationHighlighting
            | PdfFeature::HideAction
            | PdfFeature::FormDataActions
            | PdfFeature::SoundObjects
            | PdfFeature::MovieObjects
            | PdfFeature::OpiDictionaries
            | PdfFeature::ViewerPreferences => PdfVersion::V1_2,
            PdfFeature::ImageMaskEntry
            | PdfFeature::AlternateImages
            | PdfFeature::Shading
            | PdfFeature::MarkupAnnotations
            | PdfFeature::DigitalSignatures
            | PdfFeature::AnnotationMarkupState
            | PdfFeature::LinkAnnotationPressedAction
            | PdfFeature::FormFieldAdditionalActions
            | PdfFeature::FormFieldAlternateName
            | PdfFeature::FormCalculationOrder
            | PdfFeature::FormActionFieldNames
            | PdfFeature::PageTemplates
            | PdfFeature::PageBoundaries
            | PdfFeature::SeparationInfo
            | PdfFeature::TrapNetworks
            | PdfFeature::LogicalStructure
            | PdfFeature::ViewerReadingDirection => PdfVersion::V1_3,
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
            | PdfFeature::ReversedCharacters
            | PdfFeature::MarkInfo
            | PdfFeature::ViewerDisplayDocTitle
            | PdfFeature::ViewerPageBoundaryNames => PdfVersion::V1_4,
            PdfFeature::XRefStream
            | PdfFeature::ObjectStream
            | PdfFeature::HybridReferenceFiles
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
            | PdfFeature::SpanAlternateText
            | PdfFeature::AnnotationSubject
            | PdfFeature::AnnotationInReplyTo => PdfVersion::V1_5,
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
            | PdfFeature::TagSuspects
            | PdfFeature::LayoutColumnAttributes
            | PdfFeature::StructureUserProperties
            | PdfFeature::ViewerPrintScaling
            | PdfFeature::AnnotationReplyType => PdfVersion::V1_6,
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
            | PdfFeature::ThreeDMarkup
            | PdfFeature::ExtensionsDictionary
            | PdfFeature::ViewerPrintDialogPreferences => PdfVersion::V1_7,
            PdfFeature::ViewerEnforcement => PdfVersion::V2_0,
            PdfFeature::FreeTextQuadding => PdfVersion::V1_4,
            PdfFeature::FreeTextRichContent => PdfVersion::V1_5,
            PdfFeature::FreeTextCallout => PdfVersion::V1_6,
            PdfFeature::FontCharacteristics => PdfVersion::V1_4,
            PdfFeature::FontSelectorAttributes => PdfVersion::V1_5,
        }
    }
}
