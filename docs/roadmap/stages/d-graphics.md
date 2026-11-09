# 阶段 D：图形完整（读端模型 + 写端收尾）
> 规划层｜职责：本阶段的增量、依赖、验收标准与未闭项。｜禁止：测试名、统计数字、历史叙述（见 `changelog/`）。

**目标**：把已完成的图形写端能力补齐读端模型，并补齐写端剩余条目；颜色/图像/Shading/Pattern/透明度达到语法层完整。

**D0 写端基线（已完成，证据见矩阵第 8/11 章）**：

- [x] 8.4.4/8.4.5 ExtGState 读写（`/TR` `/BG` `/BG2` `/UCR` `/UCR2` 已接入；`/TR2` 透传、`/HT` 半色调对象模型见 D6，2026-10-26）；
- [x] 8.6.4–8.6.8 设备/CIE-Based/Indexed/Separation/DeviceN 颜色空间、灰度/CMYK/颜色操作符；
- [x] 8.7 Shading Type 1–7、着色图案与彩色/无色平铺图案；
- [x] 8.9/8.9.6 图像字典常用条目、图像蒙版、软蒙版图像；
- [x] 8.10.1–8.10.3 Form/Group XObject；11.3.5/11.6.4/11.4/11.6.5/11.6.6 透明度写端；
- [x] 14.6 标记内容操作符；8.11 可选内容（OCG/OCProperties/Properties/BDC）。
- 说明：8.8.2 PostScript XObject、8.10.4 Reference XObject 不在基线内。

**待推进增量**：

- [x] **D1 颜色空间读端模型**（8.6.4–8.6.8）：解析 `/ColorSpace` 资源（设备、CalGray/CalRGB/Lab/ICCBased、Indexed、Separation/DeviceN、Pattern），支持 `/DefaultGray`/`/DefaultRGB`/`/DefaultCMYK`；Indexed 还原到基色空间；Separation/DeviceN 经 tint 函数求值到备用空间；输出分量数组与空间描述。
  - 退出标准：设备/CIE/Indexed/Separation/DeviceN 样本的样本值解码正确；ICCBased 返回配置文件字节与 `/N`；非法空间返回类型化错误。
- [x] **D2 图像字典、图像蒙版与内联图像读端**（8.9.5/8.9.6/8.9.7）：建模 `/Width`/`/Height`/`/ColorSpace`/`/BitsPerComponent`/`/Decode`/`/Interpolate`/`/Intent`/`/ImageMask`/`/Mask`/`/SMask`/`/SMaskInData`/`/Alternates`；解码内联图像 `BI/ID/EI` 字典与数据；提供样本读取 API（不做合成/渲染）。
  - 退出标准：内联图像与图像 XObject 的字典/样本可读；Stencil/Explicit/Colour Key 蒙版结构可解析；`/Alternates` 列表可遍历；损坏/截断图像数据返回错误。
- [x] **D3 JBIG2/JPX 解码**（7.4.7/7.4.9）：启动前先做后端选型 spike（纯 Rust 实现 vs 第三方适配器，比较许可证、体积、维护状态与可测试性）并把结论写入增量启动说明；再定义 `Jbig2Backend`/`JpxBackend` trait 与可替换适配器；接入 `StreamFilter` 注册；补 1.4/1.5 版本门控与损坏输入测试。
  - 退出标准：选型结论已记录；规范/公开样本可解码为位图/分量数据；trait 有替代实现测试；损坏流不 panic；写端仍按透传/剥离策略处理。
  - 完成情况（2026-11-07）：选型定为 `hayro-jbig2` 0.3.0 与 `hayro-jpeg2000` 0.3.5，经 `ImageBackends` 适配器注册，`ImageModel::samples()` 已返回真实位图，版本门控与损坏/截断测试齐备；JPX 三项退化已在 `scripts/deps.allow.toml` 登记为已知限制。
  - 未闭项：无（写端仍按既定策略透传/剥离）。
- [x] **D4 写端图像压缩**（7.4.4/7.4.8 Writer 列）：定义 `DeflateBackend` 编码 trait，提供 FlateDecode 编码路径（可选 Predictor）；图像/内容流写入时按调用方选择压缩；DCT 保持透传。更新矩阵过滤器 Writer 列与 README 体积说明。
  - 失败语义（单一契约）：默认纯 Rust Flate 后端始终可用；调用方注入的后端失败时必须返回类型化 `WriteError` 且不写出任何字节，**不得静默回退明文**；未请求压缩时保持明文透传。
  - 退出标准：写入→读端解码的字节一致；压缩流体积小于原始流；后端失败路径有测试且不写字节；特性与版本门控正确。
- [x] **D5 Shading/Pattern 读端模型**（8.7）：解析 Shading 字典 Type 1–7、网格流（BitsPerCoordinate/Component/Flag、Decode）、PatternType 1/2、图案流与 `/Resources`；校验域/坐标/函数维数；输出几何与颜色函数描述。
  - 退出标准：Type 1–7 合成样本可读；网格流位打包与 `/Decode` 还原正确；着色/平铺图案资源可解析；非法条目返回错误。
- [x] **D6 ExtGState/透明度读端应用与写端剩余**（8.4.4/8.4.5、11.x）：`gs` 接入 C3 状态机已完成（LW/LC/LJ/ML/SA/D/RI/Font/CA/ca/BM/OP）；剩余写端 `/TR2`、`/HT`（半色调对象模型）、SMask `/TR`、`/SMaskInData`（8.9.5 Table 89）；读端 `/Group`（8.10.2 Table 95）、软蒙版图像、`/Matte`（Table 146）结构解析。
  - 退出标准：`gs` 应用后状态快照正确；`/TR2`/`/HT` 写出可被读端解析；`/Matte` 与 `/SMaskInData` 读写样本通过；透明度版本门控保持。
- [x] **D7 写端图像/表单补全**（8.9.5、8.10、11.5）：`/Intent`、`/Alternates` 图像、`/Matte`、Form XObject 自有 `/Resources`/`/OC`/`/Ref`（按价值取舍）、Reference XObject 评估；补齐相应版本门控与往返测试。
  - 退出标准：新增字典条目可写→解析→读回；非法参数不写字节；第三方阅读器语法检查无错误（人工验收）。

**后续增量排期（已排期，待开工；当前不计入复选框，开工时转为正式复选框并同步阶段总览）**：

- **D8 半色调类型 6/10/16 的字典/流结构解析**（矩阵 [8.4.4 / 8.4.5](../../matrix/ch08/graphics-state.md)）：把 `/HalftoneType` 分派从 `1..=5` 范围判断改为按类型分派——保留既有 1/2/3/4/5 分支，新增 ISO 32000-1 §10.5.5 Table 129 的 6/10/16 阈值数组流字典并类型化：类型 6 校验 `/Width` `/Height` 与 8 位样本流，类型 10 校验 `/Xsquare` `/Ysquare` 与 8 位样本流，类型 16 校验 `/Width` `/Height`、可选成对 `/Width2` `/Height2` 与 16 位大端样本流；同时校验正整数几何、第二矩形成对出现与流长度自洽。
  - 事实依据：`src/core/reader/graphics_state/halftone/mod.rs` 的 `1..=5` 范围把 Table 129 明列的 6/10/16 直接判为 `is outside 1..=5`；该分支不读源版本，是枚举未补全，不是版本门（Table 129 仅对类型 16 标注 PDF 1.3）。
  - 兼容边界：类型 1/2 的类型化与类型 3/4/5 的 `Unmodeled` 现状保持不变（Table 129 不定义 2/3/4，其既有处理不在本增量内重定义）；仅新增 6/10/16，避免行为回归。
  - 位置与依赖：排在当前在飞的 F1 切片收口之后；依赖 D6 的 `/HT` 解析入口，不依赖 E/F；与 C6 互不依赖，按 C6 → D8 串行落地。
  - 退出标准：6/10/16 三类合法样本均可读到类型化模型，几何或流长度不自洽时返回类型化错误；既有 1/2/3/4/5 行为不回归；矩阵 8.4.4 / 8.4.5 行证据同步；`cargo build --all-targets` 零警告、`cargo test` 全绿、`cargo clippy` 无新增告警。
  - 明确不做：阈值数组按设备坐标求值、半色调网屏生成与实际半色调绘制（渲染语义，见 [non-goals.md](../non-goals.md)）；本次也不补写端 6/10/16 的写出。

**依赖**：D0 写端基线已具备；逐增量依赖如下——D3/D4 **只依赖 A**，不依赖 C，可与 C 并行；D1 依赖 C2/C3；D2 依赖 C2（资源与内容 API）；D5 依赖 C2/C3；D6 依赖 C3；D7 依赖 D0，其中 Form 自有 `/Resources` 部分与 C2 的资源模型对齐。B 仅在 Pattern 内字体与文本场景为软依赖。

**验收标准**：

- 含 ICCBased/Separation/Pattern/图像/Shading/透明组的样本可读入为模型对象；
- 写端剩余条目补齐后，矩阵第 8/11 章 Writer 列无未解释的 🟡；
- 真实样本（含 JBIG2/JPX 图像）可解析，损坏样本不 panic；
- `cargo test` 全绿。
