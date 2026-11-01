//! 混合引用文件写出（hybrid-reference file，7.5.8.4，Table 19 `/XRefStm`）。
//!
//! 入口为 [`crate::core::writer::DocumentWriter::build_hybrid`]。产出
//! 「旧版（<1.5）读端也能打开」的文件：主交叉引用段是覆盖 `0 .. Size-1`
//! 全部条目的经典表，可隐藏对象在主表中登记为 **generation 65535 的自由
//! 条目**（旧读端把指向它们的间接引用当作 null），同时打包进对象流，由
//! **更新段** trailer 的 `/XRefStm` 指向的补充交叉引用流给出真实条目。
//! 文件形态（ISO 32000-1 EXAMPLE 2）：
//! - 文件头 + 可见顶层对象（全部非隐藏对象按普通间接对象写出）；
//! - 主经典表（`0 .. Size-1` 全条目）+ 主 trailer（**不含** `/XRefStm`，
//!   规范要求该条目只允许出现在更新段 trailer）+ `startxref` + `%%EOF`；
//! - 对象流（打包隐藏对象）；
//! - 交叉引用流对象（`/Type /XRef`，只覆盖隐藏号、对象流自身与流自身；
//!   **不写** `/Prev`——它「对混合引用文件无意义」）；
//! - 更新段 `xref 0 0`（空表，规范明说更新表可以不含条目）+ trailer
//!   （`/Size /Prev /XRefStm /Root` + `Info/ID` 透传）+ `startxref` + `%%EOF`。
//!
//! 隐藏集规则（7.5.8.4，规范 shall 与库策略混合，如实区分）：
//! - 【shall】文件根与目录字典、以及从根可见的对象不得隐藏。可见性递归
//!   按规范近似实现：可见字典的**全部直接引用边**都算可见（比规范
//!   「直接对象可见 + 必需键的值可见」更保守，属库策略，只会少隐藏）；
//!   可见数组逐元素可见；流对象的字典（即挂载资源）整体遍历。
//! - 【shall】主表自由条目的 next-generation 必须为 65535，且自由链
//!   按 Table 15 语义以 offset 字段串接「下一个自由对象号」、链尾指回 0。
//! - 【shall】`/Size` 大到包含 `/XRefStm` 流里定义的全部对象号。
//! - 【库策略 v1 边界】**流对象不隐藏**：可打包进对象流的只有非流对象，
//!   候选闭包中的流对象（如结构树挂的图片）一律顶层写出并在主表登记
//!   `n` 条目；**隐藏键白名单**默认为目录的 4 个可选键
//!   `StructTreeRoot` / `Outlines` / `PageLabels` / `Threads`，也可经
//!   [`HybridReferencePolicy::catalog_keys`] 显式指定；**不做线性化 +
//!   混合引用的额外顺序约束**（规范仅在同时使用线性化格式时要求）。
//! - 【shall】目标版本必须支持对象流与交叉引用流（PDF 1.5+），加密文档
//!   拒绝；与全量写出相同的「先缓冲、成功才写」零字节契约。

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::io::Write;

use crate::core::types::{
    Array, Dict, Document, Object, PdfFeature, PdfVersion, StreamData, Trailer, Value,
};

use super::WriteError;
use super::compression::StreamCompression;
use super::context::{WriteContext, XrefEntryOut};
use super::incremental::is_structural;
use super::serialize::{field_width, push_field};
use super::value::write_value;
use super::version::{validate_content_version, validate_output_version};

/// 默认允许隐藏的目录可选键（7.5.8.4「可隐藏的是可选间接引用指向的对象」，
/// 规范举例：结构树、大纲树、文章线、页标签）。
pub(super) const DEFAULT_HYBRID_KEYS: [&str; 4] =
    ["StructTreeRoot", "Outlines", "PageLabels", "Threads"];

/// 混合引用写出策略：哪些目录键的间接引用闭包允许被隐藏。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum HybridReferencePolicy {
    /// 默认策略：隐藏目录可选键 `StructTreeRoot` / `Outlines` /
    /// `PageLabels` / `Threads` 指向的闭包（仍需通过可见性过滤）。
    #[default]
    DefaultKeys,
    /// 显式目录键白名单：仅这些键的间接引用值参与隐藏集计算。
    CatalogKeys(Vec<String>),
}

impl HybridReferencePolicy {
    /// 以显式目录键列表构造策略。键名不带前导 `/`（如 `"Outlines"`）。
    pub fn catalog_keys(keys: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self::CatalogKeys(keys.into_iter().map(Into::into).collect())
    }

    /// 目录字典键 `key` 是否属于允许隐藏的可选键。
    fn covers(&self, key: &str) -> bool {
        match self {
            Self::DefaultKeys => DEFAULT_HYBRID_KEYS.contains(&key),
            Self::CatalogKeys(keys) => keys.iter().any(|k| k == key),
        }
    }

    /// 本策略对应的目录键集合（升序去重，便于错误信息与非目录键校验）。
    fn key_names(&self) -> Vec<String> {
        match self {
            Self::DefaultKeys => DEFAULT_HYBRID_KEYS
                .iter()
                .map(|k| (*k).to_string())
                .collect(),
            Self::CatalogKeys(keys) => {
                let mut v: Vec<String> = keys.clone();
                v.sort();
                v.dedup();
                v
            }
        }
    }
}

/// 写出混合引用文件。见模块文档。
pub(super) fn build(
    doc: &Document,
    w: &mut dyn Write,
    version: Option<PdfVersion>,
    compression: Option<StreamCompression>,
    policy: &HybridReferencePolicy,
) -> Result<(), WriteError> {
    if doc.is_encrypted() {
        return Err(WriteError::EncryptedDocument);
    }
    // 与全量写出一致：读端 Document 先预加载全部对象再重排结构。
    if doc.source.is_some()
        && let Err(e) = doc.load_all_objects()
    {
        doc.push_warning(format!(
            "hybrid writer: load_all_objects failed: {e}; continuing with partial cache"
        ));
    }
    let target = version.unwrap_or_default();
    validate_output_version(target)?;
    validate_content_version(target, doc)?;
    // 版本门：混合引用 = 经典主表 + 对象流 + 交叉引用流，三者皆需。
    for feature in [
        PdfFeature::HybridReferenceFiles,
        PdfFeature::ObjectStream,
        PdfFeature::XRefStream,
    ] {
        if !target.supports(feature) {
            return Err(WriteError::UnsupportedVersion(format!(
                "hybrid-reference output needs {feature}: requires PDF {} or later, but the target version is {target}",
                feature.min_version()
            )));
        }
    }

    let mut objs = doc.objects_for_write();
    // 剔除源文档遗留的结构容器（与新 hybrid 结构重复）。
    objs.retain(|o| !is_structural(o));

    let hidden = compute_hidden(&objs, &doc.trailer, policy)?;

    let mut ctx = WriteContext::new(target, compression);
    ctx.write_header();
    ctx.reserve_object_numbers(&objs);
    let objstm_num = ctx.next_object_number();
    let xrefstm_num = objstm_num + 1;
    // /Size 必须覆盖对象流与交叉引用流对象号（Table 19 shall），
    // 也不小于源 trailer 声明值（历史删除号留在表内成空洞条目）。
    let size = (xrefstm_num + 1).max(doc.trailer.size().unwrap_or(0));
    if size > i64::from(u32::MAX) {
        return Err(WriteError::InvalidObject(format!(
            "object numbers beyond 2^32 are not representable in a hybrid main xref table (size {size})"
        )));
    }

    // 1. 可见顶层对象：主表要按偏移登记它们，故全部普通间接对象写出。
    let top: Vec<&Object> = objs.iter().filter(|o| !hidden.contains(&o.num)).collect();
    ctx.write_top_level_objects(doc, &top)?;

    // 2. 主经典表 + 主 trailer（不含 /XRefStm）+ startxref + %%EOF。
    let main_index = ctx.entry_index();
    let free: BTreeSet<i64> = hidden
        .iter()
        .copied()
        .chain([objstm_num, xrefstm_num])
        .collect();
    let main_xref_offset = ctx.pos;
    append_main_section(&mut ctx, doc, size, &free, &main_index)?;

    // 3. 对象流：打包全部隐藏对象（均为非流对象，compute_hidden 已保证）。
    let packed: Vec<&Object> = objs.iter().filter(|o| hidden.contains(&o.num)).collect();
    ctx.write_object_stream(doc, &packed, objstm_num)?;

    // 4. 交叉引用流：只覆盖隐藏号（type 2）、对象流自身与流自身（type 1）。
    let xrefstm_offset = ctx.pos;
    append_xref_stream(
        &mut ctx,
        doc,
        &hidden,
        objstm_num,
        xrefstm_num,
        xrefstm_offset,
        size,
    )?;

    // 5. 更新段：空经典表 + trailer（/Prev 指主表、/XRefStm 指交叉引用流）。
    append_update_section(&mut ctx, doc, size, main_xref_offset, xrefstm_offset)?;

    w.write_all(&ctx.buf)?;
    Ok(())
}

/// 计算隐藏集（纯函数，供写出与单元测试共用）。
///
/// 返回要打包进对象流并在主表登记为 65535 自由条目的对象号集合；
/// 无目录、无可选键闭包、闭包全部可见或只剩流对象时返回
/// [`WriteError::InvalidObject`]（写出「没有隐藏对象的混合引用文件」
/// 没有意义，宁可拒绝且零字节）。
pub(super) fn compute_hidden(
    objs: &[Object],
    trailer: &Trailer,
    policy: &HybridReferencePolicy,
) -> Result<BTreeSet<i64>, WriteError> {
    let by_num: BTreeMap<i64, &Object> = objs.iter().map(|o| (o.num, o)).collect();
    let Some((root_num, _)) = trailer.root() else {
        return Err(WriteError::InvalidObject(
            "hybrid-reference output needs an indirect trailer /Root (the catalog must stay visible)"
                .into(),
        ));
    };

    // 可见闭包：从 /Root 目标与 trailer 其余键的间接引用（/Info、/ID 等）
    // 出发；目录（root_num）字典上「允许键 → 间接引用」的边被跳过。
    let mut visible: BTreeSet<i64> = BTreeSet::new();
    let mut seeds = vec![root_num];
    for (key, value) in &trailer.dict.0 {
        if key != "Root" {
            seeds.extend(indirect_refs_of(value));
        }
    }
    mark_closure(&seeds, &by_num, Some(root_num), policy, &mut visible);

    // 候选闭包：从各允许键的间接引用值出发，遍历全部边。
    // 值为直接对象 → 无法隐藏该键（不报错）；键缺失 → 跳过。
    let mut candidate_seeds: Vec<i64> = Vec::new();
    if let Some(catalog) = by_num.get(&root_num).and_then(|o| o.value.as_dict()) {
        for (key, value) in &catalog.0 {
            if policy.covers(key)
                && let Value::Ref(num, _) = value
            {
                candidate_seeds.push(*num);
            }
        }
    }
    let mut candidates: BTreeSet<i64> = BTreeSet::new();
    mark_closure(&candidate_seeds, &by_num, None, policy, &mut candidates);

    // 隐藏集 = 候选 − 可见 − 流对象（v1 只隐藏可打包的非流对象）。
    let hidden: BTreeSet<i64> = candidates
        .into_iter()
        .filter(|num| !visible.contains(num) && *num > 0)
        .filter(|num| {
            by_num
                .get(num)
                .is_some_and(|o| !matches!(o.value, Value::Stream(..)))
        })
        .collect();
    if hidden.is_empty() {
        let keys = policy.key_names().join(", ");
        return Err(WriteError::InvalidObject(format!(
            "hybrid-reference output has nothing to hide: no catalog key among [{keys}] resolves to a hideable (non-visible, non-stream) object closure"
        )));
    }
    Ok(hidden)
}

/// 收集一个值内的全部**间接引用**目标对象号（递归进入直接字典/数组；
/// 流对象只遍历其字典，流内容不解析）。迭代式遍历，防深嵌套栈溢出。
fn indirect_refs_of(value: &Value) -> Vec<i64> {
    let mut out = Vec::new();
    let mut stack = vec![value];
    while let Some(v) = stack.pop() {
        match v {
            Value::Dict(d) => stack.extend(d.0.values()),
            Value::Stream(d, _) => stack.extend(d.0.values()),
            Value::Array(a) => stack.extend(a.0.iter()),
            Value::Ref(num, _) => out.push(*num),
            _ => {}
        }
    }
    out
}

/// 从 `seeds` 出发做引用闭包并集入 `seen`。
///
/// `skip_edges_on` 为 `Some(catalog_num)` 时，遍历到该对象的字典会跳过
/// 「策略允许键 → 间接引用值」这一条边（值本身不入闭包，其子树也不经
/// 此边到达）；值为直接对象时照常遍历。
fn mark_closure(
    seeds: &[i64],
    by_num: &BTreeMap<i64, &Object>,
    skip_edges_on: Option<i64>,
    policy: &HybridReferencePolicy,
    seen: &mut BTreeSet<i64>,
) {
    let mut stack: Vec<i64> = seeds.to_vec();
    while let Some(num) = stack.pop() {
        if !seen.insert(num) {
            continue;
        }
        let Some(&obj) = by_num.get(&num) else {
            continue;
        };
        if skip_edges_on == Some(num)
            && let Some(dict) = obj.value.as_dict()
        {
            for (key, value) in &dict.0 {
                if policy.covers(key) && matches!(value, Value::Ref(..)) {
                    continue;
                }
                stack.extend(indirect_refs_of(value));
            }
            continue;
        }
        stack.extend(indirect_refs_of(&obj.value));
    }
}

/// 写出主经典表（`0 .. size-1` 全条目）、主 trailer 与其尾部。
///
/// 条目形态：
/// - 已写出的可见对象：`offset generation n`；
/// - 隐藏号 / 对象流号 / 交叉引用流号：自由条目，offset 字段按 Table 15
///   语义链到下一个自由号、链尾为 0，generation 固定 65535（shall：
///   防止旧读端复用这些对象号）；
/// - 从未分配的空洞号：`0000000000 00000 f`（沿用既有经典表形态）。
///
/// 主 trailer **不含** `/XRefStm` 与 `/Prev`（Table 19 shall：该条目只
/// 允许出现在更新段 trailer）。
fn append_main_section(
    ctx: &mut WriteContext,
    doc: &Document,
    size: i64,
    free: &BTreeSet<i64>,
    main_index: &HashMap<i64, XrefEntryOut>,
) -> Result<(), WriteError> {
    let mut chain = free.iter().copied().collect::<Vec<i64>>();
    chain.sort_unstable();
    let mut s = String::new();
    s.push_str("xref\n");
    let _ = writeln!(&mut s, "0 {size}");
    // 每条条目恰好 20 字节（Table 18 shall）：10 位偏移 + 空格 + 5 位
    // generation + 空格 + n/f + 两字节行尾分隔符（这里用 空格+CR）。
    // 0 号自由条目指向自由链首（EXAMPLE 2 形态）。
    let _ = write!(&mut s, "{:010} 65535 f \r", chain[0]);
    for num in 1..size {
        if let Ok(pos) = chain.binary_search(&num) {
            let next = chain.get(pos + 1).copied().unwrap_or(0);
            let _ = write!(&mut s, "{next:010} 65535 f \r");
            continue;
        }
        match main_index.get(&num) {
            Some(XrefEntryOut::Normal {
                generation, offset, ..
            }) => {
                let _ = write!(&mut s, "{offset:010} {generation:05} n \r");
            }
            _ => {
                let _ = write!(&mut s, "0000000000 00000 f \r");
            }
        }
    }
    s.push_str("trailer\n");
    s.push_str(&serialize_dict(&main_trailer_dict(doc, size)));
    s.push('\n');
    s.push_str("startxref\n");
    let _ = writeln!(&mut s, "{}", ctx.pos);
    s.push_str("%%EOF\n");
    ctx.buf.extend_from_slice(s.as_bytes());
    ctx.pos = ctx.buf.len() as u64;
    Ok(())
}

/// 写出补充交叉引用流对象（`/XRefStm` 所指）。
///
/// 只覆盖隐藏号（type 2：对象流号 + 流内索引）、对象流自身与流自身
/// （type 1）；`/Index` 为连续号归并的 (start, count) 对。不写 `/Prev`
/// （规范：混合引用文件的交叉引用流「无意义」）；`/Size` 与主表一致。
/// 流自身的条目偏移 = 写它之前记录的 `xrefstm_offset`（自引用处理，
/// 与增量写出 `write_xref_stream_section` 相同的「先取偏移再写对象」）。
fn append_xref_stream(
    ctx: &mut WriteContext,
    doc: &Document,
    hidden: &BTreeSet<i64>,
    objstm_num: i64,
    xrefstm_num: i64,
    xrefstm_offset: u64,
    size: i64,
) -> Result<(), WriteError> {
    let mut covered: BTreeMap<i64, (u64, u64, u64)> = BTreeMap::new();
    for entry in &ctx.entries {
        match entry {
            XrefEntryOut::InObjStream {
                num,
                stream_num,
                index,
            } if hidden.contains(num) => {
                covered.insert(*num, (2, *stream_num as u64, *index as u64));
            }
            XrefEntryOut::Normal { num, offset, .. } if *num == objstm_num => {
                covered.insert(*num, (1, *offset, 0));
            }
            _ => {}
        }
    }
    covered.insert(xrefstm_num, (1, xrefstm_offset, 0));

    // 连续号归并为 /Index 段。
    let mut runs: Vec<(i64, usize)> = Vec::new();
    for &num in covered.keys() {
        match runs.last_mut() {
            Some((start, count)) if *start + *count as i64 == num => *count += 1,
            _ => runs.push((num, 1)),
        }
    }

    let w1 = 1usize;
    let w2 = field_width(covered.values().map(|r| r.1).max().unwrap_or(0), 4);
    let w3 = field_width(covered.values().map(|r| r.2).max().unwrap_or(0), 2);
    let mut stream: Vec<u8> = Vec::new();
    for (start, count) in &runs {
        for num in *start..*start + *count as i64 {
            let (t, f2, f3) = covered.get(&num).copied().unwrap_or((0, 0, 0));
            push_field(&mut stream, t, w1);
            push_field(&mut stream, f2, w2);
            push_field(&mut stream, f3, w3);
        }
    }

    let ints = |vals: &[i64]| -> Value {
        Value::Array(
            vals.iter()
                .fold(Array::new(), |a, v| a.push(Value::Integer(*v))),
        )
    };
    let mut index_vals: Vec<i64> = Vec::new();
    for (start, count) in &runs {
        index_vals.push(*start);
        index_vals.push(*count as i64);
    }
    // Table 17 的 /Type /Size /Index /W 必须是直接对象：本字典全部为
    // 直接整数/数组，且流不加密（加密文档已在入口拒绝）、无 /Filter。
    let dict = Dict::new()
        .insert("Type", Value::Name("XRef".into()))
        .insert("Size", Value::Integer(size))
        .insert("W", ints(&[w1 as i64, w2 as i64, w3 as i64]))
        .insert("Index", ints(&index_vals))
        .insert("Length", Value::Integer(stream.len() as i64));
    let obj = Object::new(
        xrefstm_num,
        0,
        Value::Stream(dict, StreamData::Owned(stream)),
    );
    // 该流自身的条目已在上面手工登记（offset = xrefstm_offset），只写字节。
    ctx.write_raw_object(doc, &obj)?;
    Ok(())
}

/// 写出更新段：空经典表（`xref 0 0`，规范明说更新表可以不含条目）+
/// 带 `/Prev` 与 `/XRefStm` 的 trailer + `startxref` + `%%EOF`。
fn append_update_section(
    ctx: &mut WriteContext,
    doc: &Document,
    size: i64,
    main_xref_offset: u64,
    xrefstm_offset: u64,
) -> Result<(), WriteError> {
    let update_offset = ctx.pos;
    let mut dict = main_trailer_dict(doc, size)
        .insert("Prev", Value::Integer(main_xref_offset as i64))
        .insert("XRefStm", Value::Integer(xrefstm_offset as i64));
    // 加密文档已在入口拒绝，防御性去键（更新段 trailer 也不该带 /Encrypt）。
    dict.0.remove("Encrypt");
    let mut s = String::new();
    s.push_str("xref\n0 0\ntrailer\n");
    s.push_str(&serialize_dict(&dict));
    s.push('\n');
    s.push_str("startxref\n");
    let _ = writeln!(&mut s, "{update_offset}");
    s.push_str("%%EOF\n");
    ctx.buf.extend_from_slice(s.as_bytes());
    ctx.pos = ctx.buf.len() as u64;
    Ok(())
}

/// 干净的经典 trailer 字典：`/Size` + `/Root` + 透传 `/Encrypt /Info /ID`
/// （与 `ClassicXrefSerializer` 相同的取键集合；源 trailer 字典可能是
/// `/Type /XRef` 流字典，不可原样复用）。
fn main_trailer_dict(doc: &Document, size: i64) -> Dict {
    let mut dict = Dict::new()
        .insert("Size", Value::Integer(size))
        .insert("Root", doc.trailer.root_value());
    for key in ["Encrypt", "Info", "ID"] {
        if let Some(value) = doc.trailer.dict.get(key) {
            dict = dict.insert(key, value.clone());
        }
    }
    dict
}

/// 把字典序列化为文本（本模块内部便捷包装，避免各调用点重复样板）。
fn serialize_dict(dict: &Dict) -> String {
    let mut out = String::new();
    write_value(&Value::Dict(dict.clone()), &mut out);
    out
}

#[cfg(test)]
mod tests;
