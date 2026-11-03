//! 写出阶段的内部状态：缓冲、偏移、交叉引用条目与对象流打包。

use crate::core::types::{Dict, Document, Object, PdfVersion, StreamData, Value};
use std::collections::HashMap;
use std::fmt::Write as _;

use super::WriteError;
use super::compression::StreamCompression;
use super::value::{write_object_maybe_compressed, write_value};

/// 单个对象在输出文件中的交叉引用条目（writer 内部表达）。
#[derive(Clone)]
pub(super) enum XrefEntryOut {
    /// 顶层对象：可按文件偏移直接定位（XRef 流 type 1 / 经典表 `n`）。
    Normal {
        num: i64,
        generation: i64,
        offset: u64,
    },
    /// 打包在对象流（ObjStm）内（XRef 流 type 2）：`index` 为流内索引。
    InObjStream {
        num: i64,
        stream_num: i64,
        index: i64,
    },
}

impl XrefEntryOut {
    fn num(&self) -> i64 {
        match self {
            XrefEntryOut::Normal { num, .. } | XrefEntryOut::InObjStream { num, .. } => *num,
        }
    }
}

/// 写入阶段的内部状态：缓冲 + 当前偏移 + 对象条目表 + 已写出的最大对象号。
pub(super) struct WriteContext {
    version: PdfVersion,
    /// D4 流压缩策略；`None` 时全部流保持明文。
    compression: Option<StreamCompression>,
    pub(super) buf: Vec<u8>,
    pub(super) pos: u64,
    /// 已写出对象的交叉引用条目（不保证按对象号有序：打包条目在对象流
    /// 写完后追加）。
    pub(super) entries: Vec<XrefEntryOut>,
    /// 已写出的最大对象号（用于分配新对象号与 /Size 计算）。
    pub(super) max_num: i64,
}

impl WriteContext {
    pub(super) fn new(version: PdfVersion, compression: Option<StreamCompression>) -> Self {
        Self {
            version,
            compression,
            buf: Vec::new(),
            pos: 0,
            entries: Vec::new(),
            max_num: 0,
        }
    }

    /// 增量更新上下文：缓冲已含源文件字节，写出从文件尾开始；
    /// `base_max_num` 为源文档已占用的最大对象号，新对象号在其后分配。
    pub(super) fn for_incremental(
        version: PdfVersion,
        compression: Option<StreamCompression>,
        buf: Vec<u8>,
        base_max_num: i64,
    ) -> Self {
        let pos = buf.len() as u64;
        Self {
            version,
            compression,
            buf,
            pos,
            entries: Vec::new(),
            max_num: base_max_num.max(0),
        }
    }

    pub(super) fn write_header(&mut self) {
        self.buf.extend_from_slice(b"%PDF-");
        self.buf.extend_from_slice(self.version.as_str().as_bytes());
        self.buf.extend_from_slice(b"\n%\xE2\xE3\xCF\xD3\n");
        self.pos = self.buf.len() as u64;
    }

    /// 仅把对象字节写入缓冲，返回其起始偏移（不记条目、不改最大对象号）。
    pub(super) fn write_raw_object(
        &mut self,
        doc: &Document,
        obj: &Object,
    ) -> Result<u64, WriteError> {
        let bytes = write_object_maybe_compressed(obj, doc, self.compression.as_ref())?;
        let off = self.pos;
        self.buf.extend_from_slice(&bytes);
        self.pos = self.buf.len() as u64;
        Ok(off)
    }

    /// 写一个顶层对象并记录其 `Normal` 条目。
    pub(super) fn write_top_level_object(
        &mut self,
        doc: &Document,
        obj: &Object,
    ) -> Result<(), WriteError> {
        let off = self.write_raw_object(doc, obj)?;
        self.entries.push(XrefEntryOut::Normal {
            num: obj.num,
            generation: obj.generation,
            offset: off,
        });
        self.max_num = self.max_num.max(obj.num);
        Ok(())
    }

    pub(super) fn write_top_level_objects(
        &mut self,
        doc: &Document,
        objs: &[&Object],
    ) -> Result<(), WriteError> {
        for o in objs {
            self.write_top_level_object(doc, o)?;
        }
        Ok(())
    }

    /// 预登记文档内所有对象号的上界。打包对象的号虽不在顶层写出，但同样
    /// 占用号空间，故对象流/XRef 流新分配的对象号必须避开它们。
    pub(super) fn reserve_object_numbers(&mut self, objs: &[Object]) {
        self.max_num = objs
            .iter()
            .map(|o| o.num)
            .max()
            .unwrap_or(0)
            .max(self.max_num);
    }

    /// 下一个可用对象号。
    pub(super) fn next_object_number(&self) -> i64 {
        self.max_num + 1
    }

    /// 按对象号索引的条目表。XRef 流要为 `0..=max_num` 逐个取条目，
    /// 若逐号线性查找则整体退化为 O(n²)，故一次性建成索引。
    pub(super) fn entry_index(&self) -> HashMap<i64, XrefEntryOut> {
        self.entries.iter().cloned().map(|e| (e.num(), e)).collect()
    }

    /// 把非流对象打包写出为一个对象流（ObjStm，`/Type /ObjStm`）对象，
    /// 并为每个被打包的对象记录 `InObjStream` 条目。
    ///
    /// **7.5.7 咽喉点守卫**：进入这里的对象必须都是非流对象——规范
    /// 「The following objects shall not be stored in an object stream:
    /// ▪ Stream objects」（`docs/PDF32000_2008.pdf`，`pdftotext -raw` 第
    /// 2240–2241 行）。打包只走 [`super::value::write_value`]，它对
    /// `Value::Stream` **只输出字典、不含 `stream` 正文**，所以一旦让流
    /// 对象进来就会静默丢内容。这里在任何字节落地前返回
    /// [`WriteError::StreamInObjectStream`]；正常的选择路径
    /// （[`super::serialize::ObjectStreamLayout`]）本就排除流对象，本守卫
    /// 同时护住 hybrid/incremental 等其它调用方。
    ///
    /// 流内容：头部 N 对「对象号 相对偏移」（ASCII、空白分隔），随后
    /// 各对象本体；`/First` 为首个对象本体在流内的偏移。默认**不压缩**；
    /// 启用 D4 压缩策略时，容器流与文档流一样在序列化咽喉点压缩。
    pub(super) fn write_object_stream(
        &mut self,
        doc: &Document,
        packed: &[&Object],
        stream_num: i64,
    ) -> Result<(), WriteError> {
        if let Some(offender) = packed.iter().find(|o| matches!(o.value, Value::Stream(..))) {
            return Err(WriteError::StreamInObjectStream(offender.num));
        }
        let mut header = String::new();
        let mut body = String::new();
        for o in packed {
            let _ = write!(header, "{} {} ", o.num, body.len());
            let mut s = String::new();
            write_value(&o.value, &mut s);
            body.push_str(&s);
            body.push(' ');
        }
        let first = header.len() as i64;
        let mut content = header.into_bytes();
        content.extend_from_slice(body.as_bytes());

        let dict = Dict::new()
            .insert("Type", Value::Name("ObjStm".into()))
            .insert("N", Value::Integer(packed.len() as i64))
            .insert("First", Value::Integer(first))
            .insert("Length", Value::Integer(content.len() as i64));
        let obj = Object::new(
            stream_num,
            0,
            Value::Stream(dict, StreamData::Owned(content)),
        );
        self.write_top_level_object(doc, &obj)?;

        for (i, o) in packed.iter().enumerate() {
            self.entries.push(XrefEntryOut::InObjStream {
                num: o.num,
                stream_num,
                index: i as i64,
            });
        }
        Ok(())
    }
}
