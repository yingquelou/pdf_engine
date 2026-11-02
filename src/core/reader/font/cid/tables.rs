//! Parsers for the CID font metric arrays `/W` (Table 115) and `/W2`
//! (Table 116), which share the same entry shapes.

use std::collections::BTreeMap;

use crate::core::types::{Array, Document, Value};

use super::VerticalMetrics;
use super::{FontError, FontResult, number};

/// Parse `/W2`, which uses the same two entry shapes as `/W` but each CID
/// carries the `w1_y v_x v_y` triple defined by ISO 32000-1 Table 116.
pub(super) fn parse_vertical_metrics(
    document: &Document,
    values: &Array,
) -> FontResult<BTreeMap<u16, VerticalMetrics>> {
    let mut metrics = BTreeMap::new();
    let mut index = 0;
    while index < values.len() {
        let first = resolve_cid(document, &values.0[index], "/W2")?;
        let next = values
            .0
            .get(index + 1)
            .ok_or_else(|| FontError::Invalid("CID font /W2 has a truncated entry".into()))?;
        let next = document.resolve_value(next)?;
        if let Some(triples) = next.as_array() {
            if triples.is_empty() || triples.len() % 3 != 0 {
                return Err(FontError::Invalid(
                    "CID font /W2 metric array length must be a positive multiple of 3".into(),
                ));
            }
            for (offset, triple) in triples.0.as_chunks::<3>().0.iter().enumerate() {
                let cid = first
                    .checked_add(u16::try_from(offset).map_err(|_| {
                        FontError::Invalid("CID font /W2 metric array is too long".into())
                    })?)
                    .ok_or_else(|| FontError::Invalid("CID font /W2 CID overflows".into()))?;
                let metric = VerticalMetrics {
                    w1_y: metric_number(document, &triple[0], "/W2")?,
                    v1x: metric_number(document, &triple[1], "/W2")?,
                    v1y: metric_number(document, &triple[2], "/W2")?,
                };
                metrics.insert(cid, metric);
            }
            index += 2;
        } else {
            let last = integer_cid(&next, "/W2")?;
            if last < first {
                return Err(FontError::Invalid(
                    "CID font /W2 range ends before it starts".into(),
                ));
            }
            if values.len() < index + 5 {
                return Err(FontError::Invalid(
                    "CID font /W2 has a truncated range".into(),
                ));
            }
            let metric = VerticalMetrics {
                w1_y: metric_number(document, &values.0[index + 2], "/W2")?,
                v1x: metric_number(document, &values.0[index + 3], "/W2")?,
                v1y: metric_number(document, &values.0[index + 4], "/W2")?,
            };
            // Overlapping `/W2` entries are not forbidden by the spec; a later
            // entry for the same CID overrides the earlier one.
            for cid in first..=last {
                metrics.insert(cid, metric);
            }
            index += 5;
        }
    }
    Ok(metrics)
}

pub(super) fn parse_widths(document: &Document, values: &Array) -> FontResult<BTreeMap<u16, f64>> {
    let mut widths = BTreeMap::new();
    let mut index = 0;
    while index < values.len() {
        let first = resolve_cid(document, &values.0[index], "/W")?;
        let next = values
            .0
            .get(index + 1)
            .ok_or_else(|| FontError::Invalid("CID font /W has a truncated entry".into()))?;
        let next = document.resolve_value(next)?;
        if let Some(width_array) = next.as_array() {
            if width_array.is_empty() {
                return Err(FontError::Invalid(
                    "CID font /W contains an empty width array".into(),
                ));
            }
            for (offset, value) in width_array.iter().enumerate() {
                let cid = first
                    .checked_add(u16::try_from(offset).map_err(|_| {
                        FontError::Invalid("CID font /W width array is too long".into())
                    })?)
                    .ok_or_else(|| FontError::Invalid("CID font /W CID overflows".into()))?;
                insert_width(document, &mut widths, cid, value)?;
            }
            index += 2;
        } else {
            let last = integer_cid(&next, "/W")?;
            if last < first {
                return Err(FontError::Invalid(
                    "CID font /W range ends before it starts".into(),
                ));
            }
            let width_value = values
                .0
                .get(index + 2)
                .ok_or_else(|| FontError::Invalid("CID font /W has a truncated range".into()))?;
            let width_value = document.resolve_value(width_value)?;
            let width = number(&width_value)
                .ok_or_else(|| FontError::Invalid("CID font /W width is not finite".into()))?;
            // Overlapping `/W` entries are not forbidden by the spec; a later
            // entry for the same CID overrides the earlier one.
            for cid in first..=last {
                widths.insert(cid, width);
            }
            index += 3;
        }
    }
    Ok(widths)
}

fn resolve_cid(document: &Document, value: &Value, entry: &str) -> FontResult<u16> {
    integer_cid(&document.resolve_value(value)?, entry)
}

fn integer_cid(value: &Value, entry: &str) -> FontResult<u16> {
    match value {
        Value::Integer(cid) => u16::try_from(*cid)
            .map_err(|_| FontError::Invalid(format!("CID font {entry} CID is outside 0..=65535"))),
        _ => Err(FontError::Invalid(format!(
            "CID font {entry} CID is not an integer"
        ))),
    }
}

pub(super) fn metric_number(document: &Document, value: &Value, entry: &str) -> FontResult<f64> {
    let value = document.resolve_value(value)?;
    number(&value).ok_or_else(|| {
        FontError::Invalid(format!(
            "CID font {entry} contains a value that is not finite"
        ))
    })
}

fn insert_width(
    document: &Document,
    widths: &mut BTreeMap<u16, f64>,
    cid: u16,
    value: &Value,
) -> FontResult<()> {
    let value = document.resolve_value(value)?;
    let width = number(&value)
        .ok_or_else(|| FontError::Invalid("CID font /W width is not finite".into()))?;
    // Later entries win when `/W` assigns the same CID more than once.
    widths.insert(cid, width);
    Ok(())
}
