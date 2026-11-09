# 阶段 C：内容流读取模型（跨章节前置）
> 规划层｜职责：本阶段的增量、依赖、验收标准与未闭项。｜禁止：测试名、统计数字、历史叙述（见 `changelog/`）。

**目标**：提供通用的内容流解析、资源解析与图形状态模型，作为第 8/9/11/14 章读端能力的共同底座；本阶段只做**模型与数据**，不做渲染。

**为什么单列**：当前库只有写端 `content::ContentStream`，读端仅有 `reader/text` 为文本提取做的词法扫描。这导致矩阵第 8/9/11/14 章大量 Reader 列为 ⬜，也让颜色空间、Shading/Pattern、图像、标记内容等读端能力无法复用。该模型是所有后续读端增量的硬前置。

**增量**：

- [x] **C1 内容流词法与 Op AST**（非规范：工程决策）：复用并重构 `reader/text/tokenizer.rs`，解析操作符、操作数、内联图像 `BI/ID/EI` 边界、注释/空白/名称转义；未建模操作符保留原始 token 序列以便重新序列化。新增 `reader::content` 目录模块（触及 `reader/text/mod.rs` 前先拆分）。
  - 退出标准：合成内容流覆盖全部 ISO 32000-1 Table A.1 操作符；内联图像二进制数据不被误分词；未知操作符可往返；现有文本提取测试全部通过。
  - 测试：`parse_graphics_state_operators`、`parse_path_construction_operators`、`parse_path_painting_operators`、`parse_text_object_operators`、`parse_color_operators`、`parse_unmodeled_operator`、`parse_empty_stream`、`parse_comments_and_whitespace`、`parse_tj_array_operand`、`parse_dash_pattern_array_operand`、`parse_empty_dash_pattern_array_operand`、`tj_array_with_hex_string_elements`、`tj_rejects_non_array_operand`、`tj_rejects_unterminated_array`、`tj_rejects_name_element_in_array`、`d_rejects_non_numeric_array_element`、`d_rejects_missing_phase_operand`
- [x] **C2 资源解析与页面内容 API**（非规范：工程决策）：`Document::page_content_ops`（或等价 API）遍历页面树、合并 `/Contents` 数组、解析 `/Resources` 继承、解析 `/XObject`/`/Font`/`/ColorSpace`/`/Shading`/`/Pattern`/`/Properties` 名称。Form XObject 不在 API 内自动递归：调用方按 `Op::PaintXObject` + `page_resource("XObject", name)` 自行展开（递归深度与防环由调用方控制；C3 状态机只对 `q/Q` 栈限深，不约束 XObject 展开）。
  - 退出标准：`tests/fixtures/pdflatex-4-pages.pdf` 四页均可解析；资源名经 `#xx` 转义可解析；`/Parent` 资源继承有测试。
  - 测试：`page_content_ops_parses_simple_page`、`page_content_ops_returns_error_for_invalid_page_index`、`page_resource_resolves_font_name`、`page_resource_inherits_from_parent`、`page_resource_returns_none_for_missing_resource`、`page_resource_decodes_name_escapes`、`page_content_ops_handles_contents_array`、`decode_name_escapes_handles_various_cases`、`pdflatex_4_pages_all_content_ops_parse`
- [x] **C3 图形状态机**：`q/Q` 栈、CTM、线宽/线帽/线接/斜接/虚线、文本状态、`gs` 应用 ExtGState、颜色/图案当前值；状态复制与恢复语义按 8.4/9.3/11.x。
  - 退出标准：嵌套 `q/Q`、`cm`、`gs`、`Tf/Td/Tm` 的状态快照测试；越界/非法参数返回类型化错误。
  - 实现范围：`reader/content/state.rs` 新增 `ContentStateRunner`/`ContentState`（Table 57 初始值、Table 56 `Q` 空栈为 no-op、`MAX_SAVE_DEPTH`=512 限深）、`run_content_state` 逐操作符快照、`Document::page_content_state_trace`/`page_content_state`（经 `/Resources/ExtGState` 应用 `gs` 的 LW/LC/LJ/ML/SA/D/RI/Font/CA/ca/BM/OP）；文本状态按 9.3/9.4（BT 仅重置 Tm/Tlm、Td 平移 Tlm、TD 设置 leading=-ty、T*/'/" 依 leading 换行）；颜色/图案当前值跟踪 rg/RG/g/G/CMYK/scn/SCN/CS/cs 与分量范围校验。
  - 测试：`q_q_stack_restores_ctm_and_line_width`、`q_stack_underflow_has_no_effect`、`cm_concatenates_matrices_in_prepend_order`、`save_stack_depth_limit_returns_error`、`gs_applies_ext_g_state_parameters`、`unknown_gs_name_returns_typed_error`、`gs_font_name_entry_sets_text_font`、`invalid_state_parameters_return_typed_errors`、`non_finite_matrix_returns_typed_error`、`bt_resets_text_matrices_keeps_text_state`、`td_td_and_t_star_update_matrices`、`tick_and_quote_show_text_move_to_next_line`、`color_operators_track_current_values`、`named_pattern_color_operators_are_modeled`、`out_of_range_device_color_components_return_typed_errors`、`pdflatex_4_pages_state_machine_runs`
  - 剩余缺口：路径/裁剪/图像/Form XObject 的绘制语义不进入状态模型（仅几何与参数状态）；`gs` 的 `/Font` 名称形式只记录名称不解析字号引用；颜色分量个数与 `/ColorSpace` 基色空间的匹配依赖 D1 读端模型。
- [x] **C4 标记内容与可选内容读取语义**：`BMC/BDC/EMC/MP/DP` 嵌套与属性表解析、`/ActualText`/`/E` 提取、OCG 可见性过滤；与 14.6/8.11 的读端模型统一；`/ActualText` 见 14.9.4 Table 337。
  - 退出标准：嵌套标记内容配对错误可诊断；`ActualText` 替换与隐藏 OCG 过滤有测试；属性表支持名称与内联字典。
  - 实现范围：分词器补 `CsToken::Dictionary`（`take_dictionary`，嵌套/字符串感知，`(a>b)` 不提前闭合）并修复名称分词吞分隔符（`]`）的缺口；`parser.rs` 新增 `parse_dictionary_inner` 宽松字典构建（嵌套字典/数组/`num num R` 引用（顶层与数组内）/布尔/字符串/十六进制串）。新增 `reader/content/marked.rs`：`collect_marked_sections`（`BMC/BDC/EMC/MP/DP` 栈配对，`MAX_MARKED_DEPTH`=64 限深，类型化 `MarkedContentError`：`UnmatchedEnd`/`UnclosedSection`/`DepthExceeded`）、`MarkedSection`（span、`point`、`/ActualText`/`/E`/`/Alt`/`/Lang`，字面与 UTF-16BE 十六进制串解码）、属性表支持内联字典与 `/Properties` 名称解析（含兼容写端「名称直指 OCG 字典」形态）、`resolve_marked_visibility` 经目录 `/OCProperties` 判定 OCG 可见性（`/D` BaseState/ON/OFF）、`Document::page_marked_content`。文本提取端（`reader/text`）统一语义：`MarkedFrame` 栈实现隐藏范围不提取、`/ActualText` 整段替换（emit-once）、嵌套继承隐藏态、`EMC` 宽容失衡、Form XObject 内未闭合范围不外泄；`OptionalContentProperties::is_visible` 复用。
  - 测试：`pairs_begin_and_end_with_spans`、`nests_sections_and_closes_inner_first`、`point_operators_are_zero_length_sections`、`bdc_resolves_inline_dictionary_properties`、`bdc_resolves_property_name_through_resources`、`bdc_name_pointing_at_ocg_dictionary_marks_group`、`unmatched_emc_reports_operator_index`、`unclosed_section_reports_outermost_tag`、`depth_limit_is_enforced`、`text_entries_decode_literal_and_hex_strings`、`visibility_resolves_oc_name_and_reference`、`page_marked_content_resolves_properties_and_visibility`、`bdc_keeps_inline_dictionary_property_list`、`bdc_property_dictionary_supports_nested_dict_array_and_reference`、`dp_accepts_property_name_or_dictionary`、`hex_string_operand_in_dictionary_keeps_hex_characters`、`dictionary_with_string_containing_angle_brackets_roundtrips_entries`、`actual_text_replaces_all_shows_in_section`、`actual_text_decodes_hex_string_property`、`bdc_property_name_resolves_through_properties_resource`、`hidden_ocg_section_is_not_extracted`、`visible_ocg_section_is_extracted`、`hidden_ocg_property_resolved_by_direct_reference`、`bdc_name_pointing_at_ocg_dictionary_filters_hidden_group`、`nested_section_inherits_hidden_state_from_parent`、`unbalanced_emc_does_not_break_extraction`、`unclosed_marked_section_in_form_does_not_leak_to_caller`、`pdflatex_4_pages_marked_content_pairs_with_spans`（写→读往返 `builder_optional_content_roundtrips_through_parser` 同步升级为隐藏层不提取断言）
  - 剩余缺口：内联 OCG 字典形态的可见性未求值（OCMD 已随 B7 求值）；间接属性表操作数（`5 0 R /Tag BDC`）不支持；`page_marked_content` 不递归 Form XObject，且其回调签名无法区分间接 OCMD 引用；`/E` 已暴露但未参与提取替换；`/Artifact` 范围识别与位置感知 API 已随 B7 落地；`reader/optional_content.rs` 已目录化（主树 `2d90bac`）。
- [x] **C5 读写模型统一与 roundtrip**（非规范：内部约定，见 docs/roadmap/principles.md）：读端 Op AST 与写端 `content::Op` 的映射表；parse→model→serialize→parse 的往返测试；未建模操作符透传策略记录在矩阵。
  - 退出标准：覆盖路径/文本/颜色/XObject/Shading/Pattern/标记内容的往返样本；序列化字节可被现有 parser 重新解析；无 panic。
  - 实现范围：新增 `reader/content/mapping.rs` 三件套——`to_content_op`（读端 `Op` → 写端 `content::Op`，写端不可表达时返回类型化 `OpMappingError`：`NotRepresentableByWriter`（TD 双操作数/d0/d1/BX/EX/内嵌图像/Unmodeled/非 Name/Dict 属性表/非 UTF-8 的 `'`/`"`/`TJ` 串）与 `OperandOutOfRange`（`J`/`j` 非 0–2、`Tr` 非 0–255）；`Tj` 串按字节合法性自动选字面量或十六进制形式）；`from_content_op`（写端 → 读端全函数，唯一有损点为单操作数 `MoveNextLineSetLeading` 映射为 `TD` 且 `tx=0`）；`serialize_ops`（可映射操作符复用写端 `ContentStream` 序列化器，其余走原始回退输出合法语法，保证 `parse → serialize → parse` 模型级无损）。parser 修复：水平缩放关键字从错误的 `Th` 改为规范的 `Tz`；未建模操作数经 `tokens_to_values` 无损捕获（`[ ... ]` 数组含嵌套、`num gen R` 引用、`true/false/null` 类型化、字面量串按字节逐 char、十六进制串大写）。`ContentStream::ops()` 只读访问器与 `LineCap/LineJoin::value()` 公开（纯增量，不改公共签名）。
  - 测试：`horizontal_scaling_parses_tz_keyword`、`graphics_state_and_path_operators_roundtrip`、`text_operators_roundtrip_including_hex_binary_and_escapes`、`td_d0_d1_bx_ex_roundtrip_through_raw_fallback`、`out_of_range_linecap_linejoin_and_render_mode_roundtrip`、`to_content_op_errors_for_variants_without_writer_model`、`show_text_selects_literal_for_utf8_and_hex_for_binary`、`marked_content_operators_roundtrip_name_and_inline_dict`、`color_pattern_xobject_shading_and_extgstate_roundtrip`、`unmodeled_operators_preserve_array_name_and_string_operands`、`from_content_op_maps_writer_stream_ops_and_remaps_losslessly`、`writer_model_ops_map_to_reader_model_shapes`、`inline_image_variant_serializes_and_reparses`、`pdflatex_4_pages_content_model_serialization_roundtrips`（真实四页样本逐页模型无损）、`content_model_roundtrip_preserves_text_extraction`（builder 写出 → 读端模型 → `serialize_ops` → 重解析相等 → 经 `to_content_op` 重建 Form 写出后文本仍可提取）
  - 剩余缺口：内联图像 `Op::InlineImage` 自 D2 起可读写往返（`BI … ID <data> EI`，D2 已修复分词器入口）；写端模型仍缺 TD 双操作数、d0/d1、BX/EX、内嵌图像与 Unmodeled 透传变体（桥接层以原始回退补齐，未扩展 `content::Op` 公共 API）；Latin-1 边界：未建模操作数/属性字典内 `Value::String` 含 ≥0x80 字节时经 `write_value` 按 UTF-8 编码写出、重解析按字节逐 char，模型级不严格相等（ASCII 串往返无损）；`BDC/DP` 的数组/布尔/间接引用形态操作数仍是 `expect_properties` 的既有限制（映射为 Unmodeled 后按写值原样输出，重解析形态不变）；写端 `MoveNextLineSetLeading` 单操作数 `TD` 不合规范的问题仅记录未修（改动公共枚举超出本增量边界）。

**后续增量排期（已排期，待开工；当前不计入复选框，开工时转为正式复选框并同步阶段总览）**：

- **C6 文本渲染模式 4–7 的语法层接受**（矩阵 [9.3](../../matrix/ch09/text-model.md)）：把状态机的 `Tr` 合法范围从 0–3 扩到 0–7，接受并类型化存储 4–7；负值、非整数与大于 7 仍按非法参数报错。写端 `content::Op` 与读写映射已按 0–255 处理，本次不扩写端 API。
  - 事实依据：`src/core/reader/content/state/runner.rs` 的 `require_enum("Tr", *mode, 3)` 是范围上限未补全，不是版本门——状态机不接收源文件版本，也没有按版本拒绝 4–7 的分支；ISO 32000-1 §9.3.6 Table 106 列出 0–7（4–7 为填充/描边与裁剪组合及仅裁剪）。
  - 位置与依赖：排在当前在飞的 F1（数字签名 CMS 结构层，F1.1–F1.3）切片收口之后；依赖 C1/C3 的 Op AST 与状态机，不依赖 E/F 产物；与 D8 互不依赖，按 C6 → D8 串行落地，各自独立验收。
  - 退出标准：`Tr` 0–7 全部可解析并在状态快照中读回，既有 0–3 行为不回归；`-1`、`8` 与非整数仍返回类型化错误；矩阵 9.3 行证据同步；`cargo build --all-targets` 零警告、`cargo test` 全绿、`cargo clippy` 无新增告警。
  - 明确不做：4–7 的字形轮廓加入裁剪路径、裁剪与填充/描边的实际绘制、`ET` 时裁剪生效（渲染语义，见 [non-goals.md](../non-goals.md)）。

**依赖**：A。B 为软依赖（文本提取可在 C1/C2 完成后迁移到统一模型，但不得为迁移而扩大本阶段范围）。

**验收标准**：

- 真实多页 PDF 的内容流可完整解析，操作符计数与资源引用可核对；
- 内联图像、Form XObject、`q/Q`、`gs`、标记内容均有测试；
- 文本提取在切换到统一模型后结果不回归；
- `cargo test` 全绿；矩阵第 8/9/11/14 章中「无通用内容流读端」的备注可被逐项消除。
