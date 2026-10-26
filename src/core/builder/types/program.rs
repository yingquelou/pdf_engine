//! 嵌入的字体程序数据。

/// 嵌入的字体程序数据。
///
/// 对应 PDF 标准 5.8 节的字体程序嵌入，支持四种格式：
/// - Type 1（`/FontFile`）：PostScript Type 1 字体程序
/// - TrueType（`/FontFile2`）：TrueType 轮廓的 SFNT 字体程序
/// - CFF（`/FontFile3`）：Compact Font Format 字体程序
/// - OpenType（`/FontFile3` `/Subtype /OpenType` 或 `/FontFile2`）：
///   带 SFNT 目录的 OpenType 容器，按 SFNT 版本标签区分轮廓类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontProgram {
    /// Type 1 字体程序（PostScript Type 1，存储为 `/FontFile` 流）。
    Type1(Vec<u8>),
    /// TrueType 字体程序（SFNT 格式，存储为 `/FontFile2` 流）。
    TrueType(Vec<u8>),
    /// CFF 字体程序（Compact Font Format，存储为 `/FontFile3` 流）。
    CFF(Vec<u8>),
    /// OpenType 字体程序（SFNT 容器，版本标签决定轮廓类型）。
    ///
    /// CFF 轮廓（`OTTO`）写为 `/FontFile3` `/Subtype /OpenType`；
    /// TrueType 轮廓（`0x00010000` 或 `true`）按 ISO 32000-1 §9.9
    /// 写为 `/FontFile2`。
    OpenType(Vec<u8>),
}

/// OpenType 程序的 SFNT 轮廓类型，决定字体 `/Subtype` 与程序流键。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenTypeFlavor {
    /// `OTTO` 版本标签：CFF 轮廓。
    Cff,
    /// TrueType 轮廓（版本标签 `0x00010000` 或 `true`）。
    TrueType,
}

impl FontProgram {
    /// 返回字体程序的原始字节数据。
    pub(crate) fn data(&self) -> &[u8] {
        match self {
            FontProgram::Type1(data) => data,
            FontProgram::TrueType(data) => data,
            FontProgram::CFF(data) => data,
            FontProgram::OpenType(data) => data,
        }
    }

    /// 返回 OpenType 程序的 SFNT 轮廓类型；非 OpenType 程序或版本标签
    /// 不受支持时返回 `None`。
    ///
    /// 接受的版本标签与读端 SFNT 目录一致：`OTTO`、`0x00010000` 与
    /// `true`。写端只校验标签，不解析 SFNT 表目录。
    pub(crate) fn open_type_flavor(&self) -> Option<OpenTypeFlavor> {
        let FontProgram::OpenType(data) = self else {
            return None;
        };
        match data.get(..4)? {
            b"OTTO" => Some(OpenTypeFlavor::Cff),
            [0, 1, 0, 0] | b"true" => Some(OpenTypeFlavor::TrueType),
            _ => None,
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

    /// 同 [`subset_tag_with`](Self::subset_tag_with)，但把字体名一并纳入哈希。
    ///
    /// §9.6.4 要求同一文件内的子集标签唯一；同一程序字节以不同
    /// `/BaseFont` 名注册时也应得到不同标签，因此名称参与判别。
    pub(crate) fn subset_tag_for_name(&self, name: &str, discriminator: &[u8]) -> String {
        let mut prefixed = Vec::with_capacity(4 + name.len() + discriminator.len());
        prefixed.extend_from_slice(&(name.len() as u32).to_be_bytes());
        prefixed.extend_from_slice(name.as_bytes());
        prefixed.extend_from_slice(discriminator);
        self.subset_tag_with(&prefixed)
    }
}
