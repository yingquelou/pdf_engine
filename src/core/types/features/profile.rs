use std::collections::BTreeSet;

use super::super::version::PdfFeature;

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
