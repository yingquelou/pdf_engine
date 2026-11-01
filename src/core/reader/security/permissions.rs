//! 加密字典 `/P` 权限位的读端解释（ISO 32000-1 7.6.3.2、Table 22）。
//!
//! 标准安全处理器认证时已把 `/P` 用于文件密钥派生（见 `standard.rs` 的
//! Algorithm 2 实现），但此前没有面向调用方的权限视图。本模块补上视图：
//! 已认证打开的文档可询问「以用户身份打开时某项操作是否被允许」。
//!
//! ## 规范强制（shall）
//!
//! - 7.6.3.2：`/P` 的值 shall 按**无符号 32 位量**解释，为标志位集合，
//!   指位 shall 从 1（最低位）编到 32（最高位）；任一位为 1 shall 使能
//!   对应访问权限；哪些位有意义（以及部分位如何解读）shall 取决于安全
//!   处理器的 revision（`/R`）。
//! - 7.6.3.2：conforming reader shall 忽略除位 3、4、5、6、9、10、11、12
//!   以外的全部标志位——位 1、2、7、8、13–32 一律不定义权限。
//! - 7.6.3.2 的 NOTE：PDF 整数可按**有符号二进制补码**解读；因 `/P` 的
//!   保留高位 shall 全部置 1，`/P` 写出来 shall 是负整数（示例：revision
//!   2 下 `-44` 允许打印与复制，禁止修改内容与注解）。本类型同时保留
//!   有符号原值（[`Permissions::p_value`]）与无符号位词
//!   （[`Permissions::flags`]）：位词就是原值的低 32 位
//!   （`i32` → `u32` 的位模式保持转换），与 Algorithm 2 d 步「把 `/P` 的
//!   整数值转换为 32 位无符号二进制数、低位字节在前喂给 MD5」完全一致，
//!   故密钥派生与本视图读到的位是同一批位。
//! - Table 22 位义（编号即 1 基位号）：3 打印；4 修改内容（不含 6、9、11
//!   所控操作）；5 复制/抽取文本与图形；6 增改文本注解与填写交互表单字段，
//!   **且仅当位 4 也置位时**才可创建或修改交互表单字段（含签名字段）；
//!   9（R3+）即使位 6 为清也可填写既有交互表单字段；10（R3+）为残障人士
//!   无障碍用途（或其它目的）抽取文本与图形；11（R3+）即使位 4 为清也可
//!   组装文档（插入/旋转/删除页面、创建书签或缩略图）；12（R3+）可打印出
//!   能还原忠实数字副本的表示，该位为清且位 3 置位时打印降级为低保真表示。
//! - Table 21 `/R`：revision 2 只有在**没有任何**「R3 或更高」权限位为 0
//!   时才成立——即 conforming 写端的 R2 文档里位 9–12 必为 1（与上面的
//!   保留高位 shall 置 1 一致）。
//! - Table 21 `/P`：Required。故 `/P` 缺失或不是整数**不**适用本仓库
//!   读端「宽进降级为 `None`」的默认策略：该字典无法派生文件密钥，
//!   认证阶段就按本模块既有的错误模型报错（见 `standard.rs`）。
//!
//! ## 库策略（非规范强制）
//!
//! - revision 2 下 R3+ 才定义的位（9、10、11、12）不给「猜 false」，而是
//!   回落到 7.6.3.1 列出的 R2 受控操作集合：填写表单落在位 6（Table 22
//!   位 6 行本身不限 revision，明文含「fill in interactive form fields」）、
//!   无障碍抽取落在位 5（Table 22 位 5 行的 R2 表述明文含 accessibility）、
//!   组装落在位 4（R2 只区分「修改内容」），高保真打印落在位 3（R2 无质量
//!   分级）。 conforming 文件里这些位本来就是 1，两种读法结果相同；本库
//!   选择**语义来源明确**的回落而非无条件 `true`。
//! - `revision` 不是 2、3、4 时（R5/R6 属 PDF 2.0，本路线图明确 ❌ 非目标，
//!   且 7.6.3.1 的 R5/R6 位义不在 ISO 32000-1 内）按 R3+ 位义读取，并由
//!   [`Permissions::revision`] 原样回报实际值，交由调用方判定。
//! - Adobe PDF Reference 1.7 广为流传的「位 3 为清则位 4/5/6 视为置位」式
//!   降级链**不在** ISO 32000-1 的 7.6.3.2/Table 22 文本里，故本模块
//!   **不**实现该链，按位如实读取；需要降级链的策略由调用方决定。
//! - 7.6.3.1 用 should 描述「以所有者密码打开 shall/should 赋予完全访问」，
//!   规范又把 Table 22 明文限定为「以**用户**访问打开时」的权限。本类型
//!   只表达 `/P` 的**用户访问**权限；认证时使用的是用户密码还是所有者
//!   密码不记录，因此不提供 owner 视图。

use crate::core::types::Document;

/// 加密字典 `/P`（Table 22）的类型化用户访问权限视图。
///
/// 由 [`Document::permissions`] 从已认证的标准安全处理器取出；未加密
/// 或写入端构造的文档没有该视图。位号沿用规范的 1 基编号。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permissions {
    /// 文件里写的有符号原值（NOTE：保留高位须置 1，故合法值通常为负）。
    p_value: i32,
    /// `/P` 的低 32 位位词（7.6.3.2 的「无符号 32 位量」解读）。
    flags: u32,
    /// 标准安全处理器 revision（`/R`）；决定哪些位有定义。
    revision: u8,
}

impl Permissions {
    /// 由认证阶段读到的 `/P`（有符号 32 位）与 `/R` 构造权限视图。
    ///
    /// 位词按位模式保持转换得到，不做符号扩展或截断（7.6.3.2 的无符号
    /// 解读 + Algorithm 2 d 的 32 位无符号二进制）。
    pub(crate) const fn from_p_entry(p_value: i32, revision: u8) -> Self {
        Self {
            p_value,
            flags: p_value as u32,
            revision,
        }
    }

    /// `/P` 在文件里的有符号整数值。
    pub fn p_value(&self) -> i32 {
        self.p_value
    }

    /// `/P` 的无符号 32 位位词（含未定义位，原样保留）。
    pub fn flags(&self) -> u32 {
        self.flags
    }

    /// 安全处理器 revision（`/R`）：2、3、4 在本库支持范围内；其它值
    /// 原样回报（R5/R6 非目标，见模块文档）。
    pub fn revision(&self) -> u8 {
        self.revision
    }

    /// 按 1 基位号读原始位（7.6.3.2：位 1 是最低位、位 32 是最高位）。
    ///
    /// 位号为 0 或大于 32 返回 `None`，不 panic；该访问器**不**做
    /// revision 回落，给出的是文件里字面的位值。
    pub fn bit(&self, position: u8) -> Option<bool> {
        match position {
            1..=32 => Some(self.raw_bit(position)),
            _ => None,
        }
    }

    /// 该位在当前 revision 下是否由 Table 22 定义（1 基位号）。
    ///
    /// R2 定义位 3、4、5、6；R3 与 R4 另加 9、10、11、12。其余位（含
    /// 1、2、7、8、13–32）按 7.6.3.2 shall 被 conforming reader 忽略，
    /// 故返回 `false`。位号 0 或大于 32 同样返回 `false`。
    pub fn is_defined(&self, position: u8) -> bool {
        matches!(position, 3..=6) || (self.revision != 2 && matches!(position, 9..=12))
    }

    /// 位 3：打印文档。
    pub fn can_print(&self) -> bool {
        self.raw_bit(3)
    }

    /// 打印是否可用「可还原出忠实数字副本」的表示。
    ///
    /// R3+：位 3 与位 12 同时置位（Table 22 位 12 行：该位为清且位 3 置位
    /// 时打印限于低保真外观表示）。R2：无质量分级，跟随位 3（库策略，
    /// 理由见模块文档）。
    pub fn can_print_faithfully(&self) -> bool {
        if self.revision == 2 {
            self.raw_bit(3)
        } else {
            self.raw_bit(3) && self.raw_bit(12)
        }
    }

    /// 位 4：修改文档内容（Table 22 明示不含位 6、9、11 所控操作）。
    pub fn can_modify(&self) -> bool {
        self.raw_bit(4)
    }

    /// 位 5：复制或抽取文档中的文本与图形。
    ///
    /// 位 5 的适用范围随 revision 收窄：R2 明文包含无障碍抽取，R3+ 则
    /// 排除位 10 所控的抽取操作（见 [`Self::can_extract_for_accessibility`]）。
    pub fn can_copy(&self) -> bool {
        self.raw_bit(5)
    }

    /// 位 6：增改文本注解、填写交互表单字段。
    pub fn can_annotate(&self) -> bool {
        self.raw_bit(6)
    }

    /// 创建或修改交互表单字段（含签名字段）：Table 22 位 6 行明文的
    /// 「if bit 4 is also set」条件，即位 6 与位 4 同时置位。
    pub fn can_create_form_fields(&self) -> bool {
        self.raw_bit(6) && self.raw_bit(4)
    }

    /// 填写既有交互表单字段（含签名字段）。
    ///
    /// R3+：位 9（「即使位 6 为清」亦允许）或位 6。R2：位 9 无定义，
    /// 该操作由位 6 行的明文覆盖，故回落到位 6（库策略）。
    pub fn can_fill_forms(&self) -> bool {
        if self.revision == 2 {
            self.raw_bit(6)
        } else {
            self.raw_bit(9) || self.raw_bit(6)
        }
    }

    /// 位 10（R3+）：为残障人士无障碍用途（或其它目的）抽取文本与图形。
    ///
    /// R3+ 下该操作与位 5 的复制相互独立（位 5 行明文排除「由位 10 控制的
    /// 操作」）。R2：位 10 无定义，位 5 行的 R2 表述明文含无障碍抽取，
    /// 故回落到位 5（库策略）。
    pub fn can_extract_for_accessibility(&self) -> bool {
        if self.revision == 2 {
            self.raw_bit(5)
        } else {
            self.raw_bit(10)
        }
    }

    /// 位 11（R3+）：组装文档——插入、旋转或删除页面，创建书签或缩略图，
    /// 即使位 4 为清亦允许。
    ///
    /// R2：位 11 无定义，7.6.3.1 的 R2 受控操作集合里组装落在「修改文档
    /// 内容」，故回落到位 4（库策略）。
    pub fn can_assemble_document(&self) -> bool {
        if self.revision == 2 {
            self.raw_bit(4)
        } else {
            self.raw_bit(11)
        }
    }

    fn raw_bit(&self, position: u8) -> bool {
        match position {
            1..=32 => self.flags & (1u32 << (position - 1)) != 0,
            // 规范只把位编号到 1–32；越界一律视为未置位（库策略，不 panic）。
            _ => false,
        }
    }
}

impl Document {
    /// 已认证加密文档的 `/P` 用户访问权限（7.6.3.2、Table 22）。
    ///
    /// 返回 `None` 表示没有可读的权限模型：文档未加密（无已认证的
    /// 安全处理器）、写入端构造的 [`Document`]，或处理器不提供 `/P`
    /// 视图（公钥安全处理器 7.6.4 未实现）。标准安全处理器认证成功时
    /// `/P` 必为 32 位整数（Table 21 列为 Required，缺失、非整数或超出
    /// `i32` 范围在认证阶段即报错），故此时返回 `Some`。
    ///
    /// 返回的视图描述**以用户访问打开**时的权限；规范未把所有者密码的
    /// 完全访问写进 `/P`，本库也不判定它（见模块文档的库策略清单）。
    pub fn permissions(&self) -> Option<Permissions> {
        self.security
            .as_ref()
            .and_then(|handler| handler.permission_entry())
            .map(|(p_value, revision)| Permissions::from_p_entry(p_value, revision))
    }
}
