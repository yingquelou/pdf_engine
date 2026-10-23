//! 内容流操作符与参数类型。

/// 文本数组项（`TJ` 操作符参数）。
#[derive(Debug, Clone, PartialEq)]
pub enum TextArrayItem {
    /// 要绘制的文本字面量（引擎内部会做 `( ) \` 转义）。
    String(String),
    /// 在绘制下一项前水平方向的额外偏移（PDF 规范用负数写）。
    MoveBy(f64),
}

/// 端点线帽样式（`J` 操作符参数）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineCap {
    /// Butt 线帽（默认，值 0）。
    Butt,
    /// Round 线帽（值 1）。
    Round,
    /// Square 线帽（值 2）。
    Square,
}

impl LineCap {
    pub(super) fn value(self) -> u8 {
        match self {
            LineCap::Butt => 0,
            LineCap::Round => 1,
            LineCap::Square => 2,
        }
    }
}

/// 折线连接样式（`j` 操作符参数）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineJoin {
    /// Miter 连接（默认，值 0）。
    Miter,
    /// Round 连接（值 1）。
    Round,
    /// Bevel 连接（值 2）。
    Bevel,
}

impl LineJoin {
    pub(super) fn value(self) -> u8 {
        match self {
            LineJoin::Miter => 0,
            LineJoin::Round => 1,
            LineJoin::Bevel => 2,
        }
    }
}

/// 可选内容组（`/OC`、`/DP`）的简化表示。
#[derive(Debug, Clone, PartialEq)]
pub enum OptionalContent {
    /// `/OC /GroupName BDC ... EMC` —— 内嵌内容标记。
    ///
    /// 本引擎只记录名称，嵌套 BDC 的语义验证由调用方负责。
    Begin(String),
    /// `/DP /GroupName /DPName` —— 属性字典引用（简化形式）。
    PropertyDict(String, String),
    /// `EMC` —— 结束可选内容组。
    End,
}

/// 内容流操作符（类型化表示）。
///
/// 每个变体对应一个 PDF 操作符关键字与一组类型安全的参数。版本门控
/// 不在枚举上表达——由序列化时的 `ContentStream` 自动记录特性触发
/// （见 `ContentStream::features`）。
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    // ── 文本块（PDF 1.0） ────────────────────────────
    BeginText,
    EndText,

    // ── 文本设置（PDF 1.0） ──────────────────────────
    SetFont {
        /// 资源字典中的字体名（不含前导 `/`），如 `"F1"`。
        name: String,
        /// 字号 pt。
        size: f64,
    },
    ShowText(String),
    /// 以十六进制字符串绘制文本：`<...> Tj`。
    ///
    /// 复合字体（Type 0/CID）的字符码常为非 ASCII 的多字节值，
    /// 按十六进制形式书写。
    ShowTextHex(Vec<u8>),
    ShowTextArray(Vec<TextArrayItem>),
    MoveNextLine {
        tx: f64,
        ty: f64,
    },
    MoveNextLineSetLeading {
        leading: f64,
    },
    NextLine,
    SetTextMatrix([f64; 6]),
    SetCharSpacing(f64),
    SetWordSpacing(f64),
    SetTextScale(f64),
    SetTextLeading(f64),
    SetTextRender(u8),

    // ── 图形状态（PDF 1.0，ri 为 1.2） ────────────────
    SaveState,
    RestoreState,
    ConcatMatrix([f64; 6]),
    SetLineWidth(f64),
    SetLineCap(LineCap),
    SetLineJoin(LineJoin),
    SetMiterLimit(f64),
    SetDashPattern {
        /// 交替的实线/空白段长度数组；空数组表示实线。
        array: Vec<f64>,
        /// 图案相位 pt。
        phase: f64,
    },
    SetRenderingIntent(String),

    // ── 路径构造（PDF 1.0） ──────────────────────────
    MoveTo { x: f64, y: f64 },
    LineTo { x: f64, y: f64 },
    CurveTo {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x3: f64,
        y3: f64,
    },
    CurveToInitial {
        x2: f64,
        y2: f64,
        x3: f64,
        y3: f64,
    },
    CurveToFinal {
        x1: f64,
        y1: f64,
        x3: f64,
        y3: f64,
    },
    ClosePath,
    Rectangle {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    },

    // ── 路径绘制（PDF 1.0；f*/B*/b*/W* 为 1.2） ────────
    Stroke,
    ClosePathStroke,
    Fill,
    FillRule,
    FillEvenOdd,
    FillStroke,
    FillStrokeEvenOdd,
    ClosePathFillStroke,
    ClosePathFillStrokeEvenOdd,
    EndPath,

    // ── 裁剪（PDF 1.0；W* 为 1.2） ────────────────────
    Clip,
    ClipEvenOdd,

    // ── 基本颜色（PDF 1.0；scn/SCN 为 1.2） ────────────
    SetStrokeRGB { r: f64, g: f64, b: f64 },
    SetFillRGB { r: f64, g: f64, b: f64 },
    SetStrokeCMYK {
        c: f64,
        m: f64,
        y: f64,
        k: f64,
    },
    SetFillCMYK {
        c: f64,
        m: f64,
        y: f64,
        k: f64,
    },
    /// 设置描边颜色空间为 `/name`（Gray/RGB/CMYK 等），后续 `SC`/`sc`
    /// 提供颜色值数组。
    SetStrokeColorSpace(String),
    SetFillColorSpace(String),
    /// 用当前 `SetStrokeColorSpace` 的颜色数组设置描边颜色。
    SetStrokeColor(Vec<f64>),
    SetFillColor(Vec<f64>),
    /// `scn`：color 数组 + 可选 name 参数（pattern 分支不在本次范围内）。
    SetStrokeColorN { values: Vec<f64> },
    SetFillColorN { values: Vec<f64> },

    // ── XObject Do（PDF 1.0） ──────────────────────────
    DrawXObject(String),

    // ── 透明度（PDF 1.4；PdfFeature::Transparency） ────
    SetFillAlpha(f64),
    SetStrokeAlpha(f64),
    SetBlendMode(String),
}
