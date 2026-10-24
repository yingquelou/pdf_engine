//! Type 3 字体与字形描述。

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
/// [`PdfBuilder::text`](super::super::PdfBuilder::text) 即可按文字写出。
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

    /// 追加一个字形（链式构造）。
    pub fn glyph(mut self, glyph: Type3Glyph) -> Self {
        self.glyphs.push(glyph);
        self
    }
}
