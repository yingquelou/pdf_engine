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
