//! Type 3 字体与字形描述。

use std::collections::BTreeMap;

use crate::core::types::Value;

/// Type 3 字体的单个字形（字符过程）。
///
/// 对应 `/CharProcs` 字典中的一个条目：`name` 是字形程序名（也是
/// `/Encoding` `/Differences` 中的名字），`char_proc` 是字符过程内容流
/// 的字节。`width` 是 `/Widths` 中的字形空间宽度（PDF 32000-1 §9.6.5），
/// 通常与字符过程内的 `d0`/`d1` 声明一致。
#[derive(Debug, Clone, PartialEq)]
pub struct Type3Glyph {
    /// 单字节字符码。
    pub code: u8,
    /// 字形程序名（`/CharProcs` 键）。
    pub name: String,
    /// 字形空间宽度（`/Widths` 条目）。
    pub width: f64,
    /// 字符过程内容流字节（通常以 `d0` 或 `d1` 声明宽度与边界）。
    pub char_proc: Vec<u8>,
}

impl Type3Glyph {
    /// 创建字形；`char_proc` 接受字节切片或其所有者。
    pub fn new(code: u8, name: impl Into<String>, width: f64, char_proc: impl AsRef<[u8]>) -> Self {
        Self {
            code,
            name: name.into(),
            width,
            char_proc: char_proc.as_ref().to_vec(),
        }
    }
}

/// Type 3 字体逻辑描述（PDF 32000-1 §9.6.5）。
///
/// Type 3 字体用字符过程内容流绘制字形，不嵌入字体程序：
/// `/CharProcs` 按 [`Type3Glyph::name`] 引用各字形流，`/Widths` 按字符码
/// 排列字形空间宽度，`/Encoding` 的 `/Differences` 给出字符码到字形名
/// 的映射。通过 [`Font::type3`](super::Font::type3) 包装为 [`Font`] 后交给
/// [`PdfBuilder::text`](super::super::PdfBuilder::text) 即可按文字写出；
/// 可选的 `/Name`、`/ToUnicode`、`/Resources` 由本结构设置，
/// `/FontDescriptor` 来自 [`Font::with_descriptor`](super::Font::with_descriptor)。
///
/// 默认 `/FontBBox` 为 `[0 0 1000 1000]`、`/FontMatrix` 为
/// `[0.001 0 0 0.001 0 0]`。
#[derive(Debug, Clone, PartialEq)]
pub struct Type3Font {
    /// `/FontBBox`：字形空间边界框。
    pub font_bbox: [f64; 4],
    /// `/FontMatrix`：字形空间到文本空间的变换矩阵。
    pub font_matrix: [f64; 6],
    /// 字形列表（写出时按此顺序分配 `/CharProcs` 流对象号）。
    pub glyphs: Vec<Type3Glyph>,
    /// 可选 `/Name`（PDF 32000-1 Table 112；PDF 1.0 必需、后续版本可选）。
    ///
    /// 未设置时不写该条目；设置时必须是不含分隔符与 `#` 的非空名称。
    pub name: Option<String>,
    /// 字符码 → Unicode 文本的 `/ToUnicode` 映射（Table 112 可选）。
    ///
    /// 非空时写出独立的 ToUnicode CMap 流并挂到字体字典 `/ToUnicode`；
    /// 同一字符码后写覆盖前值。
    pub to_unicode: BTreeMap<u8, String>,
    /// 可选的 `/Resources`（PDF 32000-1 Table 112）。
    ///
    /// 字形程序是内容流，可引用这里的资源（如 `/XObject`、`/ColorSpace`、
    /// `/Pattern`）。构建时原样写出：可以直接传字典，也可以传指向字典的
    /// 间接引用（`Value::Ref`）；其他 `Value` 类型在构建时报
    /// `WriteError::InvalidObject`。间接引用不会被自动注册，其对象号
    /// 有效性由调用方自行保证（builder 的对象号在构建期分配）。
    pub resources: Option<Value>,
}

impl Default for Type3Font {
    fn default() -> Self {
        Self::new()
    }
}

impl Type3Font {
    /// 创建空字体：默认 bbox 与 0.001 缩放矩阵，无字形。
    pub fn new() -> Self {
        Self {
            font_bbox: [0.0, 0.0, 1000.0, 1000.0],
            font_matrix: [0.001, 0.0, 0.0, 0.001, 0.0, 0.0],
            glyphs: Vec::new(),
            name: None,
            to_unicode: BTreeMap::new(),
            resources: None,
        }
    }

    /// 设置 `/FontBBox`（链式构造）。
    pub fn with_bbox(mut self, bbox: [f64; 4]) -> Self {
        self.font_bbox = bbox;
        self
    }

    /// 设置 `/FontMatrix`（链式构造）。
    pub fn with_matrix(mut self, matrix: [f64; 6]) -> Self {
        self.font_matrix = matrix;
        self
    }

    /// 设置 `/Name`（链式构造）。
    ///
    /// 名称须为非空、仅可打印 ASCII 且不含分隔符与 `#`；非法名称在
    /// 构建时报 `WriteError::InvalidObject`。
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// 追加一条 `/ToUnicode` 映射（链式构造）。
    ///
    /// 同一字符码重复设置时后写覆盖前值；映射非空时构建器写出
    /// ToUnicode CMap 流。
    pub fn with_to_unicode(mut self, code: u8, text: impl Into<String>) -> Self {
        self.to_unicode.insert(code, text.into());
        self
    }

    /// 追加一个字形（链式构造）。
    pub fn glyph(mut self, glyph: Type3Glyph) -> Self {
        self.glyphs.push(glyph);
        self
    }

    /// 设置 `/Resources`（链式构造）。
    ///
    /// 值原样写出：可为直接字典或指向字典的间接引用；间接引用的对象号
    /// 有效性由调用方保证。未设置时不写该条目。
    pub fn with_resources(mut self, resources: Value) -> Self {
        self.resources = Some(resources);
        self
    }
}
