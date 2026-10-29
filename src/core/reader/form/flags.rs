//! 字段标志位（`/Ff`）：Table 221 通用位与 Table 226/228/230 类型专属位。
//!
//! 位号沿用 ISO 32000-1 的 1 基编号；同一数值位在不同字段类型下含义
//! 不同（如 bit 23 对文本与选择字段同为 `DoNotSpellCheck`，bit 26 对
//! 按钮为 `RadiosInUnison`、对文本为 `RichText`），访问器按名称暴露，
//! 调用方应结合 `FieldType` 解读。

/// 字段标志位（Table 221 `/Ff`）。
///
/// `bits()` 保留原始值，含未建模位；访问器仅按 ISO 32000-1 标准位解码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FieldFlags(u32);

impl FieldFlags {
    pub fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub fn bits(self) -> u32 {
        self.0
    }

    fn bit(self, index: u32) -> bool {
        self.0 & (1 << index) != 0
    }

    /// bit 1：只读（Table 221）。
    pub fn read_only(self) -> bool {
        self.bit(0)
    }

    /// bit 2：必填（Table 221）。
    pub fn required(self) -> bool {
        self.bit(1)
    }

    /// bit 3：不导出（Table 221）。
    pub fn no_export(self) -> bool {
        self.bit(2)
    }

    /// bit 15：单选组始终保留一个选中项（Table 226）。
    pub fn no_toggle_to_off(self) -> bool {
        self.bit(14)
    }

    /// bit 16：单选按钮组（Table 226）。
    pub fn radio(self) -> bool {
        self.bit(15)
    }

    /// bit 17：按钮（无持久值，Table 226）。
    pub fn pushbutton(self) -> bool {
        self.bit(16)
    }

    /// bit 26：同值单选按钮联动（PDF 1.5，Table 226）。
    pub fn radios_in_unison(self) -> bool {
        self.bit(25)
    }

    /// bit 13：多行文本（Table 228）。
    pub fn multiline(self) -> bool {
        self.bit(12)
    }

    /// bit 14：密码文本（Table 228）。
    pub fn password(self) -> bool {
        self.bit(13)
    }

    /// bit 21：文件选择控件（PDF 1.4，Table 228）。
    pub fn file_select(self) -> bool {
        self.bit(20)
    }

    /// bit 23：不拼写检查（PDF 1.4，Table 228/230）。
    pub fn do_not_spell_check(self) -> bool {
        self.bit(22)
    }

    /// bit 24：不滚动（PDF 1.4，Table 228）。
    pub fn do_not_scroll(self) -> bool {
        self.bit(23)
    }

    /// bit 25：按 `/MaxLen` 分格排布（PDF 1.5，Table 228）。
    pub fn comb(self) -> bool {
        self.bit(24)
    }

    /// bit 26：富文本值（PDF 1.5，Table 228）。
    pub fn rich_text(self) -> bool {
        self.bit(25)
    }

    /// bit 18：组合框（Table 230）。
    pub fn combo(self) -> bool {
        self.bit(17)
    }

    /// bit 19：可编辑组合框（Table 230）。
    pub fn edit(self) -> bool {
        self.bit(18)
    }

    /// bit 20：选项按字母排序（Table 230，面向写出方）。
    pub fn sort(self) -> bool {
        self.bit(19)
    }

    /// bit 22：多选（PDF 1.4，Table 230）。
    pub fn multi_select(self) -> bool {
        self.bit(21)
    }

    /// bit 27：选择即提交（PDF 1.5，Table 230）。
    pub fn commit_on_sel_change(self) -> bool {
        self.bit(26)
    }
}
