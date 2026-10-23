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

### 工程约定（依赖、扩展点与失败语义）

以下约定面向全库；条目中的现有模块只是当前实例，同类新增实现同样适用。

1. **第三方库必须隔离在 PDF 语义 trait/适配器之后**：
    - 代码实现方案优先级：rust标准库>成熟第三方库/本地实现
    - 需要在实现内部使用第三方能力时，禁止直接调用第三方库，需要先定义稳定的 trait，再由适配器实现（现有示例：`filters::InflateBackend` 封装 `flate2`）;
2. **扩展优先于修改核心调度**：新增能力应通过既有扩展点（trait、注册表、可注入后端）接入，核心调度逻辑保持稳定；不得为单个实现修改调度分支。现有实例：过滤器经 `filters::StreamFilter` + `builtin_filters` 注册，`decode_stream` 不改动，`decode_stream_with` 支持注入/覆盖；字体程序、函数、安全处理器等新增同类能力时，先定义或复用 trait 与注册入口。
3. **抽象须可替换、可验证**：每个扩展点（trait、适配器、注入式后端）都要有替代实现的测试，证明核心只依赖抽象而非具体实现；新增扩展点时同步交付该测试，替代实现可以是自定义模拟实现或不同后端。
4. **边界失败语义统一**：所有解析、解码与安全边界对损坏、截断或非法输入返回类型化错误（如 `DecodeError`、`FontError`、`ParseError`），不得 panic；任何宽容恢复或降级行为必须显式实现、有测试覆盖并记录在矩阵备注中，禁止隐式吞错。
5. **模块拆分约定**：单个 `.rs` 文件超过约 600 行或 25 KiB，或同时承担多个职责时，必须解构为目录模块（`foo/mod.rs` + 子模块）；编解码器实现、码表、测试应分文件；既有超限模块列入下方「工程债」逐项拆分，新增功能不得继续扩大超限文件。

## 工程债：模块目录化

已完成拆分：

- [x] `filters`：`filters/{mod,ascii,flate,lzw,predictor,tests}.rs`（原 1200+ 行单文件）
- [x] `ccitt`：`ccitt/{mod,decoder,tables,tests}.rs`（原 1000+ 行单文件）
- [x] `reader`：`reader/{mod,lexer,object,parser,document,xref,tests}.rs`（原约 2076 行 / 81 KiB）
- [x] `writer`：`writer/{mod,value,version,serialize,context,document_writer,tests}.rs`（原约 1099 行 / 43 KiB）
- [x] `types`：`types/{mod,version,features,value,document,tests}.rs`（原约 910 行 / 32 KiB）
- [x] `content`：`content/{mod,types,serialize,stream,tests}.rs`（原约 1060 行 / 34 KiB）
- [x] `builder`：`builder/{mod,types,assemble,tests}.rs`（原 842 行 / 31 KiB；公共类型、`Document` 组装与测试分文件）
- [x] `type1_program/charstring`：`charstring.rs` + `charstring/tests.rs`（527 行测试移出后实现文件 527 行 / 17 KiB；CharString 解释与测试分文件）
- [x] `reader/font`：`font/{mod,descriptor,encoding,simple,tests}.rs`（原 923 行 / 33.4 KiB；描述符指标、简单字体编码、Type 1 指标与测试分文件）
- [x] `reader/text`：`text/{mod,tokenizer,tests}.rs`（原 889 行 / 29.1 KiB；内容流分词器与测试移出，`mod.rs` 约 400 行）

`src/core` 根模块与上述条目均低于约 600 行 / 25 KiB。以下嵌套与集成测试模块仍超限，后续增量触及前必须先拆分或同步拆分，不得继续扩大：

- [ ] `reader/security/standard.rs`（998 行 / 35.2 KiB）
- [ ] `reader/font/type1_program/mod.rs`（960 行 / 34.1 KiB）
- [ ] `reader/font/cid_cmap.rs`（918 行 / 32.6 KiB）
- [ ] `reader/font/truetype/glyph.rs`（816 行 / 29.6 KiB）
- [ ] `reader/tests.rs`（689 行 / 26.1 KiB）
- [ ] `tests/main.rs`（125.6 KiB；集成测试）

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

- [ ] `9.6.2` Type 1 简单字体补全（进行中：读端已能解析直接/间接 `/Widths` 与 `/FontDescriptor` 指标，并为 Courier、Helvetica/Times 四种样式、Symbol、ZapfDingbats 提供对应内建编码宽度；`Document::type1_program` 可解析嵌入 `/FontFile` 程序的明文头、内建编码、eexec 段与解密后的 CharStrings/Subrs；`Type1FontProgram::glyph` 解释 CharString 数值编码与路径操作符（hsbw/sbw、moveto/lineto/curveto、closepath、callsubr/return、div），并把 seac 经 StandardEncoding 字符码递归展开为基字形与平移后的重音字形轮廓（嵌套上限 10），输出字宽、side bearing 与绝对坐标路径；flex 的 `callothersubr`/`pop` 序列按七点协议展开为两段三次曲线（进入/退出可跨越 subr 边界，`pop pop setcurrentpoint` 收尾，支持 rmoveto/hmoveto/vmoveto 点形式；提示替换 `callothersubr 3` 按 OtherSubrs 语义读取单个 hint code 并登记一个待 `pop` 结果，hint 本身不应用，参数个数、缺少 `pop`、flex 期间调用等错误明确报出，其他过程明确报错）；hinting/渲染、字体嵌入与子集化仍待实现）
- [ ] `9.6.3` TrueType 字体（进行中：读端解析 SFNT 目录、基础度量表、Unicode cmap format 4/12 和 loca/glyf 字形记录，可查询水平 advance、Unicode→glyph ID，并递归展开复合字形及其仿射变换/点对齐；简单字形轮廓端点、指令、标志与坐标解码已实现；字体与 CIDFontType2 的集成见 9.7.4；`Document::simple_font_encoding` 已覆盖 TrueType/MMType1/Type1 的 `/Encoding`（预定义名、`/BaseEncoding`、`/Differences`），`reader/text` 据此完成 TrueType 简单字体的 Unicode 文本提取，缺少 `/Encoding` 时按 `/BaseFont` 回退 Standard/Symbol/ZapfDingbats（不读取字体程序内建编码）；hinting、其他 cmap 格式、渲染与写入仍待实现）
- [ ] `9.6.4` 字体子集化（随字体嵌入一并实现）
- [x] `9.6.6` 字符编码补全（已完成：Type 1 读端识别 Standard/WinAnsi/MacRoman/Symbol/ZapfDingbats，支持 CP1252/MacRoman Unicode 映射、Standard/Symbol/ZapfDingbats glyph name 查询及完整 Adobe Glyph List 2.0→Unicode 映射；写端 `PdfBuilder` 选择预定义编码名、自建 `/BaseEncoding`+`/Differences` 或省略 `/Encoding`，`text` 按编码反向查表转为单字节码——可打印 ASCII 保持字面量、其余输出十六进制字符串，无法表示的字符替换为 `?`，未知编码名回退 UTF-8 直写；测试见矩阵证据）
- [ ] `9.7.6` Type 0 复合字体 / CIDFontType0（进行中：Type 0→CIDFontType2/CIDFontType0 均可读取；Identity-H/V 与嵌入 CMap（含 Identity-H/V 基座的 usecmap 继承）的字符码→CID 映射已实现；CIDFontType0 解析 CIDSystemInfo、W/DW 与 FontDescriptor，解码 FontFile3（CIDFontType0C/OpenType）程序并经 ToUnicode 接入文本提取，CFF 轮廓解析、其他预定义 CMap 和写入待实现）
- [ ] `9.7.4` CIDFontType2（进行中：读端解析 CIDSystemInfo、CIDToGIDMap Identity/流映射、W/DW 宽度，并可加载 CIDFont 的 FontFile2 TrueType 程序；已接入 Type 0 父字体与受支持的 CMap 编码；经 `DocumentWriter` 写入→重新解析→文本提取的往返见 `type0_text_roundtrips_through_writer_and_extractor`）
- [ ] `9.7.5` CMap（进行中：Identity-H/V 和嵌入 CID 编码 CMap 的 codespace/cidchar/cidrange 已实现；嵌入 CMap 的 usecmap 继承支持 Identity-H/V 基座，覆盖项优先、后定义 codespace 替换并裁剪继承映射、WMode 继承；其他预定义 CMap 待实现）
- [ ] `9.10.3` ToUnicode CMap（进行中：Type 0 字体支持嵌入 ToUnicode CMap 的 bfchar/bfrange Unicode 映射，含 UTF-16BE 补充平面字符；已接入内容流文本提取并纳入写端往返验收）
- [ ] `9.8` 字体描述符（进行中：读端 `FontDescriptorMetrics` 覆盖 FontName、FontFamily、FontStretch、FontWeight、Flags、FontBBox、ItalicAngle、Ascent、Descent、Leading、CapHeight、XHeight、StemV、StemH、AvgWidth、MaxWidth、MissingWidth、CharSet 与 CIDSet；Flags 提供九个标准位的类型化访问器，CIDSet 经过滤器解码后可按 CID 查询位；Type 1 与 CIDFontType2/CIDFontType0 均暴露描述符。测试见矩阵证据。写入端仍不生成 FontDescriptor）
- [ ] `9.9` 嵌入字体程序（FontFile/FontFile2/FontFile3）（进行中：Type 1 读端经 `Document::type1_program` 解析 `/FontFile` 的明文头、eexec 段、Private 字典与解密后的 CharStrings/Subrs，且 `Type1FontProgram::glyph` 可将单个 CharString 解释为字宽、side bearing 与绝对路径段（flex 的 `callothersubr`/`pop` 按七点协议展开为曲线段，提示替换 `callothersubr 3` 按 OtherSubrs 语义消费 hint code 并等待 `pop`；seac 按 StandardEncoding 递归展开为基/重音轮廓；FontFile3/CFF/OpenType 报错）；TrueType 与 CIDFontType2 读端解析 FontFile2 的 SFNT 目录、基础元数据和轮廓；其他字体格式和字体嵌入写入仍待实现）
- [ ] `9.10` 文本内容提取（进行中：`reader/text/{mod,tokenizer}` 实现内容流文本提取——操作数栈式分词与文本操作符解释（BT/ET/Tf/Tj/TJ/Td/TD/T*/Tm/'/"），Type 0 经 CMap+ToUnicode 解码，简单字体（Type 1/MMType1/TrueType）经 `/Encoding`+AGL 解码，无字体时回退 Latin-1；页面级 `extract_page_text` 支持页面树遍历、资源继承与多内容流合并；14 项单元测试覆盖。写端补充 `ContentStream::show_text_hex`（`<...> Tj`），集成测试 `type0_text_roundtrips_through_writer_and_extractor` 验证 Type 0 + CIDFontType2 + FontFile2 + ToUnicode 的写入→解析→提取往返。常用预定义 CMap 仍待推进）
- [ ] `7.9` 公共数据结构（已完成：`PdfDate`、`PdfRect` 已实现；`PdfTextString` 支持 PDFDocEncoding 与带 BOM 的 UTF-16BE 编解码；`NameTree` 和 `NumberTree` 支持从字典解析（含 `/Kids` 递归）、按键查找、迭代和序列化往返）

**依赖**：A（过滤器解码能力是字体程序/嵌入流读取的硬依赖；`7.10` Functions 对文本非硬依赖，为软依赖，可并行）。

**执行顺序与并行边界**：

1. **简单字体与度量（进行中）**：Type 1 字体字典、宽度读取、Encoding 字典、WinAnsi/MacRoman 映射、Standard/Symbol/ZapfDingbats glyph name 查询及 AGL 2.0 的 4,281 个 Unicode 序列映射已覆盖；写端编码配置与文本码转换（五种预定义编码 + 自定义 Differences，非 ASCII 输出十六进制字符串）已实现。Standard 14 内建字宽已按 AFM 数据整理为结构化 CSV，Courier 四种样式为固定 600 单位；Type 1 字体程序格式解析（明文头、内建编码、eexec、Private 字典、CharStrings/Subrs 解密）已完成，`Type1FontProgram::glyph` 已解释 CharString 操作符并产出字宽与绝对路径，seac 合成字形已按 StandardEncoding 展开（含递归嵌套），flex 的 `callothersubr`/`pop` 已按七点协议展开为曲线段（含 subr 内进出与 `pop pop setcurrentpoint` 收尾）；提示替换 `callothersubr 3` 已按 OtherSubrs 语义消费单个 hint code，并要求后续 `pop` 消费其结果（不应用 hint）；剩余 hinting/渲染、字体嵌入与子集化。
2. **TrueType 程序读取（进行中）**：TrueType 的 `/FontFile2` 解码后校验 SFNT 目录，并解析 `head`/`maxp` 的 unitsPerEm/glyph count、`hhea`/`hmtx` 水平度量、Unicode `cmap` format 4/12 以及 `loca`/`glyf` 字形记录。可按 glyph ID 查询 advance width（含 `numberOfHMetrics` 尾部复用规则）、Unicode 查询 glyph ID，并读取 glyph header/bounds 和原始记录；简单字形的轮廓端点、instructions、压缩 flags 及 x/y 坐标增量已解码，复合字形可递归展开，支持 2.14 缩放/旋转/斜切矩阵、XY 偏移与点对齐，并限制递归深度及展开点数；不执行 hinting，也不渲染。此二进制解析仅用 Rust 标准库，以便控制格式支持范围及错误语义，当前无需引入第三方解析依赖。
3. **CID/复合字体与文本提取（进行中）**：CIDFontType2 可解析 descendant 字典、`CIDSystemInfo`、`CIDToGIDMap`（Identity/流）、`W`/`DW`，并可从 FontDescriptor 加载 `/FontFile2`。Type 0 可连接一个 CIDFontType2 descendant；编码支持 `/Identity-H`、`/Identity-V` 及嵌入 CMap 的 codespace、`cidchar`、`cidrange` 段，并提供字符码→CID→GID/宽度查询。`/ToUnicode` 支持嵌入 CMap 的 `bfchar` 和 `bfrange` 数组/递增目标，按 UTF-16BE 转换为 Unicode 序列，并在 Type 0 decoded code 中附带可选映射结果。`reader/text/{mod,tokenizer}` 已实现内容流文本提取：操作数栈式分词解释 `BT`/`ET`/`Tf`/`Tj`/`TJ`/`Td`/`TD`/`T*`/`Tm`/`'`/`"` 操作符，Type 0 经 CMap+ToUnicode 解码，简单字体（Type 1/MMType1/TrueType）经 `/Encoding`+AGL 解码，无字体回退 Latin-1；`Document::extract_page_text` 支持页面树遍历、资源继承与多内容流合并（14 项单元测试覆盖）。写端补充 `ContentStream::show_text_hex`，集成测试 `type0_text_roundtrips_through_writer_and_extractor` 验证 Type 0/CIDFontType2 文档经 `DocumentWriter` 写入、重新解析后由 `extract_page_text` 还原原始 Unicode 文本。CMap 的 `usecmap` 继承已支持 Identity-H/V 基座；Type 0 现已同时接受 CIDFontType2 与 CIDFontType0 descendant（后者解析 W/DW 与 FontFile3 程序，文本提取经 ToUnicode 工作）。其他预定义 CMap、CIDFontType0 的 CFF 轮廓解析与专用写端字体 API 仍待实现。
4. **独立公共类型（已完成）**：`7.9` 的 `PdfDate`、`PdfRect`、`PdfTextString`、`NameTree` 与 `NumberTree` 均已实现；名称树和数值树支持 `/Kids` 递归解析、按键查找、迭代和序列化。

**范围边界**：

- 每次只将有数据表、负向用例及公开 API 验证的子项标为已实现；识别 Encoding 名称不等于完成码值映射，码值映射也不等于完成 Unicode 文本提取。
- 标准 14 字体表不得以 `/MissingWidth` 代替逐字符度量；显式 `/Widths` 优先，其次适用的字体描述符默认宽度，再使用准确的内建字宽。
- 依赖关系按上面顺序推进；独立的 `7.9` 名称树/数值树可并行，但不提前勾选阶段 B 整体完成。

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
