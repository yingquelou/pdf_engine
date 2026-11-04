//! 外部文件流读端的**非致命**诊断形态（本模块由 `mod.rs` 的体量红线拆出，
//! 采集逻辑仍在 `mod.rs` 的 `collect_problems`）。
//!
//! 规范依据（`pdftotext -raw docs/PDF32000_2008.pdf` 实测行号）：
//! - `VersionGate`：Table 5 三条目皆「(Optional; PDF 1.2)」——raw L1095、
//!   L1102、L1106。
//! - `EncryptedExternalData`：7.6.1「…refers to an external file, the
//!   stream's contents **shall not be encrypted**」raw L2743–L2745。
//! - `InlineBytesIgnored`／`LengthMismatch`：Table 5 `/F`「the bytes between
//!   `stream` and `endstream` **shall be ignored**. However, the `Length`
//!   entry **should** still specify the number of those bytes…」raw
//!   L1096–L1099。

use std::fmt;

use crate::core::types::PdfVersion;

/// Table 5 的三个外部文件流条目（raw L1095/L1102/L1106，均「Optional; PDF 1.2」）。
pub(super) const PDF_12_ENTRIES: [&str; 3] = ["F", "FFilter", "FDecodeParms"];
/// 记录范围之后探测 `endstream` 的窗口上限（库政策）。
pub(super) const ENDSTREAM_PROBE_WINDOW: usize = 512;
pub(super) const ENDSTREAM_KEYWORD: &[u8] = b"endstream";

/// 外部文件流读端的**非致命**诊断：取值全部保留，只把不一致报出来。
#[derive(Debug, Clone, PartialEq)]
pub enum ExternalFileStreamProblem {
    /// Table 5 L1095/L1102/L1106 的「PDF 1.2」门禁：文档声明版本更低。
    VersionGate {
        /// 出现的条目名（不带前导 `/`）。
        entries: Vec<&'static str>,
        /// 文档声明的版本。
        declared: PdfVersion,
    },
    /// 7.6.1 L2743–L2745 的 shall：引用外部文件的流其内容不得加密。
    EncryptedExternalData {
        /// 违例的过滤器名。
        filter: String,
    },
    /// Table 5 L1096–L1097：`/F` 在，`stream`…`endstream` 间的字节一律忽略。
    InlineBytesIgnored {
        /// 已忽略的内联字节数。
        bytes: u64,
    },
    /// Table 5 L1097–L1099 的 should：`/Length` 仍应给出那些字节的个数。
    LengthMismatch {
        /// `/Length` 声明值。
        declared: i64,
        /// 观察到的内联字节数。
        actual: u64,
    },
}

impl ExternalFileStreamProblem {
    /// 是否为提示／should 级（非规范 shall 违例）。
    pub fn is_advisory(&self) -> bool {
        matches!(
            self,
            Self::InlineBytesIgnored { .. } | Self::LengthMismatch { .. }
        )
    }
}

impl fmt::Display for ExternalFileStreamProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VersionGate { entries, declared } => write!(
                f,
                "{} used in a document declaring PDF {declared}, but Table 5 marks these entries \
                 PDF 1.2 (raw L1095/L1102/L1106)",
                entries
                    .iter()
                    .map(|entry| format!("/{entry}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::EncryptedExternalData { filter } => write!(
                f,
                "/FFilter applies {filter} to external file data, but the contents of a stream \
                 that refers to an external file shall not be encrypted (7.6.1, raw \
                 L2743-L2745)"
            ),
            Self::InlineBytesIgnored { bytes } => write!(
                f,
                "/F is present with {bytes} bytes between stream and endstream; those bytes are \
                 ignored (Table 5, raw L1096-L1097)"
            ),
            Self::LengthMismatch { declared, actual } => write!(
                f,
                "/Length is {declared} but {actual} bytes lie between stream and endstream; with \
                 /F present Length should still specify that count (Table 5, raw L1097-L1099)"
            ),
        }
    }
}

impl std::error::Error for ExternalFileStreamProblem {}
