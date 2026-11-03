//! Marked-content section opening and optional-content visibility
//! resolution for `BMC`/`BDC` property operands (8.11, 14.6.2).

use crate::core::reader::content::text_value;
use crate::core::types::{Dict, Document, Value};

use super::MAX_OC_DEPTH;
use super::context::ExtractionContext;
use super::resolve::resolve_value;
use super::state::Operand;

impl<'a> ExtractionContext<'a> {
    /// Open a marked-content section for `BMC` (no operand) or `BDC`
    /// (inline dictionary, `/Properties` resource name, or — interoperability
    /// tolerance, see [`Operand::Reference`] — an indirect reference).
    ///
    /// `/Artifact` sections are pagination or layout decorations and are
    /// excluded from extracted text (ISO 32000-1 14.8.2.2.1), as are sections
    /// whose optional-content group is off in the default configuration.
    pub(super) fn open_marked(
        &mut self,
        tag: Option<&str>,
        property_operand: Option<&Operand>,
        resources: &Dict,
    ) {
        let properties = match property_operand {
            Some(Operand::Dict(dict)) => Some(dict.clone()),
            Some(Operand::Name(name)) => resolve_property_list(self.document, resources, name),
            Some(Operand::Reference { object, generation }) => {
                resolve_property_ref(self.document, *object, *generation)
            }
            _ => None,
        };
        let hidden = self.operand_visibility(property_operand, resources) == Some(false);
        let actual_text = properties
            .as_ref()
            .and_then(|dict| dict.get("ActualText"))
            .and_then(text_value);
        self.state.begin_marked(
            hidden || tag == Some("Artifact"),
            tag == Some("ReversedChars"),
            actual_text,
        );
    }

    /// Resolve the optional-content visibility of a `BDC`/`DP` property
    /// operand (ISO 32000-1 14.6.2, 8.11.2).
    ///
    /// The operand is an inline property list, a `/Properties` resource name,
    /// or (interoperability tolerance) an indirect reference to a property
    /// list. A named or referenced entry may be the OCG dictionary itself (the
    /// pattern written by this crate's builder) or a property list carrying
    /// `/OC`. `/OC` values may be OCG references, inline OCMDs, or indirect
    /// references to either; OCMDs are evaluated through their `/P` policy or
    /// `/VE` expression against the default configuration.
    fn operand_visibility(&self, operand: Option<&Operand>, resources: &Dict) -> Option<bool> {
        match operand? {
            Operand::Dict(dict) => self.property_list_visibility(dict, resources),
            Operand::Name(name) => {
                let entry = properties_entry(self.document, resources, name)?;
                let resolved = resolve_value(self.document, &entry);
                let is_inline_group = resolved.as_ref().is_some_and(|value| {
                    value
                        .as_dict()
                        .is_some_and(|dict| is_ocg_dictionary(dict) && dict.get("OC").is_none())
                });
                if is_inline_group {
                    return match &entry {
                        Value::Ref(object, _) => self.group_visible(*object),
                        _ => None,
                    };
                }
                self.oc_visibility(&entry, resources, 0)
            }
            // 间接引用属性表：与 `/OC 5 0 R` 共用同一条解引用通路（裸 OCG /
            // OCMD / 带 `/OC` 的属性表三种落点、`MAX_OC_DEPTH` 深度与环检测
            // 上限都不变）。落点是普通属性表且不带 `/OC` 时不给可见性结论，
            // 与声明式读端 `crate::core::reader::content::marked` 的
            // `Value::Ref` 分支同口径；解不到也返回 `None` 按可见降级。
            Operand::Reference { object, generation } => {
                let dict = resolve_property_ref(self.document, *object, *generation)?;
                if is_ocg_property_dictionary(&dict) {
                    return self.group_visible(*object);
                }
                self.dictionary_visibility(&dict, resources, 0)
            }
            _ => None,
        }
    }

    /// Visibility of a property list dictionary carrying `/OC`.
    fn property_list_visibility(&self, dict: &Dict, resources: &Dict) -> Option<bool> {
        self.oc_visibility(dict.get("OC")?, resources, 0)
    }

    /// Follow `/OC` values: names through `/Properties`, references through
    /// the document, and inline dictionaries directly.
    fn oc_visibility(&self, value: &Value, resources: &Dict, depth: usize) -> Option<bool> {
        if depth > MAX_OC_DEPTH {
            return None;
        }
        match value {
            Value::Name(name) => {
                let entry = properties_entry(self.document, resources, name)?;
                self.oc_visibility(&entry, resources, depth + 1)
            }
            Value::Ref(object, generation) => {
                self.object_visibility(*object, *generation, resources, depth + 1)
            }
            Value::Dict(dict) => self.dictionary_visibility(dict, resources, depth + 1),
            _ => None,
        }
    }

    /// Visibility of an indirect `/OC` target, which may be an OCG, an OCMD,
    /// or (tolerantly) a property list that itself carries `/OC`.
    fn object_visibility(
        &self,
        object: i64,
        _generation: i64,
        resources: &Dict,
        depth: usize,
    ) -> Option<bool> {
        // 按对象号取最新修订，忽略引用中的代号（增量更新后引用可能写旧代）。
        let resolved = self
            .document
            .get_object(object)
            .ok()
            .flatten()
            .map(|entry| entry.value);
        match resolved {
            Some(Value::Dict(dict)) if dict.get("OC").is_some() => {
                self.dictionary_visibility(&dict, resources, depth)
            }
            Some(Value::Dict(dict)) if is_membership_dictionary(&dict) => {
                self.membership_visible(&dict)
            }
            _ => self.group_visible(object),
        }
    }

    fn dictionary_visibility(&self, dict: &Dict, resources: &Dict, depth: usize) -> Option<bool> {
        if is_membership_dictionary(dict) {
            self.membership_visible(dict)
        } else if let Some(oc) = dict.get("OC") {
            self.oc_visibility(oc, resources, depth)
        } else {
            // Inline OCG dictionary: its object number is unknown.
            None
        }
    }

    fn membership_visible(&self, dict: &Dict) -> Option<bool> {
        self.oc_properties.as_ref()?.membership_visible(dict)
    }

    fn group_visible(&self, object: i64) -> Option<bool> {
        self.oc_properties
            .as_ref()
            .and_then(|properties| properties.is_visible(object))
    }
}

/// The raw `/Properties` resource entry a `BDC` property *name* operand
/// refers to (ISO 32000-1 14.6.2), before dereferencing.
fn properties_entry(document: &Document, resources: &Dict, name: &str) -> Option<Value> {
    let properties = resources
        .get("Properties")
        .and_then(|value| resolve_value(document, value))?;
    properties.as_dict()?.get(name).cloned()
}

/// Resolve a `BDC` property *name* operand to its property list dictionary
/// through the `/Properties` resource (ISO 32000-1 14.6.2).
fn resolve_property_list(document: &Document, resources: &Dict, name: &str) -> Option<Dict> {
    let entry = properties_entry(document, resources, name)?;
    resolve_value(document, &entry).and_then(|value| value.as_dict().cloned())
}

/// Resolve a `BDC` property *indirect reference* operand by its object number
/// (interoperability tolerance for `/Tag 5 0 R BDC`; see
/// [`Operand::Reference`] for why this form is not ISO 32000-1 authorized).
///
/// 按对象号取最新修订、忽略引用里的代号（与 [`resolve_value`] 同一口径）。
/// 落点接受属性表字典与本 crate 写端产出的裸 OCG 字典；对象缺失、目标不是
/// 字典时返回 `None`，只降级为「无属性、可见」，绝不报错——与
/// `crate::core::reader::content::marked` 里 `section_from` 的 `Value::Ref`
/// 分支（声明式读端）同构。
fn resolve_property_ref(document: &Document, object: i64, generation: i64) -> Option<Dict> {
    let value = resolve_value(document, &Value::Ref(object, generation))?;
    value.as_dict().cloned()
}

/// Whether a dictionary is an optional content membership dictionary
/// (8.11.2.2, Table 100) rather than an OCG or a property list.
fn is_membership_dictionary(dict: &Dict) -> bool {
    dict.get("Type").and_then(Value::as_name) == Some("OCMD")
        || dict.get("OCGs").is_some()
        || dict.get("VE").is_some()
}

fn is_ocg_dictionary(dict: &Dict) -> bool {
    dict.get("Type").and_then(Value::as_name) == Some("OCG")
}

/// Whether a dereferenced operand *is* the OCG dictionary itself (rather than
/// a property list carrying `/OC`) — the pattern both this crate's writer and
/// common producers emit for `/Tag 5 0 R BDC`.
fn is_ocg_property_dictionary(dict: &Dict) -> bool {
    is_ocg_dictionary(dict) && dict.get("OC").is_none()
}
