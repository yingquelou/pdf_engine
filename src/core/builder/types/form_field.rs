//! 交互表单写端（ISO 32000-1 12.7.2–12.7.4）。
//!
//! [`AcroForm`] 持有表单级选项（`/NeedAppearances`、`/DA`、`/DR` 默认
//! 字体资源）与按页绑定的
//! 字段树；[`FormField`] 以链式构造声明字段条目（`/T`、`/FT`、`/Ff`、
//! `/V`/`/DV`、`/DA`、`/Q`、`/MaxLen`、`/Opt`、`/TU`）与子字段。
//!
//! 带 [`FormField::new`] 矩形（`/Rect`）的终端节点是 Widget 注解，写端
//! 输出字段与 Widget 合并字典并登记进页面 `/Annots`；无矩形的节点是
//! 纯字段容器（`/Kids`）。`/AP` 外观流句柄指向
//! [`PdfBuilder::form_xobject`](super::super::PdfBuilder::form_xobject) 池。
//! 经 [`PdfBuilder::set_acro_form`](super::super::PdfBuilder::set_acro_form)
//! 注册后，由 assemble 写成目录 `/AcroForm` 与独立字段对象。

use super::{Action, ColorSpace, Font, GraphicsState};

/// 字段类型（`/FT`，Table 220）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormFieldType {
    /// 文本字段（`/Tx`）。
    Text,
    /// 按钮字段（`/Btn`；复选、单选、按钮）。
    Button,
    /// 选择字段（`/Ch`；列表框、组合框）。
    Choice,
    /// 签名字段（`/Sig`）。
    Signature,
}

impl FormFieldType {
    /// 字段类型名（`/FT` 值）。
    pub fn name(self) -> &'static str {
        match self {
            Self::Text => "Tx",
            Self::Button => "Btn",
            Self::Choice => "Ch",
            Self::Signature => "Sig",
        }
    }
}

/// 表单字段附加动作触发事件（`/AA` 字典键，Table 196；均 PDF 1.3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FieldActionEvent {
    /// 修改字符或选择时执行（`/K`，JavaScript 动作）。
    Keystroke,
    /// 格式化显示值前执行（`/F`）。
    Format,
    /// 字段值变化时校验（`/V`）。
    Validate,
    /// 其他字段变化时重算本字段（`/C`）。
    Calculate,
}

impl FieldActionEvent {
    /// 事件键名（`/AA` 字典键）。
    pub fn name(self) -> &'static str {
        match self {
            Self::Keystroke => "K",
            Self::Format => "F",
            Self::Validate => "V",
            Self::Calculate => "C",
        }
    }
}

/// 字段值（`/V`、`/DV`，Table 221）。
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    /// 文本串：文本/选择字段的值，或按钮的导出值。
    Text(String),
    /// 名称：单选/复选按钮的当前外观状态。
    Name(String),
    /// 文本串数组：多选选择字段的值。
    Multiple(Vec<String>),
}

impl From<&str> for FieldValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_string())
    }
}

impl From<String> for FieldValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

/// 选择字段选项（`/Opt` 数组元素，Table 226）。
///
/// 仅导出值写单文本串；同时提供显示文本时写 `[导出值 显示文本]`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormOption {
    export: String,
    display: Option<String>,
}

impl FormOption {
    /// 以导出值创建选项（显示文本与导出值相同）。
    pub fn new(export: impl Into<String>) -> Self {
        Self {
            export: export.into(),
            display: None,
        }
    }

    /// 指定与导出值不同的显示文本。
    pub fn with_display(mut self, display: impl Into<String>) -> Self {
        self.display = Some(display.into());
        self
    }

    pub(crate) fn export(&self) -> &str {
        &self.export
    }

    pub(crate) fn display(&self) -> Option<&str> {
        self.display.as_deref()
    }
}

/// 字段树节点（字段字典 + 可选 Widget 注解）。
#[derive(Debug, Clone, PartialEq)]
pub struct FormField {
    partial_name: Option<String>,
    field_type: Option<FormFieldType>,
    flags: u32,
    value: Option<FieldValue>,
    default_value: Option<FieldValue>,
    default_appearance: Option<String>,
    quadding: Option<u8>,
    max_len: Option<u32>,
    options: Vec<FormOption>,
    tooltip: Option<String>,
    rect: Option<[f64; 4]>,
    annotation_flags: u32,
    appearance_normal: Option<usize>,
    appearance_states: Vec<(String, usize)>,
    appearance_down: Option<usize>,
    appearance_rollover: Option<usize>,
    appearance_state: Option<String>,
    page: Option<usize>,
    actions: Vec<(FieldActionEvent, Action)>,
    kids: Vec<FormField>,
}

impl FormField {
    /// bit 1：只读。
    pub const READ_ONLY: u32 = 1 << 0;
    /// bit 2：必填。
    pub const REQUIRED: u32 = 1 << 1;
    /// bit 3：不导出。
    pub const NO_EXPORT: u32 = 1 << 2;
    /// bit 13：多行文本。
    pub const MULTILINE: u32 = 1 << 12;
    /// bit 14：密码文本。
    pub const PASSWORD: u32 = 1 << 13;
    /// bit 15：单选按钮组不得通过点击取消（`/NoToggleToOff`）。
    pub const NO_TOGGLE_TO_OFF: u32 = 1 << 14;
    /// bit 16：单选按钮。
    pub const RADIO: u32 = 1 << 15;
    /// bit 17：按钮（无持久值）。
    pub const PUSHBUTTON: u32 = 1 << 16;
    /// bit 18：组合框（选择字段）。
    pub const COMBO: u32 = 1 << 17;
    /// bit 19：可编辑组合框。
    pub const EDIT: u32 = 1 << 18;
    /// bit 22：多选（选择字段）。
    pub const MULTI_SELECT: u32 = 1 << 21;

    fn node(
        field_type: Option<FormFieldType>,
        partial_name: Option<String>,
        rect: Option<[f64; 4]>,
        flags: u32,
    ) -> Self {
        Self {
            partial_name,
            field_type,
            flags,
            value: None,
            default_value: None,
            default_appearance: None,
            quadding: None,
            max_len: None,
            options: Vec::new(),
            tooltip: None,
            rect,
            annotation_flags: 0,
            appearance_normal: None,
            appearance_states: Vec::new(),
            appearance_down: None,
            appearance_rollover: None,
            appearance_state: None,
            page: None,
            actions: Vec::new(),
            kids: Vec::new(),
        }
    }

    /// 文本字段（`/FT /Tx`）；`rect` 为 Widget 区域。
    pub fn text(name: impl Into<String>, rect: [f64; 4]) -> Self {
        Self::node(Some(FormFieldType::Text), Some(name.into()), Some(rect), 0)
    }

    /// 复选框（`/FT /Btn`）。
    pub fn checkbox(name: impl Into<String>, rect: [f64; 4]) -> Self {
        Self::node(
            Some(FormFieldType::Button),
            Some(name.into()),
            Some(rect),
            0,
        )
    }

    /// 单选按钮（`/FT /Btn` + `/Radio` 标志）。
    pub fn radio(name: impl Into<String>, rect: [f64; 4]) -> Self {
        Self::node(
            Some(FormFieldType::Button),
            Some(name.into()),
            Some(rect),
            Self::RADIO,
        )
    }

    /// 单选按钮组（`/FT /Btn` + `/Radio`，无 `/Rect`）：子 Widget 经
    /// [`FormField::with_child`] 添加，各自携带 `/AP /N` 状态子字典与
    /// `/AS`（Table 189）。
    pub fn radio_group(name: impl Into<String>) -> Self {
        Self::node(
            Some(FormFieldType::Button),
            Some(name.into()),
            None,
            Self::RADIO,
        )
    }

    /// 按钮（`/FT /Btn` + `/Pushbutton` 标志）。
    pub fn push_button(name: impl Into<String>, rect: [f64; 4]) -> Self {
        Self::node(
            Some(FormFieldType::Button),
            Some(name.into()),
            Some(rect),
            Self::PUSHBUTTON,
        )
    }

    /// 选择字段（`/FT /Ch`）。
    pub fn choice(name: impl Into<String>, rect: [f64; 4]) -> Self {
        Self::node(
            Some(FormFieldType::Choice),
            Some(name.into()),
            Some(rect),
            0,
        )
    }

    /// 签名字段（`/FT /Sig`）。
    pub fn signature(name: impl Into<String>, rect: [f64; 4]) -> Self {
        Self::node(
            Some(FormFieldType::Signature),
            Some(name.into()),
            Some(rect),
            0,
        )
    }

    /// 纯字段容器（无 `/Rect`），子节点经 [`FormField::with_child`] 添加。
    pub fn group(name: impl Into<String>) -> Self {
        Self::node(None, Some(name.into()), None, 0)
    }

    /// 纯 Widget 注解节点（无 `/T` 与 `/FT`），从父字段继承属性；
    /// 用于单选按钮组等多 Widget 终端字段。
    pub fn widget(rect: [f64; 4]) -> Self {
        Self::node(None, None, Some(rect), 0)
    }

    /// 原始 `/Ff` 标志位。
    pub fn with_flags(mut self, flags: u32) -> Self {
        self.flags = flags;
        self
    }

    /// 字段值 `/V`。
    pub fn with_value(mut self, value: impl Into<FieldValue>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// 默认值 `/DV`。
    pub fn with_default_value(mut self, value: impl Into<FieldValue>) -> Self {
        self.default_value = Some(value.into());
        self
    }

    /// 默认外观字符串 `/DA`（如 `/Helv 12 Tf 0 g`）。
    pub fn with_default_appearance(mut self, appearance: impl Into<String>) -> Self {
        self.default_appearance = Some(appearance.into());
        self
    }

    /// 文本对齐 `/Q`：0 左、1 居中、2 右。
    pub fn with_quadding(mut self, quadding: u8) -> Self {
        self.quadding = Some(quadding);
        self
    }

    /// 文本最大长度 `/MaxLen`。
    pub fn with_max_len(mut self, max_len: u32) -> Self {
        self.max_len = Some(max_len);
        self
    }

    /// 追加一个选择字段选项（`/Opt`）。
    pub fn with_option(mut self, option: FormOption) -> Self {
        self.options.push(option);
        self
    }

    /// 工具提示 `/TU`。
    pub fn with_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// Widget 注解标志位（`/F`，Table 165；如 4=打印）。
    pub fn with_annotation_flags(mut self, flags: u32) -> Self {
        self.annotation_flags = flags;
        self
    }

    /// 常态外观流（`/AP /N`）；参数为 Form XObject 句柄。
    pub fn with_appearance(mut self, handle: usize) -> Self {
        self.appearance_normal = Some(handle);
        self
    }

    /// 按下态外观流（`/AP /D`）。
    pub fn with_appearance_down(mut self, handle: usize) -> Self {
        self.appearance_down = Some(handle);
        self
    }

    /// 悬停态外观流（`/AP /R`）。
    pub fn with_appearance_rollover(mut self, handle: usize) -> Self {
        self.appearance_rollover = Some(handle);
        self
    }

    /// 常态外观状态字典条目（`/AP /N << /state 句柄 >>`，Table 189）；
    /// 与单一流 `with_appearance` 互斥。多状态外观用于单选/复选框组。
    pub fn with_appearance_state(mut self, state: impl Into<String>, handle: usize) -> Self {
        self.appearance_states.push((state.into(), handle));
        self
    }

    /// 当前外观状态名 `/AS`（须为已登记的常态外观状态）。
    pub fn with_selected_appearance_state(mut self, state: impl Into<String>) -> Self {
        self.appearance_state = Some(state.into());
        self
    }

    /// 覆盖该节点（及其子节点默认）的绑定页号。
    pub fn with_page(mut self, page: usize) -> Self {
        self.page = Some(page);
        self
    }

    /// 追加子字段或子 Widget。
    pub fn with_child(mut self, child: FormField) -> Self {
        self.kids.push(child);
        self
    }

    /// 表单字段附加动作 `/AA`（Table 196，PDF 1.3）：`event` 对应 `/K`
    /// `/F` `/V` `/C` 四个触发键；同一事件只能声明一次，且规范限定为
    /// JavaScript 动作。
    pub fn with_action(mut self, event: FieldActionEvent, action: Action) -> Self {
        self.actions.push((event, action));
        self
    }

    /// 该节点是否为 Widget 注解（声明了 `/Rect`）。
    pub(crate) fn is_widget(&self) -> bool {
        self.rect.is_some()
    }

    pub(crate) fn partial_name(&self) -> Option<&str> {
        self.partial_name.as_deref()
    }

    pub(crate) fn field_type(&self) -> Option<FormFieldType> {
        self.field_type
    }

    pub(crate) fn flags(&self) -> u32 {
        self.flags
    }

    pub(crate) fn value(&self) -> Option<&FieldValue> {
        self.value.as_ref()
    }

    pub(crate) fn default_value(&self) -> Option<&FieldValue> {
        self.default_value.as_ref()
    }

    pub(crate) fn default_appearance(&self) -> Option<&str> {
        self.default_appearance.as_deref()
    }

    pub(crate) fn quadding(&self) -> Option<u8> {
        self.quadding
    }

    pub(crate) fn max_len(&self) -> Option<u32> {
        self.max_len
    }

    pub(crate) fn options(&self) -> &[FormOption] {
        &self.options
    }

    pub(crate) fn tooltip(&self) -> Option<&str> {
        self.tooltip.as_deref()
    }

    pub(crate) fn rect(&self) -> Option<[f64; 4]> {
        self.rect
    }

    pub(crate) fn annotation_flags(&self) -> u32 {
        self.annotation_flags
    }

    pub(crate) fn appearance_normal(&self) -> Option<usize> {
        self.appearance_normal
    }

    pub(crate) fn appearance_states(&self) -> &[(String, usize)] {
        &self.appearance_states
    }

    pub(crate) fn appearance_down(&self) -> Option<usize> {
        self.appearance_down
    }

    pub(crate) fn appearance_rollover(&self) -> Option<usize> {
        self.appearance_rollover
    }

    pub(crate) fn appearance_state(&self) -> Option<&str> {
        self.appearance_state.as_deref()
    }

    pub(crate) fn page(&self) -> Option<usize> {
        self.page
    }

    pub(crate) fn actions(&self) -> &[(FieldActionEvent, Action)] {
        &self.actions
    }

    pub(crate) fn kids(&self) -> &[FormField] {
        &self.kids
    }
}

/// AcroForm 写端配置：表单级条目与按页绑定的字段树。
#[derive(Debug, Clone, Default)]
pub struct AcroForm {
    need_appearances: bool,
    default_appearance: Option<String>,
    default_fonts: Vec<(String, Font)>,
    default_ext_gstates: Vec<(String, GraphicsState)>,
    default_color_spaces: Vec<(String, ColorSpace)>,
    proc_set: Vec<String>,
    calculation_order: Option<Vec<String>>,
    fields: Vec<(usize, FormField)>,
}

impl AcroForm {
    /// 新建空表单配置。
    pub fn new() -> Self {
        Self::default()
    }

    /// `/NeedAppearances`：要求阅读器为字段生成外观流。
    pub fn with_need_appearances(mut self, need_appearances: bool) -> Self {
        self.need_appearances = need_appearances;
        self
    }

    /// 文档级默认外观字符串 `/DA`。
    pub fn with_default_appearance(mut self, appearance: impl Into<String>) -> Self {
        self.default_appearance = Some(appearance.into());
        self
    }

    /// `/DR /Font` 默认资源：把外观字符串引用的资源名（如 `Helv`）绑定
    /// 到字体；同一字体与页面已用字体去重共享对象，重名报错。
    pub fn with_default_font(mut self, name: impl Into<String>, font: Font) -> Self {
        self.default_fonts.push((name.into(), font));
        self
    }

    /// `/DR /ExtGState` 默认图形状态资源：外观流用 `/name gs` 引用。
    /// 只接受不依赖外部对象（字体、软蒙版、函数或半色调）的自包含状态。
    pub fn with_default_ext_gstate(
        mut self,
        name: impl Into<String>,
        state: GraphicsState,
    ) -> Self {
        self.default_ext_gstates.push((name.into(), state));
        self
    }

    /// `/DR /ColorSpace` 默认颜色空间资源（Table 34），外观流用
    /// `/name cs` 引用。
    pub fn with_default_color_space(mut self, name: impl Into<String>, space: ColorSpace) -> Self {
        self.default_color_spaces.push((name.into(), space));
        self
    }

    /// `/DR /ProcSet` 过程集名称数组（Table 34）：常用 `/PDF`、`/Text`、
    /// `/ImageB`、`/ImageC`、`/ImageI`；名称须合法且不重复。
    pub fn with_proc_set(mut self, names: Vec<String>) -> Self {
        self.proc_set = names;
        self
    }

    /// 追加一个顶层字段树；`page` 为该树内未显式指定页号的 Widget 的
    /// 绑定页（0 基）。
    pub fn add_field(&mut self, page: usize, field: FormField) -> &mut Self {
        self.fields.push((page, field));
        self
    }

    /// 计算顺序 `/CO`（Table 218，PDF 1.3）：按全限定名列出带 `/C`
    /// 计算动作的字段，定义重算顺序；缺省时按字段树前序自动生成。
    /// 显式名录必须恰好覆盖全部计算字段且不得重复。
    pub fn with_calculation_order(mut self, names: Vec<String>) -> Self {
        self.calculation_order = Some(names);
        self
    }

    pub(crate) fn need_appearances(&self) -> bool {
        self.need_appearances
    }

    pub(crate) fn default_appearance(&self) -> Option<&str> {
        self.default_appearance.as_deref()
    }

    pub(crate) fn default_fonts(&self) -> &[(String, Font)] {
        &self.default_fonts
    }

    pub(crate) fn default_ext_gstates(&self) -> &[(String, GraphicsState)] {
        &self.default_ext_gstates
    }

    pub(crate) fn default_color_spaces(&self) -> &[(String, ColorSpace)] {
        &self.default_color_spaces
    }

    pub(crate) fn proc_set(&self) -> &[String] {
        &self.proc_set
    }

    pub(crate) fn calculation_order(&self) -> Option<&[String]> {
        self.calculation_order.as_deref()
    }

    pub(crate) fn fields(&self) -> &[(usize, FormField)] {
        &self.fields
    }
}
