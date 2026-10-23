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
        _ => Err(FontError::Invalid(format!(
            "unknown predefined CMap: {}",
            name
        ))),
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
}
