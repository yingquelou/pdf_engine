use super::super::value::{Dict, Value};
use super::profile::FeatureProfile;

/// 从任意 PDF 值里扫描特性（显式栈遍历，非递归）。
pub(crate) fn scan_value_features(features: &mut FeatureProfile, value: &Value) {
    let mut stack: Vec<&Value> = vec![value];
    while let Some(v) = stack.pop() {
        match v {
            Value::Dict(d) | Value::Stream(d, _) => {
                super::inspect::inspect_dict_features(features, d);
                stack.extend(d.0.values());
            }
            Value::Array(a) => stack.extend(a.iter()),
            _ => {}
        }
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
/// - 注解字典（`/Type /Annot`）的 `/Subj` `/IRT` → AnnotationSubject /
///   AnnotationInReplyTo（1.5），`/RT` → AnnotationReplyType（1.6）
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
/// - `/Extensions` 键存在（目录）→ ExtensionsDictionary
/// - `/Requirements` 键存在（目录）→ DocumentRequirements（1.7，12.10
///   Table 264/265）
/// - `/ViewerPreferences` 键存在（目录）→ ViewerPreferences 基档（1.2），并按键
///   登记 1.3–1.7 各档；`/Enforce` 为 PDF 2.0 扩展档（32000-1 无此条目）
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
///   `/RowSpan` `/ColSpan` `/Headers` `/Scope` `/Summary` 之一、或 `/O` 为
///   Table 341 的 7 个格式专用所有者之一 →
///   StandardStructureAttributes（1.4）；PrintField → PrintFieldAttributes
///   （1.7）、Table 的 `/Headers` `/Scope` → TableHeaderAttributes（1.5）、
///   `/Summary` → TableSummary（1.7）
/// - 目录 `/MarkInfo` → MarkInfo（1.4）；其 `/UserProperties` `/Suspects`
///   → MarkInfoUserProperties / MarkInfoSuspects（1.6）
/// - `/Type /FontDescriptor` 带 `/FontFamily` `/FontStretch` `/FontWeight`
///   任一键 → FontSelectorAttributes（1.5，Table 122 条目，14.8.2.4.3
///   Table 332 属性的文件语法来源）
/// - 页面字典的 `/BleedBox` `/TrimBox` `/ArtBox` → PageBoundaries（1.3）、
///   `/BoxColorInfo` → PageBoxColorInfo（1.4）、`/SeparationInfo` →
///   SeparationInfo（1.3，14.11.4 Table 364）
/// - `/Encrypt` 键存在 → EncryptionRC4
pub(crate) fn scan_dict_features(features: &mut FeatureProfile, d: &Dict) {
    super::inspect::inspect_dict_features(features, d);
    // 显式栈遍历子项：避免深嵌套输入触发递归栈溢出。
    let mut stack: Vec<&Value> = d.0.values().collect();
    while let Some(value) = stack.pop() {
        match value {
            Value::Dict(inner) | Value::Stream(inner, _) => {
                super::inspect::inspect_dict_features(features, inner);
                stack.extend(inner.0.values());
            }
            Value::Array(a) => stack.extend(a.iter()),
            _ => {}
        }
    }
}
