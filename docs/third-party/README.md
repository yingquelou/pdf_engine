# 第三方归属索引
> 引用层｜职责：汇总本项目直接依赖、开发依赖与 vendored 数据的许可证、版权与接入位置，作为再分发时的归属索引。｜禁止：复制上游许可证全文、记录版本发布时间/下载量等易变事实（机器可读事实源见 `scripts/deps.allow.toml`）。

## 出站许可

本项目以 **MIT OR Apache-2.0** 双许可发布，全文见仓库根的 [LICENSE-MIT](../../LICENSE-MIT) 与 [LICENSE-APACHE](../../LICENSE-APACHE)。

选择依据：本项目是供他人链接的库（`crate-type = ["cdylib","lib"]`），下游可能静态链接进任意许可证的应用，因此出站许可必须为宽松型；全部直接依赖均为 MIT / Apache-2.0 / BSD-3-Clause / Zlib / IJG 组合，与双许可无冲突；Apache-2.0 提供明示专利授权，MIT 提供最简集成路径，故采用 Rust 生态通行的双许可。

## 直接依赖

版本为解析后的实际版本，许可证取自各自 manifest 的 SPDX 字段。

| 包 | 版本 | 许可证 | 用途 | 接入位置 | 版权 |
|---|---|---|---|---|---|
| `aes` | 0.8.4 | MIT OR Apache-2.0 | AES-128-CBC 加解密 | `src/core/reader/security/crypto.rs` | RustCrypto Developers |
| `flate2` | 1.1.10 | MIT OR Apache-2.0 | FlateDecode（zlib） | `src/core/filters/flate.rs`、`src/core/writer/compression.rs` | Alex Crichton、Josh Triplett |
| `md-5` | 0.10.6 | MIT OR Apache-2.0 | 密钥派生与嵌入文件校验摘要 | `src/core/reader/security/crypto.rs`、`src/core/reader/signature/digest.rs` | RustCrypto Developers |
| `sha1` | 0.10.7 | MIT OR Apache-2.0 | 签名 ByteRange 摘要 | `src/core/reader/signature/digest.rs` | RustCrypto Developers |
| `sha2` | 0.10.9 | MIT OR Apache-2.0 | AES-256 与 CMS 摘要 | `src/core/reader/signature/digest.rs` | RustCrypto Developers |
| `rc4` | 0.1.0 | MIT OR Apache-2.0 | RC4 流密码（PDF 1.x 安全处理器） | `src/core/reader/security/crypto.rs` | The Rust-Crypto Project Developers |
| `subtle` | 2.6.1 | BSD-3-Clause | 常量时间比较（卫生型） | `src/core/reader/security/standard/key.rs` | Isis Lovecruft、Henry de Valence |
| `zeroize` | 1.9.1 | Apache-2.0 OR MIT | 密钥与口令内存清零（卫生型） | `src/core/reader/security/`、`src/core/reader/parser.rs`、`src/core/reader/fdf/` | The RustCrypto Project Developers |
| `zune-core` | 0.5.3 | MIT OR Apache-2.0 OR Zlib | `ZCursor` / `ColorSpace` 基础类型 | `src/core/filters/jpeg.rs` | zune-image 项目贡献者 |
| `zune-jpeg` | 0.5.15 | MIT OR Apache-2.0 OR Zlib | DCTDecode JPEG 解码 | `src/core/filters/jpeg.rs` | caleb |
| `ttf-parser` | 0.25.1 | MIT OR Apache-2.0 | CFF / OpenType 轮廓解析 | `src/core/reader/font/cff/ttf_parser.rs`、`src/core/reader/font/cff/raw/mod.rs` | Caleb Maclennan、Laurenz Stampfl、Yevgenii Reizner |
| `weezl` | 0.1.12 | MIT OR Apache-2.0 | LZWDecode 位流与字典解码 | `src/core/filters/lzw_backend.rs` | The image-rs Developers |
| `hayro-ccitt` | 0.3.0 | Apache-2.0 OR MIT | CCITTFaxDecode Group 3/4 位流与行解码 | `src/core/ccitt/backend.rs` | Laurenz Stampfl |
| `hayro-jbig2` | 0.3.0 | Apache-2.0 OR MIT | JBIG2Decode 像素解码（页位图与嵌入组织码流） | `src/core/reader/image/backend/jbig2_hayro.rs` | Laurenz Stampfl |
| `hayro-jpeg2000` | 0.3.5 | Apache-2.0 OR MIT | JPXDecode 像素解码（JP2 框与裸码流） | `src/core/reader/image/backend/jpx_hayro.rs` | Laurenz Stampfl |

## 开发依赖

| 包 | 版本 | 许可证 | 用途 | 版权 |
|---|---|---|---|---|
| `jpeg-encoder` | 0.7.1 | (MIT OR Apache-2.0) AND IJG | 测试构造 JPEG 样本 | Volker Ströbel |

`IJG` 为 Independent JPEG Group 的独立许可，允许在注明来源的前提下使用、修改与再分发；仅出现在 dev-dependencies，不进入运行时产物。

## Vendored 数据

| 数据 | 许可证 | 版权 | 溯源与校验 |
|---|---|---|---|
| Adobe CMap 资源（`src/core/reader/font/cmap_resources/`） | BSD-3-Clause | Copyright 1990-2023 Adobe. All rights reserved. | 来源、上游提交、逐文件 SHA-256 见 [README.txt](../../src/core/reader/font/cmap_resources/README.txt)；许可证原文见同目录 `ADOBE_CMAP_LICENSE.txt` |

数据按上游字节原样保留，未作修改；因此 BSD-3-Clause 的「保留版权声明与免责声明」义务由上述两个文件满足。

## 再分发检查表

1. 随产物分发 [LICENSE-MIT](../../LICENSE-MIT) 与 [LICENSE-APACHE](../../LICENSE-APACHE)。
2. 保留本文件与 `src/core/reader/font/cmap_resources/ADOBE_CMAP_LICENSE.txt`，以满足 BSD-3-Clause 与 Adobe 条款。
3. 不修改 vendored CMap 原文；若必须修改，按 BSD-3-Clause 在文件中标注变更，并同步更新 SHA-256 清单。
4. 新增或升级依赖时，先更新 `scripts/deps.allow.toml`，再同步本表；两者不一致由 `scripts/deps_check.ps1` 的 D1 拦截。
