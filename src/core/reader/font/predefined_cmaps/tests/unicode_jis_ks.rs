use super::decode_one;
use super::super::*;

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
