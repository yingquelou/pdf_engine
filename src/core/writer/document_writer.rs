//! `DocumentWriter` 编排与 `write_document` 兼容入口。

use crate::core::types::{Document, PdfFeature, PdfVersion};
use std::io::Write;

use super::WriteError;
use super::context::WriteContext;
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
        if doc.source.is_some() {
            if let Err(e) = doc.load_all_objects() {
                doc.push_warning(format!(
                    "DocumentWriter: load_all_objects failed: {e}; continuing with partial cache"
                ));
            }
        }
        // 默认版本用 writer 自身的 1.7（PdfVersion::default），**不再**
        // 继承 doc.source_version——写端与读端版本彻底解耦。
        let version = self.version.unwrap_or(PdfVersion::default());
        validate_output_version(version)?;
        validate_content_version(version, doc)?;

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
        let mut ctx = WriteContext::new(version);
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
}

/// 兼容入口：等价于 `DocumentWriter::default().build(doc, w)`。
pub fn write_document(doc: &Document, w: &mut dyn Write) -> Result<(), WriteError> {
    DocumentWriter::default().build(doc, w)
}
