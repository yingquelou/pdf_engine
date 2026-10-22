# PDF 标准特性矩阵（FEATURE MATRIX）

> **目的**：以 ISO 32000-1:2008（PDF 1.7）第 7–14 章章节体系为骨架，列出标准全部特性在本库中的实现状态，回答「标准规定了什么、已实现什么、还缺什么」。
>
> **数据源**：
> - 章节号：ISO 32000-1:2008 本地文档（`docs/PDF32000_2008/` HTML，章节目录见 `.trae/specs/pdf-standard-roadmap/iso-toc.md`）；
> - 状态与证据：能力审计报告 2026-03-19（`.trae/specs/pdf-standard-roadmap/capability-audit.md`，只读源码审计）。
>
> **与 ROADMAP.md 的关系**：本矩阵是事实层（现状盘点，只随代码实测更新）；`docs/ROADMAP.md` 是规划层（把本矩阵中的 ⬜/🟡 条目按价值与依赖组织为开发阶段）。ROADMAP 的阶段条目一律通过本矩阵的章节号回溯。

## 图例

- **状态四态**：✅ 已实现 / 🟡 部分实现（缺口见备注）/ ⬜ 未实现 / ❌ 明确不支持（设计决策，附理由）
- **Reader / Writer 列**：✅ 🟡 ⬜ ❌ 同上；`—` = 不适用（该特性无读/写之分）
  - 约定：本库读端**不解析内容流操作符**（`ContentStream` 仅有 `push`/`to_bytes`，无解析入口），故第 8/9/11 章内容流操作符类条目的 Reader 列一律 ⬜，仅表示流字节可整体透传。
  - 约定：写端**不编码过滤器**，已解码流剥离过滤器、引用流原样透传（`prepare_stream_dict_for_write`），故过滤器类条目的 Writer 列以 🟡 表示「透传/剥离，不主动编码」。
- **优先级**：P0 核心语法补全（真实世界 PDF 读入正确性）/ P1 高价值内容能力（字体文本、图像、色彩）/ P2 结构增强（Pattern、Form XObject、透明度完整、导航注解表单、元数据）/ P3 专门领域（多媒体、Tagged PDF、预印、Web Capture、签名、PDF 2.0）

> **路径说明（模块目录化后）**：下文证据中的 `reader.rs` / `writer.rs` / `types.rs` / `content.rs`
> 为历史引用，现分别对应目录模块 `reader/`、`writer/`、`types/`、`content/`；
> 子模块清单见 `docs/ROADMAP.md` 的「工程债：模块目录化」。

## 总览

| 章 | ✅ | 🟡 | ⬜ | ❌ | —（说明性） | 合计 |
|---|---|---|---|---|---|---|
| 第 7 章 语法 | 31 | 5 | 8 | 1 | 4 | 49 |
| 第 8 章 图形 | 6 | 4 | 11 | 0 | 2 | 23 |
| 第 9 章 文本 | 0 | 5 | 12 | 0 | 2 | 19 |
| 第 10 章 渲染 | 0 | 0 | 5 | 0 | 1 | 6 |
| 第 11 章 透明度 | 0 | 2 | 4 | 0 | 2 | 8 |
| 第 12 章 交互特性 | 0 | 0 | 40 | 0 | 1 | 41 |
| 第 13 章 多媒体 | 0 | 0 | 5 | 0 | 1 | 6 |
| 第 14 章 文档交换 | 0 | 1 | 10 | 0 | 1 | 12 |
| **总计** | **37** | **17** | **95** | **1** | **14** | **164** |

## 第 7 章 语法（Syntax）

| 章节 | 特性 | 引入版本 | 状态 | Reader | Writer | 优先级 | 证据/备注 |
|---|---|---|---|---|---|---|---|
| 7.1 | 本章范围/术语说明 | — | — | — | — | — | 无独立实现项 |
| 7.2 | 词法约定（空白/注释/分隔符/字符集） | 1.0 | ✅ | ✅ | ✅ | P0 | reader/lexer.rs `Lexer::skip_ws`/`is_whitespace`/`is_delim`（空白集含 0x00 NUL，与 filters 的 is_whitespace 对齐）；测试 `skip_comments_and_whitespace`、`nul_byte_treated_as_whitespace`。缺口：>0xFF 字符按 UTF-8 字节转义，无法精确回读 |
| 7.3.2 | Boolean 对象 | 1.0 | ✅ | ✅ | ✅ | P0 | types.rs `Value::Bool`；reader.rs `parse_object`；测试 `parse_bool_null_ref` |
| 7.3.3 | 数值对象（Integer/Real） | 1.0 | ✅ | ✅ | ✅ | P0 | `Value::Integer`/`Float`；测试 `parse_int_and_float`；writer.rs `format_float` |
| 7.3.4 | 字符串对象（字面量 `()` / 十六进制 `<>`） | 1.0 | ✅ | ✅ | ✅ | P0 | `Lexer::read_literal`（转义/八进制/续行）/`read_hex`；测试 `parse_name_and_string` |
| 7.3.4（扩展） | `<~ ~>` ASCII85 字符串（非 ISO 32000-1 语法） | — | ⬜ | ⬜ | 🟡 | — | types.rs `Value::Ascii85String` + writer.rs `Emit` 输出存在，但读端无对应 Token/解析路径，类型为死代码 |
| 7.3.5 | Name 对象（`#xx` 转义双向） | 1.0 | ✅ | ✅ | ✅ | P0 | `Lexer::read_name` / writer.rs `write_name`；测试 `name_escaping_roundtrips`。缺口：>0xFF 字符按 UTF-8 字节转义，无法精确回读 |
| 7.3.6 | 数组对象 | 1.0 | ✅ | ✅ | ✅ | P0 | `parse_object` 显式栈迭代；测试 `parse_array_and_dict`、`deep_nesting_hits_limit_gracefully`（嵌套上限 64） |
| 7.3.7 | 字典对象 | 1.0 | ✅ | ✅ | ✅ | P0 | `parse_object` 显式栈迭代（`Frame::Dict`）；测试 `parse_array_and_dict`、`deep_nesting_hits_limit_gracefully` |
| 7.3.8 | 流对象（stream/endstream） | 1.0 | ✅ | ✅ | ✅ | P0 | `parse_indirect_object` + `Lexer::compute_stream_range`；普通懒加载路径按直接/间接 `/Length` 确定字节范围，再验证 `endstream`，`StreamData::Ref` 懒加载；测试 `parse_indirect_object_stream_ref`、`stream_length_prevents_embedded_endstream_truncation`、`indirect_stream_length_handles_embedded_endstream_and_cycles`。缺少 `/Length` 或调用方没有长度解析器时保留标记扫描兼容路径 |
| 7.3.9 | null 对象 | 1.0 | ✅ | ✅ | ✅ | P0 | `Value::Null`；测试 `parse_bool_null_ref` |
| 7.3.10 | 间接对象与引用 `n g R` | 1.0 | ✅ | ✅ | ✅ | P0 | `parse_indirect_object` + `try_peek_is_number` 试探回退；测试 `parse_indirect_object_simple`、`xref_generation_mismatch_defers_to_object_header` |
| 7.4.1 | 过滤器通则（/Filter 名或数组、/DecodeParms、缩写名） | 1.0 | ✅ | ✅ | 🟡 | P0 | `filters/mod.rs` `StreamFilter`/`decode_stream`/`decode_stream_with`/`builtin_filters`/`filter_names`/`decode_params`/`canonical_filter`；内建 Flate/LZW/CCITT/ASCII/RunLength 均经 trait 调度，第三方实现经 `filters/flate.rs` 的 `InflateBackend` 隔离；测试 `multiple_filters_decode_in_sequence`、`custom_filter_can_be_registered`。缺口：`/Filter` 为间接引用时按不支持处理；写端不编码过滤器，仅透传/剥离 |
| 7.4.2 | ASCIIHexDecode | 1.0 | ✅ | ✅ | 🟡 | P0 | `filters/ascii.rs` `ascii_hex_decode`；测试 `ascii_hex_ignores_whitespace_and_pads_odd` |
| 7.4.3 | ASCII85Decode | 1.0 | ✅ | ✅ | 🟡 | P0 | `filters/ascii.rs` `ascii85_decode`（`z`、`<~` 前缀、末组补齐）；测试 `ascii85_decodes_basic_and_z_shorthand` |
| 7.4.4 | LZWDecode | 1.0 | ✅ | ✅ | 🟡 | P0 | `filters/lzw.rs` `lzw_decode`（9–12 位码宽、CLEAR/EOD、KwKwK、表满 4096 停止扩容、`/EarlyChange` 0/1、截断宽容），解码后应用 Predictor；测试 `lzw_decodes_spec_example`、`lzw_clear_resets_table`、`lzw_early_change_switches_code_width`、`lzw_applies_predictor`、`lzw_rejects_invalid_code`、`lzw_rejects_invalid_early_change`、`lzw_stops_at_truncated_stream`、`lzw_xref_stream_is_readable`、`lzw_xref_stream_with_png_up_predictor_is_readable`。缺口：写端不主动编码 LZW，仅对读端已解压流剥离过滤器 |
| 7.4.4 | FlateDecode | 1.2 | ✅ | ✅ | 🟡 | P0 | `filters/flate.rs` `FlateFilter` → `flate_decode` → `InflateBackend`（默认 `Flate2Backend` 封装 flate2/miniz_oxide 纯 Rust 后端，第三方仅在此适配器内出现）；测试 `corrupt_flate_errors` |
| 7.4.4.4 | LZW/Flate Predictor 函数（1/2/10–15，LZW 与 Flate 共用） | 1.2（待核） | ✅ | ✅ | 🟡 | P1 | `filters/predictor.rs` `apply_predictor`/`png_predictor`/`tiff_predictor`；由 `FlateFilter`/`LzwFilter` 在解码后共同调用；测试 `png_up_predictor_roundtrips`、`png_paeth_and_per_row_filters`、`png_sub_and_average_predictors`、`tiff_predictor_roundtrips`、`lzw_applies_predictor`。缺口：TIFF 预测器要求 BitsPerComponent 为 8 的倍数 |
| 7.4.5 | RunLengthDecode | 1.0 | ✅ | ✅ | 🟡 | P0 | `filters/ascii.rs` `run_length_decode`；测试 `runlength_decodes_literal_and_repeat` |
| 7.4.6 | CCITTFaxDecode | 1.0 | ✅ | ✅ | 🟡 | P0 | `ccitt/{decoder,tables}.rs` 纯 Rust 实现：Group 3 1-D（Modified Huffman）、Group 3 2-D（EOL 后 1-D/2-D 标志）、Group 4（T.6，EOFB/RTC）；参数 `/K` `/Columns` `/Rows` `/EndOfLine` `/EncodedByteAlign` `/EndOfBlock` `/BlackIs1` `/DamagedRowsBeforeError`；经 `filters/mod.rs` 的 `CcittFilter` 接入 `StreamFilter`。测试 `g3_1d_decodes_runs`、`g3_1d_makeup_run`、`g3_1d_encoded_byte_align`、`g3_2d_tag_and_eol`、`g4_2d_modes_decode`、`g4_pass_mode`、`g4_vertical_left`、`g4_matches_libtiff_sample`、`g4_matches_libtiff_varied_sample`、`g4_rows_unknown_stops_at_eofb`、`g4_black_is_1_inverts_output`、`truncated_first_row_errors`、`ccitt_filter_decodes_group4_and_alias`、`ccitt_filter_honors_black_is_1`、`ccitt_filter_rejects_invalid_columns`。缺口：写端不主动编码 CCITT，仅对读端已解压流剥离过滤器；`/EndOfBlock` 停止点按 EOB/RTC、`Rows`、数据结束三者最早者取宽容语义 |
| 7.4.7 | JBIG2Decode | 1.4 | ⬜ | ⬜ | 🟡 | P1 | 解码无实现；types.rs `image_filter_feature` 特性检测 + writer.rs `validate_content_version` 1.4 门控已实现；测试 `content_feature_checks_cover_jbig2_transparency_oc_and_encryption` |
| 7.4.8 | DCTDecode（JPEG） | 1.0 | ✅ | ✅ | 🟡 | P0 | 读端通过 `JpegBackend` trait + `ZuneJpegBackend` 适配器实现（zune-jpeg）；Adobe APP14 约定（CMYK transform=0 取反还原、YCCK transform=2 自实现 YCbCr→RGB 取补逆变换）；测试 `dct_gray_roundtrip`、`dct_rgb_roundtrip`、`dct_cmyk_roundtrip`、`dct_ycck_roundtrip`、`dct_progressive_roundtrip`、`dct_corrupt_errors_without_panic`、回归 `writer_filter_handling_matches_reader_decoding`（含 DCT/DCTDecode 剥离） |
| 7.4.9 | JPXDecode（JPEG 2000） | 1.5 | ⬜ | ⬜ | 🟡 | P2 | 解码无实现；特性检测 → `PdfFeature::JPXDecode`（1.5 门控）；测试 `content_version_check_rejects_jpx_below_1_5`、`feature_profile_matches_content_validation` |
| 7.4.10 | Crypt 过滤器 | 1.5 | 🟡 | ✅ | ❌ | P0 | `reader/security/standard.rs` 按对象号/generation 应用 Standard Security Handler crypt filter；`reader/document.rs::decode_stream_bytes` 在密码认证后先处理 `/Crypt` 再解码其余过滤器，支持 `/Identity`；测试 `standard_security_r4_decrypts_explicit_crypt_filter`、`crypt_identity_filter_is_supported_without_encryption`。缺口：写端不加密且拒绝写出加密文档；非 Standard 安全处理器不支持 |
| 7.5.1 | 本节范围说明 | — | — | — | — | — | 无独立实现项 |
| 7.5.2 | 文件头 `%PDF-x.y` | 1.0 | ✅ | ✅ | ✅ | P0 | types.rs `PdfVersion::parse`/`from_name`（1.0–1.7 + 2.0 容忍；2.1/3.0 拒绝）；测试 `reader_accepts_pdf_2_0_header`、`missing_header_defaults_version_with_warning`、`version_parsing_recognizes_2_0` |
| 7.5.2 / 附录 I | PDF 2.0（ISO 32000-2）**输出** | 2.0 | ❌ | — | ❌ | P3 | 设计决策：写端显式拒绝 2.0。writer.rs `SUPPORTED_OUTPUT_VERSIONS`（仅 1.0–1.7）+ `validate_output_version`；测试 `writer_rejects_unsupported_version` |
| 7.5.3 | 文件体（间接对象序列） | 1.0 | ✅ | ✅ | ✅ | P0 | reader.rs `Parser` 顺序解析 / writer.rs `DocumentWriter` 逐对象写出（证据同 7.3.10） |
| 7.5.4 | 经典交叉引用表 | 1.0 | ✅ | ✅ | ✅ | P0 | reader.rs `parse_xref` + `ClassicXrefStrategy`；writer.rs `ClassicXrefSerializer`；测试 `parse_xref_and_trailer`、`classic_xref_kept_below_1_5`（≤1.4 目标输出经典表） |
| 7.5.5 | 文件 Trailer 字典 | 1.0 | ✅ | ✅ | ✅ | P0 | `parse_trailer`；types.rs `Trailer::root`/`size`；writer 构造（/Root /Size，含 /Encrypt 透传） |
| 7.5.6 | 增量更新（/Prev 链） | 1.0（待核） | ✅ | ✅ | ⬜ | P2 | reader.rs `follow_prev_chain`（最新优先、自由条目不回退、防环/限深 64）；测试 `incremental_update_prev_chain_merges_history`、`incremental_update_free_entry_does_not_fall_back`、`incremental_update_prev_to_xref_stream`、`circular_prev_chain_terminates_with_warning`。缺口：写端不产出增量更新 |
| 7.5.7 | 对象流（ObjStm） | 1.5 | ✅ | ✅ | ✅ | P0 | 读：`Document::get_object_from_obj_stream`；写：`WriteContext::write_object_stream`（≥1.5 自动打包非流对象，不压缩）；测试 `uncompressed_obj_stm_resolves_inner_object`、`flate_compressed_obj_stm_resolves_inner_object`、`object_stream_packs_non_stream_objects_at_1_5`、`undecodable_filter_obj_stm_fails_gracefully`。缺口：嵌套对象流不支持（warning 放弃） |
| 7.5.8 | 交叉引用流（XRef stream） | 1.5 | ✅ | ✅ | ✅ | P0 | 读：`parse_xref_stream`（/W /Index /Size、零宽字段缺省、过滤器解码）+ `XrefStreamStrategy`；写：`XrefStreamSerializer`；测试 `xref_stream_zero_type_width_defaults_in_use`、`flate_compressed_xref_stream_is_readable`、`flate_xref_stream_with_png_up_predictor_is_readable`、`xref_stream_written_at_1_5`、`xref_stream_parsed_despite_low_declared_version`（按内容识别，宽读） |
| 7.5.8.4 | 混合引用文件（trailer /XRefStm） | 1.5 | ✅ | ✅ | ⬜ | P2 | reader.rs `resolve_xref_at` 内 /XRefStm 处理 + `merge_xref`；测试 `hybrid_reference_xref_stm_merges_entries` |
| 附录 F | 线性化（Linearized，"fast web view"） | 1.2 | 🟡 | ✅ | ⬜ | P0 | 读端不利用 hint 流、不解读线性化字典：线性化文件按普通 PDF 路径完整可读（reader/tests.rs 构造器 `build_linearized_classic`/`build_linearized_xref_stream`，`/L /H /E /T` 真值收敛）；测试 `linearized_classic_xref_readable_via_normal_path`、`linearized_hint_pair_variants_accepted`（/H 两种形态）、`linearized_corrupt_hint_stream_tolerated`（损坏 hint 流不影响文档级读取）、`linearized_xref_stream_and_objstm_readable`（1.5 变体）。缺口：不生成线性化输出与 hint 流（阶段 E） |
| 7.6.1 | 本节范围说明 | — | — | — | — | — | 无独立实现项 |
| 7.6.2 | 加密检测与透传（trailer /Encrypt） | 1.1 | 🟡 | ✅ | ❌ | P0 | `Parser::with_password` 认证 trailer `/Encrypt`；未提供密码时尝试空密码；错误密码或不支持的算法明确失败；writer 以 `WriteError::EncryptedDocument` 拒绝输出，避免残留加密字典与解密明文不匹配；测试 `standard_security_r2_decrypts_strings_and_streams`、`standard_security_r4_decrypts_explicit_crypt_filter` |
| 7.6.3 | 标准安全处理器（解密） | 1.1 | ✅ | ✅ | — | P0 | ISO 32000-1 R2–R4：用户/所有者密码认证、RC4-40/128、AESV2、字符串/流及 `/EncryptMetadata false` 处理；RustCrypto 原语经 `CryptoBackend` 适配器隔离，`security_handler_uses_an_injectable_crypto_backend` 验证后端可替换。测试 `revision_three_authenticates_user_and_owner_passwords`、`revision_four_aesv2_crypt_filter_decrypts_streams`、`standard_security_r2_decrypts_strings_and_streams`、`standard_security_r4_decrypts_explicit_crypt_filter`。不支持 PDF 2.0 R5/R6、公钥安全处理器或加密写出；加密文档写出明确失败 |
| 7.6.3 | 权限模型（/P 权限位，读端解释） | 1.1 | ⬜ | ⬜ | — | P2 | 认证读取 `/P` 以派生文件密钥，但尚未公开/解释权限位供调用方使用 |
| 7.6.4 | 公钥安全处理器（Public-Key） | 1.3（待核） | ⬜ | ⬜ | — | P3 | 无对应代码 |
| 7.7.1 | 本节范围说明 | — | — | — | — | — | 无独立实现项 |
| 7.7.2 | Document Catalog（/Version 覆盖文件头版本） | 1.0（/Version 1.4） | ✅ | ✅ | ✅ | P0 | reader.rs `parse_document` 步骤 3.5（取两者较大者）；测试 `catalog_version_overrides_header_version` |
| 7.7.3 | 页面树 / Page 对象 / 属性继承 | 1.0 | 🟡 | 🟡 | ✅ | P0 | builder.rs `assemble_document` 生成 Catalog/Pages/Page；读端仅经通用对象模型解析，无专用页面遍历/继承解析 API |
| 7.7.4 | Name Dictionary | 1.1（待核） | ⬜ | ⬜ | ⬜ | P2 | 无对应代码（按通用字典透传，无语义封装） |
| 7.8 | 内容流与资源字典（/Contents、/Resources） | 1.0 | ✅ | 🟡 | ✅ | P0 | builder.rs `assemble_document` 生成内容流对象；content.rs `ContentStream::to_bytes`；测试 `builds_minimal_text_pdf`。缺口：读端按流对象透传（操作符不解析）；不生成多内容流数组 /Contents [..] |
| 7.9 | 公共数据结构（文本串/日期/矩形/名称树/数值树） | 1.0–1.2（待核） | 🟡 | 🟡 | 🟡 | P1 | `types::PdfTextString` 支持 PDFDocEncoding 与 UTF-16BE（含 BOM）的编码/解码；未定义 PDFDocEncoding 字节及非法 UTF-16 显式报错，并避免 PDFDocEncoding 字节恰以 `FE FF` 开头时与 UTF-16BE BOM 混淆。`PdfDate` 支持日期解析/序列化，`PdfRect` 支持四坐标数组解析/序列化并保留反向坐标；测试 `pdf_text_string_decodes_ascii_and_pdf_doc_encoding`、`pdf_text_string_uses_utf16be_for_unrepresentable_characters`、`pdf_text_string_avoids_pdfdocencoding_bom_prefix_ambiguity`、`pdf_text_string_rejects_undefined_pdf_doc_encoding_bytes`、`pdf_text_string_rejects_malformed_utf16be` 及日期/矩形测试。缺口：名称树与数值树语义类型尚未实现 |
| 7.10 | Functions（Type 0/2/3/4） | 1.2–1.3（待核） | ✅ | ✅ | ⬜ | P0 | `functions/{mod,sampled,exponential,stitching,calculator}.rs` + `functions/tests/{mod,type0,type2,type3,type4}.rs`，纯 Rust 语义层求值 API（`Function::from_value` → `eval`）：Type 0 采样（MSB-first 多位打包 1/2/4/8/12/16/24/32、/Size /Encode /Decode、多线性插值；`/Order 3` 三次插值暂不支持并明确报错）、Type 2 指数插值（m=1 校验、C0/C1 补齐）、Type 3 拼接（区间左闭右开、/Bounds 递增校验、嵌套限深 64、`/Encode [1 0]` 反向映射）、Type 4 PostScript 计算器子集（算术/关系/布尔/栈操作/if/ifelse/常量，栈深上限 512）。测试 37 项（`type0_sample_values_and_interpolation`、`type0_two_dimensional`、`type2_basic_exponential`、`type3_spec_example_inverted_encode`、`type3_interval_half_open`、`type3_nesting_depth_guard`、`type4_arithmetic`、`type4_stack_operations`、`type4_conditionals_and_constants` 等）。缺口：写端随阶段 C Shading 生成落地；子函数携带 `StreamData::Ref` 时需调用方先经 `Document` 解引用 |
| 7.11 | 文件规格与嵌入文件流（Filespec、/EmbeddedFiles） | 1.3（待核） | ⬜ | ⬜ | ⬜ | P2 | 无对应代码 |
| 7.12 | Extensions 字典（/Extensions、BaseVersion、ExtensionLevel） | 1.7（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |

## 第 8 章 图形（Graphics）

| 章节 | 特性 | 引入版本 | 状态 | Reader | Writer | 优先级 | 证据/备注 |
|---|---|---|---|---|---|---|---|
| 8.1 | 本章范围/术语说明 | — | — | — | — | — | 无独立实现项 |
| 8.2 | 图形对象分类（路径/文本/图像/着色/外部对象） | 1.0 | — | — | — | — | 无独立实现项（类别实现见 8.5–8.10 各行） |
| 8.3 | 坐标系与变换矩阵（CTM，`cm`） | 1.0 | ✅ | ⬜ | ✅ | P0 | content.rs `Op::ConcatMatrix`；测试 `save_concat_restore_chain` |
| 8.4 | 图形状态操作符 `q Q w J j M d ri`（线宽/线帽/线接/斜接限/虚线/渲染意图） | 1.0 | ✅ | ⬜ | ✅ | P0 | content.rs 9 个变体（SaveState/RestoreState/SetLineWidth/SetLineCap/SetLineJoin/SetMiterLimit/SetDashPattern/SetRenderingIntent + ConcatMatrix）；测试 `graphics_state_individual` |
| 8.4.4 / 8.4.5 | `gs`（ExtGState 应用）、`i`（flatness）与图形状态参数字典 | 1.2（待核） | ⬜ | ⬜ | ⬜ | P1 | content.rs `Op` 无对应变体；影响透明度等图形状态字典的正确应用方式（见第 11 章缺口） |
| 8.5.2 | 路径构造操作符 `m l c v y h re` | 1.0 | ✅ | ⬜ | ✅ | P0 | content.rs 7 个变体；测试 `path_construction_all_kinds` |
| 8.5.3 | 路径绘制操作符 `S s f F f* B B* b b* n` | 1.0 | ✅ | ⬜ | ✅ | P0 | content.rs 10 个变体；测试 `path_painting_variants`（全量覆盖） |
| 8.5.4 | 裁剪路径操作符 `W W*` | 1.0 | ✅ | ⬜ | ✅ | P0 | content.rs `Op::Clip`/`ClipEvenOdd`；测试 `clipping_variants` |
| 8.6.4 | Device 颜色空间（DeviceGray/DeviceRGB/DeviceCMYK） | 1.0–1.1（待核） | 🟡 | ⬜ | 🟡 | P1 | builder.rs `ColorSpace` 仅 DeviceGray/DeviceRGB；CMYK 操作符（K k）存在但无资源字典构造逻辑 |
| 8.6.5 | CIE-Based 颜色空间（CalGray/CalRGB/Lab/ICCBased/Default 颜色空间/渲染意图语义） | 1.1–1.3（待核） | ⬜ | ⬜ | ⬜ | P1 | 无颜色空间资源字典构造逻辑 |
| 8.6.6 | Special 颜色空间（Pattern/Indexed/Separation/DeviceN） | 1.2–1.3（待核） | ⬜ | ⬜ | ⬜ | P1 | content.rs 仅 `SetStrokeColorSpace(String)` 按名引用，无对应资源构造 |
| 8.6.7 | Overprint 控制（OP/op/OPM） | 1.2（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码（预印向能力，见 14.11；需经 ExtGState，依赖 8.4.4 的 `gs`） |
| 8.6.8 | 颜色操作符（`RG rg K k CS cs SC sc SCN scn`） | 1.0–1.2（待核） | 🟡 | ⬜ | ✅ | P1 | content.rs 10 个变体；测试 `color_variants`。缺口：缺灰度快捷操作符 `G g`；SCN 的 pattern 分支明示不在范围 |
| 8.7 | Patterns（Tiling/Shading，Shading Type 1–7） | 1.2–1.3（待核） | ⬜ | ⬜ | ⬜ | P2 | 无对应代码 |
| 8.8 | 外部对象调用（`Do`） | 1.0（待核） | ✅ | ⬜ | ✅ | P0 | content.rs `Op::DrawXObject`；builder.rs `image`（`q cm Do Q`）；测试 `image_xobject_emitted` |
| 8.8.2 | PostScript XObjects | 1.1（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码（标准不建议使用，PDF 2.0 已弃用） |
| 8.9 | 图像 XObject 字典（Width/Height/ColorSpace/BitsPerComponent 等） | 1.0 | 🟡 | ⬜ | 🟡 | P1 | builder.rs `assemble_document` 图片 XObject（原始字节透传不压缩）；测试 `image_xobject_emitted`。缺口：仅 DeviceGray/DeviceRGB；无压缩过滤器、无 SMask/ImageMask/Decode/Interpolate/Alternate Images |
| 8.9.6 | 图像蒙版（Stencil/Explicit/Colour Key，ImageMask//Mask） | 1.0–1.3（待核） | ⬜ | ⬜ | ⬜ | P1 | 无对应代码 |
| 8.9.7 | 内嵌图像 `BI ID EI` | 1.0（待核） | ⬜ | ⬜ | ⬜ | P2 | 无对应代码 |
| 8.10.1 / 8.10.2 | Form XObject（Form 字典） | 1.1（待核） | ⬜ | ⬜ | ⬜ | P2 | 仅通用 `Do` 操作符；无 Form 字典生成 |
| 8.10.3 | Group XObjects | 1.4 | ⬜ | ⬜ | ⬜ | P2 | 无对应代码（透明组前置依赖，见 11.4） |
| 8.10.4 | Reference XObjects | 1.4 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| 8.11 | 可选内容（OCG、/OCProperties、BDC/EMC、配置字典） | 1.5 | 🟡 | 🟡 | ⬜ | P2 | 检测：types.rs `inspect_dict_features`（/OCProperties→OptionalContent，≥1.5 门控）；测试 `content_feature_checks_cover_jbig2_transparency_oc_and_encryption`。缺口：content.rs `OptionalContent` 枚举（Begin/PropertyDict/End）已定义但**未接入 Op、无序列化、无测试（死代码）**；生成端完全不产出 OCG 结构 |

## 第 9 章 文本（Text）

| 章节 | 特性 | 引入版本 | 状态 | Reader | Writer | 优先级 | 证据/备注 |
|---|---|---|---|---|---|---|---|
| 9.1 | 本章范围/术语说明 | — | — | — | — | — | 无独立实现项 |
| 9.2 | 字体的组织与使用（资源引用/显示基础/字形定位与度量） | 1.0 | 🟡 | ⬜ | 🟡 | P1 | builder.rs `Font`/`assemble_document`（标准 14 字体引用）；无字形定位/度量处理 |
| 9.3 | 文本状态参数操作符（`Tc Tw Tz TL Tf Tr`） | 1.0 | 🟡 | ⬜ | ✅ | P1 | content.rs 6 变体；测试 `text_state_operators_individual`。缺口：缺 `Ts`（text rise，9.3.7）与 `Tk`（text knockout，9.3.8，需经 ExtGState） |
| 9.4 | 文本对象与定位/显示（`BT ET Td TD Tm T* Tj TJ`） | 1.0 | 🟡 | ⬜ | ✅ | P0 | content.rs 12 变体；测试 `begin_end_text_roundtrip`、`show_text_at_format`、`show_text_array_mixed`。缺口：缺 `'` 与 `"` 显示文本操作符 |
| 9.5 | 字体数据结构总述 | — | — | — | — | — | 无独立实现项（结构实现见 9.6–9.9 各行） |
| 9.6.2 | Type 1 简单字体（标准 14，BaseFont 引用） | 1.0 | 🟡 | 🟡 | 🟡 | P1 | `reader::font::Type1Font` 读取直接/间接 `/Widths` 与 `/FontDescriptor` 数值元数据，并提供按字节码查询宽度（显式宽度优先，其次 `/MissingWidth`，无二者时 Courier 四种样式回退 600 单位、Helvetica 与 Times 各四种样式按 StandardEncoding 查询 Adobe AFM 宽度）；测试 `type1_font_resolves_indirect_widths_and_descriptor_metrics`、`type1_font_uses_builtin_courier_widths_when_no_override_exists`。builder.rs 输出标准 14 BaseFont 引用；缺口：Symbol、ZapfDingbats 内建宽度表、字体嵌入及完整字体程序支持未实现 |
| 9.6.2.3 | Multiple Master 字体（MMType1） | 1.0（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| 9.6.3 | TrueType 字体 | 1.1 | ⬜ | ⬜ | ⬜ | P1 | 无对应代码 |
| 9.6.4 | 字体子集化（Font Subsets，六字标签前缀） | 1.0（待核） | ⬜ | ⬜ | ⬜ | P1 | 无对应代码（随字体嵌入一并实现） |
| 9.6.5 | Type 3 字体 | 1.0 | ⬜ | ⬜ | ⬜ | P2 | 无对应代码 |
| 9.6.6 | 字符编码（WinAnsiEncoding 等预定义 Encoding） | 1.0 | 🟡 | ⬜ | 🟡 | P1 | `reader::font::Document::type1_font_encoding` 解析五种预定义编码名称和 `/Differences`；WinAnsi 支持 CP1252→Unicode、MacRoman 支持 0x80–0xFF→Unicode、StandardEncoding 支持已定义字形槽位；Symbol 支持 glyph name 表及 Adobe Glyph List Unicode 映射，ZapfDingbats 支持 glyph name 表及 Zapf AGL 映射；完整 Adobe Glyph List 2.0 的 4,281 个映射由 `SimpleFontEncoding::unicode_sequence_for` 提供（多标量名称在 `unicode_for` 中返回 `None`）。测试 `type1_font_encoding_resolves_predefined_encodings_and_differences`；builder.rs 写端仍输出 WinAnsiEncoding |
| 9.7.6 | Type 0 复合字体 / CIDFontType0（CID-Keyed） | 1.2–1.3（待核） | ⬜ | ⬜ | ⬜ | P1 | 无对应代码；中文/CJK 文本无法正确表达（字符串按 Latin-1 假设） |
| 9.7.4 | CIDFontType2（TrueType 轮廓 CID 字体） | 1.3（待核） | ⬜ | ⬜ | ⬜ | P1 | 无对应代码 |
| 9.7.5 | CMap（预定义 CMap / 嵌入 CMap 文件） | 1.2–1.3（待核） | ⬜ | ⬜ | ⬜ | P1 | 无对应代码（含 CIDSystemInfo 字典） |
| 9.6–9.9 | OpenType 字体（FontFile3 /Subtype /OpenType） | 1.6 | ⬜ | ⬜ | ⬜ | P2 | 无对应代码 |
| 9.8 | 字体描述符（FontDescriptor、Flags、CIDFont 描述符） | 1.1（待核） | ⬜ | ⬜ | ⬜ | P1 | 无对应代码 |
| 9.9 | 嵌入字体程序（FontFile/FontFile2/FontFile3） | 1.1–1.6（待核） | ⬜ | ⬜ | ⬜ | P1 | 无对应代码 |
| 9.10 | 文本内容提取（字符码→Unicode 映射） | 1.0–1.4（待核） | ⬜ | ⬜ | — | P1 | 无对应代码 |
| 9.10.3 | ToUnicode CMap | 1.4（待核） | ⬜ | ⬜ | ⬜ | P1 | 无对应代码（文本提取与可复制文本的前提） |

## 第 10 章 渲染（Rendering）

> 本库为语法层库而非渲染器，本章整体不在当前范围；各条目作为远期评估保留。

| 章节 | 特性 | 引入版本 | 状态 | Reader | Writer | 优先级 | 证据/备注 |
|---|---|---|---|---|---|---|---|
| 10.1 | 本章范围/术语说明 | — | — | — | — | — | 无独立实现项 |
| 10.2 | CIE-Based 颜色 → Device 颜色转换 | 1.1（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码（渲染器职责） |
| 10.3 | Device 颜色空间相互转换（Gray/RGB/CMYK） | 1.0–1.1（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码（渲染器职责） |
| 10.4 | Transfer Functions（TR/TR2） | 1.1（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| 10.5 | Halftones（半调字典 Type 1/5/6/10/16、Spot Functions、阈值数组） | 1.2（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| 10.6 | Scan Conversion 细节（flatness/smoothness/扫描转换规则/自动描边调整） | 1.0（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |

## 第 11 章 透明度（Transparency）

| 章节 | 特性 | 引入版本 | 状态 | Reader | Writer | 优先级 | 证据/备注 |
|---|---|---|---|---|---|---|---|
| 11.1 | 本章范围/术语说明 | — | — | — | — | — | 无独立实现项 |
| 11.2 | 透明度模型概述 | — | — | — | — | — | 无独立实现项（概念性章节） |
| 11.3.5 | 混合模式（Blend Mode，BM） | 1.4 | 🟡 | ⬜ | 🟡 | P2 | content.rs `Op::SetBlendMode`（自动记录 `PdfFeature::Transparency`，≥1.4 门控）；测试 `transparency_triggers_feature`。**规范符合性缺口**：本库以内联操作符形式写入，标准语法应经 ExtGState 字典 + `gs` 操作符（依赖 8.4.4） |
| 11.6.4 | 恒定透明度（ca 填充/CA 描边） | 1.4 | 🟡 | ⬜ | 🟡 | P2 | content.rs `Op::SetFillAlpha`/`SetStrokeAlpha`；测试 `plain_text_stream_has_no_transparency`。**规范符合性缺口**：同上行，内联形式非标准语法 |
| 11.4 / 11.6.6 | 透明组（Transparency Group XObject，含 isolated/knockout/Page Group） | 1.4 | ⬜ | ⬜ | ⬜ | P2 | 无对应代码（依赖 8.10.3 Group XObjects） |
| 11.5 / 11.6.5 | 软蒙版（SMask / SMaskInData、Soft-Mask 字典与图像） | 1.4 | ⬜ | 🟡 | ⬜ | P2 | 读端仅特性检测：types.rs `inspect_dict_features`（/SMask /SMaskInData 键→Transparency）；生成端无代码 |
| 11.7.4 | Overprinting 与透明度 | 1.4 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码（依赖 8.6.7 overprint 控制） |
| 11.7 其余 | 透明组颜色空间 / 专色与透明度 / 渲染参数 | 1.4 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |

## 第 12 章 交互特性（Interactive Features）

| 章节 | 特性 | 引入版本 | 状态 | Reader | Writer | 优先级 | 证据/备注 |
|---|---|---|---|---|---|---|---|
| 12.1 | 本章范围/术语说明 | — | — | — | — | — | 无独立实现项 |
| 12.2 | Viewer Preferences（/ViewerPreferences） | 1.2 | ⬜ | ⬜ | ⬜ | P2 | 无对应代码（按通用字典透传） |
| 12.3 | 文档级导航（显式/命名 Destinations、大纲 Outline、缩略图、Collections） | 1.0–1.7（待核） | ⬜ | ⬜ | ⬜ | P2 | 无对应代码 |
| 12.4 | 页面级导航（Page Labels、Articles、Presentations） | 1.1–1.3（待核） | ⬜ | ⬜ | ⬜ | P2 | 无对应代码 |
| 12.5.1–12.5.5 | 注解通用结构（Annotation 字典/标志/边框样式/外观流 AP） | 1.1 | ⬜ | ⬜ | ⬜ | P2 | 无对应代码；`tests/fixtures/annotated.pdf` 存在但无任何测试引用 |
| 12.6 | 动作（Action 字典/触发事件/16 种动作类型：GoTo/GoToR/GoToE/Launch/Thread/URI/Sound/Movie/Hide/Named/Set-OCG-State/Rendition/Transition/GoTo-3D-View/JavaScript） | 1.1–1.6（待核） | ⬜ | ⬜ | ⬜ | P2 | 无对应代码 |
| 12.7.2–12.7.4 | 交互表单（AcroForm 字典/字段字典/按钮/文本/选择/签名字段） | 1.2 | ⬜ | ⬜ | ⬜ | P2 | 无对应代码 |
| 12.7.5 | 表单动作（Submit-Form/Reset-Form/Import-Data） | 1.2 | ⬜ | ⬜ | ⬜ | P2 | 无对应代码 |
| 12.7.7 | FDF（Forms Data Format） | 1.2（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| 12.7.8 | XFA 表单 | 1.5 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码（PDF 2.0 已弃用 XFA，不建议投入） |
| 12.8 | 数字签名（DocMDP/UR/FieldMDP、PKCS#1/PKCS#7） | 1.3 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| 12.9 | Measurement Properties（测量属性） | 1.6 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| 12.10 | Document Requirements（Requirement Handlers） | 1.7 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |

### 12.5.6 注解类型子表（Annotation Types）

| 注解类型 | 引入版本 | 状态 | Reader | Writer | 优先级 | 备注 |
|---|---|---|---|---|---|---|
| Text | 1.1 | ⬜ | ⬜ | ⬜ | P2 | 无对应代码 |
| Link | 1.1 | ⬜ | ⬜ | ⬜ | P2 | 无对应代码（与 12.6 URI/GoTo 动作联动） |
| FreeText | 1.3（待核） | ⬜ | ⬜ | ⬜ | P2 | 无对应代码 |
| Line | 1.3（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| Square | 1.3（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| Circle | 1.3（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| Polygon | 1.3（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| PolyLine | 1.3（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| Highlight | 1.3（待核） | ⬜ | ⬜ | ⬜ | P2 | 无对应代码 |
| Underline | 1.3（待核） | ⬜ | ⬜ | ⬜ | P2 | 无对应代码 |
| Squiggly | 1.4（待核） | ⬜ | ⬜ | ⬜ | P2 | 无对应代码 |
| StrikeOut | 1.3（待核） | ⬜ | ⬜ | ⬜ | P2 | 无对应代码 |
| Stamp | 1.3（待核） | ⬜ | ⬜ | ⬜ | P2 | 无对应代码 |
| Caret | 1.3（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| Ink | 1.3（待核） | ⬜ | ⬜ | ⬜ | P2 | 无对应代码 |
| Popup | 1.3（待核） | ⬜ | ⬜ | ⬜ | P2 | 无对应代码 |
| FileAttachment | 1.3（待核） | ⬜ | ⬜ | ⬜ | P2 | 无对应代码（依赖 7.11 嵌入文件） |
| Sound | 1.2 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码（多媒体，见第 13 章） |
| Movie | 1.2 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码（多媒体；PDF 2.0 已弃用其部分用法） |
| Widget | 1.2 | ⬜ | ⬜ | ⬜ | P2 | 无对应代码（与 12.7 表单字段一体实现） |
| Screen | 1.5 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码（多媒体） |
| PrinterMark | 1.3（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码（预印，见 14.11） |
| TrapNet | 1.3（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码（预印陷印，见 14.11.6） |
| Watermark | 1.6 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| 3D | 1.6 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码（见 13.6） |
| Redact | 1.7 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| RichMedia | 1.7 EL3（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码（Adobe 扩展级别 3，非 ISO 32000-1 正文） |
| Projection | 1.7 EL3（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码（Adobe 扩展级别 3，非 ISO 32000-1 正文） |

## 第 13 章 多媒体（Multimedia）

| 章节 | 特性 | 引入版本 | 状态 | Reader | Writer | 优先级 | 证据/备注 |
|---|---|---|---|---|---|---|---|
| 13.1 | 本章范围/术语说明 | — | — | — | — | — | 无独立实现项 |
| 13.2 | 多媒体（Renditions、Media Clip 对象、播放/屏幕参数、Media Players） | 1.5 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| 13.3 | 声音（Sounds） | 1.2 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| 13.4 | 视频（Movies） | 1.2 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| 13.5 | Alternate Presentations | 1.4（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| 13.6 | 3D Artwork（3D 注解/流/视图/坐标系/标注） | 1.6 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |

## 第 14 章 文档交换（Document Interchange）

| 章节 | 特性 | 引入版本 | 状态 | Reader | Writer | 优先级 | 证据/备注 |
|---|---|---|---|---|---|---|---|
| 14.1 | 本章范围/术语说明 | — | — | — | — | — | 无独立实现项 |
| 14.2 | Procedure Sets | 1.0 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码（标准侧已弃用的概念，无实现价值） |
| 14.3.2 | 元数据流（XMP /Metadata） | 1.4 | ⬜ | ⬜ | ⬜ | P2 | 无对应代码；`tests/fixtures/xmp-metadata.pdf` 存在但无测试引用 |
| 14.3.3 | 文档信息字典（/Info） | 1.0 | ⬜ | ⬜ | ⬜ | P2 | 无对应代码（/Info 仅作普通 trailer 字典键透传，无语义封装） |
| 14.4 | 文件标识符（trailer /ID） | 1.1（待核） | 🟡 | 🟡 | 🟡 | P2 | writer.rs `XrefStreamSerializer` 将源 trailer 字典（含 /ID）原样并入 XRef 流字典。缺口：无生成/更新/校验逻辑；经典表写出路径不补 /ID |
| 14.5 | Page-Piece 字典（应用私有数据） | 1.3 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码（按通用字典透传） |
| 14.6 | 标记内容操作符（`MP DP BMC BDC EMC`）与属性表 | 1.2 | ⬜ | ⬜ | ⬜ | P2 | content.rs `Op` 无对应变体（`OptionalContent` 枚举未接线，见 8.11）。（audit 原标 10.5「编号待核」，按 iso-toc 归 14.6） |
| 14.7 | 逻辑结构（结构层级/结构类型/结构内容/结构属性） | 1.3 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| 14.8 | Tagged PDF（真实内容/Artifact/标准结构类型与属性） | 1.4 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| 14.9 | 无障碍支持（自然语言/替代描述/替换文本/缩写展开） | 1.4–1.5（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| 14.10 | Web Capture（内容数据库/内容集/来源信息） | 1.3 | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |
| 14.11 | 预印支持（页面边界/打印机标记/分色字典/输出意图/陷印/OPI） | 1.2–1.4（待核） | ⬜ | ⬜ | ⬜ | P3 | 无对应代码 |

## PDF 2.0（ISO 32000-2）差异摘要

**本库当前立场**：

- **Reader**：容忍 2.0 文件头（解析为已知版本、记 warning 后继续，不拒绝），证据 tests `reader_accepts_pdf_2_0_header`、`version_parsing_recognizes_2_0`（types.rs `PdfVersion::parse`/`from_name`）。
- **Writer**：输出区间 1.0–1.7，显式拒绝 2.0，证据 writer.rs `SUPPORTED_OUTPUT_VERSIONS` + `validate_output_version`，测试 `writer_rejects_unsupported_version`。
- 本节仅作**远期评估依据**，列出的条目**不进入当前开发阶段**（全部视同 P3）。

**主要差异点**（基于 ISO 32000-2 公开知识，拿不准处标「待核」）：

1. **加密**：AES-256（V=6/R=6）正式纳入标准算法族；新增 unencrypted wrapper document（未加密包装层包裹加密文件）机制。
2. **弃用/移除**：XFA 表单、NeedAppearances、Movie/Sound 注解的若干用法、PostScript XObject、Procedure Sets 等被弃用或移除（具体键清单待核）。
3. **文本串**：Text string 统一为 UTF-8 编码，PDFDocEncoding 仅作兼容保留（待核）。
4. **Associated Files**：`/AF` 键纳入正文（源自 PDF/A-3，可挂接到目录/页面/图形对象等多类字典）。
5. **文档信息字典**：/Info 大部分键弃用，XMP 元数据成为唯一推荐的文档级元数据来源（待核）。
6. **Tagged PDF**：结构元素引入命名空间（Namespaces）机制与新结构类型，无障碍语义增强（待核）。
7. **日期格式**：日期串的 `D:` 前缀由可选变为强制。
8. **数字签名**：增强长期验证与时间戳支持（PAdES 类签名配置；待核）。
9. **页面/目录字典新键**：如输出意图新增 /DestOutputProfileRef、目录级 /AF 等（详见 ISO 32000-2；完整清单待核）。
10. **语义澄清与收紧**：对 xref/trailer 键、线性化参数、实现限制等处的若干澄清与行为收紧（待核）。

## 维护说明

- 本矩阵的状态列**只随代码实测更新**：任何 ✅/🟡 条目的证据列必须能在 `src/pdf` 或 `tests` 中找到对应实现/测试；新能力落地时同步更新状态、证据与「总览」统计（自检方法：统计四态标记出现次数与总览表一致）。
- 章节号以 `.trae/specs/pdf-standard-roadmap/iso-toc.md` 为唯一权威；引用标准时先核对编号再落笔。
- 开发阶段的划分、条目排期与「完成」定义（代码+测试+文档三齐）见 `docs/ROADMAP.md`；每完成一个阶段，按 ROADMAP 的跟踪约定回写本矩阵对应行的状态与证据。
- 优先级（P0–P3）定义见图例，仅在规划评审时调整，调整需同步 ROADMAP 阶段依赖。
