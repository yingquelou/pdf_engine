use super::decode_one;
use super::super::*;

// 以下断言值来自 `cmap_resources/` 的 Adobe 原文（README.txt 记录
// 上游提交与 sha256），不是推测。
#[test]
fn vendored_gb_euc_h_maps_real_ordering() {
    let cmap = resolve_predefined_cmap("GB-EUC-H").unwrap();
    assert_eq!(cmap.cmap.writing_mode(), 0);
    assert_eq!(decode_one(&cmap, &[0x21]), 814);
    assert_eq!(decode_one(&cmap, &[0x41]), 846);
    assert_eq!(decode_one(&cmap, &[0xA1, 0xA1]), 96);
    assert_eq!(decode_one(&cmap, &[0xB0, 0xA1]), 940);
    assert!(cmap.codespace_contains(1, 0x41));
    assert!(!cmap.codespace_contains(2, 0x0041));
}

#[test]
fn vendored_gbpc_euc_h_maps_pc_ordering() {
    let cmap = resolve_predefined_cmap("GBpc-EUC-H").unwrap();
    assert_eq!(decode_one(&cmap, &[0x20]), 1);
    assert_eq!(decode_one(&cmap, &[0x80]), 7712);
    assert_eq!(decode_one(&cmap, &[0xFD]), 7713);
    assert_eq!(decode_one(&cmap, &[0xA1, 0xA1]), 96);
    assert_eq!(decode_one(&cmap, &[0xC0, 0xA1]), 2444);
}

#[test]
fn vendored_gb_h_maps_seven_bit_rows() {
    let cmap = resolve_predefined_cmap("GB-H").unwrap();
    assert_eq!(decode_one(&cmap, &[0x21, 0x21]), 96);
    assert_eq!(decode_one(&cmap, &[0x21, 0x22]), 97);
}

#[test]
fn vendored_vertical_variants_inherit_horizontal_and_set_wmode() {
    for name in ["GB-EUC-V", "GBpc-EUC-V", "GB-V"] {
        assert_eq!(
            resolve_predefined_cmap(name).unwrap().cmap.writing_mode(),
            1
        );
    }

    let euc_v = resolve_predefined_cmap("GB-EUC-V").unwrap();
    assert_eq!(decode_one(&euc_v, &[0xA1, 0xA2]), 575);
    assert_eq!(decode_one(&euc_v, &[0xA3, 0xA1]), 578);
    // 未被 V 覆盖的码从 GB-EUC-H 基座继承。
    assert_eq!(decode_one(&euc_v, &[0xA1, 0xA1]), 96);
    assert_eq!(decode_one(&euc_v, &[0xB0, 0xA1]), 940);

    let pc_v = resolve_predefined_cmap("GBpc-EUC-V").unwrap();
    assert_eq!(decode_one(&pc_v, &[0x80]), 7712);
    assert_eq!(decode_one(&pc_v, &[0xFD]), 7713);

    let gb_v = resolve_predefined_cmap("GB-V").unwrap();
    assert_eq!(decode_one(&gb_v, &[0x21, 0x21]), 96);
    assert_eq!(decode_one(&gb_v, &[0x21, 0x22]), 575);
}

#[test]
fn vendored_gbk_euc_h_maps_real_ordering() {
    let cmap = resolve_predefined_cmap("GBK-EUC-H").unwrap();
    assert_eq!(decode_one(&cmap, &[0x20]), 7716);
    assert_eq!(decode_one(&cmap, &[0x21]), 814);
    assert_eq!(decode_one(&cmap, &[0x81, 0x40]), 10072);
    assert_eq!(decode_one(&cmap, &[0x81, 0x79]), 8281);
    // `1 beginnotdefrange <00> <1f> 7716`：整段映射到同一 CID，不随码递增。
    assert_eq!(decode_one(&cmap, &[0x00]), 7716);
    assert_eq!(decode_one(&cmap, &[0x1F]), 7716);
}

#[test]
fn vendored_gbkp_euc_h_maps_pc_ordering() {
    let cmap = resolve_predefined_cmap("GBKp-EUC-H").unwrap();
    assert_eq!(decode_one(&cmap, &[0x20]), 1);
    assert_eq!(decode_one(&cmap, &[0x81, 0x40]), 10072);
    assert_eq!(decode_one(&cmap, &[0x00]), 1);
}

#[test]
fn vendored_gbt_h_maps_seven_bit_rows() {
    let cmap = resolve_predefined_cmap("GBT-H").unwrap();
    assert_eq!(decode_one(&cmap, &[0x21, 0x21]), 96);
    assert_eq!(decode_one(&cmap, &[0x21, 0x22]), 97);
}

#[test]
fn vendored_gbt_euc_h_maps_real_ordering() {
    let cmap = resolve_predefined_cmap("GBT-EUC-H").unwrap();
    assert_eq!(decode_one(&cmap, &[0x20]), 7716);
    assert_eq!(decode_one(&cmap, &[0xA1, 0xA1]), 96);
    assert_eq!(decode_one(&cmap, &[0xA2, 0xB1]), 190);
    assert_eq!(decode_one(&cmap, &[0x00]), 7716);
}

#[test]
fn vendored_gbtpc_euc_h_maps_pc_ordering() {
    let cmap = resolve_predefined_cmap("GBTpc-EUC-H").unwrap();
    assert_eq!(decode_one(&cmap, &[0x20]), 1);
    assert_eq!(decode_one(&cmap, &[0x80]), 7712);
    assert_eq!(decode_one(&cmap, &[0xA1, 0xA1]), 96);
    assert_eq!(decode_one(&cmap, &[0x00]), 1);
}

#[test]
fn vendored_gbk_gbt_vertical_variants_inherit_horizontal_and_set_wmode() {
    for name in [
        "GBK-EUC-V",
        "GBKp-EUC-V",
        "GBT-V",
        "GBT-EUC-V",
        "GBTpc-EUC-V",
    ] {
        assert_eq!(
            resolve_predefined_cmap(name).unwrap().cmap.writing_mode(),
            1
        );
    }

    let gbk_v = resolve_predefined_cmap("GBK-EUC-V").unwrap();
    assert_eq!(decode_one(&gbk_v, &[0xA1, 0xA2]), 575);
    // V 未覆盖的码从 GBK-EUC-H 继承，含 H 的 `cidrange` 区间偏移。
    assert_eq!(decode_one(&gbk_v, &[0x81, 0x40]), 10072);

    let gbt_v = resolve_predefined_cmap("GBT-V").unwrap();
    assert_eq!(decode_one(&gbt_v, &[0x21, 0x22]), 575);
    assert_eq!(decode_one(&gbt_v, &[0x21, 0x21]), 96);
}

// ---- GBK2K：4 字节 + 2 字节 codespace 前缀重叠，声明最长码优先 ----

#[test]
fn vendored_gbk2k_h_maps_four_and_two_byte_codes() {
    let cmap = resolve_predefined_cmap("GBK2K-H").unwrap();
    assert_eq!(cmap.cmap.writing_mode(), 0);
    assert!(cmap.codespace_contains(1, 0x41));
    assert!(cmap.codespace_contains(2, 0x8140));
    assert!(cmap.codespace_contains(4, 0x81308130));
    assert!(cmap.codespace_contains(4, 0xFE39FE39));
    // 0x8130 不在 2 字节 `<8140>`–`<FEFE>` 内，不同长度区间的重叠来自
    // 4 字节 codespace 的数值上界与 2 字节区间。
    assert!(!cmap.codespace_contains(2, 0x8130));
    assert_eq!(decode_one(&cmap, &[0x81, 0x40]), 10072);
    assert_eq!(decode_one(&cmap, &[0x81, 0x79]), 8281);
    assert_eq!(decode_one(&cmap, &[0x81, 0x30, 0x84, 0x36]), 22354);
    assert_eq!(decode_one(&cmap, &[0x82, 0x30, 0x81, 0x30]), 22690);
    // 1 字节 `<20>`–`<7e>` 映射到 CID 1；`notdefrange <00> <1f> 1` 整段为 1；
    // 1 字节码空间内未登记的 `<7f>` 解为 CID 0。
    assert_eq!(decode_one(&cmap, &[0x20]), 1);
    assert_eq!(decode_one(&cmap, &[0x00]), 1);
    assert_eq!(decode_one(&cmap, &[0x7F]), 0);
}

#[test]
fn vendored_gbk2k_h_prefers_the_longest_declared_code() {
    let cmap = resolve_predefined_cmap("GBK2K-H").unwrap();
    // `<8230>` 落在 2 字节 codespace 内但未登记映射；4 字节 `<82308130>`
    // 有真实映射。输入 4 字节时必须按 4 字节消费（22690），不得回退成
    // 2 字节 CID 0 再拆解剩余两字节。
    assert_eq!(decode_one(&cmap, &[0x82, 0x30, 0x81, 0x30]), 22690);
    // 4 字节码在 codespace 内但未登记映射时仍按 4 字节消费 → CID 0。
    assert_eq!(decode_one(&cmap, &[0x81, 0x30, 0x81, 0x30]), 0);
    // 只剩 2 字节时最长码不可用，回退到 2 字节 codespace。
    assert_eq!(decode_one(&cmap, &[0x82, 0x30]), 0);
}

#[test]
fn vendored_gbk2k_vertical_variant_inherits_horizontal() {
    let cmap = resolve_predefined_cmap("GBK2K-V").unwrap();
    assert_eq!(cmap.cmap.writing_mode(), 1);
    assert_eq!(decode_one(&cmap, &[0xA1, 0xA2]), 575);
    // V 未覆盖的 2 字节与 4 字节码从 GBK2K-H 继承。
    assert_eq!(decode_one(&cmap, &[0x81, 0x40]), 10072);
    assert_eq!(decode_one(&cmap, &[0x81, 0x30, 0x84, 0x36]), 22354);
}
