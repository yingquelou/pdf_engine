//! 文档特性画像与扫描逻辑（reader/writer 共用）。

use std::collections::BTreeSet;

use super::value::{Dict, Value};
use super::version::PdfFeature;

/// 特性画像：记录文档实际用到的 [`PdfFeature`] 集合。
///
/// 读端在解析对象时**自动填充**（见 `Document::features`），写端消费
/// 它做「内容-版本一致性校验」与**结构性特性的自动降级**。不记录
/// FlateDecode / ASCIIHex 等**通用压缩过滤器**——这些低版本就支持，
/// 不需要版本降级。
#[derive(Debug, Clone, Default)]
pub struct FeatureProfile(BTreeSet<PdfFeature>);

impl FeatureProfile {
    /// 空画像。
    pub fn new() -> Self {
        Self(BTreeSet::new())
    }

    /// 插入一个特性。幂等。
    pub fn insert(&mut self, feature: PdfFeature) {
        self.0.insert(feature);
    }

    /// 是否包含某特性。
    pub fn contains(&self, feature: PdfFeature) -> bool {
        self.0.contains(&feature)
    }

    /// 迭代器（按最低版本升序，因 `BTreeSet` 有序）。
    pub fn iter(&self) -> std::collections::btree_set::Iter<'_, PdfFeature> {
        self.0.iter()
    }
}

/// 从一个字典中扫描「会影响输出版本的特性」并记录到 `features`。
///
/// 这是 reader 填充 FeatureProfile 与 writer 做内容-版本校验共用的
/// 扫描逻辑，避免两端各写一份。具体检测项：
///
/// - `/Type /XRef` → XRefStream
/// - `/Type /ObjStm` → ObjectStream
/// - `/Type /Sound`、`/Subtype /Sound`、`/S /Sound` → SoundObjects（1.2）
/// - `/Subtype /Movie`、`/S /Movie` → MovieObjects（1.2）
/// - `/Subtype /Markup3D` → ThreeDMarkup（1.7）
/// - `/Subtype /PrinterMark`，或 `/Subtype /Form` 带 `/MarkStyle` `/Colorants`
///   之一 → PrinterMarks（1.4）
/// - `/Subtype /TrapNet` → TrapNetworks（1.3），带 `/LastModified` 时另记
///   TrapNetworkModificationDate（1.4）；`/Subtype /Form` 带 `/PCM`
///   `/SeparationColorNames` `/TrapRegions` `/TrapStyles` `/FontFauxing`
///   之一 → TrapNetworks（1.3）
/// - 图像或表单 XObject 的 `/OPI` → OpiDictionaries（1.2，14.11.7 Table 368）
/// - `/Type /Rendition`、`/Type /MediaClip`、`/S /Rendition` → Renditions（1.5）
/// - 3D 视图字典（`/Type /3DView` 或 `/XN`）的 `/RM` `/LS` `/SA` `/NA`
///   `/NR` 任一键 → ThreeDViewExtensions（1.7）
/// - `/Filter` 中的 JPXDecode → JPXDecode，JBIG2Decode → JBIG2Decode
/// - `/ShadingType` 或 `/PatternType 2` → Shading
/// - `/PatternType 1` → TilingPattern
/// - `/SMask`、`/SMaskInData`、`/ca`、`/CA`、`/BM` → Transparency
/// - `/Matte` → SoftMaskMatte（1.6）
/// - 图像字典（`/Subtype /Image`）的 `/Intent` → ImageIntent（1.4）
/// - 图像字典的 `/Alternates` → AlternateImages（1.3）
/// - `/OCProperties` 键存在 → OptionalContent
/// - `/Threads` 键存在（目录）→ ArticleThreads
/// - `/Collection` 键存在（目录）→ Collection
/// - `/Templates` 键存在（名称字典）→ PageTemplates
/// - `/AlternatePresentations` 键存在（名称字典）→ AlternatePresentations
/// - 目录 `/OutputIntents` 非空数组 → OutputIntents（1.4）
/// - 目录 `/Lang` → DocumentLanguage（1.4，14.9.2.1）
/// - 目录 `/StructTreeRoot` 或 `/Type /StructTreeRoot`、结构元素字典 →
///   LogicalStructure（1.3）；结构元素的 `/Lang` `/ActualText` →
///   StructureLanguageText（1.4）、`/E` → StructureExpandedText（1.5）；
///   结构元素的 `/S` 或结构树根 `/RoleMap` 的任何目标为 PDF 1.5 扩展
///   标准类型（THead/TBody/TFoot/Annot/Ruby/RB/RT/RP/Warichu/WT/WP）→
///   StructureTypeExtensions（1.5，14.8.4）
/// - 结构属性对象带 `/O /UserProperties` 与 `/P` → StructureUserProperties
///   （1.6，14.7.5.3 Table 328；子属性字典只有 `/N` `/V`，无 `/O`）
/// - 标准结构属性（14.8.5.2 Table 341）：`/O /List` 带 `/ListNumbering`、
///   `/O /PrintField` 带 `/Role` `/checked` `/Desc` 之一、`/O /Table` 带
///   `/RowSpan` `/ColSpan` `/Headers` `/Scope` `/Summary` 之一 →
///   StandardStructureAttributes（1.4）；PrintField → PrintFieldAttributes
///   （1.7）、Table 的 `/Headers` `/Scope` → TableHeaderAttributes（1.5）、
///   `/Summary` → TableSummary（1.7）
/// - 目录 `/MarkInfo` → MarkInfo（1.4）；其 `/UserProperties` `/Suspects`
///   → MarkInfoUserProperties / MarkInfoSuspects（1.6）
/// - 页面字典的 `/BleedBox` `/TrimBox` `/ArtBox` → PageBoundaries（1.3）、
///   `/BoxColorInfo` → PageBoxColorInfo（1.4）、`/SeparationInfo` →
///   SeparationInfo（1.3，14.11.4 Table 364）
/// - `/Encrypt` 键存在 → EncryptionRC4
pub(crate) fn scan_dict_features(features: &mut FeatureProfile, d: &Dict) {
    inspect_dict_features(features, d);
    // 显式栈遍历子项：避免深嵌套输入触发递归栈溢出。
    let mut stack: Vec<&Value> = d.0.values().collect();
    while let Some(value) = stack.pop() {
        match value {
            Value::Dict(inner) | Value::Stream(inner, _) => {
                inspect_dict_features(features, inner);
                stack.extend(inner.0.values());
            }
            Value::Array(a) => stack.extend(a.iter()),
            _ => {}
        }
    }
}

/// Layout 标准属性（14.8.5.4 Table 342–346）用于识别 `/O /Layout` 的条目。
const LAYOUT_KEYS: &[&str] = &[
    "Placement",
    "WritingMode",
    "BackgroundColor",
    "BorderColor",
    "BorderStyle",
    "BorderThickness",
    "Color",
    "Padding",
    "SpaceBefore",
    "SpaceAfter",
    "StartIndent",
    "EndIndent",
    "TextIndent",
    "TextAlign",
    "BBox",
    "Width",
    "Height",
    "BlockAlign",
    "InlineAlign",
    "TBorderStyle",
    "TPadding",
    "BaselineShift",
    "LineHeight",
    "TextDecorationColor",
    "TextDecorationThickness",
    "TextDecorationType",
    "RubyAlign",
    "RubyPosition",
    "GlyphOrientationVertical",
    "ColumnCount",
    "ColumnGap",
    "ColumnWidths",
];

/// Layout 中 PDF 1.5 引入的 Table 343/345 条目。
const LAYOUT_EXTENDED_KEYS: &[&str] = &[
    "BackgroundColor",
    "BorderColor",
    "BorderStyle",
    "BorderThickness",
    "Padding",
    "Color",
    "TBorderStyle",
    "TPadding",
    "TextDecorationColor",
    "TextDecorationThickness",
    "RubyAlign",
    "RubyPosition",
    "GlyphOrientationVertical",
];

/// Layout 中 PDF 1.6 引入的 Table 346 分栏条目。
const LAYOUT_COLUMN_KEYS: &[&str] = &["ColumnCount", "ColumnGap", "ColumnWidths"];

/// PDF 1.5 引入的标准结构类型（14.8.4）：`THead` `TBody` `TFoot`
/// （Table 337）、`Annot`（Table 338）与 Ruby/Warichu 家族（Table 339）。
pub(crate) const EXTENDED_STRUCTURE_TYPES: &[&str] = &[
    "THead", "TBody", "TFoot", "Annot", "Ruby", "RB", "RT", "RP", "Warichu", "WT", "WP",
];

/// 名字是否为 PDF 1.5 引入的标准结构类型。
fn is_extended_structure_type(name: &str) -> bool {
    EXTENDED_STRUCTURE_TYPES.contains(&name)
}

/// 只检查**单个字典自身**的键（不深入其值），供显式栈遍历调用。
fn inspect_dict_features(features: &mut FeatureProfile, d: &Dict) {
    // XRef 流：/Type /XRef
    if d.get("Type").and_then(|v| v.as_name()) == Some("XRef") {
        features.insert(PdfFeature::XRefStream);
    }
    // 对象流：/Type /ObjStm
    if d.get("Type").and_then(|v| v.as_name()) == Some("ObjStm") {
        features.insert(PdfFeature::ObjectStream);
    }
    // 声音（PDF 1.2）：声音流的 /Type /Sound（13.3 Table 294）、Sound
    // 注解的 /Subtype /Sound（Table 185）与 Sound 动作的 /S /Sound
    // （Table 208）。三者均为静态名称标记；仅按语法命中，个别同名
    // 但非声音用途的字典只会抬高版本画像，不影响语义。
    if d.get("Type").and_then(|v| v.as_name()) == Some("Sound")
        || d.get("Subtype").and_then(|v| v.as_name()) == Some("Sound")
        || (d.get("S").and_then(|v| v.as_name()) == Some("Sound") && d.0.contains_key("Sound"))
    {
        features.insert(PdfFeature::SoundObjects);
    }
    // 电影（PDF 1.2）：Movie 注解的 /Subtype /Movie（Table 186）与 Movie
    // 动作的 /S /Movie（Table 209）。电影字典本身没有 /Type 或
    // /Subtype 条目（Table 295），只随上述两种引用出现。
    if d.get("Subtype").and_then(|v| v.as_name()) == Some("Movie")
        || d.get("S").and_then(|v| v.as_name()) == Some("Movie")
    {
        features.insert(PdfFeature::MovieObjects);
    }
    // 3D 标记（PDF 1.7）：注解 `/ExData` 的外部数据字典以
    // `/Subtype /Markup3D` 标识（13.6.6 Table 313）。`/Type` 按表为
    // ExData、ISO 示例写作 Markup3D，写法不唯一，故只按 `/Subtype` 命中。
    if d.get("Subtype").and_then(|v| v.as_name()) == Some("Markup3D") {
        features.insert(PdfFeature::ThreeDMarkup);
    }
    // 打印机标记（PDF 1.4）：PrinterMark 注解 `/Subtype /PrinterMark`
    // （14.11.3 Table 362），以及带 `/MarkStyle` 或 `/Colorants` 的 Form
    // XObject（Table 363）。`/Colorants` 也出现在 NChannel ICCBased 配置
    // 流中，故表单条目必须先确认 `/Subtype /Form`。
    let subtype = d.get("Subtype").and_then(|v| v.as_name());
    if subtype == Some("PrinterMark")
        || (subtype == Some("Form")
            && (d.0.contains_key("MarkStyle") || d.0.contains_key("Colorants")))
    {
        features.insert(PdfFeature::PrinterMarks);
    }
    // 陷印网络（PDF 1.3）：TrapNet 注解 `/Subtype /TrapNet`（14.11.6
    // Table 366）与陷印网络 Form XObject（Table 367）。跟踪条目
    // `/LastModified` 为 PDF 1.4 新增（Table 366 注 2），单独登记。
    // 表单条目只对 `/Subtype /Form` 判断，避免把同名键的其它字典误判。
    if subtype == Some("TrapNet") {
        features.insert(PdfFeature::TrapNetworks);
        if d.0.contains_key("LastModified") {
            features.insert(PdfFeature::TrapNetworkModificationDate);
        }
    }
    if subtype == Some("Form")
        && [
            "PCM",
            "SeparationColorNames",
            "TrapRegions",
            "TrapStyles",
            "FontFauxing",
        ]
        .iter()
        .any(|key| d.0.contains_key(*key))
    {
        features.insert(PdfFeature::TrapNetworks);
    }
    // OPI 代理字典（PDF 1.2）：图像与表单 XObject 的 `/OPI`（14.11.7
    // Table 368–370）。仅对 XObject 子类型判断，避免把同名键的其它
    // 字典误判为 OPI。
    if matches!(subtype, Some("Image") | Some("Form")) && d.0.contains_key("OPI") {
        features.insert(PdfFeature::OpiDictionaries);
    }
    // 多媒体（PDF 1.5）：rendition 对象的 /Type /Rendition（Table 266）、
    // media clip 的 /Type /MediaClip（Table 273）与 Rendition 动作的
    // /S /Rendition（Table 214）。
    if d.get("Type").and_then(|v| v.as_name()) == Some("Rendition")
        || d.get("Type").and_then(|v| v.as_name()) == Some("MediaClip")
        || d.get("S").and_then(|v| v.as_name()) == Some("Rendition")
    {
        features.insert(PdfFeature::Renditions);
    }
    // 3D 视图的 PDF 1.7 条目（13.6.4.1 Table 304）：视图字典以
    // `/Type /3DView`（写端总是写出）或必填 `/XN` 文本串标识，出现
    // `/RM` `/LS` `/SA` `/NA` `/NR` 任一键即命中。`/SA` 亦出现在图形
    // 状态字典中，故必须先确认视图字典标识，避免误判。
    if (d.get("Type").and_then(|v| v.as_name()) == Some("3DView") || d.0.contains_key("XN"))
        && ["RM", "LS", "SA", "NA", "NR"]
            .iter()
            .any(|key| d.0.contains_key(*key))
    {
        features.insert(PdfFeature::ThreeDViewExtensions);
    }
    // 图像过滤器（JPX/JBIG2，其余通用过滤器不记）
    if let Some(f) = image_filter_feature(d) {
        features.insert(f);
    }
    // 图案：/PatternType 1 为平铺图案（PDF 1.2），/PatternType 2 为
    // 着色图案，依赖 PDF 1.3 的渐变。
    match d.get("PatternType").and_then(|v| v.as_integer()) {
        Some(1) => {
            features.insert(PdfFeature::TilingPattern);
        }
        Some(2) => {
            features.insert(PdfFeature::Shading);
        }
        _ => {}
    }
    // 独立渐变字典（Type 1–7）。
    if d.0.contains_key("ShadingType") {
        features.insert(PdfFeature::Shading);
    }
    // 透明度相关键
    for key in ["SMask", "SMaskInData", "ca", "CA", "BM"] {
        if d.0.contains_key(key) {
            features.insert(PdfFeature::Transparency);
            break;
        }
    }
    // 软蒙版衬底颜色（PDF 1.6）
    if d.0.contains_key("Matte") {
        features.insert(PdfFeature::SoftMaskMatte);
    }
    // 图像专属条目。/Intent 亦出现在可选内容组（OCG）字典中，故仅对
    // 图像字典登记，避免把 OCG 的意图误判成 PDF 1.4 图像特性。
    if d.get("Subtype").and_then(|v| v.as_name()) == Some("Image") {
        if d.0.contains_key("Intent") {
            features.insert(PdfFeature::ImageIntent);
        }
        if d.0.contains_key("Alternates") {
            features.insert(PdfFeature::AlternateImages);
        }
    }
    // 可选内容
    if d.0.contains_key("OCProperties") {
        features.insert(PdfFeature::OptionalContent);
    }
    // 文章线（PDF 1.1）：目录 /Threads
    if d.0.contains_key("Threads") {
        features.insert(PdfFeature::ArticleThreads);
    }
    // 便携集合（PDF 1.7）：目录 /Collection
    if d.0.contains_key("Collection") {
        features.insert(PdfFeature::Collection);
    }
    // 命名页面模板（PDF 1.3）：目录 /Names /Templates 名称树。`Templates`
    // 键只出现在名称字典中（Table 31），扫描到名称字典即命中。
    if d.0.contains_key("Templates") {
        features.insert(PdfFeature::PageTemplates);
    }
    // 备用演示（PDF 1.4）：目录 /Names /AlternatePresentations 名称树。
    // 与 `Templates` 相同，该键只出现在名称字典中（Table 31），扫描到
    // 名称字典即命中。
    if d.0.contains_key("AlternatePresentations") {
        features.insert(PdfFeature::AlternatePresentations);
    }
    // 逻辑结构（PDF 1.3）：目录 /StructTreeRoot（14.7.2 Table 322）、
    // 结构树根字典本身与结构元素（Table 323）。结构元素以 /Type
    // /StructElem 标识；部分生产者省略 /Type，故同时接受 /S 名字 +
    // /P 引用 + /K 的组合。
    if d.0.contains_key("StructTreeRoot")
        || d.get("Type").and_then(|v| v.as_name()) == Some("StructTreeRoot")
    {
        features.insert(PdfFeature::LogicalStructure);
        // 扩展标准类型（PDF 1.5）：/RoleMap 任一目标为 1.5 名字时，映射后
        // 的元素同样写出 1.5 类型名。值非名字的条目忽略（14.8.4.1）。
        if let Some(Value::Dict(role_map)) = d.get("RoleMap")
            && role_map
                .0
                .values()
                .filter_map(Value::as_name)
                .any(is_extended_structure_type)
        {
            features.insert(PdfFeature::StructureTypeExtensions);
        }
    }
    let struct_elem = d.get("Type").and_then(|v| v.as_name()) == Some("StructElem")
        || (d.get("S").and_then(|v| v.as_name()).is_some()
            && d.get("P").is_some_and(|v| v.as_ref().is_some())
            && d.0.contains_key("K"));
    if struct_elem {
        features.insert(PdfFeature::LogicalStructure);
        // 扩展标准类型（PDF 1.5）：/S 直接是 1.5 名字（14.8.4 Table 337–339）。
        if d.get("S")
            .and_then(Value::as_name)
            .is_some_and(is_extended_structure_type)
        {
            features.insert(PdfFeature::StructureTypeExtensions);
        }
        // 语言与替换文本（PDF 1.4）：Table 323 的 /Lang /ActualText。
        if d.0.contains_key("Lang") || d.0.contains_key("ActualText") {
            features.insert(PdfFeature::StructureLanguageText);
        }
        // 缩写展开（PDF 1.5）：Table 323 的 /E。
        if d.0.contains_key("E") {
            features.insert(PdfFeature::StructureExpandedText);
        }
    }
    // 标记信息（PDF 1.4）：目录 /MarkInfo（14.7.1 Table 321）。两个
    // 子条目自 PDF 1.6 起；写出时仅 true 才物化，故按键存在登记。
    if let Some(info) = d.get("MarkInfo").and_then(Value::as_dict) {
        features.insert(PdfFeature::MarkInfo);
        if info.0.contains_key("UserProperties") {
            features.insert(PdfFeature::MarkInfoUserProperties);
        }
        if info.0.contains_key("Suspects") {
            features.insert(PdfFeature::MarkInfoSuspects);
        }
    }
    // 用户属性（PDF 1.6）：14.7.5.3 的用户属性属性对象以 `/O
    // /UserProperties` 与 `/P` 数组标识（Table 328）。子属性字典只有
    // `/N` `/V`（Table 329）、没有 `/O`，故不会重复命中。
    if d.get("O").and_then(|v| v.as_name()) == Some("UserProperties") && d.0.contains_key("P") {
        features.insert(PdfFeature::StructureUserProperties);
    }
    // 标准结构属性（PDF 1.4）：14.8.5.2 Table 341 的所有者标记。仅按
    // 所有者无法与其它 `/O` 属性对象区分，故要求出现该所有者的标准条目：
    // List 需 `/ListNumbering`（Table 347）、PrintField 需 `/Role`
    // `/checked` `/Desc` 之一（Table 348，PDF 1.7）、Table 需 `/RowSpan`
    // `/ColSpan` `/Headers` `/Scope` `/Summary` 之一（Table 349）、Layout
    // 需 Table 342–346 的任一布局条目。
    // `/Headers` `/Scope` 为 PDF 1.5、`/Summary` 与 PrintField 为 PDF 1.7；
    // Layout 的 1.5/1.6 条目录入对应子特性。
    match d.get("O").and_then(|v| v.as_name()) {
        Some("List") if d.0.contains_key("ListNumbering") => {
            features.insert(PdfFeature::StandardStructureAttributes);
        }
        Some("PrintField")
            if ["Role", "checked", "Desc"]
                .iter()
                .any(|key| d.0.contains_key(*key)) =>
        {
            features.insert(PdfFeature::StandardStructureAttributes);
            features.insert(PdfFeature::PrintFieldAttributes);
        }
        Some("Table")
            if ["RowSpan", "ColSpan", "Headers", "Scope", "Summary"]
                .iter()
                .any(|key| d.0.contains_key(*key)) =>
        {
            features.insert(PdfFeature::StandardStructureAttributes);
            if d.0.contains_key("Headers") || d.0.contains_key("Scope") {
                features.insert(PdfFeature::TableHeaderAttributes);
            }
            if d.0.contains_key("Summary") {
                features.insert(PdfFeature::TableSummary);
            }
        }
        Some("Layout") if LAYOUT_KEYS.iter().any(|key| d.0.contains_key(*key)) => {
            features.insert(PdfFeature::StandardStructureAttributes);
            if LAYOUT_EXTENDED_KEYS
                .iter()
                .any(|key| d.0.contains_key(*key))
            {
                features.insert(PdfFeature::LayoutExtendedAttributes);
            }
            if LAYOUT_COLUMN_KEYS.iter().any(|key| d.0.contains_key(*key)) {
                features.insert(PdfFeature::LayoutColumnAttributes);
            }
        }
        _ => {}
    }
    // 文档默认语言（PDF 1.4）：目录 /Lang（14.9.2.1）。结构元素的 /Lang
    // 是另一项特性（StructureLanguageText），两者不混用。
    if d.get("Type").and_then(|v| v.as_name()) == Some("Catalog") && d.get("Lang").is_some() {
        features.insert(PdfFeature::DocumentLanguage);
    }
    // 输出意图（PDF 1.4）：目录 /OutputIntents 非空数组（14.11.5 Table
    // 365）。空数组等价于未声明，不登记特性。
    if d.get("Type").and_then(|v| v.as_name()) == Some("Catalog")
        && matches!(d.get("OutputIntents"), Some(Value::Array(array)) if !array.is_empty())
    {
        features.insert(PdfFeature::OutputIntents);
    }
    // 页面处理框（PDF 1.3）：14.11.2 Table 30 的 /BleedBox /TrimBox
    // /ArtBox。三键也出现在 /BoxColorInfo 的子字典中（Table 360），故须
    // 先确认页面字典：/Type /Page，或同时带 /MediaBox 与处理框/分色
    // 字典（部分生产者的页面字典省略 /Type）。
    let processing_box = ["BleedBox", "TrimBox", "ArtBox"]
        .iter()
        .any(|key| d.0.contains_key(*key));
    let page_dict = d.get("Type").and_then(|v| v.as_name()) == Some("Page")
        || (d.0.contains_key("MediaBox") && (processing_box || d.0.contains_key("SeparationInfo")));
    if page_dict {
        if processing_box {
            features.insert(PdfFeature::PageBoundaries);
        }
        // 框颜色信息（PDF 1.4）：页面 /BoxColorInfo（Table 360）。
        if d.0.contains_key("BoxColorInfo") {
            features.insert(PdfFeature::PageBoxColorInfo);
        }
        // 分色字典（PDF 1.3）：页面 /SeparationInfo（14.11.4 Table 364）。
        if d.0.contains_key("SeparationInfo") {
            features.insert(PdfFeature::SeparationInfo);
        }
    }
    // 加密：trailer /Encrypt（标准安全处理器自 PDF 1.1 引入）
    if d.0.contains_key("Encrypt") {
        features.insert(PdfFeature::EncryptionRC4);
    }
}

/// 从任意 PDF 值里扫描特性（显式栈遍历，非递归）。
pub(crate) fn scan_value_features(features: &mut FeatureProfile, value: &Value) {
    let mut stack: Vec<&Value> = vec![value];
    while let Some(v) = stack.pop() {
        match v {
            Value::Dict(d) | Value::Stream(d, _) => {
                inspect_dict_features(features, d);
                stack.extend(d.0.values());
            }
            Value::Array(a) => stack.extend(a.iter()),
            _ => {}
        }
    }
}

/// 从流/字典的 `/Filter` 提取高版本图像过滤器特性（取最高者）。
/// 仅 JPXDecode（1.5）和 JBIG2Decode（1.4）——Flate/ASCIIHex/RunLength
/// 等通用压缩过滤器不影响版本，故不返回。
fn image_filter_feature(d: &Dict) -> Option<PdfFeature> {
    let v = d.get("Filter").or_else(|| d.get("F"))?;
    let mut best: Option<PdfFeature> = None;
    let mut consider = |name: &str| {
        let f = match name {
            "JPXDecode" | "JPX" => PdfFeature::JPXDecode,
            "JBIG2Decode" | "JBIG2" => PdfFeature::JBIG2Decode,
            _ => return,
        };
        if best
            .map(|b| f.min_version() > b.min_version())
            .unwrap_or(true)
        {
            best = Some(f);
        }
    };
    match v {
        Value::Name(n) => consider(n),
        Value::Array(a) => {
            for item in a.iter() {
                if let Value::Name(n) = item {
                    consider(n);
                }
            }
        }
        _ => {}
    }
    best
}
