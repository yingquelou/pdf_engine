//! 注解标志位（12.5.2，Table 165）。
//!
//! `/F` 是 10 位标准掩码；未定义位原样保留在 [`AnnotationFlags::bits`]，
//! 由写端整数值直接回读。

/// 注解标志位（`/F`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnnotationFlags(u32);

impl AnnotationFlags {
    pub const INVISIBLE: u32 = 1 << 0;
    pub const HIDDEN: u32 = 1 << 1;
    pub const PRINT: u32 = 1 << 2;
    pub const NO_ZOOM: u32 = 1 << 3;
    pub const NO_ROTATE: u32 = 1 << 4;
    pub const NO_VIEW: u32 = 1 << 5;
    pub const READ_ONLY: u32 = 1 << 6;
    pub const LOCKED: u32 = 1 << 7;
    pub const TOGGLE_NO_VIEW: u32 = 1 << 8;
    pub const LOCKED_CONTENTS: u32 = 1 << 9;

    pub fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub fn bits(self) -> u32 {
        self.0
    }

    fn has(self, bit: u32) -> bool {
        self.0 & bit != 0
    }

    pub fn invisible(self) -> bool {
        self.has(Self::INVISIBLE)
    }

    pub fn hidden(self) -> bool {
        self.has(Self::HIDDEN)
    }

    pub fn print(self) -> bool {
        self.has(Self::PRINT)
    }

    pub fn no_zoom(self) -> bool {
        self.has(Self::NO_ZOOM)
    }

    pub fn no_rotate(self) -> bool {
        self.has(Self::NO_ROTATE)
    }

    pub fn no_view(self) -> bool {
        self.has(Self::NO_VIEW)
    }

    pub fn read_only(self) -> bool {
        self.has(Self::READ_ONLY)
    }

    pub fn locked(self) -> bool {
        self.has(Self::LOCKED)
    }

    pub fn toggle_no_view(self) -> bool {
        self.has(Self::TOGGLE_NO_VIEW)
    }

    pub fn locked_contents(self) -> bool {
        self.has(Self::LOCKED_CONTENTS)
    }
}
