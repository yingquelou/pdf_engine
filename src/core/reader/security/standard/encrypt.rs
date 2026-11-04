//! 写端标准安全处理器（ISO 32000-1 7.6.3.3、7.6.3.4）：加密字典 `/O` `/U`
//! 的生成、Table 20/Table 21 加密字典构造，以及字符串与流载荷的按对象加密。
//!
//! RC4 是对称算法（7.6.2「Because the RC4 algorithm … shall be used both to
//! encrypt and to decrypt data」），故本模块**复用读端原语**而不是另写一套：
//! - [`key::pad_password`]（Algorithm 2 a 的 32 字节填充/截断）、
//!   [`key::derive_owner_key`]（Algorithm 3 b–d）、
//!   [`key::derive_file_key`]（Algorithm 2 全部步骤）；
//! - [`decrypt::decrypt_bytes`]（Algorithm 1 的对象密钥派生 + 逐字节变换）；
//! - [`strings::decode_hex`] / [`strings::encode_hex`]。
//!
//! 明文口令只在调用栈内以 `Zeroizing` 缓冲参与密钥派生，不写盘、不入日志。

use zeroize::Zeroizing;

use crate::core::reader::security::crypto::{CryptoBackend, RustCryptoBackend};
use crate::core::types::{Dict, Document, Object, StreamData, Value};

use super::decrypt::decrypt_bytes;
use super::decrypt::object_encryption_key;
use super::filters::CryptMethod;
use super::key::{
    FileKeyParams, PASSWORD_PADDING, derive_file_key, derive_owner_key, pad_password,
};
use super::strings::{decode_hex, encode_hex};

/// Table 21 – 加密字典 `/O` 与 `/U` 的串长（字节）。
const ENTRY_LEN: usize = 32;

/// `/V 4` 写端支持的 crypt filter（7.6.5、Table 25）。
///
/// 7.6.3.1：「If a security handler of revision 4 is specified, the standard
/// security handler shall support crypt filters … The support shall be
/// limited to the Identity crypt filter (see Table 26) and crypt filters
/// named `StdCF` whose dictionaries contain a `CFM` value of `V2` or `AESV2`
/// and an `AuthEvent` value of `DocOpen`.」——写端因此只产出一个名为
/// `StdCF` 的筛选器，`CFM` 二选一；`Table 25` 的 `/CFM` 亦只列
/// `None`/`V2`/`AESV2` 三值（RC4 筛选器在规范中的名字是 `/V2`，不是
/// `/RC4V2`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StandardCryptFilter {
    /// `CFM /V2`：与文件密钥等长的 RC4（Algorithm 1 的 RC4 路径）。
    Rc4V2,
    /// `CFM /AESV2`（PDF 1.6，Table 25）：AES-128-CBC，要求 128 位文件密钥。
    /// 字典形态可用，但**写端公开入口拒绝它**——见 [`encrypt_bytes`] 的 IV
    /// shall 说明。
    AesV2,
}

impl StandardCryptFilter {
    /// Table 25 的 `/CFM` 名。
    fn cfm_name(self) -> &'static str {
        match self {
            Self::Rc4V2 => "V2",
            Self::AesV2 => "AESV2",
        }
    }

    /// Table 20（catalog `/Version` 依据 Table 28 与 7.5.2）声明的文档版本：
    /// `/CF` `/StmF` `/StrF` 与 `/V 4` 是 PDF 1.5 条目，`CFM /AESV2` 则是
    /// PDF 1.6 条目（Table 25）。
    fn min_file_version(self) -> &'static str {
        match self {
            Self::Rc4V2 => "1.5",
            Self::AesV2 => "1.6",
        }
    }

    fn method(self, key_len: usize) -> CryptMethod {
        match self {
            Self::Rc4V2 => CryptMethod::Rc4 { key_len },
            Self::AesV2 => CryptMethod::Aes128,
        }
    }
}

/// 7.6.3.3.3 Algorithm 3 f/g – 用所有者密钥加密填充后的用户密码得到 `/O`。
///
/// R 2 只做一次 RC4；R ≥ 3 再以「原密钥逐字节 XOR 迭代计数（1–19）」为密钥
/// 追加 19 次 RC4。读端 `key::recover_user_password`（Algorithm 7）是它的逆。
fn compute_owner_entry(
    owner_key: &[u8],
    padded_user: &[u8],
    revision: u8,
    crypto: &dyn CryptoBackend,
) -> Result<Vec<u8>, String> {
    let mut value = crypto.rc4(owner_key, padded_user)?;
    if revision >= 3 {
        for iteration in 1..20u8 {
            let key = Zeroizing::new(
                owner_key
                    .iter()
                    .map(|byte| byte ^ iteration)
                    .collect::<Vec<_>>(),
            );
            value = crypto.rc4(&key, &value)?;
        }
    }
    Ok(value)
}

/// Table 21 `/U`：R 2 走 7.6.3.4.2 Algorithm 4，R ≥ 3 走 Algorithm 5。
///
/// Algorithm 4 b：`RC4(文件密钥, 32 字节填充串)`；Algorithm 5 b–f：
/// `MD5(填充串 ‖ /ID 首元素)` 的 16 字节摘要先 RC4 一次、再按迭代计数
/// XOR 密钥做 19 次，最后追加 16 字节任意填充凑满 32 字节。
fn compute_user_entry(
    file_key: &[u8],
    file_id: &[u8],
    revision: u8,
    crypto: &dyn CryptoBackend,
) -> Result<Vec<u8>, String> {
    if revision == 2 {
        return crypto.rc4(file_key, &PASSWORD_PADDING);
    }
    let mut input = Zeroizing::new(Vec::with_capacity(PASSWORD_PADDING.len() + file_id.len()));
    input.extend_from_slice(&PASSWORD_PADDING);
    input.extend_from_slice(file_id);
    let digest = crypto.md5(&input);
    let mut value = crypto.rc4(file_key, &digest)?;
    for iteration in 1..20u8 {
        let key = Zeroizing::new(
            file_key
                .iter()
                .map(|byte| byte ^ iteration)
                .collect::<Vec<_>>(),
        );
        value = crypto.rc4(&key, &value)?;
    }
    // Algorithm 5 f：「Append 16 bytes of arbitrary padding」——读端 Algorithm 6
    // 只比对前 16 字节，故此处取标准填充串前 16 字节，保持输出确定性。
    value.extend_from_slice(&PASSWORD_PADDING[..ENTRY_LEN / 2]);
    Ok(value)
}

/// Algorithm 1 的正向变换（7.6.2）。
///
/// - RC4：7.6.2「RC4 is a symmetric stream cipher: the same algorithm shall
///   be used for both encryption and decryption」，直接复用
///   [`decrypt_bytes`]（其内部即 Algorithm 1 b–d 的对象密钥 + RC4）；
/// - AES-128-CBC（`CFM /AESV2`，Table 25）：Algorithm 1 d 的 CBC 形态——
///   填充按 7.6.2「the pad shall consist of 16 - (M mod 16) bytes whose
///   value shall also be 16 - (M mod 16)」（即 PKCS#5/RFC 2898，含 M 整除
///   16 时补一整块 0x10），初始化向量按 Algorithm 1 d「stored as the first
///   16 bytes of the encrypted stream or string」前缀输出。
///
/// **`CryptMethod::Aes128` 不从公共写端入口可达**：Table 25（raw L3340–
/// L3342）字面要求 IV「shall be randomly generated」，本 crate 无 CSPRNG
/// 依赖也不新增依赖，此处的 `MD5(对象密钥 ‖ 明文)` 确定性 IV 违反该 shall，
/// 故 `assemble::encryption::validate_encryption` 直接拒绝 `CFM /AESV2`
/// 输出。保留本分支只为让读端 `aes128_cbc_decrypt` 在同层测试里有一个
/// 正向对照（往返与 NIST 向量），不构成对外能力承诺。
pub(super) fn encrypt_bytes(
    file_key: &[u8],
    object_num: i64,
    generation: i64,
    method: CryptMethod,
    plain: &[u8],
    crypto: &dyn CryptoBackend,
) -> Result<Vec<u8>, String> {
    if method != CryptMethod::Aes128 {
        return decrypt_bytes(file_key, object_num, generation, method, plain, crypto);
    }
    let object_key = Zeroizing::new(object_encryption_key(
        file_key, object_num, generation, method, crypto,
    )?);
    let pad_len = 16 - plain.len() % 16;
    let mut padded = Vec::with_capacity(plain.len() + pad_len);
    padded.extend_from_slice(plain);
    padded.resize(plain.len() + pad_len, pad_len as u8);
    let iv = crypto.md5(&[object_key.as_slice(), &padded].concat());
    let body = crypto.aes128_cbc_encrypt(&object_key, &iv, &padded)?;
    Ok([iv.as_slice(), &body].concat())
}

/// Table 20/Table 21/Table 25 – 标准安全处理器的加密字典。
///
/// `/Filter /Standard`、`/V`、`/R`、`/O`、`/U`、`/P` 为 Table 20 + Table 21
/// 的必需条目。顶层 `/Length`（位）在 `/V 2` 与 `/V 4` 写出：Table 20 字面
/// 只允许「only if V is 2 or 3」，而 Algorithm 2 h/i 又以 Length 定义 R≥3 的
/// 密钥字节数（本库读端 `auth.rs` 同口径），故 `/V 4` 沿用 conforming
/// 生产者的通行写法把顶层 `/Length` 与 `StdCF` 的 `/Length` 一并写出且保持
/// 相等——`/CF` 侧按 Table 25「Standard security handler expresses the
/// length in multiples of 8 (16 means 128)」以**字节**表达。`/CF` 只含
/// `StdCF`（7.6.3.1 的 revision 4 支持上限），带 `CFM`、`Length` 与
/// `AuthEvent /DocOpen`；`/StmF` `/StrF` 均选 `StdCF`（Table 20：名字须是
/// `CF` 的键；缺省值为 Identity，缺省会让读端跳过解密，故必须写出）。
/// `/O` `/U` 是直接对象且**不加密**（7.6.1「Unlike strings within the body
/// of the document, those in the encryption dictionary shall be direct
/// objects. The contents of the encryption dictionary shall not be
/// encrypted」）。
pub(super) fn encryption_dictionary(
    handler_version: u8,
    revision: u8,
    key_bits: u8,
    permissions: i32,
    owner_entry: &[u8],
    user_entry: &[u8],
    crypt_filter: Option<StandardCryptFilter>,
) -> Dict {
    let mut dict = Dict::new()
        .insert("Filter", Value::Name("Standard".into()))
        .insert("V", Value::Integer(i64::from(handler_version)))
        .insert("R", Value::Integer(i64::from(revision)))
        .insert("O", Value::HexString(encode_hex(owner_entry)))
        .insert("U", Value::HexString(encode_hex(user_entry)))
        .insert("P", Value::Integer(i64::from(permissions)));
    if handler_version >= 2 {
        dict = dict.insert("Length", Value::Integer(i64::from(key_bits)));
    }
    if let Some(filter) = crypt_filter {
        // Table 25：/Type /CryptFilter 可选；/AuthEvent 默认 DocOpen，且
        // 「If this filter is used as the value of StrF or StmF … the
        // conforming reader shall ignore this key and behave as if the
        // value is DocOpen」——仍写出以自证意图。
        let std_cf = Dict::new()
            .insert("Type", Value::Name("CryptFilter".into()))
            .insert("CFM", Value::Name(filter.cfm_name().into()))
            .insert("Length", Value::Integer(i64::from(key_bits / 8)))
            .insert("AuthEvent", Value::Name("DocOpen".into()));
        dict = dict
            .insert(
                "CF",
                Value::Dict(Dict::new().insert("StdCF", Value::Dict(std_cf))),
            )
            .insert("StmF", Value::Name("StdCF".into()))
            .insert("StrF", Value::Name("StdCF".into()));
    }
    dict
}

/// Algorithm 1 – 按对象加密一个对象值。
///
/// 与读端 `decrypt::decrypt_value` 严格镜像：字面串按「每字符一字节」取样
/// （读端同样以 `chars().map(|c| c as u8)` 还原），密文以十六进制串写出以
/// 保持字节精确；签名值 `Contents`（`/Type /Sig` 或 `/FT /Sig`）不加密；
/// 流字典条目递归、流载荷按其自身对象号加密并同步 `/Length`。
fn encrypt_value(
    value: &mut Value,
    object_num: i64,
    generation: i64,
    signature_contents: bool,
    file_key: &[u8],
    method: CryptMethod,
    crypto: &dyn CryptoBackend,
) -> Result<(), String> {
    match value {
        Value::String(text) if !signature_contents => {
            let plain: Vec<u8> = text.chars().map(|char| char as u8).collect();
            let encrypted =
                encrypt_bytes(file_key, object_num, generation, method, &plain, crypto)?;
            *value = Value::HexString(encode_hex(&encrypted));
        }
        Value::HexString(hex) if !signature_contents => {
            let plain = decode_hex(hex)?;
            let encrypted =
                encrypt_bytes(file_key, object_num, generation, method, &plain, crypto)?;
            *hex = encode_hex(&encrypted);
        }
        Value::Array(array) => {
            for item in &mut array.0 {
                encrypt_value(
                    item, object_num, generation, false, file_key, method, crypto,
                )?;
            }
        }
        Value::Dict(dict) => {
            let is_signature = dict.get("Type").and_then(Value::as_name) == Some("Sig")
                || dict.get("FT").and_then(Value::as_name) == Some("Sig");
            for (key, item) in &mut dict.0 {
                encrypt_value(
                    item,
                    object_num,
                    generation,
                    is_signature && key == "Contents",
                    file_key,
                    method,
                    crypto,
                )?;
            }
        }
        Value::Stream(dict, data) => {
            let is_signature = dict.get("Type").and_then(Value::as_name) == Some("Sig")
                || dict.get("FT").and_then(Value::as_name) == Some("Sig");
            for (key, item) in &mut dict.0 {
                encrypt_value(
                    item,
                    object_num,
                    generation,
                    is_signature && key == "Contents",
                    file_key,
                    method,
                    crypto,
                )?;
            }
            let StreamData::Owned(bytes) = data else {
                return Err(
                    "encrypting a document requires inline stream bytes (StreamData::Owned)"
                        .to_string(),
                );
            };
            let encrypted = encrypt_bytes(file_key, object_num, generation, method, bytes, crypto)?;
            *bytes = encrypted;
            // 写端在流字典里显式写了 /Length（见 assemble/*）：RC4 不改变
            // 长度（7.6.2），AES 则按 Algorithm 1 d 多出 IV 前缀与 7.6.2
            // 填充块，故一律按实际字节重算以免漂移。
            if dict.get("Length").and_then(Value::as_integer).is_some() {
                let len = bytes.len();
                dict.0
                    .insert("Length".to_string(), Value::Integer(len as i64));
            }
        }
        _ => {}
    }
    Ok(())
}

/// [`Document::encrypt_with_standard_handler`] 的参数包（字段含义见该函数
/// 文档；聚合成结构体以满足本仓库 clippy 的参数量红线）。
pub(crate) struct StandardEncryptionRequest<'a> {
    pub user_password: &'a [u8],
    pub owner_password: Option<&'a [u8]>,
    pub permissions: i32,
    pub handler_version: u8,
    pub key_bits: u8,
    pub crypt_filter: Option<StandardCryptFilter>,
    pub file_id: &'a [u8],
}

impl Document {
    /// 7.6.3 – 以标准安全处理器加密整篇文档（写端唯一入口）。
    ///
    /// 流程：Algorithm 3 求 `/O` → Algorithm 2 派生文件密钥 → Algorithm 4/5
    /// 求 `/U` → 把 Table 20/Table 21（`/V 4` 时另加 Table 25 的 `/CF` 与
    /// `/StmF` `/StrF` 选择器）加密字典作为**未压缩的顶层间接对象**追加
    /// （7.5.7：加密字典不得存于对象流）→ trailer 挂 `/Encrypt` 并校正
    /// `/Size` → 按 Algorithm 1 加密既有对象里的全部字符串与流载荷。
    ///
    /// `handler_version` 为加密字典 `/V`（`1` = RC4-40/`R 2`，`2` = RC4-128/
    /// `R 3`，`4` = crypt filter/`R 4`，见 7.6.5「the value of the V entry
    /// shall be 4 to use crypt filters」）；`key_bits` 为顶层 `/Length`（位，
    /// R 2 恒为 40 位 5 字节）；`crypt_filter` 仅 `/V 4` 有意义（`StdCF` 的
    /// `CFM`，7.6.3.1 限定 `V2`/`AESV2`），`/V ≤ 2` 必须为 `None`；
    /// `owner_password` 为 `None` 时按 Algorithm 3 a 用用户密码代替；
    /// `file_id` 为 trailer `/ID` 首元素（Algorithm 2 e）。
    ///
    /// 失败返回错误且**不改动任何对象**（先算密钥、后改文档）。
    pub(crate) fn encrypt_with_standard_handler(
        &mut self,
        request: StandardEncryptionRequest<'_>,
    ) -> Result<(), String> {
        let StandardEncryptionRequest {
            user_password,
            owner_password,
            permissions,
            handler_version,
            key_bits,
            crypt_filter,
            file_id,
        } = request;
        if file_id.is_empty() {
            return Err("encrypted documents need a trailer /ID element".to_string());
        }
        let revision = match handler_version {
            1 => 2,
            2 => 3,
            4 => 4,
            other => {
                return Err(format!(
                    "unsupported standard security handler /V {other} (write side handles 1, 2 and 4)"
                ));
            }
        };
        if revision < 4 && crypt_filter.is_some() {
            return Err(
                "crypt filters (/CF /StmF /StrF) are only valid with /V 4 (7.6.5)".to_string(),
            );
        }
        let key_len = if revision == 2 {
            5
        } else {
            if !key_bits.is_multiple_of(8) || !(40..=128).contains(&key_bits) {
                return Err(format!(
                    "invalid encryption key length {key_bits} bits: /Length must be a multiple of 8 in 40..=128"
                ));
            }
            usize::from(key_bits / 8)
        };
        if crypt_filter == Some(StandardCryptFilter::AesV2) && key_len != 16 {
            return Err(format!(
                "AESV2 crypt filter requires a 128-bit file encryption key, got {key_bits} bits"
            ));
        }
        let crypto = &RustCryptoBackend;

        // —— Algorithm 3（/O）与 Algorithm 2（文件密钥）、Algorithm 4/5（/U）——
        // R 4 的 /O 与 /U 与 R 3 同算法（Algorithm 3/5 的措辞是
        // 「Security handlers of revision 3 or greater」），差异只在
        // Algorithm 2 f 的元数据例外与 crypt filter 的密钥选择。
        let padded_user = pad_password(user_password);
        let padded_owner = pad_password(owner_password.unwrap_or(user_password));
        let owner_key = derive_owner_key(&padded_owner, revision, key_len, crypto);
        let owner_entry = compute_owner_entry(&owner_key, &padded_user, revision, crypto)?;
        let file_key = derive_file_key(
            FileKeyParams {
                padded_password: &padded_user,
                owner_entry: &owner_entry,
                permissions,
                id: file_id,
                revision,
                // /EncryptMetadata 仅在 /V 4 有意义（Table 21），此处恒按
                // 「元数据也加密」（缺省 true）写出，与读端 `auth.rs` 的
                // 默认值一致；R 2/R 3 不受影响。
                encrypt_metadata: true,
                key_len,
            },
            crypto,
        );
        let user_entry = compute_user_entry(&file_key, file_id, revision, crypto)?;
        if owner_entry.len() != ENTRY_LEN || user_entry.len() != ENTRY_LEN {
            return Err("encryption dictionary /O and /U must be 32-byte strings".to_string());
        }

        // —— Algorithm 1：加密已装配对象里的字符串与流 ——
        let method = match crypt_filter {
            Some(filter) => filter.method(key_len),
            None => CryptMethod::Rc4 {
                key_len: file_key.len(),
            },
        };
        if handler_version == 4 {
            // /V 4 及其 CFM 属 PDF 1.5/1.6 特性（Table 20、Table 25），而
            // 本 writer 的加密输出锁定 ≤1.4 经典布局（7.5.7）。7.5.2：
            // 「Beginning with PDF 1.4, the Version entry in the document's
            // catalog dictionary … shall be used instead of the version
            // specified in the Header」，Table 28 又要求该条目是**名字**，
            // 故以 catalog /Version 声明真实符合版本，header 保持 1.4。
            self.declare_catalog_file_version(crypt_filter.unwrap_or(StandardCryptFilter::Rc4V2))?;
        }
        let mut objects = self.objects_for_write();
        let mut max_num = 0i64;
        for object in &mut objects {
            max_num = max_num.max(object.num);
            encrypt_value(
                &mut object.value,
                object.num,
                object.generation,
                false,
                &file_key,
                method,
                crypto,
            )?;
        }
        for object in objects {
            self.set_object(object);
        }

        // —— 加密字典：顶层未压缩间接对象（7.5.7）——
        let encrypt_num = max_num + 1;
        self.set_object(Object::new(
            encrypt_num,
            0,
            Value::Dict(encryption_dictionary(
                handler_version,
                revision,
                key_bits,
                permissions,
                &owner_entry,
                &user_entry,
                crypt_filter,
            )),
        ));
        self.trailer
            .dict
            .0
            .insert("Encrypt".to_string(), Value::Ref(encrypt_num, 0));
        self.trailer
            .dict
            .0
            .insert("Size".to_string(), Value::Integer(encrypt_num + 1));
        Ok(())
    }

    /// 向 catalog（trailer `/Root` 指向的对象）写入 `/Version` 名字条目。
    fn declare_catalog_file_version(&mut self, filter: StandardCryptFilter) -> Result<(), String> {
        let root_num = match self.trailer.dict.get("Root") {
            Some(Value::Ref(num, _)) => *num,
            _ => return Err("encrypted documents need a catalog (/Root) object".to_string()),
        };
        let mut objects = self.objects_for_write();
        let version = filter.min_file_version();
        let catalog = objects
            .iter_mut()
            .find(|object| object.num == root_num)
            .ok_or("encryption: /Root references an absent catalog object")?;
        match &mut catalog.value {
            Value::Dict(dict) => {
                dict.0
                    .insert("Version".to_string(), Value::Name(version.to_string()));
            }
            _ => return Err("encryption: /Root does not reference a dictionary".to_string()),
        }
        self.set_object(catalog.clone());
        Ok(())
    }
}
