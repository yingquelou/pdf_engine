//! 逻辑字体描述。

use super::{FontDescriptor, FontEncoding, FontProgram, Type3Font};

/// 简单字体的 `/Widths` 配置（PDF 32000-1 §9.6.2.1 Table 111）。
///
/// `widths[i]` 是字符码 `first_char + i` 的字形宽度（1/1000 text space
/// units），写出时对应 `/FirstChar`、`/LastChar` 与 `/Widths`。
#[derive(Debug, Clone, PartialEq)]
pub struct FontWidths {
    /// 首个字符码（`/FirstChar`）。
    pub first_char: u8,
    /// 从 `first_char` 起连续字符码的宽度，不得为空。
    pub widths: Vec<f64>,
}

/// 逻辑字体描述。
///
/// 本层支持 PDF 标准内置 Type1 字体（如 Helvetica）与嵌入字体程序。
/// 字段 [`Font::name`] 指定 BaseFont 名，[`Font::size`] 指定字号 pt，
/// [`Font::encoding`] 控制字体字典的 `/Encoding`（默认 WinAnsiEncoding），
/// [`Font::descriptor`] 可选地提供字体描述符度量，
/// [`Font::font_program`] 可选地嵌入字体程序数据，
/// [`Font::subset`] 控制是否以六字母子集标签前缀写出 BaseFont，
/// [`Font::subset_glyphs`] 可选地给出要保留的 TrueType 字形（gid）列表，
/// [`Font::subset_type1_glyphs`] 可选地给出要保留的 Type 1 字形名列表，
/// [`Font::subset_cff_glyphs`] 可选地给出要保留的 name-keyed CFF 字形名列表。
/// 可通过 [`Font::helvetica`] 便捷构造常用 Helvetica、用
/// [`Font::with_encoding`] 替换编码配置、[`Font::with_descriptor`] 添加描述符、
/// [`Font::with_font_program`] 嵌入字体程序、[`Font::subset`] 标记子集字体、
/// [`Font::with_subset_glyphs`] 请求嵌入程序按字形子集化、
/// [`Font::with_subset_type1_glyphs`] 请求嵌入的 Type 1 程序按字形名子集化、
/// [`Font::with_subset_cff_glyphs`] 请求嵌入的 name-keyed CFF 程序按字形名子集化，
/// [`Font::with_widths`] 给出简单字体的 `/Widths`，
/// [`Font::mmtype1`] 将字体作为 §9.6.2.3 的 Multiple Master 实例写出。
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// BaseFont 名（不含前导 `/`），如 `Helvetica`。
    pub name: String,
    /// 字号（pt）。
    pub size: f64,
    /// `/Encoding` 写端配置。
    pub encoding: FontEncoding,
    /// 可选的字体描述符。
    pub descriptor: Option<FontDescriptor>,
    /// 可选的嵌入字体程序。
    pub font_program: Option<FontProgram>,
    /// 是否按子集字体写出（BaseFont 加六字母标签前缀）。
    pub subset: bool,
    /// 可选的 TrueType 子集字形 ID 列表（原程序中的 gid，含 0）。
    ///
    /// 为 `Some` 时，嵌入的 TrueType 程序会先由
    /// [`subset_truetype`](crate::core::reader::font::subset_truetype) 重写为
    /// 只含这些字形（及复合字形闭包）的子集；其他程序类型会在构建时报错。
    pub subset_glyphs: Option<Vec<u16>>,
    /// 可选的 Type 1 子集字形名列表。
    ///
    /// 为 `Some` 时，嵌入的 Type 1 程序会先由
    /// [`subset_type1`](crate::core::reader::font::subset_type1) 重写为
    /// 只含 `.notdef` 与这些字形名（以及保留的 `seac` 组件）的子集；
    /// 未嵌入程序或程序不是 [`FontProgram::Type1`] 时构建报错。
    ///
    /// 本字段与 [`Font::subset_glyphs`] 互斥（同时设置会在构建时报错），
    /// 且仅适用于嵌入的 Type 1 程序；Type 3 字体不允许使用（见
    /// [`Font::type3`](super::Font::type3)）。
    pub subset_type1_glyphs: Option<Vec<String>>,
    /// 可选的 name-keyed CFF 子集字形名列表。
    ///
    /// 为 `Some` 时，嵌入的 name-keyed CFF 程序（`/FontFile3` +
    /// `/Subtype /Type1C`）会先由
    /// [`subset_cff_type1c`](crate::core::reader::font::subset_cff_type1c)
    /// 重写为只含 `.notdef` 与这些字形名的子集；其他程序类型、未嵌入
    /// 程序或未知字形名都会在构建时报错。
    ///
    /// 本字段与 [`Font::subset_glyphs`]、[`Font::subset_type1_glyphs`]
    /// 互斥（同时设置会在构建时报错），且仅适用于嵌入的 name-keyed CFF
    /// 程序；Type 3 字体不允许使用（见 [`Font::type3`](super::Font::type3)）。
    /// CFF 子集化会移除程序内的 `/Encoding`，因此不能与
    /// [`FontEncoding::BuiltIn`] 同时使用。
    pub subset_cff_glyphs: Option<Vec<String>>,
    /// 可选的简单字体 `/Widths` 配置。
    ///
    /// 为 `Some` 时字体字典写出 `/FirstChar`、`/LastChar` 与 `/Widths`
    /// （PDF 32000-1 Table 111）；提供 [`Font::font_program`] 且 BaseFont
    /// 不是标准 14 字体时必须提供，否则构建报错。Type 3 字体不允许使用
    /// （其 `/Widths` 由字形定义推导）。
    pub widths: Option<FontWidths>,
    /// 可选的 Type 3 字体描述。
    ///
    /// 为 `Some` 时写出 `/Subtype /Type3` 字体字典（`/CharProcs`、
    /// `/Widths`、`/Encoding` 等），不写 `/BaseFont` 与字体程序；
    /// `/FontDescriptor` 仅在通过 [`Font::with_descriptor`] 提供时写出，
    /// `/Name`、`/ToUnicode`、`/Resources` 由 [`Type3Font`] 设置。
    /// 此时 [`Font::font_program`]、[`Font::subset_glyphs`]、
    /// [`Font::subset_type1_glyphs`] 与 [`Font::subset_cff_glyphs`] 必须为
    /// `None`（否则构建时报错），[`Font::subset`] 无效果（Type 3 没有
    /// `/BaseFont` 可加子集标签）。
    pub type3: Option<Type3Font>,
    /// §9.6.2.3 多重母版（MMType1）实例标记。
    ///
    /// 为 `true` 时写出 `/Subtype /MMType1` 实例字典。§9.6.2.3（raw 第
    /// 13972–13973 行）说 MM 实例字典「may contain the same entries as a
    /// Type 1 font dictionary (see Table 111)」，差异**只有**规范列出的两条
    /// （raw 13974–13977）：`/Subtype` shall 为 `MMType1`；实例 PostScript 名
    /// 中的空格（20h）shall 在 `/BaseFont` 里写成 LOW LINE（下划线，5Fh）。
    /// 因此 [`Font::name`] 按**实例名原文**给出（可含空格），`/BaseFont` 由写端
    /// 换成下划线形式，其余条目按字面写出——与读端
    /// [`MultipleMasterFont::instance_name_with_spaces`](crate::core::reader::font::MultipleMasterFont::instance_name_with_spaces)
    /// 互逆。
    ///
    /// 比 Type 1 更严的必需项（构建期拒绝，不写出读端会报 `FontError::Invalid`
    /// 的文件）：
    /// - `/Widths`（连同 `/FirstChar`/`/LastChar`）必需：Table 111 的
    ///   「Required except for the standard 14 fonts」（raw 13862、13869、13876、
    ///   13891）只对 Type 1 生效——§9.6.2.2 的标准 14 名单（raw 13953–13956）
    ///   是「14 Type 1 fonts」，MM 实例不在其列，故 [`Font::name`] 取
    ///   `Helvetica` 之类的名字也不豁免。
    /// - `/FontDescriptor` 必需：同上，须显式给出 [`Font::descriptor`]，或由
    ///   [`Font::font_program`] 触发描述符写出。
    ///
    /// 嵌入程序按 §9.6.2.3 的快照要求（raw 14000–14002：MM 实例程序 shall 是
    /// 「an ordinary Type 1 font program, not a multiple master font program」）
    /// 只接受 Table 126 允许 `Type1 or MMType1` 的两种表示：`/FontFile`
    /// （[`FontProgram::Type1`]，raw 15529–15531）与 `/FontFile3`
    /// `/Subtype /Type1C`（[`FontProgram::CFF`]，raw 15536–15540）；
    /// [`FontProgram::TrueType`] 的 `/FontFile2`（raw 15532–15535 只列
    /// TrueType/CIDFontType2）与 [`FontProgram::OpenType`] 的
    /// `/FontFile3 /Subtype /OpenType`（续表 raw 15569–15572 只列
    /// Type1/CIDFontType0）都构建报错。
    ///
    /// 版本门：**不需要新的 `PdfFeature`**。Table 110（raw 13801–13810）把
    /// `MMType1`（raw 13807–13809）与 `Type1`/`Type3`/`TrueType` 并列且**没有**
    /// `(PDF 1.x)` 标注——对照同表 `Type0` 标注了 `(PDF 1.2)`（raw 13803）——
    /// 故 MMType1 自 PDF 1.0 起可用。
    ///
    /// 不写任何 MM 私有键：ISO 32000-1 未定义 MM 专有字体字典条目，设计轴
    /// （weight/width）、`/RS`、blend 机制属 Adobe TN 5015（读端亦未建模）。
    /// Type 0 的 `/DescendantFonts` 只接受 CIDFont（Table 121，raw
    /// 15097–15098），故 MM 实例不作为 composite 后代写出。
    ///
    /// [`FontProgram::Type1`]: super::FontProgram::Type1
    /// [`FontProgram::CFF`]: super::FontProgram::CFF
    /// [`FontProgram::TrueType`]: super::FontProgram::TrueType
    /// [`FontProgram::OpenType`]: super::FontProgram::OpenType
    pub mmtype1: bool,
}

impl Font {
    /// Helvetica 标准字体（默认 WinAnsiEncoding）。
    pub fn helvetica(size: f64) -> Self {
        Self {
            name: "Helvetica".to_string(),
            size,
            encoding: FontEncoding::default(),
            descriptor: None,
            font_program: None,
            subset: false,
            subset_glyphs: None,
            subset_type1_glyphs: None,
            subset_cff_glyphs: None,
            widths: None,
            type3: None,
            mmtype1: false,
        }
    }

    /// Multiple Master 实例字体（PDF 32000-1 §9.6.2.3，`/Subtype /MMType1`）。
    ///
    /// `instance_name` 是 MM 实例的 PostScript 名**原文**（可含空格，如
    /// `MinionMM 366 465 11 `）；写出的 `/BaseFont` 按规范把空格（20h）换成
    /// 下划线（5Fh），即 `/MinionMM_366_465_11_`（raw 13975–13977 与其后的
    /// EXAMPLE 2，raw 13980–13981）。除 `/Subtype` 与这条命名规则外，字典条目
    /// 与 Type 1 同构（Table 111），因此 `/Widths` 经 [`Font::with_widths`]、
    /// `/FontDescriptor` 经 [`Font::with_descriptor`]（或经
    /// [`Font::with_font_program`] 嵌入快照时自动写出）、`/Encoding` 经
    /// [`Font::with_encoding`] 配置，快照程序只接受 [`FontProgram::Type1`]
    /// （`/FontFile`）与 [`FontProgram::CFF`]（`/FontFile3 /Type1C`）；
    /// 缺 `/Widths` 或缺 `/FontDescriptor`、或嵌入其他格式程序都会在
    /// [`PdfBuilder::build`](super::super::PdfBuilder::build) 阶段报
    /// [`WriteError::InvalidObject`](crate::core::writer::WriteError::InvalidObject)
    /// 且不写出任何字节。
    ///
    /// MMType1 自 PDF 1.0 起可用（Table 110 raw 13807–13809 无 `(PDF 1.x)`
    /// 标注），因此不登记新的 `PdfFeature` 门禁。
    ///
    /// [`FontProgram::Type1`]: super::FontProgram::Type1
    /// [`FontProgram::CFF`]: super::FontProgram::CFF
    pub fn mmtype1(size: f64, instance_name: &str) -> Self {
        Self {
            name: instance_name.to_string(),
            size,
            encoding: FontEncoding::default(),
            descriptor: None,
            font_program: None,
            subset: false,
            subset_glyphs: None,
            subset_type1_glyphs: None,
            subset_cff_glyphs: None,
            widths: None,
            type3: None,
            mmtype1: true,
        }
    }

    /// Type 3 字体（PDF 32000-1 §9.6.5）。
    ///
    /// `/Encoding` 取自各 [`Type3Glyph`] 的字符码与名称，写成只含
    /// `/Differences` 的编码字典；因此 [`PdfBuilder::text`] 可像普通
    /// 简单字体一样按字符查名转码。`name` 固定为 `Type3`（Type 3 字体
    /// 没有 `/BaseFont`）；`/Name`、`/ToUnicode` 与 `/Resources` 由
    /// [`Type3Font`] 提供，`/FontDescriptor` 可经 [`Font::with_descriptor`]
    /// 添加（Tagged PDF 要求）。
    ///
    /// [`PdfBuilder::text`]: super::super::PdfBuilder::text
    pub fn type3(size: f64, font: Type3Font) -> Self {
        let differences = font
            .glyphs
            .iter()
            .map(|glyph| (glyph.code, glyph.name.clone()))
            .collect();
        Self {
            name: "Type3".to_string(),
            size,
            encoding: FontEncoding::with_differences(None, differences),
            descriptor: None,
            font_program: None,
            subset: false,
            subset_glyphs: None,
            subset_type1_glyphs: None,
            subset_cff_glyphs: None,
            widths: None,
            type3: Some(font),
            mmtype1: false,
        }
    }

    /// 设置简单字体的 `/Widths`（链式构造）。
    ///
    /// `widths[i]` 是字符码 `first_char + i` 的宽度（1/1000 text space
    /// units），字符码范围必须是单字节空间内的连续区间。构建时校验：
    /// 列表非空、`first_char + widths.len() <= 256`、每个宽度有限且
    /// 非负，否则 [`PdfBuilder::build`](super::super::PdfBuilder::build)
    /// 返回 [`WriteError::InvalidObject`](crate::core::writer::WriteError::InvalidObject)。
    pub fn with_widths(mut self, first_char: u8, widths: &[f64]) -> Self {
        self.widths = Some(FontWidths {
            first_char,
            widths: widths.to_vec(),
        });
        self
    }

    /// 替换编码配置（链式构造）。
    pub fn with_encoding(mut self, encoding: FontEncoding) -> Self {
        self.encoding = encoding;
        self
    }

    /// 添加字体描述符（链式构造）。
    pub fn with_descriptor(mut self, descriptor: FontDescriptor) -> Self {
        self.descriptor = Some(descriptor);
        self
    }

    /// 嵌入字体程序（链式构造）。
    ///
    /// [`FontProgram::OpenType`] 按 SFNT 版本标签处理：CFF 轮廓（`OTTO`）
    /// 写 `/FontFile3` `/Subtype /OpenType` 且字体子类型保持 Type1；
    /// TrueType 轮廓（`0x00010000`/`true`）按 ISO 32000-1 §9.9 写
    /// `/FontFile2` 并把字体子类型切为 TrueType。
    pub fn with_font_program(mut self, program: FontProgram) -> Self {
        self.font_program = Some(program);
        self
    }

    /// 标记为子集字体（链式构造）。
    ///
    /// 写出时若已嵌入字体程序，`/BaseFont` 会加上由程序内容确定的
    /// 六字母标签前缀，如 `ABCDEF+Helvetica`（PDF 32000-1 §9.6.4）。
    /// 未嵌入程序时该标记不产生前缀。
    pub fn subset(mut self) -> Self {
        self.subset = true;
        self
    }

    /// 按 TrueType 字形子集嵌入字体程序（链式构造）。
    ///
    /// `glyphs` 是原程序中的字形 ID（gid）列表；gid 0 与复合字形的组件
    /// 闭包总会保留，其余字形在子集程序中被重新编号。该方法同时把
    /// [`Font::subset`] 置为 `true`，因此 `/BaseFont` 会带六字母子集标签
    /// 前缀。未嵌入程序或程序不是 [`FontProgram::TrueType`] 时，
    /// [`PdfBuilder::build`](super::super::PdfBuilder::build) 返回
    /// [`WriteError::InvalidObject`](crate::core::writer::WriteError::InvalidObject)。
    pub fn with_subset_glyphs(mut self, glyphs: &[u16]) -> Self {
        self.subset_glyphs = Some(glyphs.to_vec());
        self.subset = true;
        self
    }

    /// 按 Type 1 字形名子集嵌入字体程序（链式构造）。
    ///
    /// `glyphs` 是原程序 `/CharStrings` 中的字形名；`.notdef` 与保留
    /// 字形的 `seac` 组件闭包总会保留，其余字形在子集程序中删除。
    /// 该方法同时把 [`Font::subset`] 置为 `true`，因此 `/BaseFont` 会带
    /// 六字母子集标签前缀（标签仍按原始程序字节计算）。
    ///
    /// 与 [`Font::subset_glyphs`] 互斥。未嵌入程序或程序不是
    /// [`FontProgram::Type1`] 时，
    /// [`PdfBuilder::build`](super::super::PdfBuilder::build) 返回
    /// [`WriteError::InvalidObject`](crate::core::writer::WriteError::InvalidObject)。
    pub fn with_subset_type1_glyphs(mut self, glyphs: &[&str]) -> Self {
        self.subset_type1_glyphs = Some(glyphs.iter().map(|name| (*name).to_string()).collect());
        self.subset = true;
        self
    }

    /// 按 name-keyed CFF 字形名子集嵌入字体程序（链式构造）。
    ///
    /// `glyphs` 是原程序 charset 中的字形名；`.notdef` 总会保留，其余
    /// 未请求的字形在子集程序中删除，保留字形按原 GID 升序重编号。
    /// 该方法同时把 [`Font::subset`] 置为 `true`，因此 `/BaseFont` 会带
    /// 六字母子集标签前缀（标签仍按原始程序字节计算）。
    ///
    /// 与 [`Font::subset_glyphs`]、[`Font::subset_type1_glyphs`] 互斥。
    /// 未嵌入程序、程序不是 [`FontProgram::CFF`] 或请求了 charset 中不
    /// 存在的字形名时，
    /// [`PdfBuilder::build`](super::super::PdfBuilder::build) 返回
    /// [`WriteError::InvalidObject`](crate::core::writer::WriteError::InvalidObject)。
    pub fn with_subset_cff_glyphs(mut self, glyphs: &[&str]) -> Self {
        self.subset_cff_glyphs = Some(glyphs.iter().map(|name| (*name).to_string()).collect());
        self.subset = true;
        self
    }

    /// 字体去重键是否一致。
    ///
    /// **对称、参数顺序无关**：既有的 `same_font`（`builder/mod.rs`）是纯粹
    /// 的逐字段 `==` 合取（`name`、`size` 位模式、`encoding`、`font_program`、
    /// `subset`、三种子集字形列表、`widths`、`type3`），本方法只在其后追加
    /// `self.mmtype1 == other.mmtype1`，因此
    /// `a.same_dedup_key(b) == b.same_dedup_key(a)`；池在「已注册字体」一侧、
    /// 候选在「新来字体」一侧的既有调用点（`assemble/alloc_pools.rs`、
    /// `assemble/alloc_forms.rs`、`assemble/acro_form.rs`、
    /// `assemble/graphics_state.rs`、`types/reversed_chars.rs`、
    /// `page.rs`、`marked_content.rs`）无论写成
    /// `pooled.same_dedup_key(candidate)` 还是
    /// `candidate.same_dedup_key(pooled)` 都命中同一结果，去重语义与原
    /// `same_font` 完全一致，只是额外多比 MM 标记。
    ///
    /// 用途：供跨页字体池**与**页内 `/F<n>` 字体资源列表使用，避免 MM 实例与
    /// 同名、同编码、同宽度的 Type 1 字体共用一个字体对象，导致 `/Subtype`
    /// 只跟随先注册者（Table 110 里两者是不同的字体类型）。
    pub(crate) fn same_dedup_key(&self, other: &Self) -> bool {
        super::super::same_font(self, other) && self.mmtype1 == other.mmtype1
    }
}
