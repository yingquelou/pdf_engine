# 阶段 B：字体与文本
> 规划层｜职责：本阶段的增量、依赖、验收标准与未闭项。｜禁止：测试名、统计数字、历史叙述（见 `changelog/`）。

**目标**：能读写 ISO 32000-1 定义的简单字体、Type 0/CID 字体、Type 3 字体与嵌入程序，并提取 Unicode 文本；不做 hinting/渲染。

**已完成基线**（证据见 `docs/matrix/README.md` 第 9 章）：

- [x] B1 简单字体与编码：9.6.2/9.6.6（Standard 14、`/Widths`、`/FontDescriptor`、五种预定义编码 + `/Differences` + AGL；写端宽度/编码/描述符）。
- [x] B2 TrueType 程序：9.6.3（SFNT、cmap format 0/2/4/6/8/10/12/13、水平/垂直度量、复合字形、内建编码、OpenType `OTTO` 读端）。
- [x] B3 CID/Type 0 核心：9.7.4/9.7.5/9.7.6（CIDFontType0/2、`CIDToGIDMap`、W/DW、DW2/W2、Identity-H/V 与嵌入 CMap、CFF 轮廓经 trait 适配）。
- [x] B4 嵌入程序与子集：9.9/9.6.4（Type 1/TrueType/CFF/OpenType 嵌入、`Length1–3`、TrueType/Type 1/CID-CFF/name-keyed CFF 子集、六字母标签、`/CIDSet` 裁剪；CIDFontType2 的 `/CIDToGIDMap` 见 9.7.4.2 Table 116）。CIDFontType2 子集经 `subset_truetype_with_mapping` 重写 `/CIDToGIDMap`，越界 GID 读端回退 `None`（`builder_cid_font_type2_out_of_range_gid_is_ignored_by_reader`）。

**待推进增量**：

- [x] **B5 Type 3 完善**（9.6.5，🟡；`/BaseEncoding` 见 9.6.6 Table 114）：按 Table 112 补齐**字体级** `/Resources` 的读写（多级间接引用、资源名解析）与 Table 112 要求的 `/Name`、`/FontDescriptor` 条目，以及可选 `/ToUnicode`；无 `/BaseEncoding` 时的精确码位归属（当前以 StandardEncoding 近似）；`d0`/`d1` 指标与文本提取已有。注意：Table 112 只有一个字体级 `/Resources`，CharProcs 内不存在每字形资源字典。
  - 退出标准：Type 3 字体字典的 Table 112 条目可写→解析→提取往返；字体级 `/Resources` 经多级间接引用可读回；`/Name`/`/FontDescriptor`/`/ToUnicode` 读写往返；负向用例覆盖缺失/非法资源与缺失必需条目。
  - 剩余缺口：仍不对 CharProcs 求值（只读 `d0`/`d1` 声明指标，不执行绘制操作符）；无 `/BaseEncoding` 时字符码→字形名仍以 StandardEncoding 近似（未实现 Table 112 注的精确归属）；写端不自动生成 `/Resources` 或字形所引用的资源对象，仅透传调用方字典；`/Name` 按 PDF 1.1+ 语义视为可选（PDF 1.0 要求存在，未做版本门控）。
- [x] **B6 CMap/ToUnicode 完整性**（9.7.5、9.10.3，🟡）：Adobe-GB1/CNS1/Japan1/Korea1 编码类 CMap 已全量 vendored（160 个文件 6039206 B，B6.1–B6.5），读端 ToUnicode `usecmap` 基座名册扩到 47 个 Unicode 编码名称；写端 ToUnicode `usecmap` 仅声明 `Adobe-Identity-UCS` 继承。裸 ordering 名仍为紧凑近似（边界见矩阵 9.7.5）。
  - 启动前置：先确定 ordering 数据来源、许可证与体积上限并登记到 fixtures 表，不得引入许可不明的 Adobe 数据文件；`reader/font/cid_cmap.rs` 已超限，触及前先拆分。
  - 退出标准：预定义 CMap 与完整 ordering 样本的字符码→CID 映射正确；数据来源/许可/体积已登记；`usecmap` 基座覆盖/裁剪有测试；`text_code_bytes*` 不再静默跳过非法长度。
  - 剩余缺口：裸 `Adobe-GB1`/`Adobe-CNS1`/`Adobe-Japan1`/`Adobe-Korea1` 仍走紧凑近似（不得生成近似假表，边界见矩阵 9.7.5）；`Adobe-KR-9`/`Adobe-Manga1-0`/`Adobe-Japan2-0` 不在 B6 范围且未收录；写端 `usecmap` 只声明 `Adobe-Identity-UCS` 继承、不展开写出基座映射，代理对与多码元目标一律写 `bfchar`；名册外 ToUnicode 基座名称仍按未知处理（保留本地映射）。
- [x] **B7 文本提取语义完善**（9.10，🟡；`/Artifact` 见 14.8.2.2）：跳过 `/Artifact` 标记内容、提供可选的带位置/间距的文本片段 API（不做版面分析）；`/ActualText` 替换与隐藏 OCG 过滤已随 C4 落地，B7 只需补回归样本与 OCMD 可见性。
  - 退出标准：`ActualText`、Artifact、隐藏 OCG 三类样本的提取结果正确；现有文本提取测试无回归；`try_extract_page_text` 错误语义保持。
  - 剩余缺口：片段位置不含字形宽度推进（同一行多次 `Tj` 共用起点）；不感知 `/Rotate` 与 MediaBox（坐标为默认用户空间）；内联 OCG 字典（无对象号）可见性仍未求值；`page_marked_content` 的间接 OCMD 引用仍不可判定（回调签名限制）；写端无 `/Artifact` 写出 API；`reader/optional_content.rs` 已目录化（2026-11-02，主树 `2d90bac`）。

**非目标（本阶段明确不做）**：hinting、字形光栅化、版面分析、CFF2 子集化、TTC 子集化、OpenType 子集化（保留程序透传 + 子集标签）。

**依赖**：A（过滤器/流解码）。B5 只依赖 A；B7 依赖 C4（标记内容读取模型，已完成），可后置；B6 只触及字体模块。

**验收标准**：

- 写出含嵌入 TrueType、CIDFontType2、Type 1/CFF 子集的文档，重新解析后字体程序/宽度/文本一致；
- 含 CJK 文本的样本可提取为 Unicode；ToUnicode 往返可还原；
- Type 3 字典（含字体级 `/Resources`、`/Name`、`/FontDescriptor`）与 CharProcs 指标往返；
- `cargo test` 全绿。
