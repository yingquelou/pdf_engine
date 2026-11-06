//! F.4.6 的**位装配**（模型与契约面在 [`super`]，本文件只做「校验 → 写位」）。
//!
//! 与读端 `EmbeddedFileHintTable::parse` 逐项对偶的编码序全部集中在本文件：
//! Table F.11 头部 7 项定宽（32/32/32/16/16/16/16 ＝ 160 位，35820—35832），随后
//! Table F.12 的**项主序（转置）**五列（35797—35799）——先把**所有组**的项 1 写完，
//! 再写所有组的项 2……项 5 是各组按项 4 个数依次相接的扁平标识符列。列宽依次取
//! 头部项 4/5/6/7 与**文档级** Table F.3 项 11（35835—35850）。
//!
//! 本文件的私有一切（含 [`validate`]）都不外露：`super` 只再导出
//! [`encode_embedded_file_table`]，公开面与同族单文件形态一致。

use crate::core::reader::linearization::hints::{EmbeddedFileGroupEntry, EmbeddedFileHeader};
use crate::core::writer::linearization::bits::{HintBitWriter, HintEncodeError};

use super::{
    EmbeddedFileEncodeError, EmbeddedFileTableInput, MAX_DECLARED_WIDTH, MAX_UNEVIDENCED_ITEMS,
    TABLE_F11_FIXED_ITEM_WIDTH_BITS, TABLE_F11_WIDTH_FIELD_BITS,
};

/// 把 [`EmbeddedFileTableInput`] 编码成从字节边界开始、可按 F.4.6（Table
/// F.11/F.12）解析的位流字节。顺序严格镜像读端 `EmbeddedFileHintTable::parse`：
/// Table F.11 的 7 项（32/32/32/16/16/16/16），随后按 35797—35799 的**项主序**写
/// Table F.12 的五列——项 1 列（宽＝头部项 4）→ 项 2 列（项 5）→ 项 3 列（项 6）
/// → 项 4 列（项 7）→ 项 5 的标识符列（宽＝**文档级** Table F.3 项 11，各组按项 4
/// 的个数依次扁平相接）。校验见 [`validate`]，数值溢出由 [`HintBitWriter::unsigned`]
/// 逐字段拒绝且 `Result::Err` 时不返回任何字节；产物可直接交回
/// [`EmbeddedFileHintTable::parse`](crate::core::reader::linearization::hints::EmbeddedFileHintTable::parse)
/// 解出、头 7 项与每组 5 项连同 `bits_consumed` 逐项相等。
pub fn encode_embedded_file_table(
    input: &EmbeddedFileTableInput,
) -> Result<Vec<u8>, EmbeddedFileEncodeError> {
    validate(input)?;
    let mut writer = HintBitWriter::new();
    write_header(&mut writer, &input.header)?;
    write_columns(
        &mut writer,
        &input.header,
        input.shared_object_id_bits,
        &input.groups,
    )?;
    Ok(writer.finish())
}

/// 写出前的全量自洽校验：文档级项 5 宽度与头部项 4—7 的范围、组向量非空且与
/// 项 3 等长、每组项 4 与项 5 列表等长、项 2/项 3 的互斥 shall、位宽全 0 时的
/// 读端计数上限。
fn validate(input: &EmbeddedFileTableInput) -> Result<(), EmbeddedFileEncodeError> {
    // 项 5 的宽度：Table F.3 项 11 的范围是**规范字面**（35524—35525）。
    if input.shared_object_id_bits > MAX_DECLARED_WIDTH {
        return Err(EmbeddedFileEncodeError::SharedIdWidthOutOfRange {
            bits: u64::from(input.shared_object_id_bits),
        });
    }
    // 头部项 4—7：同一措辞与同一 16 位存储的类比上限（库策略，见模块头）。
    for (item, bits) in [
        ("item 4", input.header.object_number_bits),
        ("item 5", input.header.group_object_bits),
        ("item 6", input.header.group_length_bits),
        ("item 7", input.header.shared_ref_count_bits),
    ] {
        if bits > MAX_DECLARED_WIDTH {
            return Err(EmbeddedFileEncodeError::DeclaredWidthOutOfRange {
                item,
                bits: u64::from(bits),
            });
        }
    }
    if input.groups.is_empty() {
        return Err(EmbeddedFileEncodeError::GroupsMissing);
    }
    // 项 3 是不可信形态的 32 位计数：模型里它必须已经与向量等长
    // （`usize::try_from` 失败即不等长，绝不按原值分配或写入）。
    let declared = usize::try_from(input.header.group_count);
    if declared != Ok(input.groups.len()) {
        return Err(EmbeddedFileEncodeError::GroupCountMismatch {
            declared: input.header.group_count,
            actual: input.groups.len(),
        });
    }
    // 读端位预算门的对偶：四项宽度全 0 时每组不占位，组数不得超过读端绝对上限。
    if input.bits_per_group() == 0 && input.groups.len() as u64 > MAX_UNEVIDENCED_ITEMS {
        return Err(EmbeddedFileEncodeError::UnevidencedGroupsTooLarge {
            count: input.groups.len() as u64,
            limit: MAX_UNEVIDENCED_ITEMS,
        });
    }
    for (index, entry) in input.groups.iter().enumerate() {
        // 项 4 与项 5 的列表长度：读端按项 4 把扁平列切回各组（35848—35850）。
        let declared = usize::try_from(entry.shared_reference_count);
        if declared != Ok(entry.shared_object_ids.len()) {
            return Err(EmbeddedFileEncodeError::ReferenceCountMismatch {
                group: index,
                declared: entry.shared_reference_count,
                actual: entry.shared_object_ids.len(),
            });
        }
        // 项 2/项 3/项 4 的互斥 shall（35837—35844）：判定复用读端，两侧同源。
        if let Some(rule) = entry.zero_item_problems().into_iter().next() {
            return Err(EmbeddedFileEncodeError::SharedOnlyGroupViolation {
                group: index,
                rule,
                object_count: entry.object_count,
                group_length: entry.group_length,
                shared_reference_count: entry.shared_reference_count,
            });
        }
    }
    // 文档级宽度为 0 时项 5 的每个标识符不占位，总数同样受读端上限约束。
    if input.shared_object_id_bits == 0 && input.shared_object_id_count() > MAX_UNEVIDENCED_ITEMS {
        return Err(EmbeddedFileEncodeError::UnevidencedIdentifiersTooLarge {
            count: input.shared_object_id_count(),
            limit: MAX_UNEVIDENCED_ITEMS,
        });
    }
    Ok(())
}

/// Table F.11 头部 7 项：32/32/32/16/16/16/16（35820—35832），项序与宽度严格
/// 镜像读端 `parse` 的结构体字面量。
fn write_header(w: &mut HintBitWriter, h: &EmbeddedFileHeader) -> Result<(), HintEncodeError> {
    w.unsigned(h.first_object_number, TABLE_F11_FIXED_ITEM_WIDTH_BITS)?;
    w.unsigned(h.first_object_location, TABLE_F11_FIXED_ITEM_WIDTH_BITS)?;
    w.unsigned(h.group_count, TABLE_F11_FIXED_ITEM_WIDTH_BITS)?;
    w.unsigned(u64::from(h.object_number_bits), TABLE_F11_WIDTH_FIELD_BITS)?;
    w.unsigned(u64::from(h.group_object_bits), TABLE_F11_WIDTH_FIELD_BITS)?;
    w.unsigned(u64::from(h.group_length_bits), TABLE_F11_WIDTH_FIELD_BITS)?;
    w.unsigned(
        u64::from(h.shared_ref_count_bits),
        TABLE_F11_WIDTH_FIELD_BITS,
    )?;
    Ok(())
}

/// Table F.12 的五列，按 35797—35799 的**项主序（转置）**：先把**所有组**的项 1
/// 写完，再写所有组的项 2……项 5 是各组按项 4 个数依次相接的扁平标识符列。
/// 列宽依次取头部项 4/5/6/7 与**文档级** Table F.3 项 11。
fn write_columns(
    w: &mut HintBitWriter,
    h: &EmbeddedFileHeader,
    shared_object_id_bits: u32,
    groups: &[EmbeddedFileGroupEntry],
) -> Result<(), HintEncodeError> {
    // 项 1 列：本条目关联的内嵌文件流对象号（宽度 = 头部项 4，35835—35836）。
    for entry in groups {
        w.unsigned(entry.object_number, h.object_number_bits)?;
    }
    // 项 2 列：组内对象数，绝对值（宽度 = 头部项 5，35837—35840）。
    for entry in groups {
        w.unsigned(entry.object_count, h.group_object_bits)?;
    }
    // 项 3 列：组长度（字节），绝对值（宽度 = 头部项 6，35841—35844）。
    for entry in groups {
        w.unsigned(entry.group_length, h.group_length_bits)?;
    }
    // 项 4 列：本组引用的共享对象数（宽度 = 头部项 7，35845—35846）。
    for entry in groups {
        w.unsigned(entry.shared_reference_count, h.shared_ref_count_bits)?;
    }
    // 项 5 列：逐组位打包的共享对象标识符（宽度 = 文档级项 11，35847—35850）。
    for entry in groups {
        for &id in &entry.shared_object_ids {
            w.unsigned(id, shared_object_id_bits)?;
        }
    }
    Ok(())
}
