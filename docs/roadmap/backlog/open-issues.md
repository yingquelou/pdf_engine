# 积压问题（Backlog）
> 规划层｜职责：待核条目、证据路径过期与计数不一致等已知问题。｜禁止：就地改写矩阵/阶段原文（先登记，后修复）。

> 以下问题由 `scripts/validate_matrix.py` 与 `scripts/verify_evidence_paths.py` 在 2026-11-02 评审中发现，推迟到专门增量处理，不阻塞当前开发主线。每轮增量若触及同类问题，优先从此 backlog 核掉。

### 待核条目（规范引用未锚定）

_本轮及历史增量中标记为「待核」的章节号 / Table 号 / 版本门禁，需在后续增量中查阅 `docs/PDF32000_2008.pdf` 核掉。_

（当前无待核条目）

### 证据路径过期（模块重构后路径变更）

> 由 `verify_evidence_paths.py` 扫描发现，共 170 条历史路径在矩阵中引用但实际文件已不存在（因模块目录化拆分）。这些是历史引用，不影响当前功能，但应在后续「文档清理」增量中批量更新。

典型过期路径：
- `reader/lexer.rs` → 实际位于 `reader/lexer/` 目录
- `reader/image/jbig2.rs` → 实际位于 `reader/image/jbig2/` 目录
- `builder/assemble.rs` → 实际位于 `builder/assemble/` 目录
- 其余 167 条见 `verify_evidence_paths.py` 完整输出

处理方式：后续专门增量中跑 `verify_evidence_paths.py` 获取完整列表，批量更新矩阵证据列。

### 矩阵总览计数不一致

> 由 `validate_matrix.py` 扫描发现，矩阵底部「总览」行的计数与实际行数不符。这是 2026-11-02 评审新增 13 行后未同步更新总览所致。

| 状态 | 总览写 | 实际数 | 差值 |
|---|---|---|---|
| ✅ | 40 | 42 | +2 |
| 🟡 | 93 | 98 | +5 |
| ⬜ | 14 | 13 | -1 |
| ❌ | 7 | 9 | +2 |
| — | 14 | 20 | +6 |

处理方式：后续增量中重算总览行，或在专门「文档清理」增量中修正。

### 迁移验收登记（2026-11-06）

> 本节由文档拆分迁移（S0–S7）追加。上文「证据路径过期」「矩阵总览计数不一致」两节是迁移前快照；现以 `scripts/docs_check.ps1` 的结果为准（C1 已 PASS，总览 181 行 = ✅43 / 🟡99 / ⬜10 / ❌9 / —20）。

| 编号 | 检查 | 现状 | 处置 |
|---|---|---|---|
| OI-1 | C2 证据路径 | 3 处不可定位：`tests/crypt_filter_references.rs`、`tests/mod.rs`、`tests/helpers.rs`（`matrix/ch07/security.md` 7.6.5 行） | 待核：确认是历史路径失效还是已被目录化替代；确认后只改证据列，不改状态 |
| OI-2 | C5 易漂移数字 | `next-increments.md` 第 1、2 条仍写行数/体积，与 `backlog/module-debt.md` 重复 | 待清理：改为「体量见 `backlog/module-debt.md`」，由文档清理增量统一处理 |
| OI-3 | 数据源自相矛盾 | 原路线图 2026-10-26 条目写「以 `docs/PDF32000_2008/` 重新核对」，而矩阵维护说明已更正该 HTML 目录在本仓库不存在 | 历史条目不改写；现状口径以 `matrix/README.md`「数据源与口径」为准 |
| OI-4 | 旧 backlog 计数 | 上文「矩阵总览计数不一致」表为迁移前快照（40/93/14/7/14），与现状（43/99/10/9/20）不符 | 保留为历史快照，不再引用为事实源；修复项由 OI-2 一并跟进 |

### 依赖边界越界登记（2026-11-06）

> 由 `scripts/deps_check.ps1` 的 D2 扫描发现。以下位置在能力型适配器之外直接使用第三方 crate，已记为基线债务，本轮只登记不改代码。整改时把调用改走既有适配器（`security/crypto.rs` 与 `signature/digest.rs`），并同步把对应条目从 `scripts/deps.allow.toml` 的 `known_violation` 移除。

| 编号 | 位置 | 越界内容 | 处置 |
|---|---|---|---|
| OI-5 | `src/core/reader/embedded.rs` | MD5 摘要直用，未走 `SignatureBackend`/`DigestSink` | 待整改：改走摘要适配器 |
| OI-6 | `src/core/builder/types/embedded_file.rs` | MD5 摘要直用，属 builder 侧业务代码 | 待整改：抽出统一的嵌入文件校验入口 |
| OI-7 | `src/core/reader/fdf/embedded/crypto.rs` | MD5 + RC4 重复实现 Algorithm 2，与 `security/crypto.rs` 并行 | 待整改：合并到标准安全处理器的密钥派生 |

### 依赖替换优先级（2026-11-06）

> 依据既有第三方库评估结论：叶子级编解码可由成熟 crate 承担，PDF 产品语义保持本地实现。本轮只登记顺序，不动过滤器代码。

1. 第一批：LZW 与 CCITT 均已完成（2026-11-07）。`src/core/filters/lzw.rs` 保留 PDF 语义与截断宽容，位流/字典解码改经 `lzw_backend.rs` 适配 `weezl` 0.1.12；`src/core/ccitt/mod.rs` 保留 `CcittParams` 与 PDF 语义层，位流/行解码改经 `ccitt/backend.rs` 适配 `hayro-ccitt` 0.3.0，`/BlackIs1` 反转、补白行、`/DamagedRowsBeforeError` 损坏行恢复与 `/EndOfBlock false` 三者最早终止均在适配层完成，行为与替换前一致（证据见矩阵 7.4.6 与 `docs/changelog/2026-11-07.md`）。二者原同为无产品语义的叶子编解码，替换收益最高、风险最低。
2. 第二批：JBIG2 与 JPX 已完成（2026-11-07）。`src/core/reader/image/backend/jbig2_hayro.rs` 适配 `hayro-jbig2` 0.3.0，`src/core/reader/image/backend/jpx_hayro.rs` 适配 `hayro-jpeg2000` 0.3.5；两者均经 `ImageBackends` 注册，`filters/bitmap.rs` 的 `samples()` 已返回真实位图（证据见矩阵 7.4.7 / 7.4.9 与 `docs/changelog/2026-11-07.md`）。JPX 的退化项已在 `scripts/deps.allow.toml` 以 `degraded = true` 登记：16 位以上源被上游缩放到 8 位、`/SMaskInData != 0` 返回 `Unsupported`、`colr` 与分量数不一致时按 RGB+alpha 解释；待产品决策是否补本地 16 位路径或软蒙版合成。
3. 每批替换前先跑 `scripts/deps_check.ps1` 建立基线，替换后确认 D2 无新增越界、D4 许可证相容、D7 登记已更新。
4. 子集化、CMap 码空间切分、版本特性降级、线性化与签名流程不进入替换清单，理由见 `docs/roadmap/principles.md` 工程约定 #1。


