//! 表单级条目（`/NeedAppearances`、`/DA`、`/DR`、`/CO`）与按页绑定的字段树根。

use crate::core::builder::types::{ColorSpace, Font, GraphicsState};

use super::field::FormField;

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
