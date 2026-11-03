//! 内容流操作符与参数类型。

use crate::core::types::Dict;

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
    /// 规范数值（0/1/2）。
    pub fn value(self) -> u8 {
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
    /// 规范数值（0/1/2）。
    pub fn value(self) -> u8 {
        match self {
            LineJoin::Miter => 0,
            LineJoin::Round => 1,
            LineJoin::Bevel => 2,
        }
    }
}

/// 标记内容属性表（`BDC`/`DP` 的第二个操作数，ISO 32000-1 §14.6）。
///
/// 规范原文（`pdftotext -raw docs/PDF32000_2008.pdf -` 的输出行号）：
/// - 14.6.1 L29005–29006：`Marked-content operators (PDF 1.2)`——整族标记内容
///   操作符（含属性表操作数）的版本起点是 **PDF 1.2**；
/// - Table 320（L29061）的 `DP` 行 L29067–29069 与 `BDC` 行 L29075–29078 只列
///   **两种**操作数形态：内联字典，或当前资源字典 `/Properties` 子字典中与该
///   名称关联的条目；操作数按**前缀序**排列（L29073 `tag properties BDC`），
///   因此间接引用形态写作 `/Tag 5 0 R BDC`（标签在前、属性表在后）；
/// - 14.6.2 L29087–29091 进一步规定：属性表字典的值只要有一个是间接引用，该
///   字典 **shall** 定义为 `/Properties` 命名资源、并按名称作为 `DP`/`BDC` 的
///   properties 操作数。
///
/// 因此 [`Self::Reference`]（`/Tag 5 0 R BDC`）**不是 ISO 32000-1 授权的操作数
/// 形态**，而是对生产者实态的互操作容错/本库自有写法（部分写出器把属性表字典
/// 本身作为间接对象直接引用）：读端把它当作命名资源之外的容错解引用分支，写端
/// 由调用方显式构造，或由注册句柄通路在装配期回填对象号后产生（见
/// `PdfBuilder::add_marked_content_properties`），两种来源都不是合规写法。
#[derive(Debug, Clone, PartialEq)]
pub enum MarkedContentProperties {
    /// 属性表资源名（不含前导 `/`），如 `"MC0"`。
    Name(String),
    /// 内联属性字典，如 `<< /MCID 0 >>`。
    Dict(Dict),
    /// 间接引用属性表操作数 `obj gen R`（非 ISO 32000-1 形态，见枚举文档）。
    ///
    /// 载荷沿用仓库既有的引用表示，与 `Value::Ref(object, generation)` 同形，
    /// 不引入第二套引用类型。
    ///
    /// **对象号由调用方给定**：本层只把操作数写成 `obj gen R`，不负责在文档
    /// 里分配对象号或登记那个间接对象。
    ///
    /// 装配期回填通路（注册句柄 → 占位 → 实际引用）借用 `generation` 的保留值
    /// [`Self::PENDING_GENERATION`] 表达「句柄尚未定型」，见
    /// [`Self::pending_handle`] 与 [`Self::as_pending_handle`]；正常装配完成后
    /// 页面内容里不会剩下该形态（未替换即序列化会被类型化拒绝）。
    Reference {
        /// 属性表字典所在对象的对象号；占位形态下是注册句柄。
        object: i64,
        /// 引用中的代号（generation），惯例为 0；[`Self::PENDING_GENERATION`]
        /// 表示尚未定型的句柄。
        generation: i64,
    },
}

impl MarkedContentProperties {
    /// 「属性表句柄尚未定型」的保留代号值。
    ///
    /// PDF 的 generation 非负，`-1` 既不可能出现在真实引用里，也不可能来自
    /// 读端解析结果，因此可以安全地用作占位标记。
    ///
    /// **为什么不新增枚举变体**：只读参考层
    /// `src/core/reader/content/mapping/from_op.rs` 对 `Op` 与本枚举都做
    /// 无通配的穷尽匹配（写集之外，不可修改），添加变体会直接编译失败
    /// （E0004）。占位形态因此编码在既有 `Reference` 变体内部。
    pub const PENDING_GENERATION: i64 = -1;

    /// 由注册句柄构造「未定型占位」形态：`object` 存句柄、`generation` 存
    /// [`Self::PENDING_GENERATION`]。
    ///
    /// 占位只应在装配前存在于页面内容操作数里，装配期会把它换成实际对象号
    /// （见 `PdfBuilder::add_marked_content_properties` 与
    /// `PdfBuilder::begin_marked_properties`）。
    pub fn pending_handle(handle: usize) -> Self {
        Self::Reference {
            object: handle as i64,
            generation: Self::PENDING_GENERATION,
        }
    }

    /// 若本值是未定型占位，返回其注册句柄；否则返回 `None`。
    pub fn as_pending_handle(&self) -> Option<usize> {
        match self {
            Self::Reference {
                object,
                generation: Self::PENDING_GENERATION,
            } if *object >= 0 => Some(*object as usize),
            _ => None,
        }
    }

    /// 本操作数携带的属性表是否为未定型占位（供序列化前置检查使用）。
    pub(crate) fn is_pending_handle(&self) -> bool {
        self.as_pending_handle().is_some()
    }
}

/// 可选内容组（`/OC`、`/DP`）的简化表示。
#[derive(Debug, Clone, PartialEq)]
pub enum OptionalContent {
    /// `/OC /GroupName BDC ... EMC` —— 开始可选内容组。
    ///
    /// 本引擎只记录名称，组配置字典（`/OCProperties`）与嵌套 BDC 的
    /// 语义验证由调用方负责。
    Begin(String),
    /// `tag /PropertyName DP` —— 引用命名属性字典的标记内容点。
    PropertyDict(String, String),
    /// `EMC` —— 结束标记内容组。
    End,
}

impl From<OptionalContent> for Op {
    fn from(value: OptionalContent) -> Self {
        match value {
            OptionalContent::Begin(group) => Op::BeginMarkedContentProperties {
                tag: "OC".into(),
                properties: MarkedContentProperties::Name(group),
            },
            OptionalContent::PropertyDict(tag, property) => Op::MarkedContentPointProperties {
                tag,
                properties: MarkedContentProperties::Name(property),
            },
            OptionalContent::End => Op::EndMarkedContent,
        }
    }
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
    /// 换行并绘制文本：`T* (text) '`（PDF 9.4.5）。
    ShowTextNextLine(String),
    /// 设置词距/字距、换行并绘制文本：`aw ac (text) "`（PDF 9.4.5）。
    SetSpacingShowTextNextLine {
        /// 词距调整量 `aw`。
        word_spacing: f64,
        /// 字距调整量 `ac`。
        char_spacing: f64,
        /// 要绘制的文本。
        text: String,
    },
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
    /// 设置文本上升距离（`Ts`，PDF 9.3.7）：正值上移基线，负值下移。
    SetTextRise(f64),

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
    /// 应用图形状态参数字典（`gs`，PDF 8.4.4）：参数为资源字典中的 ExtGState 名。
    SetExtGState(String),
    /// 设置平坦度容差（`i`，PDF 8.4.5）：值域 0–100，0 表示使用设备默认值。
    SetFlatness(f64),

    // ── 路径构造（PDF 1.0） ──────────────────────────
    MoveTo {
        x: f64,
        y: f64,
    },
    LineTo {
        x: f64,
        y: f64,
    },
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
    SetStrokeGray {
        gray: f64,
    },
    SetFillGray {
        gray: f64,
    },
    SetStrokeRGB {
        r: f64,
        g: f64,
        b: f64,
    },
    SetFillRGB {
        r: f64,
        g: f64,
        b: f64,
    },
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
    /// `SCN`/`scn`：数字颜色分量（图案名称分支见 `SetStrokeColorPattern`
    /// 与 `SetFillColorPattern`）。
    SetStrokeColorN {
        values: Vec<f64>,
    },
    SetFillColorN {
        values: Vec<f64>,
    },
    /// 以命名图案设置描边色：`/name SCN`（彩色图案，PDF 1.3）。
    SetStrokeColorPattern(String),
    /// 以命名图案设置填充色：`/name scn`（彩色图案，PDF 1.3）。
    SetFillColorPattern(String),
    /// 以无色图案设置描边色：`c1 … cn /name SCN`，分量按图案的
    /// `/ColorSpace` 解释。
    SetStrokeColorPatternWithComponents {
        values: Vec<f64>,
        name: String,
    },
    /// 以无色图案设置填充色：`c1 … cn /name scn`，分量按图案的
    /// `/ColorSpace` 解释。
    SetFillColorPatternWithComponents {
        values: Vec<f64>,
        name: String,
    },

    // ── 标记内容（PDF 1.2，ISO 32000-1 §14.6） ────────
    /// 开始标记内容序列：`/tag BMC`。
    BeginMarkedContent {
        tag: String,
    },
    /// 开始带属性表的标记内容序列：`/tag properties BDC`。
    BeginMarkedContentProperties {
        tag: String,
        properties: MarkedContentProperties,
    },
    /// 标记内容点：`/tag MP`。
    MarkedContentPoint {
        tag: String,
    },
    /// 带属性表的标记内容点：`/tag properties DP`。
    MarkedContentPointProperties {
        tag: String,
        properties: MarkedContentProperties,
    },
    /// 结束当前标记内容序列：`EMC`。
    EndMarkedContent,

    // ── XObject Do（PDF 1.0） ──────────────────────────
    DrawXObject(String),
    /// 用命名渐变填充当前裁剪区域：`/name sh`（PDF 1.3）。
    PaintShading(String),
}
