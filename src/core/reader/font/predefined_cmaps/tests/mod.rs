use super::*;

fn decode_one(cmap: &PredefinedCMap, input: &[u8]) -> u16 {
    let mut decoded = cmap.cmap.decode(input).unwrap();
    assert_eq!(decoded.len(), 1, "expected one code for {input:02X?}");
    decoded.pop().unwrap().1
}

mod adobe_roster;
mod legacy_cjk;
mod legacy_gb;
mod unicode_gb_cns;
mod unicode_jis_ks;
