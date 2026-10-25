//! CFF 子集化单元测试：保留语义、重编号、结构重写与往返解析。

mod type1c;

use super::super::RawCffFont;
use super::super::raw::parse::{parse_index, parse_top_dict};
use super::subset_cff;
use crate::core::reader::font::cff::fixtures;
use crate::core::reader::font::cff::outline::CffPathSegment;

/// 子集化并用 `raw` 解析层重新解析输出（往返校验）。
fn subset_and_reparse(data: &[u8], cids: &[u16]) -> RawCffFont {
    let subset = subset_cff(data, cids).expect("subset should succeed");
    RawCffFont::parse(subset).expect("subset output should re-parse")
}

/// 解析输出的 FDArray，返回 Font DICT 条目数。
fn fd_array_len(font_bytes: &[u8]) -> usize {
    let header_size = usize::from(font_bytes[2]);
    let name = parse_index(font_bytes, header_size).unwrap();
    let top_index = parse_index(font_bytes, name.end).unwrap();
    let top_dict = parse_top_dict(&font_bytes[top_index.entries[0].clone()]).unwrap();
    let fd_array = parse_index(font_bytes, top_dict.fd_array.unwrap()).unwrap();
    fd_array.entries.len()
}

#[test]
fn subset_keeps_only_notdef_for_empty_request() {
    let font = subset_and_reparse(&fixtures::cid_keyed_cff_multi(), &[]);
    assert!(font.is_cid_keyed());
    assert_eq!(font.glyph_count(), 1);
    assert_eq!(font.cid_for_glyph(0), Some(0));
    assert_eq!(font.glyph_for_cid(0), Some(0));
    // GID 0 用 FD 0 的 defaultWidthX 300；只剩一个 Font DICT。
    assert_eq!(font.glyph_width(0), Some(300.0));
}

#[test]
fn subset_retains_requested_cids_and_renumbers_gids() {
    // 乱序 + 重复请求；新 GID 按保留 CID 升序重编号。
    let font = subset_and_reparse(&fixtures::cid_keyed_cff_multi(), &[42, 7, 7]);
    assert_eq!(font.glyph_count(), 3);
    assert_eq!(font.cid_for_glyph(0), Some(0));
    assert_eq!(font.cid_for_glyph(1), Some(7));
    assert_eq!(font.cid_for_glyph(2), Some(42));
    assert_eq!(font.glyph_for_cid(7), Some(1));
    assert_eq!(font.glyph_for_cid(9), None);
    assert_eq!(font.glyph_for_cid(20), None);
    assert_eq!(font.glyph_for_cid(42), Some(2));
    // 宽度随 FD 重排保持正确：CID 7 → FD 1（480 + 20），CID 42 → FD 2（300 + 30）。
    assert_eq!(font.glyph_width(1), Some(500.0));
    assert_eq!(font.glyph_width(2), Some(330.0));
}

#[test]
fn subset_always_keeps_cid_zero() {
    let font = subset_and_reparse(&fixtures::cid_keyed_cff(), &[7]);
    assert_eq!(font.glyph_count(), 2);
    assert_eq!(font.cid_for_glyph(0), Some(0));
    assert_eq!(font.cid_for_glyph(1), Some(7));
}

#[test]
fn subset_drops_unreferenced_font_dicts() {
    // {7} 只用 FD 0/1，FD 2（含 Local Subrs）应被裁掉。
    let subset = subset_cff(&fixtures::cid_keyed_cff_multi(), &[7]).unwrap();
    assert_eq!(fd_array_len(&subset), 2, "未引用的 FD 2 应被裁掉");
    let font = RawCffFont::parse(subset).unwrap();
    assert_eq!(font.glyph_width(1), Some(500.0));
}

#[test]
fn subset_preserves_subroutines_and_outlines() {
    // 保留 GID 2（callgsubr）与 GID 4（callsubr，FD 2 的 Local Subr）。
    let font = subset_and_reparse(&fixtures::cid_keyed_cff_multi(), &[9, 42]);
    let backend_glyph = |cid: u16| {
        let gid = font.glyph_for_cid(cid).unwrap();
        font.outline(gid).unwrap()
    };
    let via_gsubr = backend_glyph(9);
    assert_eq!(via_gsubr.width, 500.0);
    assert_eq!(
        via_gsubr.segments,
        vec![
            CffPathSegment::MoveTo { x: 100.0, y: 0.0 },
            CffPathSegment::LineTo { x: 150.0, y: 0.0 },
            CffPathSegment::LineTo { x: 350.0, y: 0.0 },
            CffPathSegment::ClosePath,
        ],
        "Global Subr 应保留且编号不变"
    );
    let via_lsubr = backend_glyph(42);
    assert_eq!(via_lsubr.width, 330.0);
    assert_eq!(
        via_lsubr.segments,
        vec![
            CffPathSegment::MoveTo { x: 0.0, y: 0.0 },
            CffPathSegment::LineTo { x: 25.0, y: 0.0 },
            CffPathSegment::LineTo { x: 75.0, y: 50.0 },
            CffPathSegment::ClosePath,
        ],
        "Local Subr 应随保留的 FD 保留且相对偏移已重写"
    );
}

#[test]
fn subset_output_is_smaller() {
    let original = fixtures::cid_keyed_cff_multi();
    let subset = subset_cff(&original, &[7]).unwrap();
    assert!(
        subset.len() < original.len(),
        "子集应更小: {} >= {}",
        subset.len(),
        original.len()
    );
}

#[test]
fn subset_rejects_unknown_cid() {
    let error = subset_cff(&fixtures::cid_keyed_cff_multi(), &[8]).unwrap_err();
    assert!(error.to_string().contains("CID 8"), "{error}");
    assert!(error.to_string().contains("not present"), "{error}");
}

#[test]
fn subset_rejects_out_of_range_cid() {
    // CIDCount 为 100；CID 200 越界（即使它也不可能出现在 charset 中）。
    let error = subset_cff(&fixtures::cid_keyed_cff_multi(), &[200]).unwrap_err();
    assert!(error.to_string().contains("out of range"), "{error}");
}

#[test]
fn subset_rejects_non_cid_keyed_programs() {
    let error = subset_cff(&fixtures::name_keyed_cff(), &[1]).unwrap_err();
    assert!(error.to_string().contains("not CID-keyed"), "{error}");
}

#[test]
fn subset_rejects_cid_keyed_program_with_predefined_charset() {
    let error = subset_cff(&fixtures::cid_keyed_cff_with_predefined_charset(), &[7]).unwrap_err();
    assert!(error.to_string().contains("predefined charset"), "{error}");
}

#[test]
fn subset_rejects_cid_keyed_program_without_charset() {
    let error = subset_cff(&fixtures::cid_keyed_cff_without_charset(), &[7]).unwrap_err();
    assert!(error.to_string().contains("charset"), "{error}");
}

#[test]
fn subset_does_not_panic_on_malformed_input() {
    let fixture = fixtures::cid_keyed_cff_multi();
    for length in [0, 1, 3, 4, 5, 10, 20, fixture.len() / 2, fixture.len() - 1] {
        assert!(
            subset_cff(&fixture[..length], &[7]).is_err(),
            "截断到 {length} 字节必须报错而非 panic"
        );
    }
    assert!(subset_cff(&[], &[]).is_err());
    assert!(subset_cff(&[2, 0, 4, 1], &[0]).is_err());
}

#[test]
fn subset_handles_fd_select_format_0_source() {
    // 既有两字形式夹具的 FDSelect 是 format 0；输出统一重写为 format 3。
    let font = subset_and_reparse(&fixtures::cid_keyed_cff(), &[7]);
    assert_eq!(font.glyph_count(), 2);
    assert_eq!(font.glyph_width(0), Some(300.0));
    assert_eq!(font.glyph_width(1), Some(500.0));
    assert_eq!(font.glyph_for_cid(7), Some(1));
}

#[test]
fn subset_keeps_full_program_metadata() {
    let font = subset_and_reparse(&fixtures::cid_keyed_cff_multi(), &[7]);
    assert_eq!(font.ros(), Some([391, 392, 0]));
    assert_eq!(font.cid_count(), Some(100), "CIDCount 逐字节保留");
    assert_eq!(font.units_per_em(), 2000);
}
