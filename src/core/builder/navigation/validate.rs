//! 导航输入的构建期校验：页码范围、名称唯一性、目标操作数与大纲限深/计数。
//!
//! 旧式命名目标（12.3.2.3 的 PDF 1.1 通道）的名字是**名字对象**，故额外要求
//! 可打印 ASCII；该限制属库策略，规范只规定键的形态。该通道内数组形态与
//! `/D` 字典形态共用一个目录字典，故重名判定跨形态进行。

use std::collections::BTreeSet;

use crate::core::reader::navigation::{
    Destination, MAX_OUTLINE_DEPTH, MAX_OUTLINE_ITEMS, OutlineDestination, PageLabelRange,
};
use crate::core::writer::WriteError;

use super::{LegacyDestinationEntry, OutlineEntry, invalid};

pub(super) fn page_labels(labels: &[PageLabelRange], page_count: usize) -> Result<(), WriteError> {
    let mut previous = None;
    for range in labels {
        if range.start >= page_count {
            return Err(invalid(format!(
                "page label range starts at page {} but the document has {page_count} pages",
                range.start
            )));
        }
        if let Some(previous) = previous
            && range.start <= previous
        {
            return Err(invalid(
                "page label ranges must be in strictly increasing start order",
            ));
        }
        if range.first_number == 0 {
            return Err(invalid("page label start number must be at least 1"));
        }
        if range.first_number > i64::MAX as u64 {
            return Err(invalid("page label start number is too large"));
        }
        previous = Some(range.start);
    }
    Ok(())
}

pub(super) fn named_destinations(
    entries: &[(String, Destination)],
    page_count: usize,
) -> Result<(), WriteError> {
    let mut names = BTreeSet::new();
    for (name, destination) in entries {
        if !names.insert(name.as_str()) {
            return Err(invalid(format!("duplicate named destination {name:?}")));
        }
        destination_page(destination, page_count)?;
    }
    Ok(())
}

pub(super) fn legacy_named_destinations(
    entries: &[LegacyDestinationEntry],
    page_count: usize,
) -> Result<(), WriteError> {
    let mut names = BTreeSet::new();
    for (name, destination, _form) in entries {
        if name.is_empty() {
            return Err(invalid("legacy destination name is empty"));
        }
        if name.contains('\0') {
            return Err(invalid(format!(
                "legacy destination name {name:?} contains a NUL character"
            )));
        }
        // 名字对象不是文本串：写出时无需任何编码，故本库只接受可打印 ASCII
        // （0x20–0x7E）。这是**库策略**而非规范 shall——规范只说键是名字对象。
        if name.chars().any(|c| !matches!(c, '\u{20}'..='\u{7e}')) {
            return Err(invalid(format!(
                "legacy destination name {name:?} is not printable ASCII; destination names are \
                 name objects, not text strings"
            )));
        }
        if !names.insert(name.as_str()) {
            return Err(invalid(format!(
                "duplicate legacy destination name {name:?}"
            )));
        }
        destination_page(destination, page_count)?;
    }
    Ok(())
}

pub(super) fn named_pages(
    entries: &[(String, usize)],
    page_count: usize,
) -> Result<(), WriteError> {
    let mut names = BTreeSet::new();
    for (name, page) in entries {
        if !names.insert(name.as_str()) {
            return Err(invalid(format!("duplicate named page {name:?}")));
        }
        if *page >= page_count {
            return Err(invalid(format!(
                "named page {name:?} targets page {page} but the document has {page_count} pages"
            )));
        }
    }
    Ok(())
}

pub(super) fn outline(entries: &[OutlineEntry], page_count: usize) -> Result<(), WriteError> {
    let mut count = 0;
    outline_items(entries, page_count, 0, &mut count)
}

fn outline_items(
    items: &[OutlineEntry],
    page_count: usize,
    depth: usize,
    count: &mut usize,
) -> Result<(), WriteError> {
    if depth > MAX_OUTLINE_DEPTH {
        return Err(invalid(format!(
            "outline nesting exceeds {MAX_OUTLINE_DEPTH} levels"
        )));
    }
    for item in items {
        *count += 1;
        if *count > MAX_OUTLINE_ITEMS {
            return Err(invalid(format!(
                "outline exceeds {MAX_OUTLINE_ITEMS} items"
            )));
        }
        if let Some(color) = item.color
            && color
                .iter()
                .any(|component| !component.is_finite() || !(0.0..=1.0).contains(component))
        {
            return Err(invalid("outline color components must be between 0 and 1"));
        }
        if item.flags & !0b11 != 0 {
            return Err(invalid("outline flags may only set bits 1 and 2"));
        }
        if let Some(destination) = &item.destination
            && let OutlineDestination::Explicit(destination) = destination
        {
            destination_page(destination, page_count)?;
        }
        outline_items(&item.children, page_count, depth + 1, count)?;
    }
    Ok(())
}

fn destination_page(destination: &Destination, page_count: usize) -> Result<(), WriteError> {
    if destination.page() >= page_count {
        return Err(invalid(format!(
            "destination page {} is out of range for {page_count} pages",
            destination.page()
        )));
    }
    match destination {
        Destination::Xyz {
            left, top, zoom, ..
        } => {
            check_finite(*left, "left")?;
            check_finite(*top, "top")?;
            if let Some(zoom) = zoom
                && (!zoom.is_finite() || *zoom <= 0.0)
            {
                return Err(invalid("destination zoom must be positive"));
            }
        }
        Destination::FitH { top, .. } | Destination::FitBH { top, .. } => {
            check_finite(*top, "top")?;
        }
        Destination::FitV { left, .. } | Destination::FitBV { left, .. } => {
            check_finite(*left, "left")?;
        }
        Destination::FitR { rect, .. } => {
            if rect.iter().any(|value| !value.is_finite()) {
                return Err(invalid("destination rectangle must be finite"));
            }
        }
        Destination::Fit { .. } | Destination::FitB { .. } => {}
    }
    Ok(())
}

fn check_finite(value: Option<f64>, what: &str) -> Result<(), WriteError> {
    if let Some(value) = value
        && !value.is_finite()
    {
        return Err(invalid(format!("destination {what} must be finite")));
    }
    Ok(())
}
