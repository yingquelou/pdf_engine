//! Web Capture 写端（14.10）：页面集内容集与 URLS/IDS 名称树（Table 350–355）。
//!
//! 数字标识符（MD5）与 URL 规范化由调用方负责；本层按给定 ID/URL 写出内容集
//! 对象并把它们挂进目录 `/Names /URLS`、`/Names /IDS`，同时在目录写
//! `/SpiderInfo`（`/V`）。全部校验在分配对象号之前完成，失败零字节。

use std::collections::BTreeMap;

use crate::core::builder::types::text_string_value;
use crate::core::builder::types::{
    WebCaptureCommand, WebCaptureImageSet, WebCapturePageSet, WebCaptureSource,
};
use crate::core::types::{Array, Dict, Document, Object, PdfVersion, Value};
use crate::core::writer::WriteError;

use super::{PdfBuilder, invalid, name_tree};

/// Web Capture 写端状态。
#[derive(Debug)]
pub(crate) struct WebCaptureState {
    pub(super) version: f64,
    pub(super) page_sets: Vec<WebCapturePageSet>,
    pub(super) image_sets: Vec<WebCaptureImageSet>,
    pub(super) commands: Vec<WebCaptureCommand>,
}

impl Default for WebCaptureState {
    fn default() -> Self {
        Self {
            version: 1.0,
            page_sets: Vec::new(),
            image_sets: Vec::new(),
            commands: Vec::new(),
        }
    }
}

impl PdfBuilder {
    /// 追加一个 Web Capture 页面集（14.10.4.2）。页面以 0 基索引给出。
    pub fn add_web_capture_page_set(&mut self, set: WebCapturePageSet) -> &mut Self {
        self.navigation.web_capture.page_sets.push(set);
        self
    }

    /// 设置 `/SpiderInfo /V` 版本号（缺省 1.0）。
    pub fn with_web_capture_version(&mut self, version: f64) -> &mut Self {
        self.navigation.web_capture.version = version;
        self
    }

    /// 追加一个 Web Capture 图像集（14.10.4.3）。图像以页面索引与该页图像序号定位。
    pub fn add_web_capture_image_set(&mut self, set: WebCaptureImageSet) -> &mut Self {
        self.navigation.web_capture.image_sets.push(set);
        self
    }

    /// 追加一个 Web Capture 命令字典（14.10.5.3）。
    pub fn add_web_capture_command(&mut self, command: WebCaptureCommand) -> &mut Self {
        self.navigation.web_capture.commands.push(command);
        self
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0F) as usize] as char);
    }
    out
}

fn validate_source(source: &WebCaptureSource) -> Result<(), WriteError> {
    if source.chains.is_empty() {
        return Err(invalid(
            "web capture source must have at least one URL chain",
        ));
    }
    for chain in &source.chains {
        if chain.is_empty() {
            return Err(invalid("web capture URL alias chain must not be empty"));
        }
        for url in chain {
            if url.is_empty() || !url.is_ascii() {
                return Err(invalid("web capture source URL must be non-empty ASCII"));
            }
        }
    }
    if let Some(code) = source.submission
        && !(0..=2).contains(&code)
    {
        return Err(invalid(format!(
            "web capture source /S must be 0, 1 or 2, got {code}"
        )));
    }
    Ok(())
}

fn source_info_value(source: &WebCaptureSource) -> Value {
    let au = if source.chains.len() == 1 && source.chains[0].len() == 1 {
        text_string_value(&source.chains[0][0])
    } else {
        let mut chains = Array::new();
        for chain in &source.chains {
            let mut urls = Array::new();
            for url in chain {
                urls = urls.push(text_string_value(url));
            }
            chains = chains.push(Value::Array(urls));
        }
        Value::Dict(Dict::new().insert("C", Value::Array(chains)))
    };
    let mut dict = Dict::new().insert("AU", au);
    if let Some(code) = source.submission {
        dict = dict.insert("S", Value::Integer(code));
    }
    if let Some(timestamp) = &source.timestamp {
        dict = dict.insert("TS", text_string_value(timestamp));
    }
    if let Some(expiration) = &source.expiration {
        dict = dict.insert("E", text_string_value(expiration));
    }
    Value::Dict(dict)
}

fn source_list_value(sources: &[WebCaptureSource]) -> Value {
    if sources.len() == 1 {
        return source_info_value(&sources[0]);
    }
    let mut array = Array::new();
    for source in sources {
        array = array.push(source_info_value(source));
    }
    Value::Array(array)
}

fn page_set_dict(set: &WebCapturePageSet, page_refs: &[(i64, i64)]) -> Result<Dict, WriteError> {
    let mut objects = Array::new();
    for &page in &set.pages {
        let (num, generation) = page_refs[page];
        objects = objects.push(Value::Ref(num, generation));
    }
    let mut dict = Dict::new()
        .insert("Type", Value::Name("SpiderContentSet".into()))
        .insert("S", Value::Name("SPS".into()))
        .insert("ID", Value::HexString(hex(&set.id)))
        .insert("O", Value::Array(objects))
        .insert("SI", source_list_value(&set.sources));
    if let Some(content_type) = &set.content_type {
        dict = dict.insert("CT", text_string_value(content_type));
    }
    if let Some(timestamp) = &set.timestamp {
        dict = dict.insert("TS", text_string_value(timestamp));
    }
    if let Some(title) = &set.title {
        dict = dict.insert("T", text_string_value(title));
    }
    if let Some(text_id) = &set.text_id {
        dict = dict.insert("TID", Value::HexString(hex(text_id)));
    }
    Ok(dict)
}

fn image_set_dict(set: &WebCaptureImageSet, image_nums: &[Vec<i64>]) -> Result<Dict, WriteError> {
    let mut objects = Array::new();
    for &(page, slot) in &set.images {
        let num = image_nums[page][slot];
        objects = objects.push(Value::Ref(num, 0));
    }
    let reference_counts = if set.reference_counts.len() == 1 {
        Value::Integer(set.reference_counts[0])
    } else {
        let mut array = Array::new();
        for count in &set.reference_counts {
            array = array.push(Value::Integer(*count));
        }
        Value::Array(array)
    };
    let mut dict = Dict::new()
        .insert("Type", Value::Name("SpiderContentSet".into()))
        .insert("S", Value::Name("SIS".into()))
        .insert("ID", Value::HexString(hex(&set.id)))
        .insert("O", Value::Array(objects))
        .insert("SI", source_list_value(&set.sources))
        .insert("R", reference_counts);
    if let Some(content_type) = &set.content_type {
        dict = dict.insert("CT", text_string_value(content_type));
    }
    if let Some(timestamp) = &set.timestamp {
        dict = dict.insert("TS", text_string_value(timestamp));
    }
    Ok(dict)
}

fn record_urls(
    urls: &mut BTreeMap<String, Vec<Value>>,
    sources: &[WebCaptureSource],
    reference: &Value,
) {
    for source in sources {
        for chain in &source.chains {
            let canonical = chain.last().cloned().unwrap_or_default();
            urls.entry(canonical).or_default().push(reference.clone());
        }
    }
}

/// 写出 Web Capture 内容集与名称树，返回更新后的目录字典。
pub(super) fn write(
    doc: &mut Document,
    catalog: &Dict,
    state: &WebCaptureState,
    version: PdfVersion,
    page_refs: &[(i64, i64)],
    image_nums: &[Vec<i64>],
) -> Result<Dict, WriteError> {
    if state.page_sets.is_empty() && state.image_sets.is_empty() && state.commands.is_empty() {
        return Ok(catalog.clone());
    }
    if version < PdfVersion::V1_3 {
        return Err(WriteError::UnsupportedVersion(format!(
            "Web Capture requires PDF {}, got {version}",
            PdfVersion::V1_3.as_str()
        )));
    }
    if !state.version.is_finite() || state.version <= 0.0 {
        return Err(invalid(
            "web capture /SpiderInfo /V must be a positive number",
        ));
    }

    let mut page_owner: BTreeMap<usize, usize> = BTreeMap::new();
    for (index, set) in state.page_sets.iter().enumerate() {
        if set.id.is_empty() {
            return Err(invalid("web capture page set /ID must not be empty"));
        }
        if set.pages.is_empty() {
            return Err(invalid(
                "web capture page set must reference at least one page",
            ));
        }
        for &page in &set.pages {
            if page >= page_refs.len() {
                return Err(invalid(format!(
                    "web capture page set references page {page} out of range (page count {})",
                    page_refs.len()
                )));
            }
            if page_owner.insert(page, index).is_some() {
                return Err(invalid(format!(
                    "page {page} belongs to more than one web capture page set"
                )));
            }
        }
        if set.sources.is_empty() {
            return Err(invalid("web capture page set requires at least one source"));
        }
        for source in &set.sources {
            validate_source(source)?;
        }
        if let Some(content_type) = &set.content_type
            && (content_type.is_empty() || !content_type.is_ascii())
        {
            return Err(invalid("web capture page set /CT must be non-empty ASCII"));
        }
    }

    let mut image_owner: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    for (index, set) in state.image_sets.iter().enumerate() {
        if set.id.is_empty() {
            return Err(invalid("web capture image set /ID must not be empty"));
        }
        if set.images.is_empty() {
            return Err(invalid(
                "web capture image set must reference at least one image",
            ));
        }
        if set.reference_counts.len() != set.images.len() {
            return Err(invalid(format!(
                "web capture image set /R has {} entries for {} images",
                set.reference_counts.len(),
                set.images.len()
            )));
        }
        for &(page, slot) in &set.images {
            if page >= image_nums.len() || slot >= image_nums[page].len() {
                return Err(invalid(format!(
                    "web capture image set references image ({page}, {slot}) out of range"
                )));
            }
            if image_owner.insert((page, slot), index).is_some() {
                return Err(invalid(format!(
                    "image ({page}, {slot}) belongs to more than one web capture image set"
                )));
            }
        }
        if set.sources.is_empty() {
            return Err(invalid(
                "web capture image set requires at least one source",
            ));
        }
        for source in &set.sources {
            validate_source(source)?;
        }
        if let Some(content_type) = &set.content_type
            && (content_type.is_empty() || !content_type.is_ascii())
        {
            return Err(invalid("web capture image set /CT must be non-empty ASCII"));
        }
    }

    for command in &state.commands {
        command.validate()?;
    }

    let mut next = doc
        .objects_for_write()
        .iter()
        .map(|object| object.num)
        .max()
        .unwrap_or(0)
        + 1;
    let mut command_references = Vec::with_capacity(state.commands.len());
    for command in &state.commands {
        doc.set_object(Object::new(next, 0, Value::Dict(command.to_dict())));
        command_references.push(Value::Ref(next, 0));
        next += 1;
    }
    let mut references = Vec::with_capacity(state.page_sets.len());
    for set in &state.page_sets {
        let dict = page_set_dict(set, page_refs)?;
        doc.set_object(Object::new(next, 0, Value::Dict(dict)));
        references.push(Value::Ref(next, 0));
        next += 1;
    }
    let mut image_references = Vec::with_capacity(state.image_sets.len());
    for set in &state.image_sets {
        let dict = image_set_dict(set, image_nums)?;
        doc.set_object(Object::new(next, 0, Value::Dict(dict)));
        image_references.push(Value::Ref(next, 0));
        next += 1;
    }

    // URLS：规范 URL → 内容集（同 URL 多集时为数组）。
    let mut urls: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for (set, reference) in state.page_sets.iter().zip(&references) {
        record_urls(&mut urls, &set.sources, reference);
    }
    for (set, reference) in state.image_sets.iter().zip(&image_references) {
        record_urls(&mut urls, &set.sources, reference);
    }
    let url_entries: Vec<(String, Value)> = urls
        .into_iter()
        .map(|(url, mut refs)| {
            let value = if refs.len() == 1 {
                refs.remove(0)
            } else {
                Value::Array(Array(refs))
            };
            (url, value)
        })
        .collect();
    let urls_tree = name_tree(url_entries);

    // IDS：数字标识符按原始字节序（十六进制键）→ 内容集。
    let mut ids: Vec<(Vec<u8>, Value)> = Vec::new();
    ids.extend(
        state
            .page_sets
            .iter()
            .zip(&references)
            .map(|(set, reference)| (set.id.clone(), reference.clone())),
    );
    ids.extend(
        state
            .image_sets
            .iter()
            .zip(&image_references)
            .map(|(set, reference)| (set.id.clone(), reference.clone())),
    );
    ids.sort_by(|a, b| a.0.cmp(&b.0));
    let mut id_names = Array::new();
    for (id, reference) in ids {
        id_names = id_names.push(Value::HexString(hex(&id))).push(reference);
    }
    let ids_tree = Value::Dict(Dict::new().insert("Names", Value::Array(id_names)));

    let mut names = catalog
        .get("Names")
        .and_then(Value::as_dict)
        .cloned()
        .unwrap_or_default();
    names = names.insert("URLS", urls_tree).insert("IDS", ids_tree);
    let mut updated = catalog.clone();
    updated = updated.insert("Names", Value::Dict(names));
    let mut spider_info = Dict::new().insert("V", Value::Float(state.version));
    if !command_references.is_empty() {
        spider_info = spider_info.insert("C", Value::Array(Array(command_references)));
    }
    updated = updated.insert("SpiderInfo", Value::Dict(spider_info));
    Ok(updated)
}
