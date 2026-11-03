use super::super::value::{Dict, Value};
use super::super::version::PdfFeature;
use super::profile::FeatureProfile;

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
pub(super) fn inspect_dict_features(features: &mut FeatureProfile, d: &Dict) {
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
    // 标记类注解的附加通用条目（12.5.6.2 Table 170）：`/Subj` 与 `/IRT`
    // 为 PDF 1.5，`/RT` 为 PDF 1.6。只在 `/Type /Annot` 的字典上登记，
    // 避免别处的同名键误判。
    if d.get("Type").and_then(|v| v.as_name()) == Some("Annot") {
        if d.0.contains_key("Subj") {
            features.insert(PdfFeature::AnnotationSubject);
        }
        if d.0.contains_key("IRT") {
            features.insert(PdfFeature::AnnotationInReplyTo);
        }
        if d.0.contains_key("RT") {
            features.insert(PdfFeature::AnnotationReplyType);
        }
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
    // 扩展字典（ISO 32000-1，即 PDF 1.7）：目录 /Extensions（7.12）。
    if d.0.contains_key("Extensions") {
        features.insert(PdfFeature::ExtensionsDictionary);
    }
    // 文档需求（PDF 1.7）：目录 /Requirements（12.10、Table 264/265；
    // Table 28 raw L3692 标 (Optional; PDF 1.7)）。与相邻的 Collection /
    // Extensions 同口径：**按键存在登记，不校验值形态**——非数组、元素非
    // 字典等畸形同样把文件推高到 1.7（画像的职责是版本下限，不是合规
    // 判定），类型化违例由读端 `Document::requirements` 的
    // `RequirementsIssue` 负责。`/Requirements` 只出现在目录（Table 28），
    // 故无需子类型上下文。
    if d.0.contains_key("Requirements") {
        features.insert(PdfFeature::DocumentRequirements);
    }
    // 查看器首选项（12.2 Table 150）：/ViewerPreferences 按 1.2 基档登记，
    // 条目按键登记各自引入档位。
    if d.0.contains_key("ViewerPreferences") {
        features.insert(PdfFeature::ViewerPreferences);
        if let Some(p) = d.get("ViewerPreferences").and_then(Value::as_dict) {
            if p.0.contains_key("Direction") {
                features.insert(PdfFeature::ViewerReadingDirection);
            }
            if p.0.contains_key("DisplayDocTitle") {
                features.insert(PdfFeature::ViewerDisplayDocTitle);
            }
            if ["ViewArea", "ViewClip", "PrintArea", "PrintClip"]
                .iter()
                .any(|k| p.0.contains_key(*k))
            {
                features.insert(PdfFeature::ViewerPageBoundaryNames);
            }
            if p.0.contains_key("PrintScaling") {
                features.insert(PdfFeature::ViewerPrintScaling);
            }
            if ["Duplex", "PickTrayByPDFSize", "PrintPageRange", "NumCopies"]
                .iter()
                .any(|k| p.0.contains_key(*k))
            {
                features.insert(PdfFeature::ViewerPrintDialogPreferences);
            }
            if p.0.contains_key("Enforce") {
                features.insert(PdfFeature::ViewerEnforcement);
            }
        }
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
        // 扩展标准类型（PDF 1.5）：/RoleMap 任一目标为 1.5 名字，映射后
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
    // 字体选择器属性（PDF 1.5）：Table 122 的字体描述符 /FontFamily
    // /FontStretch /FontWeight 条目，是 14.8.2.4.3 Table 332 属性的文件
    // 语法来源；按键存在登记（值形态错误由读端派生时另行报错）。
    if d.get("Type").and_then(Value::as_name) == Some("FontDescriptor")
        && (d.0.contains_key("FontFamily")
            || d.0.contains_key("FontStretch")
            || d.0.contains_key("FontWeight"))
    {
        features.insert(PdfFeature::FontSelectorAttributes);
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
        // Table 341 的格式专用所有者：条目集合开放式，所有者名字本身即可
        // 判定标准结构属性使用。
        Some(
            "XML-1.00" | "HTML-3.20" | "HTML-4.01" | "OEB-1.00" | "RTF-1.05" | "CSS-1.00"
            | "CSS-2.00",
        ) => {
            features.insert(PdfFeature::StandardStructureAttributes);
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
