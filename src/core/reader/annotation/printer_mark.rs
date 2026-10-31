//! Printer's mark 注解专有条目读取（14.11.3，Table 362/363）。
//!
//! 打印机标记的外观由注解 `/AP` 的 `/N` 条目定义：既可以是单个
//! Form XObject 流，也可以是「状态名 → 外观流」的子字典，此时由 `/AS`
//! 选择实际显示/打印的外观（缺 `/AS` 时仅允许恰一项）。`/R` `/D`
//! 中的外观永不显示或打印，不参与选择。
//!
//! Table 362 的专有条目只有 `/MN`（name，如 `ColorBar`、
//! `RegistrationTarget`）；`/F` 与 `/AP` 按 14.11.3 正文为必填。选中的
//! 外观表单可携带 Table 363 的 `/MarkStyle`（文本串）与 `/Colorants`
//! （色料名 → Separation 颜色空间数组，8.6.6.4；键须与数组中的色料名一致）。
//!
//! 本模块只校验条目结构与外观流的 `/Subtype /Form`，不校验 `/F` 的位值
//! （写端恒写 68）。

use std::collections::BTreeMap;

use crate::core::reader::color_space::{ColorSpaceModel, SeparationSpace};
use crate::core::types::{Dict, Document, Value};

use super::{AnnotationSubtype, Result, malformed};

/// Printer's mark 注解（Table 362）：`/MN` 与选定外观表单的条目。
#[derive(Debug, Clone, PartialEq)]
pub struct PrinterMark {
    /// `/MN` 标记名称（name，非通用注解的 `/NM` 文本串）。
    pub mark_name: Option<String>,
    /// 选定的外观表单及其 Table 363 附加条目。
    pub form: PrinterMarkForm,
}

/// 打印机标记的 Form XObject（Table 363，PDF 1.4）。
#[derive(Debug, Clone, PartialEq)]
pub struct PrinterMarkForm {
    /// 选中外观流的间接引用；直接嵌入的流为 `None`。
    pub reference: Option<(i64, i64)>,
    /// `/MarkStyle` 人类可读的标记描述。
    pub mark_style: Option<String>,
    /// `/Colorants` 色料定义：键与 Separation 颜色空间的色料名一致。
    pub colorants: BTreeMap<String, SeparationSpace>,
    /// 选中外观流的字典（含 `/Subtype /Form` 等条目）。
    pub dict: Dict,
}

/// 解析 Printer's mark 注解条目；非 `PrinterMark` 子类型返回 `None`。
pub(super) fn parse_printer_mark(
    document: &Document,
    dict: &Dict,
    subtype: &AnnotationSubtype,
) -> Result<Option<PrinterMark>> {
    if *subtype != AnnotationSubtype::PrinterMark {
        return match dict.get("MN") {
            Some(_) => Err(malformed(
                "annotation /MN is specific to PrinterMark annotations",
            )),
            None => Ok(None),
        };
    }
    let mark_name = match dict.get("MN") {
        None => None,
        Some(value) => Some(
            value
                .as_name()
                .ok_or_else(|| malformed("PrinterMark annotation /MN must be a name"))?
                .to_string(),
        ),
    };
    if dict.get("F").is_none() {
        return Err(malformed("PrinterMark annotation is missing /F"));
    }
    let forms = form_entries(document, dict)?;
    Ok(Some(PrinterMark {
        mark_name,
        form: forms,
    }))
}

/// 按 `/AP /N` 与 `/AS` 选出外观流并读取 Table 363 条目。
fn form_entries(document: &Document, dict: &Dict) -> Result<PrinterMarkForm> {
    let ap = dict
        .get("AP")
        .ok_or_else(|| malformed("PrinterMark annotation is missing /AP"))?;
    let ap = resolve(document, ap)?;
    let ap = ap
        .as_dict()
        .ok_or_else(|| malformed("PrinterMark annotation /AP must be a dictionary"))?;
    let normal = ap
        .get("N")
        .ok_or_else(|| malformed("PrinterMark annotation /AP is missing /N"))?;
    let selected = select_appearance(document, dict, normal)?;
    let reference = selected.as_ref();
    let stream = resolve(document, &selected)?;
    let Value::Stream(stream_dict, _) = stream else {
        return Err(malformed(
            "PrinterMark annotation appearance must be a form XObject stream",
        ));
    };
    if stream_dict.get("Subtype").and_then(Value::as_name) != Some("Form") {
        return Err(malformed(
            "PrinterMark annotation appearance /Subtype must be Form",
        ));
    }
    Ok(PrinterMarkForm {
        reference,
        mark_style: parse_mark_style(&stream_dict)?,
        colorants: parse_colorants(document, &stream_dict)?,
        dict: stream_dict,
    })
}

/// 从 `/AP /N` 选出外观流值：直接流，或按 `/AS` 选择的状态子字典项。
fn select_appearance(document: &Document, dict: &Dict, normal: &Value) -> Result<Value> {
    let resolved = resolve(document, normal)?;
    let states = match &resolved {
        Value::Stream(_, _) => {
            if dict.get("AS").is_some() {
                return Err(malformed(
                    "PrinterMark annotation /AS is not permitted when /AP /N is a stream",
                ));
            }
            return Ok(normal.clone());
        }
        Value::Dict(states) => states,
        _ => {
            return Err(malformed(
                "PrinterMark annotation /AP /N must be a stream or a state dictionary",
            ));
        }
    };
    if states.0.is_empty() {
        return Err(malformed(
            "PrinterMark annotation /AP /N state dictionary is empty",
        ));
    }
    if let Some(state) = dict.get("AS") {
        let name = state
            .as_name()
            .ok_or_else(|| malformed("PrinterMark annotation /AS must be a name"))?;
        return states.get(name).cloned().ok_or_else(|| {
            malformed(format!(
                "PrinterMark annotation /AS names unknown appearance state {name}"
            ))
        });
    }
    if states.0.len() > 1 {
        return Err(malformed(
            "PrinterMark annotation /AP /N has multiple appearances but no /AS",
        ));
    }
    states
        .0
        .values()
        .next()
        .cloned()
        .ok_or_else(|| malformed("PrinterMark annotation /AP /N state dictionary is empty"))
}

/// Table 363 `/MarkStyle`：可选文本串。
fn parse_mark_style(form: &Dict) -> Result<Option<String>> {
    match form.get("MarkStyle") {
        None => Ok(None),
        Some(value) => crate::core::reader::content::text_value(value)
            .map(Some)
            .ok_or_else(|| malformed("PrinterMark form /MarkStyle must be a text string")),
    }
}

/// Table 363 `/Colorants`：色料名 → Separation 颜色空间数组。
fn parse_colorants(document: &Document, form: &Dict) -> Result<BTreeMap<String, SeparationSpace>> {
    let Some(value) = form.get("Colorants") else {
        return Ok(BTreeMap::new());
    };
    let resolved = resolve(document, value)?;
    let colorants = resolved
        .as_dict()
        .ok_or_else(|| malformed("PrinterMark form /Colorants must be a dictionary"))?;
    let mut out = BTreeMap::new();
    for (name, value) in colorants.0.iter() {
        let model = document.color_space(value).map_err(|error| {
            malformed(format!(
                "PrinterMark form /Colorants entry /{name} is not a valid color space: {error}"
            ))
        })?;
        let ColorSpaceModel::Separation(space) = model else {
            return Err(malformed(format!(
                "PrinterMark form /Colorants entry /{name} must be a Separation color space"
            )));
        };
        if space.colorant != *name {
            return Err(malformed(format!(
                "PrinterMark form /Colorants entry /{name} names colorant {}",
                space.colorant
            )));
        }
        out.insert(name.clone(), space);
    }
    Ok(out)
}

fn resolve(document: &Document, value: &Value) -> Result<Value> {
    crate::core::reader::navigation::resolve(document, value)
        .map_err(|error| super::AnnotationError::Malformed(error.to_string()))
}
