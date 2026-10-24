//! Tests for bare CFF1 parsing, CID mapping and synthetic-table outlining.

use super::*;
use crate::core::reader::font::cff::fixtures;
use crate::core::reader::font::cff::outline::CffPathSegment;

fn name_keyed_font() -> RawCffFont {
    RawCffFont::parse(fixtures::name_keyed_cff()).unwrap()
}

#[test]
fn parses_name_keyed_program_metadata() {
    let font = name_keyed_font();
    assert!(!font.is_cid_keyed());
    assert_eq!(font.glyph_count(), 2);
    assert_eq!(font.units_per_em(), 2000);
    assert_eq!(font.font_matrix(), [0.0005, 0.0, 0.0, 0.0005, 0.0, 0.0]);
    assert_eq!(font.font_bbox(), Some([-10.0, -20.0, 500.0, 700.0]));
    assert_eq!(font.cid_count(), None);
    assert_eq!(font.ros(), None);
    assert_eq!(font.data(), fixtures::name_keyed_cff().as_slice());
}

#[test]
fn resolves_default_and_nominal_widths() {
    let font = name_keyed_font();
    // Glyph 0 has no width operand: defaultWidthX.
    assert_eq!(font.glyph_width(0), Some(500.0));
    // Glyph 1 carries 550: nominalWidthX (50) + 550.
    assert_eq!(font.glyph_width(1), Some(600.0));
    assert_eq!(font.glyph_width(2), None);
}

#[test]
fn outlines_name_keyed_glyphs() {
    let backend = RawCffBackend::parse(fixtures::name_keyed_cff()).unwrap();
    assert_eq!(backend.glyph_count(), 2);
    assert!(!backend.is_cid_keyed());
    assert_eq!(backend.glyph_for_cid(0), None);

    let empty = backend.parse_glyph(0).unwrap();
    assert_eq!(empty.width, 500.0);
    assert!(empty.segments.is_empty());

    let glyph = backend.parse_glyph(1).unwrap();
    assert_eq!(glyph.width, 600.0);
    assert_eq!(
        glyph.segments,
        vec![
            CffPathSegment::MoveTo { x: 100.0, y: 0.0 },
            CffPathSegment::LineTo { x: 300.0, y: 0.0 },
            CffPathSegment::ClosePath,
        ]
    );

    let error = backend.parse_glyph(2).unwrap_err();
    assert!(error.to_string().contains("out of range"));
}

#[test]
fn maps_cids_to_glyphs_for_cid_keyed_programs() {
    let font = RawCffFont::parse(fixtures::cid_keyed_cff()).unwrap();
    assert!(font.is_cid_keyed());
    assert_eq!(font.glyph_count(), 2);
    assert_eq!(font.ros(), Some([391, 392, 0]));
    assert_eq!(font.cid_count(), Some(100));
    assert_eq!(font.units_per_em(), 2000);
    assert_eq!(font.cid_for_glyph(0), Some(0));
    assert_eq!(font.cid_for_glyph(1), Some(7));
    assert_eq!(font.cid_for_glyph(2), None);
    assert_eq!(font.glyph_for_cid(0), Some(0));
    assert_eq!(font.glyph_for_cid(7), Some(1));
    assert_eq!(font.glyph_for_cid(1), None);
}

#[test]
fn cid_keyed_backend_uses_cid_arguments_and_font_dict_widths() {
    let backend = RawCffBackend::parse(fixtures::cid_keyed_cff()).unwrap();
    assert!(backend.is_cid_keyed());
    assert_eq!(backend.glyph_for_cid(7), Some(1));

    // CID 0 (GID 0) takes FD 0: defaultWidthX 300, empty outline.
    let notdef = backend.parse_glyph(0).unwrap();
    assert_eq!(notdef.width, 300.0);
    assert!(notdef.segments.is_empty());

    // CID 7 (GID 1) takes FD 1: nominalWidthX 20 + 480.
    let glyph = backend.parse_glyph(7).unwrap();
    assert_eq!(glyph.width, 500.0);
    assert_eq!(
        glyph.segments,
        vec![
            CffPathSegment::MoveTo { x: 100.0, y: 0.0 },
            CffPathSegment::LineTo { x: 300.0, y: 0.0 },
            CffPathSegment::ClosePath,
        ]
    );

    // CID 1 is not in the charset even though GID 1 exists.
    let error = backend.parse_glyph(1).unwrap_err();
    assert!(error.to_string().contains("CID 1"));
}

#[test]
fn cid_keyed_fd_select_format_3_selects_the_font_dict() {
    let backend = RawCffBackend::parse(fixtures::cid_keyed_cff_with_fd_select_format_3()).unwrap();
    assert_eq!(backend.parse_glyph(0).unwrap().width, 300.0);
    assert_eq!(backend.parse_glyph(7).unwrap().width, 500.0);
}

#[test]
fn rejects_truncated_and_malformed_programs() {
    let fixture = fixtures::name_keyed_cff();
    for length in [0, 1, 3, 4, 5, 10, 20, fixture.len() / 2] {
        assert!(
            RawCffFont::parse(fixture[..length].to_vec()).is_err(),
            "truncation to {length} bytes must fail"
        );
    }

    assert!(RawCffFont::parse(Vec::new()).is_err());
    assert!(RawCffFont::parse(vec![2, 0, 4, 1]).is_err());
    assert!(RawCffFont::parse(vec![1, 0, 3, 1]).is_err());
}

#[test]
fn rejects_cid_keyed_program_without_charset() {
    let error = RawCffFont::parse(fixtures::cid_keyed_cff_without_charset()).unwrap_err();
    assert!(error.to_string().contains("charset"), "{error}");
}

#[test]
fn rejects_cid_keyed_program_with_predefined_charset() {
    let error = RawCffFont::parse(fixtures::cid_keyed_cff_with_predefined_charset()).unwrap_err();
    assert!(error.to_string().contains("predefined charset"), "{error}");
}

#[test]
fn reports_unsupported_cff_version() {
    let error = RawCffFont::parse(vec![2, 0, 4, 1]).unwrap_err();
    assert!(error.to_string().contains("CFF major version 2"), "{error}");
}
