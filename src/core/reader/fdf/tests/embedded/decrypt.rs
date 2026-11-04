//! 12.7.7.3 的 **FDF 侧解密**单元测试：Table 244 revision 1 的密钥派生
//! 独立向量（MD5(padded password) 前 5 字节，含空口令 = 整串填充与口令
//! >32 字节截断两种边界）、RC4 加/解密对称性、带口令入口把密文类型化成
//! 子文档、`/EncryptionRevision` ≠ 1 拒绝解密、错口令降级、口令逐层下传，
//! 以及「密钥/口令绝不进入诊断」的泄漏检查。
//!
//! 规范依据与 `embedded/crypto.rs` 的模块文档同一（raw L24893–L24900、
//! L3015–L3018 + L3054–L3059、L24935–L24939）；本文件的期望值由
//! 独立实现的 MD5 直接算出，不复用被测代码的派生路径。

use super::*;
use crate::core::reader::fdf::embedded::crypto;
use md5::{Digest, Md5};

/// 独立实现的 7.6.3.3 步 (a) 填充串（raw L3054–L3055），用于交叉校验
/// 被测代码里的 `PASSWORD_PADDING`。
const PADDING: [u8; 32] = [
    0x28, 0xbf, 0x4e, 0x5e, 0x4e, 0x75, 0x8a, 0x41, 0x64, 0x00, 0x4e, 0x56, 0xff, 0xfa, 0x01, 0x08,
    0x2e, 0x2e, 0x00, 0xb6, 0xd0, 0x68, 0x3e, 0x80, 0x2f, 0x0c, 0xa9, 0xfe, 0x64, 0x53, 0x69, 0x7a,
];

/// 期望密钥（`MD5(padded)` 前 5 字节）：独立算出，不走被测函数。
fn expected_key(password: &[u8]) -> [u8; 5] {
    let head = &password[..password.len().min(32)];
    let mut padded = head.to_vec();
    padded.extend_from_slice(&PADDING[..32 - padded.len()]);
    let digest = Md5::digest(&padded);
    let mut key = [0u8; 5];
    key.copy_from_slice(&digest[..5]);
    key
}

/// 一个 `/EncryptionRevision 1` 的载体流：`bytes` 是**明文**内层 FDF，
/// 写入前按派生密钥做 RC4 解扰（加密与解密同一运算）。
fn encrypted_carrier(bytes: &[u8], password: &[u8]) -> Value {
    let cipher = crypto::rc4_transform(crypto::derive_key(password).as_slice(), bytes)
        .expect("5 字节密钥必被 RC4 接受");
    carrier_stream(
        cipher.to_vec(),
        Dict::new().insert("EncryptionRevision", Value::Integer(1)),
    )
}

/// 「FDF 字典 `/EmbeddedFDFs` = [一个 `/EncryptionRevision` 条目]」的文件
/// 装配；`revision == 1` 时载体按口令 RC4 加密，其余值放**明文**字节以验证
/// 「不 Attempt 解密」（而非「解了但解错」）。
fn encrypted_fdf_document(plain: &[u8], password: &[u8], revision: i64) -> Document {
    let carrier = if revision == 1 {
        encrypted_carrier(plain, password)
    } else {
        carrier_stream(
            plain.to_vec(),
            Dict::new().insert("EncryptionRevision", Value::Integer(revision)),
        )
    };
    let spec = Dict::new()
        .insert("F", Value::String("crypto.fdf".to_string()))
        .insert("EF", carrier);
    with_embedded_fdfs(field_array(vec![Value::Dict(spec)]), Vec::new())
}

// ── 密钥派生：独立向量 ──────────────────────────────────────────────

#[test]
fn fdf_encryption_key_is_the_first_five_md5_bytes_of_the_padded_password() {
    // raw L24896–L24900：密钥 = 步 (a) 填充结果做 MD5（步 (b)）后的前
    // 5 字节（40 位），与 7.6 的 PDF 文件加密无关（不掺 /O、/P、/ID）。
    for password in [
        b"".as_slice(),
        b"secret".as_slice(),
        b"user-password".as_slice(),
        b"\x00\x01\xfe".as_slice(),
    ] {
        let key = crypto::derive_key(password);
        assert_eq!(
            key.as_slice(),
            &expected_key(password)[..],
            "口令 {password:?}"
        );
        assert_eq!(key.len(), crypto::FDF_ENCRYPTION_KEY_LEN);
    }
    // 硬编码向量（由独立 MD5 实现算出，防「两边同一处写错」）。
    assert_eq!(
        *crypto::derive_key(b"secret"),
        [0x1f, 0x7f, 0x13, 0xb7, 0x3d]
    );
}

#[test]
fn empty_fdf_password_is_replaced_by_the_whole_padding_string() {
    // 7.6.3.3 步 (a)（raw L3015–L3018）：空口令 ⇒ 整串换成填充串；
    // 故密钥 = MD5(PADDING)[..5]，而**非** MD5(b"")[..5]。
    assert_eq!(
        crypto::pad_password(b"").as_slice(),
        &PADDING[..],
        "空口令即整串填充"
    );
    assert_eq!(*crypto::derive_key(b""), expected_key(b""));
    assert_eq!(*crypto::derive_key(b""), [0x51, 0x21, 0x47, 0xb9, 0x9e]);
    assert_ne!(
        crypto::derive_key(b"").as_slice(),
        &Md5::digest(b"")[..5],
        "空口令不是「对空字节做哈希」"
    );
}

#[test]
fn fdf_password_longer_than_32_bytes_is_truncated_before_hashing() {
    // 步 (a)「truncate or pad」：只取前 32 字节，第 33 字节起不参与哈希。
    let long = b"0123456789012345678901234567890123456789";
    let truncated = &long[..32];
    assert_eq!(
        crypto::pad_password(long).as_slice(),
        truncated,
        "超长口令被截断到恰好 32 字节"
    );
    assert_eq!(
        crypto::derive_key(long).as_slice(),
        crypto::derive_key(truncated).as_slice(),
        "第 33 字节之后的内容不影响密钥"
    );
    assert_eq!(*crypto::derive_key(long), [0x29, 0x8b, 0xf0, 0x19, 0x7a]);
    assert_ne!(
        crypto::derive_key(truncated).as_slice(),
        crypto::derive_key(&long[..31]).as_slice()
    );
}

// ── RC4 对称性 ──────────────────────────────────────────────────────

#[test]
fn fdf_rc4_transform_is_its_own_inverse_for_the_derived_key() {
    // RC4 是流密码：同一密钥两次 `apply_keystream` 还原明文，故本函数
    // 既用于读端解密，也用于测试里构造密文载体。
    let plain = inner_fdf_bytes("round-trip");
    let password = b"s3cr3t-pw";
    let key = crypto::derive_key(password);
    let cipher = crypto::rc4_transform(key.as_slice(), &plain).expect("5 字节密钥");
    assert_ne!(cipher.as_slice(), plain.as_slice(), "密文不等于明文");
    let back = crypto::rc4_transform(key.as_slice(), cipher.as_slice()).expect("同一运算");
    assert_eq!(back.as_slice(), plain.as_slice());
    // 与解密入口同一把钥匙：`decrypt` 即 `rc4_transform(derive_key(..))`。
    assert_eq!(
        crypto::decrypt(cipher.as_slice(), password)
            .expect("解密成功")
            .as_slice(),
        plain.as_slice()
    );
    // 非 1..=16 字节的密钥一律 `None`（调用方降级，不 panic）。
    assert!(crypto::rc4_transform(&[], &plain).is_none());
    assert!(crypto::rc4_transform(&[0u8; 17], &plain).is_none());
}

// ── 带口令入口：解密后照常类型化 ────────────────────────────────────

#[test]
fn encrypted_embedded_fdf_with_password_types_the_child_document() {
    // 新入口：密文 → 解密 → **与未加密条目同一条**类型化路径
    // （Table 242/243/246 照常建模，字段可读）。
    let password = b"s3cr3t-pw";
    let plain = inner_fdf_bytes("inner-secret");
    let document = encrypted_fdf_document(&plain, password, 1);
    let view = document
        .fdf_embedded_fdfs_with_password(password)
        .expect("带口令读端不应报错")
        .expect("条目存在时应类型化");
    let entry = first_entry(&view);
    assert!(entry.is_encrypted());
    assert_eq!(entry.encryption_revision, Some(1));
    assert!(entry.is_decrypted(), "本条目的明文来自解密");
    let child = entry.document.clone().expect("解密后应递归出子文档");
    assert_eq!(child.fdf.status.as_deref(), Some("inner-secret"));
    assert_eq!(
        child.field("child").expect("子字段按全限定名可查").value,
        Some(Value::String("42".to_string()))
    );
    assert!(
        entry.issues.is_empty(),
        "解密成功不该记诊断：{:?}",
        entry.issues
    );
    assert!(view.document("crypto.fdf").is_some(), "按名查找仍可用");
    // 空口令也是一把合法钥匙（步 (a) 的整串填充）。
    let zero_password = encrypted_fdf_document(&plain, b"", 1);
    assert_eq!(
        zero_password
            .fdf_embedded_fdfs_with_password("")
            .expect("ok")
            .expect("视图")
            .entries[0]
            .document
            .as_ref()
            .expect("空口令解密后应有子文档")
            .fdf
            .status
            .as_deref(),
        Some("inner-secret")
    );
}

#[test]
fn encrypted_embedded_fdf_keeps_the_ciphertext_as_the_raw_value() {
    // 原值永不丢弃：`data()` 给的是文件里的密文，`/EncryptionRevision`
    // 与载体字典条目照常可取；解出的明文不驻留视图（`Zeroizing` 归零）。
    let password = b"s3cr3t-pw";
    let plain = inner_fdf_bytes("keep-raw");
    let document = encrypted_fdf_document(&plain, password, 1);
    let keyed = document
        .fdf_embedded_fdfs_with_password(password)
        .expect("ok")
        .expect("视图");
    let entry = first_entry(&keyed);
    let stored = entry.data().expect("载体字节");
    assert_ne!(stored, plain.as_slice(), "载体原值是密文");
    assert!(entry.is_decrypted());
    // 旧入口（无口令）保持今天的宽容行为：不解密、只记诊断。
    let legacy_view = embedded_of(&document);
    let legacy = first_entry(&legacy_view);
    assert!(legacy.document.is_none(), "旧入口不带口令故不解密");
    assert!(!legacy.is_decrypted());
    assert_eq!(legacy.data().expect("同一份密文原值"), stored);
    let joined = legacy.issues.join("|");
    assert!(joined.contains("no user password"), "got {joined}");
}

#[test]
fn non_one_encryption_revision_is_never_decrypted_even_with_a_password() {
    // Table 244 raw L24937–L24939：publication 时唯一被定义的值是 1 ⇒
    // 其余值不 Attempt 解密，只记「不是唯一被定义的值」的诊断。
    let password = b"s3cr3t-pw";
    let plain = inner_fdf_bytes("wrong-revision");
    let document = encrypted_fdf_document(&plain, password, 3);
    let view = document
        .fdf_embedded_fdfs_with_password(password)
        .expect("带口令也不该报错")
        .expect("视图");
    let entry = first_entry(&view);
    assert_eq!(entry.encryption_revision, Some(3));
    assert!(!entry.encryption_revision_supported());
    assert!(!entry.is_decrypted());
    assert!(
        entry.document.is_none(),
        "revision 3 无已定义算法，故不解密"
    );
    let joined = entry.issues.join("|");
    assert!(
        joined.contains("not the only defined value 1"),
        "got {joined}"
    );
    assert!(
        joined.contains("Table 244 raw L24937-L24939"),
        "诊断要留可追溯的规范行号：{joined}"
    );
}

#[test]
fn unencrypted_entries_are_identical_with_and_without_a_password() {
    // 未声明 `/EncryptionRevision` 的条目：口令不参与，行为逐字一致。
    let plain = inner_fdf_bytes("plain");
    let spec = Dict::new()
        .insert("F", Value::String("plain.fdf".to_string()))
        .insert("EF", carrier_stream(plain, Dict::new()));
    let document = with_embedded_fdfs(field_array(vec![Value::Dict(spec)]), Vec::new());
    let legacy = embedded_of(&document);
    let keyed = document
        .fdf_embedded_fdfs_with_password("irrelevant")
        .expect("ok")
        .expect("视图");
    assert_eq!(keyed, legacy, "明文条目的视图完全相同");
    assert!(!first_entry(&keyed).is_decrypted());
    assert!(first_entry(&keyed).document.is_some());
}

#[test]
fn wrong_fdf_password_degrades_to_diagnostics_without_a_child_document() {
    // 错口令 → 明文是噪声 → 「跳过 + 记录诊断 + 保留原值」，不 panic、
    // 不让整份 FDF 解析失败。
    let plain = inner_fdf_bytes("not-this-one");
    let document = encrypted_fdf_document(&plain, b"right-pw", 1);
    let view = document
        .fdf_embedded_fdfs_with_password(b"wrong-pw")
        .expect("错口令也不报错")
        .expect("条目存在时应类型化");
    let entry = first_entry(&view);
    assert!(entry.is_encrypted());
    assert!(entry.document.is_none(), "错口令不该产出子文档");
    assert!(entry.nested.is_none());
    let joined = entry.issues.join("|");
    assert!(joined.contains("password may be incorrect"), "got {joined}");
    assert_eq!(
        entry.data().expect("密文原值仍在"),
        document
            .fdf_embedded_fdfs()
            .expect("ok")
            .expect("视图")
            .entries[0]
            .data()
            .expect("同一份密文")
    );
}

#[test]
fn decryption_diagnostics_never_carry_the_password_or_key_bytes() {
    // 安全：派生密钥与口令绝不写进诊断（`FdfEmbeddedFdf` 的 issues 会
    // 被调用方直接打印）。
    let password: &[u8] = b"leak-check-pw";
    let key = crypto::derive_key(password);
    let plain = inner_fdf_bytes("leak");
    let document = encrypted_fdf_document(&plain, password, 1);
    let mut collected = Vec::new();
    collected.extend(
        document
            .fdf_embedded_fdfs()
            .expect("ok")
            .expect("视图")
            .entries[0]
            .issues
            .iter()
            .cloned(),
    );
    collected.extend(
        document
            .fdf_embedded_fdfs_with_password(password)
            .expect("ok")
            .expect("视图")
            .entries[0]
            .issues
            .iter()
            .cloned(),
    );
    let view = document
        .fdf_embedded_fdfs_with_password(b"other-pw")
        .expect("ok")
        .expect("视图");
    collected.extend(view.entries[0].issues.iter().cloned());
    let joined = collected.join("|");
    assert!(
        !joined.contains("leak-check-pw"),
        "口令不得出现在诊断里：{joined}"
    );
    let hex: String = key.iter().map(|byte| format!("{byte:02x}")).collect();
    assert!(!joined.contains(&hex), "密钥字节不得出现在诊断里：{joined}");
}

#[test]
fn the_password_reaches_encrypted_entries_in_deeper_recursion_levels() {
    // 口令逐层下传：外层条目解密后，子文档自己的 `/EncryptionRevision 1`
    // 载体同样被解密并类型化（四道防护照常生效）。
    let password = b"layered-pw";
    let leaf = inner_fdf_bytes("deep-leaf");
    let cipher =
        crypto::rc4_transform(crypto::derive_key(password).as_slice(), &leaf).expect("5 字节密钥");
    let mut mid = Vec::new();
    mid.extend_from_slice(
        b"1 0 obj\n<< /FDF << /Status (mid) /EmbeddedFDFs [ 2 0 R ] >> >>\nendobj\n",
    );
    mid.extend_from_slice(b"2 0 obj\n<< /F (leaf.fdf) /EF 3 0 R >>\nendobj\n");
    mid.extend_from_slice(
        format!(
            "3 0 obj\n<< /Length {} /EncryptionRevision 1 >>\nstream\n",
            cipher.len()
        )
        .as_bytes(),
    );
    mid.extend_from_slice(cipher.as_slice());
    mid.extend_from_slice(b"\nendstream\nendobj\n%%EOF\n");
    let document = encrypted_fdf_document(&mid, password, 1);
    let view = document
        .fdf_embedded_fdfs_with_password(password)
        .expect("ok")
        .expect("视图");
    let entry = first_entry(&view);
    assert_eq!(
        entry
            .document
            .as_ref()
            .expect("外层解密后有子文档")
            .fdf
            .status
            .as_deref(),
        Some("mid")
    );
    let nested = entry.nested.as_ref().expect("子文档的 /EmbeddedFDFs 视图");
    let grand = &nested.entries[0];
    assert!(grand.is_encrypted(), "孙层载体也声明 /EncryptionRevision");
    assert!(grand.is_decrypted(), "同一口令逐层下传后解密");
    assert_eq!(
        grand
            .document
            .as_ref()
            .expect("孙层解密后有子文档")
            .fdf
            .status
            .as_deref(),
        Some("deep-leaf")
    );
    // 旧入口在最内层就停下来：外层能解、内层记「无口令」诊断。
    let legacy_view = embedded_of(&document);
    let legacy = first_entry(&legacy_view);
    assert!(legacy.document.is_none(), "旧入口连外层都不解");
    let joined = legacy.issues.join("|");
    assert!(joined.contains("no user password"), "got {joined}");
}
