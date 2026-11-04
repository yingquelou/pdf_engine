//! 写端加密（ISO 32000-1 7.6.3 标准安全处理器）→ 本 crate 读端解密的往返，
//! 以及构建期类型化校验的零字节契约。
//!
//! 读端解密链路完全复用既有实现：`Parser::with_password` →
//! `SecurityHandler::authenticate`（Algorithm 6/7）→ `ObjectSecurity`
//! 的逐对象解密（Algorithm 1）。写端若有任何算法/字典偏差，这里会失败。

use super::*;
use pdf_engine::core::builder::{DocumentInfo, FileIdentifier, Font, PageSize, PdfBuilder};
use pdf_engine::core::writer::{StreamCompression, WriteError};

const USER_PASSWORD: &[u8] = b"user-pw";
const OWNER_PASSWORD: &[u8] = b"owner-pw";
const PAGE_TEXT: &str = "SecureText";
const INFO_TITLE: &str = "SecureMeta";

/// Table 22 全授予（位 3–32 为 1、位 1–2 为 0 的唯一形态）。
const ALL_ALLOWED: i32 = -4;
/// Table 22 NOTE 的例子：允许打印与复制，禁止改内容与注释（`/R 2`）。
const PRINT_AND_COPY: i32 = -44;

/// 单页 + 文本 + `/Info /Title` 的 1.4 文档（`PdfBuilder` 默认 1.7，
/// 而 7.5.7 禁止加密字典入对象流，故加密输出必须落在经典布局的版本）。
fn text_builder() -> PdfBuilder {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_4);
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, PAGE_TEXT);
    builder.set_document_info(DocumentInfo::new().title(INFO_TITLE));
    builder
}

/// 写出 `/V 1`（RC4-40 / `/R 2`）加密文档的字节。
fn write_rc4_40(user: &[u8], owner: Option<&[u8]>, permissions: i32) -> Vec<u8> {
    let mut builder = text_builder();
    let mut buf = Vec::new();
    builder
        .build_encrypted_rc4_40(&mut buf, user, owner, permissions)
        .expect("/V 1 加密输出应可写出");
    buf
}

/// 写出 `/V 2`（RC4-128 / `/R 3`）加密文档的字节。
fn write_rc4_128(user: &[u8], owner: Option<&[u8]>, permissions: i32, key_bits: u8) -> Vec<u8> {
    let mut builder = text_builder();
    let mut buf = Vec::new();
    builder
        .build_encrypted_rc4_128(&mut buf, user, owner, permissions, key_bits)
        .expect("/V 2 加密输出应可写出");
    buf
}

/// `/Info` 文本条目经逐对象解密后的还原值（读端把解密结果以十六进制串保留，
/// 与 `decrypt_value` 的 `HexString` 分支一致）。
fn info_title_bytes(doc: &Document) -> Vec<u8> {
    let info = doc.document_info().expect("/Info 字典应可读");
    match info.title {
        Some(Value::HexString(hex)) => decode_hex_upper(&hex),
        Some(value) => panic!("/Title 解密后应为十六进制串，实得 {value:?}"),
        None => panic!("/Title 应随文档信息写出"),
    }
}

/// 测试侧十六进制解码（大写，`hex_bytes` 的逆）。
fn decode_hex_upper(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2)
        .map(|index| u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap())
        .collect()
}

/// P1：`/V 1`（RC4-40）写出的文档由本 crate 读端解密，提取文本与明文相等。
#[test]
fn encrypted_rc4_40_roundtrip_extracts_plaintext_text() {
    let buf = write_rc4_40(USER_PASSWORD, Some(OWNER_PASSWORD), ALL_ALLOWED);
    let doc = parse_buf_with_password(&buf, USER_PASSWORD);

    assert!(doc.is_encrypted(), "读端应识别出加密处理器");
    assert_eq!(doc.extract_page_text(0).as_deref(), Some(PAGE_TEXT));
    assert_eq!(
        info_title_bytes(&doc),
        INFO_TITLE.as_bytes(),
        "/Info 字符串应与流内容一同解密"
    );

    let raw = String::from_utf8_lossy(&buf);
    assert!(
        !raw.contains(PAGE_TEXT) && !raw.contains(INFO_TITLE),
        "密文里不应残留明文内容"
    );
    assert!(
        raw.contains("/Filter /Standard"),
        "应写出标准安全处理器字典"
    );
    // Table 20：/Length「only if V is 2 or 3」——/V 1 的字典里没有该条目。
    assert!(
        raw.contains("/P -4 /R 2 /U") && raw.contains("/V 1 >>"),
        "加密字典应只带 Table 21 的 /R /O /U /P 条目：{raw}"
    );
}

/// P1：`/O`（Algorithm 3）生效——所有者口令同样能打开并解密。
#[test]
fn encrypted_rc4_40_owner_password_decrypts_document() {
    let buf = write_rc4_40(USER_PASSWORD, Some(OWNER_PASSWORD), ALL_ALLOWED);
    let doc = parse_buf_with_password(&buf, OWNER_PASSWORD);
    assert_eq!(doc.extract_page_text(0).as_deref(), Some(PAGE_TEXT));
}

/// P1：限制权限（`/P = -44`，打印 + 复制）仍按同一密钥解密，文本可读。
#[test]
fn encrypted_rc4_40_with_restricted_permissions_still_decrypts() {
    let buf = write_rc4_40(USER_PASSWORD, Some(OWNER_PASSWORD), PRINT_AND_COPY);
    let doc = parse_buf_with_password(&buf, USER_PASSWORD);
    assert_eq!(doc.extract_page_text(0).as_deref(), Some(PAGE_TEXT));
}

/// P1：错误用户口令必须被读端拒绝（Algorithm 6 比对 `/U`）。
#[test]
fn encrypted_rc4_40_wrong_user_password_fails_to_open() {
    let buf = write_rc4_40(USER_PASSWORD, Some(OWNER_PASSWORD), ALL_ALLOWED);
    assert!(
        parse_buf_result(&buf, Some(b"wrong-pw")).is_err(),
        "错误口令不应通过 /U 校验"
    );
    assert!(
        parse_buf_result(&buf, Some(b"user-PW")).is_err(),
        "口令区分大小写"
    );
    assert!(
        parse_buf_result(&buf, None).is_err(),
        "非空用户口令的文档不应免密打开"
    );
}

/// P1：trailer `/ID` 是 Algorithm 2 e 的输入；显式 ID 与自动生成的 ID
/// 两条路径都要与盘上 `/ID` 一致，否则读端派生不出文件密钥。
#[test]
fn encrypted_rc4_40_honours_explicit_file_identifier() {
    let mut builder = text_builder();
    builder.set_file_identifier(FileIdentifier::new([0x11; 16], [0x22; 16]));
    let mut buf = Vec::new();
    builder
        .build_encrypted_rc4_40(&mut buf, USER_PASSWORD, Some(OWNER_PASSWORD), ALL_ALLOWED)
        .expect("显式 /ID 的加密输出应可写出");

    let doc = parse_buf_with_password(&buf, USER_PASSWORD);
    assert_eq!(doc.extract_page_text(0).as_deref(), Some(PAGE_TEXT));
    assert_eq!(
        doc.file_identifier().expect("/ID 应写进 trailer").0,
        [0x11; 16]
    );
}

/// P2：`/V 2`（RC4-128 / `/R 3`）往返；`/U` 走 Algorithm 5 的 20 次 RC4 链。
#[test]
fn encrypted_rc4_128_roundtrip_extracts_plaintext_text() {
    let buf = write_rc4_128(USER_PASSWORD, Some(OWNER_PASSWORD), ALL_ALLOWED, 128);
    let doc = parse_buf_with_password(&buf, USER_PASSWORD);

    assert_eq!(doc.extract_page_text(0).as_deref(), Some(PAGE_TEXT));
    assert_eq!(info_title_bytes(&doc), INFO_TITLE.as_bytes());
    assert!(
        !String::from_utf8_lossy(&buf).contains(PAGE_TEXT),
        "密文里不应残留明文内容"
    );

    assert!(
        parse_buf_result(&buf, Some(b"wrong-pw")).is_err(),
        "/V 2 下错误口令同样应被拒绝"
    );

    // Table 20：/V 2 才写 /Length；按字典键序紧跟 /Filter 之后。
    let raw = String::from_utf8_lossy(&buf);
    assert!(
        raw.contains("/Filter /Standard /Length 128 /O") && raw.contains("/P -4 /R 3 /U"),
        "/V 2 的加密字典应带 /Length 128 与 /R 3：{raw}"
    );
}

/// P2：Table 20 `/Length` 允许 40–128 的 8 的倍数；不同位宽下密钥与
/// `/U` 链长度都不同，读端须逐一对得上。
#[test]
fn encrypted_rc4_128_accepts_every_permitted_key_length() {
    for key_bits in [40u8, 48, 56, 64, 80, 96, 112, 128] {
        let buf = write_rc4_128(USER_PASSWORD, Some(OWNER_PASSWORD), ALL_ALLOWED, key_bits);
        let doc = parse_buf_with_password(&buf, USER_PASSWORD);
        assert_eq!(
            doc.extract_page_text(0).as_deref(),
            Some(PAGE_TEXT),
            "{key_bits} 位 RC4 密钥应能解密"
        );
    }
}

/// P2：revision 3 才解释的权限位（Table 22 位 9–12）被清零时，文档必须
/// 以 `/V 2`（`/R 3`）写出；`/V 1` 下这类 `/P` 被类型化校验拒绝。
#[test]
fn cleared_revision_3_permission_bits_need_handler_version_two() {
    // 位 9（填写表单）清零：0xFFFFFFFC & !0x0F00 | 其余保留位不变。
    let permissions: i32 = -0x104;
    let mut builder = text_builder();
    let mut rejected = Vec::new();
    let error = builder
        .build_encrypted_rc4_40(
            &mut rejected,
            USER_PASSWORD,
            Some(OWNER_PASSWORD),
            permissions,
        )
        .expect_err("Table 21 规定 /R 2 不得置零 revision 3 权限位");
    assert!(rejected.is_empty(), "拒绝时不得写出任何字节");
    assert!(
        error.to_string().contains("/R 2"),
        "错误应指明 revision 约束：{error}"
    );

    let buf = write_rc4_128(USER_PASSWORD, Some(OWNER_PASSWORD), permissions, 128);
    let doc = parse_buf_with_password(&buf, USER_PASSWORD);
    assert_eq!(doc.extract_page_text(0).as_deref(), Some(PAGE_TEXT));
}

/// P2：空口令（本 API 不提供免密写入口）。
#[test]
fn empty_password_is_rejected_without_bytes() {
    let mut builder = text_builder();
    let mut buf = Vec::new();
    let error = builder
        .build_encrypted_rc4_40(&mut buf, b"", Some(OWNER_PASSWORD), ALL_ALLOWED)
        .expect_err("空用户口令应被拒");
    assert!(buf.is_empty(), "拒绝时不得写出任何字节");
    assert!(
        error.to_string().contains("empty"),
        "错误应说明空口令：{error}"
    );

    let mut builder = text_builder();
    let mut buf = Vec::new();
    assert!(
        builder
            .build_encrypted_rc4_40(&mut buf, USER_PASSWORD, Some(b""), ALL_ALLOWED)
            .is_err(),
        "显式给出的空所有者口令同样应被拒"
    );
    assert!(buf.is_empty());
}

/// P2：超长口令（库策略 127 字节；ISO 只规定截断/补齐到 32 字节）。
#[test]
fn oversized_password_is_rejected_without_bytes() {
    let long = vec![b'p'; 128];
    let mut builder = text_builder();
    let mut buf = Vec::new();
    let error = builder
        .build_encrypted_rc4_40(&mut buf, &long, Some(OWNER_PASSWORD), ALL_ALLOWED)
        .expect_err("超过 127 字节的口令应被拒");
    assert!(buf.is_empty());
    assert!(error.to_string().contains("127"), "错误应给出上限：{error}");

    // 恰好 127 字节在策略内（Algorithm 2 a 只取前 32 字节）。
    let boundary = vec![b'p'; 127];
    let buf = write_rc4_40(&boundary, Some(OWNER_PASSWORD), ALL_ALLOWED);
    let doc = parse_buf_with_password(&buf, &boundary);
    assert_eq!(doc.extract_page_text(0).as_deref(), Some(PAGE_TEXT));
}

/// P2：`/P` 的低序保留位（Table 22 位 1–2）必须为 0。
#[test]
fn permissions_low_reserved_bits_must_be_zero() {
    for permissions in [-1i32, -2] {
        let mut builder = text_builder();
        let mut buf = Vec::new();
        let error = builder
            .build_encrypted_rc4_40(&mut buf, USER_PASSWORD, Some(OWNER_PASSWORD), permissions)
            .expect_err("/P 的位 1-2 属保留位，置 1 应被拒");
        assert!(buf.is_empty(), "{permissions:#010x} 拒绝时不得写出字节");
        assert!(
            error.to_string().contains("bit positions 1-2"),
            "错误应指明保留位：{error}"
        );
    }
}

/// P2：`/P` 的高序保留位（位 25–32）必须全为 1，即写成负整数
/// （Table 22 NOTE）。
#[test]
fn permissions_high_reserved_bits_must_be_one() {
    for permissions in [4i32, 0xFFFF_FFFC_u32 as i32 - 0x0100_0000, 0x7FFF_FFFC] {
        let mut builder = text_builder();
        let mut buf = Vec::new();
        let error = builder
            .build_encrypted_rc4_40(&mut buf, USER_PASSWORD, Some(OWNER_PASSWORD), permissions)
            .expect_err("高位保留位为 0 的 /P 应被拒");
        assert!(buf.is_empty());
        assert!(
            error.to_string().contains("negative integer"),
            "错误应引用 Table 22 NOTE：{error}"
        );
    }
}

/// P2：限制权限却没给所有者口令时拒绝（否则 Algorithm 3 a 用用户口令
/// 代替，权限形同虚设）。
#[test]
fn restrictive_permissions_require_an_owner_password() {
    let mut builder = text_builder();
    let mut buf = Vec::new();
    let error = builder
        .build_encrypted_rc4_40(&mut buf, USER_PASSWORD, None, PRINT_AND_COPY)
        .expect_err("限制 /P 却没有所有者口令应被拒");
    assert!(buf.is_empty());
    assert!(
        error.to_string().contains("owner password"),
        "错误应指向所有者口令：{error}"
    );

    // `/P` 全授予时允许省略所有者口令（Algorithm 3 a 用用户口令代替）。
    let buf = write_rc4_40(USER_PASSWORD, None, ALL_ALLOWED);
    let doc = parse_buf_with_password(&buf, USER_PASSWORD);
    assert_eq!(doc.extract_page_text(0).as_deref(), Some(PAGE_TEXT));
}

/// P2：Table 20 `/Length` 语法（40–128 且为 8 的倍数）。
#[test]
fn invalid_encryption_key_length_is_rejected() {
    for key_bits in [39u8, 41, 63, 136, 255] {
        let mut builder = text_builder();
        let mut buf = Vec::new();
        let error = builder
            .build_encrypted_rc4_128(
                &mut buf,
                USER_PASSWORD,
                Some(OWNER_PASSWORD),
                ALL_ALLOWED,
                key_bits,
            )
            .expect_err("{key_bits} 位不是合法的 /Length");
        assert!(buf.is_empty());
        assert!(
            error.to_string().contains("multiple of 8"),
            "错误应引用 /Length 约束：{error}"
        );
    }
}

/// 写出 `/V 4 /R 4`（`StdCF`，`CFM /V2`，即 Table 25 的 RC4 筛选器）文档。
fn write_v4_rc4v2(user: &[u8], owner: Option<&[u8]>, permissions: i32, key_bits: u8) -> Vec<u8> {
    let mut builder = text_builder();
    let mut buf = Vec::new();
    builder
        .build_encrypted_v4_rc4v2(&mut buf, user, owner, permissions, key_bits)
        .expect("/V 4 CFM /V2 加密输出应可写出");
    buf
}

/// 验收主用例：`/V 4 /CF` 写出的文档由**本 crate 读端**重解析、以用户口令
/// 认证（Algorithm 6）、按 `/StmF` `/StrF` 解析到 `StdCF`（7.6.5）并解密后，
/// 提取文本与 `/Info` 字符串同明文一致；字典带 Table 20/25 的 `StdCF` 条目
/// 与选择器，catalog 带 Table 28 的 `/Version /1.5` 名字条目（7.5.2 覆盖
/// header）。
///
/// **`CFM /AESV2` 没有写端入口**：Table 25（raw L3340—L3342）要求 CBC 的
/// 初始化向量「shall be randomly generated and placed as the first 16
/// bytes」，本 crate 无 CSPRNG 依赖也不新增依赖，故构造层不产出该 `CFM`；
/// `/AESV2` 的**解密**侧覆盖见 `tests/main/filters_indirect.rs` 与
/// `tests/main/crypto.rs`。
#[test]
fn encrypted_v4_roundtrip_extracts_plaintext_through_cf_filters() {
    let buf = write_v4_rc4v2(USER_PASSWORD, Some(OWNER_PASSWORD), ALL_ALLOWED, 128);
    let doc = parse_buf_with_password(&buf, USER_PASSWORD);

    assert!(doc.is_encrypted(), "读端应识别出加密处理器");
    assert_eq!(doc.extract_page_text(0).as_deref(), Some(PAGE_TEXT));
    assert_eq!(info_title_bytes(&doc), INFO_TITLE.as_bytes());
    let raw = String::from_utf8_lossy(&buf);
    assert!(
        !raw.contains(PAGE_TEXT) && !raw.contains(INFO_TITLE),
        "密文里不应残留明文内容"
    );
    assert!(
        raw.contains("/V 4") && raw.contains("/R 4"),
        "字典应为 /V 4 /R 4：{raw}"
    );
    assert!(
        raw.contains("/CF") && raw.contains("/StdCF") && raw.contains("/CFM /V2"),
        "应写出 /CF 的 StdCF（CFM /V2）：{raw}"
    );
    assert!(
        raw.contains("/StmF /StdCF") && raw.contains("/StrF /StdCF"),
        "应写出 /StmF /StrF 选择器：{raw}"
    );
    assert!(raw.contains("/Version /1.5"), "catalog 应声明 /1.5：{raw}");
}

/// 镜像用例：`/V 4` 文档上错误口令必须被拒绝（Algorithm 6 比对 `/U`），
/// 不得静默产出乱码；所有者口令则按 Algorithm 7 恢复用户口令后解密成功。
#[test]
fn encrypted_v4_wrong_password_fails_and_owner_password_succeeds() {
    let buf = write_v4_rc4v2(USER_PASSWORD, Some(OWNER_PASSWORD), ALL_ALLOWED, 128);
    for wrong in [b"wrong-pw".as_slice(), b"user-PW".as_slice()] {
        assert!(
            parse_buf_result(&buf, Some(wrong)).is_err(),
            "错误口令不应通过 /U 校验"
        );
    }
    assert!(
        parse_buf_result(&buf, None).is_err(),
        "非空用户口令的文档不应免密打开"
    );
    let doc = parse_buf_with_password(&buf, OWNER_PASSWORD);
    assert_eq!(doc.extract_page_text(0).as_deref(), Some(PAGE_TEXT));
}

/// Table 21：`/R 4` 下 Table 22 位 9–12 可自由清零——`/V 4` 的字典能表达
/// `/V ≤ 3` 表达不了的受限权限，且读端按同一 `/P` 认证成功解密。
#[test]
fn encrypted_v4_rc4v2_carries_revision_four_permission_bits() {
    let restrictive: i32 = -0x104; // 位 9 清零：只有 /R ≥ 3 的字典能表达。
    let buf = write_v4_rc4v2(USER_PASSWORD, Some(OWNER_PASSWORD), restrictive, 128);
    let doc = parse_buf_with_password(&buf, USER_PASSWORD);
    assert_eq!(doc.extract_page_text(0).as_deref(), Some(PAGE_TEXT));
    assert_eq!(info_title_bytes(&doc), INFO_TITLE.as_bytes());
}

/// Table 20 `/Length` + Table 25：`/V 4` 的 RC4 筛选器在 40–128 全部合法
/// 位宽下都能被读端按 `StdCF` 的字节制 `Length` 还原并解密。
#[test]
fn encrypted_v4_rc4v2_accepts_every_permitted_key_length() {
    for key_bits in [40u8, 48, 56, 64, 80, 96, 112, 128] {
        let buf = write_v4_rc4v2(USER_PASSWORD, Some(OWNER_PASSWORD), ALL_ALLOWED, key_bits);
        let doc = parse_buf_with_password(&buf, USER_PASSWORD);
        assert_eq!(
            doc.extract_page_text(0).as_deref(),
            Some(PAGE_TEXT),
            "/V 4 下 {key_bits} 位 CFM /V2 密钥应能解密"
        );
    }
}

/// `/V 4` 的目标版本门：低于 1.4 时 catalog `/Version` 覆盖机制不存在
/// （7.5.2「Beginning with PDF 1.4」），高于 1.4 时对象流布局与 7.5.7
/// 冲突，两端都应在任何字节写出前返回 [`WriteError::UnsupportedVersion`]。
#[test]
fn encrypted_v4_output_is_locked_to_the_classic_layout_with_version_override() {
    for version in [PdfVersion::V1_3, PdfVersion::V1_7] {
        let mut builder = PdfBuilder::with_version(version);
        let page = builder.page(PageSize::A4);
        builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, PAGE_TEXT);
        let mut buf = Vec::new();
        let error = builder
            .build_encrypted_v4_rc4v2(
                &mut buf,
                USER_PASSWORD,
                Some(OWNER_PASSWORD),
                ALL_ALLOWED,
                128,
            )
            .expect_err("目标版本 {version} 不应接受 /V 4 加密输出");
        assert!(buf.is_empty(), "版本门禁拒绝时不得写出字节");
        assert!(
            matches!(error, WriteError::UnsupportedVersion(_)),
            "应返回类型化的版本错误：{error:?}"
        );
    }
}

/// Table 20 `/Length` 的语法门在 `/V 4` 侧同样生效：非 8 的倍数或越出
/// 40–128 的 `key_bits` 在任何字节写出前零字节拒绝。
#[test]
fn v4_rc4v2_still_validates_the_key_length_syntax() {
    for key_bits in [39u8, 41, 136] {
        let mut builder = text_builder();
        let mut buf = Vec::new();
        let error = builder
            .build_encrypted_v4_rc4v2(
                &mut buf,
                USER_PASSWORD,
                Some(OWNER_PASSWORD),
                ALL_ALLOWED,
                key_bits,
            )
            .expect_err("{key_bits} 位不是合法的 /Length");
        assert!(buf.is_empty());
        assert!(
            error.to_string().contains("multiple of 8"),
            "错误应引用 /Length 约束：{error}"
        );
    }
}

/// 目标版本门：加密是 PDF 1.1 特性（7.6.1），`/V 2` 是 PDF 1.4 特性
/// （Table 20），而 PDF 1.5 起本 writer 的对象流布局与 7.5.7 冲突。
#[test]
fn encryption_is_version_gated() {
    for version in [PdfVersion::V1_0, PdfVersion::V1_7] {
        let mut builder = PdfBuilder::with_version(version);
        let page = builder.page(PageSize::A4);
        builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, PAGE_TEXT);
        let mut buf = Vec::new();
        let error = builder
            .build_encrypted_rc4_40(&mut buf, USER_PASSWORD, Some(OWNER_PASSWORD), ALL_ALLOWED)
            .expect_err("目标版本 {version} 不应接受加密输出");
        assert!(buf.is_empty(), "版本门禁拒绝时不得写出字节");
        assert!(
            matches!(error, WriteError::UnsupportedVersion(_)),
            "应返回类型化的版本错误：{error:?}"
        );
    }

    // /V 2 需要 1.4：1.3 下被拒。
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_3);
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, PAGE_TEXT);
    let mut buf = Vec::new();
    assert!(
        builder
            .build_encrypted_rc4_128(
                &mut buf,
                USER_PASSWORD,
                Some(OWNER_PASSWORD),
                ALL_ALLOWED,
                128,
            )
            .is_err()
    );
    assert!(buf.is_empty());
}

/// 压缩与逐对象加密不可共存：读端先解密再解 `/Filter`，故在写出前拒绝。
#[test]
fn compression_and_encryption_are_mutually_exclusive() {
    let mut builder = text_builder();
    builder.compress_streams(StreamCompression::flate());
    let mut buf = Vec::new();
    let error = builder
        .build_encrypted_rc4_40(&mut buf, USER_PASSWORD, Some(OWNER_PASSWORD), ALL_ALLOWED)
        .expect_err("压缩文档不应走加密写出");
    assert!(buf.is_empty(), "拒绝时不得写出任何字节");
    assert!(
        matches!(error, WriteError::Compression(_)),
        "应返回类型化的压缩错误：{error:?}"
    );
}
