//! 注解子类型表（12.5.3 Table 169，PDF 1.7 全集）。
//!
//! 本文件是 Table 169 的唯一权威来源：三列（子类型名、引入版本、是否标记类）
//! 逐行对应规范的 `Annotation type` / `(PDF x.y)` / `Markup` 列，
//! 写端 [`crate::core::builder::types::AnnotationKind`] 的子集经
//! [`AnnotationSubtype`] 复用同一张表，不再各写一份版本与归属。

use crate::core::types::PdfVersion;

/// 注解子类型（Table 169 全 26 种 + 未识别名）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnotationSubtype {
    /// 文本批注（PDF 1.0，标记类）。
    Text,
    /// 链接（PDF 1.0）。
    Link,
    /// 自由文本（PDF 1.3，标记类）。
    FreeText,
    /// 直线（PDF 1.3，标记类）。
    Line,
    /// 矩形（PDF 1.3，标记类）。
    Square,
    /// 椭圆（PDF 1.3，标记类）。
    Circle,
    /// 多边形（PDF 1.5，标记类）。
    Polygon,
    /// 折线（PDF 1.5，标记类）。
    PolyLine,
    /// 高亮（PDF 1.3，标记类）。
    Highlight,
    /// 下划线（PDF 1.3，标记类）。
    Underline,
    /// 波浪下划线（PDF 1.4，标记类）。
    Squiggly,
    /// 删除线（PDF 1.3，标记类）。
    StrikeOut,
    /// 插入点（PDF 1.5，标记类）。
    Caret,
    /// 墨迹（PDF 1.3，标记类）。
    Ink,
    /// 弹出窗口（PDF 1.3）。
    Popup,
    /// 文件附件（PDF 1.3，标记类）。
    FileAttachment,
    /// 声音（PDF 1.2，标记类）。
    Sound,
    /// 影片（PDF 1.2）。
    Movie,
    /// 表单控件（PDF 1.2）。
    Widget,
    /// 屏幕（PDF 1.5）。
    Screen,
    /// 图章（PDF 1.3，标记类）。
    Stamp,
    /// 打印标记（PDF 1.4）。
    PrinterMark,
    /// Trap 网络（PDF 1.3）。
    TrapNet,
    /// 水印（PDF 1.6）。
    Watermark,
    /// 三维注解（PDF 1.6）。
    ThreeD,
    /// 涂消（`/Redact`，PDF 1.7，标记类）。
    Redact,
    /// 未识别的子类型名，原样保留。
    Unknown(String),
}

/// Table 169 的一行：`(子类型名, 引入版本, 是否标记类)`。
pub type Row = (&'static str, PdfVersion, bool);

/// Table 169 全 26 种注解类型，按规范行序。
pub const TABLE_169: [Row; 26] = [
    ("Text", PdfVersion::V1_0, true),
    ("Link", PdfVersion::V1_0, false),
    ("FreeText", PdfVersion::V1_3, true),
    ("Line", PdfVersion::V1_3, true),
    ("Square", PdfVersion::V1_3, true),
    ("Circle", PdfVersion::V1_3, true),
    ("Polygon", PdfVersion::V1_5, true),
    ("PolyLine", PdfVersion::V1_5, true),
    ("Highlight", PdfVersion::V1_3, true),
    ("Underline", PdfVersion::V1_3, true),
    ("Squiggly", PdfVersion::V1_4, true),
    ("StrikeOut", PdfVersion::V1_3, true),
    ("Stamp", PdfVersion::V1_3, true),
    ("Caret", PdfVersion::V1_5, true),
    ("Ink", PdfVersion::V1_3, true),
    ("Popup", PdfVersion::V1_3, false),
    ("FileAttachment", PdfVersion::V1_3, true),
    ("Sound", PdfVersion::V1_2, true),
    ("Movie", PdfVersion::V1_2, false),
    ("Widget", PdfVersion::V1_2, false),
    ("Screen", PdfVersion::V1_5, false),
    ("PrinterMark", PdfVersion::V1_4, false),
    ("TrapNet", PdfVersion::V1_3, false),
    ("Watermark", PdfVersion::V1_6, false),
    ("3D", PdfVersion::V1_6, false),
    ("Redact", PdfVersion::V1_7, true),
];

impl AnnotationSubtype {
    pub(super) fn from_name(name: &str) -> Self {
        match name {
            "Text" => Self::Text,
            "Link" => Self::Link,
            "FreeText" => Self::FreeText,
            "Line" => Self::Line,
            "Square" => Self::Square,
            "Circle" => Self::Circle,
            "Polygon" => Self::Polygon,
            "PolyLine" => Self::PolyLine,
            "Highlight" => Self::Highlight,
            "Underline" => Self::Underline,
            "Squiggly" => Self::Squiggly,
            "StrikeOut" => Self::StrikeOut,
            "Caret" => Self::Caret,
            "Ink" => Self::Ink,
            "Popup" => Self::Popup,
            "FileAttachment" => Self::FileAttachment,
            "Sound" => Self::Sound,
            "Movie" => Self::Movie,
            "Widget" => Self::Widget,
            "Screen" => Self::Screen,
            "Stamp" => Self::Stamp,
            "PrinterMark" => Self::PrinterMark,
            "TrapNet" => Self::TrapNet,
            "Watermark" => Self::Watermark,
            "3D" => Self::ThreeD,
            "Redact" => Self::Redact,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// 标准子类型名（`/Subtype` 值；[`Self::Unknown`] 返回其原始名）。
    pub fn name(&self) -> &str {
        match self {
            Self::Text => "Text",
            Self::Link => "Link",
            Self::FreeText => "FreeText",
            Self::Line => "Line",
            Self::Square => "Square",
            Self::Circle => "Circle",
            Self::Polygon => "Polygon",
            Self::PolyLine => "PolyLine",
            Self::Highlight => "Highlight",
            Self::Underline => "Underline",
            Self::Squiggly => "Squiggly",
            Self::StrikeOut => "StrikeOut",
            Self::Caret => "Caret",
            Self::Ink => "Ink",
            Self::Popup => "Popup",
            Self::FileAttachment => "FileAttachment",
            Self::Sound => "Sound",
            Self::Movie => "Movie",
            Self::Widget => "Widget",
            Self::Screen => "Screen",
            Self::Stamp => "Stamp",
            Self::PrinterMark => "PrinterMark",
            Self::TrapNet => "TrapNet",
            Self::Watermark => "Watermark",
            Self::ThreeD => "3D",
            Self::Redact => "Redact",
            Self::Unknown(other) => other,
        }
    }

    /// 该子类型引入的最低 PDF 版本（Table 169 的 `(PDF x.y)` 标注）；
    /// 未识别子类型无版本信息，返回 `None`。
    pub fn min_version(&self) -> Option<PdfVersion> {
        TABLE_169
            .iter()
            .find(|(name, _, _)| *name == self.name())
            .map(|(_, version, _)| *version)
    }

    /// 是否为标记类注解（Table 169 的 `Markup` 列；标记类可携带 `/QuadPoints`
    /// 与 12.5.6.2 的公共条目）。
    pub fn is_markup(&self) -> bool {
        TABLE_169
            .iter()
            .find(|(name, _, _)| *name == self.name())
            .is_some_and(|(_, _, markup)| *markup)
    }
}
