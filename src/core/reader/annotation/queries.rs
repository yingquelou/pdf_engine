//! 注解读取查询 API：从 [`Document`] 出发的页面 `/Annots` 遍历（12.5.1）与
//! 注解专有条目（`/Dest`、Link `/PA`、Sound `/Sound`）的类型化解析。

use std::collections::HashSet;

use crate::core::reader::action::Action;
use crate::core::reader::navigation::OutlineDestination;
use crate::core::reader::sound::Sound;
use crate::core::types::{Dict, Document, Value};

use super::{
    Annotation, AnnotationError, MAX_ANNOTATIONS_PER_PAGE, Result, malformed,
    parse::parse_annotation,
};

/// 页面树遍历最大深度。
const MAX_PAGE_DEPTH: usize = 64;

impl Document {
    /// 读取指定页面的注解列表（12.5.1 `/Annots`）。
    ///
    /// 页码越界、页面或注解字典不可解析时返回类型化错误。
    pub fn page_annotations(&self, page_index: usize) -> Result<Vec<Annotation>> {
        let page = self.page_dict_at(page_index)?;
        let Some(annots) = page.get("Annots") else {
            return Ok(Vec::new());
        };
        let resolved = crate::core::reader::navigation::resolve(self, annots)
            .map_err(|err| AnnotationError::Malformed(err.to_string()))?;
        let array = resolved
            .as_array()
            .ok_or_else(|| malformed("page /Annots is not an array"))?;
        if array.len() > MAX_ANNOTATIONS_PER_PAGE {
            return Err(AnnotationError::LimitExceeded("annotations per page"));
        }
        let mut out = Vec::with_capacity(array.len());
        for entry in array.iter() {
            let reference = entry.as_ref();
            let resolved = crate::core::reader::navigation::resolve(self, entry)
                .map_err(|err| AnnotationError::Malformed(err.to_string()))?;
            let dict = resolved
                .as_dict()
                .ok_or_else(|| malformed("annotation entry is not a dictionary"))?;
            out.push(parse_annotation(self, page_index, reference, dict)?);
        }
        Ok(out)
    }

    /// 解析注解 `/Dest` 为目标模型（12.5.6.20 与 12.3.2）。
    ///
    /// 支持目的地数组、名字/文本串（命名目标）与 PDF 1.1 的 `/D` 字典形式；
    /// 无 `/Dest` 返回 `None`。返回 `Named` 时可继续用
    /// [`Document::named_destination`] 在目录 `/Dests` 名称树中查找实际目标。
    pub fn annotation_destination(
        &self,
        annotation: &Annotation,
    ) -> Result<Option<OutlineDestination>> {
        let Some(value) = annotation.destination.as_ref() else {
            return Ok(None);
        };
        let resolved = crate::core::reader::navigation::resolve(self, value)
            .map_err(|err| AnnotationError::Malformed(err.to_string()))?;
        let value = match &resolved {
            Value::Dict(dict) => dict
                .get("D")
                .ok_or_else(|| malformed("annotation /Dest dictionary is missing /D"))?,
            other => other,
        };
        let pages = crate::core::reader::navigation::page_tree_index(self)?;
        let target = crate::core::reader::navigation::destination::parse_outline_destination(
            self, value, &pages,
        )?;
        Ok(Some(target))
    }

    /// 解析 Link 注解 `/PA` 按下动作（Table 173，PDF 1.3）。
    ///
    /// `/PA` 是 Web Capture 把 URI 动作改写为跳转动作时保留的原 URI 数据；
    /// 无 `/PA` 返回 `None`，动作字典不可解析时返回类型化错误。
    pub fn annotation_pressed_action(&self, annotation: &Annotation) -> Result<Option<Action>> {
        let Some(value) = annotation.pressed_action.as_ref() else {
            return Ok(None);
        };
        Ok(Some(self.action(value)?))
    }

    /// 解析 Sound 注解的 `/Sound` 声音对象（12.5.6.16 Table 185）。
    ///
    /// 无 `/Sound` 返回 `None`；引用的流对象非法或解码失败时返回
    /// [`AnnotationError::Malformed`]。
    pub fn annotation_sound(&self, annotation: &Annotation) -> Result<Option<Sound>> {
        let Some(value) = annotation.sound.as_ref() else {
            return Ok(None);
        };
        self.sound(value)
            .map(Some)
            .map_err(|error| malformed(format!("Sound annotation: {error}")))
    }

    /// 按 0 基页号遍历页面树，返回页面字典（含直接嵌入页）。
    fn page_dict_at(&self, page_index: usize) -> Result<Dict> {
        let catalog = crate::core::reader::navigation::catalog(self)
            .map_err(|_| AnnotationError::CatalogMissing)?;
        let pages = catalog.get("Pages").ok_or(AnnotationError::PageNotFound)?;
        let root_key = pages.as_ref();
        let root = crate::core::reader::navigation::resolve(self, pages)
            .map_err(|err| AnnotationError::Malformed(err.to_string()))?;
        let root = root.as_dict().ok_or(AnnotationError::PageNotFound)?;

        let mut visited: HashSet<(i64, i64)> = HashSet::new();
        if let Some(key) = root_key {
            visited.insert(key);
        }

        let mut count = 0usize;
        let mut stack = vec![(Value::Dict(root.clone()), 0usize)];
        while let Some((value, depth)) = stack.pop() {
            if depth > MAX_PAGE_DEPTH {
                return Err(AnnotationError::LimitExceeded("page tree depth"));
            }
            let resolved = crate::core::reader::navigation::resolve(self, &value)
                .map_err(|err| AnnotationError::Malformed(err.to_string()))?;
            let node = resolved.as_dict().ok_or(AnnotationError::PageNotFound)?;
            match node.get("Type").and_then(Value::as_name) {
                Some("Page") => {
                    if count == page_index {
                        return Ok(node.clone());
                    }
                    count += 1;
                }
                Some("Pages") => {
                    let mut refs_out: Vec<Value> = Vec::new();
                    if let Some(kids) = node.get("Kids") {
                        let kids = crate::core::reader::navigation::resolve(self, kids)
                            .map_err(|err| AnnotationError::Malformed(err.to_string()))?;
                        let kids = kids
                            .as_array()
                            .ok_or_else(|| malformed("page tree /Kids is not an array"))?;
                        refs_out.extend(kids.iter().rev().cloned());
                    }
                    for kid in refs_out {
                        if let Some(key) = kid.as_ref()
                            && !visited.insert(key)
                        {
                            return Err(AnnotationError::Malformed(
                                "page tree contains a reference cycle".to_string(),
                            ));
                        }
                        stack.push((kid, depth + 1));
                    }
                }
                _ => {
                    return Err(AnnotationError::PageNotFound);
                }
            }
        }
        Err(AnnotationError::PageNotFound)
    }
}
