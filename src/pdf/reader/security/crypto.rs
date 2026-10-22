use aes::Aes128;
use aes::cipher::{BlockDecrypt, KeyInit, generic_array::GenericArray};
use md5::{Digest, Md5};
use rc4::{
    Rc4, StreamCipher,
    consts::{U1, U2, U3, U4, U5, U6, U7, U8, U9, U10, U11, U12, U13, U14, U15, U16},
};
use zeroize::Zeroizing;

pub(crate) trait CryptoBackend: Send + Sync {
    fn md5(&self, input: &[u8]) -> [u8; 16];
    fn rc4(&self, key: &[u8], input: &[u8]) -> Result<Vec<u8>, String>;
    fn aes128_cbc_decrypt(&self, key: &[u8], iv: &[u8], input: &[u8]) -> Result<Vec<u8>, String>;
}

pub(crate) struct RustCryptoBackend;

impl CryptoBackend for RustCryptoBackend {
    fn md5(&self, input: &[u8]) -> [u8; 16] {
        Md5::digest(input).into()
    }

    fn rc4(&self, key: &[u8], input: &[u8]) -> Result<Vec<u8>, String> {
        macro_rules! apply {
            ($key_size:ty) => {{
                let mut output = Zeroizing::new(input.to_vec());
                let mut cipher = Rc4::<$key_size>::new_from_slice(key)
                    .map_err(|_| "invalid RC4 key length".to_string())?;
                cipher.apply_keystream(&mut output);
                Ok(std::mem::take(&mut *output))
            }};
        }
        match key.len() {
            1 => apply!(U1),
            2 => apply!(U2),
            3 => apply!(U3),
            4 => apply!(U4),
            5 => apply!(U5),
            6 => apply!(U6),
            7 => apply!(U7),
            8 => apply!(U8),
            9 => apply!(U9),
            10 => apply!(U10),
            11 => apply!(U11),
            12 => apply!(U12),
            13 => apply!(U13),
            14 => apply!(U14),
            15 => apply!(U15),
            16 => apply!(U16),
            _ => Err("invalid RC4 key length".to_string()),
        }
    }

    fn aes128_cbc_decrypt(&self, key: &[u8], iv: &[u8], input: &[u8]) -> Result<Vec<u8>, String> {
        if key.len() != 16 || iv.len() != 16 || input.is_empty() || input.len() % 16 != 0 {
            return Err("invalid AES-128-CBC key, IV, or ciphertext length".to_string());
        }

        let cipher =
            Aes128::new_from_slice(key).map_err(|_| "invalid AES-128 key length".to_string())?;
        let mut output = Zeroizing::new(input.to_vec());
        let mut previous = *<&[u8; 16]>::try_from(iv).expect("validated IV length");
        for chunk in output.chunks_mut(16) {
            let ciphertext: [u8; 16] = chunk.try_into().expect("exact block size");
            let mut block = GenericArray::clone_from_slice(chunk);
            cipher.decrypt_block(&mut block);
            for (dst, (plain, prior)) in chunk.iter_mut().zip(block.iter().zip(previous)) {
                *dst = plain ^ prior;
            }
            previous = ciphertext;
        }
        Ok(std::mem::take(&mut *output))
    }
}

#[cfg(test)]
mod tests {
    use super::{CryptoBackend, RustCryptoBackend};

    #[test]
    fn md5_matches_the_standard_empty_input_vector() {
        assert_eq!(
            RustCryptoBackend.md5(b""),
            [
                0xd4, 0x1d, 0x8c, 0xd9, 0x8f, 0x00, 0xb2, 0x04, 0xe9, 0x80, 0x09, 0x98, 0xec, 0xf8,
                0x42, 0x7e,
            ]
        );
    }

    #[test]
    fn rc4_matches_the_standard_key_plaintext_vector() {
        assert_eq!(
            RustCryptoBackend.rc4(b"Key", b"Plaintext").unwrap(),
            [0xbb, 0xf3, 0x16, 0xe8, 0xd9, 0x40, 0xaf, 0x0a, 0xd3]
        );
    }

    #[test]
    fn aes128_cbc_matches_a_nist_decryption_vector() {
        let key = decode_hex("2b7e151628aed2a6abf7158809cf4f3c");
        let iv = decode_hex("000102030405060708090a0b0c0d0e0f");
        let ciphertext = decode_hex("7649abac8119b246cee98e9b12e9197d");
        let plaintext = decode_hex("6bc1bee22e409f96e93d7e117393172a");
        assert_eq!(
            RustCryptoBackend
                .aes128_cbc_decrypt(&key, &iv, &ciphertext)
                .unwrap(),
            plaintext
        );
    }

    fn decode_hex(value: &str) -> Vec<u8> {
        value
            .as_bytes()
            .chunks(2)
            .map(|pair| {
                let high = (pair[0] as char).to_digit(16).unwrap();
                let low = (pair[1] as char).to_digit(16).unwrap();
                ((high << 4) | low) as u8
            })
            .collect()
    }
}
