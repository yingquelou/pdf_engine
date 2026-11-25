//! 写端签名增量更新（12.8，基于 E7 增量写出）。
//!
//! 入口为 [`crate::core::writer::DocumentWriter::sign_incremental`]。语义：
//! - 在文件式、未加密、≥1.3 的宿主 `Document` 上追加一个 `FT /Sig`
//!   字段（无 `/AcroForm` 时新建并挂目录 `/SigFlags`）；
//! - `/Contents` 先写等长十六进制占位，`/ByteRange` 数字经定点迭代
//!   重算直到布局不动点收敛（数字宽度会移动占位偏移）；
//! - 收敛后对覆盖字节段计算摘要，交给调用方签名器产出签名数据，
//!   以等长十六进制原位回填占位——回填不移动任何字节偏移；
//! - CMS 结构在本层、密码学不在本层（12.8 F1.1/F1.2）：`SigningSpec` 可注入
//!   [`CmsSignBackend`]，由本层构造 detached CMS SignedData（signedAttrs
//!   `messageDigest` 取自覆盖字节、签名值与证书集来自后端）；未注入后端时
//!   签名器返回什么字节就回填什么，但 `/SubFilter` 必须与 `/Contents` 载荷
//!   自洽，CMS 类子过滤器只接受可被解析为 CMS 的载荷；
//!   签名值计算、证书链与信任判定一律经后端注入；
//! - 零字节契约：全过程先缓冲，任一步失败返回 `Err` 且不写出；
//!   宿主 `Document` 的缓存会被追加签名对象（与 E7 对象手术用法一致）。

use crate::core::reader::signature::{
    CmsSignedData, DigestAlgorithm, SignatureBackend, SignatureSubFilter,
};
use crate::core::types::{Array, Dict, Document, Object, PdfFeature, Value};

use super::DocumentWriter;
use super::WriteError;

mod cms;

pub use cms::{CmsBuildError, CmsSignBackend, CmsSignerId, CmsSignerMaterial};
use cms::{build_detached_cms, signed_attrs_der};

/// 定点迭代上限：ByteRange 数字宽度至多把布局偏移推动几轮。
const MAX_PASSES: usize = 8;

/// `/Contents` 载荷类型（12.8 F1.1 自洽校验）。
///
/// `/SubFilter` 是载荷类型的声明，二者必须相符，否则写出的是结构不成立
/// 的签名字典（例如把裸摘要挂到 `adbe.pkcs7.detached` 上）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentsPayload {
    /// 签名器原始输出字节：裸摘要或 PKCS#1 签名字节，不是 CMS DER。
    Raw,
    /// DER 编码的 detached CMS SignedData；写出前经读端解析器做结构校验。
    CmsDetached,
}

/// `/SubFilter` 是否接受给定的 `/Contents` 载荷类型。
///
/// `adbe.pkcs7.*` / `ETSI.CAdES.detached` / `ETSI.RFC3161` 都是 CMS 类，
/// 只接受 CMS 载荷；`adbe.x509.rsa_sha1` 的 `/Contents` 是原始签名字节；
/// 自定义名称由调用方自行负责，不在此做臆断。
fn sub_filter_accepts_payload(sub_filter: &SignatureSubFilter, payload: ContentsPayload) -> bool {
    match sub_filter {
        SignatureSubFilter::Pkcs7Sha1
        | SignatureSubFilter::Pkcs7Detached
        | SignatureSubFilter::CadesDetached
        | SignatureSubFilter::Rfc3161 => payload == ContentsPayload::CmsDetached,
        SignatureSubFilter::X509RsaSha1 => payload == ContentsPayload::Raw,
        SignatureSubFilter::Unknown(_) => true,
    }
}

/// 写端签名增量写出的产物。
#[derive(Debug, Clone)]
pub struct SignedIncremental {
    /// 完整的新文件字节（源字节 + 签名增量段，`/Contents` 已回填）。
    pub bytes: Vec<u8>,
    /// 最终 `/ByteRange [o1 l1 o2 l2]`。
    pub byte_range: [i64; 4],
    /// 覆盖字段摘要所用算法。
    pub algorithm: DigestAlgorithm,
    /// 签名字典的对象号。
    pub signature_object: i64,
    /// Sig 字段（Widget）的对象号。
    pub field_object: i64,
}

/// 摘要回调：输入 `/ByteRange` 覆盖字节算出的摘要，输出签名字节。
type SignerFn = Box<dyn Fn(&[u8]) -> Vec<u8>>;

/// 签名参数（Table 257 写出子集 + 目录 `/SigFlags` + DocMDP 引用）。
pub struct SigningSpec {
    signer: SignerFn,
    digest_len: usize,
    algorithm: DigestAlgorithm,
    field_name: String,
    name: Option<String>,
    location: Option<String>,
    reason: Option<String>,
    date: Option<Value>,
    filter: String,
    sub_filter: Option<String>,
    contents_payload: ContentsPayload,
    cms_backend: Option<Box<dyn CmsSignBackend>>,
    sig_flags: i64,
    docmdp_permission: Option<i64>,
}

impl SigningSpec {
    /// 创建签名参数；`algorithm` 决定覆盖字段摘要，`digest_len` 声明
    /// 签名器输出字节数（裸摘要场景 = 摘要长度：Md5=16、Sha1=20、Sha256=32）。
    ///
    /// 默认载荷为 [`ContentsPayload::Raw`]（恒等签名器即裸摘要），且不写
    /// `/SubFilter`（该条目在 Table 257 中可选），因此不会声明与载荷矛盾
    /// 的 CMS 子过滤器。真实签名需显式声明 [`Self::with_contents_payload`]
    /// 与 [`Self::with_sub_filter`]。
    pub fn new(algorithm: DigestAlgorithm, digest_len: usize) -> Self {
        Self {
            signer: Box::new(move |digest| digest.to_vec()),
            digest_len,
            algorithm,
            field_name: "Signature1".to_string(),
            name: None,
            location: None,
            reason: None,
            date: None,
            filter: "Adobe.PPKLite".to_string(),
            sub_filter: None,
            contents_payload: ContentsPayload::Raw,
            cms_backend: None,
            sig_flags: 1,
            docmdp_permission: None,
        }
    }

    /// 注入签名器：输入覆盖字段的摘要，输出写入 `/Contents` 的原始字节
    /// （真实 PKCS#7 后端在此封装 DER）；`digest_len` 为输出长度，
    /// 决定占位宽度。
    #[must_use]
    pub fn with_signer(
        mut self,
        digest_len: usize,
        signer: impl Fn(&[u8]) -> Vec<u8> + 'static,
    ) -> Self {
        self.digest_len = digest_len;
        self.signer = Box::new(signer);
        self
    }

    /// Sig 字段的 `/T` 部分名（不得含 `.`，它是字段树分隔符）。
    #[must_use]
    pub fn with_field_name(mut self, name: impl Into<String>) -> Self {
        self.field_name = name.into();
        self
    }

    /// 签名字典 `/Name`。
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// 签名字典 `/Location`。
    #[must_use]
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    /// 签名字典 `/Reason`。
    #[must_use]
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// 签名字典 `/M` 签署日期（文本串值）。
    #[must_use]
    pub fn with_date(mut self, date: Value) -> Self {
        self.date = Some(date);
        self
    }

    /// `/Filter` 签名处理程序名。
    #[must_use]
    pub fn with_filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = filter.into();
        self
    }

    /// `/SubFilter` 子过滤器名（12.8.4.3 Table 258）。
    ///
    /// 必须与 [`Self::with_contents_payload`] 声明的载荷类型自洽：CMS 类
    /// 子过滤器只接受 [`ContentsPayload::CmsDetached`]，反之亦然。不调用
    /// 本方法则不写 `/SubFilter`（Table 257 中该条目可选）。
    #[must_use]
    pub fn with_sub_filter(mut self, sub_filter: impl Into<String>) -> Self {
        self.sub_filter = Some(sub_filter.into());
        self
    }

    /// 声明签名器输出的 `/Contents` 载荷类型，用于 `/SubFilter` 自洽校验。
    #[must_use]
    pub fn with_contents_payload(mut self, payload: ContentsPayload) -> Self {
        self.contents_payload = payload;
        self
    }

    /// 声明载荷为 detached CMS SignedData 并默认 `/SubFilter` 为
    /// `adbe.pkcs7.detached`（调用方可再 [`Self::with_sub_filter`] 覆盖）。
    #[must_use]
    pub fn with_cms_payload(mut self) -> Self {
        self.contents_payload = ContentsPayload::CmsDetached;
        if self.sub_filter.is_none() {
            self.sub_filter = Some("adbe.pkcs7.detached".to_string());
        }
        self
    }

    /// 注入 CMS 签名值后端（F1.2）：写端据覆盖字节摘要构造 signedAttrs，
    /// 调后端取签名值与证书集，再编排为 detached CMS SignedData 回填
    /// `/Contents`。隐式声明 CMS 载荷与 `adbe.pkcs7.detached` 子过滤器。
    ///
    /// `digest_len` 此时表示分配给 `/Contents` 的字节数（须 ≥ 构造出的 CMS
    /// 长度）；多出的尾部字节补零，读端 `CmsSignedData::parse` 按 PDF 惯例
    /// 接受尾随全零填充，因此占位宽度在定点迭代中保持稳定。
    #[must_use]
    pub fn with_cms_backend(mut self, backend: impl CmsSignBackend + 'static) -> Self {
        self.contents_payload = ContentsPayload::CmsDetached;
        if self.sub_filter.is_none() {
            self.sub_filter = Some("adbe.pkcs7.detached".to_string());
        }
        self.cms_backend = Some(Box::new(backend));
        self
    }

    /// 目录 `/SigFlags`（缺省 1；2=签名后仅允许增量追加）。
    #[must_use]
    pub fn with_sig_flags(mut self, flags: i64) -> Self {
        self.sig_flags = flags;
        self
    }

    /// 附加 DocMDP 变换引用（`/Reference` → `/TransformMethod /DocMDP`，
    /// `/TransformParams /P` 取 `permission` 1–3）。
    #[must_use]
    pub fn with_docmdp(mut self, permission: i64) -> Self {
        self.docmdp_permission = Some(permission);
        self
    }
}

impl DocumentWriter {
    /// 以增量更新写出一个签名字段并回填 `/Contents`（12.8.1）。
    ///
    /// 要求宿主为文件解析、未加密、源文件头版本 ≥1.3 的 `Document`
    /// （会先 `load_all_objects` 固定脏集，目标版本 = 源版本）。
    /// 成功返回完整新文件字节与最终 ByteRange；任何失败返回 `Err`，
    /// 不产出部分字节。
    pub fn sign_incremental(
        &self,
        doc: &Document,
        spec: &SigningSpec,
        backend: &dyn SignatureBackend,
    ) -> Result<SignedIncremental, WriteError> {
        if doc.is_encrypted() || doc.trailer().dict.get("Encrypt").is_some() {
            return Err(WriteError::EncryptedDocument);
        }
        if doc.source.is_none() {
            return Err(WriteError::InvalidObject(
                "signing requires a document parsed from a file".into(),
            ));
        }
        if !doc.source_version.supports(PdfFeature::DigitalSignatures) {
            return Err(WriteError::UnsupportedVersion(format!(
                "digital signatures require PDF 1.3, got {}",
                doc.source_version
            )));
        }
        if spec.digest_len == 0 {
            return Err(WriteError::InvalidObject(
                "signature digest length must be positive".into(),
            ));
        }
        // F1.1：/SubFilter 声明的载荷类型必须与 /Contents 载荷自洽。
        if let Some(name) = spec.sub_filter.as_deref() {
            let sub_filter = SignatureSubFilter::from_name(name);
            if !sub_filter_accepts_payload(&sub_filter, spec.contents_payload) {
                return Err(WriteError::InvalidObject(format!(
                    "signature /SubFilter {name} is incompatible with /Contents payload {:?}",
                    spec.contents_payload
                )));
            }
        }
        if spec.field_name.is_empty() || spec.field_name.contains('.') {
            return Err(WriteError::InvalidObject(format!(
                "invalid signature field name {:?}",
                spec.field_name
            )));
        }
        doc.load_all_objects().map_err(|error| {
            WriteError::InvalidObject(format!("failed to preload document: {error}"))
        })?;
        let catalog_num = match doc.trailer().root_value() {
            Value::Ref(num, _) => num,
            _ => {
                return Err(WriteError::InvalidObject(
                    "signing requires an indirect /Root catalog reference".into(),
                ));
            }
        };
        let snapshot = doc.objects_for_write();
        let existing: std::collections::BTreeSet<i64> =
            snapshot.iter().map(|object| object.num).collect();
        let mut free_numbers = FreeNumbers::new(&existing);
        let form_num = match catalog_field(&snapshot, catalog_num, "AcroForm") {
            Some(num) => num,
            None => free_numbers.next(),
        };
        let fields_num = if existing.contains(&form_num) {
            match catalog_field(&snapshot, form_num, "Fields") {
                Some(num) => num,
                None => free_numbers.next(),
            }
        } else {
            free_numbers.next()
        };
        let field_num = free_numbers.next();
        let signature_num = free_numbers.next();
        let plan = match &spec.docmdp_permission {
            Some(permission) => SignaturePlan {
                reference_num: Some(free_numbers.next()),
                params_num: Some(free_numbers.next()),
                permission: Some(*permission),
            },
            None => SignaturePlan::default(),
        };

        let placeholder_hex: String = std::iter::repeat_n('0', spec.digest_len * 2).collect();
        let mut previous: Option<[i64; 4]> = None;
        for _ in 0..MAX_PASSES {
            install_signature_objects(
                doc,
                &snapshot,
                spec,
                &plan,
                catalog_num,
                form_num,
                fields_num,
                field_num,
                signature_num,
                previous.unwrap_or([0, 0, 0, 0]),
                &placeholder_hex,
            );
            let mut buf: Vec<u8> = Vec::new();
            if let Err(error) = self.build_incremental(doc, &mut buf) {
                doc.warnings.borrow_mut().clear();
                return Err(error);
            }
            doc.warnings.borrow_mut().clear();
            let needle = format!("<{placeholder_hex}>");
            let at = find_unique(&buf, needle.as_bytes()).ok_or_else(|| {
                WriteError::InvalidObject(
                    "signature /Contents placeholder missing or ambiguous in output".into(),
                )
            })?;
            let placeholder_len = needle.len() as i64;
            let range = [
                0,
                at as i64,
                at as i64 + placeholder_len,
                buf.len() as i64 - at as i64 - placeholder_len,
            ];
            if previous == Some(range) {
                let (bytes, _filled) = fill_contents_placeholder(
                    buf,
                    range,
                    spec,
                    backend,
                    at,
                    placeholder_hex.as_str(),
                )?;
                return Ok(SignedIncremental {
                    bytes,
                    byte_range: range,
                    algorithm: spec.algorithm,
                    signature_object: signature_num,
                    field_object: field_num,
                });
            }
            previous = Some(range);
        }
        Err(WriteError::InvalidObject(
            "signature /ByteRange fixed point did not converge".into(),
        ))
    }
}

#[derive(Default)]
struct SignaturePlan {
    reference_num: Option<i64>,
    params_num: Option<i64>,
    permission: Option<i64>,
}

/// 从收敛后的缓冲回填签名数据（等长替换，不动偏移）。
/// 返回 `(缓冲, ())` 仅为保持调用处简洁。
fn fill_contents_placeholder(
    mut buf: Vec<u8>,
    range: [i64; 4],
    spec: &SigningSpec,
    backend: &dyn SignatureBackend,
    at: usize,
    placeholder_hex: &str,
) -> Result<(Vec<u8>, bool), WriteError> {
    let mut sink = backend.start(spec.algorithm);
    for (offset, len) in [(range[0], range[1]), (range[2], range[3])] {
        let mut cursor = offset as usize;
        let mut remaining = len as usize;
        while remaining > 0 {
            let take = remaining.min(64 * 1024);
            sink.push(&buf[cursor..cursor + take]);
            cursor += take;
            remaining -= take;
        }
    }
    let digest = sink.finish();
    let blob = if let Some(cms_signer) = spec.cms_backend.as_deref() {
        // F1.2：由本层构造 CMS，签名值与证书集来自后端。
        let attrs = signed_attrs_der(&digest);
        let material = cms_signer.sign(&attrs, spec.algorithm).map_err(|error| {
            WriteError::InvalidObject(format!("CMS signing backend failed: {error}"))
        })?;
        let cms = build_detached_cms(spec.algorithm, &digest, &material).map_err(|error| {
            WriteError::InvalidObject(format!("failed to construct CMS SignedData: {error}"))
        })?;
        CmsSignedData::parse(&cms).map_err(|error| {
            WriteError::InvalidObject(format!(
                "constructed CMS SignedData is not parseable: {error}"
            ))
        })?;
        if cms.len() > spec.digest_len {
            return Err(WriteError::InvalidObject(format!(
                "constructed CMS is {} bytes, exceeding the {}-byte /Contents slot",
                cms.len(),
                spec.digest_len
            )));
        }
        let mut padded = cms;
        padded.resize(spec.digest_len, 0);
        padded
    } else {
        (spec.signer)(&digest)
    };
    if spec.cms_backend.is_none() {
        if blob.len() != spec.digest_len {
            return Err(WriteError::InvalidObject(format!(
                "signer returned {} bytes, expected {} for the /Contents placeholder",
                blob.len(),
                spec.digest_len
            )));
        }
        // CMS 类载荷必须是可解析的 detached SignedData，杜绝「裸摘要冒充 CMS」。
        if spec.contents_payload == ContentsPayload::CmsDetached {
            CmsSignedData::parse(&blob).map_err(|error| {
                WriteError::InvalidObject(format!(
                    "signature /Contents payload is not valid CMS SignedData: {error}"
                ))
            })?;
        }
    }
    let encoded = hex_encode(&blob);
    let start = at + 1;
    buf[start..start + placeholder_hex.len()].copy_from_slice(encoded.as_bytes());
    Ok((buf, true))
}

fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0F) as usize] as char);
    }
    out
}

fn find_unique(hay: &[u8], needle: &[u8]) -> Option<usize> {
    let first = hay.windows(needle.len()).position(|w| w == needle)?;
    if hay[first + 1..].windows(needle.len()).any(|w| w == needle) {
        return None;
    }
    Some(first)
}

/// 从快照字典中取某键的间接引用对象号。
fn catalog_field(snapshot: &[Object], num: i64, key: &str) -> Option<i64> {
    let object = snapshot.iter().find(|object| object.num == num)?;
    let direct = match &object.value {
        Value::Dict(dict) => dict.get(key),
        _ => None,
    }
    .cloned()?;
    match direct {
        Value::Ref(target, _) => Some(target),
        _ => None,
    }
}

struct FreeNumbers {
    used: std::collections::BTreeSet<i64>,
    cursor: i64,
}

impl FreeNumbers {
    fn new(used: &std::collections::BTreeSet<i64>) -> Self {
        let max = used.iter().copied().max().unwrap_or(0);
        Self {
            used: used.clone(),
            cursor: max + 1,
        }
    }

    fn next(&mut self) -> i64 {
        loop {
            let num = self.cursor;
            self.cursor += 1;
            if num > 0 && !self.used.contains(&num) {
                self.used.insert(num);
                return num;
            }
        }
    }
}

/// 把签名相关对象装入宿主缓存：目录、`/AcroForm`、`/Fields` 数组、
/// Sig Widget 字段、签名字典（含占位 `/Contents` 与当前 `/ByteRange`）
/// 与可选 DocMDP 引用/参数字典。
#[allow(clippy::too_many_arguments)]
fn install_signature_objects(
    doc: &Document,
    snapshot: &[Object],
    spec: &SigningSpec,
    plan: &SignaturePlan,
    catalog_num: i64,
    form_num: i64,
    fields_num: i64,
    field_num: i64,
    signature_num: i64,
    range: [i64; 4],
    placeholder_hex: &str,
) {
    for object in snapshot {
        doc.set_object(object.clone());
    }
    let catalog_dict = snapshot
        .iter()
        .find(|object| object.num == catalog_num)
        .and_then(|object| object.value.as_dict())
        .cloned()
        .unwrap_or_default();
    doc.set_object(Object::new(
        catalog_num,
        0,
        Value::Dict(
            catalog_dict
                .insert("SigFlags", Value::Integer(spec.sig_flags))
                .insert("AcroForm", Value::Ref(form_num, 0)),
        ),
    ));

    let form_dict = snapshot
        .iter()
        .find(|object| object.num == form_num)
        .and_then(|object| object.value.as_dict())
        .cloned()
        .unwrap_or_default();
    // 既有字段条目：/Fields 为间接引用时取引用对象数组，为直接数组时
    // 直接取元素（新输出统一改为间接引用数组）。
    let existing_fields: Vec<Value> = match form_dict.get("Fields") {
        Some(Value::Array(array)) => array.0.clone(),
        Some(Value::Ref(num, _)) => snapshot
            .iter()
            .find(|object| object.num == *num)
            .and_then(|object| match &object.value {
                Value::Array(array) => Some(array.0.clone()),
                _ => None,
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let mut fields = Array::new();
    for entry in existing_fields {
        fields = fields.push(entry);
    }
    fields = fields.push(Value::Ref(field_num, 0));
    doc.set_object(Object::new(fields_num, 0, Value::Array(fields)));
    doc.set_object(Object::new(
        form_num,
        0,
        Value::Dict(form_dict.insert("Fields", Value::Ref(fields_num, 0))),
    ));

    doc.set_object(Object::new(
        field_num,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Annot".into()))
                .insert("Subtype", Value::Name("Widget".into()))
                .insert("FT", Value::Name("Sig".into()))
                .insert("T", Value::String(spec.field_name.clone()))
                .insert(
                    "Rect",
                    Value::Array(
                        Array::new()
                            .push(Value::Integer(0))
                            .push(Value::Integer(0))
                            .push(Value::Integer(0))
                            .push(Value::Integer(0)),
                    ),
                )
                .insert("V", Value::Ref(signature_num, 0)),
        ),
    ));

    let mut signature_dict = Dict::new()
        .insert("Type", Value::Name("Sig".into()))
        .insert("Filter", Value::Name(spec.filter.clone()))
        .insert(
            "ByteRange",
            Value::Array(range.iter().fold(Array::new(), |array, value| {
                array.push(Value::Integer(*value))
            })),
        )
        .insert("Contents", Value::HexString(placeholder_hex.into()));
    if let Some(name) = &spec.sub_filter {
        signature_dict = signature_dict.insert("SubFilter", Value::Name(name.clone()));
    }
    if let Some(name) = &spec.name {
        signature_dict = signature_dict.insert("Name", Value::String(name.clone()));
    }
    if let Some(location) = &spec.location {
        signature_dict = signature_dict.insert("Location", Value::String(location.clone()));
    }
    if let Some(reason) = &spec.reason {
        signature_dict = signature_dict.insert("Reason", Value::String(reason.clone()));
    }
    if let Some(date) = &spec.date {
        signature_dict = signature_dict.insert("M", date.clone());
    }
    if let Some(reference_num) = plan.reference_num {
        signature_dict = signature_dict.insert(
            "Reference",
            Value::Array(Array::new().push(Value::Ref(reference_num, 0))),
        );
    }
    doc.set_object(Object::new(signature_num, 0, Value::Dict(signature_dict)));

    if let (Some(reference_num), Some(params_num), Some(permission)) =
        (plan.reference_num, plan.params_num, plan.permission)
    {
        doc.set_object(Object::new(
            reference_num,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("SigReference".into()))
                    .insert("Data", Value::Ref(signature_num, 0))
                    .insert("TransformMethod", Value::Name("DocMDP".into()))
                    .insert("TransformParams", Value::Ref(params_num, 0)),
            ),
        ));
        doc.set_object(Object::new(
            params_num,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("TransformParams".into()))
                    .insert("P", Value::Integer(permission)),
            ),
        ));
    }
}

#[cfg(test)]
mod tests;
