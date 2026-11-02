//! Type 1 加密与 `eexec` 段解密：CharString/Subrs 段（含 `/lenIV` 处理）与
//! eexec 数据段的边界与解密。

use super::super::FontResult;
use super::error::invalid_type1;

pub(super) fn decrypt_segment(ciphertext: &[u8], len_iv: i32) -> FontResult<Vec<u8>> {
    if len_iv < 0 {
        return Ok(ciphertext.to_vec());
    }
    let skip = usize::try_from(len_iv).expect("non-negative /lenIV");
    if ciphertext.len() < skip {
        return Err(invalid_type1("CharString or Subr is shorter than /lenIV"));
    }
    Ok(decrypt(ciphertext, 4330, skip))
}

/// Type 1 encryption: the high byte of the running key XORs the plaintext.
pub(super) fn decrypt(ciphertext: &[u8], key: u16, skip: usize) -> Vec<u8> {
    let mut running = key;
    let mut plaintext = Vec::with_capacity(ciphertext.len().saturating_sub(skip));
    for (index, &cipher) in ciphertext.iter().enumerate() {
        let plain = cipher ^ (running >> 8) as u8;
        running = u16::from(cipher)
            .wrapping_add(running)
            .wrapping_mul(52845)
            .wrapping_add(22719);
        if index >= skip {
            plaintext.push(plain);
        }
    }
    plaintext
}

/// The eexec data ends at the 512-zero guard that precedes `cleartomark`.
pub(super) fn eexec_section(data: &[u8]) -> &[u8] {
    let mut run = 0usize;
    for (index, &byte) in data.iter().enumerate() {
        if byte == b'0' {
            run += 1;
            if run == 512 {
                return &data[..index + 1 - 512];
            }
        } else {
            run = 0;
        }
    }
    data
}
