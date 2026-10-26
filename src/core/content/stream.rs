//! `ContentStream` 容器与链式高层绘制 API。

use super::types::{
    LineCap, LineJoin, MarkedContentProperties, Op, OptionalContent, TextArrayItem,
};

/// 内容流：操作符序列容器 + 高层绘制 API + 字节序列化。
#[derive(Debug, Clone, Default)]
pub struct ContentStream {
    ops: Vec<Op>,
}

impl ContentStream {
    /// 新建空内容流。
    pub fn new() -> Self {
        Self::default()
    }

    /// 推入一个原始操作符。
    pub fn push(&mut self, op: Op) {
        self.ops.push(op);
    }

    /// 当前已推入操作符数量。
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// 操作符序列是否为空。
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// 把操作符序列序列化为 PDF 内容流字节。
    ///
    /// 每个操作符输出其参数（经 `format_float` 格式化）+ 操作符关键字，
    /// 后跟一个换行。
    pub fn to_bytes(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        for op in self.ops.iter() {
            Self::serialize_one(op, &mut out);
        }
        out
    }

    // ── 链式高层绘制 API ────────────────────────────

    /// 开始文本块：`BT`。
    pub fn begin_text(&mut self) -> &mut Self {
        self.push(Op::BeginText);
        self
    }

    /// 结束文本块：`ET`。
    pub fn end_text(&mut self) -> &mut Self {
        self.push(Op::EndText);
        self
    }

    /// 设置当前字体：`/name size Tf`。
    pub fn set_font(&mut self, name: impl Into<String>, size: f64) -> &mut Self {
        self.push(Op::SetFont {
            name: name.into(),
            size,
        });
        self
    }

    /// 在绝对坐标 `(x, y)` 处绘制字符串文本——**不包 BT/ET**。
    ///
    /// 仅输出 `x y Td (text) Tj`；调用方需保证已在文本块内（或用
    /// [`Self::show_text_at`] 这种一体式便捷方法）。
    pub fn show_text_raw(&mut self, x: f64, y: f64, text: impl Into<String>) -> &mut Self {
        self.push(Op::MoveNextLine { tx: x, ty: y });
        self.push(Op::ShowText(text.into()));
        self
    }

    /// 绘制字符串文本：`(text) Tj`——**不包 BT/ET 或移动**。
    pub fn show_text(&mut self, text: impl Into<String>) -> &mut Self {
        self.push(Op::ShowText(text.into()));
        self
    }

    /// 以十六进制字符串绘制文本：`<...> Tj`——**不包 BT/ET 或移动**。
    ///
    /// 用于 Type 0/CID 等复合字体的多字节字符码。
    pub fn show_text_hex(&mut self, bytes: impl Into<Vec<u8>>) -> &mut Self {
        self.push(Op::ShowTextHex(bytes.into()));
        self
    }

    /// 最常用一体式便捷方法：`BT /name size Tf x y Td (text) Tj ET`。
    ///
    /// 覆盖当前 `PdfBuilder::text` 的输出模式。
    pub fn show_text_at(
        &mut self,
        font_name: impl Into<String>,
        font_size: f64,
        x: f64,
        y: f64,
        text: impl Into<String>,
    ) -> &mut Self {
        let text = text.into();
        self.begin_text();
        self.set_font(font_name, font_size);
        self.show_text_raw(x, y, text);
        self.end_text();
        self
    }

    /// 文本数组绘制：`[... ] TJ`——**不包 BT/ET**。
    pub fn show_text_array(&mut self, items: Vec<TextArrayItem>) -> &mut Self {
        self.push(Op::ShowTextArray(items));
        self
    }

    /// 换行并绘制文本：`T* (text) '`——**不包 BT/ET**。
    ///
    /// 等效于先调用 [`Self::next_line`]，再调用 [`Self::show_text`]。
    pub fn show_text_next_line(&mut self, text: impl Into<String>) -> &mut Self {
        self.push(Op::ShowTextNextLine(text.into()));
        self
    }

    /// 设置词距/字距、换行并绘制文本：`aw ac (text) "`——**不包 BT/ET**。
    ///
    /// 等效于 `Tw Tc T* Tj` 的组合。
    pub fn set_spacing_show_text_next_line(
        &mut self,
        word_spacing: f64,
        char_spacing: f64,
        text: impl Into<String>,
    ) -> &mut Self {
        self.push(Op::SetSpacingShowTextNextLine {
            word_spacing,
            char_spacing,
            text: text.into(),
        });
        self
    }

    /// 文本块内水平/垂直移动：`tx ty Td`。
    pub fn move_next_line(&mut self, tx: f64, ty: f64) -> &mut Self {
        self.push(Op::MoveNextLine { tx, ty });
        self
    }

    /// 保存图形状态：`q`。
    pub fn save_state(&mut self) -> &mut Self {
        self.push(Op::SaveState);
        self
    }

    /// 恢复图形状态：`Q`。
    pub fn restore_state(&mut self) -> &mut Self {
        self.push(Op::RestoreState);
        self
    }

    /// 串联矩阵：`a b c d e f cm`。
    pub fn concat_matrix(&mut self, m: [f64; 6]) -> &mut Self {
        self.push(Op::ConcatMatrix(m));
        self
    }

    /// 绘制外部对象：`/name Do`。
    pub fn draw_xobject(&mut self, name: impl Into<String>) -> &mut Self {
        self.push(Op::DrawXObject(name.into()));
        self
    }

    /// 用命名渐变填充当前裁剪区域：`/name sh`。
    pub fn paint_shading(&mut self, name: impl Into<String>) -> &mut Self {
        self.push(Op::PaintShading(name.into()));
        self
    }

    /// 绘制直线：`x1 y1 m x2 y2 l S`。
    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) -> &mut Self {
        self.push(Op::MoveTo { x: x1, y: y1 });
        self.push(Op::LineTo { x: x2, y: y2 });
        self.push(Op::Stroke);
        self
    }

    /// 画矩形路径（**只加操作符**，不自动绘制）：`x y w h re`。
    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64) -> &mut Self {
        self.push(Op::Rectangle { x, y, w, h });
        self
    }

    /// 描边当前路径：`S`。
    pub fn stroke(&mut self) -> &mut Self {
        self.push(Op::Stroke);
        self
    }

    /// 填充当前路径（非零环绕）：`f`。
    pub fn fill(&mut self) -> &mut Self {
        self.push(Op::Fill);
        self
    }

    /// 填充当前路径（even-odd）：`f*`。
    pub fn fill_even_odd(&mut self) -> &mut Self {
        self.push(Op::FillEvenOdd);
        self
    }

    /// 填充并描边当前路径：`B`。
    pub fn fill_stroke(&mut self) -> &mut Self {
        self.push(Op::FillStroke);
        self
    }

    /// 填充并描边（even-odd）：`B*`。
    pub fn fill_stroke_even_odd(&mut self) -> &mut Self {
        self.push(Op::FillStrokeEvenOdd);
        self
    }

    /// 设置描边颜色（灰度）：`gray G`。
    pub fn set_stroke_gray(&mut self, gray: f64) -> &mut Self {
        self.push(Op::SetStrokeGray { gray });
        self
    }

    /// 设置填充颜色（灰度）：`gray g`。
    pub fn set_fill_gray(&mut self, gray: f64) -> &mut Self {
        self.push(Op::SetFillGray { gray });
        self
    }

    /// 设置描边颜色（RGB）：`r g b RG`。
    pub fn set_stroke_rgb(&mut self, r: f64, g: f64, b: f64) -> &mut Self {
        self.push(Op::SetStrokeRGB { r, g, b });
        self
    }

    /// 设置填充颜色（RGB）：`r g b rg`。
    pub fn set_fill_rgb(&mut self, r: f64, g: f64, b: f64) -> &mut Self {
        self.push(Op::SetFillRGB { r, g, b });
        self
    }

    /// 设置描边颜色（CMYK）：`c m y k K`。
    pub fn set_stroke_cmyk(&mut self, c: f64, m: f64, y: f64, k: f64) -> &mut Self {
        self.push(Op::SetStrokeCMYK { c, m, y, k });
        self
    }

    /// 设置填充颜色（CMYK）：`c m y k k`。
    pub fn set_fill_cmyk(&mut self, c: f64, m: f64, y: f64, k: f64) -> &mut Self {
        self.push(Op::SetFillCMYK { c, m, y, k });
        self
    }

    /// 设置描边颜色空间（命名资源）：`/name CS`。
    pub fn set_stroke_color_space(&mut self, name: impl Into<String>) -> &mut Self {
        self.push(Op::SetStrokeColorSpace(name.into()));
        self
    }

    /// 设置填充颜色空间（命名资源）：`/name cs`。
    pub fn set_fill_color_space(&mut self, name: impl Into<String>) -> &mut Self {
        self.push(Op::SetFillColorSpace(name.into()));
        self
    }

    /// 以命名图案设置描边色：`/name SCN`。
    pub fn set_stroke_pattern(&mut self, name: impl Into<String>) -> &mut Self {
        self.push(Op::SetStrokeColorPattern(name.into()));
        self
    }

    /// 以命名图案设置填充色：`/name scn`。
    pub fn set_fill_pattern(&mut self, name: impl Into<String>) -> &mut Self {
        self.push(Op::SetFillColorPattern(name.into()));
        self
    }

    /// 以无色图案设置描边色：`c1 … cn /name SCN`。
    pub fn set_stroke_pattern_with_components(
        &mut self,
        values: impl Into<Vec<f64>>,
        name: impl Into<String>,
    ) -> &mut Self {
        self.push(Op::SetStrokeColorPatternWithComponents {
            values: values.into(),
            name: name.into(),
        });
        self
    }

    /// 以无色图案设置填充色：`c1 … cn /name scn`。
    pub fn set_fill_pattern_with_components(
        &mut self,
        values: impl Into<Vec<f64>>,
        name: impl Into<String>,
    ) -> &mut Self {
        self.push(Op::SetFillColorPatternWithComponents {
            values: values.into(),
            name: name.into(),
        });
        self
    }

    /// 设置线宽：`w w`。
    pub fn set_line_width(&mut self, w: f64) -> &mut Self {
        self.push(Op::SetLineWidth(w));
        self
    }

    /// 设置线帽：`J n`。
    pub fn set_line_cap(&mut self, cap: LineCap) -> &mut Self {
        self.push(Op::SetLineCap(cap));
        self
    }

    /// 设置折线连接：`j n`。
    pub fn set_line_join(&mut self, join: LineJoin) -> &mut Self {
        self.push(Op::SetLineJoin(join));
        self
    }

    /// 设置 miter 限制：`M m`。
    pub fn set_miter_limit(&mut self, m: f64) -> &mut Self {
        self.push(Op::SetMiterLimit(m));
        self
    }

    /// 设置虚线模式：`[a b ...] phase d`。
    pub fn set_dash(&mut self, array: Vec<f64>, phase: f64) -> &mut Self {
        self.push(Op::SetDashPattern { array, phase });
        self
    }

    /// 应用图形状态参数字典：`/name gs`（PDF 8.4.4）。
    pub fn set_ext_g_state(&mut self, name: impl Into<String>) -> &mut Self {
        self.push(Op::SetExtGState(name.into()));
        self
    }

    /// 设置平坦度容差：`i flatness`（PDF 8.4.5），值域 0–100。
    pub fn set_flatness(&mut self, flatness: f64) -> &mut Self {
        self.push(Op::SetFlatness(flatness));
        self
    }

    /// 开始标记内容序列：`/tag BMC`（PDF 14.6）。
    pub fn begin_marked_content(&mut self, tag: impl Into<String>) -> &mut Self {
        self.push(Op::BeginMarkedContent { tag: tag.into() });
        self
    }

    /// 开始带属性表的标记内容序列：`/tag properties BDC`（PDF 14.6）。
    pub fn begin_marked_content_properties(
        &mut self,
        tag: impl Into<String>,
        properties: MarkedContentProperties,
    ) -> &mut Self {
        self.push(Op::BeginMarkedContentProperties {
            tag: tag.into(),
            properties,
        });
        self
    }

    /// 标记内容点：`/tag MP`（PDF 14.6）。
    pub fn marked_content_point(&mut self, tag: impl Into<String>) -> &mut Self {
        self.push(Op::MarkedContentPoint { tag: tag.into() });
        self
    }

    /// 带属性表的标记内容点：`/tag properties DP`（PDF 14.6）。
    pub fn marked_content_point_properties(
        &mut self,
        tag: impl Into<String>,
        properties: MarkedContentProperties,
    ) -> &mut Self {
        self.push(Op::MarkedContentPointProperties {
            tag: tag.into(),
            properties,
        });
        self
    }

    /// 结束当前标记内容序列：`EMC`（PDF 14.6）。
    pub fn end_marked_content(&mut self) -> &mut Self {
        self.push(Op::EndMarkedContent);
        self
    }

    /// 按 [`OptionalContent`] 简化表示输出可选内容标记：
    /// `Begin` → `/OC /name BDC`，`PropertyDict` → `tag /name DP`，
    /// `End` → `EMC`。
    pub fn optional_content(&mut self, content: OptionalContent) -> &mut Self {
        self.push(content.into());
        self
    }
}
