//! Content stream parsing and operator AST.
//!
//! Parses PDF content streams into a sequence of operators with their operands,
//! supporting roundtrip serialization of both modeled and unmodeled operators.

use crate::core::types::Value;

mod mapping;
mod marked;
mod parser;
mod state;

#[cfg(test)]
mod marked_tests;

#[cfg(test)]
mod mapping_tests;

#[cfg(test)]
mod operand_guard_tests;

#[cfg(test)]
mod state_tests;

#[cfg(test)]
mod tests;

pub use mapping::{OpMappingError, from_content_op, serialize_ops, to_content_op};
pub use marked::{
    MAX_MARKED_DEPTH, MarkedContentError, MarkedSection, collect_marked_sections,
    resolve_marked_visibility,
};
pub use parser::{ContentStreamError, parse_content_stream};

pub(crate) use marked::text_value;
pub(crate) use parser::parse_dictionary_inner;
pub use state::{
    ColorValue, ContentGraphicsState, ContentStateError, ContentStateRunner, IDENTITY_MATRIX,
    MAX_SAVE_DEPTH, concat_matrices, run_content_state,
};

/// A single content stream operator with its operands.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    // ── Graphics state operators (8.4) ──────────────────────
    /// `q` — save graphics state
    SaveState,
    /// `Q` — restore graphics state
    RestoreState,
    /// `cm` — concatenate matrix to CTM
    ConcatMatrix([f64; 6]),
    /// `w` — set line width
    SetLineWidth(f64),
    /// `J` — set line cap style
    SetLineCap(i64),
    /// `j` — set line join style
    SetLineJoin(i64),
    /// `M` — set miter limit
    SetMiterLimit(f64),
    /// `d` — set line dash pattern
    SetDashPattern(Vec<f64>, f64),
    /// `ri` — set rendering intent
    SetRenderingIntent(String),
    /// `i` — set flatness tolerance
    SetFlatness(f64),
    /// `gs` — set graphics state from dictionary
    SetExtGState(String),

    // ── Path construction operators (8.5) ───────────────────
    /// `m` — move to
    MoveTo(f64, f64),
    /// `l` — line to
    LineTo(f64, f64),
    /// `c` — cubic Bézier curve (initial point specified)
    CurveTo(f64, f64, f64, f64, f64, f64),
    /// `v` — cubic Bézier curve (initial point replicated)
    CurveToReplicateInitial(f64, f64, f64, f64),
    /// `y` — cubic Bézier curve (final point replicated)
    CurveToReplicateFinal(f64, f64, f64, f64),
    /// `h` — close subpath
    CloseSubpath,
    /// `re` — append rectangle
    Rectangle(f64, f64, f64, f64),

    // ── Path painting operators (8.5) ───────────────────────
    /// `S` — stroke path
    Stroke,
    /// `s` — close and stroke path
    CloseAndStroke,
    /// `f` — fill path (nonzero winding number)
    Fill,
    /// `F` — fill path (nonzero winding number, obsolete)
    FillObsolete,
    /// `f*` — fill path (even-odd rule)
    FillEvenOdd,
    /// `B` — fill and stroke (nonzero winding number)
    FillAndStroke,
    /// `B*` — fill and stroke (even-odd rule)
    FillAndStrokeEvenOdd,
    /// `b` — close, fill, and stroke (nonzero winding number)
    CloseFillAndStroke,
    /// `b*` — close, fill, and stroke (even-odd rule)
    CloseFillAndStrokeEvenOdd,
    /// `n` — end path (no paint)
    EndPath,

    // ── Clipping path operators (8.5.4) ─────────────────────
    /// `W` — set clipping path (nonzero winding number)
    Clip,
    /// `W*` — set clipping path (even-odd rule)
    ClipEvenOdd,

    // ── Text object operators (9.4) ─────────────────────────
    /// `BT` — begin text object
    BeginText,
    /// `ET` — end text object
    EndText,

    // ── Text state operators (9.3) ──────────────────────────
    /// `Tc` — set character spacing
    SetCharSpacing(f64),
    /// `Tw` — set word spacing
    SetWordSpacing(f64),
    /// `Tz` — set horizontal scaling
    SetHorizontalScaling(f64),
    /// `TL` — set text leading
    SetTextLeading(f64),
    /// `Tf` — set text font and size
    SetFont(String, f64),
    /// `Tr` — set text rendering mode
    SetTextRenderingMode(i64),
    /// `Ts` — set text rise
    SetTextRise(f64),

    // ── Text positioning operators (9.4) ────────────────────
    /// `Td` — move text position
    MoveTextPosition(f64, f64),
    /// `TD` — move text position and set leading
    MoveTextPositionAndSetLeading(f64, f64),
    /// `Tm` — set text matrix
    SetTextMatrix([f64; 6]),
    /// `T*` — move to start of next line
    NextLine,

    // ── Text showing operators (9.4) ────────────────────────
    /// `Tj` — show text string
    ShowText(Vec<u8>),
    /// `'` — move to next line and show text
    NextLineAndShowText(Vec<u8>),
    /// `"` — set word/character spacing, move to next line, and show text
    SetSpacingAndShowText(f64, f64, Vec<u8>),
    /// `TJ` — show text with individual glyph positioning
    ShowTextWithPositioning(Vec<TextElement>),

    // ── Type 3 font operators (9.6.5) ───────────────────────
    /// `d0` — set glyph width in Type 3 font
    SetGlyphWidth(f64, f64),
    /// `d1` — set glyph width and bounding box in Type 3 font
    SetGlyphWidthAndBBox(f64, f64, f64, f64, f64, f64),

    // ── Color operators (8.6.8) ─────────────────────────────
    /// `CS` — set stroke color space
    SetStrokeColorSpace(String),
    /// `cs` — set fill color space
    SetFillColorSpace(String),
    /// `SC` — set stroke color (device-specific)
    SetStrokeColor(Vec<f64>),
    /// `SCN` — set stroke color (pattern or device-specific)
    SetStrokeColorN(Vec<f64>),
    /// `sc` — set fill color (device-specific)
    SetFillColor(Vec<f64>),
    /// `scn` — set fill color (device-specific components)
    SetFillColorN(Vec<f64>),
    /// `SCN` with a trailing pattern name — set stroke pattern colour
    SetStrokeColorPattern(String),
    /// `SCN` with components and a trailing pattern name
    SetStrokeColorPatternWithComponents(Vec<f64>, String),
    /// `scn` with a trailing pattern name — set fill pattern colour
    SetFillColorPattern(String),
    /// `scn` with components and a trailing pattern name
    SetFillColorPatternWithComponents(Vec<f64>, String),
    /// `G` — set stroke gray
    SetStrokeGray(f64),
    /// `g` — set fill gray
    SetFillGray(f64),
    /// `RG` — set stroke RGB
    SetStrokeRgb(f64, f64, f64),
    /// `rg` — set fill RGB
    SetFillRgb(f64, f64, f64),
    /// `K` — set stroke CMYK
    SetStrokeCmyk(f64, f64, f64, f64),
    /// `k` — set fill CMYK
    SetFillCmyk(f64, f64, f64, f64),

    // ── Shading operator (8.7.4) ────────────────────────────
    /// `sh` — paint shape/colour shading
    PaintShading(String),

    // ── Inline image operators (8.9.7) ──────────────────────
    /// `BI` ... `ID` ... `EI` — inline image
    InlineImage(InlineImage),

    // ── XObject operators (8.8) ─────────────────────────────
    /// `Do` — paint XObject
    PaintXObject(String),

    // ── Marked content operators (14.6) ─────────────────────
    /// `MP` — marked content point
    MarkedContentPoint(String),
    /// `DP` — marked content point with properties
    MarkedContentPointWithProperties(String, Value),
    /// `BMC` — begin marked content
    BeginMarkedContent(String),
    /// `BDC` — begin marked content with properties
    BeginMarkedContentWithProperties(String, Value),
    /// `EMC` — end marked content
    EndMarkedContent,

    // ── Compatibility operators (8.4.3) ─────────────────────
    /// `BX` — begin compatibility section
    BeginCompatibility,
    /// `EX` — end compatibility section
    EndCompatibility,

    // ── Unmodeled operators ─────────────────────────────────
    /// Operator not yet modeled; preserved for roundtrip
    Unmodeled {
        operator: String,
        operands: Vec<Value>,
    },
}

/// An element in a `TJ` text-showing array.
#[derive(Debug, Clone, PartialEq)]
pub enum TextElement {
    /// A text string to show.
    String(Vec<u8>),
    /// A positioning adjustment (negative moves right, positive moves left).
    Adjustment(f64),
}

/// An inline image (BI/ID/EI sequence).
#[derive(Debug, Clone, PartialEq)]
pub struct InlineImage {
    /// Image dictionary entries (keys are names, values are the entry values).
    pub dict: Vec<(String, Value)>,
    /// Raw image data bytes (between ID and EI).
    pub data: Vec<u8>,
}
