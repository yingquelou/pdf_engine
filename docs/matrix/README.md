# PDF 标准特性矩阵（事实层）

> 职责：登记 ISO 32000-1:2008 第 7–14 章全部特性在本库的读写状态与证据，回答「标准规定了什么、已实现什么、还缺什么」。
> 禁止：排期、阶段归属、测试计数、历史叙述（分别属于 `roadmap/`、`changelog/`）。

## 数据源与口径

> **目的**：以 ISO 32000-1:2008（PDF 1.7）第 7–14 章章节体系为骨架，列出标准全部特性在本库中的实现状态，回答「标准规定了什么、已实现什么、还缺什么」。
>
> **数据源**：
> - 章节号与章节名：ISO 32000-1:2008 本地全文 `docs/PDF32000_2008.pdf`（权威副本）。核对方法：`pdftotext -raw` 抽取为纯文本后按条款号/表号检索（多列表格**勿用 `-layout`**，该模式会列错位、导致表号张冠李戴）。原引用的 `docs/PDF32000_2008/`（HTML 导出目录）在本仓库不存在，2026-11-02 评审已删除该引用；
> - 状态与证据：2026-10-26 对 `src/core` 与 `tests` 的源码实测审计（证据列直接给出文件/符号/测试名；早期引用的 `.trae/specs` 审计文件已不存在，不再作为数据源）。
>
> **与 roadmap/README.md 的关系**：本矩阵是事实层（现状盘点，只随代码实测更新）；`docs/roadmap/README.md` 是规划层（把本矩阵中的 ⬜/🟡 条目按价值与依赖组织为开发阶段）。ROADMAP 的阶段条目一律通过本矩阵的章节号回溯。

## 图例与合并规则

- **状态四态**：✅ 已实现 / 🟡 部分实现（缺口见备注）/ ⬜ 未实现 / ❌ 明确不支持（设计决策，附理由）
- **Reader / Writer 列**：✅ 🟡 ⬜ ❌ 同上；`—` = 不适用（该特性无读/写之分）
  - **总状态由 Reader/Writer 合并得出，不独立评级**：所有适用侧（`—` 不计）均为 ✅ 时总状态才是 ✅；均为 ❌（设计决策整体不支持）时总状态为 ❌；其余任一适用侧为 🟡/⬜/❌ 的混合情形总状态至多为 🟡。读端完全缺失（Reader ⬜）的条目不得标 ✅。
  - 约定：读端已有通用内容流读端模型（`reader/content`：分词 + `Op` AST + 图形状态机 `ContentStateRunner`，入口 `Document::page_content_ops` / `page_content_state_trace`，资源解析 `page_resource`，标记内容配对与属性表 `collect_marked_sections`/`Document::page_marked_content`，见 14.6）。第 8/9 章内容流操作符条目的 Reader 列按该模型评价：操作符进入 Op AST 且被状态机跟踪为 ✅；仅被 Op AST 建模、语义不进入状态模型（当前路径、裁剪路径、内嵌图像与 Form 字典等）为 🟡；`reader/text` 为文本提取所做的解释仍单独见 9.10。
  - 约定：除 **Flate** 外，写端**不编码过滤器**，已解码流剥离过滤器、引用流原样透传（`prepare_stream_dict_for_write`），故过滤器类条目的 Writer 列以 🟡 表示「透传/剥离，不主动编码」。D4 起 `FlateDecode` 有写端主动编码路径（`writer/compression.rs`，文档级 opt-in，见 7.4.4，该条 Writer 列已更新为 ✅）。
  - 约定（C5 未建模操作符透传策略）：读写内容模型经 `reader::content::{to_content_op, from_content_op, serialize_ops}` 桥接。写端 `content::Op` 无法表达的操作符——`TD` 双操作数、`d0`/`d1`、`BX`/`EX`、内嵌图像 `BI…EI`、一切 `Op::Unmodeled`、越界的 `J`/`j`（非 0–2）与 `Tr`（非 0–255）、非 UTF-8 的 `'`/`"`/`TJ` 字符串、非 Name/Dict 的 BDC/DP 属性表——在 `to_content_op` 返回类型化 `OpMappingError`（`NotRepresentableByWriter`/`OperandOutOfRange`），`serialize_ops` 对其走原始回退直接输出合法 PDF 语法，保证 `parse → serialize → parse` 模型级无损；`Tj` 串按字节是否为合法 UTF-8 在 `(字面量)` 与 `<十六进制>` 两种形式间自动选择。已知限制：内嵌图像 `Op::InlineImage` 自 D2 起可读写往返（`BI … ID <data> EI`）；写端 `MoveNextLineSetLeading` 单操作数 `TD` 不合规范，`from_content_op` 以 `tx=0` 有损映射且未改动该公共枚举；未建模操作数/属性字典内 `Value::String` 含 ≥0x80 字节时经 `write_value` 按 UTF-8 编码写出、重解析按字节逐 char，模型级不严格相等（ASCII 串往返无损）。
- **优先级**：P0 核心语法补全（真实世界 PDF 读入正确性）/ P1 高价值内容能力（字体文本、图像、色彩）/ P2 结构增强（Pattern、Form XObject、透明度完整、导航注解表单、元数据）/ P3 专门领域（多媒体、Tagged PDF、预印、Web Capture、签名、PDF 2.0）

> **路径说明（模块目录化后）**：下文证据中的 `reader/` / `writer/` / `types.rs` / `content/`
> 为历史引用，现分别对应目录模块 `reader/`、`writer/`、`types/`、`content/`；同理 `builder/assemble/`、`builder/types/`、`builder/navigation/structure/` 等写法
> 指同名目录模块（`…/mod.rs` 或其子模块），条目级搬迁不逐一回溯改写——新增证据请写完整模块路径；子模块清单见 `../roadmap/backlog/module-debt.md` 的「工程债：模块目录化」。

## 总览

| 章 | ✅ | 🟡 | ⬜ | ❌ | —（说明性） | 合计 |
|---|---|---|---|---|---|---|
| 第 7 章 语法 | 25 | 19 | 0 | 3 | 5 | 52 |
| 第 8 章 图形 | 6 | 16 | 1 | 1 | 5 | 29 |
| 第 9 章 文本 | 5 | 13 | 0 | 1 | 3 | 22 |
| 第 10 章 渲染 | 0 | 2 | 3 | 0 | 1 | 6 |
| 第 11 章 透明度 | 0 | 5 | 2 | 2 | 3 | 12 |
| 第 12 章 交互特性 | 3 | 35 | 2 | 1 | 1 | 42 |
| 第 13 章 多媒体 | 1 | 4 | 0 | 0 | 1 | 6 |
| 第 14 章 文档交换 | 3 | 6 | 2 | 0 | 1 | 12 |
| **总计** | **43** | **100** | **10** | **8** | **20** | **181** |

> 口径变更记录见 [changelog/2026-11-02.md](../changelog/2026-11-02.md)。


> 历史变更条目已移入 [changelog/](../changelog/README.md)。

## 章节索引

| 章 | 目录 |
|---|---|
| 第 7 章 语法 | [ch07/README.md](ch07/README.md) |
| 第 8 章 图形 | [ch08/README.md](ch08/README.md) |
| 第 9 章 文本 | [ch09/README.md](ch09/README.md) |
| 第 10 章 渲染 | [ch10/README.md](ch10/README.md) |
| 第 11 章 透明度 | [ch11/README.md](ch11/README.md) |
| 第 12 章 交互特性 | [ch12/README.md](ch12/README.md) |
| 第 13 章 多媒体 | [ch13/README.md](ch13/README.md) |
| 第 14 章 文档交换 | [ch14/README.md](ch14/README.md) |
| PDF 2.0 与标准版本 | [standard-versions.md](standard-versions.md) |

## 维护说明


- 本矩阵的状态列**只随代码实测更新**：任何 ✅/🟡 条目的证据列必须能在 `src/core` 或 `tests` 中找到对应实现/测试；新能力落地时同步更新状态、证据与「总览」统计（自检方法：统计四态标记出现次数与总览表一致）。
- 章节号与表号以本地全文 `docs/PDF32000_2008.pdf` 为唯一权威（用 `pdftotext -raw` 抽取后按条款号/`Table N` 检索；`-layout` 会令多列表格列错位，不可用于核对表号）。引用标准时先在抽取文本中核对编号再落笔；无法在抽取文本中定位的编号一律标「待核」或删除。
- 开发阶段的划分、条目排期与「完成」定义（代码+测试+文档三齐）见 `docs/roadmap/README.md`；每完成一个阶段，按 ROADMAP 的跟踪约定回写本矩阵对应行的状态与证据。
- 优先级（P0–P3）定义见图例，仅在规划评审时调整，调整需同步 ROADMAP 阶段依赖。
