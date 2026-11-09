use super::decode_one;
use super::super::*;

#[test]
fn resolves_uni_gb_ucs2() {
    let cmap = resolve_predefined_cmap("UniGB-UCS2-H").unwrap();
    assert_eq!(cmap.cmap.writing_mode(), 0);
    assert!(cmap.codespace_contains(2, 0x4E2D));
    assert!(
        !cmap.codespace_contains(1, 0x41),
        "UCS-2 仅声明 2 字节码空间"
    );
    // 真实 ordering：`<0020> <007e> 1` 线性递增，`<4e2d>` 映射 CID 4559。
    assert_eq!(decode_one(&cmap, &[0x00, 0x41]), 34);
    assert_eq!(decode_one(&cmap, &[0x4E, 0x2D]), 4559);
    // `notdefrange <0000> <001f> 1`：整段映射到同一 CID。
    assert_eq!(decode_one(&cmap, &[0x00, 0x00]), 1);
    assert_eq!(decode_one(&cmap, &[0x00, 0x1F]), 1);
}

// 以上近似分支已删除：UniGB-UCS2-H 与其余 7 个 UniGB 变体均为 vendored 原文。
#[test]
fn vendored_uni_gb_utf16_maps_bmp_and_surrogate_pairs() {
    let cmap = resolve_predefined_cmap("UniGB-UTF16-H").unwrap();
    assert_eq!(cmap.cmap.writing_mode(), 0);
    assert!(cmap.codespace_contains(2, 0x4E2D));
    assert!(cmap.codespace_contains(4, 0xD83DDE00));
    assert!(!cmap.codespace_contains(4, 0x004E2D));
    assert_eq!(decode_one(&cmap, &[0x00, 0x41]), 34);
    assert_eq!(decode_one(&cmap, &[0x4E, 0x2D]), 4559);
    // 代理对（U+1F600）落在 4 字节 codespace 内但未登记映射 → CID 0。
    assert_eq!(decode_one(&cmap, &[0xD8, 0x3D, 0xDE, 0x00]), 0);
    assert_eq!(decode_one(&cmap, &[0x00, 0x00]), 1);
}

#[test]
fn vendored_uni_gb_utf32_maps_four_byte_codes() {
    let cmap = resolve_predefined_cmap("UniGB-UTF32-H").unwrap();
    assert_eq!(cmap.cmap.writing_mode(), 0);
    assert!(cmap.codespace_contains(4, 0x0000_4E2D));
    assert!(cmap.codespace_contains(4, 0x0010_FFFF));
    assert!(!cmap.codespace_contains(2, 0x4E2D));
    assert_eq!(decode_one(&cmap, &[0x00, 0x00, 0x00, 0x41]), 34);
    assert_eq!(decode_one(&cmap, &[0x00, 0x00, 0x4E, 0x2D]), 4559);
    // U+1F600 在 codespace 内但未登记映射 → CID 0。
    assert_eq!(decode_one(&cmap, &[0x00, 0x01, 0xF6, 0x00]), 0);
    assert_eq!(decode_one(&cmap, &[0x00, 0x00, 0x00, 0x00]), 1);
}

#[test]
fn vendored_uni_gb_utf8_maps_variable_length_codes() {
    let cmap = resolve_predefined_cmap("UniGB-UTF8-H").unwrap();
    assert_eq!(cmap.cmap.writing_mode(), 0);
    assert!(cmap.codespace_contains(1, 0x41));
    assert!(cmap.codespace_contains(2, 0xC2A0));
    assert!(cmap.codespace_contains(3, 0xE4B8AD));
    assert!(cmap.codespace_contains(4, 0xF09F9880));
    assert_eq!(decode_one(&cmap, &[0x41]), 34);
    assert_eq!(decode_one(&cmap, &[0xE4, 0xB8, 0xAD]), 4559);
    // 4 字节 emoji 在 codespace 内但未登记映射 → CID 0，按 4 字节消费。
    assert_eq!(decode_one(&cmap, &[0xF0, 0x9F, 0x98, 0x80]), 0);
    assert_eq!(decode_one(&cmap, &[0x00]), 1);
}

#[test]
fn vendored_uni_gb_vertical_variants_inherit_and_override() {
    for name in [
        "UniGB-UCS2-V",
        "UniGB-UTF16-V",
        "UniGB-UTF32-V",
        "UniGB-UTF8-V",
    ] {
        let cmap = resolve_predefined_cmap(name).unwrap();
        assert_eq!(cmap.cmap.writing_mode(), 1, "{name}");
    }

    // UCS2-V 无本地映射，全部继承 H。
    let ucs2_v = resolve_predefined_cmap("UniGB-UCS2-V").unwrap();
    assert_eq!(decode_one(&ucs2_v, &[0x4E, 0x2D]), 4559);
    assert_eq!(decode_one(&ucs2_v, &[0x00, 0x41]), 34);

    // UTF16/UTF32/UTF8-V 的竖排覆盖：`<2014>` 105 → 598，`<3001>` 97 → 575。
    let utf16_v = resolve_predefined_cmap("UniGB-UTF16-V").unwrap();
    assert_eq!(decode_one(&utf16_v, &[0x20, 0x14]), 598);
    assert_eq!(decode_one(&utf16_v, &[0x30, 0x01]), 575);
    assert_eq!(decode_one(&utf16_v, &[0x4E, 0x2D]), 4559, "继承 H");

    let utf32_v = resolve_predefined_cmap("UniGB-UTF32-V").unwrap();
    assert_eq!(decode_one(&utf32_v, &[0x00, 0x00, 0x20, 0x14]), 598);
    assert_eq!(decode_one(&utf32_v, &[0x00, 0x00, 0x30, 0x01]), 575);

    let utf8_v = resolve_predefined_cmap("UniGB-UTF8-V").unwrap();
    assert_eq!(decode_one(&utf8_v, &[0xE2, 0x80, 0x94]), 598);
    assert_eq!(decode_one(&utf8_v, &[0xE3, 0x80, 0x81]), 575);
    assert_eq!(decode_one(&utf8_v, &[0xE4, 0xB8, 0xAD]), 4559, "继承 H");
}

// B6.5：UniCNS 的真实值取自 `cmap_resources/` 原文（README.txt 记录 sha256）。
#[test]
fn vendored_uni_cns_ucs2_maps_real_ordering() {
    let cmap = resolve_predefined_cmap("UniCNS-UCS2-H").unwrap();
    assert_eq!(cmap.cmap.writing_mode(), 0);
    assert!(cmap.codespace_contains(2, 0x4E2D));
    assert!(
        !cmap.codespace_contains(1, 0x41),
        "UCS-2 仅声明 2 字节码空间"
    );
    // `<0020> <007e> 1` 线性递增；`<4e2d> <4e2d> 661`。
    assert_eq!(decode_one(&cmap, &[0x00, 0x41]), 34);
    assert_eq!(decode_one(&cmap, &[0x4E, 0x2D]), 661);
    assert_eq!(decode_one(&cmap, &[0x20, 0x13]), 121);
    // notdefrange <0000> <001f> 1：整段映射到同一 CID。
    assert_eq!(decode_one(&cmap, &[0x00, 0x00]), 1);
    assert_eq!(decode_one(&cmap, &[0x00, 0x1F]), 1);
}

#[test]
fn vendored_uni_cns_utf16_maps_bmp_and_surrogate_pairs() {
    let cmap = resolve_predefined_cmap("UniCNS-UTF16-H").unwrap();
    assert_eq!(cmap.cmap.writing_mode(), 0);
    assert!(cmap.codespace_contains(2, 0x4E2D));
    assert!(cmap.codespace_contains(4, 0xD840DC21));
    assert!(!cmap.codespace_contains(4, 0x004E2D));
    assert_eq!(decode_one(&cmap, &[0x4E, 0x2D]), 661);
    // U+20021 的代理对，原文登记 CID 15861。
    assert_eq!(decode_one(&cmap, &[0xD8, 0x40, 0xDC, 0x21]), 15861);
    // U+1F600 在 4 字节 codespace 内但未登记映射 → CID 0。
    assert_eq!(decode_one(&cmap, &[0xD8, 0x3D, 0xDE, 0x00]), 0);
    assert_eq!(decode_one(&cmap, &[0x00, 0x00]), 1);
}

#[test]
fn vendored_uni_cns_utf32_maps_four_byte_codes() {
    let cmap = resolve_predefined_cmap("UniCNS-UTF32-H").unwrap();
    assert_eq!(cmap.cmap.writing_mode(), 0);
    assert!(cmap.codespace_contains(4, 0x0000_4E2D));
    assert!(cmap.codespace_contains(4, 0x0010_FFFF));
    assert!(!cmap.codespace_contains(2, 0x4E2D));
    assert_eq!(decode_one(&cmap, &[0x00, 0x00, 0x4E, 0x2D]), 661);
    assert_eq!(decode_one(&cmap, &[0x00, 0x02, 0x00, 0x21]), 15861);
    assert_eq!(decode_one(&cmap, &[0x00, 0x01, 0xF6, 0x00]), 0);
    assert_eq!(decode_one(&cmap, &[0x00, 0x00, 0x00, 0x00]), 1);
}

#[test]
fn vendored_uni_cns_utf8_maps_variable_length_codes() {
    let cmap = resolve_predefined_cmap("UniCNS-UTF8-H").unwrap();
    assert_eq!(cmap.cmap.writing_mode(), 0);
    assert!(cmap.codespace_contains(1, 0x41));
    assert!(cmap.codespace_contains(2, 0xC2A0));
    assert!(cmap.codespace_contains(3, 0xE4B8AD));
    assert!(cmap.codespace_contains(4, 0xF0A080A1));
    assert_eq!(decode_one(&cmap, &[0x00]), 1);
    assert_eq!(decode_one(&cmap, &[0xC2, 0xA0]), 1);
    assert_eq!(decode_one(&cmap, &[0xE4, 0xB8, 0xAD]), 661);
    assert_eq!(decode_one(&cmap, &[0xF0, 0xA0, 0x80, 0xA1]), 15861);
    assert_eq!(decode_one(&cmap, &[0xF0, 0x9F, 0x98, 0x80]), 0);
}

#[test]
fn vendored_uni_cns_vertical_variants_inherit_and_override() {
    for name in [
        "UniCNS-UCS2-V",
        "UniCNS-UTF16-V",
        "UniCNS-UTF32-V",
        "UniCNS-UTF8-V",
    ] {
        assert_eq!(
            resolve_predefined_cmap(name).unwrap().cmap.writing_mode(),
            1,
            "{name}"
        );
    }

    // 竖排覆盖：`<2013>` 横排 121、竖排 120；未覆盖的 `<4e2d>` 继承 H 的 661。
    let ucs2_v = resolve_predefined_cmap("UniCNS-UCS2-V").unwrap();
    assert_eq!(decode_one(&ucs2_v, &[0x20, 0x13]), 120);
    assert_eq!(decode_one(&ucs2_v, &[0x4E, 0x2D]), 661, "继承 H");

    let utf16_v = resolve_predefined_cmap("UniCNS-UTF16-V").unwrap();
    assert_eq!(decode_one(&utf16_v, &[0x20, 0x13]), 120);
    assert_eq!(decode_one(&utf16_v, &[0xFF, 0x5B]), 134);
    assert_eq!(decode_one(&utf16_v, &[0x4E, 0x2D]), 661, "继承 H");

    let utf32_v = resolve_predefined_cmap("UniCNS-UTF32-V").unwrap();
    assert_eq!(decode_one(&utf32_v, &[0x00, 0x00, 0x20, 0x13]), 120);
    assert_eq!(
        decode_one(&utf32_v, &[0x00, 0x00, 0x4E, 0x2D]),
        661,
        "继承 H"
    );

    let utf8_v = resolve_predefined_cmap("UniCNS-UTF8-V").unwrap();
    assert_eq!(decode_one(&utf8_v, &[0xE2, 0x80, 0x93]), 120);
    assert_eq!(decode_one(&utf8_v, &[0xEF, 0xBD, 0x9B]), 134);
    assert_eq!(decode_one(&utf8_v, &[0xE4, 0xB8, 0xAD]), 661, "继承 H");
}
