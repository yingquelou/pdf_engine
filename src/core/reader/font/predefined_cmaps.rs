//! 预定义 CJK CMap 名册（`Adobe-GB1`/`Adobe-CNS1`/`Adobe-Japan1`/`Adobe-Korea1`）。
//!
//! 名册分两类数据：
//!
//! 1. **vendored 真实数据**：`cmap_resources/` 下是 Adobe 官方
//!    [`adobe-type-tools/cmap-resources`](https://github.com/adobe-type-tools/cmap-resources)
//!    的原文（BSD-3-Clause，许可证见 `cmap_resources/ADOBE_CMAP_LICENSE.txt`，
//!    来源提交与逐文件 sha256 见 `cmap_resources/README.txt`）。当前覆盖四套
//!    ordering 的全部编码类变体：Adobe-GB1 26 个、Adobe-CNS1-7 44 个、
//!    Adobe-Japan1-7 67 个、Adobe-Korea1-2 23 个（B6.1/B6.2/B6.3、GBK2K 与
//!    B6.4/B6.5 的 47 个 Unicode 变体），共 160 个文件 6039206 字节；含带
//!    版本号的 identity CMap，code→CID 与 Adobe 发布的 ordering 一致；
//!    竖排 V 通过文件内自带的 `usecmap` 继承横排基座，未复制任何映射表；
//!    `notdefrange` 按规范解析（整段映射到同一个 CID）；`CNS-EUC-*` 的 4 字节
//!    EUC-TW 码、`GBK2K-*` 与 UniGB/UniCNS/UniJIS/UniKS 的 UTF16/UTF32/UTF8
//!    可变长码均由「声明最长码优先、失败逐级回退」的解码顺序处理，不同码长
//!    区间的数值前缀重叠不再视为错误。
//! 2. **紧凑近似**：只有 `Adobe-GB1` 等裸 ordering 名仍由运行时生成
//!    （ASCII 透传、双字节顺序分配 CID）。这些表只保证本库写出的文本可
//!    往返，**不匹配** Adobe 对第三方字体使用的真实 ordering。Unicode 变体
//!    已全部以 vendored 原文覆盖；调用仍未覆盖的名称一律返回 `Err`，绝不
//!    合成看似完整的映射。

use std::collections::BTreeMap;
use std::sync::OnceLock;

use super::cid_cmap::{CidCMap, CodeSpace};
use super::{FontError, FontResult};

mod sources;

use sources::VENDORED_SOURCES;

/// 一个可解析的预定义 CMap。
///
/// 内部保持 `CidCMap` 的 range 表示：真实 Adobe CMap 的 `cidrange` 条目
/// 可直接保留为区间，避免展开成数万条逐码映射。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PredefinedCMap {
    cmap: CidCMap,
}

impl PredefinedCMap {
    fn from_parts(
        codespace_ranges: Vec<(u32, u32, usize)>,
        cid_mappings: BTreeMap<u32, u16>,
        writing_mode: u8,
    ) -> Self {
        Self {
            cmap: CidCMap::from_predefined(&codespace_ranges, &cid_mappings, writing_mode),
        }
    }

    /// 判断 `code` 是否落在指定码长的 codespace 区间内。
    pub fn codespace_contains(&self, length: usize, code: u32) -> bool {
        self.cmap.codespace_contains(length, code)
    }

    /// 该 CMap 声明的源字符码空间（ToUnicode 预定义基座按此过滤）。
    pub(in crate::core::reader::font) fn code_spaces(&self) -> &[CodeSpace] {
        self.cmap.code_spaces()
    }

    /// 转为解码用的 `CidCMap`（`type0` 与嵌入 CMap 基座解析使用）。
    pub(in crate::core::reader::font) fn into_cid_cmap(self) -> CidCMap {
        self.cmap
    }
}

/// 按名称解析 vendored 的 Adobe CMap 原文（首次使用时解析并缓存）。
///
/// 内嵌数据损坏属于构建期缺陷（不是不可信输入），与
/// `standard14_metrics` 的内嵌数据先例一致：直接 panic，避免静默退化成
/// 「无映射」而让调用方误以为数据可用。
fn vendored_roster() -> &'static BTreeMap<&'static str, PredefinedCMap> {
    static ROSTER: OnceLock<BTreeMap<&'static str, PredefinedCMap>> = OnceLock::new();
    ROSTER.get_or_init(|| {
        let mut parsed: Vec<(&'static str, CidCMap)> = Vec::new();
        for &(name, data) in VENDORED_SOURCES {
            let resolve = |base: &str| -> FontResult<CidCMap> {
                parsed
                    .iter()
                    .find(|(parsed_name, _)| *parsed_name == base)
                    .map(|(_, cmap)| cmap.clone())
                    .ok_or_else(|| {
                        FontError::Invalid(format!("unknown vendored CMap base: {base}"))
                    })
            };
            let cmap = CidCMap::parse_with_base(data.as_bytes(), None, resolve)
                .unwrap_or_else(|error| panic!("embedded CMap {name} failed to parse: {error}"));
            parsed.push((name, cmap));
        }
        parsed
            .into_iter()
            .map(|(name, cmap)| (name, PredefinedCMap { cmap }))
            .collect()
    })
}

/// 按名称解析预定义 CMap。
///
/// 裸 ordering 名返回紧凑近似（保持既有写端行为不变），其余名称查 vendored
/// 的 Adobe 原文数据（含 UniGB/UniCNS/UniJIS/UniKS 的 47 个 Unicode 编码
/// CMap）。未覆盖的名称返回 [`FontError::Invalid`]，不做任何猜测。
pub fn resolve_predefined_cmap(name: &str) -> FontResult<PredefinedCMap> {
    match name {
        "Adobe-GB1" => Ok(build_adobe_gb1()),
        "Adobe-CNS1" => Ok(build_adobe_cns1()),
        "Adobe-Japan1" => Ok(build_adobe_japan1()),
        "Adobe-Korea1" => Ok(build_adobe_korea1()),
        _ => vendored_roster()
            .get(name)
            .cloned()
            .ok_or_else(|| FontError::Invalid(format!("unknown predefined CMap: {name}"))),
    }
}

fn build_adobe_gb1() -> PredefinedCMap {
    let mut mappings = BTreeMap::new();

    for code in 0x00..=0x7F {
        mappings.insert(code, code as u16);
    }

    let mut cid = 0x0100;
    for section in 0xA1..=0xA9 {
        for pos in 0xA1..=0xFE {
            let code = ((section as u32) << 8) | (pos as u32);
            mappings.insert(code, cid);
            cid += 1;
        }
    }

    PredefinedCMap::from_parts(
        vec![(0x00, 0x7F, 1), (0xA1, 0xFE, 1), (0xA1A1, 0xFEFE, 2)],
        mappings,
        0,
    )
}

fn build_adobe_cns1() -> PredefinedCMap {
    let mut mappings = BTreeMap::new();

    for code in 0x00..=0x7F {
        mappings.insert(code, code as u16);
    }

    let mut cid = 0x0100;
    for high in 0xA1..=0xF9 {
        for low in 0x40..=0xFE {
            if low == 0x7F {
                continue;
            }
            let code = ((high as u32) << 8) | (low as u32);
            mappings.insert(code, cid);
            cid += 1;
        }
    }

    PredefinedCMap::from_parts(
        vec![(0x00, 0x7F, 1), (0xA1, 0xF9, 1), (0xA140, 0xF9FE, 2)],
        mappings,
        0,
    )
}

fn build_adobe_japan1() -> PredefinedCMap {
    let mut mappings = BTreeMap::new();

    for code in 0x00..=0x7F {
        mappings.insert(code, code as u16);
    }

    let mut cid = 0x0100;
    for high in 0x81..=0xFC {
        for low in 0x40..=0xFC {
            if low == 0x7F {
                continue;
            }
            let code = ((high as u32) << 8) | (low as u32);
            mappings.insert(code, cid);
            cid += 1;
        }
    }

    PredefinedCMap::from_parts(
        vec![(0x00, 0x7F, 1), (0x81, 0xFC, 1), (0x8140, 0xFCFC, 2)],
        mappings,
        0,
    )
}

fn build_adobe_korea1() -> PredefinedCMap {
    let mut mappings = BTreeMap::new();

    for code in 0x00..=0x7F {
        mappings.insert(code, code as u16);
    }

    let mut cid = 0x0100;
    for high in 0xA1..=0xFE {
        for low in 0xA1..=0xFE {
            let code = ((high as u32) << 8) | (low as u32);
            mappings.insert(code, cid);
            cid += 1;
        }
    }

    PredefinedCMap::from_parts(
        vec![(0x00, 0x7F, 1), (0xA1, 0xFE, 1), (0xA1A1, 0xFEFE, 2)],
        mappings,
        0,
    )
}

#[cfg(test)]
mod tests;
