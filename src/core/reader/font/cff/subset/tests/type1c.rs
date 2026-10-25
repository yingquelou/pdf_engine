//! name-keyed（`/Type1C`）CFF 子集化单元测试。

use super::super::super::RawCffFont;
use super::super::super::raw::parse::{parse_charset, parse_index, parse_top_dict};
use super::super::OP_ENCODING;
use super::super::dict::walk_dict;
use super::super::subset_cff_type1c;
use crate::core::reader::font::FontError;
use crate::core::reader::font::cff::fixtures;
use crate::core::reader::font::cff::outline::CffPathSegment;

/// 子集化并用 `raw` 解析层重新解析输出（往返校验）。
fn subset_font(data: &[u8], glyph_names: &[&str]) -> RawCffFont {
    let subset = subset_cff_type1c(data, glyph_names).expect("subset should succeed");
    RawCffFont::parse(subset).expect("subset output should re-parse")
}

/// 读取输出程序的显式 charset（GID → SID）。
fn charset_sids(font: &RawCffFont) -> Vec<u16> {
    let bytes = font.data();
    let header = usize::from(bytes[2]);
    let name = parse_index(bytes, header).unwrap();
    let top_index = parse_index(bytes, name.end).unwrap();
    let top_dict = parse_top_dict(&bytes[top_index.entries[0].clone()]).unwrap();
    let offset = top_dict.charset.expect("subset output must have charset");
    parse_charset(bytes, offset, font.glyph_count()).unwrap()
}

/// 输出 Top DICT 是否仍保留 `Encoding` 条目。
fn top_dict_has_encoding(font: &RawCffFont) -> bool {
    let bytes = font.data();
    let header = usize::from(bytes[2]);
    let name = parse_index(bytes, header).unwrap();
    let top_index = parse_index(bytes, name.end).unwrap();
    let entries = walk_dict(&bytes[top_index.entries[0].clone()]).unwrap();
    entries.iter().any(|entry| entry.operator == OP_ENCODING)
}

#[test]
fn type1c_subset_keeps_notdef_for_empty_request() {
    let font = subset_font(&fixtures::name_keyed_cff_custom_charset(0), &[]);
    assert!(!font.is_cid_keyed());
    assert_eq!(font.glyph_count(), 1);
    assert_eq!(charset_sids(&font), vec![0], "GID 0 应为 SID 0（.notdef）");
    assert_eq!(font.glyph_width(0), Some(500.0));
}

#[test]
fn type1c_subset_retains_named_glyphs_in_source_gid_order() {
    // 乱序 + 重复请求；新 GID 按原 GID 升序重编号。
    let font = subset_font(
        &fixtures::name_keyed_cff_custom_charset(0),
        &["Gamma", "Alpha", "Alpha"],
    );
    assert_eq!(font.glyph_count(), 3);
    assert_eq!(charset_sids(&font), vec![0, 391, 393]);
    // Alpha 原 GID 1 宽度 550 + 50，Gamma 原 GID 3 宽度 570 + 50。
    assert_eq!(font.glyph_width(1), Some(600.0));
    assert_eq!(font.glyph_width(2), Some(620.0));
    assert_eq!(
        font.outline(1).unwrap().segments,
        vec![
            CffPathSegment::MoveTo { x: 100.0, y: 0.0 },
            CffPathSegment::LineTo { x: 300.0, y: 0.0 },
            CffPathSegment::ClosePath,
        ],
        "保留字形的 charstring 应原样搬移"
    );
}

#[test]
fn type1c_subset_rewrites_charset_to_format_zero() {
    let subset = subset_cff_type1c(&fixtures::name_keyed_cff_custom_charset(0), &["Beta"]).unwrap();
    let font = RawCffFont::parse(subset.clone()).unwrap();
    let header = usize::from(subset[2]);
    let name = parse_index(&subset, header).unwrap();
    let top_index = parse_index(&subset, name.end).unwrap();
    let top_dict = parse_top_dict(&subset[top_index.entries[0].clone()]).unwrap();
    let offset = top_dict.charset.unwrap();
    assert_eq!(subset[offset], 0, "charset 必须是显式 format 0");
    // GID 0 不写字形表；两个 GID 各 2 字节 SID。
    assert_eq!(charset_sids(&font), vec![0, 392]);
}

#[test]
fn type1c_subset_output_reparses_with_raw_layer() {
    let font = subset_font(&fixtures::name_keyed_cff_custom_charset(0), &["Beta"]);
    assert_eq!(font.glyph_count(), 2);
    assert_eq!(font.units_per_em(), 2000);
    assert_eq!(font.font_bbox(), Some([-10.0, -20.0, 500.0, 700.0]));
    assert_eq!(font.glyph_width(1), Some(610.0));
}

#[test]
fn type1c_subset_output_is_smaller() {
    let original = fixtures::name_keyed_cff_with_encoding();
    let subset = subset_cff_type1c(&original, &["Beta"]).unwrap();
    assert!(
        subset.len() < original.len(),
        "子集输出应小于源程序（{} vs {}）",
        subset.len(),
        original.len()
    );
}

#[test]
fn type1c_subset_drops_top_dict_encoding() {
    let font = subset_font(&fixtures::name_keyed_cff_with_encoding(), &["Beta"]);
    assert!(!top_dict_has_encoding(&font), "/Encoding 条目应被删除");
    assert_eq!(charset_sids(&font), vec![0, 392]);
}

#[test]
fn type1c_subset_handles_explicit_charset_format_zero() {
    assert_explicit_charset_format(0);
}

#[test]
fn type1c_subset_handles_explicit_charset_format_one() {
    assert_explicit_charset_format(1);
}

#[test]
fn type1c_subset_handles_explicit_charset_format_two() {
    assert_explicit_charset_format(2);
}

fn assert_explicit_charset_format(format: u8) {
    let font = subset_font(&fixtures::name_keyed_cff_custom_charset(format), &["Beta"]);
    assert_eq!(font.glyph_count(), 2, "charset format {format}");
    assert_eq!(charset_sids(&font), vec![0, 392], "charset format {format}");
}

#[test]
fn type1c_subset_handles_predefined_iso_adobe_charset() {
    let font = subset_font(&fixtures::name_keyed_cff(), &["space"]);
    assert_eq!(font.glyph_count(), 2);
    assert_eq!(charset_sids(&font), vec![0, 1], "GID 1 的 SID 1 为 space");
    assert_eq!(font.glyph_width(1), Some(600.0));
}

#[test]
fn type1c_subset_handles_predefined_expert_charset() {
    // Expert charset：GID 1 → SID 1（space），GID 2 → SID 229（exclamsmall）。
    let font = subset_font(
        &fixtures::name_keyed_cff_predefined_charset(1, 3),
        &["exclamsmall"],
    );
    assert_eq!(charset_sids(&font), vec![0, 229]);
    assert_eq!(font.glyph_width(1), Some(620.0));
}

#[test]
fn type1c_subset_handles_predefined_expert_subset_charset() {
    // ExpertSubset charset：GID 2 → SID 231（dollaroldstyle）。
    let font = subset_font(
        &fixtures::name_keyed_cff_predefined_charset(2, 3),
        &["dollaroldstyle"],
    );
    assert_eq!(charset_sids(&font), vec![0, 231]);
}

#[test]
fn type1c_subset_defaults_missing_charset_to_iso_adobe() {
    let font = subset_font(&fixtures::name_keyed_cff_without_charset(), &["space"]);
    assert_eq!(font.glyph_count(), 2);
    assert_eq!(charset_sids(&font), vec![0, 1]);
}

#[test]
fn type1c_subset_preserves_local_subrs() {
    let font = subset_font(&fixtures::name_keyed_cff_with_local_subrs(), &["Gamma"]);
    assert_eq!(font.glyph_count(), 2);
    assert_eq!(charset_sids(&font), vec![0, 393]);
    assert_eq!(font.glyph_width(1), Some(620.0));
    assert_eq!(
        font.outline(1).unwrap().segments,
        vec![
            CffPathSegment::MoveTo { x: 0.0, y: 0.0 },
            CffPathSegment::LineTo { x: 25.0, y: 0.0 },
            CffPathSegment::LineTo { x: 75.0, y: 50.0 },
            CffPathSegment::ClosePath,
        ],
        "Local Subr 应整组保留且相对偏移已重写"
    );
}

#[test]
fn type1c_subset_rejects_unknown_glyph_name() {
    match subset_cff_type1c(
        &fixtures::name_keyed_cff_custom_charset(0),
        &["NoSuchGlyph"],
    ) {
        Err(FontError::Invalid(message)) => {
            assert!(
                message.contains("NoSuchGlyph"),
                "错误应包含字形名：{message}"
            );
        }
        Err(other) => panic!("expected Invalid, got {other}"),
        Ok(_) => panic!("unknown glyph name should fail"),
    }
}

#[test]
fn type1c_subset_rejects_cid_keyed_program() {
    match subset_cff_type1c(&fixtures::cid_keyed_cff(), &["space"]) {
        Err(FontError::Invalid(message)) => {
            assert!(
                message.contains("CID-keyed") && message.contains("subset_cff"),
                "应提示改用 subset_cff：{message}"
            );
        }
        Err(other) => panic!("expected Invalid, got {other}"),
        Ok(_) => panic!("CID-keyed program should be rejected"),
    }
}

#[test]
fn type1c_subset_does_not_panic_on_malformed_input() {
    let data = fixtures::name_keyed_cff_custom_charset(0);
    for len in 0..data.len() {
        let truncated = &data[..len];
        let _ = subset_cff_type1c(truncated, &["Beta"]);
        let _ = subset_cff_type1c(truncated, &[]);
    }
    // 头部之后的偏移字段被破坏时同样只返回错误。
    let mut corrupted = data.clone();
    corrupted[2] = 0; // hdrSize 非法
    let _ = subset_cff_type1c(&corrupted, &["Beta"]);
    assert!(subset_cff_type1c(&corrupted, &[]).is_err());
}
