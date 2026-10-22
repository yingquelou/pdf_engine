//! writer 输出区间与内容-版本一致性校验。

use crate::pdf::types::{Document, PdfFeature, PdfVersion, scan_dict_features};

use super::WriteError;

/// writer 目前接受的**输出目标版本**集合：1.0–1.7。
///
/// 这是「版本号是否在支持范围内」的集中表达。注意此处仅约束输出头部
/// 可用的版本区间（2.0 虽在能力模型中受支持，但不在本 writer 的
/// 输出区间内，故被拒）；**某个内容特性是否被目标版本允许**由
/// 内部校验函数经 [`PdfVersion::supports`] 判定。
const SUPPORTED_OUTPUT_VERSIONS: [PdfVersion; 8] = [
    PdfVersion::V1_0,
    PdfVersion::V1_1,
    PdfVersion::V1_2,
    PdfVersion::V1_3,
    PdfVersion::V1_4,
    PdfVersion::V1_5,
    PdfVersion::V1_6,
    PdfVersion::V1_7,
];

/// 校验目标版本在 writer 支持范围内（见 [`SUPPORTED_OUTPUT_VERSIONS`]）。
pub(super) fn validate_output_version(v: PdfVersion) -> Result<(), WriteError> {
    if SUPPORTED_OUTPUT_VERSIONS.contains(&v) {
        Ok(())
    } else {
        Err(WriteError::UnsupportedVersion(format!(
            "{} is outside the supported range 1.0-1.7",
            v.as_str()
        )))
    }
}

/// 内容-版本一致性校验（**严出**）：直接读取 Document 的
/// `FeatureProfile`（读端已填充、writer 预加载后覆盖完整画像），
/// 若特性所需最低版本高于目标版本则返回 [`WriteError::UnsupportedVersion`]。
///
/// 仅检查**文档内容**里的特性：
/// - 流/字典 `/Filter` 为 `/JPXDecode`（1.5）或 `/JBIG2Decode`（1.4）；
/// - 透明度（1.4）：`/SMask`、`/SMaskInData`，或 ExtGState 的 `/ca`
///   `/CA` `/BM`；
/// - 可选内容（1.5）：根 Catalog 的 `/OCProperties`；
/// - 加密（1.1）：trailer `/Encrypt`。
///
/// writer 自身为满足目标版本而选择的 XRef 流/对象流**不在**扫描范围：
/// 它们由目标版本能力决定，本身必然 ≤ 目标版本，不存在「越版」。
///
/// **数据源**：直接读取 Document 已填充的 `FeatureProfile`（读端解析时
/// 自动扫描填充，writer 构建前调 `Document::load_all_objects` 强制全量）。
/// 不再重新遍历对象，避免两端重复实现相同的特性检测逻辑。
pub(super) fn validate_content_version(
    version: PdfVersion,
    doc: &Document,
) -> Result<(), WriteError> {
    // 先检查 doc.features（读端已填充、writer 预加载后覆盖完整画像）。
    // 再额外扫 trailer.dict —— 写入端构造的 Document 可能在构造后手动
    // 修改了 trailer（加 /Encrypt 等），但 features 不会自动同步。
    let mut features = doc.features.borrow().clone();
    scan_dict_features(&mut features, &doc.trailer.dict);

    for f in features.iter() {
        // 结构性特性（ObjStm / XRef 流）由 writer 自身的策略模式自动降级，
        // 不是不可降级特性——不在此报错。
        match f {
            PdfFeature::ObjectStream | PdfFeature::XRefStream => continue,
            _ => {}
        }
        if !version.supports(*f) {
            return Err(WriteError::UnsupportedVersion(format!(
                "document content uses {f}: needs PDF {} or later, but the target version is {}",
                f.min_version(),
                version.as_str()
            )));
        }
    }
    Ok(())
}

// ── writer 本地特性扫描函数已迁移至 `types.rs::scan_dict_features` /
// `scan_value_features`，由 reader 和 writer 共用，避免重复实现。
// writer 自身只需消费 doc.features（FeatureProfile），无需重新扫描。
