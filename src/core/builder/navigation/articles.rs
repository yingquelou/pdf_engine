//! 文章线写端（ISO 32000-1 12.4.3、Table 160/161）。
//!
//! 线程与 bead 都是间接对象，在 [`PdfBuilder::build`] 的目录后处理阶段
//! 从现有最大对象号之后分配；bead 的 `/N` `/V` 首尾闭合成双向环，
//! 页面字典追加 `/B` 数组，目录写入 `/Threads`。PDF 1.1 起可用。

use crate::core::reader::navigation::MAX_ARTICLE_BEADS;
use crate::core::types::{Array, Dict, Document, Object, PdfFeature, PdfVersion, Value};
use crate::core::writer::WriteError;

use super::super::metadata::DocumentInfo;
use super::PdfBuilder;
use super::invalid;

/// 写端文章线上的一个 bead（12.4.3 Table 161）。
#[derive(Debug, Clone, PartialEq)]
pub struct ArticleBead {
    /// bead 所在页面（0 基页索引）。
    pub page: usize,
    /// `/R` 矩形 `[x0, y0, x1, y1]`，须为有限数。
    pub rect: [f64; 4],
}

/// 写端文章线（12.4.3 Table 160）。
#[derive(Debug, Clone, PartialEq)]
pub struct ArticleThread {
    /// bead 序列，按文章阅读顺序给出（至少一个）。
    pub beads: Vec<ArticleBead>,
    /// 线程信息字典 `/I`（14.3.3 语法）；`None` 时不写。
    pub info: Option<DocumentInfo>,
}

impl ArticleThread {
    /// 新建文章线；`/I` 留空。
    pub fn new(beads: Vec<ArticleBead>) -> Self {
        Self { beads, info: None }
    }

    /// 附带线程信息字典。
    pub fn with_info(mut self, info: DocumentInfo) -> Self {
        self.info = Some(info);
        self
    }
}

/// 校验全部文章线；错误串直接用于 [`WriteError`]。
pub(super) fn validate(threads: &[ArticleThread], page_count: usize) -> Result<(), WriteError> {
    let mut total = 0usize;
    for (index, thread) in threads.iter().enumerate() {
        if thread.beads.is_empty() {
            return Err(invalid(format!(
                "article thread {index} must contain at least one bead"
            )));
        }
        total += thread.beads.len();
        for bead in &thread.beads {
            if bead.page >= page_count {
                return Err(invalid(format!(
                    "article thread {index} bead page {} is out of range (page count {page_count})",
                    bead.page
                )));
            }
            if bead.rect.iter().any(|value| !value.is_finite()) {
                return Err(invalid(format!(
                    "article thread {index} bead /R must contain four finite numbers"
                )));
            }
        }
    }
    if total > MAX_ARTICLE_BEADS {
        return Err(invalid(format!(
            "article threads contain {total} beads (limit {MAX_ARTICLE_BEADS})"
        )));
    }
    Ok(())
}

/// 写出线程与 bead 对象、页面 `/B`，返回目录 `/Threads` 数组。
pub(super) fn write_threads(
    doc: &mut Document,
    version: PdfVersion,
    page_refs: &[(i64, i64)],
    threads: &[ArticleThread],
) -> Result<Array, WriteError> {
    let mut next_num = doc
        .objects_for_write()
        .iter()
        .map(|object| object.num)
        .max()
        .unwrap_or(0)
        + 1;
    let mut page_beads: Vec<Vec<Value>> = vec![Vec::new(); page_refs.len()];
    let mut thread_refs = Array::new();

    for thread in threads {
        let thread_num = next_num;
        next_num += 1;
        let first = next_num;
        next_num += thread.beads.len() as i64;
        let last = first + thread.beads.len() as i64 - 1;

        for (index, bead) in thread.beads.iter().enumerate() {
            let num = first + index as i64;
            let next = if num == last { first } else { num + 1 };
            let previous = if num == first { last } else { num - 1 };
            let (page_num, page_generation) = page_refs[bead.page];
            let mut dict = Dict::new()
                .insert("Type", Value::Name("Bead".into()))
                .insert("N", Value::Ref(next, 0))
                .insert("V", Value::Ref(previous, 0))
                .insert("P", Value::Ref(page_num, page_generation))
                .insert(
                    "R",
                    Value::Array(Array(
                        bead.rect.iter().map(|value| Value::Float(*value)).collect(),
                    )),
                );
            if num == first {
                dict = dict.insert("T", Value::Ref(thread_num, 0));
            }
            doc.set_object(Object::new(num, 0, Value::Dict(dict)));
            page_beads[bead.page].push(Value::Ref(num, 0));
        }

        let mut dict = Dict::new()
            .insert("Type", Value::Name("Thread".into()))
            .insert("F", Value::Ref(first, 0));
        if let Some(info) = &thread.info
            && !info.is_empty()
        {
            dict = dict.insert("I", Value::Dict(info.to_dict(version)?));
        }
        doc.set_object(Object::new(thread_num, 0, Value::Dict(dict)));
        thread_refs = thread_refs.push(Value::Ref(thread_num, 0));
    }

    for (page, refs) in page_beads.iter().enumerate() {
        if refs.is_empty() {
            continue;
        }
        let (num, generation) = page_refs[page];
        let objects = doc.objects_for_write();
        let object = objects
            .iter()
            .find(|object| object.num == num && object.generation == generation)
            .ok_or_else(|| invalid(format!("page object {num} {generation} is missing")))?;
        let mut dict = object.value.as_dict().cloned().ok_or_else(|| {
            invalid(format!(
                "page object {num} {generation} is not a dictionary"
            ))
        })?;
        let mut beads = dict
            .get("B")
            .and_then(Value::as_array)
            .map(|array| array.0.clone())
            .unwrap_or_default();
        beads.extend(refs.iter().cloned());
        dict = dict.insert("B", Value::Array(Array(beads)));
        doc.set_object(Object::new(num, generation, Value::Dict(dict)));
    }

    doc.features.borrow_mut().insert(PdfFeature::ArticleThreads);
    Ok(thread_refs)
}

impl PdfBuilder {
    /// 追加一条文章线（12.4.3），按注册顺序写入目录 `/Threads`。
    ///
    /// 线程与 bead 作为间接对象写出，bead 链首尾闭合，所在页面追加
    /// `/B` 数组。bead 至少一个、页索引须在范围内、矩形须为有限数，
    /// 否则构建返回 [`WriteError::InvalidObject`]；目标版本低于 1.1
    /// 返回 [`WriteError::UnsupportedVersion`]。
    pub fn add_article_thread(&mut self, thread: ArticleThread) -> &mut Self {
        self.navigation.article_threads.push(thread);
        self
    }
}
