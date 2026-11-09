# 阶段 A：核心读入与安全
> 规划层｜职责：本阶段的增量、依赖、验收标准与未闭项。｜禁止：测试名、统计数字、历史叙述（见 `changelog/`）。

**目标**：常见 PDF 1.0–1.7 的语法、文件结构、核心过滤器、加密与函数可读；读端对损坏输入返回类型化错误而非 panic。

**已完成基线**（证据与测试名见 `docs/matrix/README.md` 第 7 章对应行）：

- [x] A1 词法与对象模型：7.2、7.3.2–7.3.10；嵌套上限、流长度与间接引用解析。
- [x] A2 文件结构与交叉引用：7.5.2–7.5.8（经典表/ XRef 流/ ObjStm/ `/Prev` 链/混合引用）、7.7.2 Catalog 版本覆盖。
- [x] A3 核心过滤器：7.4.1–7.4.6、7.4.8、7.4.4.4（Flate/LZW/CCITT/DCT/ASCIIHex/ASCII85/RunLength/Predictor），第三方后端经 trait 隔离。
- [x] A4 安全：7.6.2/7.6.3/7.4.10 的 Standard Security Handler R2–R4 解密（RC4-40/128、AESV2、`/EncryptMetadata false`、`/Crypt`），公钥与 R5/R6 明确不支持。
- [x] A5 Functions 与线性化容忍：7.10 Type 0/2/3/4 读端求值；附录 F 线性化文件按普通路径完整可读（不利用 hint 流）。

**待推进增量**：

- [x] **A6 真实样本回归与 fixture 纪律**（非规范：内部约定，见 docs/roadmap/principles.md；原则 5、样本来源与许可登记见 docs/roadmap/backlog/fixtures.md）：把 `tests/fixtures/minimal-document.pdf` 与 `tests/fixtures/pdflatex-4-pages.pdf` 接入自动化测试（文档解析、页面树遍历、文本提取冒烟），在 fixtures 表补齐来源/许可/体积；用至少一个真实 PDF 验证损坏输入不 panic。`tests/main/mod.rs` 已超限，接入前按「工程债」要求先把对应主题迁出为独立测试模块。
  - 退出标准：两个 fixture 在 `cargo test` 中自动运行（新增用例位于拆分后的主题模块）；二者的来源/许可/体积已登记；无法追溯许可的既有 fixture 由 A6 决定替换为自生成样本并记录决策；截断副本返回类型化错误而非 panic。

**完成口径**：仅指上述 A1–A6。本阶段验收**只看 Reader 列**：A 范围内条目 Reader ✅ 即达成基线，Writer 侧缺口不参与本阶段判定（例如过滤器 Writer 列为 🟡 是「透传/剥离，不主动编码」的已知策略）。以下**不属于**阶段 A，避免「A 完成 = 所有真实 PDF 可读」的误读：

- JBIG2/JPX 解码 → D3；
- 图像字典/内联图像/图像蒙版读端 → D2；
- 通用内容流 Op 模型 → C；
- 增量更新写出 → E7；
- Name Dictionary（7.7.4 `/Names`）→ E1；Filespec/`/EmbeddedFiles` → E6；Extensions 字典 → F5；
- 非 ISO 32000-1 的 `<~ ~>` ASCII85 字符串扩展（矩阵 7.3.4 扩展行）不进入阶段 A；
- 权限位解释（矩阵 7.6.3 权限行）已于 2026-11-01 作为 E1 切片落地（`Document::permissions()`），不再待决策；
- 加密写出、公钥安全处理器、PDF 2.0 R5/R6 → 非目标（见文末）。

**验收标准**：

- 含上述过滤器的合成样本与真实样本可解析，解码字节正确；损坏/截断样本返回类型化错误；
- 线性化样本按普通路径读取无错误；
- 真实样本 fixture 已接入自动化测试，来源/许可/体积已登记；
- `cargo test` 全绿；除上文已排除的条目（7.3.4 扩展、7.4.7/7.4.9、7.7.4、7.11；7.12 与 7.6.3 权限行已于 2026-11-01 落地）外，A1–A5 所列章节的矩阵条目 Reader 均为 ✅；上述排除条目的归属见「不属于阶段 A」与「未分配条目」；A6 的 fixture 要求全部满足。
