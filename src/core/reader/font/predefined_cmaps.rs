//! Predefined CJK CMap data for Adobe-GB1, Adobe-CNS1, Adobe-Japan1, Adobe-Korea1.
//!
//! These CMaps map character codes to CIDs for common CJK character sets.
//! Data is compactly represented as code ranges with CID offsets.

use std::collections::BTreeMap;

use super::{FontError, FontResult};

/// A predefined CMap with codespace ranges and CID mappings.
#[derive(Clone)]
pub struct PredefinedCMap {
    pub codespace_ranges: Vec<(u32, u32, usize)>,
    pub cid_mappings: BTreeMap<u32, u16>,
    pub writing_mode: u8,
}

/// Resolve a predefined CMap by name.
pub fn resolve_predefined_cmap(name: &str) -> FontResult<PredefinedCMap> {
    match name {
        "Adobe-GB1" => Ok(build_adobe_gb1()),
        "Adobe-CNS1" => Ok(build_adobe_cns1()),
        "Adobe-Japan1" => Ok(build_adobe_japan1()),
        "Adobe-Korea1" => Ok(build_adobe_korea1()),
        "UniGB-UCS2-H" => Ok(build_uni_gb_ucs2()),
        _ => Err(FontError::Invalid(format!(
            "unknown predefined CMap: {}",
            name
        ))),
    }
}

/// UniGB-UCS2-H：字符码即 UCS-2 码位（2 字节大端），映射为 CID。
///
/// codespace 为整段 `<0000> <FFFF>`，覆盖全部 BMP；写端按名称引用该
/// CMap 时可直接把 Unicode 文本按码位写成 2 字节字符码。
/// 注意：`from_predefined` 按起始码推断映射范围的字节长度（≤ 0xFF 记为
/// 1 字节），因此恒等映射从 0x0100 起登记，保证整段落入 2 字节
/// codespace；0x0000–0x00FF 的低位码解码为 CID 0（文本提取由
/// ToUnicode 驱动，不受影响）。
fn build_uni_gb_ucs2() -> PredefinedCMap {
    let mut mappings = BTreeMap::new();
    for code in 0x0100..=u32::from(u16::MAX) {
        mappings.insert(code, code as u16);
    }
    PredefinedCMap {
        codespace_ranges: vec![(0x0000, 0xFFFF, 2)],
        cid_mappings: mappings,
        writing_mode: 0,
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

    PredefinedCMap {
        codespace_ranges: vec![(0x00, 0x7F, 1), (0xA1, 0xFE, 1), (0xA1A1, 0xFEFE, 2)],
        cid_mappings: mappings,
        writing_mode: 0,
    }
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

    PredefinedCMap {
        codespace_ranges: vec![(0x00, 0x7F, 1), (0xA1, 0xF9, 1), (0xA140, 0xF9FE, 2)],
        cid_mappings: mappings,
        writing_mode: 0,
    }
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

    PredefinedCMap {
        codespace_ranges: vec![(0x00, 0x7F, 1), (0x81, 0xFC, 1), (0x8140, 0xFCFC, 2)],
        cid_mappings: mappings,
        writing_mode: 0,
    }
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

    PredefinedCMap {
        codespace_ranges: vec![(0x00, 0x7F, 1), (0xA1, 0xFE, 1), (0xA1A1, 0xFEFE, 2)],
        cid_mappings: mappings,
        writing_mode: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_adobe_gb1() {
        let cmap = resolve_predefined_cmap("Adobe-GB1").unwrap();
        assert_eq!(cmap.cid_mappings.get(&0x41), Some(&0x41));
        assert_eq!(cmap.cid_mappings.get(&0xA1A1), Some(&0x0100));
    }

    #[test]
    fn resolves_adobe_cns1() {
        let cmap = resolve_predefined_cmap("Adobe-CNS1").unwrap();
        assert_eq!(cmap.cid_mappings.get(&0x41), Some(&0x41));
    }

    #[test]
    fn resolves_adobe_japan1() {
        let cmap = resolve_predefined_cmap("Adobe-Japan1").unwrap();
        assert_eq!(cmap.cid_mappings.get(&0x41), Some(&0x41));
    }

    #[test]
    fn resolves_adobe_korea1() {
        let cmap = resolve_predefined_cmap("Adobe-Korea1").unwrap();
        assert_eq!(cmap.cid_mappings.get(&0x41), Some(&0x41));
    }

    #[test]
    fn rejects_unknown_cmap() {
        assert!(resolve_predefined_cmap("Unknown-CMap").is_err());
    }

    #[test]
    fn resolves_uni_gb_ucs2() {
        let cmap = resolve_predefined_cmap("UniGB-UCS2-H").unwrap();
        assert_eq!(cmap.writing_mode, 0);
        assert_eq!(cmap.codespace_ranges, vec![(0x0000, 0xFFFF, 2)]);
        assert_eq!(cmap.cid_mappings.get(&0x4E2D), Some(&0x4E2D));
        assert_eq!(cmap.cid_mappings.get(&0x0041), None);
    }
}
