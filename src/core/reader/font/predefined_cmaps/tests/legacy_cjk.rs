use super::decode_one;
use super::super::*;

// ---- B6.3：CNS1 / Japan1 / Korea1 真实 ordering ----

#[test]
fn vendored_b5_and_b5pc_map_real_ordering() {
    let b5 = resolve_predefined_cmap("B5-H").unwrap();
    assert_eq!(b5.cmap.writing_mode(), 0);
    assert_eq!(decode_one(&b5, &[0xA1, 0x40]), 99);
    assert_eq!(decode_one(&b5, &[0xA1, 0xA1]), 162);
    assert_eq!(decode_one(&b5, &[0x00]), 13648);

    let b5pc = resolve_predefined_cmap("B5pc-H").unwrap();
    assert_eq!(decode_one(&b5pc, &[0x80]), 61);
    assert_eq!(decode_one(&b5pc, &[0xA1, 0x40]), 99);
    assert_eq!(decode_one(&b5pc, &[0xFD]), 96);
}

#[test]
fn vendored_cns_euc_h_maps_two_and_four_byte_codes() {
    let cmap = resolve_predefined_cmap("CNS-EUC-H").unwrap();
    assert_eq!(decode_one(&cmap, &[0xA1, 0xA1]), 99);
    // 4 字节 EUC-TW 码与 2 字节码同前缀，按最长码优先取 4 字节区间。
    assert_eq!(decode_one(&cmap, &[0x8E, 0xA2, 0xA1, 0xA1]), 5996);
    assert!(cmap.codespace_contains(4, 0x8EA2A1A1));
    assert!(!cmap.codespace_contains(2, 0x8EA2));
}

#[test]
fn vendored_cns1_and_cns2_map_seven_bit_rows() {
    let cns1 = resolve_predefined_cmap("CNS1-H").unwrap();
    assert_eq!(decode_one(&cns1, &[0x21, 0x21]), 99);
    let cns2 = resolve_predefined_cmap("CNS2-H").unwrap();
    assert_eq!(decode_one(&cns2, &[0x21, 0x21]), 5996);
}

#[test]
fn vendored_eten_b5_h_maps_real_ordering() {
    let cmap = resolve_predefined_cmap("ETen-B5-H").unwrap();
    assert_eq!(decode_one(&cmap, &[0xA1, 0x40]), 99);
    assert_eq!(decode_one(&cmap, &[0x00]), 13648);
}

#[test]
fn vendored_hk_b5_extensions_map_real_codes() {
    let ethk = resolve_predefined_cmap("ETHK-B5-H").unwrap();
    assert_eq!(decode_one(&ethk, &[0x87, 0x40]), 18965);
    let hkscs = resolve_predefined_cmap("HKscs-B5-H").unwrap();
    assert_eq!(decode_one(&hkscs, &[0x87, 0x40]), 18965);
    let hkgccs = resolve_predefined_cmap("HKgccs-B5-H").unwrap();
    assert_eq!(decode_one(&hkgccs, &[0x8A, 0x40]), 17160);
    let hkdlb = resolve_predefined_cmap("HKdlb-B5-H").unwrap();
    assert_eq!(decode_one(&hkdlb, &[0x8E, 0x40]), 15735);
    // HKdla/HKm314/HKm471 在 <A140> 处与 B5 本体重叠，取本表值 99。
    for name in ["HKdla-B5-H", "HKm314-B5-H", "HKm471-B5-H"] {
        let cmap = resolve_predefined_cmap(name).unwrap();
        assert_eq!(decode_one(&cmap, &[0xA1, 0x40]), 99, "{name}");
    }
}

#[test]
fn vendored_cns_vertical_variants_inherit_and_override() {
    for name in [
        "B5-V",
        "B5pc-V",
        "CNS-EUC-V",
        "CNS1-V",
        "CNS2-V",
        "ETen-B5-V",
        "ETenms-B5-V",
        "ETHK-B5-V",
        "HKscs-B5-V",
        "HKdla-B5-V",
        "HKdlb-B5-V",
        "HKgccs-B5-V",
        "HKm314-B5-V",
        "HKm471-B5-V",
    ] {
        assert_eq!(
            resolve_predefined_cmap(name).unwrap().cmap.writing_mode(),
            1,
            "{name}"
        );
    }
    let b5_v = resolve_predefined_cmap("B5-V").unwrap();
    assert_eq!(decode_one(&b5_v, &[0xA1, 0x40]), 99, "应继承 B5-H");
    let hkscs_v = resolve_predefined_cmap("HKscs-B5-V").unwrap();
    assert_eq!(decode_one(&hkscs_v, &[0xA1, 0x40]), 99, "应继承 HKscs-B5-H");
    let etenms_v = resolve_predefined_cmap("ETenms-B5-V").unwrap();
    assert_eq!(decode_one(&etenms_v, &[0xA1, 0xA2]), 135, "本地竖排映射");
}

#[test]
fn vendored_japan_h_euc_and_rksj_map_real_codes() {
    let h = resolve_predefined_cmap("H").unwrap();
    assert_eq!(decode_one(&h, &[0x21, 0x21]), 633);
    assert_eq!(decode_one(&h, &[0x21, 0x22]), 634);

    let euc = resolve_predefined_cmap("EUC-H").unwrap();
    assert_eq!(decode_one(&euc, &[0xA1, 0xA1]), 633);
    assert_eq!(decode_one(&euc, &[0xA2, 0xA1]), 727);
    assert_eq!(decode_one(&euc, &[0x8E, 0xA1]), 327);
    assert_eq!(decode_one(&euc, &[0x41]), 264);

    let rksj = resolve_predefined_cmap("RKSJ-H").unwrap();
    assert_eq!(decode_one(&rksj, &[0x81, 0x40]), 633);
    assert_eq!(decode_one(&rksj, &[0x82, 0xA0]), 843);
    assert_eq!(decode_one(&rksj, &[0xA0]), 326);
    assert_eq!(
        decode_one(
            &resolve_predefined_cmap("90ms-RKSJ-H").unwrap(),
            &[0x81, 0x40]
        ),
        633
    );
}

#[test]
fn vendored_japan_legacy_and_single_byte_variants() {
    for (name, expected) in [
        ("78-H", 633),
        ("Add-H", 633),
        ("Ext-H", 633),
        ("NWP-H", 633),
    ] {
        assert_eq!(
            decode_one(&resolve_predefined_cmap(name).unwrap(), &[0x21, 0x21]),
            expected,
            "{name}"
        );
    }
    assert_eq!(
        decode_one(&resolve_predefined_cmap("78-EUC-H").unwrap(), &[0xA1, 0xA1]),
        633
    );
    let rksj78 = resolve_predefined_cmap("78-RKSJ-H").unwrap();
    assert_eq!(decode_one(&rksj78, &[0x81, 0x40]), 633);
    assert_eq!(decode_one(&rksj78, &[0xDF]), 389);
    assert_eq!(
        decode_one(
            &resolve_predefined_cmap("83pv-RKSJ-H").unwrap(),
            &[0x81, 0x40]
        ),
        633
    );

    // 单字节字符集：Hankaku/Roman 与 Hiragana/Katakana 行使用不同 CID 区段。
    assert_eq!(
        decode_one(&resolve_predefined_cmap("Hankaku").unwrap(), &[0x41]),
        264
    );
    assert_eq!(
        decode_one(&resolve_predefined_cmap("Roman").unwrap(), &[0x41]),
        264
    );
    assert_eq!(
        decode_one(&resolve_predefined_cmap("Hiragana").unwrap(), &[0x21]),
        327
    );
    assert_eq!(
        decode_one(&resolve_predefined_cmap("Katakana").unwrap(), &[0x21]),
        327
    );
    assert_eq!(
        decode_one(&resolve_predefined_cmap("WP-Symbol").unwrap(), &[0x41]),
        8118
    );
}

#[test]
fn vendored_japan_vertical_variants_inherit_and_set_wmode() {
    for name in [
        "V",
        "78-V",
        "Add-V",
        "Ext-V",
        "NWP-V",
        "EUC-V",
        "RKSJ-V",
        "90ms-RKSJ-V",
    ] {
        assert_eq!(
            resolve_predefined_cmap(name).unwrap().cmap.writing_mode(),
            1,
            "{name}"
        );
    }
    for (name, input) in [
        ("V", vec![0x21, 0x22]),
        ("78-V", vec![0x21, 0x22]),
        ("Add-V", vec![0x21, 0x22]),
        ("Ext-V", vec![0x21, 0x22]),
        ("NWP-V", vec![0x21, 0x22]),
        ("EUC-V", vec![0xA1, 0xA2]),
        ("RKSJ-V", vec![0x81, 0x41]),
        ("90ms-RKSJ-V", vec![0x81, 0x41]),
    ] {
        assert_eq!(
            decode_one(&resolve_predefined_cmap(name).unwrap(), &input),
            7887,
            "{name}"
        );
    }
}

#[test]
fn vendored_korea_ksc_and_uhc_map_real_ordering() {
    let ksc = resolve_predefined_cmap("KSC-EUC-H").unwrap();
    assert_eq!(decode_one(&ksc, &[0xA1, 0xA1]), 101);
    assert_eq!(decode_one(&ksc, &[0xB0, 0xA1]), 1086);
    assert_eq!(
        decode_one(&resolve_predefined_cmap("KSC-H").unwrap(), &[0x21, 0x21]),
        101
    );

    let uhc = resolve_predefined_cmap("KSCms-UHC-H").unwrap();
    assert_eq!(decode_one(&uhc, &[0x81, 0x41]), 9333);
    assert_eq!(decode_one(&uhc, &[0xA1, 0xA1]), 101);
    assert_eq!(decode_one(&uhc, &[0xC0, 0xA1]), 2590);
    assert_eq!(
        decode_one(
            &resolve_predefined_cmap("KSCms-UHC-HW-H").unwrap(),
            &[0x81, 0x41]
        ),
        9333
    );

    let kscpc = resolve_predefined_cmap("KSCpc-EUC-H").unwrap();
    assert_eq!(decode_one(&kscpc, &[0xA1, 0x41]), 8191);
    assert_eq!(decode_one(&kscpc, &[0xA1, 0xA1]), 101);
    assert_eq!(decode_one(&kscpc, &[0xFE]), 99);
}

#[test]
fn vendored_korea_johab_and_vertical_variants() {
    let johab = resolve_predefined_cmap("KSC-Johab-H").unwrap();
    assert_eq!(decode_one(&johab, &[0xE0, 0x31]), 3436);
    assert_eq!(decode_one(&johab, &[0xA1, 0xA1]), 1917);
    // codespace 允许但未登记映射的码仍解为 CID 0。
    assert_eq!(decode_one(&johab, &[0xD8, 0x31]), 0);

    for name in [
        "KSC-V",
        "KSC-EUC-V",
        "KSC-Johab-V",
        "KSCms-UHC-V",
        "KSCms-UHC-HW-V",
        "KSCpc-EUC-V",
    ] {
        assert_eq!(
            resolve_predefined_cmap(name).unwrap().cmap.writing_mode(),
            1,
            "{name}"
        );
    }
    assert_eq!(
        decode_one(&resolve_predefined_cmap("KSC-V").unwrap(), &[0x21, 0x22]),
        8056
    );
    for name in ["KSC-EUC-V", "KSCms-UHC-V", "KSCms-UHC-HW-V", "KSCpc-EUC-V"] {
        assert_eq!(
            decode_one(&resolve_predefined_cmap(name).unwrap(), &[0xA1, 0xA2]),
            8056,
            "{name}"
        );
    }
    assert_eq!(
        decode_one(
            &resolve_predefined_cmap("KSC-Johab-V").unwrap(),
            &[0xD9, 0x32]
        ),
        8056
    );
}
