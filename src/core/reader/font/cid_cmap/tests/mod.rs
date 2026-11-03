use super::{CidCMap, ToUnicodeMap, identity_cmap, invalid_cmap};

mod cmap_dict;
mod consistency;
mod use_cmap;

#[test]
fn parses_codespaces_cid_chars_ranges_and_wmode() {
    let source = b"
        /WMode 1 def
        2 begincodespacerange
          <00> <7F>
          <8100> <81FF>
        endcodespacerange
        2 begincidchar
          <41> 5
          <8101> 99
        endcidchar
        1 begincidrange
          <50> <52> 20
        endcidrange";
    let cmap = CidCMap::parse(source).unwrap();
    assert_eq!(cmap.writing_mode(), 1);
    assert_eq!(
        cmap.decode(&[0x41, 0x50, 0x51, 0x81, 0x01]).unwrap(),
        [
            (vec![0x41], 5),
            (vec![0x50], 20),
            (vec![0x51], 21),
            (vec![0x81, 1], 99)
        ]
    );
    assert_eq!(cmap.decode(&[0x42]).unwrap(), [(vec![0x42], 0)]);
}

#[test]
fn identity_cmaps_decode_two_byte_codes_in_both_modes() {
    for mode in 0..=1 {
        let cmap = identity_cmap(mode);
        assert_eq!(cmap.writing_mode(), mode);
        assert_eq!(
            cmap.decode(&[0x00, 0x41, 0x20, 0xAC]).unwrap(),
            [(vec![0, 0x41], 0x41), (vec![0x20, 0xAC], 0x20AC)]
        );
    }
}

#[test]
fn rejects_invalid_cmap_sections_ranges_and_inputs() {
    assert!(CidCMap::parse(b"").is_err());
    assert!(CidCMap::parse(b"1 begincodespacerange <00> <FF> endcodespacerange usecmap").is_err());
    assert!(
        CidCMap::parse(b"/WMode 2 def 1 begincodespacerange <00> <FF> endcodespacerange").is_err()
    );
    assert!(
        CidCMap::parse(b"2 begincodespacerange <00> <FF> <0000> <FFFF> endcodespacerange").is_err()
    );
    assert!(
        CidCMap::parse(
            b"1 begincodespacerange <00> <FF> endcodespacerange 1 begincidrange <01> <02> 65535 endcidrange"
        )
        .is_err()
    );
    let cmap = CidCMap::parse(b"1 begincodespacerange <00> <FF> endcodespacerange").unwrap();
    assert!(cmap.decode(&[0, 1, 2]).is_ok());
    let one_byte = CidCMap::parse(
        b"1 begincodespacerange <00> <FF> endcodespacerange 1 begincidchar <00> 2 endcidchar",
    )
    .unwrap();
    assert_eq!(
        one_byte.decode(&[0x01, 0x00]).unwrap(),
        [(vec![0x01], 0), (vec![0x00], 2)]
    );
}

#[test]
fn usecmap_inherits_identity_base_and_overrides_mappings() {
    let source = b"
        /Identity-H usecmap
        1 begincodespacerange <0000> <FFFF> endcodespacerange
        1 begincidrange <0041> <0042> 100 endcidrange";
    let cmap = CidCMap::parse_with_base(source, None, |name| match name {
        "Identity-H" => Ok(identity_cmap(0)),
        other => panic!("unexpected base {other}"),
    })
    .unwrap();
    assert_eq!(
        cmap.decode(&[0, 0x41, 0, 0x43]).unwrap(),
        [(vec![0, 0x41], 100), (vec![0, 0x43], 0x43)]
    );
}

#[test]
fn usecmap_inherits_writing_mode_and_rejects_unknown_bases() {
    let source = b"/Identity-V usecmap 1 begincodespacerange <0000> <FFFF> endcodespacerange";
    let cmap = CidCMap::parse_with_base(source, None, |_| Ok(identity_cmap(1))).unwrap();
    assert_eq!(cmap.writing_mode(), 1);
    assert!(CidCMap::parse_with_base(source, None, |_| Err(invalid_cmap("unknown base"))).is_err());
    assert!(CidCMap::parse(source).is_err());
    assert!(
        CidCMap::parse_with_base(
            b"usecmap 1 begincodespacerange <00> <FF> endcodespacerange",
            None,
            |_| Ok(identity_cmap(0)),
        )
        .is_err()
    );
}

#[test]
fn usecmap_clips_inherited_mappings_to_defined_codespace() {
    let source = b"/Identity-H usecmap 1 begincodespacerange <00> <FF> endcodespacerange";
    let cmap = CidCMap::parse_with_base(source, None, |_| Ok(identity_cmap(0))).unwrap();
    assert_eq!(cmap.decode(&[0x41]).unwrap(), [(vec![0x41], 0)]);
    assert_eq!(cmap.decode(&[0x00, 0x41]).unwrap().len(), 2);
}

#[test]
fn parses_tounicode_bfchar_and_both_bfrange_forms() {
    let source = b"
        2 beginbfchar
          <01> <0041>
          <02> <D83DDE00>
        endbfchar
        2 beginbfrange
          <10> <12> <0061>
          <20> <21> [<4E2D> <6587>]
        endbfrange";
    let map = ToUnicodeMap::parse(source).unwrap();
    assert_eq!(map.unicode_for(&[0x01]), Some("A"));
    assert_eq!(map.unicode_for(&[0x02]), Some("😀"));
    assert_eq!(map.unicode_for(&[0x10]), Some("a"));
    assert_eq!(map.unicode_for(&[0x11]), Some("b"));
    assert_eq!(map.unicode_for(&[0x12]), Some("c"));
    assert_eq!(map.unicode_for(&[0x20]), Some("中"));
    assert_eq!(map.unicode_for(&[0x21]), Some("文"));
    assert_eq!(map.unicode_for(&[0x22]), None);
}

#[test]
fn rejects_malformed_tounicode_mappings() {
    assert!(ToUnicodeMap::parse(b"1 beginbfchar <01> <D800> endbfchar").is_err());
    assert!(ToUnicodeMap::parse(b"1 beginbfchar <01> <0041> <02> <0042> endbfchar").is_err());
    assert!(ToUnicodeMap::parse(b"1 beginbfrange <FE> <FF> <FFFF> endbfrange").is_err());
    assert!(ToUnicodeMap::parse(b"1 beginbfrange <01> <02> [<0041>] endbfrange").is_err());
    // usecmap 的基名必须写成带斜杠的名称对象。
    assert!(ToUnicodeMap::parse(b"Base usecmap 1 beginbfchar <01> <0041> endbfchar").is_err());
}

#[test]
fn tounicode_duplicate_source_uses_last_mapping() {
    let map = ToUnicodeMap::parse(b"2 beginbfchar <01> <0041> <01> <0042> endbfchar").unwrap();
    assert_eq!(map.unicode_for(&[0x01]), Some("B"));
}

#[test]
fn tounicode_usecmap_inherits_base_and_prefers_local_mappings() {
    let source = b"/Base usecmap 2 beginbfchar <01> <0041> <02> <0042> endbfchar";
    let map = ToUnicodeMap::parse_lenient_with(source, |name| match name {
        "Base" => {
            Ok(ToUnicodeMap::parse(b"2 beginbfchar <02> <0058> <03> <0059> endbfchar").unwrap())
        }
        other => panic!("unexpected base {other}"),
    })
    .expect("基座可解析时应合并映射");
    assert_eq!(map.unicode_for(&[0x01]), Some("A"));
    assert_eq!(map.unicode_for(&[0x02]), Some("B"), "本地映射应覆盖基座");
    assert_eq!(
        map.unicode_for(&[0x03]),
        Some("Y"),
        "未覆盖的基座映射应保留"
    );
}

#[test]
fn tounicode_usecmap_clips_inherited_mappings_to_local_codespace() {
    // 基座：1 字节 <01>、2 字节 <0001> 与 <0100>；本地 codespace 仅
    // <0000>–<00FF>，因此 1 字节与 <0100> 的继承映射都应被裁掉。
    let base =
        ToUnicodeMap::parse(b"3 beginbfchar <01> <0041> <0001> <0042> <0100> <0043> endbfchar")
            .unwrap();
    let source = b"/Base usecmap 1 begincodespacerange <0000> <00FF> endcodespacerange 1 beginbfchar <0002> <0058> endbfchar";
    let map = ToUnicodeMap::parse_lenient_with(source, move |_| Ok(base.clone()))
        .expect("本地映射应可用");
    assert_eq!(map.unicode_for(&[0x01]), None, "码长不在本地 codespace 内");
    assert_eq!(map.unicode_for(&[0x00, 0x01]), Some("B"));
    assert_eq!(
        map.unicode_for(&[0x01, 0x00]),
        None,
        "码值不在本地 codespace 内"
    );
    assert_eq!(map.unicode_for(&[0x00, 0x02]), Some("X"));
}

#[test]
fn tounicode_usecmap_inherits_all_mappings_without_local_codespace() {
    let base = ToUnicodeMap::parse(b"1 beginbfchar <01> <0041> endbfchar").unwrap();
    let map = ToUnicodeMap::parse_lenient_with(b"/Base usecmap", move |_| Ok(base.clone()))
        .expect("无本地映射时仍应可用");
    assert_eq!(
        map.unicode_for(&[0x01]),
        Some("A"),
        "无 codespace 时继承全部基座映射"
    );
}

#[test]
fn tounicode_usecmap_rejects_duplicate_and_malformed_bases() {
    let base = || Ok(ToUnicodeMap::parse(b"1 beginbfchar <01> <0041> endbfchar").unwrap());
    assert!(
        ToUnicodeMap::parse_lenient_with(
            b"/A usecmap /B usecmap 1 beginbfchar <01> <0041> endbfchar",
            |_| base(),
        )
        .is_none(),
        "重复 usecmap 应报错"
    );
    assert!(
        ToUnicodeMap::parse_lenient_with(
            b"Base usecmap 1 beginbfchar <01> <0041> endbfchar",
            |_| base(),
        )
        .is_none(),
        "缺少斜杠的基名应报错"
    );
}

#[test]
fn tounicode_lenient_parse_keeps_local_mappings_when_base_is_unknown() {
    let source = b"/Unknown usecmap 1 beginbfchar <01> <0041> endbfchar";
    let map = ToUnicodeMap::parse_lenient(source).expect("本地映射应可用");
    assert_eq!(map.unicode_for(&[0x01]), Some("A"));
}

#[test]
fn tounicode_identity_base_resolves_unmapped_two_byte_codes() {
    let source = b"/Adobe-Identity-UCS usecmap 1 begincodespacerange <0000> <FFFF> endcodespacerange 1 beginbfchar <0001> <0058> endbfchar";
    let map = ToUnicodeMap::parse_lenient(source).expect("内置恒等基座应可解析");
    assert_eq!(
        map.resolve_unicode(&[0x00, 0x01]).as_deref(),
        Some("X"),
        "本地映射应优先于恒等基座"
    );
    assert_eq!(
        map.resolve_unicode(&[0x00, 0x42]).as_deref(),
        Some("B"),
        "未映射的 2 字节码应按码值解析"
    );
    assert_eq!(
        map.resolve_unicode(&[0x01, 0x00]).as_deref(),
        Some("\u{100}")
    );
    assert_eq!(map.resolve_unicode(&[0x01]), None, "1 字节码不受影响");
    assert_eq!(
        map.unicode_for(&[0x00, 0x42]),
        None,
        "unicode_for 仅查本地映射"
    );
}

#[test]
fn tounicode_identity_base_respects_local_codespace() {
    let source =
        b"/Adobe-Identity-UCS usecmap 1 begincodespacerange <0000> <00FF> endcodespacerange";
    let map = ToUnicodeMap::parse_lenient(source).expect("仅有基座时应可用");
    assert_eq!(map.resolve_unicode(&[0x00, 0x42]).as_deref(), Some("B"));
    assert_eq!(
        map.resolve_unicode(&[0x01, 0x00]),
        None,
        "码值超出本地 codespace 时不解析"
    );
}

#[test]
fn tounicode_identity_base_skips_surrogates_and_other_lengths() {
    let map =
        ToUnicodeMap::parse_lenient(b"/Adobe-Identity-UCS usecmap").expect("无本地映射时仍可用");
    assert_eq!(map.resolve_unicode(&[0x00, 0x41]).as_deref(), Some("A"));
    assert_eq!(map.resolve_unicode(&[0xD8, 0x00]), None, "代理码无标量");
    assert_eq!(map.resolve_unicode(&[0x00]), None);
    assert_eq!(map.resolve_unicode(&[0x00, 0x41, 0x00]), None);
}
