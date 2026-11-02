//! Optional content group (OCG) and default configuration data model
//! (PDF 8.11, Tables 99 and 101).

use crate::core::types::Dict;

use super::visibility::membership_visibility;

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
