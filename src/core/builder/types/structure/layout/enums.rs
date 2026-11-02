//! Layout 名字枚举（ISO 32000-1 14.8.5.4 Table 343–345）：单值 PDF 名字
//! 条目的类型化建模，每个枚举带 `name()` 返回不含前导 `/` 的名字，未知
//! 名字统一由 `Other` 变体按原名写出。

/// 元素放置方式（Table 343 `/Placement`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placement {
    /// `/Block`：沿块方向堆叠。
    Block,
    /// `/Inline`：沿行内方向排列（缺省）。
    Inline,
    /// `/Before`：浮到参考区 before 边。
    Before,
    /// `/Start`：浮到参考区 start 边。
    Start,
    /// `/End`：浮到参考区 end 边。
    End,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl Placement {
    /// `/Placement` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::Block => "Block",
            Self::Inline => "Inline",
            Self::Before => "Before",
            Self::Start => "Start",
            Self::End => "End",
            Self::Other(name) => name,
        }
    }
}

/// 书写方向（Table 343 `/WritingMode`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WritingMode {
    /// `/LrTb`：行内从左到右、块从上到下（缺省）。
    LrTb,
    /// `/RlTb`：行内从右到左、块从上到下。
    RlTb,
    /// `/TbRl`：行内从上到下、块从右到左。
    TbRl,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl WritingMode {
    /// `/WritingMode` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::LrTb => "LrTb",
            Self::RlTb => "RlTb",
            Self::TbRl => "TbRl",
            Self::Other(name) => name,
        }
    }
}

/// 边框样式（Table 343 `/BorderStyle`、Table 345 `/TBorderStyle`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BorderStyle {
    /// `/None`：无边框（缺省）。
    None,
    /// `/Hidden`：无边框，但参与表格边框冲突消解。
    Hidden,
    /// `/Dotted`：点线。
    Dotted,
    /// `/Dashed`：短划线。
    Dashed,
    /// `/Solid`：实线。
    Solid,
    /// `/Double`：双线。
    Double,
    /// `/Groove`：凹槽。
    Groove,
    /// `/Ridge`：凸脊。
    Ridge,
    /// `/Inset`：内嵌。
    Inset,
    /// `/Outset`：外凸。
    Outset,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl BorderStyle {
    /// 边框样式的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::None => "None",
            Self::Hidden => "Hidden",
            Self::Dotted => "Dotted",
            Self::Dashed => "Dashed",
            Self::Solid => "Solid",
            Self::Double => "Double",
            Self::Groove => "Groove",
            Self::Ridge => "Ridge",
            Self::Inset => "Inset",
            Self::Outset => "Outset",
            Self::Other(name) => name,
        }
    }
}

/// 行内方向对齐（Table 344 `/TextAlign`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextAlign {
    /// `/Start`：对齐 start 边（缺省）。
    Start,
    /// `/Center`：居中。
    Center,
    /// `/End`：对齐 end 边。
    End,
    /// `/Justify`：两端对齐。
    Justify,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl TextAlign {
    /// `/TextAlign` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::Start => "Start",
            Self::Center => "Center",
            Self::End => "End",
            Self::Justify => "Justify",
            Self::Other(name) => name,
        }
    }
}

/// 表格单元块方向对齐（Table 344 `/BlockAlign`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockAlign {
    /// `/Before`：对齐 before 边（缺省）。
    Before,
    /// `/Middle`：居中。
    Middle,
    /// `/After`：对齐 after 边。
    After,
    /// `/Justify`：两端对齐。
    Justify,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl BlockAlign {
    /// `/BlockAlign` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::Before => "Before",
            Self::Middle => "Middle",
            Self::After => "After",
            Self::Justify => "Justify",
            Self::Other(name) => name,
        }
    }
}

/// 表格单元行内方向对齐（Table 345 `/InlineAlign`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineAlign {
    /// `/Start`：对齐 start 边（缺省）。
    Start,
    /// `/Center`：居中。
    Center,
    /// `/End`：对齐 end 边。
    End,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl InlineAlign {
    /// `/InlineAlign` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::Start => "Start",
            Self::Center => "Center",
            Self::End => "End",
            Self::Other(name) => name,
        }
    }
}

/// 文本装饰类型（Table 345 `/TextDecorationType`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextDecorationType {
    /// `/None`：无装饰（缺省）。
    None,
    /// `/Underline`：下划线。
    Underline,
    /// `/Overline`：上划线。
    Overline,
    /// `/LineThrough`：删除线。
    LineThrough,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl TextDecorationType {
    /// `/TextDecorationType` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::None => "None",
            Self::Underline => "Underline",
            Self::Overline => "Overline",
            Self::LineThrough => "LineThrough",
            Self::Other(name) => name,
        }
    }
}

/// Ruby 对齐方式（Table 345 `/RubyAlign`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RubyAlign {
    /// `/Start`：对齐 start 边。
    Start,
    /// `/Center`：居中。
    Center,
    /// `/End`：对齐 end 边。
    End,
    /// `/Justify`：填满可用宽度。
    Justify,
    /// `/Distribute`：填满宽度并在两端加空隙（缺省）。
    Distribute,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl RubyAlign {
    /// `/RubyAlign` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::Start => "Start",
            Self::Center => "Center",
            Self::End => "End",
            Self::Justify => "Justify",
            Self::Distribute => "Distribute",
            Self::Other(name) => name,
        }
    }
}

/// Ruby 注音位置（Table 345 `/RubyPosition`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RubyPosition {
    /// `/Before`：注音位于 before 边（缺省）。
    Before,
    /// `/After`：注音位于 after 边。
    After,
    /// `/Warichu`：按 warichu 排版。
    Warichu,
    /// `/Inline`：按括号注释排版。
    Inline,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl RubyPosition {
    /// `/RubyPosition` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::Before => "Before",
            Self::After => "After",
            Self::Warichu => "Warichu",
            Self::Inline => "Inline",
            Self::Other(name) => name,
        }
    }
}
