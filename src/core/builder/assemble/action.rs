//! 动作字典写端校验与版本门禁（12.6 / 12.7.5）。
//!
//! 注解动作（含 Hide 目标与表单动作取值）的校验在对象写出前完成；
//! 版本门禁按特性表统一判定，并把所需特性交回主流程登记。

use crate::core::builder::{Action, PdfBuilder};
use crate::core::types::PdfFeature;
use crate::core::writer::WriteError;

impl PdfBuilder {
    /// 注解与大纲实际引用到的全部动作（含 Link `/PA` 与父条目动作）。
    pub(super) fn used_actions(&self) -> Vec<&Action> {
        let mut actions = Vec::new();
        for entry in &self.annotations {
            if let Some(action) = entry.annot.action() {
                actions.push(action);
            }
            if let Some(action) = entry.annot.link().pressed_action() {
                actions.push(action);
            }
        }
        actions.extend(self.navigation.outline_actions());
        actions
    }

    /// 校验全部注解动作并返回其版本特性需求。
    ///
    /// 非法取值返回 [`WriteError::InvalidObject`]，版本不足返回
    /// [`WriteError::UnsupportedVersion`]；两者都在任何对象写出前发生。
    pub(super) fn action_requirements(&self) -> Result<Vec<PdfFeature>, WriteError> {
        let mut features = Vec::new();
        let thread_beads = self.navigation.article_thread_bead_counts();
        for action in self.used_actions() {
            validate_action(
                action,
                self.annotations.len(),
                self.optional_content_groups.len(),
                &thread_beads,
            )?;
            for feature in action_features(action) {
                if !features.contains(&feature) {
                    features.push(feature);
                }
            }
        }
        for feature in &features {
            if !self.version.supports(*feature) {
                return Err(WriteError::UnsupportedVersion(format!(
                    "{} require PDF {}, got {}",
                    feature,
                    feature.min_version().as_str(),
                    self.version.as_str()
                )));
            }
        }
        Ok(features)
    }
}

/// 动作所需的版本特性（12.7.5.2–12.7.5.4）。
pub(super) fn action_features(action: &Action) -> Vec<PdfFeature> {
    match action {
        Action::SubmitForm {
            fields, url_type, ..
        } => {
            let mut features = vec![PdfFeature::FormDataActions];
            if !fields.is_empty() {
                features.push(PdfFeature::FormActionFieldNames);
            }
            if url_type.is_some() {
                features.push(PdfFeature::SubmitFormUrlType);
            }
            features
        }
        Action::ResetForm { fields, .. } => {
            let mut features = vec![PdfFeature::FormDataActions];
            if !fields.is_empty() {
                features.push(PdfFeature::FormActionFieldNames);
            }
            features
        }
        Action::ImportData { .. } => vec![PdfFeature::FormDataActions],
        Action::Hide(_) => vec![PdfFeature::HideAction],
        Action::GoToE(_) => vec![PdfFeature::EmbeddedGoToAction],
        Action::SetOcgState(_) => vec![PdfFeature::SetOcgState],
        Action::Transition(_) => vec![PdfFeature::TransitionAction],
        Action::Thread(_) => vec![PdfFeature::ThreadAction],
        _ => Vec::new(),
    }
}

fn validate_action(
    action: &Action,
    annotation_count: usize,
    ocg_count: usize,
    thread_beads: &[usize],
) -> Result<(), WriteError> {
    match action {
        Action::Hide(hide) => hide.validate(annotation_count)?,
        Action::GoToE(action) => action.validate()?,
        Action::SetOcgState(state) => state.validate(ocg_count)?,
        Action::Transition(transition) => transition.validate()?,
        Action::Thread(thread) => thread.validate(thread_beads)?,
        Action::SubmitForm {
            url,
            fields,
            flags,
            url_type,
        } => {
            validate_url(url)?;
            validate_fields(fields, "SubmitForm")?;
            validate_flags(*flags, 0x2FFF, "SubmitForm")?;
            if let Some(name) = url_type
                && name.is_empty()
            {
                return Err(WriteError::InvalidObject(
                    "SubmitForm action /URLType must not be empty".into(),
                ));
            }
        }
        Action::ResetForm { fields, flags } => {
            validate_fields(fields, "ResetForm")?;
            validate_flags(*flags, 0x1, "ResetForm")?;
        }
        Action::ImportData { file } if file.trim().is_empty() => {
            return Err(WriteError::InvalidObject(
                "ImportData action /F must not be empty".into(),
            ));
        }
        _ => {}
    }
    Ok(())
}

fn validate_url(url: &str) -> Result<(), WriteError> {
    if url.is_empty() || !url.bytes().all(|byte| (0x21..=0x7E).contains(&byte)) {
        return Err(WriteError::InvalidObject(
            "SubmitForm action /F URL must be non-empty printable ASCII".into(),
        ));
    }
    Ok(())
}

fn validate_fields(fields: &[String], context: &str) -> Result<(), WriteError> {
    let mut seen = std::collections::HashSet::new();
    for field in fields {
        if field.trim().is_empty() {
            return Err(WriteError::InvalidObject(format!(
                "{context} action /Fields name must not be empty"
            )));
        }
        if !seen.insert(field.as_str()) {
            return Err(WriteError::InvalidObject(format!(
                "{context} action /Fields name {field:?} is duplicated"
            )));
        }
    }
    Ok(())
}

fn validate_flags(flags: u32, defined: u32, context: &str) -> Result<(), WriteError> {
    if flags & !defined != 0 {
        return Err(WriteError::InvalidObject(format!(
            "{context} action /Flags has undefined bits set (0x{:X})",
            flags & !defined
        )));
    }
    Ok(())
}
