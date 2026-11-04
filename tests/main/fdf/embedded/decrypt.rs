//! 12.7.7.3 的 **FDF 侧解密**集成往返：内层 FDF 明文字节 → 用派生密钥
//! RC4 加密 → 包进 `/EncryptionRevision 1` 的 EFS 文件规格 → 外层 FDF
//! 手写字节 → **真实 Parser** → [`Document::fdf_embedded_fdfs_with_password`]
//! 拿到解密后照常类型化的子文档（字段、`/Differences`、递归层都可读）。
//!
//! 密钥按规范独立算出（不经被测代码）：raw L24893–L24900「revision 1
//! (the only one defined) uses a similar RC4 encryption algorithm based on a
//! 40-bit encryption key … identical to steps (a) and (b) of the "Algorithm
//! 2: Computing an encryption key" in 7.6.3.3 … the first 5 bytes of the
//! result shall be the encryption key」，步 (a) 的 32 字节填充串见 raw
//! L3015–L3018 + L3054–L3055；Table 244（raw L24935–L24939）里
//! `EncryptionRevision` 是「加密时必需」的整数，且发布时唯一被定义的值是 1。

use super::*;

/// 7.6.3.3 步 (a) 的填充串（raw L3054–L3055）。
const PADDING: [u8; 32] = [
    0x28, 0xbf, 0x4e, 0x5e, 0x4e, 0x75, 0x8a, 0x41, 0x64, 0x00, 0x4e, 0x56, 0xff, 0xfa, 0x01, 0x08,
    0x2e, 0x2e, 0x00, 0xb6, 0xd0, 0x68, 0x3e, 0x80, 0x2f, 0x0c, 0xa9, 0xfe, 0x64, 0x53, 0x69, 0x7a,
];

/// 独立实现：口令截断/填充到 32 字节后取 MD5 摘要的前 5 字节（步 (a)+(b)）。
fn fdf_key(password: &[u8]) -> [u8; 5] {
    let head = &password[..password.len().min(32)];
    let mut padded = head.to_vec();
    padded.extend_from_slice(&PADDING[..32 - padded.len()]);
    let digest = Md5::digest(&padded);
    let mut key = [0u8; 5];
    key.copy_from_slice(&digest[..5]);
    key
}

/// RC4 解扰（加解密同一运算）。
fn rc4_5(key: &[u8], input: &[u8]) -> Vec<u8> {
    let mut cipher = Rc4::<U5>::new_from_slice(key).expect("5 字节密钥");
    let mut output = input.to_vec();
    cipher.apply_keystream(&mut output);
    output
}

/// 一份「外层 FDF + 一个 `/EncryptionRevision 1` 的加密嵌入式 FDF 条目」
/// 的文件字节：`plain` 是内层明文字节，`password` 决定密钥。
fn encrypted_outer_fdf(plain: &[u8], password: &[u8], name: &str) -> Vec<u8> {
    let cipher = rc4_5(&fdf_key(password), plain);
    let mut file = FdfFile::new("%FDF-1.4\n");
    file.object(1, b"<< /FDF 2 0 R >>");
    file.object(2, b"<< /Status (outer) /EmbeddedFDFs [ 3 0 R ] >>");
    file.object(3, filespec_body(name, 4).as_bytes());
    file.object(
        4,
        &stream_body(&cipher, "/Type /EmbeddedFile /EncryptionRevision 1"),
    );
    file.finish(1)
}

#[test]
fn embedded_fdfs_revision_one_carrier_decrypts_into_a_typed_child_document() {
    // 新入口解密 → 走与未加密条目**完全相同**的类型化路径；旧入口（无
    // 口令）依旧只识别不解密，载体原值始终是密文。
    let password = b"s3cr3t-pw";
    let plain = inner_fdf_bytes("secret-kid", "total", "2024");
    let document = parse_buf(&encrypted_outer_fdf(&plain, password, "crypto.fdf"));
    let cipher_bytes = rc4_5(&fdf_key(password), &plain);

    // 旧入口：行为与本切片之前一致（不解密 + 诊断 + 子文档 None）。
    let legacy_view = document.fdf_embedded_fdfs().expect("ok").expect("视图");
    let legacy = legacy_view.entries.first().expect("一个条目");
    assert!(legacy.is_encrypted());
    assert!(!legacy.is_decrypted());
    assert!(legacy.document.is_none());
    assert!(
        legacy
            .issues
            .join("|")
            .contains("supplied no user password"),
        "got {:?}",
        legacy.issues
    );

    // 新入口：解密后子文档字段可读。
    let view = document
        .fdf_embedded_fdfs_with_password(password)
        .expect("带口令读端不应报错")
        .expect("条目存在时应类型化");
    let entry = view.entries.first().expect("一个条目");
    assert_eq!(entry.encryption_revision, Some(1));
    assert!(entry.is_decrypted(), "本条目的明文来自 FDF 侧解密");
    assert!(
        entry.issues.is_empty(),
        "解密成功不该记诊断：{:?}",
        entry.issues
    );
    let child = entry.document.as_ref().expect("解密后应递归出子文档");
    assert_eq!(child.fdf.status.as_deref(), Some("secret-kid"));
    assert_eq!(
        child.field("total").expect("子字段按全限定名可查").value,
        Some(Value::String("2024".into())),
        "解密后的字段树照常类型化"
    );
    assert!(view.document("crypto.fdf").is_some(), "按名查找照常可用");
    assert_eq!(
        entry.data().expect("载体字节"),
        cipher_bytes.as_slice(),
        "文件里的原值仍是密文（原值永不丢弃）"
    );

    // 空口令同样是一把合法钥匙（步 (a) 的整串填充，raw L3015–L3018）。
    let zero_plain = {
        let mut file = FdfFile::new("%FDF-1.4\n");
        file.object(1, b"<< /FDF 2 0 R >>");
        file.object(2, b"<< /Status (outer) /EmbeddedFDFs [ 3 0 R ] >>");
        file.object(3, filespec_body("empty-pw.fdf", 4).as_bytes());
        let cipher = rc4_5(&fdf_key(b""), &plain);
        file.object(
            4,
            &stream_body(&cipher, "/Type /EmbeddedFile /EncryptionRevision 1"),
        );
        file.finish(1)
    };
    let zero = parse_buf(&zero_plain);
    let zero_entry = zero
        .fdf_embedded_fdfs_with_password("")
        .expect("空口令也合法")
        .expect("视图")
        .entries
        .remove(0);
    assert!(zero_entry.is_decrypted());
    assert_eq!(
        zero_entry
            .document
            .as_ref()
            .expect("空口令解密后有子文档")
            .fdf
            .status
            .as_deref(),
        Some("secret-kid")
    );
}

#[test]
fn embedded_fdfs_decrypted_child_keeps_its_differences_and_nested_levels() {
    // 内层明文字节自带 `/Differences`（引用形态）与一层**明文**
    // `/EmbeddedFDFs`：证明解密后的明文走的是一条完整既有路径（递归
    // 子文档 + `/Differences` 视图 + 逐层类型化）。
    let password = b"round-trip-pw";
    let kid = inner_fdf_bytes("kid", "deep", "1");
    let mut inner = FdfFile::new("%FDF-1.4\n");
    inner.object(1, b"<< /FDF 2 0 R >>");
    inner.object(
        2,
        b"<< /Status (middle) /Differences 3 0 R /EmbeddedFDFs [ 4 0 R ] >>",
    );
    inner.object(3, &stream_body(b"delta-bytes", "/Custom (kept)"));
    inner.object(4, filespec_body("kid.fdf", 5).as_bytes());
    inner.object(5, &stream_body(&kid, "/Type /EmbeddedFile"));
    let inner_bytes = inner.finish(1);

    let mut file = FdfFile::new("%FDF-1.4\n");
    file.object(1, b"<< /FDF 2 0 R >>");
    file.object(2, b"<< /Status (outer) /EmbeddedFDFs [ 3 0 R ] >>");
    file.object(3, filespec_body("encrypted.fdf", 4).as_bytes());
    file.object(
        4,
        &stream_body(
            &rc4_5(&fdf_key(password), &inner_bytes),
            "/Type /EmbeddedFile /EncryptionRevision 1",
        ),
    );
    let document = parse_buf(&file.finish(1));
    let view = document
        .fdf_embedded_fdfs_with_password(password)
        .expect("解密读端不应报错")
        .expect("视图");
    let entry = view.entries.first().expect("一个条目");
    let child = entry.document.as_ref().expect("解密后有子文档");
    assert_eq!(child.fdf.status.as_deref(), Some("middle"));
    assert_eq!(
        child.fdf.differences,
        Some(Value::Ref(3, 0)),
        "子模型的 `/Differences` 原值不丢"
    );
    let differences = entry
        .differences
        .as_ref()
        .expect("解密后的子文档 /Differences 应就地类型化");
    assert!(differences.is_stream());
    assert_eq!(differences.reference, Some((3, 0)));
    assert_eq!(
        differences
            .decoded_bytes(&document)
            .expect("字节已内联可读"),
        b"delta-bytes"
    );
    let nested = entry.nested.as_ref().expect("解密后的递归层视图");
    let grand = nested.entries.first().expect("孙条目");
    assert_eq!(grand.name(), Some("kid.fdf"));
    assert!(!grand.is_encrypted(), "孙层载体未声明 /EncryptionRevision");
    assert_eq!(
        grand
            .document
            .as_ref()
            .expect("孙文档照常递归")
            .fdf
            .status
            .as_deref(),
        Some("kid")
    );
    assert!(entry.issues.is_empty(), "got {:?}", entry.issues);
}

#[test]
fn embedded_fdfs_password_reaches_encrypted_entries_in_deeper_levels() {
    // 口令逐层下传：外层与孙层都是 `/EncryptionRevision 1`，两层都解密。
    let password = b"layered-pw";
    let leaf = inner_fdf_bytes("deep-leaf", "k", "v");
    let mut mid = FdfFile::new("%FDF-1.4\n");
    mid.object(1, b"<< /FDF 2 0 R >>");
    mid.object(2, b"<< /Status (middle) /EmbeddedFDFs [ 3 0 R ] >>");
    mid.object(3, filespec_body("leaf.fdf", 4).as_bytes());
    mid.object(
        4,
        &stream_body(
            &rc4_5(&fdf_key(password), &leaf),
            "/Type /EmbeddedFile /EncryptionRevision 1",
        ),
    );
    let document = parse_buf(&encrypted_outer_fdf(&mid.finish(1), password, "top.fdf"));
    let view = document
        .fdf_embedded_fdfs_with_password(password)
        .expect("ok")
        .expect("视图");
    let entry = view.entries.first().expect("外层条目");
    assert!(entry.is_decrypted());
    let nested = entry.nested.as_ref().expect("中间层的 /EmbeddedFDFs 视图");
    let grand = nested.entries.first().expect("孙条目");
    assert!(grand.is_encrypted(), "孙层载体声明 /EncryptionRevision 1");
    assert!(grand.is_decrypted(), "同一口令逐层下传后解密");
    assert_eq!(
        grand
            .document
            .as_ref()
            .expect("孙文档解密后有子文档")
            .fdf
            .status
            .as_deref(),
        Some("deep-leaf")
    );

    // 对照：旧入口连最外层都不解，自然也没有递归层。
    let legacy = document
        .fdf_embedded_fdfs()
        .expect("ok")
        .expect("视图")
        .entries
        .remove(0);
    assert!(legacy.document.is_none());
    assert!(legacy.nested.is_none());
}

#[test]
fn embedded_fdfs_non_one_revision_is_reported_and_never_decrypted() {
    // Table 244 raw L24935–L24939：发布时唯一被定义的值是 1 ⇒ 其余值
    // **不 Attempt 解密**，即使给了口令（此处载体放的是明文，若误解密
    // 反而会毁掉可解析的字节）。
    let password = b"whatever-pw";
    let plain = inner_fdf_bytes("plain", "k", "v");
    let mut file = FdfFile::new("%FDF-1.4\n");
    file.object(1, b"<< /FDF 2 0 R >>");
    file.object(2, b"<< /Status (outer) /EmbeddedFDFs [ 3 0 R ] >>");
    file.object(3, filespec_body("rev-two.fdf", 4).as_bytes());
    file.object(
        4,
        &stream_body(&plain, "/Type /EmbeddedFile /EncryptionRevision 2"),
    );
    let document = parse_buf(&file.finish(1));
    let view = document
        .fdf_embedded_fdfs_with_password(password)
        .expect("非 1 的 revision 也不报错")
        .expect("视图");
    let entry = view.entries.first().expect("一个条目");
    assert_eq!(entry.encryption_revision, Some(2));
    assert!(!entry.encryption_revision_supported());
    assert!(!entry.is_decrypted(), "revision 2 无已定义算法，故不解密");
    assert!(
        entry.document.is_none(),
        "未解密的条目不当作明文递归（内容其实是明文，这是规范允许的保守读法）"
    );
    let joined = entry.issues.join("|");
    assert!(
        joined.contains("not the only defined value 1"),
        "got {joined}"
    );
    assert!(
        joined.contains("Table 244 raw L24937-L24939"),
        "诊断留可追溯的规范行号：{joined}"
    );
    assert_eq!(entry.data().expect("原值保留"), plain.as_slice());
}

#[test]
fn embedded_fdfs_wrong_password_degrades_without_failing_the_real_file() {
    // 错口令 → 明文是噪声 → 「跳过 + 记录诊断 + 保留原值」：不 panic、
    // 外层 FDF 的其余读模型不受影响。
    let plain = inner_fdf_bytes("not-this-one", "k", "v");
    let document = parse_buf(&encrypted_outer_fdf(&plain, b"right-pw", "crypto.fdf"));
    let view = document
        .fdf_embedded_fdfs_with_password(b"wrong-pw")
        .expect("错口令也不应报错")
        .expect("视图");
    let entry = view.entries.first().expect("一个条目");
    assert!(entry.is_encrypted());
    assert!(entry.document.is_none(), "错口令不该产出子文档");
    assert!(entry.nested.is_none());
    assert!(entry.differences.is_none());
    let joined = entry.issues.join("|");
    assert!(joined.contains("password may be incorrect"), "got {joined}");
    // 同份文件的 FDF 目录/FDF 字典照常类型化。
    let fdf = document.fdf().expect("ok").expect("FDF");
    assert_eq!(fdf.fdf.status.as_deref(), Some("outer"));
    assert_eq!(
        entry.data().expect("密文原值仍在"),
        rc4_5(&fdf_key(b"right-pw"), &plain).as_slice()
    );
}

#[test]
fn embedded_fdfs_decryption_diagnostics_never_leak_the_key_or_password() {
    // 安全：口令与派生密钥都不进诊断（诊断会被调用方直接打印/记日志）。
    let password = b"leak-check-pw";
    let key = fdf_key(password);
    let hex: String = key.iter().map(|byte| format!("{byte:02x}")).collect();
    let plain = inner_fdf_bytes("leak", "k", "v");
    let document = parse_buf(&encrypted_outer_fdf(&plain, password, "crypto.fdf"));
    let mut collected = Vec::new();
    for view in [
        document.fdf_embedded_fdfs().expect("ok").expect("视图"),
        document
            .fdf_embedded_fdfs_with_password(password)
            .expect("ok")
            .expect("视图"),
        document
            .fdf_embedded_fdfs_with_password(b"other-pw")
            .expect("ok")
            .expect("视图"),
    ] {
        collected.extend(view.issues.iter().cloned());
        for entry in &view.entries {
            collected.extend(entry.issues.iter().cloned());
        }
    }
    let joined = collected.join("|");
    assert!(
        !joined.contains("leak-check-pw"),
        "口令不得进诊断：{joined}"
    );
    assert!(!joined.contains(&hex), "密钥字节不得进诊断：{joined}");
    assert!(!joined.contains("other-pw"), "got {joined}");
}

#[test]
fn embedded_fdfs_unencrypted_carriers_ignore_the_password_entirely() {
    // 未声明 `/EncryptionRevision` 的条目：两个入口结果逐字相同，
    // 口令不参与（写端产出的内层 FDF 也照常递归）。
    let inner = writer_inner_fdf_bytes("from writer", "name", "Ada");
    let mut file = FdfFile::new("%FDF-1.4\n");
    file.object(1, b"<< /FDF 2 0 R >>");
    file.object(2, b"<< /Status (outer) /EmbeddedFDFs [ 3 0 R ] >>");
    file.object(3, filespec_body("plain.fdf", 4).as_bytes());
    file.object(4, &stream_body(&inner, "/Type /EmbeddedFile"));
    let document = parse_buf(&file.finish(1));
    let legacy = document.fdf_embedded_fdfs().expect("ok").expect("视图");
    let keyed = document
        .fdf_embedded_fdfs_with_password(b"irrelevant")
        .expect("ok")
        .expect("视图");
    assert_eq!(keyed, legacy, "明文条目的视图完全相同");
    assert!(!keyed.entries.first().expect("条目").is_decrypted());
    assert_eq!(
        keyed
            .document("plain.fdf")
            .expect("按名查找子文档")
            .fdf
            .status
            .as_deref(),
        Some("from writer")
    );
}
