//! 查看器首选项读取（ISO 32000-1 12.2，Table 150）。
//!
//! 查看器首选项字典挂在文档目录 `/ViewerPreferences` 上（PDF 1.2 引入），
//! 控制文档在屏幕与打印时的呈现方式。本模块把 Table 150 的全部条目建模为
//! [`ViewerPreferences`]，缺省条目按规范 Default value 物化；每字段的引入
//! 版本与缺省值见字段文档。规范原文核对（`docs/PDF32000_2008.pdf`，
//! 12.2 与 Table 150，pdftotext `-raw` 抽取）：
//!
//! | 条目 | 类型 | 缺省 | 引入版本 |
//! |------|------|------|----------|
//! | `/HideToolbar` | boolean | false | 1.2 |
//! | `/HideMenubar` | boolean | false | 1.2 |
//! | `/HideWindowUI` | boolean | false | 1.2 |
//! | `/FitWindow` | boolean | false | 1.2 |
//! | `/CenterWindow` | boolean | false | 1.2 |
//! | `/NonFullScreenPageMode` | name | UseNone | 1.2 |
//! | `/Direction` | name | L2R | 1.3 |
//! | `/ViewArea` `/ViewClip` `/PrintArea` `/PrintClip` | name | CropBox | 1.4 |
//! | `/DisplayDocTitle` | boolean | false | 1.4 |
//! | `/PrintScaling` | name | AppDefault | 1.6 |
//! | `/Duplex` | name | 无（未定义缺省） | 1.7 |
//! | `/PickTrayByPDFSize` | boolean | 由阅读器定义 | 1.7 |
//! | `/PrintPageRange` | array | 由阅读器定义 | 1.7 |
//! | `/NumCopies` | integer | 由阅读器定义（通常 1） | 1.7 |
//! | `/Enforce` | array of names | — | **不在 ISO 32000-1**（PDF 2.0 扩展） |
//!
//! `/Enforce` 未出现在 32000-1:2008 的 Table 150（全文亦无该名字），是
//! PDF 2.0（ISO 32000-2）追加的条目；本模型按名数组宽松保留，写入门禁
//! 要求目标版本 ≥2.0（本库 writer 输出区间为 1.0–1.7，因此实际不可写出）。
//!
//! 读取策略（原则 6：类型化错误、宽松行为显式实现并记录，读取永不 panic）：
//!
//! - 目录缺失或无 `/ViewerPreferences` 返回 `Ok(None)`；字典值可为间接
//!   引用（解一层），不是字典返回 [`NavigationError::Malformed`]。
//! - 条目类型错误（如 `/HideToolbar 1`）返回类型化错误，不静默按缺省。
//! - 封闭名字集合（`/NonFullScreenPageMode`、`/Direction`、四个页面边界
//!   名、`/Duplex`）的未知名字返回类型化错误。规范以「the following
//!   values are valid」列举，未定义未知名字的阅读器行为；拒绝是本库
//!   **策略**而非规范 shall。
//! - `/PrintScaling` 的未识别值按规范 shall「If this entry has an
//!   unrecognized value, AppDefault shall be used」宽松归一化为
//!   [`PrintScaling::AppDefault`]（规范强制的宽松）。
//! - `/PrintPageRange` 要求成对偶数个整数（规范 shall）且页码 ≥1（首个
//!   页面 denoted by 1）；**start > end 的倒序子区间读取端保留原值**，
//!   写出端拒绝——后者是本库策略，规范未禁止。
//! - `/NumCopies` 读取端保留任意整数（规范的范围表述含糊）；写出端要求
//!   ≥1，属本库策略。
//! - `/Enforce` 必须是名数组，名集合开放保留原文（PDF 2.0 可能扩展）。
//! - 字典中未定义的额外键忽略不报错（通用字典透传语义保留）。
//!
//! 版本档位经 [`PdfFeature`] 登记：读端在扫描目录时按 [`ViewerPreferences`]
//! 实际出现的条目登记（见 `types/features.rs`），写端在 preflight 门禁。

use crate::core::types::{Dict, Document, PdfFeature, PdfVersion, Value};

use super::{NavigationError, catalog, malformed, resolve};

/// 退出全屏时的页面模式（Table 150 `/NonFullScreenPageMode`，缺省 `UseNone`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonFullScreenPageMode {
    /// `UseNone`：大纲与缩略图均不可见（缺省）。
    UseNone,
    /// `UseOutlines`：文档大纲可见。
    UseOutlines,
    /// `UseThumbs`：缩略图可见。
    UseThumbs,
    /// `UseOC`：可选内容组面板可见。
    UseOC,
}

/// 文本 predominant 阅读顺序（Table 150 `/Direction`，PDF 1.3，缺省 `L2R`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadingDirection {
    /// `L2R`：从左到右（缺省）。
    LeftToRight,
    /// `R2L`：从右到左（含竖排 CJK）。
    RightToLeft,
}

/// 页面边界名（Table 150 的 `/ViewArea` 等条目取值，Table 30 的框键）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageBoundaryName {
    /// `MediaBox`。
    MediaBox,
    /// `CropBox`（四个边界条目的缺省）。
    CropBox,
    /// `BleedBox`。
    BleedBox,
    /// `TrimBox`。
    TrimBox,
    /// `ArtBox`。
    ArtBox,
}

/// 打印缩放选项（Table 150 `/PrintScaling`，PDF 1.6，缺省 `AppDefault`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintScaling {
    /// `None`：不缩放。
    None,
    /// `AppDefault`：使用阅读器默认缩放（缺省；未识别值按规范归一化为此）。
    AppDefault,
}

/// 打印双面模式（Table 150 `/Duplex`，PDF 1.7，无规范缺省）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Duplex {
    /// `Simplex`：单面打印。
    Simplex,
    /// `DuplexFlipShortEdge`：双面、沿短边翻转。
    DuplexFlipShortEdge,
    /// `DuplexFlipLongEdge`：双面、沿长边翻转。
    DuplexFlipLongEdge,
}

macro_rules! name_enum {
    ($ty:ty: $($variant:ident => $name:literal),+ ,) => {
        impl $ty {
            /// PDF 名字原文。
            pub(crate) fn pdf_name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name),+
                }
            }

            /// 按 PDF 名字解析；封闭集合之外的名字返回 `None`。
            pub(crate) fn from_pdf_name(name: &str) -> Option<Self> {
                match name {
                    $($name => Some(Self::$variant)),+,
                    _ => None,
                }
            }
        }
    };
}

name_enum!(NonFullScreenPageMode:
    UseNone => "UseNone",
    UseOutlines => "UseOutlines",
    UseThumbs => "UseThumbs",
    UseOC => "UseOC",
);
name_enum!(ReadingDirection:
    LeftToRight => "L2R",
    RightToLeft => "R2L",
);
name_enum!(PageBoundaryName:
    MediaBox => "MediaBox",
    CropBox => "CropBox",
    BleedBox => "BleedBox",
    TrimBox => "TrimBox",
    ArtBox => "ArtBox",
);
name_enum!(PrintScaling:
    None => "None",
    AppDefault => "AppDefault",
);
name_enum!(Duplex:
    Simplex => "Simplex",
    DuplexFlipShortEdge => "DuplexFlipShortEdge",
    DuplexFlipLongEdge => "DuplexFlipLongEdge",
);

/// 查看器首选项字典（12.2，Table 150）的完整类型化模型。
///
/// 缺省构造 [`ViewerPreferences::default`] 即「全部条目缺省」：布尔缺省
/// `false`、`/NonFullScreenPageMode` 缺省 `UseNone`、`/Direction` 缺省
/// `L2R`、四个边界名条目缺省 `CropBox`、`/PrintScaling` 缺省 `AppDefault`。
/// `/Duplex`、`/PickTrayByPDFSize`、`/NumCopies` 与打印页码子区间没有可
/// 物化的规范缺省（「as defined by the conforming reader」），以 `None` /
/// 空数组表达「未声明」。引入版本经 [`ViewerPreferences::min_version`]
/// 汇总为写入门禁；逐条目版本见模块文档表格。
#[derive(Debug, Clone, PartialEq)]
pub struct ViewerPreferences {
    /// `/HideToolbar`（boolean，缺省 false，1.2）：隐藏阅读器工具栏。
    pub hide_toolbar: bool,
    /// `/HideMenubar`（boolean，缺省 false，1.2）：隐藏菜单栏。
    pub hide_menubar: bool,
    /// `/HideWindowUI`（boolean，缺省 false，1.2）：隐藏文档窗口 UI 元素。
    pub hide_window_ui: bool,
    /// `/FitWindow`（boolean，缺省 false，1.2）：窗口尺寸贴合首页。
    pub fit_window: bool,
    /// `/CenterWindow`（boolean，缺省 false，1.2）：窗口居中。
    pub center_window: bool,
    /// `/NonFullScreenPageMode`（name，缺省 UseNone，1.2）。
    pub non_full_screen_page_mode: NonFullScreenPageMode,
    /// `/Direction`（name，缺省 L2R，1.3）。
    pub direction: ReadingDirection,
    /// `/DisplayDocTitle`（boolean，缺省 false，1.4）：标题栏显示 `/Info /Title`。
    pub display_doc_title: bool,
    /// `/ViewArea`（name，缺省 CropBox，1.4）：屏幕显示用的页面边界。
    pub view_area: PageBoundaryName,
    /// `/ViewClip`（name，缺省 CropBox，1.4）：屏幕显示裁剪用的页面边界。
    pub view_clip: PageBoundaryName,
    /// `/PrintArea`（name，缺省 CropBox，1.4）：打印渲染用的页面边界。
    pub print_area: PageBoundaryName,
    /// `/PrintClip`（name，缺省 CropBox，1.4）：打印裁剪用的页面边界。
    pub print_clip: PageBoundaryName,
    /// `/PrintScaling`（name，缺省 AppDefault，1.6）。
    pub print_scaling: PrintScaling,
    /// `/Duplex`（name，无规范缺省，1.7）：`None` 表示未声明。
    pub duplex: Option<Duplex>,
    /// `/PickTrayByPDFSize`（boolean，缺省由阅读器定义，1.7）：`None` 表示未声明。
    pub pick_tray_by_pdf_size: Option<bool>,
    /// `/PrintPageRange`（array，缺省由阅读器定义，1.7）：打印子区间对
    /// `(first, last)`，页码 1 基；空数组表示未声明。
    pub print_page_range: Vec<(i64, i64)>,
    /// `/NumCopies`（integer，缺省由阅读器定义、通常为 1，1.7）：`None` 表示未声明。
    pub num_copies: Option<i64>,
    /// `/Enforce`（array of names，PDF 2.0 扩展、不在 ISO 32000-1）：
    /// 需强制执行的特性名集合，开放保留原文；空数组表示未声明。
    pub enforce: Vec<String>,
}

impl Default for ViewerPreferences {
    fn default() -> Self {
        Self {
            hide_toolbar: false,
            hide_menubar: false,
            hide_window_ui: false,
            fit_window: false,
            center_window: false,
            non_full_screen_page_mode: NonFullScreenPageMode::UseNone,
            direction: ReadingDirection::LeftToRight,
            display_doc_title: false,
            view_area: PageBoundaryName::CropBox,
            view_clip: PageBoundaryName::CropBox,
            print_area: PageBoundaryName::CropBox,
            print_clip: PageBoundaryName::CropBox,
            print_scaling: PrintScaling::AppDefault,
            duplex: None,
            pick_tray_by_pdf_size: None,
            print_page_range: Vec::new(),
            num_copies: None,
            enforce: Vec::new(),
        }
    }
}

impl ViewerPreferences {
    /// 空首选项（全部条目缺省）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 声明 `/HideToolbar`（1.2）。
    pub fn with_hide_toolbar(mut self, value: bool) -> Self {
        self.hide_toolbar = value;
        self
    }

    /// 声明 `/HideMenubar`（1.2）。
    pub fn with_hide_menubar(mut self, value: bool) -> Self {
        self.hide_menubar = value;
        self
    }

    /// 声明 `/HideWindowUI`（1.2）。
    pub fn with_hide_window_ui(mut self, value: bool) -> Self {
        self.hide_window_ui = value;
        self
    }

    /// 声明 `/FitWindow`（1.2）。
    pub fn with_fit_window(mut self, value: bool) -> Self {
        self.fit_window = value;
        self
    }

    /// 声明 `/CenterWindow`（1.2）。
    pub fn with_center_window(mut self, value: bool) -> Self {
        self.center_window = value;
        self
    }

    /// 声明 `/NonFullScreenPageMode`（1.2）。
    pub fn with_non_full_screen_page_mode(mut self, mode: NonFullScreenPageMode) -> Self {
        self.non_full_screen_page_mode = mode;
        self
    }

    /// 声明 `/Direction`（1.3）。
    pub fn with_direction(mut self, direction: ReadingDirection) -> Self {
        self.direction = direction;
        self
    }

    /// 声明 `/DisplayDocTitle`（1.4）。
    pub fn with_display_doc_title(mut self, value: bool) -> Self {
        self.display_doc_title = value;
        self
    }

    /// 声明 `/ViewArea`（1.4）。
    pub fn with_view_area(mut self, boundary: PageBoundaryName) -> Self {
        self.view_area = boundary;
        self
    }

    /// 声明 `/ViewClip`（1.4）。
    pub fn with_view_clip(mut self, boundary: PageBoundaryName) -> Self {
        self.view_clip = boundary;
        self
    }

    /// 声明 `/PrintArea`（1.4）。
    pub fn with_print_area(mut self, boundary: PageBoundaryName) -> Self {
        self.print_area = boundary;
        self
    }

    /// 声明 `/PrintClip`（1.4）。
    pub fn with_print_clip(mut self, boundary: PageBoundaryName) -> Self {
        self.print_clip = boundary;
        self
    }

    /// 声明 `/PrintScaling`（1.6）。
    pub fn with_print_scaling(mut self, scaling: PrintScaling) -> Self {
        self.print_scaling = scaling;
        self
    }

    /// 声明 `/Duplex`（1.7）。
    pub fn with_duplex(mut self, duplex: Duplex) -> Self {
        self.duplex = Some(duplex);
        self
    }

    /// 声明 `/PickTrayByPDFSize`（1.7）。
    pub fn with_pick_tray_by_pdf_size(mut self, value: bool) -> Self {
        self.pick_tray_by_pdf_size = Some(value);
        self
    }

    /// 声明 `/PrintPageRange` 子区间对（1.7）。
    pub fn with_print_page_range(mut self, ranges: Vec<(i64, i64)>) -> Self {
        self.print_page_range = ranges;
        self
    }

    /// 声明 `/NumCopies`（1.7）。
    pub fn with_num_copies(mut self, copies: i64) -> Self {
        self.num_copies = Some(copies);
        self
    }

    /// 声明 `/Enforce` 特性名数组（PDF 2.0 扩展）。
    pub fn with_enforce(mut self, names: Vec<String>) -> Self {
        self.enforce = names;
        self
    }

    /// 已声明条目要求的最低 PDF 版本：空首选项也要求 1.2（字典本身），
    /// 其余按 [`PdfFeature`] 各档位取最大者。
    pub fn min_version(&self) -> PdfVersion {
        self.declared_features()
            .iter()
            .map(|feature| feature.min_version())
            .max()
            .unwrap_or(PdfVersion::V1_2)
    }

    /// 已声明（非缺省）条目对应的 [`PdfFeature`] 档位集合，按引入版本升序。
    /// 写端 preflight 版本门禁与特征画像登记共用；字典存在即含
    /// [`PdfFeature::ViewerPreferences`]（1.2）。
    pub(crate) fn declared_features(&self) -> Vec<PdfFeature> {
        let default = Self::default();
        let mut features = vec![PdfFeature::ViewerPreferences];
        if self.direction != default.direction {
            features.push(PdfFeature::ViewerReadingDirection);
        }
        if self.display_doc_title != default.display_doc_title {
            features.push(PdfFeature::ViewerDisplayDocTitle);
        }
        if self.view_area != default.view_area
            || self.view_clip != default.view_clip
            || self.print_area != default.print_area
            || self.print_clip != default.print_clip
        {
            features.push(PdfFeature::ViewerPageBoundaryNames);
        }
        if self.print_scaling != default.print_scaling {
            features.push(PdfFeature::ViewerPrintScaling);
        }
        if self.duplex.is_some()
            || self.pick_tray_by_pdf_size.is_some()
            || !self.print_page_range.is_empty()
            || self.num_copies.is_some()
        {
            features.push(PdfFeature::ViewerPrintDialogPreferences);
        }
        if !self.enforce.is_empty() {
            features.push(PdfFeature::ViewerEnforcement);
        }
        features
    }
}

impl Document {
    /// 读取查看器首选项（12.2，Table 150）；目录无 `/ViewerPreferences`
    /// 时返回 `Ok(None)`。
    ///
    /// 缺失的条目按规范缺省物化（见 [`ViewerPreferences::default`]）；
    /// 类型错误与封闭集合未知名字返回 [`NavigationError::Malformed`]，
    /// 宽松行为（`/PrintScaling` 未识别值、倒序 `/PrintPageRange`）见
    /// 模块文档的策略说明。读取路径不 panic。
    pub fn viewer_preferences(&self) -> Result<Option<ViewerPreferences>, NavigationError> {
        let catalog = catalog(self)?;
        let Some(value) = catalog.get("ViewerPreferences") else {
            return Ok(None);
        };
        let value = resolve(self, value)?;
        let dict = value
            .as_dict()
            .ok_or_else(|| malformed("catalog /ViewerPreferences is not a dictionary"))?;
        read_viewer_preferences(dict).map(Some)
    }
}

fn read_viewer_preferences(dict: &Dict) -> Result<ViewerPreferences, NavigationError> {
    Ok(ViewerPreferences {
        hide_toolbar: bool_entry(dict, "HideToolbar")?,
        hide_menubar: bool_entry(dict, "HideMenubar")?,
        hide_window_ui: bool_entry(dict, "HideWindowUI")?,
        fit_window: bool_entry(dict, "FitWindow")?,
        center_window: bool_entry(dict, "CenterWindow")?,
        non_full_screen_page_mode: closed_name::<NonFullScreenPageMode>(
            dict,
            "NonFullScreenPageMode",
        )?
        .unwrap_or(NonFullScreenPageMode::UseNone),
        direction: closed_name::<ReadingDirection>(dict, "Direction")?
            .unwrap_or(ReadingDirection::LeftToRight),
        display_doc_title: bool_entry(dict, "DisplayDocTitle")?,
        view_area: boundary_entry(dict, "ViewArea")?,
        view_clip: boundary_entry(dict, "ViewClip")?,
        print_area: boundary_entry(dict, "PrintArea")?,
        print_clip: boundary_entry(dict, "PrintClip")?,
        // /PrintScaling：规范 shall 未识别值按 AppDefault 处理（宽松是规范强制）。
        print_scaling: match dict.get("PrintScaling") {
            Some(Value::Name(name)) => {
                PrintScaling::from_pdf_name(name).unwrap_or(PrintScaling::AppDefault)
            }
            Some(other) => {
                return Err(malformed(format!(
                    "viewer preferences /PrintScaling is not a name, got {}",
                    other.type_name()
                )));
            }
            None => PrintScaling::AppDefault,
        },
        duplex: closed_name::<Duplex>(dict, "Duplex")?,
        pick_tray_by_pdf_size: match dict.get("PickTrayByPDFSize") {
            Some(Value::Bool(value)) => Some(*value),
            Some(other) => {
                return Err(malformed(format!(
                    "viewer preferences /PickTrayByPDFSize is not a boolean, got {}",
                    other.type_name()
                )));
            }
            None => None,
        },
        print_page_range: read_print_page_range(dict)?,
        num_copies: match dict.get("NumCopies") {
            Some(Value::Integer(value)) => Some(*value),
            Some(other) => {
                return Err(malformed(format!(
                    "viewer preferences /NumCopies is not an integer, got {}",
                    other.type_name()
                )));
            }
            None => None,
        },
        enforce: read_enforce(dict)?,
    })
}

fn bool_entry(dict: &Dict, key: &str) -> Result<bool, NavigationError> {
    match dict.get(key) {
        Some(Value::Bool(value)) => Ok(*value),
        Some(other) => Err(malformed(format!(
            "viewer preferences /{key} is not a boolean, got {}",
            other.type_name()
        ))),
        None => Ok(false),
    }
}

/// 封闭名字集合条目：缺失返回 `Ok(None)`，未知名字返回类型化错误。
fn closed_name<T: NameValue>(dict: &Dict, key: &str) -> Result<Option<T>, NavigationError> {
    match dict.get(key) {
        Some(Value::Name(name)) => T::from_pdf_name(name).map(Some).ok_or_else(|| {
            malformed(format!(
                "viewer preferences /{key} has unknown value /{name}"
            ))
        }),
        Some(other) => Err(malformed(format!(
            "viewer preferences /{key} is not a name, got {}",
            other.type_name()
        ))),
        None => Ok(None),
    }
}

/// 名字条目的共同解析协议（实现见 `name_enum!` 宏）。
pub(crate) trait NameValue: Copy {
    fn from_pdf_name(name: &str) -> Option<Self>;
}

macro_rules! impl_name_value {
    ($ty:ty) => {
        impl NameValue for $ty {
            fn from_pdf_name(name: &str) -> Option<Self> {
                <$ty>::from_pdf_name(name)
            }
        }
    };
}

impl_name_value!(NonFullScreenPageMode);
impl_name_value!(ReadingDirection);
impl_name_value!(PageBoundaryName);
impl_name_value!(PrintScaling);
impl_name_value!(Duplex);

fn boundary_entry(dict: &Dict, key: &str) -> Result<PageBoundaryName, NavigationError> {
    Ok(closed_name::<PageBoundaryName>(dict, key)?.unwrap_or(PageBoundaryName::CropBox))
}

/// `/PrintPageRange`：偶数个 1 基整数的成对子区间（规范 shall 成对）。
/// 倒序子区间（start > end）读取端保留——写出端拒绝是本库策略。
fn read_print_page_range(dict: &Dict) -> Result<Vec<(i64, i64)>, NavigationError> {
    let Some(value) = dict.get("PrintPageRange") else {
        return Ok(Vec::new());
    };
    let items = value.as_array().ok_or_else(|| {
        malformed(format!(
            "viewer preferences /PrintPageRange is not an array, got {}",
            value.type_name()
        ))
    })?;
    if items.is_empty() {
        return Ok(Vec::new());
    }
    if !items.len().is_multiple_of(2) {
        return Err(malformed(
            "viewer preferences /PrintPageRange must hold an even number of page numbers",
        ));
    }
    let mut ranges = Vec::with_capacity(items.len() / 2);
    let mut numbers = Vec::with_capacity(items.len());
    for item in items.iter() {
        let Value::Integer(number) = item else {
            return Err(malformed(format!(
                "viewer preferences /PrintPageRange item is not an integer, got {}",
                item.type_name()
            )));
        };
        if *number < 1 {
            return Err(malformed(
                "viewer preferences /PrintPageRange page numbers start at 1",
            ));
        }
        numbers.push(*number);
    }
    for pair in numbers.chunks(2) {
        ranges.push((pair[0], pair[1]));
    }
    Ok(ranges)
}

/// `/Enforce`：名数组，开放集合按原文保留（PDF 2.0 扩展条目）。
fn read_enforce(dict: &Dict) -> Result<Vec<String>, NavigationError> {
    let Some(value) = dict.get("Enforce") else {
        return Ok(Vec::new());
    };
    let items = value.as_array().ok_or_else(|| {
        malformed(format!(
            "viewer preferences /Enforce is not an array, got {}",
            value.type_name()
        ))
    })?;
    items
        .iter()
        .map(|item| {
            item.as_name()
                .map(str::to_owned)
                .ok_or_else(|| malformed("viewer preferences /Enforce item is not a name"))
        })
        .collect()
}
