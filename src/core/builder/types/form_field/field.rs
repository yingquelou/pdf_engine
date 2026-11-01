//! 字段树节点：`/FT`、`/Ff`、`/V`、`/Opt`、`/AP`、`/AA` 等条目的链式构造器。

use crate::core::builder::types::Action;

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

/// 选项条目（`/Opt` 数组元素，Table 231）。
///
/// 选择字段：仅导出值写单文本串；同时提供显示文本时写
/// `[导出值 显示文本]`。按钮字段（复选框/单选组）的 `/Opt` 只允许
/// 导出值，且须按 [`FormField::with_option`] 的顺序与 Widget 逐一对应
/// （Table 227）。
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
    top_index: Option<u32>,
    alternate_indices: Vec<u32>,
    alternate_name: Option<String>,
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
    /// bit 20：选项按字母排序（面向写出方）。
    pub const SORT: u32 = 1 << 19;
    /// bit 21：文件选择控件（PDF 1.4）。
    pub const FILE_SELECT: u32 = 1 << 20;
    /// bit 22：多选（选择字段）。
    pub const MULTI_SELECT: u32 = 1 << 21;
    /// bit 23：不拼写检查（PDF 1.4）。
    pub const DO_NOT_SPELL_CHECK: u32 = 1 << 22;
    /// bit 24：不滚动（PDF 1.4）。
    pub const DO_NOT_SCROLL: u32 = 1 << 23;
    /// bit 25：按 `/MaxLen` 分格排布（PDF 1.5）。
    pub const COMB: u32 = 1 << 24;
    /// bit 26：富文本值（PDF 1.5）。
    pub const RICH_TEXT: u32 = 1 << 25;
    /// bit 26：同值单选按钮联动（PDF 1.5，按钮字段专用）。
    pub const RADIOS_IN_UNISON: u32 = 1 << 25;
    /// bit 27：选择即提交（PDF 1.5）。
    pub const COMMIT_ON_SEL_CHANGE: u32 = 1 << 26;

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
            top_index: None,
            alternate_indices: Vec::new(),
            alternate_name: None,
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

    /// 追加一个选项（`/Opt`）。
    ///
    /// 选择字段接受 `[导出值 显示文本]` 对（Table 231）；复选框与单选
    /// 按钮字段只接受导出值，条目数须等于 Widget 数，且各 Widget 的
    /// 非 `Off` 外观状态名须为其位置序号（`/0`、`/1` …，Table 227）。
    pub fn with_option(mut self, option: FormOption) -> Self {
        self.options.push(option);
        self
    }

    /// 可滚动列表框首个可见选项的 `/Opt` 下标（`/TI`，Table 231，缺省 0）；
    /// 仅选择字段可用，下标须落在本节点 `/Opt` 之内。
    pub fn with_top_index(mut self, top_index: u32) -> Self {
        self.top_index = Some(top_index);
        self
    }

    /// 多选选择字段当前选中项的 `/Opt` 下标数组（`/I`，Table 231，PDF 1.4）：
    /// 须严格升序、不重复并要求 [`FormField::MULTI_SELECT`] 标志；与 `/V`
    /// 冲突时阅读器以 `/V` 为准。
    pub fn with_alternate_indices(mut self, indices: impl IntoIterator<Item = u32>) -> Self {
        self.alternate_indices.extend(indices);
        self
    }

    /// 替代字段名 `/TU`（Table 220，PDF 1.3；14.9.3）：阅读器在用户界面
    /// 标识该字段时以本条目取代实际字段名，亦供无障碍朗读与内容抽取使用；
    /// 文本串按 PDFDocEncoding 或 Unicode 编码写出。
    pub fn with_alternate_name(mut self, name: impl Into<String>) -> Self {
        self.alternate_name = Some(name.into());
        self
    }

    /// [`FormField::with_alternate_name`] 的别名（阅读器界面惯例把 `/TU`
    /// 显示为工具提示）。
    pub fn with_tooltip(self, tooltip: impl Into<String>) -> Self {
        self.with_alternate_name(tooltip)
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

    pub(crate) fn top_index(&self) -> Option<u32> {
        self.top_index
    }

    pub(crate) fn alternate_indices(&self) -> &[u32] {
        &self.alternate_indices
    }

    pub(crate) fn alternate_name(&self) -> Option<&str> {
        self.alternate_name.as_deref()
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
