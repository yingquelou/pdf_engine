//! PDF text strings, encoded using PDFDocEncoding or UTF-16BE.

use std::fmt;

/// An owned Unicode string represented as a PDF text string when encoded.
///
/// Decoding uses PDFDocEncoding unless the byte sequence starts with the
/// UTF-16BE byte-order mark (`FE FF`). Encoding uses PDFDocEncoding whenever
/// every character has a defined representation, and UTF-16BE with a BOM
/// otherwise.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct PdfTextString(String);

/// Errors encountered while decoding a PDF text string.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PdfTextStringError {
    /// The byte has no mapping in PDFDocEncoding.
    UndefinedPdfDocEncodingByte { byte: u8 },
    /// UTF-16BE data after the BOM had an odd number of bytes.
    InvalidUtf16ByteLength { length: usize },
    /// UTF-16BE contained an unmatched or malformed surrogate.
    InvalidUtf16Surrogate { byte_index: usize, unit: u16 },
}

impl PdfTextString {
    /// Creates a PDF text string from Unicode text.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Decodes PDFDocEncoding bytes, or UTF-16BE bytes when prefixed by `FE FF`.
    pub fn decode(bytes: &[u8]) -> Result<Self, PdfTextStringError> {
        if let Some(utf16_bytes) = bytes.strip_prefix(&[0xFE, 0xFF]) {
            Self::decode_utf16_be(utf16_bytes)
        } else {
            let mut value = String::with_capacity(bytes.len());
            for &byte in bytes {
                let character = PDF_DOC_ENCODING[usize::from(byte)]
                    .ok_or(PdfTextStringError::UndefinedPdfDocEncodingByte { byte })?;
                value.push(character);
            }
            Ok(Self(value))
        }
    }

    /// Encodes using PDFDocEncoding when possible, otherwise UTF-16BE with BOM.
    pub fn encode(&self) -> Vec<u8> {
        let mut pdf_doc_bytes = Vec::with_capacity(self.0.len());
        for character in self.0.chars() {
            if let Some(byte) = PDF_DOC_ENCODING
                .iter()
                .position(|entry| *entry == Some(character))
            {
                pdf_doc_bytes.push(byte as u8);
            } else {
                return self.encode_utf16_be();
            }
        }
        if pdf_doc_bytes.starts_with(&[0xFE, 0xFF]) {
            self.encode_utf16_be()
        } else {
            pdf_doc_bytes
        }
    }

    /// Returns the Unicode text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the PDF text string and returns its Unicode text.
    pub fn into_string(self) -> String {
        self.0
    }

    fn decode_utf16_be(bytes: &[u8]) -> Result<Self, PdfTextStringError> {
        if bytes.len() % 2 != 0 {
            return Err(PdfTextStringError::InvalidUtf16ByteLength {
                length: bytes.len(),
            });
        }

        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        let mut value = String::with_capacity(units.len());
        let mut index = 0;
        while index < units.len() {
            let unit = units[index];
            match unit {
                0xD800..=0xDBFF => {
                    let Some(&low) = units.get(index + 1) else {
                        return Err(PdfTextStringError::InvalidUtf16Surrogate {
                            byte_index: 2 + index * 2,
                            unit,
                        });
                    };
                    if !(0xDC00..=0xDFFF).contains(&low) {
                        return Err(PdfTextStringError::InvalidUtf16Surrogate {
                            byte_index: 2 + (index + 1) * 2,
                            unit: low,
                        });
                    }
                    let scalar =
                        0x10000 + ((u32::from(unit) - 0xD800) << 10) + (u32::from(low) - 0xDC00);
                    value.push(char::from_u32(scalar).expect("valid UTF-16 surrogate pair"));
                    index += 2;
                }
                0xDC00..=0xDFFF => {
                    return Err(PdfTextStringError::InvalidUtf16Surrogate {
                        byte_index: 2 + index * 2,
                        unit,
                    });
                }
                _ => {
                    value.push(char::from_u32(u32::from(unit)).expect("non-surrogate UTF-16 unit"));
                    index += 1;
                }
            }
        }
        Ok(Self(value))
    }

    fn encode_utf16_be(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(2 + self.0.len() * 2);
        bytes.extend_from_slice(&[0xFE, 0xFF]);
        for unit in self.0.encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        bytes
    }
}

impl From<String> for PdfTextString {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for PdfTextString {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl AsRef<str> for PdfTextString {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for PdfTextStringError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UndefinedPdfDocEncodingByte { byte } => {
                write!(formatter, "undefined PDFDocEncoding byte 0x{byte:02X}")
            }
            Self::InvalidUtf16ByteLength { length } => {
                write!(formatter, "UTF-16BE data has odd byte length {length}")
            }
            Self::InvalidUtf16Surrogate { byte_index, unit } => write!(
                formatter,
                "invalid UTF-16BE surrogate 0x{unit:04X} at byte index {byte_index}"
            ),
        }
    }
}

impl std::error::Error for PdfTextStringError {}

const fn pdf_doc_encoding() -> [Option<char>; 256] {
    let mut table = [None; 256];
    let mut byte = 0;
    while byte <= 0x17 {
        table[byte] = Some(byte as u8 as char);
        byte += 1;
    }
    table[0x18] = Some('\u{02D8}');
    table[0x19] = Some('\u{02C7}');
    table[0x1A] = Some('\u{02C6}');
    table[0x1B] = Some('\u{02D9}');
    table[0x1C] = Some('\u{02DD}');
    table[0x1D] = Some('\u{02DB}');
    table[0x1E] = Some('\u{02DA}');
    table[0x1F] = Some('\u{02DC}');
    byte = 0x20;
    while byte <= 0x7E {
        table[byte] = Some(byte as u8 as char);
        byte += 1;
    }
    table[0x80] = Some('\u{2022}');
    table[0x81] = Some('\u{2020}');
    table[0x82] = Some('\u{2021}');
    table[0x83] = Some('\u{2026}');
    table[0x84] = Some('\u{2014}');
    table[0x85] = Some('\u{2013}');
    table[0x86] = Some('\u{0192}');
    table[0x87] = Some('\u{2044}');
    table[0x88] = Some('\u{2039}');
    table[0x89] = Some('\u{203A}');
    table[0x8A] = Some('\u{2212}');
    table[0x8B] = Some('\u{2030}');
    table[0x8C] = Some('\u{201E}');
    table[0x8D] = Some('\u{201C}');
    table[0x8E] = Some('\u{201D}');
    table[0x8F] = Some('\u{2018}');
    table[0x90] = Some('\u{2019}');
    table[0x91] = Some('\u{201A}');
    table[0x92] = Some('\u{2122}');
    table[0x93] = Some('\u{FB01}');
    table[0x94] = Some('\u{FB02}');
    table[0x95] = Some('\u{0141}');
    table[0x96] = Some('\u{0152}');
    table[0x97] = Some('\u{0160}');
    table[0x98] = Some('\u{0178}');
    table[0x99] = Some('\u{017D}');
    table[0x9A] = Some('\u{0131}');
    table[0x9B] = Some('\u{0142}');
    table[0x9C] = Some('\u{0153}');
    table[0x9D] = Some('\u{0161}');
    table[0x9E] = Some('\u{017E}');
    table[0xA0] = Some('\u{20AC}');
    byte = 0xA1;
    while byte <= 0xFF {
        table[byte] = Some(byte as u8 as char);
        byte += 1;
    }
    table
}

/// Undefined entries at 0x7F and 0x9F intentionally remain `None`.
const PDF_DOC_ENCODING: [Option<char>; 256] = pdf_doc_encoding();
