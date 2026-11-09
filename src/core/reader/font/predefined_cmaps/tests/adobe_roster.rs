use super::decode_one;
use super::super::*;

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
