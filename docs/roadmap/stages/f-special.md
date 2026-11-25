# 阶段 F：专门领域
> 规划层｜职责：本阶段的增量、依赖、验收标准与未闭项。｜禁止：测试名、统计数字、历史叙述（见 `changelog/`）。

**目标**：按需求逐步立项，不预设排期。

**增量**：

- [x] **F1 数字签名**（12.8）：ByteRange、签名字典、DocMDP/UR/FieldMDP、PKCS#1/PKCS#7；密码学经后端 trait 隔离；依赖 E7。三层 F1.1/F1.2/F1.3 均已三齐（2026-11-24）；人工验收项「生成的签名被外部工具判为有效」仍待真实签名样本/第三方工具现场确认，单独记录、不替代结构断言。矩阵行：[signatures.md](../../matrix/ch12/signatures.md)（12.8）。
  - 已落地基线（事实与证据以矩阵为准）：读端签名字典、ByteRange、SubFilter、变换引用与 `/Perms` 解析，CMS SignedData 结构解析，signedAttrs `messageDigest` 与覆盖字节摘要的绑定校验；写端增量签名对象安装、ByteRange 定点收敛、`/Contents` 等长回填。
  - 契约对账结论：**F1 做到 CMS 结构层为止**。库负责 `/ByteRange`、`/Contents`、`/SubFilter` 的一致性与 CMS SignedData 的解析、构造和编排；摘要与签名值计算、证书链与信任判定、时间戳验证一律经后端 trait 注入，库内不实现、也不做信任决策。
  - 契约改动与影响面：写端 `src/core/writer/signature.rs` 的「PKCS#7/CMS 语法不在本层」改为「CMS 结构在本层，密码学不在本层」；`SigningSpec` 的默认 `/SubFilter` 必须与 `/Contents` 载荷类型自洽（adbe.pkcs7.* 仅接受 CMS 载荷，当前裸摘要 + `adbe.pkcs7.detached` 的组合不成立）；新增 CMS 构造模块与签名/验证后端 trait；读端能力边界补充「签名值验证经后端注入」。不新增依赖、不改 MSRV。
  - 增量（严格依序，每层独立验收；三层全部满足才勾选 F1，完成证据回写矩阵 12.8 行）：
    - [x] **F1.1 解析与结构自洽层**（矩阵 12.8）：真实签名文件的 `/Contents` 可解析为 CMS SignedData，并暴露签署者标识、摘要算法、signedAttrs 与证书集；损坏或截断输入返回类型化错误且不 panic；写端拒绝 `/SubFilter` 与 `/Contents` 载荷类型不一致的组合（含当前默认的裸摘要 + `adbe.pkcs7.detached`）；`/ByteRange` 覆盖段不含 `/Contents`，边界与文件长度一致。
      - 退出标准：上述四项均有代码路径与矩阵证据；本层不做签名值验证与证书链判定，不依赖外部工具。（2026-11-24 四项均已落地：CMS 解析、损坏 DER 类型化错误、写端 `/SubFilter`↔`/Contents` 载荷自洽与 CMS DER 校验、ByteRange 边界校验；默认恒等签名器改为 Raw 载荷且不写 `/SubFilter`，消除「裸摘要 + `adbe.pkcs7.detached`」矛盾组合。证据回写矩阵 12.8 与 changelog。）
    - [x] **F1.2 摘要与签名值构造层**（矩阵 12.8）：写端可产出 detached CMS SignedData——digestAlgorithms、detached `data` encapContentInfo、signedAttrs 的 `messageDigest` 取自 ByteRange 覆盖字节、signerInfos 的签名算法/签署者标识/签名值；签名值计算经后端 trait（输入待签名 DER 字节与算法标识，输出签名值与证书集），库内不实现 RSA/ECDSA 原语；产物由本库读端解析后 `messageDigest` 与重算摘要一致，`/SubFilter` 与载荷一致。
      - 退出标准：写→读回环满足上述结构断言；第三方阅读器「签名有效」为人工验收项，单独记录，不替代结构断言。（2026-11-24 已落地：`src/core/writer/signature/cms.rs` 以最小 DER 构造器编排 detached SignedData；`CmsSignBackend` 只接收 signedAttrs DER + 摘要算法，返回签名值/签署者标识/证书集，库内无 RSA/ECDSA；`SigningSpec::with_cms_backend` 接通写端，`/Contents` 多余槽位补零、读端按 PDF 惯例忽略。写→读回环断言 digestAlgorithms、detached eContent、证书集、签署者标识、签名算法、`messageDigest` 与重算摘要一致，且后端签名字节 == 读端重建 signedAttrs。证据回写矩阵 12.8 与 changelog。）
    - [x] **F1.3 验证层**（矩阵 12.8）：提供验证后端 trait，覆盖签名值对 signedAttrs 的有效性、证书链可信性与时间戳令牌；库只负责调用编排与结果聚合，逐签署者状态必须区分匹配、不匹配、未提供，未注入后端不得报「通过」；时间戳仅识别 `ETSI.RFC3161` 与 `/Reference` 时间戳条目并原样暴露，验证委派后端。
      - 退出标准：篡改覆盖字节必报不匹配；合法样本在注入可信锚时报告可信、未注入时报告未提供；证书链构建、吊销查询与时间戳权威交互不在库内。（2026-11-24 已落地：`src/core/reader/signature/verify.rs` 的 `SignatureVerifyBackend` 分签名值/链/时间戳三项判定，`Document::verify_signature` 只做编排与聚合；`SignatureCheck`/`ChainCheck`/`TimestampCheck` 含 NotProvided，未注入后端 `all_matched()` 必为 false；`/Reference` 时间戳条目经 `TimestampReference`/`signature_timestamp_references`/`timestamp_reference_token` 原样暴露、验证委派后端。证据回写矩阵 12.8 与 changelog。）
  - 当前缺口（矩阵现状，F1 三层收口后）：无真实签名 fixture；人工验收项「外部工具判签名有效」待现场确认。F1.1 修复了默认 `/SubFilter` 与载荷不一致，F1.2 补齐写端 detached CMS 构造与签名值后端，F1.3 补齐验证后端编排；三项均不依赖真实签名文件，fixture 缺口独立登记。
- [ ] **F2 逻辑结构/Tagged PDF/无障碍**（14.7–14.9）：结构树、结构类型/属性、`/ActualText`/`/Alt`/`/Lang`、角色映射、无障碍检查。
- [ ] **F3 预印**（14.11、11.7.4）：页面边界、打印机标记、分色字典、输出意图、陷印、OPI；叠印控制已在 8.6.7 完成，输出意图（14.11.5 Table 365）、页面边界/框颜色信息（14.11.2 Table 30/360/361）、打印机标记（14.11.3 Table 362/363）、分色字典（14.11.4 Table 364）、陷印网络（14.11.6 Table 366/367）与 OPI 代理字典（14.11.7 Table 368–370）均已读写，六个条目全部完成，仅剩第三方阅读器语法检查（人工验收）。
- [ ] **F4 多媒体**（13.2–13.6）：3D 与更完整的 Sound/Renditions（13.2 Renditions/media clip（含 MCS 片段）、13.3 Sound、13.4 Movie 基础对象/动作/注解已读写，13.6 3D 语法层含 `/Resources` 名称树、Table 307–312 视图负载字典与 Table 313 `/ExData` 3D 标记已读写，剩 13.6.5 与音频解码）；Alternate Presentations 已读写（13.5）；均为 P3，按使用场景立项。
- [ ] **F5 其他交换能力**（14.10、7.12、附录 F）：Web Capture、Extensions 字典、线性化输出与 hint 流、PDF 2.0 差异评估。

**依赖**：F1 依赖 E7；F2 依赖 C4（标记内容读取模型）；F3 依赖 A 与 D0（页面级图形对象写端）；F4 依赖 E2/E3（注解/动作）；F5 依赖 A，线性化输出可选复用 E7。各项彼此独立，可任意顺序立项。

**验收标准**（逐项直接写入docs/roadmap/README.md，不引用外部 spec）：

- **F1**：ByteRange 覆盖签名字典之外的全部字节且边界正确；生成的签名可被外部工具验证（人工验收）；DocMDP/FieldMDP 变换限制可读；增量更新文件能定位签名字典。
- **F2**：结构树/结构元素读写往返；`/ActualText`/`/Alt`/`/Lang` 可提取；角色映射与结构类型可解析；非法结构条目返回类型化错误。
- **F3**：页面边界、打印机标记、分色字典、输出意图、陷印、OPI 字典可写→解析往返；第三方阅读器语法检查无错误（人工验收）。输出意图（14.11.5 Table 365）、页面边界/框颜色信息（14.11.2 Table 30/360/361）、打印机标记（14.11.3 Table 362/363）、分色字典（14.11.4 Table 364）、陷印（14.11.6 Table 366/367）与 OPI（14.11.7 Table 368–370）均已完成读写往返；仅剩第三方阅读器语法检查（人工验收）。
- **F4**：Renditions/Sound/Movie/3D 字典与注释可读写往返（只做语法层，不播放、不渲染）；Renditions（含播放/屏幕参数、media players 与 MCS 片段）/Sound/Movie 已完成语法层读写，3D 已完成语法层读写（GoTo-3D-View、Table 303 引用字典、`/OnInstantiate`、视图 `/O` 叠加外观、`/AN` 动画样式、视图 PDF 1.7 条目门禁与视图负载字典（Table 307–312）、`/ExData` 3D 标记（Table 313, 13.6.6）与流 `/Resources` 名称树（Table 300）已类型化读写；剩 13.6.5 坐标系语义）。
- **F5**：Web Capture 与 Extensions 字典读写往返；线性化输出可被本库读端按普通路径读取，hint 流结构经语法校验。Extensions 字典（7.12，Table 50）已完成读写往返（读端类型化前缀字典 + 写端直接对象目录条目与 1.7 门禁）；剩 Web Capture（14.10）、线性化输出与 hint 流、PDF 2.0 差异评估。
- 每项完成必须同步矩阵状态与证据，不得以「部分可用」标 ✅。
