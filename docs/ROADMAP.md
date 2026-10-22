# PDF 库开发路线图（ROADMAP）

> 目的：把 `docs/FEATURE_MATRIX.md` 中的 ⬜/🟡 条目按价值与依赖组织为有序开发阶段，并定义进度跟踪方式。
> 与矩阵的关系：矩阵是事实层（现状），本文档是规划层（排期）；条目一律用矩阵章节号引用。

## 进度跟踪约定

### 状态机

- 单条目的状态流转：`⬜ → 🚧（进行中，可选标注于备注）→ ✅`。
- 允许 `⬜ → ❌`：评审后决定不支持，须在矩阵备注中注明理由。
- 禁止 `✅ → ⬜` 回退；除非实现被移除，且须在矩阵备注中注明原因。
- 状态只在代码实测后修改，禁止凭印象改动。

### 「完成」定义（三齐）

一个条目标记 ✅ 必须同时满足：

1. **代码实现合入**：功能代码进入主分支；
2. **有对应测试**：测试名写入矩阵该行的证据列；
3. **文档同步**：矩阵状态/证据/总览统计同步更新，必要时更新 README 版本特性速查表。

三者缺一，该条目不得标 ✅（至多标 🚧 或 🟡）。

### 更新流程

每次开发迭代（对应一个 spec/PR）完成时，按顺序执行：

1. 回写矩阵对应行的状态与证据；
2. 重算矩阵「总览」统计，使四态计数与表内一致；
3. 勾选本文件对应阶段条目的复选框；
4. 当某阶段全部条目 ✅ 时，更新该阶段状态。

阶段状态同样四态：⬜ 未开始 / 🚧 进行中 / ✅ 完成 / ❌ 放弃。

### 工程约定（第三方库与扩展点）

1. **第三方库必须隔离在 PDF 语义 trait/适配器之后**：过滤器调度、解析器、构建器等业务代码禁止直接调用第三方库；需要在实现内部使用第三方能力时，先定义稳定的 trait，再由适配器实现（现有示例：`filters::InflateBackend` 封装 `flate2`）。
2. **过滤器统一经 `filters::StreamFilter` 扩展**：新增过滤器只实现 trait 并注册到 `builtin_filters`，不改 `decode_stream` 的调度逻辑；`decode_stream_with` 支持外部注入/覆盖实现。
3. **扩展点须可测试**：每个新 trait/适配器至少有自定义实现的测试，证明可用替代实现替换第三方依赖。
4. **解码器失败语义**：任何过滤器对损坏/截断输入返回 `DecodeError`，不得 panic；宽容恢复策略须在矩阵备注中写明。
5. **模块拆分约定**：单个 `.rs` 文件超过约 600 行或 25 KiB，或同时承担多个职责时，必须解构为目录模块（`foo/mod.rs` + 子模块）；编解码器实现、码表、测试应分文件；既有超限模块列入下方「工程债」逐项拆分，新增功能不得继续扩大超限文件。

## 工程债：模块目录化（已完成）

- [x] `filters`：`filters/{mod,ascii,flate,lzw,predictor,tests}.rs`（原 1200+ 行单文件）
- [x] `ccitt`：`ccitt/{mod,decoder,tables,tests}.rs`（原 1000+ 行单文件）
- [x] `reader`：`reader/{mod,lexer,object,parser,document,xref,tests}.rs`（原约 2076 行 / 81 KiB）
- [x] `writer`：`writer/{mod,value,version,serialize,context,document_writer,tests}.rs`（原约 1099 行 / 43 KiB）
- [x] `types`：`types/{mod,version,features,value,document,tests}.rs`（原约 910 行 / 32 KiB）
- [x] `content`：`content/{mod,types,serialize,stream,tests}.rs`（原约 1060 行 / 34 KiB）

各根模块当前均低于约 600 行；编解码器、码表、序列化与测试均已独立成文件。

## 阶段总览

| 阶段 | 目标 | 条目数 | 主要依赖 | 状态 |
|---|---|---|---|---|
| A 读得全 | 核心语法补全，真实世界 PDF 读入不失败 | 8 | 无 | ✅ |
| B 文本可用 | 字体与文本提取 | 12 | A（Functions 为软依赖） | 🚧 |
| C 图形完整 | 色彩/Pattern/Form/透明度规范完整 | 16 | A（Functions）、B（部分字体度量） | ⬜ |
| D 交互与交换 | 导航/注解/表单/元数据基础可用 | 9 | C | ⬜ |
| E 专门领域 | 需求驱动，不预设排期 | 15 | D | ⬜ |

## 阶段 A：读得全 —— 核心语法补全

**目标**：真实世界 PDF 读入不再因过滤器/加密/函数缺失而失败（对应矩阵 P0 缺口）。

**包含条目**：

- [x] `7.4.4` LZWDecode（已完成：9–12 位码宽、CLEAR/EOD、KwKwK、`/EarlyChange`、Predictor、截断宽容；测试见矩阵证据）
- [x] `7.4.6` CCITTFaxDecode
  - [x] 位流与解码器基础设施：MSB-first 位读取、白/黑游程表、2D 模式表、按行打包输出
  - [x] Group 3 1-D（`K=0`，Modified Huffman）与 EOL/填充位容忍
  - [x] Group 3 2-D（`K>0`，EOL 后 1-D/2-D 标志位）
  - [x] Group 4（`K<0`，T.6 纯二维，EOFB/RTC 终止）
  - [x] 参数：`/Columns` `/Rows` `/EndOfLine` `/EncodedByteAlign` `/EndOfBlock` `/BlackIs1` `/DamagedRowsBeforeError`
  - [x] 测试：手工码流（1-D/2-D/G4）+ libtiff 编码样本交叉验证 + 损坏/截断负向测试
- [x] `7.4.8` DCTDecode（JPEG，同上）
  - 已定义 PDF 语义的 `JpegBackend` trait，由 `ZuneJpegBackend` 适配器接入 `zune-jpeg 0.5`；Adobe APP14 CMYK 取反与 YCCK 逆变换在 PDF 语义层自实现；crates.io 镜像（`.cargo/config.toml` rsproxy sparse）已就绪，依赖可拉取。测试：4 色彩路径 + progressive + 损坏负向 + writer 过滤器回归 |
- [x] `7.4.10` Crypt 过滤器（已完成：密码认证后按对象号/generation 解密 `/Crypt`；Identity 与 R4 AESV2 具名过滤器覆盖端到端测试；writer 不支持加密输出）
- [x] `7.6.3` 标准安全处理器解密（已完成：ISO 32000-1 R2–R4，RC4-40/128、AESV2、用户/所有者密码、字符串/流与 `/EncryptMetadata false`；第三方密码原语封装在 `CryptoBackend` 适配器后。测试见矩阵证据。PDF 2.0 R5/R6、公钥安全处理器及加密写出不在本项支持范围）
- [x] `7.10` Functions（Type 0/2/3/4；已完成：`functions/` 目录模块，纯 Rust 语义层求值 API，Type 0 采样多线性插值 / Type 2 指数插值 / Type 3 拼接 / Type 4 PostScript 计算器子集；测试见矩阵证据。`/Order 3` 三次插值暂不支持；写端随阶段 C Shading 落地）
- [x] `附录 F` 线性化容忍验证（已完成：reader/tests.rs 手工构造线性化样本集——经典 xref 与 XRef 流 + ObjStm 两个变体、/H 两种合法形态、损坏 hint 流负向用例；线性化参数字典 /L /H /E /T 按实际布局定点迭代取真值。读端不利用 hint 流、不解读线性化字典，全部样本按普通 PDF 路径解析无错误、无告警回退，全部对象可解析、流可解码；测试见矩阵证据。写端线性化输出列入阶段 E）
- [x] `7.2` 词法缺口：Lexer 空白集补 0x00（已完成：`Lexer::is_whitespace` 含 NUL，与 filters 对齐；测试 `nul_byte_treated_as_whitespace`）

**依赖**：无（起始阶段）。

**验收标准**：

- 构造/收集含 LZW、CCITT、DCT、Crypt 过滤器的 PDF 样本，可完整解析且解码字节正确；
- 线性化 PDF 样本集按普通路径读取无错误；
- `7.6.3` 已按 ISO 32000-1 实现 R2–R4；PDF 2.0 R5/R6 与公钥处理器作为未实现范围明确记录；
- writer 对加密文档明确返回错误，不输出解密内容并保留原 `/Encrypt` 字典的无效文档；
- `cargo test` 全绿，新增过滤器均有正/负向测试。

## 阶段 B：文本可用 —— 字体与文本

**目标**：能正确读写含 TrueType/CID 字体的文档并提取文本（对应矩阵 P1）。

**包含条目**：

- [ ] `9.6.2` Type 1 简单字体补全（进行中：读端已能解析直接/间接 `/Widths` 与 `/FontDescriptor` 指标、简单字体编码，并为 Courier 四种样式提供 600 单位内建宽度；其余标准 14 宽度表、字体嵌入与写端嵌入路径仍待实现）
- [ ] `9.6.3` TrueType 字体
- [ ] `9.6.4` 字体子集化（随字体嵌入一并实现）
- [ ] `9.6.6` 字符编码补全（进行中：Type 1 读端识别 Standard/WinAnsi/MacRoman/Symbol/ZapfDingbats 基础编码并解析 `/Differences`；完整编码字形名表与 Unicode 映射仍待实现）
- [ ] `9.7.6` Type 0 复合字体 / CIDFontType0（解决 CJK 文本表达）
- [ ] `9.7.4` CIDFontType2（TrueType 轮廓 CID 字体）
- [ ] `9.7.5` CMap（预定义/嵌入，含 CIDSystemInfo）
- [ ] `9.10.3` ToUnicode CMap
- [ ] `9.8` 字体描述符（FontDescriptor、Flags、CIDFont 描述符）
- [ ] `9.9` 嵌入字体程序（FontFile/FontFile2/FontFile3）
- [ ] `9.10` 文本内容提取（字符码→Unicode 映射）
- [ ] `7.9` 公共数据结构（进行中：`PdfDate`、`PdfRect` 已实现；`PdfTextString` 支持 PDFDocEncoding 与带 BOM 的 UTF-16BE 编解码；名称树、数值树待实现）

**依赖**：A（过滤器解码能力是字体程序/嵌入流读取的硬依赖；`7.10` Functions 对文本非硬依赖，为软依赖，可并行）。

**执行顺序与并行边界**：

1. **简单字体基础（进行中）**：完成 Type 1 字体字典、宽度读取和 Encoding 字典结构解析；WinAnsi CP1252 与 MacRoman 的 Unicode 码值映射已实现，StandardEncoding 已覆盖 ASCII 和已定义高位字形槽位，可查询 glyph name 和已知 glyph name 的 Unicode。下一步补 Symbol/ZapfDingbats 映射及更完整的 Adobe glyph name 表；各表与查询逻辑分离，便于后续增加编码。
2. **标准字体度量**：依赖 glyph name 映射，逐族补齐 Standard 14 内建宽度表；Courier 固定 600 单位已覆盖，新增数据需有规范样例或独立表测试，不得用近似值填充。
3. **字体程序与 TrueType**：完善 FontDescriptor、FontFile/FontFile2/FontFile3 流读取后，再实现 TrueType 字体解析及子集嵌入；第三方字体解析依赖如有需要，必须通过 PDF 语义适配层隔离。
4. **CID 与复合字体**：在字体程序基础设施后实现 CIDFontType2/Type 0、CMap 与 CIDSystemInfo，先读端再写端。
5. **Unicode 与文本提取**：实现 ToUnicode CMap，再串联简单字体编码、CID CMap 与内容操作符，完成字符码到 Unicode 的提取；写入/提取往返作为阶段验收。
6. **独立工作并行**：`7.9` 的 PDFDocEncoding 文本串、名称树/数值树等公共类型不阻塞字体解析，可异步实现并单独更新矩阵证据。

**范围边界**：

- 每次只将有数据表、负向用例及公开 API 验证的子项标为已实现；识别 Encoding 名称不等于完成码值映射，码值映射也不等于完成 Unicode 文本提取。
- 标准 14 字体表不得以 `/MissingWidth` 代替逐字符度量；显式 `/Widths` 优先，其次适用的字体描述符默认宽度，再使用准确的内建字宽。
- 依赖关系按上面顺序推进；独立的 `7.9` 公共类型可并行，但不提前勾选阶段 B 整体完成。

**验收标准**：

- 写出含嵌入 TrueType 与 CIDFontType2 的文档，第三方阅读器打开显示正确；
- 含 CJK 文本的样本可正确提取为 Unicode 文本；
- ToUnicode 往返：写入文档的文本可被提取还原；
- 各字体类型均有解析/生成测试，`cargo test` 全绿。

## 阶段 C：图形完整 —— 色彩/Pattern/Form/透明度

**目标**：色彩/图案/Form XObject/透明度达到规范语法层完整（对应矩阵 P1–P2）。

**包含条目**：

- [ ] `8.4.4 / 8.4.5` `gs`（ExtGState 应用）、`i`（flatness）与图形状态参数字典
- [ ] `8.6.4` Device 颜色空间补全（DeviceCMYK 资源字典构造）
- [ ] `8.6.5` CIE-Based 颜色空间（CalGray/CalRGB/Lab/ICCBased）
- [ ] `8.6.6` Special 颜色空间（Pattern/Indexed/Separation/DeviceN）
- [ ] `8.6.8` 颜色操作符补全（灰度快捷 `G g`）
- [ ] `8.7` Patterns（Tiling/Shading，Shading Type 1–7；依赖 `7.10` Functions）
- [ ] `8.10.1 / 8.10.2` Form XObject（Form 字典生成与 `Do` 联动）
- [ ] `8.10.3` Group XObjects（透明组前置）
- [ ] `8.9` 图像 XObject 补全（SMask/Decode/Interpolate/压缩写入）
- [ ] `8.9.6` 图像蒙版（Stencil/Explicit/Colour Key）
- [ ] `11.3.5` 混合模式（BM）规范符合性修正：改经 ExtGState 字典 + `gs`（替代当前内联写法）
- [ ] `11.6.4` 恒定透明度（ca/CA）规范符合性修正：同上
- [ ] `11.4 / 11.6.6` 透明组（Transparency Group XObject，含 isolated/knockout）
- [ ] `11.5 / 11.6.5` 软蒙版（SMask 字典与 Soft-Mask 图像生成）
- [ ] `14.6` 标记内容操作符（`MP DP BMC BDC EMC`，可选内容前置）
- [ ] `8.11` 可选内容完整（OCG 结构生成、`OptionalContent` 枚举接入 Op 并序列化）

**依赖**：A（`7.10` Functions 是 Shading 的硬依赖）、B（Pattern/Form 内文本需字体度量支持，为部分软依赖）。

**验收标准**：

- ca/CA/BM 全部经 ExtGState + `gs` 序列化，矩阵 11.x 行的「规范符合性缺口」备注消除；
- 含 ICCBased/Separation/Pattern 颜色的文档可生成并被第三方阅读器正确渲染；
- 含透明组与软蒙版的文档生成正确；
- OCG 生成 + BDC/EMC 操作符接入并有测试，`OptionalContent` 死代码标记消除；
- `cargo test` 全绿。

## 阶段 D：交互与交换 —— 导航/注解/表单/元数据

**目标**：导航/注解/表单/元数据基础可用（对应矩阵 P2）。

**包含条目**：

- [ ] `12.3` 文档级导航（显式/命名 Destinations、大纲 Outline）
- [ ] `12.5.1–12.5.5` 注解通用结构（Annotation 字典/标志/边框样式/外观流 AP）
- [ ] `12.5.6` 常用注解类型（引用子表对应行：Text、Link、FreeText、Highlight、Underline、Squiggly、StrikeOut、Stamp、Ink、Popup、FileAttachment、Widget）
- [ ] `12.6` 常用动作（GoTo/URI/Named，其余动作类型按需增补）
- [ ] `12.7.2–12.7.4` AcroForm 基础（AcroForm 字典/字段字典/Widget 注解联动）
- [ ] `14.3.2` 元数据流（XMP /Metadata 读写）
- [ ] `14.3.3` 文档信息字典（/Info 语义封装）
- [ ] `14.4` 文件标识符（/ID 生成、更新与校验；经典表写出路径补 /ID）
- [ ] `7.11` 文件规格与嵌入文件流（Filespec、/EmbeddedFiles；FileAttachment 注解前置）

**依赖**：C（注解外观流/表单外观依赖图形能力完整）。

**验收标准**：

- 生成含大纲与 Link 注解的文档，第三方阅读器可点击跳转；
- Text/Highlight 等常用注解生成+解析往返测试通过，`tests/fixtures/annotated.pdf` 接入测试；
- AcroForm 文本字段+Widget 写出后可被识别填写；
- XMP 与 /Info 读写一致，/ID 生成符合规范算法；
- `12.4`、`12.7.5`、`12.10`、`12.2` 等按需增补，不阻塞本阶段验收；
- `cargo test` 全绿。

## 阶段 E：专门领域（长期）

**目标**：按需求驱动，不预设排期；仅在明确使用场景出现时逐项立项（对应矩阵 P3）。

**包含条目**：

- [ ] `13.2` 多媒体（Renditions、Media Clip）
- [ ] `13.3` 声音（Sounds）
- [ ] `13.4` 视频（Movies）
- [ ] `13.5` Alternate Presentations
- [ ] `13.6` 3D Artwork
- [ ] `12.8` 数字签名（DocMDP/UR/FieldMDP、PKCS#7）
- [ ] `14.7` 逻辑结构
- [ ] `14.8` Tagged PDF
- [ ] `14.9` 无障碍支持
- [ ] `14.10` Web Capture
- [ ] `14.11` 预印支持（含 `8.6.7` Overprint、`11.7.4` Overprinting 与透明度联动）
- [ ] `7.12` Extensions 字典
- [ ] PDF 2.0 差异评估（引用矩阵「PDF 2.0 差异摘要」节，评估结果决定写端 2.0 策略是否调整）
- [ ] `7.5.6` 写端增量更新（写出 /Prev 链）
- [ ] `附录 F` 线性化输出（生成 hint 流）

**依赖**：D（多数专门领域建立在导航/注解/元数据基础之上）；各项彼此独立，可任意顺序立项。

**验收标准**：

- 逐项立项时单独定义验收标准并挂到对应 spec；
- `12.7.8` XFA 不建议投入（PDF 2.0 已弃用，矩阵该行有注），维持 ⬜ 或评审转 ❌。

## 阶段依赖图

```mermaid
flowchart LR
    A["阶段 A：读得全"] --> B["阶段 B：文本可用"]
    A -->|"7.10 Functions（硬）"| C["阶段 C：图形完整"]
    B -->|"字体度量（软）"| C
    C --> D["阶段 D：交互与交换"]
    D --> E["阶段 E：专门领域"]
```

依赖构成无环 DAG：A → B → C → D → E；A 对 C、B 对 C 的细化依赖如图所示，均不构成回边。
