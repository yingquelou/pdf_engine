# PDF 库开发路线图（ROADMAP）

> 目的：把 `docs/FEATURE_MATRIX.md` 中的 ⬜/🟡 条目组织为**可独立验收的增量**，定义执行顺序、依赖与完成口径（「未分配条目（待决策）」除外，它们尚未进入交付计划）。
> 与矩阵的关系：矩阵是**事实层**（只记录代码实测状态与测试证据）；本文件是**规划层**（范围、增量拆分、依赖、验收）。规划条目不复制测试名，完成证据一律回写矩阵对应行。
>
> **2026-10-26 规划修订**：以 `docs/PDF32000_2008/` 重新核对 ISO 32000-1 章节号；修正阶段 A 的完成口径（不再宣称「真实世界 PDF 全部可读」）；把原阶段 C 中的「读端通用内容流模型」拆为独立阶段 C，原 C/D/E 顺延为 D/E/F；条目计数改为按「可独立验收增量」统计。旧阶段与新阶段的对应关系见文末「阶段映射」。
>
> **2026-10-26 评审修复**：按文档评审结论修正规划层——阶段 A 回退为 🚧 并新增 A6 fixture 回归（既有 fixture 全部未接入测试，不满足原则 5）；B5 改为 Table 112 的字体级 `/Resources`（删除不存在的「每字形资源字典」）；D3 增加解码后端选型 spike，D4 明确压缩后端失败的单一契约；E1 拥有 `/Names` 字典语义、E5 拥有 `/ID` 生成/校验（E6/E7 复用）；F 的验收标准直接写入本文档；新增「未分配条目（待决策）」；模块债清单与增量依赖按源码实测重写。

## 规划原则

1. **读写闭环**：每个新特性先明确读端模型、写端生成与往返验证；只有一端的能力在矩阵 Reader/Writer 列分开标注，不得以写端完成代替整体完成。
2. **读端模型先行**：内容流、资源、颜色空间、图像、Shading/Pattern 等读端能力统一建立在阶段 C 的通用内容流模型上，避免各功能重复实现分词与资源解析。
3. **语法层边界**：本库目标是 PDF 语法/对象层读写与数据提取，不实现光栅化、hinting、扫描转换、半调、颜色管理或页面合成。第 10 章整体为非目标；字体条目中的「渲染/hinting」缺口不再作为字体阶段未完成理由。
4. **增量粒度**：一个增量 = 一个可独立评审的 PR，必须包含代码、单元测试、必要时的集成往返测试、矩阵回写；单个增量不同时跨两个阶段的核心目标。
5. **真实样本驱动**：每个阶段结束前至少用一个真实 PDF 做回归；fixtures 必须有来源与许可说明，读端不得 panic。合成样本用于边界与负向用例。
6. **失败语义统一**：解析/解码/安全边界返回类型化错误；任何宽容行为显式实现、测试并记录于矩阵备注（沿用工程约定 #4）。
7. **第三方隔离**：新增解码器/编码器/密码学能力必须先定义 PDF 语义 trait，再由适配器接入（沿用工程约定 #1/#2/#3）。
8. **版本门控**：所有新增读写能力必须接入 `PdfFeature`/`PdfVersion` 机制，写端目标版本不足时返回 `WriteError::UnsupportedVersion` 且不写字节。

## 进度跟踪约定

### 状态机

- 单条目状态：`⬜ → 🚧（进行中）→ ✅`；评审决定不做的转 `❌` 并注明理由。
- 矩阵条目的四态（✅/🟡/⬜/❌）与总状态合并规则以 `FEATURE_MATRIX.md` 图例为准；矩阵条目由 `✅` 降为 `🟡` 必须在备注中写明缺口，并在矩阵「总览」修订记录中登记。
- 禁止 `✅ → ⬜` 回退；除非实现被移除，且须在矩阵备注中注明原因。增量条目与阶段状态的 `✅ → 🚧` 回退仅允许在验收口径修订时发生，并须在矩阵备注与本节「阶段映射」同时记录原因。
- 阶段状态：⬜ 未开始 / 🚧 进行中 / ✅ 完成 / ❌ 放弃。阶段完成后发现验收缺口时回退为 🚧，并记录缺口与补做增量（参见阶段 A 的 A6）。
- 状态只在代码实测后修改，禁止凭印象改动。

### 「完成」定义（三齐）

一个增量标记 ✅ 必须同时满足：

1. **代码实现合入**：功能代码进入主分支；
2. **有对应测试**：单元测试 + 必要的集成往返测试，测试名写入矩阵对应行；
3. **文档同步**：矩阵状态/证据/总览统计同步更新，必要时更新 README 版本特性速查表与本文件阶段状态。

三者缺一，不得标 ✅（至多标 🚧）。

### 更新流程

每次增量完成时按顺序执行：

1. 回写矩阵对应行的状态、Reader/Writer 分列与测试证据；
2. 重算矩阵「总览」统计，使四态计数与表内一致；
3. 勾选本文件对应增量的复选框，更新阶段总览的「已完成/增量数」；
4. 当阶段全部增量 ✅ 时更新阶段状态；若阶段完成口径发生变化，先在本文档「阶段映射/口径变更」中记录。

## 工程约定（依赖、扩展点与失败语义）

以下约定面向全库；条目中的现有模块只是当前实例，同类新增实现同样适用。

1. **第三方库必须隔离在 PDF 语义 trait/适配器之后**：
   - 代码实现方案优先级：Rust 标准库 > 成熟第三方库/本地实现；
   - 需要在实现内部使用第三方能力时，禁止直接调用第三方库，必须先定义稳定的 trait，再由适配器实现（现有示例：`filters::InflateBackend` 封装 `flate2`；`CffOutlineParser` 封装 `ttf-parser`；`JpegBackend` 封装 `zune-jpeg`；`CryptoBackend` 封装 RustCrypto 原语）。
2. **扩展优先于修改核心调度**：新增能力应通过既有扩展点（trait、注册表、可注入后端）接入；核心调度逻辑保持稳定，不得为单个实现修改调度分支。现有实例：过滤器经 `filters::StreamFilter` + `builtin_filters` 注册，`decode_stream` 不改动，`decode_stream_with` 支持注入/覆盖。
3. **抽象须可替换、可验证**：每个扩展点都要有替代实现的测试，证明核心只依赖抽象而非具体实现；新增扩展点时同步交付该测试。
4. **边界失败语义统一**：所有解析、解码与安全边界对损坏、截断或非法输入返回类型化错误（`DecodeError`、`FontError`、`ParseError` 等），不得 panic；任何宽容恢复或降级行为必须显式实现、有测试覆盖并记录在矩阵备注中。
5. **模块拆分约定**：单个 `.rs` 文件超过约 600 行或 25 KiB，或同时承担多个职责时，必须解构为目录模块（`foo/mod.rs` + 子模块）；编解码器实现、码表、测试应分文件；既有超限模块列入「工程债」逐项拆分，新增功能不得继续扩大超限文件。
6. **PDF 标准特性实现约束**：当前库实现的所有 PDF 标准特性都必须接受 PDF 版本机制管理。
7. **读写对称**：新增写端特性时，矩阵必须同时评估读端状态；只写不读的能力在阶段验收中不得被称为「完整」。
8. **fixtures 纪律**：真实 PDF 放入 `tests/fixtures/` 并在本文档登记用途与来源；测试必须能在无网络环境运行，fixture 体积保持可控。

## 工程债：模块目录化

已完成拆分（历史记录）：

- [x] `filters`：`filters/{mod,ascii,flate,lzw,predictor,tests}.rs`
- [x] `ccitt`：`ccitt/{mod,decoder,tables,tests}.rs`
- [x] `reader`：`reader/{mod,lexer,object,parser,document,xref,tests}.rs`
- [x] `writer`：`writer/{mod,value,version,serialize,context,document_writer,tests}.rs`
- [x] `types`：`types/{mod,version,features,value,document,tests}.rs`
- [x] `content`：`content/{mod,types,serialize,stream,tests}.rs`
- [x] `builder`：`builder/{mod,types,assemble,tests}.rs`
- [x] `type1_program/charstring`：实现与测试分文件
- [x] `reader/font`：`font/{mod,descriptor,encoding,simple,tests}.rs`
- [x] `reader/text`：`text/{mod,tokenizer,tests}.rs`

**2026-10-26 实测仍超限、后续增量触及前必须先拆分或同步拆分的模块**（按行数降序；阈值见工程约定 #5：>600 行或 >25 KiB）：

- [ ] `reader/font/truetype/cmap.rs`（1537 行 / 69.2 KiB；任何 cmap 增量前拆分）
- [ ] `builder/tests/mod.rs`（1484 行 / 48.1 KiB；新增 builder 测试前按主题拆分）
- [ ] `builder/tests/cid.rs`（1217 行 / 39.4 KiB；新增 CID 写端测试前拆分）
- [ ] `reader/font/truetype/sfnt.rs`（1164 行 / 50.0 KiB；SFNT 增量前拆分）
- [ ] `reader/font/cid.rs`（1140 行 / 40.4 KiB；CID 读端增量前拆分）
- [ ] `builder/tests/shading.rs`（1087 行 / 36.5 KiB；D5 前拆分）
- [ ] `reader/font/cid_cmap.rs`（1037 行 / 36.6 KiB；B6 前拆分）
- [ ] `reader/font/type1_program/mod.rs`（1010 行 / 36.0 KiB；Type 1 增量前拆分）
- [ ] `reader/security/standard.rs`（998 行 / 35.2 KiB；安全增量前拆分）
- [ ] `builder/types/shading.rs`（982 行 / 32.0 KiB；D5/D6 前拆分）
- [ ] `builder/mod.rs`（969 行 / 37.9 KiB；新增 builder 公共 API 前拆分）
- [ ] `reader/text/tests.rs`（856 行 / 28.0 KiB；C1 前拆分）
- [ ] `builder/assemble/mod.rs`（853 行 / 36.6 KiB；组装逻辑变更前拆分）
- [ ] `reader/font/truetype/glyph.rs`（816 行 / 29.6 KiB；字形增量前拆分）
- [ ] `reader/text/mod.rs`（778 行 / 28.3 KiB；C1 复用/重构时拆分）
- [ ] `builder/tests/font_program.rs`（722 行 / 25.1 KiB；新增字体程序测试前拆分）
- [ ] `reader/tests.rs`（689 行 / 26.1 KiB；新增读端测试前拆分）
- [ ] `builder/types/image.rs`（675 行 / 25.0 KiB；D2/D7 前拆分）
- [ ] `builder/assemble/shading.rs`（651 行 / 20.9 KiB；D5 前拆分）
- [ ] `reader/optional_content.rs`（616 行 / 21.1 KiB；B7 前拆分；C4 仅更新模块文档未拆分）
- [ ] `builder/tests/graphics_state.rs`（606 行 / 20.5 KiB；新增图形状态测试前拆分）
- [ ] `tests/main.rs`（8203 行 / 292.3 KiB；集成测试按主题拆分，新增集成测试前先迁出对应主题）

## 阶段总览

| 阶段 | 目标 | 增量数 | 已完成 | 主要依赖 | 状态 |
|---|---|---|---|---|---|
| A 核心读入与安全 | 语法/文件结构/核心过滤器/加密/函数可读 | 6 | 6 | — | ✅ |
| B 字体与文本 | 字体字典、程序、编码、文本提取（语法层） | 7 | 4 | A | 🚧 |
| C 内容流读取模型 | 通用 Op/资源/状态/标记内容读取，解除第 8/9/11/14 章读端缺口 | 5 | 4 | A | 🚧 |
| D 图形完整 | 颜色/图像/Shading/透明度的读端模型与写端收尾 | 7 | 0（写端基线见 D0） | A、B、C（D3/D4 只需 A） | 🚧 |
| E 交互与文档交换 | 导航/注解/表单/元数据/嵌入文件/增量更新 | 7 | 0 | D（E1/E3/E5 只需 A） | ⬜ |
| F 专门领域 | 签名/Tagged PDF/预印/多媒体/其他交换能力 | 5 | 0 | E | ⬜ |

> 计数口径：只统计仍需推进的「增量」；阶段内已完成的历史工作以「基线」列出并指向矩阵，不重复计入增量数。阶段状态以「阶段目标是否全部达成」为准，不以历史基线多少为准。

## 阶段 A：核心读入与安全 🚧

**目标**：常见 PDF 1.0–1.7 的语法、文件结构、核心过滤器、加密与函数可读；读端对损坏输入返回类型化错误而非 panic。

**已完成基线**（证据与测试名见 `FEATURE_MATRIX.md` 第 7 章对应行）：

- [x] A1 词法与对象模型：7.2、7.3.2–7.3.10；嵌套上限、流长度与间接引用解析。
- [x] A2 文件结构与交叉引用：7.5.2–7.5.8（经典表/ XRef 流/ ObjStm/ `/Prev` 链/混合引用）、7.7.2 Catalog 版本覆盖。
- [x] A3 核心过滤器：7.4.1–7.4.6、7.4.8、7.4.4.4（Flate/LZW/CCITT/DCT/ASCIIHex/ASCII85/RunLength/Predictor），第三方后端经 trait 隔离。
- [x] A4 安全：7.6.2/7.6.3/7.4.10 的 Standard Security Handler R2–R4 解密（RC4-40/128、AESV2、`/EncryptMetadata false`、`/Crypt`），公钥与 R5/R6 明确不支持。
- [x] A5 Functions 与线性化容忍：7.10 Type 0/2/3/4 读端求值；附录 F 线性化文件按普通路径完整可读（不利用 hint 流）。

**待推进增量**：

- [x] **A6 真实样本回归与 fixture 纪律**（原则 5、工程约定 #8）：把 `tests/fixtures/minimal-document.pdf` 与 `tests/fixtures/pdflatex-4-pages.pdf` 接入自动化测试（文档解析、页面树遍历、文本提取冒烟），在 fixtures 表补齐来源/许可/体积；用至少一个真实 PDF 验证损坏输入不 panic。`tests/main.rs` 已超限，接入前按「工程债」要求先把对应主题迁出为独立测试模块。
  - 退出标准：两个 fixture 在 `cargo test` 中自动运行（新增用例位于拆分后的主题模块）；二者的来源/许可/体积已登记；无法追溯许可的既有 fixture 由 A6 决定替换为自生成样本并记录决策；截断副本返回类型化错误而非 panic。
  - **完成记录（2026-10-26）**：新增 `tests/fixtures.rs` 集成测试（5 项：`minimal_document_parses_without_error`、`pdflatex_4_pages_parses_without_error`、`truncated_minimal_document_returns_typed_error`、`truncated_pdflatex_4_pages_returns_typed_error`、`corrupted_pdf_returns_typed_error_not_panic`）；fixtures 表已登记体积，来源/许可标「待追溯」；既有 fixture 许可未确认，待后续决策是否替换为自生成样本。

**完成口径**：仅指上述 A1–A6。本阶段验收**只看 Reader 列**：A 范围内条目 Reader ✅ 即达成基线，Writer 侧缺口不参与本阶段判定（例如过滤器 Writer 列为 🟡 是「透传/剥离，不主动编码」的已知策略）。以下**不属于**阶段 A，避免「A 完成 = 所有真实 PDF 可读」的误读：

- JBIG2/JPX 解码 → D3；
- 图像字典/内联图像/图像蒙版读端 → D2；
- 通用内容流 Op 模型 → C；
- 增量更新写出 → E7；
- Name Dictionary（7.7.4 `/Names`）→ E1；Filespec/`/EmbeddedFiles` → E6；Extensions 字典 → F5；
- 非 ISO 32000-1 的 `<~ ~>` ASCII85 字符串扩展（矩阵 7.3.4 扩展行）不进入阶段 A；
- 权限位解释（矩阵 7.6.3 权限行）→「未分配条目」；
- 加密写出、公钥安全处理器、PDF 2.0 R5/R6 → 非目标（见文末）。

**验收标准**：

- 含上述过滤器的合成样本与真实样本可解析，解码字节正确；损坏/截断样本返回类型化错误；
- 线性化样本按普通路径读取无错误；
- 真实样本 fixture 已接入自动化测试，来源/许可/体积已登记；
- `cargo test` 全绿；除上文已排除的条目（7.3.4 扩展、7.4.7/7.4.9、7.7.4、7.11、7.12、7.6.3 权限行）外，A1–A5 所列章节的矩阵条目 Reader 均为 ✅；上述排除条目的归属见「不属于阶段 A」与「未分配条目」；A6 的 fixture 要求全部满足。

## 阶段 B：字体与文本 🚧

**目标**：能读写 ISO 32000-1 定义的简单字体、Type 0/CID 字体、Type 3 字体与嵌入程序，并提取 Unicode 文本；不做 hinting/渲染。

**已完成基线**（证据见 `FEATURE_MATRIX.md` 第 9 章）：

- [x] B1 简单字体与编码：9.6.2/9.6.6（Standard 14、`/Widths`、`/FontDescriptor`、五种预定义编码 + `/Differences` + AGL；写端宽度/编码/描述符）。
- [x] B2 TrueType 程序：9.6.3（SFNT、cmap format 0/2/4/6/8/10/12/13、水平/垂直度量、复合字形、内建编码、OpenType `OTTO` 读端）。
- [x] B3 CID/Type 0 核心：9.7.4/9.7.5/9.7.6（CIDFontType0/2、`CIDToGIDMap`、W/DW、DW2/W2、Identity-H/V 与嵌入 CMap、CFF 轮廓经 trait 适配）。
- [x] B4 嵌入程序与子集：9.9/9.6.4（Type 1/TrueType/CFF/OpenType 嵌入、`Length1–3`、TrueType/Type 1/CID-CFF/name-keyed CFF 子集、六字母标签、`/CIDSet` 裁剪）。

**待推进增量**：

- [ ] **B5 Type 3 完善**（9.6.5，🟡）：按 Table 112 补齐**字体级** `/Resources` 的读写（多级间接引用、资源名解析）与 Table 112 要求的 `/Name`、`/FontDescriptor` 条目，以及可选 `/ToUnicode`；无 `/BaseEncoding` 时的精确码位归属（当前以 StandardEncoding 近似）；`d0`/`d1` 指标与文本提取已有。注意：Table 112 只有一个字体级 `/Resources`，CharProcs 内不存在每字形资源字典。
  - 退出标准：Type 3 字体字典的 Table 112 条目可写→解析→提取往返；字体级 `/Resources` 经多级间接引用可读回；`/Name`/`/FontDescriptor`/`/ToUnicode` 读写往返；负向用例覆盖缺失/非法资源与缺失必需条目。
- [ ] **B6 CMap/ToUnicode 完整性**（9.7.5、9.10.3，🟡）：补全 Adobe-GB1/CNS1/Japan1/Korea1 ordering 数据（当前为紧凑近似）；读端 ToUnicode `usecmap` 继承基座映射；写端 ToUnicode 支持 `usecmap` 继承（可选）。
  - 启动前置：先确定 ordering 数据来源、许可证与体积上限并登记到 fixtures 表，不得引入许可不明的 Adobe 数据文件；`reader/font/cid_cmap.rs` 已超限，触及前先拆分。
  - 退出标准：预定义 CMap 与完整 ordering 样本的字符码→CID 映射正确；数据来源/许可/体积已登记；`usecmap` 基座覆盖/裁剪有测试；`text_code_bytes*` 不再静默跳过非法长度。
- [ ] **B7 文本提取语义完善**（9.10，🟡）：跳过 `/Artifact` 标记内容、提供可选的带位置/间距的文本片段 API（不做版面分析）；`/ActualText` 替换与隐藏 OCG 过滤已随 C4 落地，B7 只需补回归样本与 OCMD 可见性。
  - 退出标准：`ActualText`、Artifact、隐藏 OCG 三类样本的提取结果正确；现有文本提取测试无回归；`try_extract_page_text` 错误语义保持。

**非目标（本阶段明确不做）**：hinting、字形光栅化、版面分析、CFF2 子集化、TTC 子集化、OpenType 子集化（保留程序透传 + 子集标签）。

**依赖**：A（过滤器/流解码）。B5 只依赖 A；B7 依赖 C4（标记内容读取模型，已完成），可后置；B6 只触及字体模块。

**验收标准**：

- 写出含嵌入 TrueType、CIDFontType2、Type 1/CFF 子集的文档，重新解析后字体程序/宽度/文本一致；
- 含 CJK 文本的样本可提取为 Unicode；ToUnicode 往返可还原；
- Type 3 字典（含字体级 `/Resources`、`/Name`、`/FontDescriptor`）与 CharProcs 指标往返；
- `cargo test` 全绿。

## 阶段 C：内容流读取模型（跨章节前置）🚧 (4/5)

**目标**：提供通用的内容流解析、资源解析与图形状态模型，作为第 8/9/11/14 章读端能力的共同底座；本阶段只做**模型与数据**，不做渲染。

**为什么单列**：当前库只有写端 `content::ContentStream`，读端仅有 `reader/text` 为文本提取做的词法扫描。这导致矩阵第 8/9/11/14 章大量 Reader 列为 ⬜，也让颜色空间、Shading/Pattern、图像、标记内容等读端能力无法复用。该模型是所有后续读端增量的硬前置。

**增量**：

- [x] **C1 内容流词法与 Op AST**：复用并重构 `reader/text/tokenizer.rs`，解析操作符、操作数、内联图像 `BI/ID/EI` 边界、注释/空白/名称转义；未建模操作符保留原始 token 序列以便重新序列化。新增 `reader::content` 目录模块（触及 `reader/text/mod.rs` 前先拆分）。
  - 退出标准：合成内容流覆盖全部 ISO 32000-1 Table A.1 操作符；内联图像二进制数据不被误分词；未知操作符可往返；现有文本提取测试全部通过。
  - 测试：`parse_graphics_state_operators`、`parse_path_construction_operators`、`parse_path_painting_operators`、`parse_text_object_operators`、`parse_color_operators`、`parse_unmodeled_operator`、`parse_empty_stream`、`parse_comments_and_whitespace`、`parse_tj_array_operand`、`parse_dash_pattern_array_operand`、`parse_empty_dash_pattern_array_operand`、`tj_array_with_hex_string_elements`、`tj_rejects_non_array_operand`、`tj_rejects_unterminated_array`、`tj_rejects_name_element_in_array`、`d_rejects_non_numeric_array_element`、`d_rejects_missing_phase_operand`
- [x] **C2 资源解析与页面内容 API**：`Document::page_content_ops`（或等价 API）遍历页面树、合并 `/Contents` 数组、解析 `/Resources` 继承、解析 `/XObject`/`/Font`/`/ColorSpace`/`/Shading`/`/Pattern`/`/Properties` 名称。Form XObject 不在 API 内自动递归：调用方按 `Op::PaintXObject` + `page_resource("XObject", name)` 自行展开（递归深度与防环由调用方控制；C3 状态机只对 `q/Q` 栈限深，不约束 XObject 展开）。
  - 退出标准：`tests/fixtures/pdflatex-4-pages.pdf` 四页均可解析；资源名经 `#xx` 转义可解析；`/Parent` 资源继承有测试。
  - 测试：`page_content_ops_parses_simple_page`、`page_content_ops_returns_error_for_invalid_page_index`、`page_resource_resolves_font_name`、`page_resource_inherits_from_parent`、`page_resource_returns_none_for_missing_resource`、`page_resource_decodes_name_escapes`、`page_content_ops_handles_contents_array`、`decode_name_escapes_handles_various_cases`、`pdflatex_4_pages_all_content_ops_parse`
- [x] **C3 图形状态机**：`q/Q` 栈、CTM、线宽/线帽/线接/斜接/虚线、文本状态、`gs` 应用 ExtGState、颜色/图案当前值；状态复制与恢复语义按 8.4/9.3/11.x。
  - 退出标准：嵌套 `q/Q`、`cm`、`gs`、`Tf/Td/Tm` 的状态快照测试；越界/非法参数返回类型化错误。
  - 实现范围：`reader/content/state.rs` 新增 `ContentStateRunner`/`ContentState`（Table 57 初始值、Table 56 `Q` 空栈为 no-op、`MAX_SAVE_DEPTH`=512 限深）、`run_content_state` 逐操作符快照、`Document::page_content_state_trace`/`page_content_state`（经 `/Resources/ExtGState` 应用 `gs` 的 LW/LC/LJ/ML/SA/D/RI/Font/CA/ca/BM/OP）；文本状态按 9.3/9.4（BT 仅重置 Tm/Tlm、Td 平移 Tlm、TD 设置 leading=-ty、T*/'/" 依 leading 换行）；颜色/图案当前值跟踪 rg/RG/g/G/CMYK/scn/SCN/CS/cs 与分量范围校验。
  - 测试：`q_q_stack_restores_ctm_and_line_width`、`q_stack_underflow_has_no_effect`、`cm_concatenates_matrices_in_prepend_order`、`save_stack_depth_limit_returns_error`、`gs_applies_ext_g_state_parameters`、`unknown_gs_name_returns_typed_error`、`gs_font_name_entry_sets_text_font`、`invalid_state_parameters_return_typed_errors`、`non_finite_matrix_returns_typed_error`、`bt_resets_text_matrices_keeps_text_state`、`td_td_and_t_star_update_matrices`、`tick_and_quote_show_text_move_to_next_line`、`color_operators_track_current_values`、`named_pattern_color_operators_are_modeled`、`out_of_range_device_color_components_return_typed_errors`、`pdflatex_4_pages_state_machine_runs`
  - 剩余缺口：路径/裁剪/图像/Form XObject 的绘制语义不进入状态模型（仅几何与参数状态）；`gs` 的 `/Font` 名称形式只记录名称不解析字号引用；内联图像 `BI…EI` 由分词器整段跳过，`Op::InlineImage` 路径不可达（见矩阵 8.9.7，D2 处理）；颜色分量个数与 `/ColorSpace` 基色空间的匹配依赖 D1 读端模型。
- [x] **C4 标记内容与可选内容读取语义**：`BMC/BDC/EMC/MP/DP` 嵌套与属性表解析、`/ActualText`/`/E` 提取、OCG 可见性过滤；与 14.6/8.11 的读端模型统一。
  - 退出标准：嵌套标记内容配对错误可诊断；`ActualText` 替换与隐藏 OCG 过滤有测试；属性表支持名称与内联字典。
  - 实现范围：分词器补 `CsToken::Dictionary`（`take_dictionary`，嵌套/字符串感知，`(a>b)` 不提前闭合）并修复名称分词吞分隔符（`]`）的缺口；`parser.rs` 新增 `parse_dictionary_inner` 宽松字典构建（嵌套字典/数组/`num num R` 引用（顶层与数组内）/布尔/字符串/十六进制串）。新增 `reader/content/marked.rs`：`collect_marked_sections`（`BMC/BDC/EMC/MP/DP` 栈配对，`MAX_MARKED_DEPTH`=64 限深，类型化 `MarkedContentError`：`UnmatchedEnd`/`UnclosedSection`/`DepthExceeded`）、`MarkedSection`（span、`point`、`/ActualText`/`/E`/`/Alt`/`/Lang`，字面与 UTF-16BE 十六进制串解码）、属性表支持内联字典与 `/Properties` 名称解析（含兼容写端「名称直指 OCG 字典」形态）、`resolve_marked_visibility` 经目录 `/OCProperties` 判定 OCG 可见性（`/D` BaseState/ON/OFF）、`Document::page_marked_content`。文本提取端（`reader/text`）统一语义：`MarkedFrame` 栈实现隐藏范围不提取、`/ActualText` 整段替换（emit-once）、嵌套继承隐藏态、`EMC` 宽容失衡、Form XObject 内未闭合范围不外泄；`OptionalContentProperties::is_visible` 复用。
  - 测试：`pairs_begin_and_end_with_spans`、`nests_sections_and_closes_inner_first`、`point_operators_are_zero_length_sections`、`bdc_resolves_inline_dictionary_properties`、`bdc_resolves_property_name_through_resources`、`bdc_name_pointing_at_ocg_dictionary_marks_group`、`unmatched_emc_reports_operator_index`、`unclosed_section_reports_outermost_tag`、`depth_limit_is_enforced`、`text_entries_decode_literal_and_hex_strings`、`visibility_resolves_oc_name_and_reference`、`page_marked_content_resolves_properties_and_visibility`、`bdc_keeps_inline_dictionary_property_list`、`bdc_property_dictionary_supports_nested_dict_array_and_reference`、`dp_accepts_property_name_or_dictionary`、`hex_string_operand_in_dictionary_keeps_hex_characters`、`dictionary_with_string_containing_angle_brackets_roundtrips_entries`、`actual_text_replaces_all_shows_in_section`、`actual_text_decodes_hex_string_property`、`bdc_property_name_resolves_through_properties_resource`、`hidden_ocg_section_is_not_extracted`、`visible_ocg_section_is_extracted`、`hidden_ocg_property_resolved_by_direct_reference`、`bdc_name_pointing_at_ocg_dictionary_filters_hidden_group`、`nested_section_inherits_hidden_state_from_parent`、`unbalanced_emc_does_not_break_extraction`、`unclosed_marked_section_in_form_does_not_leak_to_caller`、`pdflatex_4_pages_marked_content_pairs_with_spans`（写→读往返 `builder_optional_content_roundtrips_through_parser` 同步升级为隐藏层不提取断言）
  - 剩余缺口：OCMD 与内联 OCG 字典形态的可见性未求值（按可见处理）；间接属性表操作数（`5 0 R /Tag BDC`）不支持；`page_marked_content` 不递归 Form XObject；`/E` 已暴露但未参与提取替换；`/Artifact` 范围识别与位置感知 API 留给 B7；`reader/optional_content.rs` 模块拆分未做。
- [ ] **C5 读写模型统一与 roundtrip**：读端 Op AST 与写端 `content::Op` 的映射表；parse→model→serialize→parse 的往返测试；未建模操作符透传策略记录在矩阵。
  - 退出标准：覆盖路径/文本/颜色/XObject/Shading/Pattern/标记内容的往返样本；序列化字节可被现有 parser 重新解析；无 panic。

**依赖**：A。B 为软依赖（文本提取可在 C1/C2 完成后迁移到统一模型，但不得为迁移而扩大本阶段范围）。

**验收标准**：

- 真实多页 PDF 的内容流可完整解析，操作符计数与资源引用可核对；
- 内联图像、Form XObject、`q/Q`、`gs`、标记内容均有测试；
- 文本提取在切换到统一模型后结果不回归；
- `cargo test` 全绿；矩阵第 8/9/11/14 章中「无通用内容流读端」的备注可被逐项消除。

## 阶段 D：图形完整（读端模型 + 写端收尾）🚧

**目标**：把已完成的图形写端能力补齐读端模型，并补齐写端剩余条目；颜色/图像/Shading/Pattern/透明度达到语法层完整。

**D0 写端基线（已完成，证据见矩阵第 8/11 章）**：

- [x] 8.4.4/8.4.5 ExtGState 读写（`/TR` `/BG` `/BG2` `/UCR` `/UCR2` 已接入；`/TR2` `/HT` 见 D6）；
- [x] 8.6.4–8.6.8 设备/CIE-Based/Indexed/Separation/DeviceN 颜色空间、灰度/CMYK/颜色操作符；
- [x] 8.7 Shading Type 1–7、着色图案与彩色/无色平铺图案；
- [x] 8.9/8.9.6 图像字典常用条目、图像蒙版、软蒙版图像；
- [x] 8.10.1–8.10.3 Form/Group XObject；11.3.5/11.6.4/11.4/11.6.5/11.6.6 透明度写端；
- [x] 14.6 标记内容操作符；8.11 可选内容（OCG/OCProperties/Properties/BDC）。
- 说明：8.8.2 PostScript XObject、8.10.4 Reference XObject 不在基线内。

**待推进增量**：

- [ ] **D1 颜色空间读端模型**（8.6.4–8.6.8）：解析 `/ColorSpace` 资源（设备、CalGray/CalRGB/Lab/ICCBased、Indexed、Separation/DeviceN、Pattern），支持 `/DefaultGray`/`/DefaultRGB`/`/DefaultCMYK`；Indexed 还原到基色空间；Separation/DeviceN 经 tint 函数求值到备用空间；输出分量数组与空间描述。
  - 退出标准：设备/CIE/Indexed/Separation/DeviceN 样本的样本值解码正确；ICCBased 返回配置文件字节与 `/N`；非法空间返回类型化错误。
- [ ] **D2 图像字典、图像蒙版与内联图像读端**（8.9.5/8.9.6/8.9.7）：建模 `/Width`/`/Height`/`/ColorSpace`/`/BitsPerComponent`/`/Decode`/`/Interpolate`/`/Intent`/`/ImageMask`/`/Mask`/`/SMask`/`/SMaskInData`/`/Alternates`；解码内联图像 `BI/ID/EI` 字典与数据；提供样本读取 API（不做合成/渲染）。
  - 退出标准：内联图像与图像 XObject 的字典/样本可读；Stencil/Explicit/Colour Key 蒙版结构可解析；`/Alternates` 列表可遍历；损坏/截断图像数据返回错误。
- [ ] **D3 JBIG2/JPX 解码**（7.4.7/7.4.9）：启动前先做后端选型 spike（纯 Rust 实现 vs 第三方适配器，比较许可证、体积、维护状态与可测试性）并把结论写入增量启动说明；再定义 `Jbig2Backend`/`JpxBackend` trait 与可替换适配器；接入 `StreamFilter` 注册；补 1.4/1.5 版本门控与损坏输入测试。
  - 退出标准：选型结论已记录；规范/公开样本可解码为位图/分量数据；trait 有替代实现测试；损坏流不 panic；写端仍按透传/剥离策略处理。
- [ ] **D4 写端图像压缩**（7.4.4/7.4.8 Writer 列）：定义 `DeflateBackend` 编码 trait，提供 FlateDecode 编码路径（可选 Predictor）；图像/内容流写入时按调用方选择压缩；DCT 保持透传。更新矩阵过滤器 Writer 列与 README 体积说明。
  - 失败语义（单一契约）：默认纯 Rust Flate 后端始终可用；调用方注入的后端失败时必须返回类型化 `WriteError` 且不写出任何字节，**不得静默回退明文**；未请求压缩时保持明文透传。
  - 退出标准：写入→读端解码的字节一致；压缩流体积小于原始流；后端失败路径有测试且不写字节；特性与版本门控正确。
- [ ] **D5 Shading/Pattern 读端模型**（8.7）：解析 Shading 字典 Type 1–7、网格流（BitsPerCoordinate/Component/Flag、Decode）、PatternType 1/2、图案流与 `/Resources`；校验域/坐标/函数维数；输出几何与颜色函数描述。
  - 退出标准：Type 1–7 合成样本可读；网格流位打包与 `/Decode` 还原正确；着色/平铺图案资源可解析；非法条目返回错误。
- [ ] **D6 ExtGState/透明度读端应用与写端剩余**（8.4.4/8.4.5、11.x）：`gs` 接入 C3 状态机已完成（LW/LC/LJ/ML/SA/D/RI/Font/CA/ca/BM/OP）；剩余写端 `/TR2`、`/HT`（半色调对象模型）、SMask `/TR`、`/SMaskInData`；读端 `/Group`、软蒙版图像、`/Matte`（Table 146）结构解析。
  - 退出标准：`gs` 应用后状态快照正确；`/TR2`/`/HT` 写出可被读端解析；`/Matte` 与 `/SMaskInData` 读写样本通过；透明度版本门控保持。
- [ ] **D7 写端图像/表单补全**（8.9.5、8.10、11.5）：`/Intent`、`/Alternates` 图像、`/Matte`、Form XObject 自有 `/Resources`/`/OC`/`/Ref`（按价值取舍）、Reference XObject 评估；补齐相应版本门控与往返测试。
  - 退出标准：新增字典条目可写→解析→读回；非法参数不写字节；第三方阅读器语法检查无错误（人工验收）。

**依赖**：D0 写端基线已具备；逐增量依赖如下——D3/D4 **只依赖 A**，不依赖 C，可与 C 并行；D1 依赖 C2/C3；D2 依赖 C2（资源与内容 API）；D5 依赖 C2/C3；D6 依赖 C3；D7 依赖 D0，其中 Form 自有 `/Resources` 部分与 C2 的资源模型对齐。B 仅在 Pattern 内字体与文本场景为软依赖。

**验收标准**：

- 含 ICCBased/Separation/Pattern/图像/Shading/透明组的样本可读入为模型对象；
- 写端剩余条目补齐后，矩阵第 8/11 章 Writer 列无未解释的 🟡；
- 真实样本（含 JBIG2/JPX 图像）可解析，损坏样本不 panic；
- `cargo test` 全绿。

## 阶段 E：交互与文档交换 ⬜

**目标**：导航、注解、表单、元数据、嵌入文件与增量更新基础可用。

**增量**：

- [ ] **E1 导航与页面标签**（12.3、12.4.2、12.7.6）：显式/命名 Destinations、Outline、Page Labels、Named Pages（`/Pages` 名称树）。本增量拥有 7.7.4 `/Names` 名称字典的语义封装（解析、查找、写回），作为 E6 的前置，不得由 E6 另建一套。
- [ ] **E2 注解通用结构与常用类型**（12.5.1–12.5.6）：Annotation 字典/标志/边框/AP 外观流；先做 Text、Link、FreeText、Highlight、Underline、Squiggly、StrikeOut、Stamp、Ink、Popup、FileAttachment、Widget（Widget 与 E4 联动）。
- [ ] **E3 动作**（12.6）：GoTo/URI/Named 优先；GoToR/Launch/JavaScript 等按需增补；动作字典读写与注解/大纲联动。
- [ ] **E4 AcroForm**（12.7.2–12.7.5）：AcroForm 字典、字段字典、文本/按钮/选择字段、Widget 注解、字段动作；外观流可由 D0 的 Form XObject 写端生成。
- [ ] **E5 元数据与文件标识**（14.3.2、14.3.3、14.4）：`/Info` 语义封装、XMP `/Metadata` 读写（先透传/基本包，后做属性访问）、`/ID` 生成/更新/校验（经典 xref 路径补 `/ID`）。本增量拥有 `/ID` 生成与校验的唯一实现，E7 复用。
- [ ] **E6 嵌入文件**（7.11）：Filespec、`/EmbeddedFiles` 名称树、嵌入文件流；名称树复用 E1 的 `/Names` 语义封装；与 FileAttachment 注解联动。
- [ ] **E7 增量更新写出**（7.5.6 Writer）：`/Prev` 链、新/自由 xref 条目、trailer 合并、`/ID` 更新；复用 E5 的 `/ID` API，不另造生成/校验逻辑；为 F1 数字签名提供基础。

**依赖**：E1/E3/E5 只依赖 A；E2 依赖 D0 的 Form XObject 写端；E4 依赖 E1/E3 与 D0；E6 依赖 E1（`/Names`）；E7 依赖 E5（`/ID`）与 A（xref 读写），可与 E2/E4 并行。F1 依赖 E7。

**验收标准**：

- `tests/fixtures/annotated.pdf` 可读，注解/动作结构可遍历；生成含 Link 注解的文档可点击跳转（人工验收）；
- AcroForm 文本字段 + Widget 写出后可被第三方阅读器识别；
- XMP 与 `/Info` 读写一致，`/ID` 生成符合规范算法；
- 增量更新文件可被读端与第三方阅读器按 `/Prev` 链读取；
- `cargo test` 全绿。

## 阶段 F：专门领域 ⬜

**目标**：按需求逐步立项，不预设排期。

**增量**：

- [ ] **F1 数字签名**（12.8）：ByteRange、签名字典、DocMDP/UR/FieldMDP、PKCS#1/PKCS#7；密码学经 `SignatureBackend` trait 隔离；依赖 E7。
- [ ] **F2 逻辑结构/Tagged PDF/无障碍**（14.7–14.9）：结构树、结构类型/属性、`/ActualText`/`/Alt`/`/Lang`、角色映射、无障碍检查。
- [ ] **F3 预印**（14.11、11.7.4）：页面边界、打印机标记、分色字典、输出意图、陷印、OPI；叠印控制已在 8.6.7 完成。
- [ ] **F4 多媒体**（13.2–13.6）：Renditions、Sound、Movie、Alternate Presentations、3D；均为 P3，按使用场景立项。
- [ ] **F5 其他交换能力**（14.10、7.12、附录 F）：Web Capture、Extensions 字典、线性化输出与 hint 流、PDF 2.0 差异评估。

**依赖**：F1 依赖 E7；F2 依赖 C4（标记内容读取模型）；F3 依赖 A 与 D0（页面级图形对象写端）；F4 依赖 E2/E3（注解/动作）；F5 依赖 A，线性化输出可选复用 E7。各项彼此独立，可任意顺序立项。

**验收标准**（逐项直接写入本文档，不引用外部 spec）：

- **F1**：ByteRange 覆盖签名字典之外的全部字节且边界正确；生成的签名可被外部工具验证（人工验收）；DocMDP/FieldMDP 变换限制可读；增量更新文件能定位签名字典。
- **F2**：结构树/结构元素读写往返；`/ActualText`/`/Alt`/`/Lang` 可提取；角色映射与结构类型可解析；非法结构条目返回类型化错误。
- **F3**：页面边界、打印机标记、分色字典、输出意图、陷印、OPI 字典可写→解析往返；第三方阅读器语法检查无错误（人工验收）。
- **F4**：Renditions/Sound/Movie/3D 字典与注释可读写往返（只做语法层，不播放、不渲染）。
- **F5**：Web Capture 与 Extensions 字典读写往返；线性化输出可被本库读端按普通路径读取，hint 流结构经语法校验。
- 每项完成必须同步矩阵状态与证据，不得以「部分可用」标 ✅。

## 非目标与明确不支持

以下条目不进入开发计划，矩阵状态标 ❌ 或保留 ⬜ 并注明「非目标」：

- **第 10 章渲染**：CIE→Device 转换、Device 空间互转、Transfer Function 渲染语义、半调、扫描转换/flatness/smoothness。本库为语法层；如未来需要渲染，另立渲染器项目，不在本路线图内。
- **12.7.8 XFA**：PDF 2.0 已弃用，评审决定不投入（矩阵标 ❌）。
- **8.8.2 PostScript XObjects**：不执行 PostScript；仅作为对象透传。
- **14.2 Procedure Sets**：标准侧已弃用，无实现价值。
- **PDF 2.0 输出**：写端仅 1.0–1.7；读端容忍 2.0 头部。R5/R6 解密、公钥安全处理器、加密写出均不计划。
- **CFF2/TTC 子集化、OpenType 子集化**：不计划；OpenType 程序保留透传 + 子集标签。
- **字体 hinting/光栅化**：不计划；仅提供轮廓、度量与文本提取数据。

## 未分配条目（待决策）

以下矩阵条目不归属任何阶段增量，也**不阻塞**任何阶段验收；决定立项时先在本文件补增量、依赖与验收口径。

| 条目 | 矩阵行 | 优先级 | 待决策点 |
|---|---|---|---|
| ASCII85 字符串扩展 `<~ ~>`（非 ISO 32000-1） | 7.3.4（扩展） | P3 | 读端无解析路径、写端类型为死代码；补读端、保留或删除需决策 |
| 混合引用文件写出（trailer `/XRefStm`） | 7.5.8.4 | P3 | 读端已支持；是否需要写出「经典表 + xref 流」的混合引用文件未定（E7 仅覆盖增量更新） |
| 权限模型（`/P` 位解释） | 7.6.3（权限行） | P2 | 认证已读取 `/P` 派生密钥；加密写出为非目标，是否公开只读权限位 API 需产品决策 |
| Multiple Master 字体（MMType1） | 9.6.2.3 | P3 | 现有编码路径接受 MMType1（9.6.6），但字体程序/实例语义未建模，是否值得投入 |
| FDF（Forms Data Format） | 12.7.7 | P3 | 与 AcroForm（E4）的数据交换场景是否在范围内 |
| Measurement Properties | 12.9 | P3 | 与预印/工程文档场景绑定，等 F3 需求明确 |
| Document Requirements | 12.10 | P3 | 标准定义的需求处理器生态已边缘化，是否投入 |
| Page-Piece 字典 | 14.5 | P3 | 应用私有数据，通用字典透传已够用，是否需要类型化封装 |

## 下一批建议增量（按依赖与收益排序）

1. **C5 读写模型统一与 roundtrip**：阶段 C 收尾项，依赖 C1–C4（均已完成）。
2. **D3 JBIG2 解码**：真实扫描件（MRC）常见，独立于 C；先完成后端选型 spike。
3. **D4 写端 Flate 压缩**：对生成文件体积收益最大，独立于 C。
4. **D2 图像字典/内联图像读端**：依赖 C3（已完成），含解析 `BI…EI` 不可达路径的整改；单独评审。
5. **B6 ToUnicode/CMap 完整性**：CJK 文本正确性的关键，可与 C/D 并行；启动前拆分 `cid_cmap.rs` 并确定数据许可。
6. **D1 颜色空间读端模型**：为 Shading/Pattern/图像读端提供基础。

并行边界：C5 依赖 C1–C4（均已完成）；D2 依赖 C3（已完成）；D3/D4 只依赖 A；B6 只触及字体模块，可与 C/D 并行；B7 的 C4 依赖已满足，剩余 `/Artifact` 与位置 API 可独立推进。

## 真实样本与 fixtures backlog

| fixture | 用途 | 来源/许可 | 体积 | 当前状态 |
|---|---|---|---|---|
| `tests/fixtures/minimal-document.pdf` | 基础解析回归 | 待追溯 | 17 KiB | ✅ A6 接入（`tests/fixtures.rs`） |
| `tests/fixtures/pdflatex-4-pages.pdf` | 页面树/内容流/文本提取 | 待追溯 | 25 KiB | ✅ A6 接入解析；C2 接入内容流；C3 接入状态机 |
| `tests/fixtures/annotated.pdf` | 注解/动作 | 待追溯 | 1.8 KiB | 已存在，未接入测试；E2/E3 接入 |
| `tests/fixtures/xmp-metadata.pdf` | XMP/Info | 待追溯 | 2.1 KiB | 已存在，未接入测试；E5 接入 |
| 待补：JBIG2 扫描件 | D3 | 需许可安全的公开样本或自行生成 | — | 未获取 |
| 待补：JPX 图像 | D3 | 可使用公开测试图或生成工具（须登记许可） | — | 未获取 |
| 待补：Shading/Pattern/透明组 | D5/D6 | 优先用 builder 生成合成样本，再补真实样本 | — | 未获取 |
| 待补：AcroForm | E4 | 自行生成最小表单样本 | — | 未获取 |
| 待补：CJK CID/Type 0 | B6 | 使用开源字体生成最小样本；ordering 数据许可须单独登记 | — | 未获取 |
| 待补：增量更新/签名 | E7/F1 | 自行构造 v1 + update 样本 | — | 未获取 |

> 纪律：真实 PDF 必须有可追溯来源与许可；无法确认许可的样本不得入库。合成样本由 builder 或脚本生成，生成脚本随 fixture 一并入库。既有 4 个 fixture 来源待追溯，A6 已接入测试，后续决策是否替换为自生成样本。

## 阶段依赖图

```mermaid
flowchart LR
    A["A 核心读入与安全 🚧"] --> B["B 字体与文本 🚧"]
    A --> C["C 内容流读取模型 🚧"]
    B --> D["D 图形完整 🚧"]
    C --> D
    A --> D
    A -.-> E["E 交互与文档交换 ⬜"]
    D -.-> E
    E --> F["F 专门领域 ⬜"]
    A --> E7["E7 增量更新"]
    E5["E5 元数据与 /ID"] --> E7
    E7 --> F1["F1 数字签名"]
```

实线表示阶段完成顺序，虚线表示 E 阶段的 E1/E3/E5 等增量只依赖 A、可在 D 完成前启动（逐增量依赖以各阶段正文为准）。依赖构成无环 DAG：A → B/C → D → E → F；B 与 C 可并行；E7 是 F1 的额外前置。

## 阶段映射（旧 → 新）

| 旧规划 | 新规划 | 说明 |
|---|---|---|
| 阶段 A 读得全 ✅ | 阶段 A 核心读入与安全 🚧 | 收窄完成口径；JBIG2/JPX/图像字典/内容流读端移出；评审后回退为 🚧 并补 A6 fixture 回归 |
| 阶段 B 文本可用 🚧 | 阶段 B 字体与文本 🚧 | 条目合并为 7 个增量；完成基线 B1–B4 |
| 阶段 C 图形完整 🚧 | 阶段 C 内容流读取模型 ⬜ + 阶段 D 图形完整 🚧 | 拆出跨章节前置的读端内容流模型 |
| 阶段 D 交互与交换 ⬜ | 阶段 E 交互与文档交换 ⬜ | 顺延；新增 E7 增量更新 |
| 阶段 E 专门领域 ⬜ | 阶段 F 专门领域 ⬜ | 顺延；签名/线性化等重新分组 |

## 维护说明

- 本文件只写「计划、依赖、验收」，不复制测试名；测试证据以矩阵为准。
- 每次增量完成后更新阶段总览的「已完成」计数；若增量拆分/合并，先更新本文件再动代码。
- 新增条目必须给出 ISO 32000-1 章节号与矩阵行；章节号以 `docs/PDF32000_2008/` 为准。
- 规划调整需在「阶段映射」或本文件顶部记录日期与原因，避免历史记忆与文档冲突。
