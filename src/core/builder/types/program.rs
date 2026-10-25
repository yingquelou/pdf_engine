//! 嵌入的字体程序数据。

/// 嵌入的字体程序数据。
///
/// 对应 PDF 标准 5.8 节的字体程序嵌入，支持三种格式：
/// - Type 1（`/FontFile`）：PostScript Type 1 字体程序
/// - TrueType（`/FontFile2`）：TrueType 轮廓的 SFNT 字体程序
/// - CFF（`/FontFile3`）：Compact Font Format 字体程序
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontProgram {
    /// Type 1 字体程序（PostScript Type 1，存储为 `/FontFile` 流）。
    Type1(Vec<u8>),
    /// TrueType 字体程序（SFNT 格式，存储为 `/FontFile2` 流）。
    TrueType(Vec<u8>),
    /// CFF 字体程序（Compact Font Format，存储为 `/FontFile3` 流）。
    CFF(Vec<u8>),
}

impl FontProgram {
    /// 返回字体程序的原始字节数据。
    pub(crate) fn data(&self) -> &[u8] {
        match self {
            FontProgram::Type1(data) => data,
            FontProgram::TrueType(data) => data,
            FontProgram::CFF(data) => data,
        }
    }

    /// 按字体程序内容与子集判别字节生成确定性的六字母子集标签
    /// （PDF 32000-1 §9.6.4）。
    ///
    /// 使用 FNV-1a 64 位哈希按 26 进制展开为 6 个大写字母，同一程序
    /// 与同一判别字节总是得到同一标签，便于重复构建与测试。
    ///
    /// `discriminator` 用于区分同一字体程序的不同子集请求（例如保留的
    /// gid 或字形名列表），否则不同子集会共享同一 `/BaseFont` 标签，
    /// 违反 §9.6.4 对同文件内标签唯一性的要求。程序字节与判别字节之间
    /// 写入长度前缀，避免两者边界产生哈希碰撞。
    pub(crate) fn subset_tag_with(&self, discriminator: &[u8]) -> String {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        let mut mix = |bytes: &[u8]| {
            for &byte in bytes {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        mix(self.data());
        if !discriminator.is_empty() {
            mix(&(discriminator.len() as u64).to_be_bytes());
            mix(discriminator);
        }
        let mut tag = [b'A'; 6];
        let mut value = hash;
        for slot in tag.iter_mut().rev() {
            *slot = b'A' + (value % 26) as u8;
            value /= 26;
        }
        String::from_utf8(tag.to_vec()).expect("subset tag is ASCII")
    }
}
