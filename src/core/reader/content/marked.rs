//! Marked-content reading semantics (ISO 32000-1 14.6, 8.11.2, 14.9.1).
//!
//! [`collect_marked_sections`] walks a parsed content stream's [`Op`] sequence,
//! pairs `BMC/BDC … EMC` nesting, resolves property lists (inline dictionary
//! or `/Properties` resource name), and reports typed errors for unbalanced
//! sections. Each section exposes `/ActualText` and `/E` text and — through a
//! visibility callback — whether its optional-content group (8.11) is on.

use std::fmt;

use super::Op;
use crate::core::reader::optional_content::membership_visibility;
use crate::core::reader::page_content::PageContentError;
use crate::core::types::{Dict, Document, PdfFeature, PdfTextString, Value};

/// Maximum marked-content nesting depth (`BMC`/`BDC` without `EMC`).
pub const MAX_MARKED_DEPTH: usize = 64;

/// Errors produced while pairing marked-content sections.
#[derive(Debug, Clone, PartialEq)]
pub enum MarkedContentError {
    /// An `EMC` appeared with an empty section stack.
    UnmatchedEnd {
        /// Index of the offending `EMC` operator.
        index: usize,
    },
    /// The stream ended inside a section that was never closed.
    UnclosedSection {
        /// Tag of the outermost unclosed section.
        tag: String,
        /// Index of its `BMC`/`BDC` operator.
        start: usize,
    },
    /// Nesting exceeded [`MAX_MARKED_DEPTH`], which cannot be well-formed.
    DepthExceeded {
        /// Index of the `BMC`/`BDC` that crossed the limit.
        index: usize,
    },
    /// The page content stream could not be parsed at all.
    ContentStream(String),
}

impl fmt::Display for MarkedContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnmatchedEnd { index } => {
                write!(f, "EMC without a matching BMC/BDC at operator {index}")
            }
            Self::UnclosedSection { tag, start } => write!(
                f,
                "marked section /{tag} opened at operator {start} is never closed"
            ),
            Self::DepthExceeded { index } => write!(
                f,
                "marked content nesting at operator {index} exceeds the depth limit {MAX_MARKED_DEPTH}"
            ),
            Self::ContentStream(message) => write!(f, "page content stream: {message}"),
        }
    }
}

impl std::error::Error for MarkedContentError {}

/// One marked-content section (or point) of a content stream.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkedSection {
    /// Section tag (the `BMC`/`BDC`/`DP`/`MP` name).
    pub tag: String,
    /// Resolved property list dictionary, when present and resolvable.
    pub properties: Option<Dict>,
    /// Effective optional-content visibility: `Some(false)` means hidden by
    /// the default configuration; `None` means no `/OC` entry or unknown.
    pub visibility: Option<bool>,
    /// Index of the opening `BMC`/`BDC` or the point `MP`/`DP` operator.
    pub start: usize,
    /// Index just past the closing `EMC` (or the point operator itself).
    pub end: usize,
    /// `true` for zero-length `MP`/`DP` points.
    pub point: bool,
    /// Index in the returned [`Vec`] of the nearest enclosing section;
    /// `None` for top-level sections and points.
    pub parent: Option<usize>,
}

impl MarkedSection {
    /// `/ActualText`: the text that replaces the section's rendered content.
    pub fn actual_text(&self) -> Option<String> {
        self.text_entry("ActualText")
    }

    /// `/E`: abbreviated replacement text for the section.
    pub fn element_text(&self) -> Option<String> {
        self.text_entry("E")
    }

    /// `/Alt`: alternate (descriptive) text for the section.
    pub fn alternate_text(&self) -> Option<String> {
        self.text_entry("Alt")
    }

    /// `/Lang`: language of the section content.
    pub fn language(&self) -> Option<String> {
        let properties = self.properties.as_ref()?;
        match properties.get("Lang") {
            Some(Value::Name(name)) => Some(name.clone()),
            Some(value) => text_value(value),
            None => None,
        }
    }

    fn text_entry(&self, key: &str) -> Option<String> {
        let properties = self.properties.as_ref()?;
        text_value(properties.get(key)?)
    }
}

/// Decode a text-string property entry.
///
/// Content-stream dictionary strings keep their raw bytes one-per-char
/// (literal) or uppercase hex characters (hexadecimal); both are decoded
/// through [`PdfTextString`] with PDFDocEncoding or UTF-16BE semantics.
pub(crate) fn text_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let bytes: Vec<u8> = text.chars().map(|c| c as u8).collect();
            PdfTextString::decode(&bytes)
                .ok()
                .map(PdfTextString::into_string)
                .or_else(|| Some(text.clone()))
        }
        Value::HexString(hex) => {
            let bytes = hex_decode(hex)?;
            PdfTextString::decode(&bytes)
                .ok()
                .map(PdfTextString::into_string)
        }
        Value::Name(name) => Some(name.clone()),
        _ => None,
    }
}

fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    let digits: Vec<u8> = hex.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if digits.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(digits.len() / 2);
    for pair in digits.chunks_exact(2) {
        let hi = hex_value(pair[0])?;
        let lo = hex_value(pair[1])?;
        bytes.push(hi * 16 + lo);
    }
    Some(bytes)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Resolve the optional-content visibility of a property dictionary.
///
/// `resolve_ocg_name` maps a property-list `/OC` *name* to the OCG object
/// number through the `/Properties` resource; direct `num gen R` values are
/// already object numbers. Inline membership dictionaries (`/OC << /OCGs …
/// /P … >>`) are evaluated with their `/P` policy or `/VE` expression; an
/// indirect OCMD reference cannot be identified here because it is
/// indistinguishable from an OCG object number with this callback shape.
/// Returns `None` when there is no `/OC`, the group cannot be identified, or
/// the default configuration leaves the state unchanged.
pub fn resolve_marked_visibility(
    properties: Option<&Dict>,
    resolve_ocg_name: &dyn Fn(&str) -> Option<i64>,
    is_visible: &dyn Fn(i64) -> Option<bool>,
) -> Option<bool> {
    let oc = properties?.get("OC")?;
    match oc {
        Value::Name(name) => resolve_ocg_name(name).and_then(is_visible),
        Value::Ref(object, _) => is_visible(*object),
        Value::Dict(dict) => membership_visibility(dict, is_visible),
        _ => None,
    }
}

/// Visibility for a `BDC`/`DP` *name* operand whose `/Properties` entry is
/// the OCG dictionary itself rather than a property list carrying `/OC`
/// (the pattern written by this crate's optional-content builder).
fn visibility_from_named_group(
    name: &str,
    properties: Option<&Dict>,
    resolve_ocg_name: &dyn Fn(&str) -> Option<i64>,
    is_visible: &dyn Fn(i64) -> Option<bool>,
) -> Option<bool> {
    let is_ocg_dictionary = properties.is_some_and(|dict| {
        dict.get("Type").and_then(Value::as_name) == Some("OCG") && dict.get("OC").is_none()
    });
    if !is_ocg_dictionary {
        return None;
    }
    resolve_ocg_name(name).and_then(is_visible)
}

/// Pair marked-content sections in `ops` and resolve their property lists.
///
/// `resolve_property_name` maps a `BDC`/`DP` property *name* to its property
/// list dictionary through the `/Properties` resource (already dereferenced).
/// `resolve_ocg_name` and `is_visible` feed [`resolve_marked_visibility`].
pub fn collect_marked_sections(
    ops: &[Op],
    resolve_property_name: &dyn Fn(&str) -> Option<Dict>,
    resolve_ocg_name: &dyn Fn(&str) -> Option<i64>,
    is_visible: &dyn Fn(i64) -> Option<bool>,
) -> Result<Vec<MarkedSection>, MarkedContentError> {
    // 先按「打开顺序」收集节点并记录父节点 id，最后再映射为「关闭顺序」
    // 的输出下标：points 立即输出，sections 在 EMC 时输出（内层先关）。
    let mut nodes: Vec<MarkedSection> = Vec::new();
    let mut parent_ids: Vec<Option<usize>> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    let mut order: Vec<usize> = Vec::new();
    for (index, op) in ops.iter().enumerate() {
        match op {
            Op::BeginMarkedContent(tag) => {
                open_section(
                    &mut open,
                    &mut nodes,
                    &mut parent_ids,
                    tag.clone(),
                    None,
                    index,
                    resolve_property_name,
                    resolve_ocg_name,
                    is_visible,
                )?;
            }
            Op::BeginMarkedContentWithProperties(tag, properties) => {
                open_section(
                    &mut open,
                    &mut nodes,
                    &mut parent_ids,
                    tag.clone(),
                    Some(properties),
                    index,
                    resolve_property_name,
                    resolve_ocg_name,
                    is_visible,
                )?;
            }
            Op::MarkedContentPoint(tag) => {
                let id = nodes.len();
                nodes.push(section_from(
                    tag.clone(),
                    None,
                    index,
                    index + 1,
                    true,
                    resolve_property_name,
                    resolve_ocg_name,
                    is_visible,
                ));
                parent_ids.push(open.last().copied());
                order.push(id);
            }
            Op::MarkedContentPointWithProperties(tag, properties) => {
                let id = nodes.len();
                nodes.push(section_from(
                    tag.clone(),
                    Some(properties),
                    index,
                    index + 1,
                    true,
                    resolve_property_name,
                    resolve_ocg_name,
                    is_visible,
                ));
                parent_ids.push(open.last().copied());
                order.push(id);
            }
            Op::EndMarkedContent => {
                let Some(id) = open.pop() else {
                    return Err(MarkedContentError::UnmatchedEnd { index });
                };
                nodes[id].end = index + 1;
                order.push(id);
            }
            _ => {}
        }
    }
    if let Some(&outermost) = open.first() {
        return Err(MarkedContentError::UnclosedSection {
            tag: nodes[outermost].tag.clone(),
            start: nodes[outermost].start,
        });
    }
    // 关闭顺序 → 输出下标。父节点关闭得比子节点晚，因此只能等收集
    // 完成后才能把父 id 映射为它在返回 Vec 中的位置。
    let mut positions = vec![0usize; nodes.len()];
    for (position, &id) in order.iter().enumerate() {
        positions[id] = position;
    }
    let mut sections = Vec::with_capacity(order.len());
    for &id in &order {
        let mut section = nodes[id].clone();
        section.parent = parent_ids[id].map(|parent| positions[parent]);
        sections.push(section);
    }
    Ok(sections)
}

#[allow(clippy::too_many_arguments)]
fn open_section(
    open: &mut Vec<usize>,
    nodes: &mut Vec<MarkedSection>,
    parent_ids: &mut Vec<Option<usize>>,
    tag: String,
    properties_operand: Option<&Value>,
    index: usize,
    resolve_property_name: &dyn Fn(&str) -> Option<Dict>,
    resolve_ocg_name: &dyn Fn(&str) -> Option<i64>,
    is_visible: &dyn Fn(i64) -> Option<bool>,
) -> Result<(), MarkedContentError> {
    if open.len() >= MAX_MARKED_DEPTH {
        return Err(MarkedContentError::DepthExceeded { index });
    }
    let section = section_from(
        tag,
        properties_operand,
        index,
        index,
        false,
        resolve_property_name,
        resolve_ocg_name,
        is_visible,
    );
    let id = nodes.len();
    nodes.push(section);
    parent_ids.push(open.last().copied());
    open.push(id);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn section_from(
    tag: String,
    properties_operand: Option<&Value>,
    start: usize,
    end: usize,
    point: bool,
    resolve_property_name: &dyn Fn(&str) -> Option<Dict>,
    resolve_ocg_name: &dyn Fn(&str) -> Option<i64>,
    is_visible: &dyn Fn(i64) -> Option<bool>,
) -> MarkedSection {
    let mut visibility = None;
    let properties = properties_operand.and_then(|operand| match operand {
        Value::Dict(dict) => Some(dict.clone()),
        Value::Name(name) => {
            let dict = resolve_property_name(name);
            visibility =
                visibility_from_named_group(name, dict.as_ref(), resolve_ocg_name, is_visible);
            dict
        }
        _ => None,
    });
    if visibility.is_none() {
        visibility = resolve_marked_visibility(properties.as_ref(), resolve_ocg_name, is_visible);
    }
    MarkedSection {
        tag,
        properties,
        visibility,
        start,
        end,
        point,
        parent: None,
    }
}

impl Document {
    /// Collect the marked-content sections of a page's content stream.
    ///
    /// Property lists are resolved through the page `/Properties` resource
    /// and optional-content visibility is evaluated against the catalog
    /// `/OCProperties` default configuration. Form XObject content is not
    /// recursed into; callers expand `Op::PaintXObject` themselves.
    pub fn page_marked_content(
        &self,
        page_index: usize,
    ) -> Result<Vec<MarkedSection>, MarkedContentError> {
        let ops = self
            .page_content_ops(page_index)
            .map_err(|error: PageContentError| {
                MarkedContentError::ContentStream(format!("{error:?}"))
            })?;
        let properties = self.optional_content_properties().ok().flatten();
        let resolve_property_name = |name: &str| -> Option<Dict> {
            let value = self
                .page_resource(page_index, "Properties", name)
                .ok()
                .flatten()?;
            let value = self.resolve_graphics_state_value(&value).ok()?;
            value.as_dict().cloned()
        };
        let resolve_ocg_name = |name: &str| -> Option<i64> {
            let value = self
                .page_resource(page_index, "Properties", name)
                .ok()
                .flatten()?;
            let Value::Ref(object, generation) = value else {
                return None;
            };
            let resolved = self
                .resolve_graphics_state_value(&Value::Ref(object, generation))
                .ok()?;
            let is_group = resolved
                .as_dict()
                .is_some_and(|dict| dict.get("Type").and_then(Value::as_name) == Some("OCG"));
            is_group.then_some(object)
        };
        let is_visible = |object: i64| {
            properties
                .as_ref()
                .and_then(|props| props.is_visible(object))
        };
        let sections =
            collect_marked_sections(&ops, &resolve_property_name, &resolve_ocg_name, &is_visible)?;
        self.record_marked_content_features(&sections);
        Ok(sections)
    }

    /// 把内容流里出现的 Artifact 属性表登记进特性画像（14.8.2.2.2）。
    ///
    /// 特性画像的常规扫描只看对象字典，内容流中的 `BDC` 属性表不参与；
    /// 标记内容被读取时顺带补登记，保证 `ArtifactPropertyLists` 与
    /// PDF 1.7 的 `ArtifactBackground` / `ArtifactSubtype` 在解析后的
    /// 文档特性画像中可见。标签为 `ReversedChars` 的区段同样补登记
    /// `ReversedCharacters`（14.8.2.3.3）。
    fn record_marked_content_features(&self, sections: &[MarkedSection]) {
        let mut features = self.features.borrow_mut();
        for section in sections {
            if section.tag == "ReversedChars" {
                features.insert(PdfFeature::ReversedCharacters);
                continue;
            }
            if section.tag != "Artifact" {
                continue;
            }
            let Some(properties) = section.properties.as_ref() else {
                continue;
            };
            features.insert(PdfFeature::ArtifactPropertyLists);
            if properties.get("Type").and_then(Value::as_name) == Some("Background") {
                features.insert(PdfFeature::ArtifactBackground);
            }
            if properties.get("Subtype").is_some() {
                features.insert(PdfFeature::ArtifactSubtype);
            }
        }
    }
}
