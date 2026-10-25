# PDF 库开发路线图（ROADMAP）

> 目的：把 `docs/FEATURE_MATRIX.md` 中的 ⬜/🟡 条目按价值与依赖组织为有序开发阶段，并定义进度跟踪方式。
> 与矩阵的关系：矩阵是事实层（现状），本文档是规划层（排期）；条目一律用矩阵章节号引用。

## 进度跟踪约定

### 状态机

- 单条目的状态流转：`⬜ → 🚧（进行中，可选标注于备注）→ ✅`。
- 允许 `⬜ → ❌`：评审后决定不支持，须在矩阵备注中注明理由。
- 禁止 `✅ → ⬜` 回退；除非实现被移除，且须在矩阵备注中注明原因。
- 状态只在代码实测后修改，禁止凭印象改动。

### 「完成」定义（三齐）

一个条目标记 ✅ 必须同时满足：

1. **代码实现合入**：功能代码进入主分支；
2. **有对应测试**：测试名写入矩阵该行的证据列；
3. **文档同步**：矩阵状态/证据/总览统计同步更新，必要时更新 README 版本特性速查表。

三者缺一，该条目不得标 ✅（至多标 🚧 或 🟡）。

### 更新流程

每次开发迭代（对应一个 spec/PR）完成时，按顺序执行：

1. 回写矩阵对应行的状态与证据；
2. 重算矩阵「总览」统计，使四态计数与表内一致；
3. 勾选本文件对应阶段条目的复选框；
4. 当某阶段全部条目 ✅ 时，更新该阶段状态。

阶段状态同样四态：⬜ 未开始 / 🚧 进行中 / ✅ 完成 / ❌ 放弃。

### 工程约定（依赖、扩展点与失败语义）

以下约定面向全库；条目中的现有模块只是当前实例，同类新增实现同样适用。

1. **第三方库必须隔离在 PDF 语义 trait/适配器之后**：
    - 代码实现方案优先级：rust标准库>成熟第三方库/本地实现
    - 需要在实现内部使用第三方能力时，禁止直接调用第三方库，需要先定义稳定的 trait，再由适配器实现（现有示例：`filters::InflateBackend` 封装 `flate2`）;
2. **扩展优先于修改核心调度**：新增能力应通过既有扩展点（trait、注册表、可注入后端）接入，核心调度逻辑保持稳定；不得为单个实现修改调度分支。现有实例：过滤器经 `filters::StreamFilter` + `builtin_filters` 注册，`decode_stream` 不改动，`decode_stream_with` 支持注入/覆盖；字体程序、函数、安全处理器等新增同类能力时，先定义或复用 trait 与注册入口。
3. **抽象须可替换、可验证**：每个扩展点（trait、适配器、注入式后端）都要有替代实现的测试，证明核心只依赖抽象而非具体实现；新增扩展点时同步交付该测试，替代实现可以是自定义模拟实现或不同后端。
4. **边界失败语义统一**：所有解析、解码与安全边界对损坏、截断或非法输入返回类型化错误（如 `DecodeError`、`FontError`、`ParseError`），不得 panic；任何宽容恢复或降级行为必须显式实现、有测试覆盖并记录在矩阵备注中，禁止隐式吞错。
5. **模块拆分约定**：单个 `.rs` 文件超过约 600 行或 25 KiB，或同时承担多个职责时，必须解构为目录模块（`foo/mod.rs` + 子模块）；编解码器实现、码表、测试应分文件；既有超限模块列入下方「工程债」逐项拆分，新增功能不得继续扩大超限文件。

## 工程债：模块目录化

已完成拆分：

- [x] `filters`：`filters/{mod,ascii,flate,lzw,predictor,tests}.rs`（原 1200+ 行单文件）
- [x] `ccitt`：`ccitt/{mod,decoder,tables,tests}.rs`（原 1000+ 行单文件）
- [x] `reader`：`reader/{mod,lexer,object,parser,document,xref,tests}.rs`（原约 2076 行 / 81 KiB）
- [x] `writer`：`writer/{mod,value,version,serialize,context,document_writer,tests}.rs`（原约 1099 行 / 43 KiB）
- [x] `types`：`types/{mod,version,features,value,document,tests}.rs`（原约 910 行 / 32 KiB）
- [x] `content`：`content/{mod,types,serialize,stream,tests}.rs`（原约 1060 行 / 34 KiB）
- [x] `builder`：`builder/{mod,types,assemble,tests}.rs`（原 842 行 / 31 KiB；公共类型、`Document` 组装与测试分文件）
- [x] `type1_program/charstring`：`charstring.rs` + `charstring/tests.rs`（527 行测试移出后实现文件 527 行 / 17 KiB；CharString 解释与测试分文件）
- [x] `reader/font`：`font/{mod,descriptor,encoding,simple,tests}.rs`（原 923 行 / 33.4 KiB；描述符指标、简单字体编码、Type 1 指标与测试分文件）
- [x] `reader/text`：`text/{mod,tokenizer,tests}.rs`（原 889 行 / 29.1 KiB；内容流分词器与测试移出，`mod.rs` 约 400 行）

`src/core` 根模块与上述条目均低于约 600 行 / 25 KiB。以下嵌套与集成测试模块仍超限，后续增量触及前必须先拆分或同步拆分，不得继续扩大：

- [ ] `reader/security/standard.rs`（998 行 / 35.2 KiB）
- [ ] `reader/font/type1_program/mod.rs`（960 行 / 34.1 KiB）
- [ ] `reader/font/cid_cmap.rs`（918 行 / 32.6 KiB）
- [ ] `reader/font/truetype/glyph.rs`（816 行 / 29.6 KiB）
- [ ] `reader/tests.rs`（689 行 / 26.1 KiB）
- [ ] `tests/main.rs`（125.6 KiB；集成测试）

## 阶段总览

| 阶段 | 目标 | 条目数 | 主要依赖 | 状态 |
|---|---|---|---|---|
| A 读得全 | 核心语法补全，真实世界 PDF 读入不失败 | 8 | 无 | ✅ |
| B 文本可用 | 字体与文本提取 | 12 | A（Functions 为软依赖） | 🚧 |
| C 图形完整 | 色彩/Pattern/Form/透明度规范完整 | 16 | A（Functions）、B（部分字体度量） | 🚧 |
| D 交互与交换 | 导航/注解/表单/元数据基础可用 | 9 | C | ⬜ |
| E 专门领域 | 需求驱动，不预设排期 | 15 | D | ⬜ |

## 阶段 A：读得全 —— 核心语法补全

**目标**：真实世界 PDF 读入不再因过滤器/加密/函数缺失而失败（对应矩阵 P0 缺口）。

**包含条目**：

核心验收清单共 12 项。`9.6.5` Type 3 字体虽有读写与文本提取实现，但仍缺 CharProcs 执行与渲染，按 P2 扩展支线跟踪，不计入阶段 B 的核心验收项；其状态以 [FEATURE_MATRIX.md](./FEATURE_MATRIX.md) 为准。

- [x] `7.4.4` LZWDecode（已完成：9–12 位码宽、CLEAR/EOD、KwKwK、`/EarlyChange`、Predictor、截断宽容；测试见矩阵证据）
- [x] `7.4.6` CCITTFaxDecode
  - [x] 位流与解码器基础设施：MSB-first 位读取、白/黑游程表、2D 模式表、按行打包输出
  - [x] Group 3 1-D（`K=0`，Modified Huffman）与 EOL/填充位容忍
  - [x] Group 3 2-D（`K>0`，EOL 后 1-D/2-D 标志位）
  - [x] Group 4（`K<0`，T.6 纯二维，EOFB/RTC 终止）
  - [x] 参数：`/Columns` `/Rows` `/EndOfLine` `/EncodedByteAlign` `/EndOfBlock` `/BlackIs1` `/DamagedRowsBeforeError`
  - [x] 测试：手工码流（1-D/2-D/G4）+ libtiff 编码样本交叉验证 + 损坏/截断负向测试
- [x] `7.4.8` DCTDecode（JPEG，同上）
  - 已定义 PDF 语义的 `JpegBackend` trait，由 `ZuneJpegBackend` 适配器接入 `zune-jpeg 0.5`；Adobe APP14 CMYK 取反与 YCCK 逆变换在 PDF 语义层自实现；crates.io 镜像（`.cargo/config.toml` rsproxy sparse）已就绪，依赖可拉取。测试：4 色彩路径 + progressive + 损坏负向 + writer 过滤器回归 |
- [x] `7.4.10` Crypt 过滤器（已完成：密码认证后按对象号/generation 解密 `/Crypt`；Identity 与 R4 AESV2 具名过滤器覆盖端到端测试；writer 不支持加密输出）
- [x] `7.6.3` 标准安全处理器解密（已完成：ISO 32000-1 R2–R4，RC4-40/128、AESV2、用户/所有者密码、字符串/流与 `/EncryptMetadata false`；第三方密码原语封装在 `CryptoBackend` 适配器后。测试见矩阵证据。PDF 2.0 R5/R6、公钥安全处理器及加密写出不在本项支持范围）
- [x] `7.10` Functions（Type 0/2/3/4；已完成：`functions/` 目录模块，纯 Rust 语义层求值 API，Type 0 采样多线性插值 / Type 2 指数插值 / Type 3 拼接 / Type 4 PostScript 计算器子集；测试见矩阵证据。`/Order 3` 三次插值暂不支持；写端随阶段 C Shading 落地）
- [x] `附录 F` 线性化容忍验证（已完成：reader/tests.rs 手工构造线性化样本集——经典 xref 与 XRef 流 + ObjStm 两个变体、/H 两种合法形态、损坏 hint 流负向用例；线性化参数字典 /L /H /E /T 按实际布局定点迭代取真值。读端不利用 hint 流、不解读线性化字典，全部样本按普通 PDF 路径解析无错误、无告警回退，全部对象可解析、流可解码；测试见矩阵证据。写端线性化输出列入阶段 E）
- [x] `7.2` 词法缺口：Lexer 空白集补 0x00（已完成：`Lexer::is_whitespace` 含 NUL，与 filters 对齐；测试 `nul_byte_treated_as_whitespace`）

**依赖**：无（起始阶段）。

**验收标准**：

- 构造/收集含 LZW、CCITT、DCT、Crypt 过滤器的 PDF 样本，可完整解析且解码字节正确；
- 线性化 PDF 样本集按普通路径读取无错误；
- `7.6.3` 已按 ISO 32000-1 实现 R2–R4；PDF 2.0 R5/R6 与公钥处理器作为未实现范围明确记录；
- writer 对加密文档明确返回错误，不输出解密内容并保留原 `/Encrypt` 字典的无效文档；
- `cargo test` 全绿，新增过滤器均有正/负向测试。

## 阶段 B：文本可用 —— 字体与文本

**目标**：能正确读写含 TrueType/CID 字体的文档并提取文本（对应矩阵 P1）。

**包含条目**：

- [ ] `9.6.2` Type 1 简单字体补全（进行中：读端已能解析直接/间接 `/Widths` 与 `/FontDescriptor` 指标，并为 Courier、Helvetica/Times 四种样式、Symbol、ZapfDingbats 提供对应内建编码宽度；`Document::type1_program` 可解析嵌入 `/FontFile` 程序的明文头、内建编码、eexec 段与解密后的 CharStrings/Subrs；`Type1FontProgram::glyph` 解释 CharString 数值编码与路径操作符（hsbw/sbw、moveto/lineto/curveto、closepath、callsubr/return、div），并把 seac 经 StandardEncoding 字符码递归展开为基字形与平移后的重音字形轮廓（嵌套上限 10），输出字宽、side bearing 与绝对坐标路径；flex 的 `callothersubr`/`pop` 序列按七点协议展开为两段三次曲线（进入/退出可跨越 subr 边界，`pop pop setcurrentpoint` 收尾，支持 rmoveto/hmoveto/vmoveto 点形式；提示替换 `callothersubr 3` 按 OtherSubrs 语义读取单个 hint code 并登记一个待 `pop` 结果，hint 本身不应用，参数个数、缺少 `pop`、flex 期间调用等错误明确报出，其他过程明确报错）；hinting/渲染、字体嵌入与子集化仍待实现）
- [ ] `9.6.3` TrueType 字体（进行中：读端解析 SFNT 目录、基础度量表、Unicode cmap format 0/2/4/6/12/13 和 loca/glyf 字形记录，可查询水平 advance、Unicode→glyph ID，并递归展开复合字形及其仿射变换/点对齐；简单字形轮廓端点、指令、标志与坐标解码已实现；`vhea`/`vmtx` 垂直度量已解析，`TrueTypeFont::vertical_advance` 与 `top_side_bearing` 支持 numberOfVMetrics 末尾复用规则与尾部 TSB 数组，缺失表时返回 `None`；字体与 CIDFontType2 的集成见 9.7.4；`Document::simple_font_encoding` 已覆盖 TrueType/MMType1/Type1 的 `/Encoding`（预定义名、`/BaseEncoding`、`/Differences`），`reader/text` 据此完成 TrueType 简单字体的 Unicode 文本提取；`TrueTypeFont::unicode_for_glyph` 支持反转 cmap 查询；`TrueTypeFont::builtin_encoding` 一次性构建 256 项单字节内建编码表（字符码→cmap glyph→反向 Unicode），`reader/text` 在 TrueType 简单字体缺 `/Encoding` 且嵌入 `/FontFile2` 可解析时优先使用该表，程序缺失/非法时回退预定义编码；cmap 选择支持 Microsoft Symbol `(3,0)` 子表（format 0/4/6/12/13，优先级低于所有 Unicode 子表），符号字体直查失败时依次按 `0xF000`/`0xF100`/`0xF200` 偏移回退，`reader/text` 优先采用非 PUA 内建字符、PUA 字符让位于预定义编码；format 2 高字节映射与 format 13 多码对一也已解析（format 13 反向映射取最小码）；读端 `Document::truetype_font` 同时接受 `/FontFile3` `/Subtype /OpenType`（与 `/FontFile2` 共用程序解析；缺 `/Subtype` 或为 `CIDFontType0C` 时明确报错）：SFNT 目录新增 `OTTO` flavor，`loca`/`glyf` 同时缺失时按 CFF-flavored OpenType 处理——仍解析 `cmap`/`hmtx`/`vhea`/`vmtx` 供文本提取与度量查询，`glyph` 轮廓返回 `None`，二者只有其一存在则报错；测试 `parses_cff_flavored_opentype_without_glyph_tables`、`opentype_without_glyph_tables_exposes_vertical_metrics`、`rejects_opentype_without_cmap`、`rejects_opentype_with_only_one_glyph_table`、`rejects_truetype_flavor_without_glyph_tables`、`truetype_font_accepts_opentype_fontfile3_program`、`truetype_font_still_accepts_fontfile2_program`、`truetype_font_rejects_fontfile3_type1c_program`、`truetype_font_rejects_fontfile3_program_without_subtype`；写端仍只经 `FontProgram::TrueType` 嵌入 `/FontFile2` 并自动使用 `/Subtype /TrueType`；CFF 表内容未校验、该路径不解析轮廓；测试 `builtin_encoding_maps_codes_through_cmap`、`selects_symbol_only_format4_cmap`、`prefers_unicode_format4_over_symbolic_format4`、`builtin_encoding_maps_codes_through_symbolic_cmap`、`builtin_encoding_maps_codes_through_f100_symbolic_cmap`、`builtin_encoding_maps_codes_through_f200_symbolic_cmap`、`builtin_encoding_prefers_f000_offset_over_f100`、`builtin_encoding_ignores_offsets_for_unicode_cmap`、`maps_high_byte_subheaders_with_format2`、`maps_many_codes_to_one_glyph_with_format13`、`prefers_format12_over_format13`、`prefers_format4_over_format2`、`private_use_builtin_loses_to_encoding`、`truetype_without_encoding_falls_back_when_program_is_missing`、`truetype_without_encoding_falls_back_when_program_is_invalid`、`truetype_without_encoding_extracts_through_builtin_cmap`；hinting 与渲染仍待实现）
- [ ] `9.6.4` 字体子集化（进行中：写端 `FontProgram::subset_tag` 按程序字节做 FNV-1a 哈希并按 26 进制展开为确定性六字母标签，`Font::subset` 标记后 `assemble_document` 把 `/BaseFont` 写为 `TAG+Name`；未嵌入程序时标记不产生前缀，字体去重键纳入程序与子集标志。读端 `strip_subset_tag` 剥离六字母前缀，`default_encoding`、标准 14 判定与内建字宽回退均按剥离后的名称工作。测试 `subset_font_prefixes_base_font_with_six_letters`、`subset_tag_is_deterministic_and_content_dependent`、`subset_without_font_program_keeps_base_font`、`strip_subset_tag_removes_only_six_letter_prefixes`、`subset_prefix_keeps_default_encoding_and_standard14_metrics`、`subset_font_roundtrips_through_parser`。另新增独立 TrueType 子集化工具 `reader::font::subset_truetype(data, glyph_ids)`（`reader/font/truetype/subset.rs`）：始终保留 glyph 0、递归纳入复合组件闭包、按旧 gid 升序重编号；重写 `maxp`/`loca`/`glyf`（patch 组件 glyphIndex）/`hmtx`/`hhea`/`cmap`/`head`（indexToLocFormat 与 checkSumAdjustment），重建 4 字节对齐目录与表校验和；`cmap` 始终保留 format 4（仅存活 BMP 码，Unicode BMP 或 Microsoft Symbol 编码），源 cmap 为 format 12/13 且补充平面映射仍存活时追加 format 12 子表（Windows 全字符面 `(3,10)`，符号字体 `(3,0)`；format 12 重复 BMP 条目以保证首选子表可独立查询）；源同时含 `vhea`/`vmtx` 时按新 gid 重建垂直度量（`numberOfVMetrics = min(源值, 子集字形数)`，重新生成尾部 TSB 数组，并按子集保留字形重算 `advanceHeightMax`/`minTopSideBearing`/`minBottomSideBearing`/`yMaxExtent` 四个汇总字段——逐字形读重建后 vmtx 与 glyf 头 yMin/yMax，空字形按零 bbox，越界饱和 int16），缺一即丢弃；保留 head/hhea/maxp/hmtx/cmap/loca/glyf/vhea/vmtx/name/OS/2/cvt /fpgm/prep/gasp，丢弃 post/kern/GDEF/GSUB/GPOS 等旧 gid 敏感表。测试 `subsets_simple_glyph_and_remaps_cmap`、`includes_composite_component_closure`、`rejects_out_of_range_glyphs_and_handles_empty_request`、`rebuilds_aligned_directory_and_checksums`、`writes_format4_segments_and_search_parameters`、`drops_tables_that_depend_on_original_glyphs`、`preserves_short_loca_format`、`preserves_symbolic_cmap_encoding`、`supports_maxp_version_half`、`preserves_supplementary_cmap_in_format12`、`omits_supplementary_mappings_for_dropped_glyphs`、`preserves_vertical_metrics_for_subset_glyphs`、`drops_incomplete_vertical_metrics`、`rejects_malformed_vertical_metrics`、`recomputes_vertical_summary_fields_from_retained_glyphs`、`recomputes_vertical_summaries_for_empty_request`、`ignores_removed_advances_when_recomputing_vertical_summaries`。另新增 Type 1 子集化工具 `reader::font::subset_type1(data, glyph_names)`（`reader/font/type1_program/subset.rs`）：保留 `.notdef` 与请求字形，删除其余 `/CharStrings` 条目（按名称到终结符的完整字节区间），保留明文头、Private 字典其余条目与全部 `/Subrs`，仅重新加密 `eexec` 层（确定性 4 字节零前缀、R=55665），再补 512 零守卫与 `cleartomark`；保留字形按 `seac` 操作数经 StandardEncoding 递归补齐基/重音组件闭包（含嵌套），只解释被保留字形，组件名无法映射或组件缺失时报错；未知字形、空 `/CharStrings`、条目无法安全定位时报 `FontError::Invalid`。测试 `subset_keeps_notdef_and_requested_glyph`、`subset_preserves_cleartext_header`、`subset_preserves_private_dictionary_entries`、`rejects_unknown_glyph`、`rejects_program_with_empty_charstrings`、`empty_request_keeps_only_notdef`、`subset_output_is_smaller_than_source`、`subsets_unencrypted_len_iv_minus_one`、`malformed_inputs_return_errors_without_panicking`、`subsets_seac_component_closure`、`rejects_seac_component_missing_from_charstrings`、`dropped_seac_glyph_does_not_require_components`、`subsets_nested_seac_component_closure`。写端 builder 已接入 TrueType 裁剪：`Font::with_subset_glyphs(&[gid])` 置子集标记并携带字形列表，`assemble_document` 对 `FontProgram::TrueType` 调用 `subset_truetype` 后写 `/FontFile2`（`/Length` 与流内容均为裁剪结果，`/BaseFont` 标签仍按原程序哈希）；非 TrueType 程序、缺少程序或越界 gid 返回 `WriteError::InvalidObject`，字体去重键纳入字形列表。测试 `subset_glyphs_rewrite_truetype_fontfile2`、`subset_glyphs_reject_non_truetype_programs`、`subset_glyphs_without_font_program_is_invalid`、`subset_glyphs_invalid_glyph_id_is_invalid_object`、`subset_glyph_lists_do_not_deduplicate_fonts`、集成往返 `builder_truetype_subset_roundtrips_through_parser`（裁剪后 glyph_count 与映射恢复、字体程序变短、文本提取还原）。Type 1 子集已接入 builder：`Font::with_subset_type1_glyphs(&[name])` 携带字形名列表（与 `subset_glyphs` 互斥），`assemble_document` 对 `FontProgram::Type1` 调用 `subset_type1` 后写 `/FontFile`；无程序/非 Type 1 程序/未知字形名/两类子集混用返回 `WriteError::InvalidObject`，去重键纳入字形名列表；测试 `type1_subset_glyphs_rewrite_fontfile`、`type1_subset_glyphs_without_font_program_is_invalid`、`type1_subset_glyphs_reject_non_type1_programs`、`type1_subset_glyphs_unknown_glyph_is_invalid_object`、`subset_glyphs_and_type1_subset_glyphs_cannot_be_combined`、`type1_subset_glyph_lists_do_not_deduplicate_fonts`、集成往返 `builder_type1_subset_roundtrips_through_parser`。CFF 子集工具 `reader::font::subset_cff(data, cids)`（`cff/subset/{mod,dict}.rs`）已实现并接入 builder：仅支持 CID-keyed CFF（含 ROS；非 CID-keyed、预定义 charset 0..=2、Top DICT 层 Private 按错误拒绝），CID 0 恒保留、GID 按保留 CID 升序重编号，charset 重写为 format 0、FDSelect 重写为 format 3、FDArray 只保留被引用 FD 并重编号，保留 FD 的 Private/Local Subrs 整组保留但重写内部偏移，Name/String/Global Subr INDEX 整组保留（subr 编号保持有效），Top DICT 偏移按新布局重算、未触及条目逐字节拷贝；`CidFont::with_subset_cids(&[u16])` 对 CIDFontType0 + `FontProgram::CFF` 写裁剪后的 `/FontFile3`，Type 0 与 CIDFont 的 `/BaseFont` 一致加六字母标签，描述符已提供的 `/CIDSet` 按子集交集裁剪（CID 0 恒保留），与 `with_predefined_encoding` 可共存；测试 `subset_keeps_only_notdef_for_empty_request`、`subset_retains_requested_cids_and_renumbers_gids`、`subset_drops_unreferenced_font_dicts`、`subset_preserves_subroutines_and_outlines`、`subset_rejects_unknown_cid`、`subset_rejects_out_of_range_cid`、`subset_rejects_non_cid_keyed_programs`、`subset_rejects_cid_keyed_program_with_predefined_charset`、`subset_does_not_panic_on_malformed_input`、`subset_handles_fd_select_format_0_source`、`cid_subset_rewrites_fontfile3_with_tagged_base_font`、`cid_subset_cids_require_cidfonttype0`、`cid_subset_cids_require_cff_program`、`cid_subset_unknown_cid_is_invalid_object`、`cid_subset_cids_do_not_deduplicate_fonts`、`cid_subset_trims_cid_set`、`cid_subset_coexists_with_predefined_encoding`、集成往返 `builder_cid_font_type0_subset_roundtrips_through_reader`。缺口：CFF2 子集化未实现；name-keyed（非 CID-keyed）CFF 子集工具 `reader::font::subset_cff_type1c(data, glyph_names)`（`cff/subset/{type1c,strings}.rs`）已实现并接入 builder——GID 0 恒保留、按原 GID 升序重编号，字形名经 charset（GID→SID）与 String INDEX 解析，支持预定义 charset 0/1/2、缺省 ISOAdobe 与显式 format 0/1/2，charset 统一重写为 format 0 并保留 SID，CharStrings 仅含保留字形，Top DICT 的 charset/Private/CharStrings 偏移重算，Private/Local Subrs 整组保留（Subrs 相对偏移重写），Global Subr/Name/String INDEX 整组保留，Top DICT `/Encoding` 删除（重编号后旧 code→GID 失效），带 ROS/FDArray/FDSelect 的程序拒绝并提示改用 `subset_cff`；`Font::with_subset_cff_glyphs(&[name])` 对 `FontProgram::CFF` 写裁剪后的 `/FontFile3`（`/Subtype /Type1C`），三类子集列表互斥、非 CFF 程序/无程序/未知字形名报 `WriteError::InvalidObject`；测试 `type1c_subset_keeps_notdef_for_empty_request`、`type1c_subset_retains_named_glyphs_in_source_gid_order`、`type1c_subset_rewrites_charset_to_format_zero`、`type1c_subset_output_reparses_with_raw_layer`、`type1c_subset_output_is_smaller`、`type1c_subset_drops_top_dict_encoding`、`type1c_subset_handles_explicit_charset_format_zero`、`type1c_subset_handles_explicit_charset_format_one`、`type1c_subset_handles_explicit_charset_format_two`、`type1c_subset_handles_predefined_iso_adobe_charset`、`type1c_subset_handles_predefined_expert_charset`、`type1c_subset_handles_predefined_expert_subset_charset`、`type1c_subset_defaults_missing_charset_to_iso_adobe`、`type1c_subset_preserves_local_subrs`、`type1c_subset_rejects_unknown_glyph_name`、`type1c_subset_rejects_cid_keyed_program`、`type1c_subset_does_not_panic_on_malformed_input`、`cff_subset_glyphs_rewrite_fontfile3`、`cff_subset_glyphs_require_cff_program`、`cff_subset_glyphs_unknown_glyph_is_invalid_object`、`cff_subset_glyphs_cannot_be_combined_with_other_subset_lists`、`cff_subset_glyph_lists_do_not_deduplicate_fonts`；预定义 charset 的 CID-keyed 程序与 Top DICT 层 Private 仍未支持；字形列表需调用方提供（未按文本自动收集）；Type 1 子集保持 `/CharStrings N dict` 容量值不变（PostScript 语义合法））
- [ ] `9.6.5` Type 3 字体（进行中：读端 `Document::type3_font` 解析 `/FontBBox`、`/FontMatrix`、`/CharProcs`（字形程序名集合）、`/FirstChar`/`/LastChar`/`/Widths`（原始 glyph-space 宽度，`width_for` 按码查询）、`/Encoding` 与 `/Resources`（Table 112；解析期原样保存直接字典或间接引用，`Type3Font::resources(document)` 查询时解引用一层返回 `Option<Dict>`，缺失为 `None`，非字典或不可解析报 `FontError::Invalid`）；`Type3Font::glyph_metrics` 按需执行 CharProcs 首操作符（须为 `d0`/`d1`），返回声明宽度与可选 glyph 空间 bbox，过滤器/间接流走标准解码路径，畸形流报 `FontError::Invalid`，编码未分配字形名返回 `None`；`/Widths` 与 d0/d1 宽度为独立数据源不做一致性校验；测试 `glyph_metrics_reads_d0_declared_width`、`glyph_metrics_reads_d1_declared_width_and_bbox`、`glyph_metrics_coexists_with_widths_entry`、`glyph_metrics_decodes_filtered_and_indirect_char_procs` 及六项负向用例。`Document::simple_font_encoding` 接受 Type3（缺 `/Encoding` 报错；Encoding 字典缺 `/BaseEncoding` 时以 StandardEncoding 近似），`reader/text` 经该编码解码 Type3 文本。测试 `type3_font_parses_font_matrix_bbox_widths_and_char_procs`、`type3_font_requires_char_procs`、`extracts_type3_differences_text`、`extracts_type3_predefined_encoding_text`、`type3_encoding_without_base_encoding_uses_differences`。写端新增 `builder::Type3Glyph`（字符码、字形名、glyph-space 宽度、CharProcs 字节）与 `builder::Type3Font`（`/FontBBox` 与 `/FontMatrix` 默认值/链式设置、字形列表），`Font::type3(size, font)` 以字形列表构造 `/Differences` 编码；`assemble_document` 写出 `/Subtype /Type3` 字体字典与各 CharProcs 流（对象号按字形列表顺序分配），`/Widths` 覆盖 FirstChar..=LastChar 且缺号写 0，不含 `/BaseFont`、`/FontDescriptor` 与字体程序；`Type3Font::with_resources(Value)` 可设置 `/Resources`（直接字典或间接引用原样写出，其他类型报 `InvalidObject`，未设置时不写该条目）；嵌入字体程序、请求 `subset_glyphs`、空字形列表、重复字符码/名称或缺失 `/Encoding` 时 `build` 返回 `WriteError::InvalidObject`。测试 `type3_font_writes_char_procs_widths_and_encoding`、`type3_font_requires_glyphs`、`type3_font_rejects_duplicate_codes_or_names`、`type3_font_rejects_embedded_font_program`、`type3_font_rejects_subset_glyphs`、`type3_font_reads_direct_resources`、`type3_font_resolves_indirect_resources`、`type3_font_resources_missing_returns_none`、`type3_font_resources_rejects_non_dict_and_unresolved`、`type3_font_writes_resources`、`type3_font_writes_indirect_resources_as_is`、`type3_font_omits_resources_when_unset`、`type3_font_rejects_non_dict_resources`、`type3_font_resources_round_trip`、集成往返 `builder_type3_font_roundtrips_through_extractor` 与 `builder_type3_glyph_metrics_roundtrip_with_text_extraction`（写入→解析→d0/d1 指标与文本提取）。缺口：渲染未支持（本库为语法层）；`/Resources` 仅解析为字典（不执行/不校验资源名，仅解引用一层）；写端 `Value::Ref` 原样写出、目标对象由调用方保证；CharProcs 内的每字形资源字典未写出；无 BaseEncoding 的码位归属为近似）
- [x] `9.6.6` 字符编码补全（已完成：Type 1 读端识别 Standard/WinAnsi/MacRoman/Symbol/ZapfDingbats，支持 CP1252/MacRoman Unicode 映射、Standard/Symbol/ZapfDingbats glyph name 查询及完整 Adobe Glyph List 2.0→Unicode 映射；写端 `PdfBuilder` 选择预定义编码名、自建 `/BaseEncoding`+`/Differences` 或省略 `/Encoding`，`text` 按编码反向查表转为单字节码——可打印 ASCII 保持字面量、其余输出十六进制字符串，无法表示的字符替换为 `?`，未知编码名回退 UTF-8 直写；测试见矩阵证据）
- [ ] `9.7.6` Type 0 复合字体 / CIDFontType0（进行中：Type 0→CIDFontType2/CIDFontType0 均可读取；Identity-H/V 与嵌入 CMap（含 Identity-H/V 基座的 usecmap 继承）的字符码→CID 映射已实现；CIDFontType0 解析 `CIDSystemInfo`（Registry/Ordering/Supplement）、W/DW 与 FontDescriptor，`CidFontType0::system_info` 暴露字符集合标识，解码 FontFile3（CIDFontType0C/OpenType）程序并经 ToUnicode 接入文本提取；CFF 轮廓解析已实现（`CidFontType0::cff_outline_parser` 经 `ttf-parser` 适配器提供 glyph 轮廓与宽度）；写端 `builder::CidFontType0` 子类型可经 `PdfBuilder::cid_font` 生成 Type 0 + CIDFontType0 字典与 `/CIDSet` 描述符，写端 `CidFont::with_font_program(FontProgram::CFF)` 可嵌入 `/FontFile3`（`/CIDFontType0C`），往返见 `builder_cid_font_type0_embeds_cff_program_roundtrip`；预定义名册 CMap 的名称引用解析已由 9.7.5 实现；CFF 子集化已实现（`CidFont::with_subset_cids` → `subset_cff` 裁剪 `/FontFile3` 并按交集裁剪 `/CIDSet`，详见 9.6.4），往返见 `builder_cid_font_type0_subset_roundtrips_through_reader`）
- [ ] `9.7.4` CIDFontType2（进行中：读端解析 `CIDSystemInfo`（Registry/Ordering/Supplement）、CIDToGIDMap Identity/流映射、W/DW 宽度，并可加载 CIDFont 的 FontFile2 TrueType 程序；`CidFontType2::system_info` 暴露字符集合标识；已接入 Type 0 父字体与受支持的 CMap 编码；经 `DocumentWriter` 写入→重新解析→文本提取的往返见 `type0_text_roundtrips_through_writer_and_extractor`。写端 `builder::CidFont`（子类型、`CidSystemInfo`、描述符含 `/CIDSet`、`DW`/`W`）经 `PdfBuilder::cid_font` 注册，生成 Type 0 父字体 + CIDFont 后代 + 描述符 + CIDSet 流对象；`PdfBuilder::text_cid` 按 Identity-H 输出 2 字节 CID 十六进制串；`CidFont::with_font_program(FontProgram::TrueType)` 写入 `/FontFile2`；读端往返见 `builder_cid_font_descriptor_roundtrips_through_parser`、`builder_cid_font_type2_embeds_truetype_program_roundtrip`）
- [ ] `9.7.5` CMap（进行中：Identity-H/V 和嵌入 CID 编码 CMap 的 codespace/cidchar/cidrange 已实现；嵌入 CMap 的 usecmap 继承支持 Identity-H/V 基座，覆盖项优先、后定义 codespace 替换并裁剪继承映射、WMode 继承；预定义 CJK CMap 已实现（Adobe-GB1/CNS1/Japan1/Korea1 及 2 字节恒等映射的 UniGB-UCS2-H），经 `resolve_predefined_cmap` 与 `CidCMap::from_predefined` 集成到 Type 0 解析流程，`/Encoding` 名称引用按名册回退解析（`resolves_uni_gb_ucs2`、`resolves_predefined_encoding_names_by_reference`）。写端 `builder::CidEncoding`（`wmode` + 字符码→CID，支持 1..=4 字节码：`mapping` 为 2 字节、`mapping_bytes` 为任意 1..=4 字节，另可 `with_cmap_name`/`with_usecmap`）与 `CidFont::with_encoding`，`PdfBuilder::text_codes`/`text_codes_unicode` 按字符码写 2 字节十六进制并让 ToUnicode 以字符码为键，`text_code_bytes`/`text_code_bytes_unicode` 写任意 1..=4 字节源码；`assemble_document` 生成嵌入 CMap 流（`/CMapType 1`、`/WMode`、`/CMapName` 默认 `/Custom-H|V` 可覆盖、`usecmap` 基座写在 `/WMode` 之后 codespace 之前，仅 2 字节码时 codespace 保持 `<0000> <FFFF>`、存在自定义码长时按「长度 + 实际连续码区间」分组并拒绝跨长度前缀歧义，字符码与 CID 同时连续递增时写 `cidrange`、其余 `cidchar`、每节 100 条）并作为 Type 0 `/Encoding` 引用；构建期校验 wmode、空映射、`/CMapName` 合法性与 `usecmap` 基座（须为 Identity-H/V 或名册已知名称且不得同名）。写端亦支持按名称引用预定义 CMap：`CidFont::with_predefined_encoding` 把 `/Encoding` 写为名册名称引用（不生成嵌入流，与 `with_encoding` 互斥，未知名称含竖排 `-V` 变体在 `build` 报 `WriteError::InvalidObject`），`PdfBuilder::text_predefined_unicode` 把 Unicode 码位按 2 字节大端写入并登记 ToUnicode（补充平面与目标 2 字节码空间外的字符跳过）；测试 `predefined_cid_encoding_writes_name_reference_without_cmap_stream`、`predefined_cid_encoding_rejects_unknown_and_vertical_names`、`predefined_cid_encoding_conflicts_with_custom_encoding`、`predefined_cid_encoding_unicode_text_encodes_ucs2_and_registers_to_unicode`、`predefined_cid_encoding_unicode_text_skips_codes_outside_codespace`、集成往返 `builder_predefined_cmap_encoding_roundtrips_through_extractor`（写入"中文GB"→解析→提取还原）。测试 `custom_cid_encoding_writes_embedded_cmap_with_ranges_and_chars`、`custom_cid_encoding_wmode_one_is_vertical`、`text_codes_writes_two_byte_big_endian_hex`、`cid_encoding_rejects_invalid_wmode_and_empty_mapping`、`builder_custom_cid_encoding_roundtrips_through_extractor`、`custom_cmap_name_replaces_default`、`usecmap_identity_base_is_written_before_codespace_and_inherited`、`usecmap_roster_base_is_written_and_inherited`、`one_byte_codes_write_codespace_and_decode`、`three_and_four_byte_codes_write_codespace_and_decode`、`mixed_lengths_decode_by_longest_match`、`to_unicode_keys_follow_code_length`、`usecmap_with_custom_code_length_keeps_explicit_codespace`、`invalid_cmap_name_usecmap_base_code_length_and_conflict_are_rejected`、集成往返 `builder_multibyte_cid_cmap_roundtrips_through_extractor` 与 `builder_usecmap_and_custom_cmap_name_roundtrip`。缺口：完整 Adobe ordering 数据未实现；`text_code_bytes*` 对非法长度条目静默跳过；`usecmap` 基座仅限 Identity-H/V 与名册已知名称，且本 CMap 仍写完整 codespace 与映射）
- [ ] `9.10.3` ToUnicode CMap（进行中：Type 0 字体支持嵌入 ToUnicode CMap 的 bfchar/bfrange Unicode 映射，含 UTF-16BE 补充平面字符；读端已接入内容流文本提取。写端 `CidFont.to_unicode` 记录 CID→Unicode，`PdfBuilder::text_cid_unicode` 在写文本时自动登记，`PdfBuilder::text_codes_unicode`/`text_code_bytes_unicode` 以源字符码为键登记（键长随源码 1..=4 字节），`assemble_document` 生成标准 ToUnicode CMap（codespacerange 与 bfchar/bfrange 键按源码实际字节长度写出，仅 2 字节码时保持 `<0000> <FFFF>`；CID/源码与单个 UTF-16 码元目标同时连续递增时合并为 `bfrange`，其余代理对/多码元串/目标跳变写 `bfchar`，两类节均按 100 条上限分段；目标 UTF-16BE、代理对展开），同一 CID 以最后一次登记为准。测试 `text_cid_unicode_writes_to_unicode_cmap`、`to_unicode_cmap_uses_bfrange_for_consecutive_cids`、`to_unicode_cmap_keeps_surrogate_pairs_and_discontinuous_targets_in_bfchar`、`to_unicode_cmap_splits_sections_at_100_entries`、`to_unicode_keys_follow_code_length`、`cid_font_without_unicode_mappings_omits_to_unicode`、`text_cid_unicode_last_mapping_wins`、`builder_cid_font_to_unicode_roundtrips_through_extractor`（写入→解析→提取还原 `Té测😀`）、`builder_cid_font_bfrange_roundtrips_through_extractor`（连续 CID 写入→解析→提取 `ABCDE`）、`builder_custom_cid_encoding_roundtrips_through_extractor`（非 Identity code→CID 嵌入 CMap，ToUnicode 以字符码为键）、`builder_multibyte_cid_cmap_roundtrips_through_extractor`（1..=4 字节混合码长的 ToUnicode 往返）。缺口：写端 ToUnicode 不生成 `usecmap` 继承（仅覆盖同文件内映射）；代理对与多码元目标一律写 `bfchar`（不合并 `bfrange`））
- [ ] `9.8` 字体描述符（阶段项仍进行中；与功能矩阵状态一致：读端部分完成、写端与测试已完成，状态为 🟡/✅/✅。读端 `FontDescriptorMetrics` 覆盖 FontName、FontFamily、FontStretch、FontWeight、Flags、FontBBox、ItalicAngle、Ascent、Descent、Leading、CapHeight、XHeight、StemV、StemH、AvgWidth、MaxWidth、MissingWidth、CharSet 与 CIDSet；Flags 提供九个标准位的类型化访问器，CIDSet 经过滤器解码后可按 CID 查询位；Type 1 与 CIDFontType2/CIDFontType0 均暴露描述符。写端 `builder::FontDescriptor` 支持链式构造并输出完整度量字典（含 FontName、CharSet），`Font::with_descriptor` 接入 `PdfBuilder` 字体组装流程；CIDFont 描述符经 `CidFont` 写出，`/CIDSet` 作为独立流对象（`StreamData::Owned` 明文）挂到描述符。测试见矩阵证据）
- [ ] `9.9` 嵌入字体程序（FontFile/FontFile2/FontFile3）（进行中：Type 1 读端经 `Document::type1_program` 解析 `/FontFile` 的明文头、eexec 段、Private 字典与解密后的 CharStrings/Subrs，且 `Type1FontProgram::glyph` 可将单个 CharString 解释为字宽、side bearing 与绝对路径段（flex 的 `callothersubr`/`pop` 按七点协议展开为曲线段，提示替换 `callothersubr 3` 按 OtherSubrs 语义消费 hint code 并等待 `pop`；seac 按 StandardEncoding 递归展开为基/重音轮廓；FontFile3/CFF/OpenType 报错）；TrueType 与 CIDFontType2 读端解析 FontFile2 的 SFNT 目录、基础元数据和轮廓。写端新增 `builder::FontProgram`（Type1/TrueType/CFF）与 `Font::with_font_program`，`assemble_document` 分别输出 `/FontFile`、`/FontFile2`、`/FontFile3`（CFF 带 `/Subtype /Type1C`）流对象；TrueType 程序自动把字体 `/Subtype` 切为 `TrueType`；builder 构造的 `StreamData::Owned` 流按 writer 约定写明文（不声明 FlateDecode）。CIDFont 的字体程序嵌入已支持（Type2→`/FontFile2`、Type0→`/FontFile3` `/CIDFontType0C`，经 `CidFont::with_font_program`；程序类型与子类型不匹配时 `build` 返回 `WriteError::InvalidObject`）；TrueType 字形裁剪工具 `subset_truetype` 已接入 builder（`Font::with_subset_glyphs`），并保留补充平面 format 12 cmap 与 vhea/vmtx 垂直度量；Type 1 子集工具 `subset_type1` 已接入 builder（`Font::with_subset_type1_glyphs`，与 `subset_glyphs` 互斥，`/FontFile` 写裁剪结果）；CID-keyed CFF 子集工具 `subset_cff` 已接入 builder（`CidFont::with_subset_cids`，`/FontFile3` 写裁剪结果并按交集裁剪 `/CIDSet`，见 9.6.4）；读端 `Document::truetype_font` 已接受 `/FontFile3` `/Subtype /OpenType`（`OTTO` flavor，CFF-flavored 程序可无 `loca`/`glyf`，仅 cmap 与度量可用）；name-keyed CFF 子集工具 `subset_cff_type1c` 已接入 builder（`Font::with_subset_cff_glyphs`，三类子集列表互斥，`/FontFile3` `/Subtype /Type1C` 写裁剪结果，CFF2 未支持），`9.6.4` 的六字母名称标签前缀已支持）
- [ ] `9.10` 文本内容提取（进行中：`reader/text/{mod,tokenizer}` 实现内容流文本提取——操作数栈式分词与文本操作符解释（BT/ET/Tf/Tj/TJ/Td/TD/T*/Tm/'/"），Type 0 经 CMap+ToUnicode 解码，简单字体（Type 1/MMType1/TrueType/Type3）经 `/Encoding`+AGL 解码，缺 `/Encoding` 的 TrueType 若 `/FontFile2` 可解析则先按 `TrueTypeFont::builtin_encoding`（cmap，含 Symbol `(3,0)` 的 0xF000/0xF100/0xF200 偏移回退）解码、PUA 字符让位于预定义编码、仍未命中再回退预定义编码，无字体时回退 Latin-1；页面级 `extract_page_text` 支持页面树遍历、资源继承与多内容流合并；单元测试覆盖。写端补充 `ContentStream::show_text_hex`（`<...> Tj`），集成测试 `type0_text_roundtrips_through_writer_and_extractor` 验证 Type 0 + CIDFontType2 + FontFile2 + ToUnicode 的写入→解析→提取往返，`truetype_without_encoding_extracts_through_builtin_cmap` 验证缺 `/Encoding` 的 TrueType 经内建 cmap 提取，`builder_custom_cid_encoding_roundtrips_through_extractor` 验证非 Identity 嵌入 CMap 的写入→解析→提取往返，`builder_multibyte_cid_cmap_roundtrips_through_extractor` 验证 1..=4 字节混合码长的写入→解析→提取往返。写端 `PdfBuilder::text_code_bytes`/`text_code_bytes_unicode` 支持任意 1..=4 字节源码。完整 Adobe ordering 数据仍待补齐；`text_code_bytes*` 对非法长度条目静默跳过）
- [ ] `7.9` 公共数据结构（已完成：`PdfDate`、`PdfRect` 已实现；`PdfTextString` 支持 PDFDocEncoding 与带 BOM 的 UTF-16BE 编解码；`NameTree` 和 `NumberTree` 支持从字典解析（含 `/Kids` 递归）、按键查找、迭代和序列化往返）

**依赖**：A（过滤器解码能力是字体程序/嵌入流读取的硬依赖；`7.10` Functions 对文本非硬依赖，为软依赖，可并行）。

**执行顺序与并行边界**：

1. **简单字体与度量（进行中）**：Type 1 字体字典、宽度读取、Encoding 字典、WinAnsi/MacRoman 映射、Standard/Symbol/ZapfDingbats glyph name 查询及 AGL 2.0 的 4,281 个 Unicode 序列映射已覆盖；写端编码配置与文本码转换（五种预定义编码 + 自定义 Differences，非 ASCII 输出十六进制字符串）已实现。Standard 14 内建字宽已按 AFM 数据整理为结构化 CSV，Courier 四种样式为固定 600 单位；Type 1 字体程序格式解析（明文头、内建编码、eexec、Private 字典、CharStrings/Subrs 解密）已完成，`Type1FontProgram::glyph` 已解释 CharString 操作符并产出字宽与绝对路径，seac 合成字形已按 StandardEncoding 展开（含递归嵌套），flex 的 `callothersubr`/`pop` 已按七点协议展开为曲线段（含 subr 内进出与 `pop pop setcurrentpoint` 收尾）；提示替换 `callothersubr 3` 已按 OtherSubrs 语义消费单个 hint code，并要求后续 `pop` 消费其结果（不应用 hint）；写端 `FontProgram::Type1`/`TrueType`/`CFF` 与 `Font::with_font_program` 已可嵌入字体程序；`subset_type1` 已可按字形名裁剪 Type 1 `/CharStrings`、补齐 seac 组件闭包并重加密 `eexec` 段，并经 `Font::with_subset_type1_glyphs` 接入 builder（与 `subset_glyphs` 互斥）。剩余 hinting/渲染与 CFF 子集程序重写。
2. **TrueType 程序读取（进行中）**：TrueType 的 `/FontFile2` 解码后校验 SFNT 目录，并解析 `head`/`maxp` 的 unitsPerEm/glyph count、`hhea`/`hmtx` 水平度量、`vhea`/`vmtx` 垂直度量（含 numberOfVMetrics 末尾复用与尾部 TSB）、Unicode `cmap` format 0/2/4/6/12/13 以及 `loca`/`glyf` 字形记录。可按 glyph ID 查询 advance width 与 vertical advance/top side bearing（含复用规则）、Unicode 查询 glyph ID，并读取 glyph header/bounds 和原始记录；简单字形的轮廓端点、instructions、压缩 flags 及 x/y 坐标增量已解码，复合字形可递归展开，支持 2.14 缩放/旋转/斜切矩阵、XY 偏移与点对齐，并限制递归深度及展开点数；`TrueTypeFont::builtin_encoding` 提供由 cmap 派生的 256 项单字节内建编码，供 `reader/text` 在简单 TrueType 字体缺 `/Encoding` 时解码（`/Encoding` 优先，程序缺失/非法时回退预定义编码）；cmap 选择支持 Microsoft Symbol `(3,0)` 子表，符号字体直查失败时依次按 0xF000/0xF100/0xF200 偏移探测，提取时非 PUA 内建字符优先、PUA 字符让位于预定义编码；不执行 hinting，也不渲染。此二进制解析仅用 Rust 标准库，以便控制格式支持范围及错误语义，当前无需引入第三方解析依赖。独立的 `subset_truetype` 已完成 glyf/loca/hmtx/hhea/maxp/cmap/head 重写、复合组件闭包与表校验和重建，保留补充平面 format 12 cmap 与 vhea/vmtx，并经 `Font::with_subset_glyphs` 接入 builder 写入 `/FontFile2`。读端同时接受 `/FontFile3` `/Subtype /OpenType`：`OTTO` flavor 在 `loca`/`glyf` 缺失时仍解析 cmap 与度量（轮廓不可用），为 CFF-flavored OpenType 提供文本提取能力。
3. **CID/复合字体与文本提取（进行中）**：CIDFontType2 可解析 descendant 字典、`CIDSystemInfo`、`CIDToGIDMap`（Identity/流）、`W`/`DW`，并可从 FontDescriptor 加载 `/FontFile2`。Type 0 可连接一个 CIDFontType2 descendant；编码支持 `/Identity-H`、`/Identity-V` 及嵌入 CMap 的 codespace、`cidchar`、`cidrange` 段，并提供字符码→CID→GID/宽度查询。`/ToUnicode` 支持嵌入 CMap 的 `bfchar` 和 `bfrange` 数组/递增目标，按 UTF-16BE 转换为 Unicode 序列，并在 Type 0 decoded code 中附带可选映射结果。`reader/text/{mod,tokenizer}` 已实现内容流文本提取：操作数栈式分词解释 `BT`/`ET`/`Tf`/`Tj`/`TJ`/`Td`/`TD`/`T*`/`Tm`/`'`/`"` 操作符，Type 0 经 CMap+ToUnicode 解码，简单字体（Type 1/MMType1/TrueType/Type3）经 `/Encoding`+AGL 解码，无字体回退 Latin-1；`Document::extract_page_text` 支持页面树遍历、资源继承与多内容流合并（14 项单元测试覆盖）。写端补充 `ContentStream::show_text_hex`，集成测试 `type0_text_roundtrips_through_writer_and_extractor` 验证 Type 0/CIDFontType2 文档经 `DocumentWriter` 写入、重新解析后由 `extract_page_text` 还原原始 Unicode 文本；`builder::CidFont` + `PdfBuilder::cid_font`/`text_cid` 可生成 Type 0 父字体、CIDFont 后代、描述符（含 `/CIDSet` 流）与 Identity-H 4 位十六进制文本，往返见 `builder_cid_font_descriptor_roundtrips_through_parser`；`builder::CidEncoding` + `CidFont::with_encoding` + `text_codes`/`text_codes_unicode` 可生成非 Identity 嵌入 CMap（cidrange/cidchar、WMode）并按字符码登记 ToUnicode，往返见 `builder_custom_cid_encoding_roundtrips_through_extractor`；`PdfBuilder::text_cid_unicode` 可同时登记 CID→Unicode 并写出 `/ToUnicode` CMap（连续 CID 与单码元目标合并为 `bfrange`，其余 `bfchar`，每节 100 条上限），写入→解析→提取往返见 `builder_cid_font_to_unicode_roundtrips_through_extractor`、`builder_cid_font_bfrange_roundtrips_through_extractor`。Type 3 字体读端已解析字体矩阵/包围盒/宽度/CharProcs 与 `/Encoding` 并接入文本提取（`extracts_type3_differences_text`、`extracts_type3_predefined_encoding_text`）；写端 `Type3Glyph`/`Type3Font`/`Font::type3` 已可生成 Type 3 字典与 CharProcs 流，并在 `builder_type3_font_roundtrips_through_extractor` 中完成写入→解析→提取往返。CMap 的 `usecmap` 继承已支持 Identity-H/V 基座；Type 0 现已同时接受 CIDFontType2 与 CIDFontType0 descendant（后者解析 W/DW 与 FontFile3 程序，文本提取经 ToUnicode 工作）。CIDFont 的字体程序嵌入已支持（`CidFont::with_font_program`，Type2 `/FontFile2`、Type0 `/FontFile3` `/CIDFontType0C`）；完整 Adobe ordering 数据与 CFF 子集程序重写仍待实现；TrueType 字形裁剪已接入 builder（`with_subset_glyphs`），Type 1 子集工具 `subset_type1` 已实现含 seac 组件闭包并经 `with_subset_type1_glyphs` 接入 builder，`9.6.4` 名称标签前缀已实现。
4. **独立公共类型（已完成）**：`7.9` 的 `PdfDate`、`PdfRect`、`PdfTextString`、`NameTree` 与 `NumberTree` 均已实现；名称树和数值树支持 `/Kids` 递归解析、按键查找、迭代和序列化。

**范围边界**：

- 每次只将有数据表、负向用例及公开 API 验证的子项标为已实现；识别 Encoding 名称不等于完成码值映射，码值映射也不等于完成 Unicode 文本提取。
- 标准 14 字体表不得以 `/MissingWidth` 代替逐字符度量；显式 `/Widths` 优先，其次适用的字体描述符默认宽度，再使用准确的内建字宽。
- 依赖关系按上面顺序推进；独立的 `7.9` 名称树/数值树可并行，但不提前勾选阶段 B 整体完成。

**验收标准**：

- 写出含嵌入 TrueType 与 CIDFontType2 的文档，第三方阅读器打开显示正确；
- 含 CJK 文本的样本可正确提取为 Unicode 文本；
- ToUnicode 往返：写入文档的文本可被提取还原；
- 各字体类型均有解析/生成测试，`cargo test` 全绿。

## 阶段 C：图形完整 —— 色彩/Pattern/Form/透明度

**目标**：色彩/图案/Form XObject/透明度达到规范语法层完整（对应矩阵 P1–P2）。

**包含条目**：

- [ ] `8.4.4 / 8.4.5` `gs`（ExtGState 应用）、`i`（flatness）与图形状态参数字典（进行中：`gs`/`i` 操作符与 `ContentStream::set_ext_g_state`/`set_flatness` 之外，已新增读端 `reader/graphics_state/` 的 Table 58 全条目解析（`Document::ext_g_state`/`Document::ext_g_states`，`/SMask` `/TR` `/TR2` `/HT` `/BG` `/BG2` `/UCR` `/UCR2` 保留为原始值）与写端 `PdfBuilder::set_graphics_state` + `builder/types/graphics_state.rs` 的 `/ExtGState` 资源构造（标量参数、`/D`、`/BM`、`/Font`，页内/跨页去重、透明度版本门控、非法值报错）。剩余缺口：写端尚不支持上述 8 个复杂条目，随 `11.5 / 11.6.5` 软蒙版与 `8.9` 等条目落地；`gs` 仍由调用方显式发起，内联 ca/CA/BM 写法待 `11.3.5`/`11.6.4` 条目替换）
- [ ] `8.6.4` Device 颜色空间补全（DeviceCMYK 资源字典构造）
- [ ] `8.6.5` CIE-Based 颜色空间（CalGray/CalRGB/Lab/ICCBased）
- [ ] `8.6.6` Special 颜色空间（Pattern/Indexed/Separation/DeviceN）
- [ ] `8.6.8` 颜色操作符补全（灰度快捷 `G g`）
- [ ] `8.7` Patterns（Tiling/Shading，Shading Type 1–7；依赖 `7.10` Functions）
- [ ] `8.10.1 / 8.10.2` Form XObject（Form 字典生成与 `Do` 联动）
- [ ] `8.10.3` Group XObjects（透明组前置）
- [ ] `8.9` 图像 XObject 补全（SMask/Decode/Interpolate/压缩写入）
- [ ] `8.9.6` 图像蒙版（Stencil/Explicit/Colour Key）
- [ ] `11.3.5` 混合模式（BM）规范符合性修正：改经 ExtGState 字典 + `gs`（替代当前内联写法）
- [ ] `11.6.4` 恒定透明度（ca/CA）规范符合性修正：同上
- [ ] `11.4 / 11.6.6` 透明组（Transparency Group XObject，含 isolated/knockout）
- [ ] `11.5 / 11.6.5` 软蒙版（SMask 字典与 Soft-Mask 图像生成）
- [ ] `14.6` 标记内容操作符（`MP DP BMC BDC EMC`，可选内容前置）
- [ ] `8.11` 可选内容完整（OCG 结构生成、`OptionalContent` 枚举接入 Op 并序列化）

**依赖**：A（`7.10` Functions 是 Shading 的硬依赖）、B（Pattern/Form 内文本需字体度量支持，为部分软依赖）。

**验收标准**：

- ca/CA/BM 全部经 ExtGState + `gs` 序列化，矩阵 11.x 行的「规范符合性缺口」备注消除；
- 含 ICCBased/Separation/Pattern 颜色的文档可生成并被第三方阅读器正确渲染；
- 含透明组与软蒙版的文档生成正确；
- OCG 生成 + BDC/EMC 操作符接入并有测试，`OptionalContent` 死代码标记消除；
- `cargo test` 全绿。

## 阶段 D：交互与交换 —— 导航/注解/表单/元数据

**目标**：导航/注解/表单/元数据基础可用（对应矩阵 P2）。

**包含条目**：

- [ ] `12.3` 文档级导航（显式/命名 Destinations、大纲 Outline）
- [ ] `12.5.1–12.5.5` 注解通用结构（Annotation 字典/标志/边框样式/外观流 AP）
- [ ] `12.5.6` 常用注解类型（引用子表对应行：Text、Link、FreeText、Highlight、Underline、Squiggly、StrikeOut、Stamp、Ink、Popup、FileAttachment、Widget）
- [ ] `12.6` 常用动作（GoTo/URI/Named，其余动作类型按需增补）
- [ ] `12.7.2–12.7.4` AcroForm 基础（AcroForm 字典/字段字典/Widget 注解联动）
- [ ] `14.3.2` 元数据流（XMP /Metadata 读写）
- [ ] `14.3.3` 文档信息字典（/Info 语义封装）
- [ ] `14.4` 文件标识符（/ID 生成、更新与校验；经典表写出路径补 /ID）
- [ ] `7.11` 文件规格与嵌入文件流（Filespec、/EmbeddedFiles；FileAttachment 注解前置）

**依赖**：C（注解外观流/表单外观依赖图形能力完整）。

**验收标准**：

- 生成含大纲与 Link 注解的文档，第三方阅读器可点击跳转；
- Text/Highlight 等常用注解生成+解析往返测试通过，`tests/fixtures/annotated.pdf` 接入测试；
- AcroForm 文本字段+Widget 写出后可被识别填写；
- XMP 与 /Info 读写一致，/ID 生成符合规范算法；
- `12.4`、`12.7.5`、`12.10`、`12.2` 等按需增补，不阻塞本阶段验收；
- `cargo test` 全绿。

## 阶段 E：专门领域（长期）

**目标**：按需求驱动，不预设排期；仅在明确使用场景出现时逐项立项（对应矩阵 P3）。

**包含条目**：

- [ ] `13.2` 多媒体（Renditions、Media Clip）
- [ ] `13.3` 声音（Sounds）
- [ ] `13.4` 视频（Movies）
- [ ] `13.5` Alternate Presentations
- [ ] `13.6` 3D Artwork
- [ ] `12.8` 数字签名（DocMDP/UR/FieldMDP、PKCS#7）
- [ ] `14.7` 逻辑结构
- [ ] `14.8` Tagged PDF
- [ ] `14.9` 无障碍支持
- [ ] `14.10` Web Capture
- [ ] `14.11` 预印支持（含 `8.6.7` Overprint、`11.7.4` Overprinting 与透明度联动）
- [ ] `7.12` Extensions 字典
- [ ] PDF 2.0 差异评估（引用矩阵「PDF 2.0 差异摘要」节，评估结果决定写端 2.0 策略是否调整）
- [ ] `7.5.6` 写端增量更新（写出 /Prev 链）
- [ ] `附录 F` 线性化输出（生成 hint 流）

**依赖**：D（多数专门领域建立在导航/注解/元数据基础之上）；各项彼此独立，可任意顺序立项。

**验收标准**：

- 逐项立项时单独定义验收标准并挂到对应 spec；
- `12.7.8` XFA 不建议投入（PDF 2.0 已弃用，矩阵该行有注），维持 ⬜ 或评审转 ❌。

## 阶段依赖图

```mermaid
flowchart LR
    A["阶段 A：读得全"] --> B["阶段 B：文本可用"]
    A -->|"7.10 Functions（硬）"| C["阶段 C：图形完整"]
    B -->|"字体度量（软）"| C
    C --> D["阶段 D：交互与交换"]
    D --> E["阶段 E：专门领域"]
```

依赖构成无环 DAG：A → B → C → D → E；A 对 C、B 对 C 的细化依赖如图所示，均不构成回边。
