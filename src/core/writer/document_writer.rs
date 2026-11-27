//! `DocumentWriter` 编排与 `write_document` 兼容入口。

use crate::core::types::{Document, PdfFeature, PdfVersion};
use std::io::Write;

use super::WriteError;
use super::compression::StreamCompression;
use super::context::WriteContext;
use super::hybrid::{self, HybridReferencePolicy};
use super::incremental;
use super::serialize::{
    ClassicXrefSerializer, ObjectLayout, ObjectStreamLayout, PlainObjectLayout, XrefSerializer,
    XrefStreamSerializer,
};
use super::version::{validate_content_version, validate_output_version};

/// 文档序列化器（Builder 风格）。
///
/// 封装写入的各阶段（头部 → 对象/对象流 → 交叉引用 → startxref/%%EOF），
/// 使每阶段独立可测。默认配置即等价于 [`write_document`] 兼容入口。
///
/// **优先较新特性**：目标版本 ≥1.5 时输出 XRef 流（并打包非流对象为
/// 对象流）；否则输出经典 xref 表 + trailer。头部写 `%PDF-{version}`。
#[derive(Debug, Clone, Default)]
pub struct DocumentWriter {
    version: Option<PdfVersion>,
    /// D4 流压缩策略；未设置时输出保持明文透传（既有行为）。
    compression: Option<StreamCompression>,
    /// E7 混合引用隐藏策略；仅 [`Self::build_hybrid`] 消费，`build` 忽略。
    hybrid_policy: Option<HybridReferencePolicy>,
}

impl DocumentWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// 指定目标版本；未指定则用 writer 自身默认值（`PdfVersion::default()`，即 1.7）。
    pub fn with_version(mut self, v: PdfVersion) -> Self {
        self.version = Some(v);
        self
    }

    /// 启用写端流压缩（D4）：所有未声明 `/Filter` 的明文流在序列化时按
    /// `FlateDecode` 压缩写出。`FlateDecode` 要求目标版本 ≥1.2；启用图像
    /// PNG 预测器要求 ≥1.3；不满足时报 [`WriteError::UnsupportedVersion`]
    /// 且不写出任何字节。压缩后端失败报 [`WriteError::Compression`]，
    /// 同样零字节，不静默回退明文。
    pub fn with_compression(mut self, compression: StreamCompression) -> Self {
        self.compression = Some(compression);
        self
    }

    /// 序列化整个 [`Document`] 到 `w`。
    ///
    /// 输出形态按目标版本经能力模型选择（策略模式）：
    /// - 目标版本 `supports(PdfFeature::ObjectStream)` → 对象流打包布局；
    /// - 目标版本 `supports(PdfFeature::XRefStream)` → XRef 流交叉引用；
    /// - 否则 → 朴素布局 + 经典交叉引用表。
    pub fn build(&self, doc: &Document, w: &mut dyn Write) -> Result<(), WriteError> {
        if doc.is_encrypted() {
            return Err(WriteError::EncryptedDocument);
        }
        // 若 Document 有源文件（读端），自动预加载全部对象。
        // 失败只记 warning，不中断。load_all_objects 同时填充完整
        // FeatureProfile（读端特性画像的唯一数据源）。
        if doc.source.is_some()
            && let Err(e) = doc.load_all_objects()
        {
            doc.push_warning(format!(
                "DocumentWriter: load_all_objects failed: {e}; continuing with partial cache"
            ));
        }
        // 默认版本用 writer 自身的 1.7（PdfVersion::default），**不再**
        // 继承 doc.source_version——写端与读端版本彻底解耦。
        let version = self.version.unwrap_or_default();
        validate_output_version(version)?;
        validate_content_version(version, doc)?;
        // 版本门控：按流过滤器引入版本（Flate 1.2，其余 1.0），
        // PNG 预测器为 1.3 引入。
        if let Some(comp) = &self.compression {
            if version < comp.min_version() {
                return Err(WriteError::UnsupportedVersion(format!(
                    "{} stream encoding requires PDF {}, got {version}",
                    comp.filter().name(),
                    comp.min_version()
                )));
            }
            if comp.uses_image_predictor() && version < PdfVersion::V1_3 {
                return Err(WriteError::UnsupportedVersion(format!(
                    "PNG image predictor requires PDF 1.3, got {version}"
                )));
            }
        }

        // 策略选择：唯一事实源为版本能力模型（PdfVersion::supports）。
        let layout: Box<dyn ObjectLayout> = if version.supports(PdfFeature::ObjectStream) {
            Box::new(ObjectStreamLayout)
        } else {
            Box::new(PlainObjectLayout)
        };
        let xref: Box<dyn XrefSerializer> = if version.supports(PdfFeature::XRefStream) {
            Box::new(XrefStreamSerializer)
        } else {
            Box::new(ClassicXrefSerializer)
        };

        let mut objs = doc.objects_for_write();

        // 剔除源文档遗留的**结构性对象**：对象流（`/Type /ObjStm`）与
        // XRef 流（`/Type /XRef`）。它们的内部对象已由 load_all_objects
        // 解包进 cache，容器本身不再需要——writer 会按目标版本重新生成
        // 新的对象流 / XRef 流。若保留，它们会被当作普通顶层对象再次写出，
        // 使输出里出现与新结构重复的副本（降级时则出现「经典 xref 表 +
        // 冗余 /Type /XRef 字典」这种四不像）。
        objs.retain(|o| {
            !matches!(
                o.value
                    .as_dict()
                    .and_then(|d| d.get("Type"))
                    .and_then(|v| v.as_name()),
                Some("ObjStm") | Some("XRef")
            )
        });
        let mut ctx = WriteContext::new(version, self.compression.clone());
        ctx.write_header();
        // 先预留文档内全部对象号，再分配对象流/XRef 流的新对象号。
        ctx.reserve_object_numbers(&objs);

        let (top, packed) = layout.plan(&objs);
        ctx.write_top_level_objects(doc, &top)?;
        // 非流对象打包：对象流对象自身另分配一个对象号，作为顶层对象写出。
        if !packed.is_empty() {
            let stream_num = ctx.next_object_number();
            ctx.write_object_stream(doc, &packed, stream_num)?;
        }
        xref.write_xref_and_tail(&mut ctx, doc)?;

        w.write_all(&ctx.buf)?;
        Ok(())
    }

    /// 以**增量更新**（7.5.6）序列化 [`Document`]：保留源文件字节，
    /// 仅追加变更/删除对象与新交叉引用段（`/Prev` 链、trailer 合并、
    /// `/ID` 更新）。宿主必须是从文件解析的 `Document`；加密文档与
    /// 改写文件头版本的请求一律拒绝且零字节。详见 [`mod@incremental`]。
    pub fn build_incremental(&self, doc: &Document, w: &mut dyn Write) -> Result<(), WriteError> {
        incremental::build(doc, w, self.version, self.compression.clone())
    }

    /// 设置 [`Self::build_hybrid`] 的隐藏集策略（链式，与
    /// [`Self::with_version`] 风格一致）；不影响 [`Self::build`]。
    pub fn hybrid_reference(mut self, policy: HybridReferencePolicy) -> Self {
        self.hybrid_policy = Some(policy);
        self
    }

    /// 以**混合引用文件**（7.5.8.4，Table 19 `/XRefStm`）序列化 [`Document`]：
    /// 经典主表登记可见对象（隐藏号在表中为 generation 65535 的自由条目），
    /// 隐藏对象打包进对象流，由文件末尾更新段 trailer 的 `/XRefStm` 指向的
    /// 交叉引用流补充真实条目——PDF 1.5 之前的读端把可选引用的目标当作
    /// null 也能打开。目标版本须 ≥1.5，加密文档拒绝；校验失败零字节。
    /// 隐藏键白名单与 v1 边界详见 [`mod@hybrid`]。
    pub fn build_hybrid(&self, doc: &mut Document, w: &mut dyn Write) -> Result<(), WriteError> {
        let policy = self.hybrid_policy.clone().unwrap_or_default();
        hybrid::build(doc, w, self.version, self.compression.clone(), &policy)
    }
}

/// 兼容入口：等价于 `DocumentWriter::default().build(doc, w)`。
pub fn write_document(doc: &Document, w: &mut dyn Write) -> Result<(), WriteError> {
    DocumentWriter::default().build(doc, w)
}
