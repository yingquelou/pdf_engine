//! PDF 值、字典、流与文件结构（xref/trailer）类型。

use std::collections::BTreeMap;

/// PDF 字典：键为 `Name`（已去掉前导 `/`），值为任意 PDF 对象。
///
/// 使用 `BTreeMap` 而非 `HashMap`，使输出顺序稳定，方便
/// 序列化与逐字节比对。
#[derive(Debug, Clone, PartialEq)]
pub struct Dict(pub BTreeMap<String, Value>);

impl Dict {
    /// 创建空字典。
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// 插入 `(name, value)`，返回自身以便链式调用。
    pub fn insert(mut self, name: impl Into<String>, value: Value) -> Self {
        self.0.insert(name.into(), value);
        self
    }

    /// 不可变引用方式取值。
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.0.get(name)
    }
}

impl Default for Dict {
    fn default() -> Self {
        Self::new()
    }
}

/// PDF 数组：有序的对象序列。
#[derive(Debug, Clone, PartialEq)]
pub struct Array(pub Vec<Value>);

impl Array {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn push(mut self, value: Value) -> Self {
        self.0.push(value);
        self
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Value> {
        self.0.iter()
    }
}

impl Default for Array {
    fn default() -> Self {
        Self::new()
    }
}

/// 流数据来源。
///
/// 避免把大流的字节直接装进每个 [`Object`]（大 PDF 的内容流会
/// 全量驻留内存）。读端用 [`StreamData::Ref`] 记录偏移 + 长度，
/// 指向 `Document::source` 的共享字节；写端构造或测试用
/// [`StreamData::Owned`] 内联字节。
///
/// 取实际字节统一走 `Document::stream_bytes`。
#[derive(Debug, Clone, PartialEq)]
pub enum StreamData {
    /// 引用 `Document::source` 的切片：偏移 + 长度。
    /// 不拷贝字节，仅占 16 字节。
    Ref { offset: u64, len: u64 },
    /// Encrypted stream bytes tied to the containing indirect object.
    EncryptedRef {
        offset: u64,
        len: u64,
        object_num: i64,
        generation: i64,
    },
    /// 内联字节（写端构造、测试、小流）。
    Owned(Vec<u8>),
}

/// PDF 对象：八种基础类型 + 流。
///
/// 注意：`Name` 已去掉前导 `/`，`String` 已去掉包裹的 `()`/`<>`，
/// 由 [`crate::pdf::reader`] 负责规整；[`crate::pdf::writer`] 在
/// 序列化时重新补回。
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// 实数。PDF 没有专门的整数/浮点区分，统一按 f64 存放。
    Float(f64),
    /// 整数。
    Integer(i64),
    /// 字面字符串（已去掉 `()` 或 `<>` 包裹）。
    String(String),
    /// 16 进制字符串的原始字符（已去掉 `<>`）。
    HexString(String),
    /// ASCII85 编码字符串的原始字符（已去掉 `<~` `~>`）。
    Ascii85String(String),
    /// Name 对象，已去掉前导 `/`。
    Name(String),
    /// 字典。
    Dict(Dict),
    /// 数组。
    Array(Array),
    /// 布尔。
    Bool(bool),
    /// 间接引用 `num gen R`，存 `(num, gen)`。
    Ref(i64, i64),
    /// 流：`(字典, 数据来源)`。读端用 [`StreamData::Ref`] 偏移
    /// 引用 `Document::source`，不拷贝字节；写端用
    /// [`StreamData::Owned`] 内联。取字节统一走
    /// `Document::stream_bytes`。
    Stream(Dict, StreamData),
    /// null。
    Null,
}

impl Value {
    /// 类型名称，用于错误信息。
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Float(_) => "float",
            Value::Integer(_) => "integer",
            Value::String(_) => "string",
            Value::HexString(_) => "hex-string",
            Value::Ascii85String(_) => "ascii85-string",
            Value::Name(_) => "name",
            Value::Dict(_) => "dict",
            Value::Array(_) => "array",
            Value::Bool(_) => "bool",
            Value::Ref(_, _) => "ref",
            Value::Stream(_, _) => "stream",
            Value::Null => "null",
        }
    }

    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Value::Integer(v) => Some(*v),
            Value::Float(v) => Some(*v as i64),
            _ => None,
        }
    }

    pub fn as_name(&self) -> Option<&str> {
        match self {
            Value::Name(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_dict(&self) -> Option<&Dict> {
        match self {
            Value::Dict(d) | Value::Stream(d, _) => Some(d),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Array> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_ref(&self) -> Option<(i64, i64)> {
        match self {
            Value::Ref(n, g) => Some((*n, *g)),
            _ => None,
        }
    }
}

/// 间接对象：`num gen obj <value> endobj`。
///
/// 字段 `generation` 对应 PDF 中的 generation number。
#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    pub num: i64,
    pub generation: i64,
    pub value: Value,
}

impl Object {
    pub fn new(num: i64, generation: i64, value: Value) -> Self {
        Self {
            num,
            generation,
            value,
        }
    }
}

/// `startxref` 偏移。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StartXref(pub i64);

/// xref 表项的种类，决定对象如何定位。
///
/// 取代早期把「对象位于对象流内」硬塞进 `offset`/`generation` 的做法：
/// 那时 XRef 流 type=2 条目的 `offset` 记对象流号、`generation` 记流内
/// 索引、`in_use=false`，导致对象永远不可取。现在用显式枚举表达，语义
/// 不再与「文件偏移」复用。
#[derive(Debug, Clone, PartialEq)]
pub enum XrefKind {
    /// 自由条目（经典表 `f`，或 XRef 流 type 0）。
    Free,
    /// 普通在用条目：对象可按 [`XrefEntry::offset`] 直接定位。
    Normal,
    /// 对象打包在对象流（ObjStm）内（XRef 流 type 2）：
    /// `stream_num` 为对象流对象号，`index` 为对象在流内的索引。
    InObjStream { stream_num: i64, index: i64 },
}

/// 单条 xref 表项：`(offset, generation, kind)`。
///
/// `offset`/`generation` 仅在 [`XrefKind::Normal`]（以及自由条目记载的
/// 下一空闲对象号）下有实际含义；[`XrefKind::InObjStream`] 条目没有文件
/// 偏移，这两字段为占位 0，真正的位置由 `kind` 携带的
/// `stream_num`/`index` 表达。
#[derive(Debug, Clone, PartialEq)]
pub struct XrefEntry {
    pub offset: i64,
    pub generation: i64,
    /// 条目种类。
    pub kind: XrefKind,
}

impl XrefEntry {
    /// 是否在用（非自由条目）。由 [`XrefEntry::kind`] 派生，
    /// 不额外存字段，避免与 `kind` 两套语义并存。
    pub fn in_use(&self) -> bool {
        self.kind != XrefKind::Free
    }
}

/// xref 子段：`start count <entries...>`。
#[derive(Debug, Clone, PartialEq)]
pub struct SubXref {
    pub start: i64,
    pub entries: Vec<XrefEntry>,
}

impl SubXref {
    pub fn new(start: i64, entries: Vec<XrefEntry>) -> Self {
        Self { start, entries }
    }

    /// 子段覆盖的最大对象号（不含）。
    pub fn end(&self) -> i64 {
        self.start + self.entries.len() as i64
    }
}

/// 交叉引用表：若干子段。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Xref(pub Vec<SubXref>);

impl Xref {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// 按对象号查找条目。
    pub fn get(&self, num: i64) -> Option<&XrefEntry> {
        for sub in &self.0 {
            if (sub.start..sub.end()).contains(&num) {
                let idx = (num - sub.start) as usize;
                return sub.entries.get(idx);
            }
        }
        None
    }

    /// 全表最大对象号 + 1，即 Size 字段应取的值。
    pub fn max_size(&self) -> i64 {
        self.0.iter().map(|s| s.end()).max().unwrap_or(0)
    }
}

/// trailer 字典。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Trailer {
    pub dict: Dict,
}

impl Trailer {
    pub fn new() -> Self {
        Self::default()
    }

    /// 取 `/Root` 间接引用（对象号 + generation）。
    pub fn root(&self) -> Option<(i64, i64)> {
        self.dict.get("Root").and_then(|v| v.as_ref())
    }

    /// 取 `/Root` 的原始 Value（用于构造新的 trailer 字典）。
    pub fn root_value(&self) -> Value {
        self.dict.get("Root").cloned().unwrap_or(Value::Ref(1, 0))
    }

    /// 取 `/Size`。
    pub fn size(&self) -> Option<i64> {
        self.dict.get("Size").and_then(|v| v.as_integer())
    }
}
