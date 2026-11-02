//! 解析后的 Type 1 程序模型、解析入口与 `/FontFile` 三段长度切分
//! （Table 127）。

use std::collections::BTreeMap;

use crate::core::types::{Document, Value};

use super::super::{FontError, FontResult, embedded_program::FontProgramSource};

use super::charstring::{Type1CharString, interpret};
use super::crypto::{decrypt, eexec_section};
use super::error::invalid_type1;
use super::header::{Type1Header, parse_header};
use super::private_dict::parse_private;
use super::scanner::Scanner;

/// A parsed Type 1 font program.
///
/// `/CharStrings` and `/Subrs` are decrypted according to the Private
/// dictionary's `/lenIV`. [`Type1FontProgram::glyph`] interprets an individual
/// charstring into widths and absolute path segments.
#[derive(Debug, Clone, PartialEq)]
pub struct Type1FontProgram {
    /// Metric metadata from the cleartext header.
    pub header: Type1Header,
    /// The Private dictionary's `/lenIV`; `-1` means charstrings are not
    /// encrypted.
    pub len_iv: i32,
    /// Decrypted charstring bytes keyed by glyph name.
    pub charstrings: BTreeMap<String, Vec<u8>>,
    /// Decrypted subroutine bytes keyed by subroutine index.
    pub subrs: BTreeMap<u32, Vec<u8>>,
}

impl Type1FontProgram {
    /// Parse a decoded Type 1 font program.
    ///
    /// This understands the cleartext header, the `eexec`-encrypted Private
    /// dictionary, and the encrypted `/CharStrings` and `/Subrs` string
    /// segments. It does not execute PostScript or parse CFF and OpenType data
    /// (which use `/FontFile3`).
    pub fn parse(data: &[u8]) -> FontResult<Self> {
        let mut scanner = Scanner::new(data);
        let header = parse_header(&mut scanner)?;
        scanner.skip_single_gap();
        let encrypted = eexec_section(&data[scanner.position..]);
        if encrypted.len() < 4 {
            return Err(invalid_type1("eexec section is truncated"));
        }
        let plaintext = decrypt(encrypted, 55665, 4);
        let private = parse_private(&plaintext)?;
        Ok(Self {
            header,
            len_iv: private.len_iv,
            charstrings: private.charstrings,
            subrs: private.subrs,
        })
    }

    /// Interpret a glyph charstring into its width, side bearing, and path.
    ///
    /// Returns `Ok(None)` when the program has no charstring for the glyph.
    /// `seac` composites are flattened into their base and accent outlines by
    /// resolving the component character codes through StandardEncoding, and
    /// flex `callothersubr`/`pop` sequences are flattened into curve segments.
    /// Hint replacement and other `callothersubr` procedures are rejected.
    pub fn glyph(&self, glyph_name: &str) -> FontResult<Option<Type1CharString>> {
        let Some(charstring) = self.charstrings.get(glyph_name) else {
            return Ok(None);
        };
        interpret(charstring, &self.charstrings, &self.subrs, glyph_name).map(Some)
    }
}

impl Document {
    /// Resolve and parse the embedded Type 1 font program in a font dictionary.
    ///
    /// Returns `Ok(None)` when the font has no embedded program. The program
    /// must be stored as `/FontFile`: `/FontFile3` streams (Type 1C CFF or
    /// OpenType) are not Type 1 programs and are reported as errors.
    pub fn type1_program(&self, value: &Value) -> FontResult<Option<Type1FontProgram>> {
        let Some(program) = self.type1_font_program(value)? else {
            return Ok(None);
        };
        if program.source != FontProgramSource::FontFile {
            return Err(FontError::Invalid(
                "embedded Type 1 font programs must use /FontFile; /FontFile3 is not parsed as Type 1"
                    .to_string(),
            ));
        }
        Type1FontProgram::parse(&program.data).map(Some)
    }
}

/// Split a Type 1 program into the `/FontFile` length entries required by
/// Table 127: clear-text header, `eexec` encrypted section, and trailing
/// fixed content. Programs without a locatable `eexec` section are reported
/// entirely as clear text.
pub(crate) fn font_file_lengths(data: &[u8]) -> (usize, usize, usize) {
    let mut scanner = Scanner::new(data);
    if parse_header(&mut scanner).is_err() {
        return (data.len(), 0, 0);
    }
    scanner.skip_single_gap();
    let length1 = scanner.position.min(data.len());
    let length2 = eexec_section(&data[length1..]).len();
    let length3 = data.len() - length1 - length2;
    (length1, length2, length3)
}
