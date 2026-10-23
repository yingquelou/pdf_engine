//! Semantic representation of PDF date strings.

use std::fmt;
use std::str::FromStr;

/// A PDF date's relationship to Universal Time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfDateTimezone {
    /// No timezone was specified; the time is local.
    Local,
    /// The time is explicitly UTC (`Z`).
    Utc,
    /// A signed offset from UTC.
    Offset {
        /// `true` for `+`, `false` for `-`.
        positive: bool,
        hours: u8,
        minutes: u8,
    },
}

/// A PDF date (`D:YYYYMMDDHHmmSSOHH'mm'`).
///
/// Components after the year are optional, but must appear in order. For
/// example, a date containing a day must also contain a month. The timezone
/// defaults to [`PdfDateTimezone::Local`] when omitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfDate {
    year: u16,
    month: Option<u8>,
    day: Option<u8>,
    hour: Option<u8>,
    minute: Option<u8>,
    second: Option<u8>,
    timezone: PdfDateTimezone,
}

/// An error encountered while parsing a PDF date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfDateError {
    InvalidFormat,
    InvalidMonth,
    InvalidDay,
    InvalidHour,
    InvalidMinute,
    InvalidSecond,
    InvalidTimezone,
}

impl PdfDate {
    /// Parse a PDF date string, accepting omitted trailing date/time fields.
    pub fn parse(input: &str) -> Result<Self, PdfDateError> {
        let input = input
            .strip_prefix("D:")
            .ok_or(PdfDateError::InvalidFormat)?;
        let bytes = input.as_bytes();
        if bytes.len() < 4 || !bytes[..4].iter().all(u8::is_ascii_digit) {
            return Err(PdfDateError::InvalidFormat);
        }

        let year = read_number(&bytes[..4])? as u16;
        let mut pos = 4;
        let mut fields = [None; 5];
        for (index, field) in fields.iter_mut().enumerate() {
            if pos == bytes.len() || matches!(bytes[pos], b'+' | b'-' | b'Z') {
                break;
            }
            if pos + 2 > bytes.len() || !bytes[pos..pos + 2].iter().all(u8::is_ascii_digit) {
                return Err(PdfDateError::InvalidFormat);
            }
            *field = Some(read_number(&bytes[pos..pos + 2])? as u8);
            pos += 2;
            let error = match index {
                0 => PdfDateError::InvalidMonth,
                1 => PdfDateError::InvalidDay,
                2 => PdfDateError::InvalidHour,
                3 => PdfDateError::InvalidMinute,
                _ => PdfDateError::InvalidSecond,
            };
            let value = field.expect("field was just populated");
            let max = match index {
                0 => 12,
                2 => 23,
                3 | 4 => 59,
                _ => u8::MAX,
            };
            if value == 0 || value > max {
                return Err(error);
            }
        }

        let [month, day, hour, minute, second] = fields;
        if let (Some(month), Some(day)) = (month, day)
            && day > days_in_month(year, month)
        {
            return Err(PdfDateError::InvalidDay);
        }

        let timezone = if pos == bytes.len() {
            PdfDateTimezone::Local
        } else {
            match bytes[pos] {
                b'Z' if pos + 1 == bytes.len() => PdfDateTimezone::Utc,
                b'+' | b'-' => {
                    let positive = bytes[pos] == b'+';
                    let rest = &bytes[pos + 1..];
                    if rest.len() != 6
                        || !rest[..2].iter().all(u8::is_ascii_digit)
                        || rest[2] != b'\''
                        || !rest[3..5].iter().all(u8::is_ascii_digit)
                        || rest[5] != b'\''
                    {
                        return Err(PdfDateError::InvalidTimezone);
                    }
                    let hours = read_number(&rest[..2])? as u8;
                    let minutes = read_number(&rest[3..5])? as u8;
                    if hours > 23 || minutes > 59 {
                        return Err(PdfDateError::InvalidTimezone);
                    }
                    PdfDateTimezone::Offset {
                        positive,
                        hours,
                        minutes,
                    }
                }
                _ => return Err(PdfDateError::InvalidFormat),
            }
        };

        Ok(Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
            timezone,
        })
    }

    pub fn year(&self) -> u16 {
        self.year
    }

    pub fn month(&self) -> Option<u8> {
        self.month
    }

    pub fn day(&self) -> Option<u8> {
        self.day
    }

    pub fn hour(&self) -> Option<u8> {
        self.hour
    }

    pub fn minute(&self) -> Option<u8> {
        self.minute
    }

    pub fn second(&self) -> Option<u8> {
        self.second
    }

    pub fn timezone(&self) -> PdfDateTimezone {
        self.timezone
    }

    /// Serialize this value in canonical PDF date-string form.
    pub fn to_pdf_string(&self) -> String {
        let mut output = format!("D:{:04}", self.year);
        for value in [self.month, self.day, self.hour, self.minute, self.second]
            .into_iter()
            .flatten()
        {
            use std::fmt::Write;
            write!(output, "{value:02}").expect("writing to a String cannot fail");
        }
        match self.timezone {
            PdfDateTimezone::Local => {}
            PdfDateTimezone::Utc => output.push('Z'),
            PdfDateTimezone::Offset {
                positive,
                hours,
                minutes,
            } => {
                use std::fmt::Write;
                let sign = if positive { '+' } else { '-' };
                write!(output, "{sign}{hours:02}'{minutes:02}'")
                    .expect("writing to a String cannot fail");
            }
        }
        output
    }
}

impl FromStr for PdfDate {
    type Err = PdfDateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl fmt::Display for PdfDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_pdf_string())
    }
}

impl fmt::Display for PdfDateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidFormat => "invalid PDF date format",
            Self::InvalidMonth => "month is out of range",
            Self::InvalidDay => "day is invalid for the specified month",
            Self::InvalidHour => "hour is out of range",
            Self::InvalidMinute => "minute is out of range",
            Self::InvalidSecond => "second is out of range",
            Self::InvalidTimezone => "invalid PDF date timezone",
        };
        f.write_str(message)
    }
}

impl std::error::Error for PdfDateError {}

fn read_number(bytes: &[u8]) -> Result<u32, PdfDateError> {
    bytes.iter().try_fold(0_u32, |value, byte| {
        if byte.is_ascii_digit() {
            Ok(value * 10 + u32::from(byte - b'0'))
        } else {
            Err(PdfDateError::InvalidFormat)
        }
    })
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}
