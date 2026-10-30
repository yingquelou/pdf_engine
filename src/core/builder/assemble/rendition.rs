//! media clip section 的对象分配与写出（13.2.6 Table 277/278）。
//!
//! media clip section 的 `/D` 必须写成间接引用，因此 section 链上每个
//! 下一级 clip 都占用一个对象号；最外层 clip 内联在 rendition `/C` 中，
//! 不占用对象号。分配按动作实际写出顺序遍历：注解按注册顺序（Link
//! `/PA` 只允许 URI 动作，不会消费 section 目标），随后大纲按子项先于
//! 父项的写出顺序；选择器 rendition 的候选按数组顺序。
//!
//! 分配结果分两份：`roots` 为每个内联 section 的首级 `/D` 目标号，由
//! 注解与大纲共享的游标按同一顺序消费；`targets` 为全部 section `/D`
//! 目标号，供 [`write_objects`] 逐级写出。若内联消费与写对象顺序不一致，
//! 共享游标会在越界时返回 [`WriteError::InvalidObject`]。

use crate::core::builder::types::{Action, MediaClip, Rendition};
use crate::core::types::{Document, Object, Value};
use crate::core::writer::WriteError;

/// section 链对象号分配结果。
pub(super) struct RenditionNumbers {
    /// 每个内联 section 的首级 `/D` 目标号（按动作写出顺序）。
    pub(super) roots: Vec<i64>,
    /// 全部 section `/D` 目标号（按分配顺序）。
    pub(super) targets: Vec<i64>,
}

/// 按动作写出顺序为 section 链分配对象号。
pub(super) fn allocate(actions: &[&Action], num: &mut i64) -> RenditionNumbers {
    let mut numbers = RenditionNumbers {
        roots: Vec::new(),
        targets: Vec::new(),
    };
    for action in actions {
        let Action::Rendition(action) = action else {
            continue;
        };
        let Some(rendition) = action.rendition() else {
            continue;
        };
        allocate_rendition(rendition, num, &mut numbers);
    }
    numbers
}

fn allocate_rendition(rendition: &Rendition, num: &mut i64, numbers: &mut RenditionNumbers) {
    match rendition {
        Rendition::Media(media) => {
            let clip = media.clip();
            if clip.is_section() {
                // 内联 `/C` 的 `/D` 指向 section 链的第一个目标对象。
                numbers.roots.push(*num);
                allocate_targets(clip, num, &mut numbers.targets);
            }
        }
        Rendition::Selector(selector) => {
            for candidate in selector.renditions() {
                allocate_rendition(candidate, num, numbers);
            }
        }
    }
}

/// 为 section 链的每个下一级 clip 分配对象号（前序）。
fn allocate_targets(clip: &MediaClip, num: &mut i64, targets: &mut Vec<i64>) {
    let MediaClip::Section(section) = clip else {
        return;
    };
    targets.push(*num);
    *num += 1;
    allocate_targets(section.next(), num, targets);
}

/// 写出全部 section 链对象；`nums` 为 [`allocate`] 返回的 `targets`。
pub(super) fn write_objects(
    doc: &mut Document,
    actions: &[&Action],
    nums: &[i64],
    form_nums: &[i64],
    embedded_nums: &[i64],
) -> Result<(), WriteError> {
    let mut index = 0usize;
    for action in actions {
        let Action::Rendition(action) = action else {
            continue;
        };
        let Some(rendition) = action.rendition() else {
            continue;
        };
        write_rendition(doc, rendition, nums, &mut index, form_nums, embedded_nums)?;
    }
    if index != nums.len() {
        return Err(WriteError::InvalidObject(format!(
            "media clip section wrote {index} of {} allocated target objects",
            nums.len()
        )));
    }
    Ok(())
}

fn write_rendition(
    doc: &mut Document,
    rendition: &Rendition,
    nums: &[i64],
    index: &mut usize,
    form_nums: &[i64],
    embedded_nums: &[i64],
) -> Result<(), WriteError> {
    match rendition {
        Rendition::Media(media) => {
            if let MediaClip::Section(section) = media.clip() {
                let first = next_num(nums, index, "media clip section /D")?;
                write_clip_object(
                    doc,
                    section.next(),
                    first,
                    nums,
                    index,
                    form_nums,
                    embedded_nums,
                )?;
            }
            Ok(())
        }
        Rendition::Selector(selector) => {
            for candidate in selector.renditions() {
                write_rendition(doc, candidate, nums, index, form_nums, embedded_nums)?;
            }
            Ok(())
        }
    }
}

/// 在 `num` 处写出一个属于 section 链的 clip 对象。
fn write_clip_object(
    doc: &mut Document,
    clip: &MediaClip,
    num: i64,
    nums: &[i64],
    index: &mut usize,
    form_nums: &[i64],
    embedded_nums: &[i64],
) -> Result<(), WriteError> {
    match clip {
        MediaClip::Data(data) => {
            let dict = data.to_dict(form_nums, embedded_nums)?;
            doc.set_object(Object::new(num, 0, Value::Dict(dict)));
        }
        MediaClip::Section(section) => {
            let child = next_num(nums, index, "media clip section /D")?;
            let dict = section.to_dict(Value::Ref(child, 0));
            doc.set_object(Object::new(num, 0, Value::Dict(dict)));
            write_clip_object(
                doc,
                section.next(),
                child,
                nums,
                index,
                form_nums,
                embedded_nums,
            )?;
        }
    }
    Ok(())
}

fn next_num(nums: &[i64], index: &mut usize, context: &str) -> Result<i64, WriteError> {
    let num = nums.get(*index).copied().ok_or_else(|| {
        WriteError::InvalidObject(format!(
            "{context} has no allocated object number at index {}",
            *index
        ))
    })?;
    *index += 1;
    Ok(num)
}
