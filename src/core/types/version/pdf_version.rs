//! PDF 版本定义与解析。

use std::fmt;

use super::PdfFeature;

/// PDF 版本号（`%PDF-x.y` 头部声明）。
///
/// 覆盖 1.0–1.7 与 2.0。reader 用它记录源文档声明的版本（版本中立
/// 元数据）并据此补宽读说明；writer **不**读取源版本——输出特性一律
/// 由 writer 自身的目标版本决定。
///
/// **版本→特性**判断统一走 [`PdfVersion::supports`]（唯一事实源），
/// 各特性引入的最低版本由 [`PdfFeature::min_version`] 给出。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PdfVersion {
    pub major: u8,
    pub minor: u8,
}

impl PdfVersion {
    pub const V1_0: Self = Self { major: 1, minor: 0 };
    pub const V1_1: Self = Self { major: 1, minor: 1 };
    pub const V1_2: Self = Self { major: 1, minor: 2 };
    pub const V1_3: Self = Self { major: 1, minor: 3 };
    pub const V1_4: Self = Self { major: 1, minor: 4 };
    pub const V1_5: Self = Self { major: 1, minor: 5 };
    pub const V1_6: Self = Self { major: 1, minor: 6 };
    pub const V1_7: Self = Self { major: 1, minor: 7 };
    /// PDF 2.0（ISO 32000-2）。字段顺序派生 `Ord`，故 `V1_7 < V2_0` 成立。
    pub const V2_0: Self = Self { major: 2, minor: 0 };

    /// 从文件头字节解析 `%PDF-x.y`。找不到或格式不符返回 `None`。
    pub fn parse(header: &[u8]) -> Option<Self> {
        let head = header.iter().take(1024).copied().collect::<Vec<_>>();
        let s = String::from_utf8_lossy(&head);
        let marker = s.find("%PDF-")?;
        Self::from_name(&s[marker + 5..])
    }

    /// 从版本名字符串解析版本，形如 `1.5`、`2.0`。
    ///
    /// 用于文件头 `%PDF-x.y` 之后的部分，以及根 Catalog 字典的
    /// `/Version` 覆盖值（PDF 1.4+）。接受 `1.0–1.7` 与 `2.0`；
    /// 无法解析或超出范围（如 `2.1`、`3.0`）返回 `None`，由调用方
    /// 忽略该覆盖。
    pub fn from_name(name: &str) -> Option<Self> {
        let mut parts = name.split(|c: char| !c.is_ascii_digit());
        let major: u8 = parts.next()?.parse().ok()?;
        let minor: u8 = parts.next()?.parse().ok()?;
        if (major == 1 && minor <= 7) || (major == 2 && minor == 0) {
            Some(Self { major, minor })
        } else {
            None
        }
    }

    pub fn as_str(&self) -> String {
        format!("{}.{}", self.major, self.minor)
    }

    /// 版本是否支持某特性，即 `self >= feature.min_version()`。
    ///
    /// 这是版本→特性判断的**唯一事实源**：reader 的 xref 流/对象流
    /// 版本门控、writer 的输出特性校验，均应经此方法而非自行比较
    /// 版本常量。[`PdfVersion::supports_xref_streams`] 等为此的便捷
    /// 包装。
    pub fn supports(self, feature: PdfFeature) -> bool {
        self >= feature.min_version()
    }

    /// 是否支持交叉引用流（XRef stream，PDF 1.5+ 引入）。
    /// 便捷包装，等价于 `self.supports(PdfFeature::XRefStream)`。
    pub fn supports_xref_streams(self) -> bool {
        self.supports(PdfFeature::XRefStream)
    }

    /// 是否支持对象流（ObjStm，PDF 1.5+ 引入）。
    /// 便捷包装，等价于 `self.supports(PdfFeature::ObjectStream)`。
    pub fn supports_object_streams(self) -> bool {
        self.supports(PdfFeature::ObjectStream)
    }
}

impl Default for PdfVersion {
    fn default() -> Self {
        Self::V1_7
    }
}

impl fmt::Display for PdfVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
