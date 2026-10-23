use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::core::types::ObjectSecurity;
use crate::core::types::{Dict, Object, StreamData, Value};

use super::crypto::{CryptoBackend, RustCryptoBackend};

const PASSWORD_PADDING: [u8; 32] = [
    0x28, 0xbf, 0x4e, 0x5e, 0x4e, 0x75, 0x8a, 0x41, 0x64, 0x00, 0x4e, 0x56, 0xff, 0xfa, 0x01, 0x08,
    0x2e, 0x2e, 0x00, 0xb6, 0xd0, 0x68, 0x3e, 0x80, 0x2f, 0x0c, 0xa9, 0xfe, 0x64, 0x53, 0x69, 0x7a,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CryptMethod {
    Identity,
    Rc4 { key_len: usize },
    Aes128,
}

struct SecurityOptions {
    pub revision: u8,
    pub version: u8,
    pub file_key: Zeroizing<Vec<u8>>,
    pub string_method: CryptMethod,
    pub stream_method: CryptMethod,
    pub embedded_file_method: CryptMethod,
    pub encrypt_metadata: bool,
    pub encrypt_dict_ref: Option<(i64, i64)>,
    crypt_filters: Option<Dict>,
}

pub(crate) struct SecurityHandler {
    options: SecurityOptions,
}

impl std::fmt::Debug for SecurityHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecurityHandler")
            .field("revision", &self.options.revision)
            .field("version", &self.options.version)
            .field("file_key", &"[REDACTED]")
            .finish()
    }
}

impl SecurityHandler {
    pub(crate) fn authenticate(
        encrypt: &Dict,
        file_id: &Value,
        password: &[u8],
        encrypt_dict_ref: Option<(i64, i64)>,
        crypto: &dyn CryptoBackend,
    ) -> Result<Self, String> {
        let options = SecurityOptions::from_dict(encrypt, file_id, password, crypto)?;
        Ok(Self {
            options: SecurityOptions {
                encrypt_dict_ref,
                ..options
            },
        })
    }

    #[cfg(test)]
    fn file_key(&self) -> &[u8] {
        &self.options.file_key
    }
}

impl SecurityOptions {
    fn from_dict(
        encrypt: &Dict,
        file_id: &Value,
        password: &[u8],
        crypto: &dyn CryptoBackend,
    ) -> Result<Self, String> {
        if encrypt.get("Filter").and_then(Value::as_name) != Some("Standard") {
            return Err("unsupported PDF security handler".to_string());
        }

        let version = required_integer(encrypt, "V")?;
        let revision = required_integer(encrypt, "R")?;
        let version = u8::try_from(version).map_err(|_| "invalid encryption /V".to_string())?;
        let revision = u8::try_from(revision).map_err(|_| "invalid encryption /R".to_string())?;
        if !matches!((version, revision), (1, 2) | (2, 3) | (4, 4)) {
            return Err(format!(
                "unsupported Standard Security Handler version/revision ({version}/{revision})"
            ));
        }

        let permissions = required_integer(encrypt, "P")?;
        let permissions =
            i32::try_from(permissions).map_err(|_| "invalid encryption permissions".to_string())?;
        let owner = pdf_string(encrypt.get("O").ok_or("missing encryption /O")?)?;
        let user = pdf_string(encrypt.get("U").ok_or("missing encryption /U")?)?;
        if owner.len() != 32 || user.len() != 32 {
            return Err("invalid encryption /O or /U string length".to_string());
        }
        let id_array = file_id
            .as_array()
            .ok_or_else(|| "missing trailer /ID array".to_string())?;
        let id = id_array
            .iter()
            .next()
            .ok_or_else(|| "empty trailer /ID array".to_string())
            .and_then(pdf_string)?;

        let key_len = match revision {
            2 => {
                if let Some(value) = encrypt.get("Length")
                    && value.as_integer() != Some(40)
                {
                    return Err("revision 2 requires a 40-bit encryption key".to_string());
                }
                5
            }
            3 | 4 => {
                let bits = match encrypt.get("Length") {
                    Some(Value::Integer(bits)) => *bits,
                    None => 40,
                    Some(_) => return Err("invalid encryption /Length".to_string()),
                };
                if !(40..=128).contains(&bits) || bits % 8 != 0 {
                    return Err("invalid encryption key length".to_string());
                }
                usize::try_from(bits / 8)
                    .map_err(|_| "invalid encryption key length".to_string())?
            }
            _ => unreachable!(),
        };

        let encrypt_metadata = match encrypt.get("EncryptMetadata") {
            Some(Value::Bool(value)) if revision == 4 => *value,
            Some(Value::Bool(false)) => {
                return Err("/EncryptMetadata is only valid for revision 4".to_string());
            }
            Some(Value::Bool(true)) | None => true,
            Some(_) => return Err("invalid encryption /EncryptMetadata".to_string()),
        };
        let padded_password = pad_password(password);
        let file_key = derive_file_key(
            FileKeyParams {
                padded_password: &padded_password,
                owner_entry: &owner,
                permissions,
                id: &id,
                revision,
                encrypt_metadata,
                key_len,
            },
            crypto,
        );

        let user_valid = validate_user_password(&file_key, &user, &id, revision, crypto)?;
        if !user_valid {
            let owner_key = derive_owner_key(&padded_password, revision, key_len, crypto);
            let recovered = recover_user_password(&owner_key, &owner, revision, crypto)?;
            let owner_file_key = derive_file_key(
                FileKeyParams {
                    padded_password: &recovered,
                    owner_entry: &owner,
                    permissions,
                    id: &id,
                    revision,
                    encrypt_metadata,
                    key_len,
                },
                crypto,
            );
            if !validate_user_password(&owner_file_key, &user, &id, revision, crypto)? {
                return Err("invalid password for encrypted PDF".to_string());
            }
            return Self::with_filters(
                encrypt,
                version,
                revision,
                owner_file_key,
                encrypt_metadata,
                crypto,
            );
        }

        Self::with_filters(
            encrypt,
            version,
            revision,
            file_key,
            encrypt_metadata,
            crypto,
        )
    }

    fn with_filters(
        encrypt: &Dict,
        version: u8,
        revision: u8,
        file_key: Zeroizing<Vec<u8>>,
        encrypt_metadata: bool,
        _crypto: &dyn CryptoBackend,
    ) -> Result<Self, String> {
        let default_method = if version < 4 {
            CryptMethod::Rc4 {
                key_len: file_key.len(),
            }
        } else {
            CryptMethod::Identity
        };
        let file_key_len = file_key.len();
        let string_method = if version < 4 {
            default_method
        } else {
            crypt_filter(encrypt, "StrF", default_method, file_key_len)?
        };
        let stream_method = if version < 4 {
            default_method
        } else {
            crypt_filter(encrypt, "StmF", default_method, file_key_len)?
        };
        let embedded_file_method = if version < 4 {
            default_method
        } else {
            crypt_filter(encrypt, "EFF", stream_method, file_key_len)?
        };
        if [string_method, stream_method, embedded_file_method].contains(&CryptMethod::Aes128)
            && file_key_len != 16
        {
            return Err("AESV2 requires a 128-bit file encryption key".to_string());
        }

        Ok(Self {
            revision,
            version,
            file_key,
            string_method,
            stream_method,
            embedded_file_method,
            encrypt_metadata,
            encrypt_dict_ref: None,
            crypt_filters: encrypt.get("CF").and_then(Value::as_dict).cloned(),
        })
    }
}

impl ObjectSecurity for SecurityHandler {
    fn decrypt_object(&self, object: &mut Object) -> Result<(), String> {
        if self.options.encrypt_dict_ref == Some((object.num, object.generation)) {
            return Ok(());
        }
        decrypt_value(
            &mut object.value,
            object.num,
            object.generation,
            false,
            &self.options,
            &RustCryptoBackend,
        )?;
        Ok(())
    }

    fn decrypt_stream(
        &self,
        object_num: i64,
        generation: i64,
        dict: &Dict,
        bytes: &[u8],
    ) -> Result<Vec<u8>, String> {
        if dict.get("Type").and_then(Value::as_name) == Some("XRef")
            || (!self.options.encrypt_metadata
                && dict.get("Type").and_then(Value::as_name) == Some("Metadata"))
        {
            return Ok(bytes.to_vec());
        }

        let method = explicit_crypt_filter(dict, &self.options)?.unwrap_or_else(|| {
            match dict.get("Type").and_then(Value::as_name) {
                Some("EmbeddedFile") => self.options.embedded_file_method,
                _ => self.options.stream_method,
            }
        });
        decrypt_bytes(
            &self.options.file_key,
            object_num,
            generation,
            method,
            bytes,
            &RustCryptoBackend,
        )
    }
}

fn decrypt_value(
    value: &mut Value,
    object_num: i64,
    generation: i64,
    signature_contents: bool,
    options: &SecurityOptions,
    crypto: &dyn CryptoBackend,
) -> Result<(), String> {
    match value {
        Value::String(text) if !signature_contents => {
            let encrypted: Vec<u8> = text.chars().map(|c| c as u8).collect();
            let plain = decrypt_bytes(
                &options.file_key,
                object_num,
                generation,
                options.string_method,
                &encrypted,
                crypto,
            )?;
            *text = plain.into_iter().map(char::from).collect();
        }
        Value::HexString(hex) if !signature_contents => {
            let encrypted = decode_hex(hex)?;
            let plain = decrypt_bytes(
                &options.file_key,
                object_num,
                generation,
                options.string_method,
                &encrypted,
                crypto,
            )?;
            *hex = encode_hex(&plain);
        }
        Value::Array(array) => {
            for item in &mut array.0 {
                decrypt_value(item, object_num, generation, false, options, crypto)?;
            }
        }
        Value::Dict(dict) => {
            let is_signature = dict.get("Type").and_then(Value::as_name) == Some("Sig")
                || dict.get("FT").and_then(Value::as_name) == Some("Sig");
            for (key, item) in &mut dict.0 {
                decrypt_value(
                    item,
                    object_num,
                    generation,
                    is_signature && key == "Contents",
                    options,
                    crypto,
                )?;
            }
        }
        Value::Stream(dict, data) => {
            let is_signature = dict.get("Type").and_then(Value::as_name) == Some("Sig")
                || dict.get("FT").and_then(Value::as_name) == Some("Sig");
            for (key, item) in &mut dict.0 {
                decrypt_value(
                    item,
                    object_num,
                    generation,
                    is_signature && key == "Contents",
                    options,
                    crypto,
                )?;
            }
            if dict.get("Type").and_then(Value::as_name) != Some("XRef")
                && (options.encrypt_metadata
                    || dict.get("Type").and_then(Value::as_name) != Some("Metadata"))
                && let StreamData::Ref { offset, len } = data
            {
                *data = StreamData::EncryptedRef {
                    offset: *offset,
                    len: *len,
                    object_num,
                    generation,
                };
            }
        }
        _ => {}
    }
    Ok(())
}

fn decrypt_bytes(
    file_key: &[u8],
    object_num: i64,
    generation: i64,
    method: CryptMethod,
    encrypted: &[u8],
    crypto: &dyn CryptoBackend,
) -> Result<Vec<u8>, String> {
    match method {
        CryptMethod::Identity => Ok(encrypted.to_vec()),
        CryptMethod::Rc4 { .. } | CryptMethod::Aes128 => {
            let mut input = Zeroizing::new(Vec::with_capacity(file_key.len() + 9));
            input.extend_from_slice(file_key);
            let object = u32::try_from(object_num)
                .map_err(|_| "invalid encrypted object number".to_string())?;
            let generation = u16::try_from(generation)
                .map_err(|_| "invalid encrypted object generation".to_string())?;
            input.extend_from_slice(&object.to_le_bytes()[..3]);
            input.extend_from_slice(&generation.to_le_bytes());
            if method == CryptMethod::Aes128 {
                input.extend_from_slice(b"sAlT");
            }
            let digest = crypto.md5(&input);
            let file_key_len = match method {
                CryptMethod::Rc4 { key_len } => key_len,
                CryptMethod::Aes128 => file_key.len(),
                CryptMethod::Identity => unreachable!(),
            };
            let key_len = (file_key_len + 5).min(16);
            let object_key = Zeroizing::new(digest[..key_len].to_vec());
            match method {
                CryptMethod::Rc4 { .. } => crypto.rc4(&object_key, encrypted),
                CryptMethod::Aes128 => {
                    if encrypted.len() < 32 || (encrypted.len() - 16) % 16 != 0 {
                        return Err("invalid AES-encrypted PDF string or stream".to_string());
                    }
                    let mut plain = Zeroizing::new(crypto.aes128_cbc_decrypt(
                        &object_key,
                        &encrypted[..16],
                        &encrypted[16..],
                    )?);
                    let padding = *plain.last().ok_or("empty AES plaintext")? as usize;
                    if padding == 0 || padding > 16 || plain.len() < padding {
                        return Err("invalid AES-encrypted PDF padding".to_string());
                    }
                    if !plain[plain.len() - padding..]
                        .iter()
                        .all(|byte| *byte as usize == padding)
                    {
                        return Err("invalid AES-encrypted PDF padding".to_string());
                    }
                    let unpadded_len = plain.len() - padding;
                    plain.truncate(unpadded_len);
                    Ok(std::mem::take(&mut *plain))
                }
                CryptMethod::Identity => unreachable!(),
            }
        }
    }
}

fn explicit_crypt_filter(
    dict: &Dict,
    options: &SecurityOptions,
) -> Result<Option<CryptMethod>, String> {
    let filters: Vec<&str> = match dict.get("Filter") {
        Some(Value::Name(name)) => vec![name],
        Some(Value::Array(array)) => array
            .iter()
            .map(|filter| {
                filter
                    .as_name()
                    .ok_or_else(|| "invalid PDF stream /Filter array".to_string())
            })
            .collect::<Result<_, _>>()?,
        None => return Ok(None),
        Some(_) => return Err("invalid PDF stream /Filter".to_string()),
    };
    let Some(position) = filters.iter().position(|filter| *filter == "Crypt") else {
        return Ok(None);
    };
    if position != 0 {
        return Err("PDF /Crypt filter must be the first stream filter".to_string());
    }
    let params = match dict.get("DecodeParms").or_else(|| dict.get("DP")) {
        Some(Value::Dict(dict)) if filters.len() == 1 => Some(dict),
        Some(Value::Dict(_)) => {
            return Err("PDF /DecodeParms must be an array for multiple filters".to_string());
        }
        Some(Value::Array(array)) => match array.0.get(position) {
            Some(Value::Dict(dict)) => Some(dict),
            Some(Value::Null) | None => None,
            Some(_) => return Err("invalid /Crypt DecodeParms entry".to_string()),
        },
        None | Some(Value::Null) => None,
        Some(_) => return Err("invalid PDF stream /DecodeParms".to_string()),
    };
    if options.version < 4 {
        return Err("/Crypt filter requires Standard Security Handler version 4".to_string());
    }
    let name = match params.and_then(|params| params.get("Name")) {
        Some(Value::Name(name)) => name.as_str(),
        None => "Identity",
        Some(_) => return Err("invalid /Crypt DecodeParms /Name".to_string()),
    };
    named_crypt_filter(options, name).map(Some)
}

fn named_crypt_filter(options: &SecurityOptions, name: &str) -> Result<CryptMethod, String> {
    if name == "Identity" {
        return Ok(CryptMethod::Identity);
    }
    let filter = options
        .crypt_filters
        .as_ref()
        .and_then(|filters| filters.get(name))
        .and_then(Value::as_dict)
        .ok_or_else(|| format!("missing encryption crypt filter /{name}"))?;
    crypt_method(filter, name, options.file_key.len())
}

fn crypt_method(filter: &Dict, name: &str, file_key_len: usize) -> Result<CryptMethod, String> {
    let method = match filter.get("CFM") {
        Some(Value::Name(method)) => method.as_str(),
        None => "None",
        Some(_) => return Err(format!("invalid encryption crypt filter /{name} /CFM")),
    };
    let key_len = match filter.get("Length") {
        Some(Value::Integer(length)) if *length > 0 => usize::try_from(*length)
            .map_err(|_| format!("invalid encryption crypt filter /{name} /Length"))?,
        Some(_) => return Err(format!("invalid encryption crypt filter /{name} /Length")),
        None => file_key_len,
    };
    match method {
        "None" => {
            validate_auth_event(filter, name)?;
            Ok(CryptMethod::Identity)
        }
        "V2" if (5..=file_key_len).contains(&key_len) => {
            validate_auth_event(filter, name)?;
            Ok(CryptMethod::Rc4 { key_len })
        }
        "V2" => Err(format!(
            "invalid encryption crypt filter /{name} key length"
        )),
        "AESV2" if key_len == 16 && file_key_len == 16 => {
            validate_auth_event(filter, name)?;
            Ok(CryptMethod::Aes128)
        }
        "AESV2" => Err(format!("AESV2 crypt filter /{name} requires a 16-byte key")),
        method => Err(format!("unsupported encryption crypt method /{method}")),
    }
}

fn validate_auth_event(filter: &Dict, name: &str) -> Result<(), String> {
    if let Some(event) = filter.get("AuthEvent")
        && !matches!(event.as_name(), Some("DocOpen" | "EFOpen"))
    {
        return Err(format!(
            "invalid encryption crypt filter /{name} /AuthEvent"
        ));
    }
    Ok(())
}

fn decode_hex(hex: &str) -> Result<Vec<u8>, String> {
    let digits: Vec<u8> = hex.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let mut output = Vec::with_capacity(digits.len().div_ceil(2));
    for pair in digits.chunks(2) {
        let high = hex_digit(pair[0])?;
        let low = pair.get(1).map(|b| hex_digit(*b)).transpose()?.unwrap_or(0);
        output.push((high << 4) | low);
    }
    Ok(output)
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

fn required_integer(dict: &Dict, key: &str) -> Result<i64, String> {
    dict.get(key)
        .and_then(Value::as_integer)
        .ok_or_else(|| format!("missing or invalid encryption /{key}"))
}

fn pdf_string(value: &Value) -> Result<Vec<u8>, String> {
    match value {
        Value::String(value) => Ok(value.chars().map(|c| c as u8).collect()),
        Value::HexString(value) => {
            let mut digits = value.bytes().filter(|b| !b.is_ascii_whitespace());
            let mut bytes = Vec::with_capacity(value.len().div_ceil(2));
            while let Some(high) = digits.next() {
                let high = hex_digit(high)?;
                let low = digits.next().map(hex_digit).transpose()?.unwrap_or(0);
                bytes.push((high << 4) | low);
            }
            Ok(bytes)
        }
        _ => Err("encryption values must be PDF strings".to_string()),
    }
}

fn hex_digit(digit: u8) -> Result<u8, String> {
    match digit {
        b'0'..=b'9' => Ok(digit - b'0'),
        b'a'..=b'f' => Ok(digit - b'a' + 10),
        b'A'..=b'F' => Ok(digit - b'A' + 10),
        _ => Err("invalid hexadecimal PDF string".to_string()),
    }
}

fn pad_password(password: &[u8]) -> Zeroizing<Vec<u8>> {
    let mut padded = Zeroizing::new(Vec::with_capacity(PASSWORD_PADDING.len()));
    padded.extend_from_slice(&password[..password.len().min(PASSWORD_PADDING.len())]);
    let remaining = PASSWORD_PADDING.len() - padded.len();
    padded.extend_from_slice(&PASSWORD_PADDING[..remaining]);
    padded
}

fn derive_owner_key(
    padded_password: &[u8],
    revision: u8,
    key_len: usize,
    crypto: &dyn CryptoBackend,
) -> Zeroizing<Vec<u8>> {
    let mut digest = crypto.md5(padded_password);
    if revision >= 3 {
        for _ in 0..50 {
            digest = crypto.md5(&digest[..key_len]);
        }
    }
    Zeroizing::new(digest[..key_len].to_vec())
}

fn recover_user_password(
    owner_key: &[u8],
    owner_entry: &[u8],
    revision: u8,
    crypto: &dyn CryptoBackend,
) -> Result<Zeroizing<Vec<u8>>, String> {
    let mut recovered = Zeroizing::new(owner_entry.to_vec());
    if revision == 2 {
        *recovered = crypto.rc4(owner_key, &recovered)?;
    } else {
        for iteration in (0..20).rev() {
            let key = Zeroizing::new(owner_key.iter().map(|b| b ^ iteration).collect::<Vec<_>>());
            *recovered = crypto.rc4(&key, &recovered)?;
        }
    }
    Ok(recovered)
}

struct FileKeyParams<'a> {
    padded_password: &'a [u8],
    owner_entry: &'a [u8],
    permissions: i32,
    id: &'a [u8],
    revision: u8,
    encrypt_metadata: bool,
    key_len: usize,
}

fn derive_file_key(params: FileKeyParams<'_>, crypto: &dyn CryptoBackend) -> Zeroizing<Vec<u8>> {
    let mut input = Zeroizing::new(Vec::with_capacity(72 + params.id.len()));
    input.extend_from_slice(params.padded_password);
    input.extend_from_slice(params.owner_entry);
    input.extend_from_slice(&params.permissions.to_le_bytes());
    input.extend_from_slice(params.id);
    if params.revision >= 4 && !params.encrypt_metadata {
        input.extend_from_slice(&[0xff; 4]);
    }
    let mut digest = crypto.md5(&input);
    if params.revision >= 3 {
        for _ in 0..50 {
            digest = crypto.md5(&digest[..params.key_len]);
        }
    }
    Zeroizing::new(digest[..params.key_len].to_vec())
}

fn validate_user_password(
    file_key: &[u8],
    user_entry: &[u8],
    id: &[u8],
    revision: u8,
    crypto: &dyn CryptoBackend,
) -> Result<bool, String> {
    let expected = if revision == 2 {
        crypto.rc4(file_key, &PASSWORD_PADDING)?
    } else {
        let mut input = Zeroizing::new(Vec::with_capacity(PASSWORD_PADDING.len() + id.len()));
        input.extend_from_slice(&PASSWORD_PADDING);
        input.extend_from_slice(id);
        let digest = crypto.md5(&input);
        let mut value = crypto.rc4(file_key, &digest)?;
        for iteration in 1..20u8 {
            let key = Zeroizing::new(file_key.iter().map(|b| b ^ iteration).collect::<Vec<_>>());
            value = crypto.rc4(&key, &value)?;
        }
        value
    };
    let compare_len = if revision == 2 { 32 } else { 16 };
    Ok(expected[..compare_len]
        .ct_eq(&user_entry[..compare_len])
        .into())
}

fn crypt_filter(
    dict: &Dict,
    selector: &str,
    default: CryptMethod,
    file_key_len: usize,
) -> Result<CryptMethod, String> {
    let name = match dict.get(selector) {
        Some(Value::Name(name)) => name.as_str(),
        None => return Ok(default),
        Some(_) => return Err(format!("invalid encryption /{selector}")),
    };
    if name == "Identity" {
        return Ok(CryptMethod::Identity);
    }
    let filters = dict
        .get("CF")
        .and_then(Value::as_dict)
        .ok_or("missing encryption /CF dictionary")?;
    let filter = filters
        .get(name)
        .and_then(Value::as_dict)
        .ok_or_else(|| format!("missing encryption crypt filter /{name}"))?;
    crypt_method(filter, name, file_key_len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reader::security::crypto::RustCryptoBackend;
    use aes::Aes128;
    use aes::cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct CountingBackend {
        calls: AtomicUsize,
    }

    impl CryptoBackend for CountingBackend {
        fn md5(&self, input: &[u8]) -> [u8; 16] {
            self.calls.fetch_add(1, Ordering::Relaxed);
            RustCryptoBackend.md5(input)
        }

        fn rc4(&self, key: &[u8], input: &[u8]) -> Result<Vec<u8>, String> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            RustCryptoBackend.rc4(key, input)
        }

        fn aes128_cbc_decrypt(
            &self,
            key: &[u8],
            iv: &[u8],
            input: &[u8],
        ) -> Result<Vec<u8>, String> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            RustCryptoBackend.aes128_cbc_decrypt(key, iv, input)
        }
    }

    #[test]
    fn security_handler_uses_an_injectable_crypto_backend() {
        let reference = RustCryptoBackend;
        let (dict, id, expected_key) = revision_three_dictionary(&reference);
        let backend = CountingBackend::default();
        let handler = SecurityHandler::authenticate(&dict, &id, b"user", None, &backend).unwrap();
        assert_eq!(handler.file_key(), &*expected_key);
        assert!(backend.calls.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn password_padding_truncates_at_32_bytes() {
        assert_eq!(pad_password(b"abc").len(), 32);
        assert_eq!(
            &pad_password(b"01234567890123456789012345678901")[..32],
            b"01234567890123456789012345678901"
        );
        assert_eq!(
            &pad_password(b"01234567890123456789012345678901extra")[..32],
            b"01234567890123456789012345678901"
        );
    }

    #[test]
    fn crypt_filter_names_select_supported_methods() {
        let filters = Dict::new()
            .insert(
                "StdCF",
                Value::Dict(Dict::new().insert("CFM", Value::Name("AESV2".into()))),
            )
            .insert(
                "Rc4CF",
                Value::Dict(Dict::new().insert("CFM", Value::Name("V2".into()))),
            );
        let dict = Dict::new()
            .insert("CF", Value::Dict(filters))
            .insert("StrF", Value::Name("StdCF".into()))
            .insert("StmF", Value::Name("Rc4CF".into()));
        assert_eq!(
            crypt_filter(&dict, "StrF", CryptMethod::Identity, 16).unwrap(),
            CryptMethod::Aes128
        );
        assert_eq!(
            crypt_filter(&dict, "StmF", CryptMethod::Identity, 16).unwrap(),
            CryptMethod::Rc4 { key_len: 16 }
        );
    }

    #[test]
    fn security_dictionary_authenticates_user_and_owner_passwords() {
        let crypto = RustCryptoBackend;
        let id = b"known file id";
        let user_password = pad_password(b"user");
        let owner_password = pad_password(b"owner");
        let key_len = 5;
        let owner_key = derive_owner_key(&owner_password, 2, key_len, &crypto);
        let owner_entry = crypto.rc4(&owner_key, &user_password).unwrap();
        let permissions = -4i32;
        let file_key = derive_file_key(
            FileKeyParams {
                padded_password: &user_password,
                owner_entry: &owner_entry,
                permissions,
                id,
                revision: 2,
                encrypt_metadata: true,
                key_len,
            },
            &crypto,
        );
        let user_entry = crypto.rc4(&file_key, &PASSWORD_PADDING).unwrap();
        let dict = Dict::new()
            .insert("Filter", Value::Name("Standard".into()))
            .insert("V", Value::Integer(1))
            .insert("R", Value::Integer(2))
            .insert("O", Value::HexString(hex(&owner_entry)))
            .insert("U", Value::HexString(hex(&user_entry)))
            .insert("P", Value::Integer(permissions as i64));
        let id_value = Value::Array(crate::core::types::Array(vec![
            Value::HexString(hex(id)),
            Value::HexString(hex(id)),
        ]));

        let user = SecurityHandler::authenticate(&dict, &id_value, b"user", None, &crypto).unwrap();
        assert_eq!(user.file_key(), &*file_key);
        let owner =
            SecurityHandler::authenticate(&dict, &id_value, b"owner", None, &crypto).unwrap();
        assert_eq!(owner.file_key(), &*file_key);
        assert!(SecurityHandler::authenticate(&dict, &id_value, b"wrong", None, &crypto).is_err());
    }

    #[test]
    fn revision_three_authenticates_user_and_owner_passwords() {
        let crypto = RustCryptoBackend;
        let (dict, id, expected_key) = revision_three_dictionary(&crypto);
        for password in [b"user".as_slice(), b"owner".as_slice()] {
            let handler =
                SecurityHandler::authenticate(&dict, &id, password, None, &crypto).unwrap();
            assert_eq!(handler.file_key(), &*expected_key);
        }
        assert!(SecurityHandler::authenticate(&dict, &id, b"wrong", None, &crypto).is_err());
    }

    #[test]
    fn revision_four_aesv2_crypt_filter_decrypts_streams() {
        let crypto = RustCryptoBackend;
        let (dict, id, file_key) = revision_four_dictionary(&crypto);
        let handler = SecurityHandler::authenticate(&dict, &id, b"user", None, &crypto).unwrap();
        let stream_dict = Dict::new()
            .insert("Filter", Value::Name("Crypt".into()))
            .insert(
                "DecodeParms",
                Value::Dict(Dict::new().insert("Name", Value::Name("StdCF".into()))),
            );
        let encrypted = encrypt_aes_stream(b"R4 AESV2 stream", &file_key, 7, 0, &crypto);
        assert_eq!(
            handler
                .decrypt_stream(7, 0, &stream_dict, &encrypted)
                .unwrap(),
            b"R4 AESV2 stream"
        );
        let metadata_dict = Dict::new().insert("Type", Value::Name("Metadata".into()));
        let metadata_bytes = b"unencrypted metadata stream";
        assert_eq!(
            handler
                .decrypt_stream(8, 0, &metadata_dict, metadata_bytes)
                .unwrap(),
            metadata_bytes
        );
    }

    fn revision_three_dictionary(crypto: &dyn CryptoBackend) -> (Dict, Value, Zeroizing<Vec<u8>>) {
        revision_three_or_four_dictionary(3, false, crypto)
    }

    fn revision_four_dictionary(crypto: &dyn CryptoBackend) -> (Dict, Value, Zeroizing<Vec<u8>>) {
        revision_three_or_four_dictionary(4, true, crypto)
    }

    fn revision_three_or_four_dictionary(
        revision: u8,
        aes: bool,
        crypto: &dyn CryptoBackend,
    ) -> (Dict, Value, Zeroizing<Vec<u8>>) {
        let id_bytes = b"R3-R4 test identifier";
        let id = Value::Array(crate::core::types::Array(vec![
            Value::HexString(hex(id_bytes)),
            Value::HexString(hex(id_bytes)),
        ]));
        let user_password = pad_password(b"user");
        let owner_password = pad_password(b"owner");
        let key_len = 16;
        let owner_key = derive_owner_key(&owner_password, revision, key_len, crypto);
        let mut owner_entry = crypto.rc4(&owner_key, &user_password).unwrap();
        for iteration in 1..20u8 {
            let key = Zeroizing::new(
                owner_key
                    .iter()
                    .map(|byte| byte ^ iteration)
                    .collect::<Vec<_>>(),
            );
            owner_entry = crypto.rc4(&key, &owner_entry).unwrap();
        }
        let permissions = -4i32;
        let encrypt_metadata = !aes;
        let file_key = derive_file_key(
            FileKeyParams {
                padded_password: &user_password,
                owner_entry: &owner_entry,
                permissions,
                id: id_bytes,
                revision,
                encrypt_metadata,
                key_len,
            },
            crypto,
        );
        let mut user_material = Zeroizing::new(Vec::from(PASSWORD_PADDING));
        user_material.extend_from_slice(id_bytes);
        let user_digest = crypto.md5(&user_material);
        let mut user_entry = crypto.rc4(&file_key, &user_digest).unwrap();
        for iteration in 1..20u8 {
            let key = Zeroizing::new(
                file_key
                    .iter()
                    .map(|byte| byte ^ iteration)
                    .collect::<Vec<_>>(),
            );
            user_entry = crypto.rc4(&key, &user_entry).unwrap();
        }
        user_entry.extend_from_slice(&[0; 16]);
        let mut encrypt = Dict::new()
            .insert("Filter", Value::Name("Standard".into()))
            .insert("V", Value::Integer(if aes { 4 } else { 2 }))
            .insert("R", Value::Integer(revision as i64))
            .insert("Length", Value::Integer(128))
            .insert("O", Value::HexString(hex(&owner_entry)))
            .insert("U", Value::HexString(hex(&user_entry)))
            .insert("P", Value::Integer(permissions as i64));
        if aes {
            encrypt = encrypt
                .insert(
                    "CF",
                    Value::Dict(
                        Dict::new().insert(
                            "StdCF",
                            Value::Dict(
                                Dict::new()
                                    .insert("CFM", Value::Name("AESV2".into()))
                                    .insert("Length", Value::Integer(16)),
                            ),
                        ),
                    ),
                )
                .insert("StmF", Value::Name("StdCF".into()))
                .insert("StrF", Value::Name("StdCF".into()));
            if !encrypt_metadata {
                encrypt = encrypt.insert("EncryptMetadata", Value::Bool(false));
            }
        }
        (encrypt, id, file_key)
    }

    fn encrypt_aes_stream(
        plaintext: &[u8],
        file_key: &[u8],
        object_num: i64,
        generation: i64,
        crypto: &dyn CryptoBackend,
    ) -> Vec<u8> {
        let mut key_material = file_key.to_vec();
        key_material.extend_from_slice(&(object_num as u32).to_le_bytes()[..3]);
        key_material.extend_from_slice(&(generation as u16).to_le_bytes());
        key_material.extend_from_slice(b"sAlT");
        let key = crypto.md5(&key_material);
        let iv = [0x2a; 16];
        let padding = 16 - plaintext.len() % 16;
        let mut padded = plaintext.to_vec();
        padded.extend(std::iter::repeat_n(padding as u8, padding));
        let cipher = Aes128::new_from_slice(&key[..16]).unwrap();
        let mut previous = iv;
        for chunk in padded.chunks_mut(16) {
            for (byte, prior) in chunk.iter_mut().zip(previous) {
                *byte ^= prior;
            }
            let mut block = GenericArray::clone_from_slice(chunk);
            cipher.encrypt_block(&mut block);
            chunk.copy_from_slice(&block);
            previous.copy_from_slice(chunk);
        }
        [iv.as_slice(), padded.as_slice()].concat()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02X}")).collect()
    }
}
