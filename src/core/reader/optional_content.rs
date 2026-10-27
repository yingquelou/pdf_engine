//! Optional content properties (PDF 8.11, Tables 99–101).
//!
//! [`Document::optional_content_properties`] parses the catalog's
//! `/OCProperties` entry into typed optional content groups (OCGs) and the
//! default configuration dictionary. Text extraction (`14.6` marked content,
//! see [`crate::core::reader::content::MarkedSection`]) suppresses sections
//! whose group the default configuration turns off; XObjects are not yet
//! filtered by group.

use std::fmt;

use crate::core::types::{Dict, Document, PdfTextString, Value};

use super::ParseError;

type Result<T> = std::result::Result<T, OptionalContentError>;

/// Errors produced while parsing `/OCProperties`.
#[derive(Debug)]
pub enum OptionalContentError {
    /// An indirect object referenced by the structure could not be parsed.
    Parse(ParseError),
    /// The structure does not satisfy Tables 99–101.
    Invalid(String),
}

impl fmt::Display for OptionalContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "{error}"),
            Self::Invalid(message) => write!(f, "invalid optional content: {message}"),
        }
    }
}

impl std::error::Error for OptionalContentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::Invalid(_) => None,
        }
    }
}

impl From<ParseError> for OptionalContentError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

/// Default visibility of groups absent from `/ON` and `/OFF` (Table 101).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseState {
    /// `/ON`: groups default to visible.
    On,
    /// `/OFF`: groups default to hidden.
    Off,
    /// `/Unchanged`: leave the current state untouched.
    Unchanged,
}

/// Parsed optional content group dictionary (Table 99).
#[derive(Debug, Clone, PartialEq)]
pub struct OptionalContentGroup {
    /// Object number of the `/OCG` dictionary.
    pub object: i64,
    /// `/Name` decoded as a PDF text string.
    pub name: String,
    /// `/Intent` purpose names; empty means the default `/View`.
    pub intent: Vec<String>,
}

/// Parsed default optional content configuration dictionary (Table 101).
#[derive(Debug, Clone, PartialEq)]
pub struct OptionalContentConfiguration {
    /// `/BaseState` (defaults to `/ON` when absent).
    pub base_state: BaseState,
    /// OCG object numbers listed in `/ON`.
    pub on: Vec<i64>,
    /// OCG object numbers listed in `/OFF`.
    pub off: Vec<i64>,
    /// OCG object numbers collected from `/Order` (nested arrays flattened).
    pub order: Vec<i64>,
}

impl OptionalContentConfiguration {
    /// Effective visibility of an OCG: `/ON` wins over `/OFF`, otherwise
    /// `/BaseState` decides. Returns `None` when the base state is
    /// `/Unchanged`.
    pub fn is_visible(&self, object: i64) -> Option<bool> {
        if self.on.contains(&object) {
            return Some(true);
        }
        if self.off.contains(&object) {
            return Some(false);
        }
        match self.base_state {
            BaseState::On => Some(true),
            BaseState::Off => Some(false),
            BaseState::Unchanged => None,
        }
    }
}

/// Parsed `/OCProperties` dictionary (Table 100).
#[derive(Debug, Clone, PartialEq)]
pub struct OptionalContentProperties {
    /// Every group listed in `/OCGs`, in array order.
    pub groups: Vec<OptionalContentGroup>,
    /// Default configuration `/D`; `None` when omitted.
    pub default: Option<OptionalContentConfiguration>,
}

impl OptionalContentProperties {
    /// Find a group by its OCG object number.
    pub fn group(&self, object: i64) -> Option<&OptionalContentGroup> {
        self.groups.iter().find(|group| group.object == object)
    }

    /// Effective visibility under the default configuration. Returns `None`
    /// when there is no default configuration or it is `/Unchanged`.
    pub fn is_visible(&self, object: i64) -> Option<bool> {
        self.default
            .as_ref()
            .and_then(|configuration| configuration.is_visible(object))
    }

    /// Visibility of an optional content membership dictionary (OCMD,
    /// 8.11.2.2) under the default configuration.
    pub fn membership_visible(&self, dict: &Dict) -> Option<bool> {
        membership_visibility(dict, &|object| self.is_visible(object))
    }
}

/// Evaluate an optional content membership dictionary (Table 100).
///
/// `/VE` takes precedence over `/P`; without either, `/P` defaults to
/// `/AllOn`. Returns `None` when the dictionary carries no `/OCGs` array, the
/// policy is unknown, or the state of a referenced group cannot be
/// determined (for example an `/Unchanged` base state).
pub fn membership_visibility(
    dict: &Dict,
    is_visible: &dyn Fn(i64) -> Option<bool>,
) -> Option<bool> {
    if let Some(expression) = dict.get("VE") {
        return visibility_expression(expression, is_visible);
    }
    let groups = dict.get("OCGs")?.as_array()?;
    let policy = dict.get("P").and_then(Value::as_name).unwrap_or("AllOn");
    let mut any_visible = false;
    let mut any_hidden = false;
    for group in groups.iter() {
        match group.as_ref().and_then(|(object, _)| is_visible(object)) {
            Some(true) => any_visible = true,
            Some(false) => any_hidden = true,
            None => return None,
        }
    }
    match policy {
        "AllOn" => Some(!any_hidden),
        "AnyOn" => Some(any_visible),
        "AnyOff" => Some(any_hidden),
        "AllOff" => Some(!any_visible),
        _ => None,
    }
}

/// Evaluate a `/VE` visibility expression: `[/And …]`, `[/Or …]`,
/// `[/Not expr]`, or an OCG reference.
fn visibility_expression(value: &Value, is_visible: &dyn Fn(i64) -> Option<bool>) -> Option<bool> {
    match value {
        Value::Ref(object, _) => is_visible(*object),
        Value::Array(items) => {
            let mut operands = items.iter();
            let operator = operands.next()?.as_name()?;
            match operator {
                "And" | "Or" => {
                    let dominates = operator == "Or";
                    let mut state = Some(!dominates);
                    for item in operands {
                        state = combine(state, visibility_expression(item, is_visible), dominates);
                    }
                    state
                }
                "Not" => Some(!visibility_expression(operands.next()?, is_visible)?),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Fold one visibility operand into a short-circuiting `And`/`Or` result.
///
/// `dominates` is the operator's absorbing value: `false` for `And`, `true`
/// for `Or`. An unknown operand only survives when no absorbing value was
/// seen.
fn combine(state: Option<bool>, operand: Option<bool>, dominates: bool) -> Option<bool> {
    match (state, operand) {
        (Some(value), _) if value == dominates => Some(dominates),
        (_, Some(value)) if value == dominates => Some(dominates),
        (Some(true), Some(true)) | (Some(false), Some(false)) => state,
        _ => None,
    }
}

impl Document {
    fn resolve_optional_content_value(&self, value: &Value) -> Result<Value> {
        match value {
            Value::Ref(num, generation) => {
                let object = self
                    .get_object(*num)?
                    .filter(|object| object.generation == *generation)
                    .ok_or_else(|| {
                        invalid(format!("unresolved object reference {num} {generation} R"))
                    })?;
                Ok(object.value)
            }
            _ => Ok(value.clone()),
        }
    }

    /// Parse the catalog's `/OCProperties` entry (Table 100).
    ///
    /// Returns `Ok(None)` when the catalog has no `/OCProperties`.
    pub fn optional_content_properties(&self) -> Result<Option<OptionalContentProperties>> {
        let Some((root, generation)) = self.trailer.root() else {
            return Err(invalid("trailer is missing /Root"));
        };
        let catalog = self
            .get_object(root)?
            .filter(|object| object.generation == generation)
            .ok_or_else(|| {
                invalid(format!(
                    "unresolved catalog object reference {root} {generation} R"
                ))
            })?;
        let catalog = catalog
            .value
            .as_dict()
            .ok_or_else(|| invalid("catalog is not a dictionary"))?;
        let Some(properties) = catalog.get("OCProperties") else {
            return Ok(None);
        };
        let properties = self.resolve_optional_content_value(properties)?;
        let properties = properties
            .as_dict()
            .ok_or_else(|| invalid("/OCProperties must be a dictionary"))?;
        parse_properties(self, properties).map(Some)
    }
}

fn parse_properties(document: &Document, dict: &Dict) -> Result<OptionalContentProperties> {
    let groups_value = dict
        .get("OCGs")
        .ok_or_else(|| invalid("/OCProperties /OCGs is required"))?;
    let groups_value = document.resolve_optional_content_value(groups_value)?;
    let groups_array = groups_value
        .as_array()
        .ok_or_else(|| invalid("/OCProperties /OCGs must be an array"))?;
    let mut groups = Vec::with_capacity(groups_array.len());
    for item in groups_array.iter() {
        let Value::Ref(object, generation) = item else {
            return Err(invalid(
                "/OCProperties /OCGs entries must be indirect references",
            ));
        };
        let ocg = document
            .get_object(*object)?
            .filter(|ocg| ocg.generation == *generation)
            .ok_or_else(|| {
                invalid(format!(
                    "unresolved OCG object reference {object} {generation} R"
                ))
            })?;
        groups.push(parse_group(document, *object, &ocg.value)?);
    }

    let default = match dict.get("D") {
        None => None,
        Some(value) => {
            let value = document.resolve_optional_content_value(value)?;
            let value = value
                .as_dict()
                .ok_or_else(|| invalid("/OCProperties /D must be a dictionary"))?;
            Some(parse_configuration(document, value)?)
        }
    };

    Ok(OptionalContentProperties { groups, default })
}

fn parse_group(document: &Document, object: i64, value: &Value) -> Result<OptionalContentGroup> {
    let dict = value
        .as_dict()
        .ok_or_else(|| invalid(format!("OCG {object} is not a dictionary")))?;
    if let Some(type_value) = dict.get("Type") {
        let type_value = document.resolve_optional_content_value(type_value)?;
        if type_value.as_name() != Some("OCG") {
            return Err(invalid(format!("OCG {object} /Type must be /OCG")));
        }
    }
    let name = match dict.get("Name") {
        None => return Err(invalid(format!("OCG {object} is missing /Name"))),
        Some(value) => {
            let value = document.resolve_optional_content_value(value)?;
            text_string(&value)
                .ok_or_else(|| invalid(format!("OCG {object} /Name must be a text string")))?
        }
    };
    let intent = match dict.get("Intent") {
        None => Vec::new(),
        Some(value) => {
            let value = document.resolve_optional_content_value(value)?;
            match value {
                Value::Name(name) => vec![name],
                Value::Array(items) => {
                    let mut names = Vec::with_capacity(items.len());
                    for item in items.iter() {
                        let item = document.resolve_optional_content_value(item)?;
                        let Some(name) = item.as_name() else {
                            return Err(invalid(format!(
                                "OCG {object} /Intent entries must be names"
                            )));
                        };
                        names.push(name.to_string());
                    }
                    names
                }
                _ => {
                    return Err(invalid(format!(
                        "OCG {object} /Intent must be a name or an array of names"
                    )));
                }
            }
        }
    };
    Ok(OptionalContentGroup {
        object,
        name,
        intent,
    })
}

fn parse_configuration(document: &Document, dict: &Dict) -> Result<OptionalContentConfiguration> {
    let base_state = match dict.get("BaseState") {
        None => BaseState::On,
        Some(value) => {
            let value = document.resolve_optional_content_value(value)?;
            match value.as_name() {
                Some("ON") => BaseState::On,
                Some("OFF") => BaseState::Off,
                Some("Unchanged") => BaseState::Unchanged,
                _ => {
                    return Err(invalid(
                        "/OCProperties /D /BaseState must be /ON, /OFF or /Unchanged",
                    ));
                }
            }
        }
    };
    let on = object_references(document, dict, "ON")?;
    let off = object_references(document, dict, "OFF")?;
    let order = match dict.get("Order") {
        None => Vec::new(),
        Some(value) => {
            let value = document.resolve_optional_content_value(value)?;
            let mut order = Vec::new();
            collect_order(&value, &mut order)?;
            order
        }
    };
    Ok(OptionalContentConfiguration {
        base_state,
        on,
        off,
        order,
    })
}

fn object_references(document: &Document, dict: &Dict, key: &str) -> Result<Vec<i64>> {
    let Some(value) = dict.get(key) else {
        return Ok(Vec::new());
    };
    let value = document.resolve_optional_content_value(value)?;
    let array = value
        .as_array()
        .ok_or_else(|| invalid(format!("/OCProperties /D /{key} must be an array")))?;
    let mut objects = Vec::with_capacity(array.len());
    for item in array.iter() {
        let Some((object, _)) = item.as_ref() else {
            return Err(invalid(format!(
                "/OCProperties /D /{key} entries must be indirect references"
            )));
        };
        objects.push(object);
    }
    Ok(objects)
}

fn collect_order(value: &Value, order: &mut Vec<i64>) -> Result<()> {
    match value {
        Value::Ref(object, _) => order.push(*object),
        Value::Array(items) => {
            for item in items.iter() {
                collect_order(item, order)?;
            }
        }
        _ => {
            return Err(invalid(
                "/OCProperties /D /Order entries must be OCG references or arrays",
            ));
        }
    }
    Ok(())
}

fn text_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::HexString(hex) => {
            let bytes = hex_bytes(hex)?;
            PdfTextString::decode(&bytes)
                .ok()
                .map(PdfTextString::into_string)
        }
        _ => None,
    }
}

fn invalid(message: impl Into<String>) -> OptionalContentError {
    OptionalContentError::Invalid(message.into())
}

fn hex_bytes(value: &str) -> Option<Vec<u8>> {
    let digits: Vec<u8> = value
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if !digits.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(digits.len() / 2);
    for index in (0..digits.len()).step_by(2) {
        let high = (digits[index] as char).to_digit(16)?;
        let low = (digits[index + 1] as char).to_digit(16)?;
        bytes.push((high * 16 + low) as u8);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{Array, Object, Trailer};

    fn doc_with_catalog(catalog: Dict) -> Document {
        let mut doc = Document::new();
        doc.set_object(Object::new(1, 0, Value::Dict(catalog)));
        doc.trailer = Trailer {
            dict: Dict::new().insert("Root", Value::Ref(1, 0)),
        };
        doc
    }

    fn hex_of(text: &str) -> String {
        let bytes = PdfTextString::from(text).encode();
        let mut hex = String::new();
        for byte in bytes {
            use std::fmt::Write as _;
            let _ = write!(hex, "{byte:02X}");
        }
        hex
    }

    fn sample_document() -> Document {
        let mut doc = Document::new();
        doc.set_object(Object::new(
            1,
            0,
            Value::Dict(
                Dict::new().insert(
                    "OCProperties",
                    Value::Dict(
                        Dict::new()
                            .insert(
                                "OCGs",
                                Value::Array(
                                    Array::new().push(Value::Ref(10, 0)).push(Value::Ref(11, 0)),
                                ),
                            )
                            .insert(
                                "D",
                                Value::Dict(
                                    Dict::new()
                                        .insert("BaseState", Value::Name("ON".into()))
                                        .insert(
                                            "ON",
                                            Value::Array(Array::new().push(Value::Ref(10, 0))),
                                        )
                                        .insert(
                                            "OFF",
                                            Value::Array(Array::new().push(Value::Ref(11, 0))),
                                        )
                                        .insert(
                                            "Order",
                                            Value::Array(
                                                Array::new().push(Value::Ref(10, 0)).push(
                                                    Value::Array(
                                                        Array::new().push(Value::Ref(11, 0)),
                                                    ),
                                                ),
                                            ),
                                        ),
                                ),
                            ),
                    ),
                ),
            ),
        ));
        doc.set_object(Object::new(
            10,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("OCG".into()))
                    .insert("Name", Value::String("Layer 1".into()))
                    .insert(
                        "Intent",
                        Value::Array(
                            Array::new()
                                .push(Value::Name("View".into()))
                                .push(Value::Name("Print".into())),
                        ),
                    ),
            ),
        ));
        doc.set_object(Object::new(
            11,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("OCG".into()))
                    .insert("Name", Value::HexString(hex_of("图层")))
                    .insert("Intent", Value::Name("View".into())),
            ),
        ));
        doc.trailer = Trailer {
            dict: Dict::new().insert("Root", Value::Ref(1, 0)),
        };
        doc
    }

    #[test]
    fn parses_groups_and_default_configuration() {
        let doc = sample_document();
        let properties = doc
            .optional_content_properties()
            .expect("parse /OCProperties")
            .expect("expected /OCProperties");
        assert_eq!(properties.groups.len(), 2);
        assert_eq!(properties.groups[0].object, 10);
        assert_eq!(properties.groups[0].name, "Layer 1");
        assert_eq!(properties.groups[0].intent, ["View", "Print"]);
        assert_eq!(properties.groups[1].object, 11);
        assert_eq!(properties.groups[1].name, "图层");
        assert_eq!(properties.groups[1].intent, ["View"]);

        let configuration = properties.default.as_ref().expect("/D");
        assert_eq!(configuration.base_state, BaseState::On);
        assert_eq!(configuration.on, [10]);
        assert_eq!(configuration.off, [11]);
        assert_eq!(configuration.order, [10, 11]);
        assert_eq!(properties.is_visible(10), Some(true));
        assert_eq!(properties.is_visible(11), Some(false));
        assert_eq!(
            properties.group(11).map(|group| group.name.as_str()),
            Some("图层")
        );
    }

    #[test]
    fn base_state_controls_groups_outside_on_and_off() {
        let configuration = OptionalContentConfiguration {
            base_state: BaseState::Off,
            on: vec![10],
            off: vec![10],
            order: Vec::new(),
        };
        assert_eq!(
            configuration.is_visible(10),
            Some(true),
            "/ON wins over /OFF"
        );
        assert_eq!(configuration.is_visible(12), Some(false));
        let unchanged = OptionalContentConfiguration {
            base_state: BaseState::Unchanged,
            on: Vec::new(),
            off: Vec::new(),
            order: Vec::new(),
        };
        assert_eq!(unchanged.is_visible(10), None);
    }

    #[test]
    fn missing_ocproperties_returns_none() {
        let doc = doc_with_catalog(Dict::new().insert("Type", Value::Name("Catalog".into())));
        assert_eq!(doc.optional_content_properties().expect("parse"), None);
    }

    #[test]
    fn rejects_malformed_optional_content() {
        let assert_invalid = |catalog: Dict, expected: &str| {
            let doc = doc_with_catalog(catalog);
            let error = doc
                .optional_content_properties()
                .expect_err("malformed structure should be rejected");
            assert!(
                matches!(&error, OptionalContentError::Invalid(_)),
                "{error}"
            );
            assert!(error.to_string().contains(expected), "{error}");
        };

        assert_invalid(
            Dict::new().insert("OCProperties", Value::Dict(Dict::new())),
            "/OCGs is required",
        );
        assert_invalid(
            Dict::new().insert(
                "OCProperties",
                Value::Dict(
                    Dict::new()
                        .insert("OCGs", Value::Array(Array::new()))
                        .insert(
                            "D",
                            Value::Dict(
                                Dict::new().insert("BaseState", Value::Name("Maybe".into())),
                            ),
                        ),
                ),
            ),
            "/BaseState",
        );
    }

    #[test]
    fn membership_policy_matrix() {
        let is_visible = |object: i64| -> Option<bool> {
            match object {
                10 => Some(true),
                11 => Some(false),
                _ => None,
            }
        };
        let ocmd = |policy: &str| {
            Dict::new()
                .insert(
                    "OCGs",
                    Value::Array(Array::new().push(Value::Ref(10, 0)).push(Value::Ref(11, 0))),
                )
                .insert("P", Value::Name(policy.into()))
        };
        assert_eq!(
            membership_visibility(&ocmd("AllOn"), &is_visible),
            Some(false)
        );
        assert_eq!(
            membership_visibility(&ocmd("AnyOn"), &is_visible),
            Some(true)
        );
        assert_eq!(
            membership_visibility(&ocmd("AnyOff"), &is_visible),
            Some(true)
        );
        assert_eq!(
            membership_visibility(&ocmd("AllOff"), &is_visible),
            Some(false)
        );

        let defaulted = Dict::new().insert(
            "OCGs",
            Value::Array(Array::new().push(Value::Ref(10, 0)).push(Value::Ref(11, 0))),
        );
        assert_eq!(
            membership_visibility(&defaulted, &is_visible),
            Some(false),
            "/P defaults to /AllOn"
        );
        assert_eq!(membership_visibility(&ocmd("Maybe"), &is_visible), None);
        assert_eq!(membership_visibility(&Dict::new(), &is_visible), None);

        let unknown_group =
            Dict::new().insert("OCGs", Value::Array(Array::new().push(Value::Ref(99, 0))));
        assert_eq!(membership_visibility(&unknown_group, &is_visible), None);

        let empty = OptionalContentProperties {
            groups: Vec::new(),
            default: None,
        };
        assert_eq!(empty.membership_visible(&ocmd("AllOn")), None);
    }

    #[test]
    fn membership_visibility_expression_overrides_policy() {
        let is_visible = |object: i64| -> Option<bool> {
            match object {
                10 => Some(true),
                11 => Some(false),
                _ => None,
            }
        };
        // /AnyOn alone is false (group 11 is off), but /Or 10 11 is true.
        let ve_wins = Dict::new()
            .insert("OCGs", Value::Array(Array::new().push(Value::Ref(11, 0))))
            .insert("P", Value::Name("AnyOn".into()))
            .insert(
                "VE",
                Value::Array(
                    Array::new()
                        .push(Value::Name("Or".into()))
                        .push(Value::Ref(10, 0))
                        .push(Value::Ref(11, 0)),
                ),
            );
        assert_eq!(membership_visibility(&ve_wins, &is_visible), Some(true));

        let nested = Dict::new().insert(
            "VE",
            Value::Array(
                Array::new()
                    .push(Value::Name("And".into()))
                    .push(Value::Array(
                        Array::new()
                            .push(Value::Name("Or".into()))
                            .push(Value::Ref(10, 0))
                            .push(Value::Ref(11, 0)),
                    ))
                    .push(Value::Array(
                        Array::new()
                            .push(Value::Name("Not".into()))
                            .push(Value::Ref(11, 0)),
                    )),
            ),
        );
        assert_eq!(membership_visibility(&nested, &is_visible), Some(true));

        let short_circuit = Dict::new().insert(
            "VE",
            Value::Array(
                Array::new()
                    .push(Value::Name("And".into()))
                    .push(Value::Ref(11, 0))
                    .push(Value::Ref(99, 0)),
            ),
        );
        assert_eq!(
            membership_visibility(&short_circuit, &is_visible),
            Some(false),
            "a known false /And operand absorbs an unknown one"
        );
    }

    #[test]
    fn rejects_invalid_group_entries() {
        let mut doc = Document::new();
        doc.set_object(Object::new(
            1,
            0,
            Value::Dict(Dict::new().insert(
                "OCProperties",
                Value::Dict(
                    Dict::new().insert("OCGs", Value::Array(Array::new().push(Value::Integer(10)))),
                ),
            )),
        ));
        doc.trailer = Trailer {
            dict: Dict::new().insert("Root", Value::Ref(1, 0)),
        };
        let error = doc
            .optional_content_properties()
            .expect_err("direct values are not OCG references");
        assert!(error.to_string().contains("indirect references"), "{error}");

        let mut doc = Document::new();
        doc.set_object(Object::new(
            1,
            0,
            Value::Dict(Dict::new().insert(
                "OCProperties",
                Value::Dict(
                    Dict::new().insert("OCGs", Value::Array(Array::new().push(Value::Ref(10, 0)))),
                ),
            )),
        ));
        doc.set_object(Object::new(
            10,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("OCG".into()))
                    .insert("Intent", Value::Name("View".into())),
            ),
        ));
        doc.trailer = Trailer {
            dict: Dict::new().insert("Root", Value::Ref(1, 0)),
        };
        let error = doc
            .optional_content_properties()
            .expect_err("missing /Name should be rejected");
        assert!(error.to_string().contains("/Name"), "{error}");
    }
}
