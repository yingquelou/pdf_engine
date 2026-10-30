//! Sound 注解专有条目（12.5.6.16 Table 185）的构建态与构建期校验。
//!
//! `/Sound` 是声音流对象的间接引用（Sound 注解必填），句柄来自
//! [`PdfBuilder::add_sound`](crate::core::builder::PdfBuilder::add_sound)；
//! `/Name` 为图标名（标准值 `Speaker`、`Mic`，缺省 `Speaker`），与 Text
//! 注解共享 [`PdfAnnotation`] 的 `icon_name` 字段，仅在显式设置时写出。
//! `/AP` 外观流存在时优先于 `/Sound` 与 `/Name` 呈现。

use super::{AnnotationKind, PdfAnnotation, validate_name_value};

impl PdfAnnotation {
    /// 声音对象引用 `/Sound`（Table 185，Sound 注解必填）；参数为
    /// [`PdfBuilder::add_sound`](crate::core::builder::PdfBuilder::add_sound)
    /// 返回的声音句柄。
    pub fn with_sound(mut self, handle: usize) -> Self {
        self.sound = Some(handle);
        self
    }

    /// 图标名 `/Name`（Sound 注解；标准值 `Speaker`、`Mic`，缺省 `Speaker`
    /// 由呈现端应用，未显式设置时不写出）。
    pub fn with_sound_icon_name(mut self, name: impl Into<String>) -> Self {
        self.icon_name = Some(name.into());
        self
    }

    /// 校验 `/Sound` 的子类型归属与必填性；`/Name` 的名字语法由
    /// `subtype::validate_subtype_entries` 统一校验。
    pub(super) fn validate_sound_entries(&self) -> Result<(), String> {
        if self.kind == AnnotationKind::Sound {
            if self.sound.is_none() {
                return Err("Sound annotation requires /Sound (sound object handle)".into());
            }
        } else if self.sound.is_some() {
            return Err(format!(
                "annotation /Sound is specific to Sound annotations, got {}",
                self.kind.name()
            ));
        }
        if let Some(icon) = &self.icon_name {
            validate_name_value(icon, "annotation /Name")?;
        }
        Ok(())
    }
}
