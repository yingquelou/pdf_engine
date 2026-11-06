# 真实样本与 fixtures backlog
> 规划层｜职责：真实样本来源、许可与用途登记。｜禁止：引用为当前测试计数来源（计数见 changelog）。

| fixture | 用途 | 来源/许可 | 体积 | 当前状态 |
|---|---|---|---|---|
| `tests/fixtures/minimal-document.pdf` | 基础解析回归 | 待追溯 | 17 KiB | ✅ A6 接入（`tests/fixtures.rs`） |
| `tests/fixtures/pdflatex-4-pages.pdf` | 页面树/内容流/文本提取 | 待追溯 | 25 KiB | ✅ A6 接入解析；C2 接入内容流；C3 接入状态机；C4 接入标记内容；C5 接入模型往返无损校验 |
| `tests/fixtures/annotated.pdf` | 注解/动作 | 待追溯 | 1.8 KiB | ✅ E2 接入（`annotated_fixture_reads_text_highlight_and_ink_annotations`）；动作联动待 E2 写端/E4 |
| `tests/fixtures/xmp-metadata.pdf` | XMP/Info | 待追溯 | 2.1 KiB | ✅ E5 接入（`tests/fixtures.rs`：解析 + `/Metadata`/`/Info`/`/ID` 提取） |
| 待补：JBIG2 扫描件 | D3 | 需许可安全的公开样本或自行生成 | — | 未获取 |
| 待补：JPX 图像 | D3 | 可使用公开测试图或生成工具（须登记许可） | — | 未获取 |
| 待补：Shading/Pattern/透明组 | D5/D6 | 优先用 builder 生成合成样本，再补真实样本 | — | D5/D6 合成样本已由 builder 往返集成测试覆盖；真实样本未获取 |
| 待补：AcroForm | E4 | 自行生成最小表单样本 | — | 未获取 |
| 待补：CJK CID/Type 0 | B6 | 使用开源字体生成最小样本 | — | 样本未获取；CMap ordering 数据已登记（见下列六行），`Adobe-Identity-UCS` 基座为算法恒等解析（无第三方数据文件），B6.5 后内置 47 个 UniGB/UniCNS/UniJIS/UniKS 预定义基座 |
| `src/core/reader/font/cmap_resources/`（`GB-EUC-H/V`、`GBpc-EUC-H/V`、`GB-H/V`） | B6.1 预定义 CMap 真实 ordering | `adobe-type-tools/cmap-resources`，分支 `master`，提交 `f5cf3bca`（2023-11-15），BSD-3-Clause（许可证原文 `ADOBE_CMAP_LICENSE.txt`，逐文件 sha256 见 `README.txt`） | 22492 B（约 22.0 KiB；6 个文件 4427/3100/4455/3112/4322/3076 B） | ✅ 2026-11-06 接入（`predefined_cmaps.rs` vendored 名册 + 真实值/往返测试）；GBK/GBT/UniGB 与 CNS1/Japan1/Korea1 见后续行 |
| `src/core/reader/font/cmap_resources/`（`GBK-EUC-H/V`、`GBKp-EUC-H/V`、`GBT-H/V`、`GBT-EUC-H/V`、`GBTpc-EUC-H/V`） | B6.2 预定义 CMap 真实 ordering（编码类大体积变体） | 同上（`adobe-type-tools/cmap-resources`，`master`，`f5cf3bca`，BSD-3-Clause；逐文件 sha256 见 `README.txt`） | 321925 B（目录总计 344417 B；单文件 83102/3089/83083/3095/46668/3084/46773/3108/46803/3120 B） | ✅ 2026-11-06 接入（vendored 名册扩到 16 个编码类变体，`notdefrange` 按规范解析 + 真实值/V 继承/负向往返测试）；未收录：`UniGB-*`（归 B6.4）；`GBK2K-*` 见 GBK2K 行；CNS1/Japan1/Korea1 见 B6.3 行 |
| `src/core/reader/font/cmap_resources/`（`Adobe-CNS1-7` 36 个、`Adobe-Japan1-7` 44 个、`Adobe-Korea1-2` 15 个非 Unicode CMap） | B6.3 CNS1/Japan1/Korea1 真实 ordering | 同上（`adobe-type-tools/cmap-resources`，`master`，`f5cf3bca`，BSD-3-Clause；逐文件 sha256 见 `README.txt`） | 727377 B（CNS1 265949 + Japan1 279527 + Korea1 181901；`cmap_resources/` 合计 111 个文件 1071794 B） | ✅ 2026-11-06 接入（含带版本号的 identity CMap；13 个 lib + 3 个 main 往返测试）；未收录：UniCNS/UniJIS/UniKS 39 个 Unicode 变体 3866824 B（占三套目录总计 4594201 B 的 84.2%）归 B6.4；`Adobe-KR-9`/`Adobe-Manga1-0`/`Adobe-Japan2-0` 不在 B6.3 范围 |
| `src/core/reader/font/cmap_resources/`（`GBK2K-H`、`GBK2K-V`） | GBK2K 真实 ordering 与跨长度 codespace 解码策略 | 同上（`adobe-type-tools/cmap-resources`，`master`，`f5cf3bca`，BSD-3-Clause，许可证原文 `ADOBE_CMAP_LICENSE.txt`） | 97967 B（H 94485 / V 3482 B；`cmap_resources/` 合计 113 个文件 1169761 B） | ✅ 2026-11-06 接入（4 字节 `<81308130>`–`<FE39FE39>` 与 2 字节 `<8140>`–`<FEFE>` 前缀重叠按「声明最长码优先」解码，同长度重叠仍拒绝；3 个 vendored lib + 2 个解码器 lib + 1 个写端 lib + 1 个 main 往返测试）；未收录：UniGB/UniCNS/UniJIS/UniKS 归 B6.4 |
| `src/core/reader/font/cmap_resources/`（`UniGB-UCS2-H/V`、`UniGB-UTF16-H/V`、`UniGB-UTF32-H/V`、`UniGB-UTF8-H/V`） | B6.4 UniGB 编码 CMap 与 ToUnicode 预定义基座 | 同上（`adobe-type-tools/cmap-resources`，`master`，`f5cf3bca`，BSD-3-Clause；来源镜像为 jsDelivr 同提交固定地址，逐文件 sha256 见 `README.txt`） | 1002621 B（274383/3167 + 203288/3062 + 271244/3178 + 241183/3116；`cmap_resources/` 合计 121 个文件 2172382 B，GB1 26 个 1445005 B） | ✅ 2026-11-06 接入（8 个名称统一走 vendored 名册、删除 `UniGB-UCS2-H` 运行时近似；lib 4 + lib 6 + lib 3 + main 1 个新增测试）；UniCNS/UniJIS/UniKS 39 个变体见下一行（B6.5） |
| `src/core/reader/font/cmap_resources/`（`Adobe-CNS1-7` 8 个、`Adobe-Japan1-7` 23 个、`Adobe-Korea1-2` 8 个 Unicode 编码 CMap） | B6.5 UniCNS/UniJIS/UniKS 39 个 Unicode 编码变体 | 同上（`adobe-type-tools/cmap-resources`，`master`，`f5cf3bca`，BSD-3-Clause；来源镜像为 jsDelivr 同提交固定地址，逐文件 sha256 见 `README.txt`） | 3866824 B（CNS1 1206282 + Japan1 2048428 + Korea1 612114；`cmap_resources/` 由 121 个文件 2172382 B 增至 160 个文件 6039206 B，目录另含 `README.txt`/`ADOBE_CMAP_LICENSE.txt` 共 162 个文件 6058047 B） | ✅ 2026-11-06 接入（47 个 ToUnicode 基座名称 + 14 个新增测试；UTF16/UTF32/UTF8 码长结构按原文实测确认，未生成近似表）；裸 ordering 名仍为紧凑近似 |
| 待补：增量更新/签名 | E7/F1 | 自行构造 v1 + update 样本 | — | 未获取 |

> 纪律：真实 PDF 必须有可追溯来源与许可；无法确认许可的样本不得入库。合成样本由 builder 或脚本生成，生成脚本随 fixture 一并入库。既有 4 个 fixture 来源待追溯，A6 已接入测试，后续决策是否替换为自生成样本。
