//! xref/对象流输出策略与对象布局（策略模式）。

use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};
use std::fmt::Write as _;

use super::WriteError;
use super::context::{WriteContext, XrefEntryOut};
use super::value::write_value;

// ── 交叉引用序列化与对象打包：策略模式 ─────────────────────────────
//
// 「用哪些较新特性输出」由统一的版本能力模型（[`PdfVersion::supports`]）
// 驱动：目标版本 ≥1.5 时选 XRef 流 + 对象流，否则退回经典 xref 表。
// 两条选择各自抽象为 trait + 两个策略实现，使版本特性管理内聚、可扩展，
// 而不是散落的 if。

/// 交叉引用序列化策略：把交叉引用段与文件尾部写入输出缓冲。
pub(super) trait XrefSerializer {
    fn write_xref_and_tail(&self, ctx: &mut WriteContext, doc: &Document)
    -> Result<(), WriteError>;
}

/// 经典交叉引用表策略（1.0–1.4 合法）：输出 `xref ... trailer`，
/// `startxref` 指向该表偏移。
pub(super) struct ClassicXrefSerializer;

impl XrefSerializer for ClassicXrefSerializer {
    fn write_xref_and_tail(
        &self,
        ctx: &mut WriteContext,
        doc: &Document,
    ) -> Result<(), WriteError> {
        let xref_offset = ctx.pos;
        // 顶层对象条目：仅 Normal（可按偏移定位）。InObjStream 条目无法用
        // 经典表表达，而经典表策略仅在目标版本 <1.5（对象打包为
        // PlainObjectLayout）时被选中，故此处不会出现。
        let mut items: Vec<(i64, i64, u64)> = ctx
            .entries
            .iter()
            .filter_map(|e| match e {
                XrefEntryOut::Normal {
                    num,
                    generation,
                    offset,
                } => Some((*num, *generation, *offset)),
                XrefEntryOut::InObjStream { .. } => None,
            })
            .collect();
        items.sort_by_key(|(num, _, _)| *num);

        let size = doc.trailer.size().unwrap_or(0).max(ctx.max_num + 1);
        let mut s = String::new();
        s.push_str("xref\n");
        let _ = writeln!(&mut s, "0 {size}");
        let _ = writeln!(&mut s, "0000000000 65535 f \r");
        let mut next_num = 1i64;
        for (num, generation, off) in &items {
            while next_num < *num {
                let _ = writeln!(&mut s, "0000000000 00000 f \r");
                next_num += 1;
            }
            let _ = writeln!(&mut s, "{:010} {:05} n \r", off, generation);
            next_num = num + 1;
        }
        s.push_str("trailer\n");
        let mut trailer_str = String::new();
        // 经典 xref 表的 trailer 必须是干净的经典字典（只有 /Root /Size，
        // 如有加密加 /Encrypt）。不能直接 clone doc.trailer.dict——
        // 对于 XRef 流源 PDF，doc.trailer.dict 本身就是 /Type /XRef /W ...
        // 的 XRef 流字典，不能原样当经典 trailer 输出。
        let mut tdict = Dict::new()
            .insert("Root", doc.trailer.root_value().clone())
            .insert("Size", Value::Integer(size));
        // 如果源文档有 /Encrypt（加密），透传到经典 trailer。
        for key in ["Encrypt", "Info", "ID"] {
            if let Some(value) = doc.trailer.dict.get(key) {
                tdict = tdict.insert(key, value.clone());
            }
        }
        write_value(&Value::Dict(tdict), &mut trailer_str);
        s.push_str(&trailer_str);
        s.push('\n');
        s.push_str("startxref\n");
        let _ = writeln!(&mut s, "{xref_offset}");
        s.push_str("%%EOF\n");
        ctx.buf.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

/// XRef 流策略（PDF 1.5+）：把交叉引用写成 `/Type /XRef` 流对象，
/// `startxref` 指向该对象偏移；不再输出经典表与 `trailer` 关键字。
///
/// 条目中：普通对象用 type 1（f2=偏移、f3=generation），打包在对象流内的
/// 对象用 type 2（f2=对象流对象号、f3=流内索引），未出现的号与 0 号用
/// type 0（自由）。
pub(super) struct XrefStreamSerializer;

impl XrefSerializer for XrefStreamSerializer {
    fn write_xref_and_tail(
        &self,
        ctx: &mut WriteContext,
        doc: &Document,
    ) -> Result<(), WriteError> {
        // XRef 流对象紧随已写出的对象之后，其偏移即当前位置。
        let xref_offset = ctx.pos;
        let xref_num = ctx.max_num + 1;
        let size = xref_num + 1;

        // 逐号构造条目三元组 (type, f2, f3)，覆盖 0..size-1（/Index [0 size]）。
        // 条目索引一次建好（O(n)），避免逐号线性查找。
        let index = ctx.entry_index();
        let mut rows: Vec<(u8, u64, u64)> = Vec::with_capacity(size as usize);
        for num in 0..=ctx.max_num {
            rows.push(match index.get(&num) {
                Some(XrefEntryOut::Normal {
                    generation, offset, ..
                }) => (1, *offset, *generation as u64),
                Some(XrefEntryOut::InObjStream {
                    stream_num, index, ..
                }) => (2, *stream_num as u64, *index as u64),
                None => (0, 0, 0),
            });
        }
        // 0 号固定为自由头条目（规范：next free 记 65535）。
        rows[0] = (0, 0, 65535);
        // XRef 流对象自身作为普通对象（type 1）计入。
        rows.push((1, xref_offset, 0));

        // /W 宽度按数据计算：type 固定 1 字节；f2 需容纳最大偏移
        // （≤2^32 用 4 字节，更大自动加宽）；f3 需容纳最大 generation
        // 或流内索引（至少 2 字节）。
        let w1 = 1usize;
        let w2 = field_width(rows.iter().map(|r| r.1).max().unwrap_or(0), 4);
        let w3 = field_width(rows.iter().map(|r| r.2).max().unwrap_or(0), 2);

        let mut stream: Vec<u8> = Vec::with_capacity(rows.len() * (w1 + w2 + w3));
        for (t, f2, f3) in &rows {
            push_field(&mut stream, *t as u64, w1);
            push_field(&mut stream, *f2, w2);
            push_field(&mut stream, *f3, w3);
        }

        let ints = |vals: &[i64]| -> Value {
            Value::Array(
                vals.iter()
                    .fold(Array::new(), |a, v| a.push(Value::Integer(*v))),
            )
        };
        // /Size 由实际写出的对象号决定（含 XRef 流自身）；trailer 的其余
        // 字段（/Root /Info /ID /Encrypt 等）原样并入该字典。
        // /Prev 例外：它指向源文件上一版本的偏移，全量重写输出自包含，
        // 保留会产生悬空/误触发的 /Prev 链。
        let mut dict = doc.trailer.dict.clone();
        dict.0.remove("Prev");
        let dict = dict
            .insert("Type", Value::Name("XRef".into()))
            .insert("Size", Value::Integer(size))
            .insert("W", ints(&[w1 as i64, w2 as i64, w3 as i64]))
            .insert("Index", ints(&[0, size]))
            .insert("Length", Value::Integer(stream.len() as i64));
        let obj = Object::new(xref_num, 0, Value::Stream(dict, StreamData::Owned(stream)));
        // 该对象的条目已在上面的 rows 中手工构造（偏移即 xref_offset），
        // 此处只写字节。
        ctx.write_raw_object(doc, &obj)?;

        let mut s = String::new();
        s.push_str("startxref\n");
        let _ = writeln!(&mut s, "{xref_offset}");
        s.push_str("%%EOF\n");
        ctx.buf.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

/// 对象打包策略：决定哪些对象作为顶层对象、哪些打包进对象流（ObjStm）。
pub(super) trait ObjectLayout {
    /// 返回 `(顶层对象, 打包对象)`。
    fn plan<'a>(&self, objs: &'a [Object]) -> (Vec<&'a Object>, Vec<&'a Object>);
}

/// 朴素布局（≤1.4）：所有对象各自顶层写出，不使用对象流。
pub(super) struct PlainObjectLayout;

impl ObjectLayout for PlainObjectLayout {
    fn plan<'a>(&self, objs: &'a [Object]) -> (Vec<&'a Object>, Vec<&'a Object>) {
        (objs.iter().collect(), Vec::new())
    }
}

/// 对象流布局（≥1.5）：**非流对象**打包进一个 ObjStm；流对象（内容流、
/// 图片 XObject 等）仍是顶层对象（规范禁止把流对象放进对象流）。
pub(super) struct ObjectStreamLayout;

impl ObjectLayout for ObjectStreamLayout {
    fn plan<'a>(&self, objs: &'a [Object]) -> (Vec<&'a Object>, Vec<&'a Object>) {
        let mut top = Vec::new();
        let mut packed = Vec::new();
        for o in objs {
            if matches!(o.value, Value::Stream(_, _)) {
                top.push(o);
            } else {
                packed.push(o);
            }
        }
        (top, packed)
    }
}

/// 计算容纳 `max` 所需的最小字节宽度（大端），不小于 `min`，上限 8。
pub(super) fn field_width(max: u64, min: usize) -> usize {
    let mut w = min.max(1);
    while w < 8 && max >= (1u64 << (8 * w)) {
        w += 1;
    }
    w
}

/// 以 `width` 字节大端把 `v` 写入 `out`。
pub(super) fn push_field(out: &mut Vec<u8>, v: u64, width: usize) {
    for i in (0..width).rev() {
        out.push(((v >> (8 * i)) & 0xff) as u8);
    }
}
