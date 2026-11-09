# 文档目录（总入口）

本目录是 PDF 库的开发文档树。**唯一入口**：先读本页，再按目标分流。

## 三种读者，三条路径

| 我想知道 | 从这里进 | 读什么 |
|---|---|---|
| 某特性做了没有、证据在哪 | [matrix/README.md](matrix/README.md) | 图例 → 总览 → 对应章节目录 |
| 下一步做什么、怎样算完成 | [roadmap/README.md](roadmap/README.md) | 阶段总览 → 完成定义 → 当前阶段文件 |
| 某天改了什么、为什么 | [changelog/README.md](changelog/README.md) | 按日期索引 |

新读者顺序：本页 → `matrix/README.md`（看懂四态） → `roadmap/README.md`（看懂三齐与阶段） → `roadmap/principles.md` → 当前阶段的 `roadmap/stages/<stage>.md` → 需要细节再进章节文件。

## 目录结构

```
docs/
├── README.md                  # 本页：阅读顺序、事实源纪律、目录树
├── matrix/                    # 事实层：做了什么、证据在哪
│   ├── README.md              # 图例、四态合并规则、优先级、总览统计（唯一）
│   ├── standard-versions.md   # PDF 2.0 差异摘要与标准版本史
│   ├── ch07/ … ch14/          # 按 ISO 32000-1 章节分目录，章内按子域分文件
├── roadmap/                   # 规划层：接下来做什么、怎样算完成
│   ├── README.md              # 状态机、三齐口径、更新流程、阶段总览（派生）
│   ├── principles.md          # 规划原则 1–8、工程约定 1–8
│   ├── spec-consistency.md    # 规范—规划—实现一致性机制（S1–S8 判定规则）
│   ├── stages/a-… f-….md      # 阶段文件（只留目标/增量/退出标准/未闭项）
│   ├── non-goals.md
│   ├── next-increments.md
│   ├── graph.md               # 依赖图与阶段映射
│   └── backlog/               # 工程债、fixtures、未决策、积压问题
├── changelog/                 # 历史层：按日期一文件，append-only
├── third-party/               # 引用层：第三方归属索引（许可证、版权、接入位置）
└── archive/                   # 历史归档，按日期建子目录（当前为空）
```

## 单一事实源纪律

| 事实 | 唯一存放 | 其他位置怎么写 |
|---|---|---|
| 特性状态、Reader/Writer、证据 | 对应章节矩阵行 | 引用「文件 + §条款号」，不复制状态 |
| 四态总览计数 | `matrix/README.md` | 只写「见总览」，不写数字 |
| 阶段完成数 | `roadmap/README.md`（由复选框派生） | 阶段标题不带状态与计数 |
| 三齐口径、状态机、更新流程 | `roadmap/README.md` | 链接引用 |
| 工程约定编号 | `roadmap/principles.md` | 写「工程约定 #N」并链接 |
| 非目标理由 | `roadmap/non-goals.md` | 矩阵行只写「❌ 非目标（理由见 …）」 |
| fixture 元数据（生成脚本、清单、来源 URL、SHA-256、许可） | `roadmap/backlog/fixtures.md` | 矩阵证据列只写 fixture 名；任何 PDF 文件不入仓 |
| 测试数、行数、体积数字 | 只进 `changelog/<date>.md` 与 `backlog/fixtures.md`、`backlog/module-debt.md` | 当前状态不写计数 |
| 第三方许可证、版权、接入位置 | `third-party/README.md` | 写包名并链接本文件，不复制许可证全文 |
| 依赖分类、判定顺序、门槛阈值 | `roadmap/principles.md` 与 `scripts/deps.allow.toml` | 易变元数据只进 `deps.allow.toml` |
| 规范锚点判定、三方一致性检查项 | [roadmap/spec-consistency.md](roadmap/spec-consistency.md) | 只写检查项编号（S1–S8），不复制规则原文 |

引用写法：

1. 只用「文件相对路径 + §条款号」，**不使用 `#锚点`**（中文标题锚点会百分号编码，改名即断链）。
2. 引用不带数字、不带状态符号、不带测试名。
3. 禁止「见上 / 见下文」，必须写文件名。
4. 跨层单向：`matrix/` 只引用口径，不引用阶段结论；`roadmap/` 引用矩阵行。

## 规范数据源

- 权威副本：`docs/PDF32000_2008.pdf`。
- 核对方法：`pdftotext -raw` 抽取纯文本后按条款号 / 表号检索；多列表格**勿用 `-layout`**（会列错位）。
- 机器索引：`docs/spec-index/iso32000-1.txt` 与 `meta.json` 由 `scripts/spec_index.ps1 -Update` 从权威副本抽取，只供 `scripts/spec_check.ps1` 做锚点/键名溯源；引用规范仍以 PDF 的 §条款号 / 表号为准，不要把索引文本当作引用来源。
- 三方一致性：锚点判定、索引生成/缓存/失效与 S1–S8 检查项见 [roadmap/spec-consistency.md](roadmap/spec-consistency.md)；日常运行 `powershell -File scripts/spec_check.ps1`（离线、只读），门禁加 `-Strict`，重建比对加 `-RebuildVerify`，`-Json` 含 `nonSpec` 非规范条目清单，`-RunTests` 才执行注入自测。
- 历史：`docs/PDF32000_2008/`（HTML 导出目录）在本仓库不存在，不作为数据源。

## 归档

`docs/archive/` 是历史归档区，按日期建子目录（当前为空）。归档内容为迁移前旧文档快照（**不再维护、不得引用为事实源**）。`scripts/docs_check.ps1` 的检查范围排除该目录。

## 迁移记录

拆分迁移的日期条目见 [changelog/2026-11-06.md](changelog/2026-11-06.md)。
