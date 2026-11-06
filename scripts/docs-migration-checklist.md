# 文档拆分迁移清单（S0–S7）

> 一次性迁移记录。执行结果见下方各步「验证」。脚本 `scripts/docs_check.ps1` 保留为长期门禁。

## 前置条件

- `docs/FEATURE_MATRIX.md`、`docs/ROADMAP.md` 停止改写（主线本轮已收尾）。
- 工作区可写，且改动未提交（迁移与主线改动叠加，等统一提交）。
- PowerShell 5.1+；脚本以 UTF-8 读写。

## 步骤

| 步 | 动作 | 前置 | 验证 |
|---|---|---|---|
| S0 | 冻结基线：记录两份旧文档行数/体积、矩阵 181 行四态计数、阶段复选框数、`docs_check.ps1` 六项结果 | 旧文档静止 | 见「基线」 |
| S1 | 建目录树与入口：`docs/README.md`、`docs/matrix/`、`docs/roadmap/{stages,backlog}`、`docs/changelog/`、`docs/archive/2026-11-06/` | S0 完成 | `docs/README.md` 三条读者路径可点开 |
| S2 | 矩阵事实层：按章节/子域拆 `docs/matrix/chNN/*.md`；总览留在 `matrix/README.md` | S1 完成 | `docs_check.ps1` C1；逐章计数与总览一致；单文件 < 600 行 / 40 KiB |
| S3 | 规划层：拆 `roadmap/{README,principles,stages/*,non-goals,next-increments,graph}.md` 与 `backlog/*` | S2 完成 | C3（阶段复选框 vs 总览）；阶段标题无状态/计数 |
| S4 | 历史层：两份旧文档的 dated 条目按日期进 `docs/changelog/YYYY-MM-DD.md`；无日期进 `undated.md` | S3 完成 | 条目守恒（矩阵 163、路线图 153）；阶段文件不再含 `- 进度（2026-…` |
| S5 | 引用改写与归档：`README.md`、`.github/copilot-instructions.md`、两处代码注释改新路径；旧文件移入 `docs/archive/2026-11-06/` | S4 完成 | C4（0 断链）；`rg "docs/FEATURE_MATRIX.md\|docs/ROADMAP.md"` 仅剩归档/历史/脚本 |
| S6 | 验收：跑 C1–C6，对比基线；`cargo build --workspace --all-targets` | S5 完成 | 见「结果」 |
| S7 | 清理一次性脚本；把残留问题登记 `docs/roadmap/backlog/open-issues.md` | S6 完成 | 本文件保留；`scripts/_migrate_*.ps1` 删除 |

## 验证命令

```powershell
powershell -ExecutionPolicy Bypass -File scripts/docs_check.ps1          # 逐项报告
powershell -ExecutionPolicy Bypass -File scripts/docs_check.ps1 -Strict   # 门禁（FAIL 退出 1）
powershell -ExecutionPolicy Bypass -File scripts/docs_check.ps1 -Json     # 机器对比
cargo build --workspace --all-targets
```

## 基线（迁移前，2026-11-06 旧文件）

| 项 | 数值 |
|---|---|
| `FEATURE_MATRIX.md` | 615 行 / 787.9 KiB |
| `ROADMAP.md` | 668 行 / 410.6 KiB |
| 矩阵四态 | 181 行 = ✅43 / 🟡99 / ⬜10 / ❌9 / —20 |
| 阶段复选框 | A 6/6、B 7/7、C 5/5、D 6/7、E 0/7、F 0/5 |
| C1 / C3 / C4 | PASS / PASS / PASS |
| C2 | FAIL：证据路径缺失 3 处 |
| C5 | FAIL：易漂移数字 370 处、阶段标题带状态 6 处 |
| C6 | FAIL：超限文件 2 个（两份旧文档） |

## 结果（迁移后）

| 检查 | 结果 | 说明 |
|---|---|---|
| C1 | PASS | 181 行，计数一致，合并规则 0 违规 |
| C2 | FAIL 3 | 遗留，已登记 OI-1（`tests/crypt_filter_references.rs`、`tests/mod.rs`、`tests/helpers.rs`） |
| C3 | PASS | 6 个阶段与总览一致 |
| C4 | PASS | 83 条相对链接，0 断链 |
| C5 | FAIL 2 | 仅剩 `next-increments.md` 两处体积重复，已登记 OI-2；原 370 处随历史层迁移消除 |
| C6 | PASS | 55 个文件全部 < 600 行 / 40 KiB |

C5 规则在迁移中收紧：只保留「测试/用例计数」「行数 N」「`N 行 / X KiB` 文件体量组合」；`N 字节`、`N 行`（条款文本行号）等误报剔除；`module-debt*.md`、`fixtures.md`、`principles.md` 白名单（规则与台账本来就允许写体量）。
