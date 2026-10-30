//! 注解子类型枚举（Table 169 写端支持子集）。
//!
//! 引入版本与标记类归属一律取自读端
//! [`AnnotationSubtype`]（Table 169 唯一权威表），本枚举只提供写端子集。
//! `PrinterMark`/`TrapNet`/`3D` 暂不建模：Widget 由 AcroForm 写端
//! 的 [`FormField`](super::super::FormField) 合并字段与 Widget 字典写出（E4），
//! 其余按需增补。Sound（Table 185）的 `/Sound` 与 `/Name` 由 `sound` 子模块
//! 建模，Movie（Table 186）的 `/Movie` 与 `/A` 播放设置由 `movie` 子模块
//! 建模。

use crate::core::reader::annotation::AnnotationSubtype;
use crate::core::types::PdfVersion;

/// 注解子类型（Table 169 写端支持子集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationKind {
    /// 文本批注（`/Text`，PDF 1.0）。
    Text,
    /// 链接（`/Link`，PDF 1.0）。
    Link,
    /// 自由文本（`/FreeText`，PDF 1.3）。
    FreeText,
    /// 直线（`/Line`，PDF 1.3）。
    Line,
    /// 矩形（`/Square`，PDF 1.3）。
    Square,
    /// 椭圆（`/Circle`，PDF 1.3）。
    Circle,
    /// 多边形（`/Polygon`，PDF 1.5）。
    Polygon,
    /// 折线（`/PolyLine`，PDF 1.5）。
    PolyLine,
    /// 高亮（`/Highlight`，PDF 1.3）。
    Highlight,
    /// 下划线（`/Underline`，PDF 1.3）。
    Underline,
    /// 波浪线（`/Squiggly`，PDF 1.4）。
    Squiggly,
    /// 删除线（`/StrikeOut`，PDF 1.3）。
    StrikeOut,
    /// 插入点（`/Caret`，PDF 1.5）。
    Caret,
    /// 墨迹（`/Ink`，PDF 1.3）。
    Ink,
    /// 弹出窗口（`/Popup`，PDF 1.3）。
    Popup,
    /// 文件附件（`/FileAttachment`，PDF 1.3）。
    FileAttachment,
    /// 声音（`/Sound`，PDF 1.2；`/Sound` 声音对象引用，Table 185）。
    Sound,
    /// 电影（`/Movie`，PDF 1.2；`/Movie` 电影字典与 `/A` 播放设置，Table 186）。
    Movie,
    /// 图章（`/Stamp`，PDF 1.3）。
    Stamp,
    /// 涂消（`/Redact`，PDF 1.7，标记类）。
    Redact,
    /// 屏幕（`/Screen`，PDF 1.5；外观特性字典 `/MK` 的注解级宿主）。
    Screen,
    /// 水印（`/Watermark`，PDF 1.6；固定打印字典 `/FixedPrint` 的宿主）。
    Watermark,
}

impl AnnotationKind {
    /// 对应的读端子类型（Table 169 同一枚举）；版本与标记类归属由其单点给出。
    pub fn subtype(self) -> AnnotationSubtype {
        match self {
            Self::Text => AnnotationSubtype::Text,
            Self::Link => AnnotationSubtype::Link,
            Self::FreeText => AnnotationSubtype::FreeText,
            Self::Line => AnnotationSubtype::Line,
            Self::Square => AnnotationSubtype::Square,
            Self::Circle => AnnotationSubtype::Circle,
            Self::Polygon => AnnotationSubtype::Polygon,
            Self::PolyLine => AnnotationSubtype::PolyLine,
            Self::Highlight => AnnotationSubtype::Highlight,
            Self::Underline => AnnotationSubtype::Underline,
            Self::Squiggly => AnnotationSubtype::Squiggly,
            Self::StrikeOut => AnnotationSubtype::StrikeOut,
            Self::Caret => AnnotationSubtype::Caret,
            Self::Ink => AnnotationSubtype::Ink,
            Self::Popup => AnnotationSubtype::Popup,
            Self::FileAttachment => AnnotationSubtype::FileAttachment,
            Self::Sound => AnnotationSubtype::Sound,
            Self::Movie => AnnotationSubtype::Movie,
            Self::Stamp => AnnotationSubtype::Stamp,
            Self::Redact => AnnotationSubtype::Redact,
            Self::Screen => AnnotationSubtype::Screen,
            Self::Watermark => AnnotationSubtype::Watermark,
        }
    }

    /// 标准子类型名（`/Subtype` 值）。
    pub fn name(self) -> &'static str {
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
            Self::Stamp => "Stamp",
            Self::Redact => "Redact",
            Self::Screen => "Screen",
            Self::Watermark => "Watermark",
        }
    }

    /// 该子类型引入的最低 PDF 版本（Table 169）。
    pub fn min_version(self) -> PdfVersion {
        self.subtype()
            .min_version()
            .expect("写端子类型必属 Table 169 标准集")
    }

    /// 是否为标记类注解（Table 169 的 `Markup` 列，可携带 `/QuadPoints`）。
    pub fn is_markup(self) -> bool {
        self.subtype().is_markup()
    }
}
