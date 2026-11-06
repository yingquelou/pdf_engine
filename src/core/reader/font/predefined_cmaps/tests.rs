use super::*;

fn decode_one(cmap: &PredefinedCMap, input: &[u8]) -> u16 {
    let mut decoded = cmap.cmap.decode(input).unwrap();
    assert_eq!(decoded.len(), 1, "expected one code for {input:02X?}");
    decoded.pop().unwrap().1
}

#[test]
fn resolves_adobe_gb1() {
    let cmap = resolve_predefined_cmap("Adobe-GB1").unwrap();
    assert_eq!(decode_one(&cmap, &[0x41]), 0x41);
    assert_eq!(decode_one(&cmap, &[0xA1, 0xA1]), 0x0100);
}

#[test]
fn resolves_adobe_cns1() {
    let cmap = resolve_predefined_cmap("Adobe-CNS1").unwrap();
    assert_eq!(decode_one(&cmap, &[0x41]), 0x41);
}

#[test]
fn resolves_adobe_japan1() {
    let cmap = resolve_predefined_cmap("Adobe-Japan1").unwrap();
    assert_eq!(decode_one(&cmap, &[0x41]), 0x41);
}

#[test]
fn resolves_adobe_korea1() {
    let cmap = resolve_predefined_cmap("Adobe-Korea1").unwrap();
    assert_eq!(decode_one(&cmap, &[0x41]), 0x41);
}

#[test]
fn rejects_unknown_cmap() {
    assert!(resolve_predefined_cmap("Unknown-CMap").is_err());
}

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

// B6.5：UniJIS 的真实值取自 `cmap_resources/` 原文（README.txt 记录 sha256）。
#[test]
fn vendored_uni_jis_ucs2_and_hw_variants() {
    let ucs2 = resolve_predefined_cmap("UniJIS-UCS2-H").unwrap();
    assert_eq!(ucs2.cmap.writing_mode(), 0);
    assert_eq!(decode_one(&ucs2, &[0x00, 0x41]), 34);
    assert_eq!(decode_one(&ucs2, &[0x00, 0x5C]), 97);
    assert_eq!(decode_one(&ucs2, &[0x4E, 0x2D]), 2980);
    assert_eq!(decode_one(&ucs2, &[0x00, 0xB0]), 707);
    assert_eq!(decode_one(&ucs2, &[0x00, 0x1F]), 1);

    // HW 变体把 ASCII 段整体改映射（`<0020> <005b> 231`）。
    let hw = resolve_predefined_cmap("UniJIS-UCS2-HW-H").unwrap();
    assert_eq!(decode_one(&hw, &[0x00, 0x20]), 231);
    assert_eq!(decode_one(&hw, &[0x00, 0x5C]), 8719);

    // 竖排覆盖 `<00b0>`（横排 707 → 竖排 8269），未覆盖码继承 H。
    let v = resolve_predefined_cmap("UniJIS-UCS2-V").unwrap();
    assert_eq!(v.cmap.writing_mode(), 1);
    assert_eq!(decode_one(&v, &[0x00, 0xB0]), 8269);
    assert_eq!(decode_one(&v, &[0x4E, 0x2D]), 2980);

    let hw_v = resolve_predefined_cmap("UniJIS-UCS2-HW-V").unwrap();
    assert_eq!(hw_v.cmap.writing_mode(), 1);
    assert_eq!(decode_one(&hw_v, &[0x00, 0x20]), 231);
    assert_eq!(decode_one(&hw_v, &[0x00, 0x5C]), 8719);
}

#[test]
fn vendored_uni_jis_utf16_utf32_utf8_variants() {
    let utf16 = resolve_predefined_cmap("UniJIS-UTF16-H").unwrap();
    assert_eq!(decode_one(&utf16, &[0x4E, 0x2D]), 2980);
    // U+2000B 的代理对，原文登记 CID 13839。
    assert_eq!(decode_one(&utf16, &[0xD8, 0x40, 0xDC, 0x0B]), 13839);
    assert_eq!(decode_one(&utf16, &[0xD8, 0x3D, 0xDE, 0x00]), 0);
    assert_eq!(decode_one(&utf16, &[0x00, 0x00]), 1);
    assert!(utf16.codespace_contains(4, 0xD840DC0B));

    let utf32 = resolve_predefined_cmap("UniJIS-UTF32-H").unwrap();
    assert_eq!(decode_one(&utf32, &[0x00, 0x00, 0x4E, 0x2D]), 2980);
    assert_eq!(decode_one(&utf32, &[0x00, 0x02, 0x00, 0x0B]), 13839);
    assert_eq!(decode_one(&utf32, &[0x00, 0x00, 0x00, 0x00]), 1);

    let utf8 = resolve_predefined_cmap("UniJIS-UTF8-H").unwrap();
    assert_eq!(decode_one(&utf8, &[0xE4, 0xB8, 0xAD]), 2980);
    assert_eq!(decode_one(&utf8, &[0xF0, 0xA0, 0x80, 0x8B]), 13839);
    assert_eq!(decode_one(&utf8, &[0x00]), 1);
    assert!(utf8.codespace_contains(1, 0x41));
    assert!(utf8.codespace_contains(2, 0xC2A0));
    assert!(utf8.codespace_contains(3, 0xE4B8AD));
    assert!(utf8.codespace_contains(4, 0xF0A0808B));
}

#[test]
fn vendored_uni_jis_2004_and_pro_and_x0213_variants() {
    for name in [
        "UniJIS-UTF16-V",
        "UniJIS-UTF32-V",
        "UniJIS-UTF8-V",
        "UniJIS2004-UTF16-V",
        "UniJIS2004-UTF32-V",
        "UniJIS2004-UTF8-V",
        "UniJISPro-UCS2-HW-V",
        "UniJISPro-UCS2-V",
        "UniJISPro-UTF8-V",
        "UniJISX0213-UTF32-V",
        "UniJISX02132004-UTF32-V",
    ] {
        assert_eq!(
            resolve_predefined_cmap(name).unwrap().cmap.writing_mode(),
            1,
            "{name}"
        );
    }

    // 2004 横排与竖排的真实值。
    let h2004 = resolve_predefined_cmap("UniJIS2004-UTF16-H").unwrap();
    assert_eq!(decode_one(&h2004, &[0x4E, 0x2D]), 2980);
    let h2004_32 = resolve_predefined_cmap("UniJIS2004-UTF32-H").unwrap();
    assert_eq!(decode_one(&h2004_32, &[0x00, 0x00, 0x4E, 0x2D]), 2980);
    let h2004_8 = resolve_predefined_cmap("UniJIS2004-UTF8-H").unwrap();
    assert_eq!(decode_one(&h2004_8, &[0xE4, 0xB8, 0xAD]), 2980);
    let v2004 = resolve_predefined_cmap("UniJIS2004-UTF16-V").unwrap();
    assert_eq!(decode_one(&v2004, &[0x00, 0xB0]), 8269);
    let v2004_32 = resolve_predefined_cmap("UniJIS2004-UTF32-V").unwrap();
    assert_eq!(decode_one(&v2004_32, &[0x00, 0x00, 0x00, 0xB0]), 8269);
    let v2004_8 = resolve_predefined_cmap("UniJIS2004-UTF8-V").unwrap();
    assert_eq!(decode_one(&v2004_8, &[0xC2, 0xB0]), 8269);

    // Pro 竖排只覆盖竖排码位，其余继承 UniJIS-UCS2-H / UniJIS-UTF8-H。
    let pro = resolve_predefined_cmap("UniJISPro-UCS2-V").unwrap();
    assert_eq!(decode_one(&pro, &[0x00, 0xB0]), 8269);
    assert_eq!(decode_one(&pro, &[0x4E, 0x2D]), 2980, "继承 H");
    let pro_hw = resolve_predefined_cmap("UniJISPro-UCS2-HW-V").unwrap();
    assert_eq!(decode_one(&pro_hw, &[0x00, 0x20]), 231);
    assert_eq!(decode_one(&pro_hw, &[0x00, 0x5C]), 8719);
    assert_eq!(decode_one(&pro_hw, &[0x4E, 0x2D]), 2980);
    let pro_utf8 = resolve_predefined_cmap("UniJISPro-UTF8-V").unwrap();
    assert_eq!(decode_one(&pro_utf8, &[0xC2, 0xB0]), 8269);
    assert_eq!(decode_one(&pro_utf8, &[0xE4, 0xB8, 0xAD]), 2980);

    // X0213 的 4 字节 identity 映射与竖排覆盖。
    let x0213 = resolve_predefined_cmap("UniJISX0213-UTF32-H").unwrap();
    assert_eq!(decode_one(&x0213, &[0x00, 0x00, 0x00, 0x5C]), 97);
    assert_eq!(decode_one(&x0213, &[0x00, 0x00, 0x00, 0xA0]), 1);
    assert_eq!(decode_one(&x0213, &[0x00, 0x00, 0x00, 0x00]), 1);
    let x0213_v = resolve_predefined_cmap("UniJISX0213-UTF32-V").unwrap();
    assert_eq!(decode_one(&x0213_v, &[0x00, 0x00, 0x20, 0x15]), 7892);
    let x02132004 = resolve_predefined_cmap("UniJISX02132004-UTF32-H").unwrap();
    assert_eq!(decode_one(&x02132004, &[0x00, 0x00, 0x00, 0x5C]), 97);
    let x02132004_v = resolve_predefined_cmap("UniJISX02132004-UTF32-V").unwrap();
    assert_eq!(decode_one(&x02132004_v, &[0x00, 0x00, 0x20, 0x15]), 7892);
}

// B6.5：UniKS 的真实值取自 `cmap_resources/` 原文（README.txt 记录 sha256）。
#[test]
fn vendored_uni_ks_maps_real_ordering() {
    let ucs2 = resolve_predefined_cmap("UniKS-UCS2-H").unwrap();
    assert_eq!(ucs2.cmap.writing_mode(), 0);
    assert_eq!(decode_one(&ucs2, &[0x00, 0x41]), 34);
    assert_eq!(decode_one(&ucs2, &[0xAC, 0x00]), 1086);
    assert_eq!(decode_one(&ucs2, &[0x4E, 0x2D]), 6922);
    assert_eq!(decode_one(&ucs2, &[0x00, 0xB0]), 138);
    assert_eq!(decode_one(&ucs2, &[0x00, 0x1F]), 1);

    let utf16 = resolve_predefined_cmap("UniKS-UTF16-H").unwrap();
    assert_eq!(decode_one(&utf16, &[0x4E, 0x2D]), 6922);
    assert_eq!(decode_one(&utf16, &[0xAC, 0x00]), 1086);
    // 代理区在 codespace 内，但原文没有补充平面映射 → CID 0。
    assert!(utf16.codespace_contains(4, 0xD800DC00));
    assert_eq!(decode_one(&utf16, &[0xD8, 0x00, 0xDC, 0x00]), 0);
    assert_eq!(decode_one(&utf16, &[0x00, 0x00]), 1);

    let utf32 = resolve_predefined_cmap("UniKS-UTF32-H").unwrap();
    assert_eq!(decode_one(&utf32, &[0x00, 0x00, 0x4E, 0x2D]), 6922);
    assert_eq!(decode_one(&utf32, &[0x00, 0x00, 0xAC, 0x00]), 1086);
    assert!(utf32.codespace_contains(4, 0x0002_0000));
    assert_eq!(decode_one(&utf32, &[0x00, 0x02, 0x00, 0x00]), 0);
    assert_eq!(decode_one(&utf32, &[0x00, 0x00, 0x00, 0x00]), 1);

    let utf8 = resolve_predefined_cmap("UniKS-UTF8-H").unwrap();
    assert_eq!(decode_one(&utf8, &[0xEA, 0xB0, 0x80]), 1086);
    assert_eq!(decode_one(&utf8, &[0xE4, 0xB8, 0xAD]), 6922);
    assert!(utf8.codespace_contains(4, 0xF0908080));
    assert_eq!(decode_one(&utf8, &[0xF0, 0x90, 0x80, 0x80]), 0);
    assert_eq!(decode_one(&utf8, &[0x00]), 1);
}

#[test]
fn vendored_uni_ks_vertical_variants_inherit_and_override() {
    for name in [
        "UniKS-UCS2-V",
        "UniKS-UTF16-V",
        "UniKS-UTF32-V",
        "UniKS-UTF8-V",
    ] {
        assert_eq!(
            resolve_predefined_cmap(name).unwrap().cmap.writing_mode(),
            1,
            "{name}"
        );
    }

    let ucs2_v = resolve_predefined_cmap("UniKS-UCS2-V").unwrap();
    assert_eq!(decode_one(&ucs2_v, &[0x20, 0x13]), 8059);
    assert_eq!(decode_one(&ucs2_v, &[0x4E, 0x2D]), 6922, "继承 H");

    let utf16_v = resolve_predefined_cmap("UniKS-UTF16-V").unwrap();
    assert_eq!(decode_one(&utf16_v, &[0x20, 0x13]), 8059);
    assert_eq!(decode_one(&utf16_v, &[0x30, 0x01]), 8056);
    assert_eq!(decode_one(&utf16_v, &[0x4E, 0x2D]), 6922, "继承 H");

    let utf32_v = resolve_predefined_cmap("UniKS-UTF32-V").unwrap();
    assert_eq!(decode_one(&utf32_v, &[0x00, 0x00, 0x20, 0x13]), 8059);
    assert_eq!(decode_one(&utf32_v, &[0x00, 0x00, 0x30, 0x01]), 8056);

    let utf8_v = resolve_predefined_cmap("UniKS-UTF8-V").unwrap();
    assert_eq!(decode_one(&utf8_v, &[0xE2, 0x80, 0x93]), 8059);
    assert_eq!(decode_one(&utf8_v, &[0xE3, 0x80, 0x81]), 8056);
}

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

#[test]
fn vendored_identity_cmaps_map_two_byte_codes_to_same_cid() {
    for name in ["Adobe-CNS1-7", "Adobe-Japan1-7", "Adobe-Korea1-2"] {
        let cmap = resolve_predefined_cmap(name).unwrap();
        assert_eq!(cmap.cmap.writing_mode(), 0, "{name}");
        assert_eq!(decode_one(&cmap, &[0x01, 0x00]), 256, "{name}");
        assert!(cmap.codespace_contains(2, 0x0100));
        assert!(
            !cmap.codespace_contains(1, 0x41),
            "{name} 仅声明 2 字节码空间"
        );
    }
    let cns = resolve_predefined_cmap("Adobe-CNS1-7").unwrap();
    assert!(
        cns.cmap.decode(&[0x4B, 0x00]).is_err(),
        "超出 codespace 上界应报错"
    );
}

#[test]
fn rejects_unicode_and_out_of_scope_variants() {
    for name in [
        "UniCNS-UCS2-HW",
        "UniJIS-UTF16-HW",
        "UniKS-UCS2-HW",
        "Adobe-KR-9",
        "Adobe-Manga1-0",
        "Adobe-Japan2-0",
    ] {
        assert!(resolve_predefined_cmap(name).is_err(), "{name}");
    }
}
