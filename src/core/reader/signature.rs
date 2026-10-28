//! 数字签名字典读取（12.8.1–12.8.4）。
//!
//! - [`Document::document_signatures`]：沿目录 `/AcroForm` 字段树收集
//!   `/FT /Sig` 字段的 `/V` 签名字典（Table 257）。
//! - [`Document::signature`]：解析任意签名字典（直接字典或间接引用）。
//! - ByteRange（`[offset1 len1 offset2 len2]`）结构校验与覆盖范围；
//!   覆盖字段的分段摘要经 [`SignatureBackend`] 计算（默认
//!   [`RustCryptoSignatureBackend`]：MD5/SHA-1/SHA-256），PKCS#7
//!   语法与证书链验证不在本层（签名后端可插入更完整的验证器）。
//! - 防护：摘要要求文件式 `Document`（源字节可读）；ByteRange 必须
//!   单调、留正间隔给 `/Contents` 占位且不越过文件末尾。
//! - 变换引用（12.8.2–12.8.4，Table 252–257）：`/Reference` 数组的
//!   签名字典引用条目建模为 [`SignatureReference`]，`/TransformParams`
//!   按 `/TransformMethod` 解析为 [`TransformParams`]
//!   （DocMDP / FieldMDP / UR）。

use std::fmt;

use md5::Md5;
use sha1::Sha1;
use sha2::Sha256;

use crate::core::reader::content::text_value;
use crate::core::reader::form::FieldType;
use crate::core::reader::navigation::{catalog, resolve};
use crate::core::types::{Dict, Document, Value};

/// 单段读取的分块大小（与增量写出复制源字节的分块一致）。
const DIGEST_CHUNK: usize = 64 * 1024;

/// 签名读取中的类型化错误。
#[derive(Debug, Clone, PartialEq)]
pub enum SignatureError {
    /// 文档目录缺失或无法解析为字典。
    CatalogMissing,
    /// 签名字典、ByteRange 或 SubFilter 条目结构损坏。
    Malformed(String),
    /// 计算覆盖字节摘要要求从文件解析、保留源字节的 `Document`。
    NoSource,
}

impl fmt::Display for SignatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CatalogMissing => write!(f, "document catalog is missing or malformed"),
            Self::Malformed(message) => write!(f, "{message}"),
            Self::NoSource => {
                write!(f, "signature digest requires a file-backed document")
            }
        }
    }
}

impl std::error::Error for SignatureError {}

type Result<T> = std::result::Result<T, SignatureError>;

fn malformed(message: impl Into<String>) -> SignatureError {
    SignatureError::Malformed(message.into())
}

/// 摘要算法（Table 258 SubFilter 与后端能力的交集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestAlgorithm {
    /// 遗留 `/R <= 2` 签名用 MD5。
    Md5,
    /// `adbe.pkcs7.sha1` 与 `adbe.x509.rsa_sha1`。
    Sha1,
    /// detached PKCS#7 / CAdES 的当前推荐。
    Sha256,
}

/// 签名字典 `/SubFilter`（12.8.4.3 Table 258）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureSubFilter {
    /// `adbe.pkcs7.sha1`：Contents 内嵌完整 PKCS#7（含签名数据）。
    Pkcs7Sha1,
    /// `adbe.pkcs7.detached`：Contents 内嵌 detached PKCS#7。
    Pkcs7Detached,
    /// `adbe.x509.rsa_sha1`（遗留形态）。
    X509RsaSha1,
    /// `ETSI.CAdES.detached`：CAdES detached。
    CadesDetached,
    /// `ETSI.RFC3161`：时间戳签名。
    Rfc3161,
    /// 其他/自定义名称，原样保留。
    Unknown(String),
}

impl SignatureSubFilter {
    /// 由 `/SubFilter` 名称解析。
    pub fn from_name(name: &str) -> Self {
        match name {
            "adbe.pkcs7.sha1" => Self::Pkcs7Sha1,
            "adbe.pkcs7.detached" => Self::Pkcs7Detached,
            "adbe.x509.rsa_sha1" => Self::X509RsaSha1,
            "ETSI.CAdES.detached" => Self::CadesDetached,
            "ETSI.RFC3161" => Self::Rfc3161,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// 名称（不含前导 `/`）。
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pkcs7Sha1 => "adbe.pkcs7.sha1",
            Self::Pkcs7Detached => "adbe.pkcs7.detached",
            Self::X509RsaSha1 => "adbe.x509.rsa_sha1",
            Self::CadesDetached => "ETSI.CAdES.detached",
            Self::Rfc3161 => "ETSI.RFC3161",
            Self::Unknown(name) => name,
        }
    }

    /// 名称隐含的默认摘要算法；detached 形态取决于 PKCS#7 内部，返回 `None`。
    pub fn default_digest(&self) -> Option<DigestAlgorithm> {
        match self {
            Self::Pkcs7Sha1 | Self::X509RsaSha1 => Some(DigestAlgorithm::Sha1),
            _ => None,
        }
    }
}

/// 签名字典（12.8.4.1 Table 257，`/Type /Sig`）。
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentSignature {
    /// 签名字典对象的间接引用；直接嵌入时为 `None`。
    pub reference: Option<(i64, i64)>,
    /// `/Filter` 签名处理程序名（如 `Adobe.PPKLite`）。
    pub filter: Option<String>,
    /// `/SubFilter` 子过滤器。
    pub sub_filter: Option<SignatureSubFilter>,
    /// `/ByteRange [offset1 len1 offset2 len2]`。
    pub byte_range: [i64; 4],
    /// `/Contents` 原始值（占位十六进制/字面串）。
    pub contents: Option<Value>,
    /// `/Name` 签署者名（文本串解码）。
    pub name: Option<String>,
    /// `/Location` 签署地点。
    pub location: Option<String>,
    /// `/Reason` 签署原因。
    pub reason: Option<String>,
    /// `/ContactInfo` 联系方式。
    pub contact_info: Option<String>,
    /// `/M` 签署日期（文本串原样保留）。
    pub date: Option<Value>,
    /// `/Reference` 变换/时间戳引用数组（原样保留）。
    pub references: Option<Value>,
    /// `/PropBuild` 等遗留字段不进模型；完整字典经此访问。
    pub dict: Dict,
}

impl DocumentSignature {
    /// ByteRange 覆盖的两段 `(offset, len)`；已通过结构校验。
    pub fn covered_ranges(&self) -> [(u64, u64); 2] {
        [
            (self.byte_range[0] as u64, self.byte_range[1] as u64),
            (self.byte_range[2] as u64, self.byte_range[3] as u64),
        ]
    }

    /// 覆盖总字节数（两段长度之和）。
    pub fn covered_len(&self) -> u64 {
        self.byte_range[1].max(0) as u64 + self.byte_range[3].max(0) as u64
    }

    /// `/Contents` 为十六进制串时返回其解码后的占位字节数。
    pub fn contents_len(&self) -> Option<usize> {
        match self.contents.as_ref() {
            Some(Value::HexString(hex)) => Some(hex.len() / 2),
            _ => None,
        }
    }
}

/// 变换方法（Table 252 与 12.8.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformMethod {
    /// `/DocMDP`：文档级修改防护（12.8.4.3）。
    Docmdp,
    /// `/FieldMDP`：字段级修改防护（12.8.4.2）。
    Fieldmdp,
    /// `/UR`：用户权限（12.8.4.1）。
    Ur,
}

impl TransformMethod {
    /// 由 `/TransformMethod` 名称解析。
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "DocMDP" => Some(Self::Docmdp),
            "FieldMDP" => Some(Self::Fieldmdp),
            "UR" => Some(Self::Ur),
            _ => None,
        }
    }

    /// 名称（不含前导 `/`）。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Docmdp => "DocMDP",
            Self::Fieldmdp => "FieldMDP",
            Self::Ur => "UR",
        }
    }
}

/// 文档修改防护权限（Table 253 `/P`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocmdpPermission {
    /// 1：仅允许填表单与签名字段（最严格）。
    NoChanges,
    /// 2：在 1 之外还可创建/删除注解、填表单、签名。
    FillAndAnnotate,
    /// 3：在 2 之外还可添加/删除页面与页面注释。
    FillAnnotateAndForm,
}

impl DocmdpPermission {
    /// 由 `/P` 整数解析；越界返回 `None`。
    pub fn from_i64(value: i64) -> Option<Self> {
        match value {
            1 => Some(Self::NoChanges),
            2 => Some(Self::FillAndAnnotate),
            3 => Some(Self::FillAnnotateAndForm),
            _ => None,
        }
    }

    /// `/P` 数值。
    pub fn as_i64(&self) -> i64 {
        match self {
            Self::NoChanges => 1,
            Self::FillAndAnnotate => 2,
            Self::FillAnnotateAndForm => 3,
        }
    }
}

/// 字段修改防护范围（Table 254 `/P`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldMdPermission {
    /// `/All`：除 `/FieldNames` 列出的字段外全部锁定。
    All,
    /// `/Include`：仅 `/FieldNames` 列出的字段可修改。
    Include,
    /// `/Exclude`：除 `/FieldNames` 外全部可修改。
    Exclude,
}

/// 用户权限作业范围（Table 255 `/Scope`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrScope {
    /// 是否含 `/DocAttachment`。
    pub doc_attachment: bool,
    /// 是否含 `/Annotation`。
    pub annotation: bool,
    /// 是否含 `/Form`。
    pub form: bool,
}

/// `/TransformParams` 字典（按 `/TransformMethod` 区分，Table 253–255）。
#[derive(Debug, Clone, PartialEq)]
pub enum TransformParams {
    /// DocMDP：`/P` 权限级别。
    Docmdp { permission: DocmdpPermission },
    /// FieldMDP：`/P` 范围、`/Annots` 目标与原始参数键。
    Fieldmdp {
        permission: FieldMdPermission,
        annotations: Vec<u8>,
        dict: Dict,
    },
    /// UR：作业范围、版本与原因等；未建模条目保留在 `dict`。
    Ur {
        version: i64,
        scope: UrScope,
        needs_appearances: bool,
        reason: Option<String>,
        permission: Option<i64>,
        v2: Option<i64>,
        dict: Dict,
    },
}

/// 签名字典引用条目（12.8.3 Table 256–257，`/Reference` 数组元素）。
#[derive(Debug, Clone, PartialEq)]
pub struct SignatureReference {
    /// 变换方法；缺失表示纯时间戳引用（本层不建模）。
    pub transform_method: Option<TransformMethod>,
    /// 已按方法校验的变换参数。
    pub params: Option<TransformParams>,
    /// `/Data` 引用的嵌入 PDF 流（原样保留，取流经 `data_reference`）。
    pub data: Option<Value>,
    /// `/DigestValue` 原始十六进制/字面串。
    pub digest_value: Option<Value>,
    /// `/DigestLength` 声明的摘要字节数。
    pub digest_length: Option<i64>,
    /// 完整引用字典；`/TransformParams` 的间接引用等经此访问。
    pub dict: Dict,
}

impl SignatureReference {
    /// `/Data` 若为间接引用则返回 `(num, gen)`。
    pub fn data_reference(&self) -> Option<(i64, i64)> {
        self.data.as_ref().and_then(|value| value.as_ref())
    }

    /// 声明的摘要长度与算法输出长度是否一致（算法未知返回 `None`）。
    pub fn matches_digest_length(&self, algorithm: DigestAlgorithm) -> Option<bool> {
        let declared = self.digest_length?;
        let actual = match algorithm {
            DigestAlgorithm::Md5 => 16,
            DigestAlgorithm::Sha1 => 20,
            DigestAlgorithm::Sha256 => 32,
        };
        Some(declared == actual)
    }
}

/// 增量摘要接收器：按块喂入覆盖字节，产出最终摘要。
pub trait DigestSink {
    /// 追加一块字节。
    fn push(&mut self, bytes: &[u8]);

    /// 结束并返回摘要。
    fn finish(self: Box<Self>) -> Vec<u8>;
}

/// 可替换的签名摘要后端（Roadmap F1：密码学经 trait 隔离）。
pub trait SignatureBackend {
    /// 为指定算法启动一个摘要累加器。
    fn start(&self, algorithm: DigestAlgorithm) -> Box<dyn DigestSink>;
}

struct GenericDigest<D: md5::Digest>(D);

impl<D: md5::Digest + Default> DigestSink for GenericDigest<D> {
    fn push(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    fn finish(self: Box<Self>) -> Vec<u8> {
        self.0.finalize().to_vec()
    }
}

/// 内置 RustCrypto 后端（md-5 / sha1 / sha2，digest 0.10 家族）。
#[derive(Debug, Clone, Copy, Default)]
pub struct RustCryptoSignatureBackend;

impl SignatureBackend for RustCryptoSignatureBackend {
    fn start(&self, algorithm: DigestAlgorithm) -> Box<dyn DigestSink> {
        match algorithm {
            DigestAlgorithm::Md5 => Box::new(GenericDigest(Md5::default())),
            DigestAlgorithm::Sha1 => Box::new(GenericDigest(Sha1::default())),
            DigestAlgorithm::Sha256 => Box::new(GenericDigest(Sha256::default())),
        }
    }
}

impl Document {
    /// 目录 `/SigFlags`（Table 246）；缺失返回 `None`。
    pub fn signature_flags(&self) -> Result<Option<i64>> {
        let cat = catalog(self).map_err(|_| SignatureError::CatalogMissing)?;
        match cat.get("SigFlags") {
            None => Ok(None),
            Some(value) => Some(
                value
                    .as_integer()
                    .ok_or_else(|| malformed("/SigFlags is not an integer")),
            )
            .transpose(),
        }
    }

    /// 收集目录 `/AcroForm` 字段树中全部 `/FT /Sig` 字段的签名字典。
    ///
    /// 只取字段自身字典的 `/V`（不读继承值，避免父节点重复）；`/V` 缺失
    /// 或非签名字典的字段被跳过（未签名字段是常态）。
    pub fn document_signatures(&self) -> Result<Vec<DocumentSignature>> {
        let form = self.acro_form().map_err(|error| match error {
            crate::core::reader::form::FormError::CatalogMissing => SignatureError::CatalogMissing,
            other => malformed(other.to_string()),
        })?;
        let mut out = Vec::new();
        if let Some(form) = form {
            self.collect_field_signatures(&form.fields, &mut out)?;
        }
        Ok(out)
    }

    fn collect_field_signatures(
        &self,
        fields: &[crate::core::reader::form::Field],
        out: &mut Vec<DocumentSignature>,
    ) -> Result<()> {
        for field in fields {
            if field.field_type == Some(FieldType::Signature)
                && let Some(value) = field.dict.get("V")
            {
                out.push(self.signature(value)?);
            }
            self.collect_field_signatures(&field.kids, out)?;
        }
        Ok(())
    }

    /// 解析单个签名字典值（直接字典或间接引用）。
    pub fn signature(&self, value: &Value) -> Result<DocumentSignature> {
        let reference = value.as_ref();
        let resolved = resolve(self, value).map_err(|error| malformed(error.to_string()))?;
        let dict = resolved
            .as_dict()
            .ok_or_else(|| malformed("signature value is not a dictionary"))?;
        if let Some(item) = dict.get("Type")
            && item.as_name() != Some("Sig")
        {
            return Err(malformed(format!(
                "signature dictionary has /Type /{}",
                item.as_name().unwrap_or("?")
            )));
        }
        let byte_range = parse_byte_range(dict.get("ByteRange"))?;
        let sub_filter = match dict.get("SubFilter") {
            None => None,
            Some(item) => Some(SignatureSubFilter::from_name(
                item.as_name()
                    .ok_or_else(|| malformed("/SubFilter of a signature is not a name"))?,
            )),
        };
        Ok(DocumentSignature {
            reference,
            filter: dict
                .get("Filter")
                .and_then(|item| item.as_name())
                .map(str::to_string),
            sub_filter,
            byte_range,
            contents: dict.get("Contents").cloned(),
            name: dict.get("Name").and_then(text_value),
            location: dict.get("Location").and_then(text_value),
            reason: dict.get("Reason").and_then(text_value),
            contact_info: dict.get("ContactInfo").and_then(text_value),
            date: dict.get("M").cloned(),
            references: dict.get("Reference").cloned(),
            dict: dict.clone(),
        })
    }

    /// ByteRange 相对当前源文件的结构校验，并返回覆盖两段。
    ///
    /// 要求：全部非负；第一段起点为 0（惯例，覆盖「文件头到 Contents」）；
    /// 两段单调且之间留出正间隔给 `/Contents` 占位；第二段不超过文件末尾。
    pub fn validate_signature_byte_range(
        &self,
        signature: &DocumentSignature,
    ) -> Result<[(u64, u64); 2]> {
        let [o1, l1, o2, l2] = signature.byte_range;
        if o1 != 0 {
            return Err(malformed(format!(
                "/ByteRange first offset must be 0, got {o1}"
            )));
        }
        if l1 <= 0 || l2 <= 0 || o2 <= o1 + l1 {
            return Err(malformed(format!(
                "/ByteRange {o1} {l1} {o2} {l2} is not monotonic with a positive gap"
            )));
        }
        let file_len = self.source_len().ok_or(SignatureError::NoSource)?;
        if o2 as u64 + l2 as u64 > file_len {
            return Err(malformed(format!(
                "/ByteRange extends to {} past end of file ({file_len})",
                o2 as u64 + l2 as u64
            )));
        }
        Ok(signature.covered_ranges())
    }

    /// 计算 ByteRange 覆盖字段的分段摘要（12.8.1）。
    ///
    /// 要求文件式 `Document`；按 [`DIGEST_CHUNK`] 分块读取，不整档驻留内存。
    /// `/Contents` 占位字节不在覆盖范围内，因此摘要对「先写占位、后填
    /// Contents」的签名流程稳定。
    pub fn signature_digest(
        &self,
        signature: &DocumentSignature,
        algorithm: DigestAlgorithm,
        backend: &dyn SignatureBackend,
    ) -> Result<Vec<u8>> {
        let ranges = self.validate_signature_byte_range(signature)?;
        let mut sink = backend.start(algorithm);
        for (offset, len) in ranges {
            let mut remaining = len;
            let mut cursor = offset;
            while remaining > 0 {
                let take = remaining.min(DIGEST_CHUNK as u64) as usize;
                let Some(bytes) = self.read_at(cursor, take) else {
                    return Err(malformed(format!(
                        "failed to read /ByteRange segment at {cursor} ({take} bytes)"
                    )));
                };
                if bytes.is_empty() {
                    return Err(malformed(format!(
                        "unexpected end of file inside /ByteRange segment at {cursor}"
                    )));
                }
                sink.push(&bytes);
                cursor += bytes.len() as u64;
                remaining -= bytes.len() as u64;
            }
        }
        Ok(sink.finish())
    }

    /// 解析签名字典 `/Reference` 数组的变换引用条目（12.8.3）。
    ///
    /// 无 `/TransformMethod` 的元素视为时间戳引用并跳过；参数字典
    /// 结构损坏返回类型化错误。
    pub fn signature_references(
        &self,
        signature: &DocumentSignature,
    ) -> Result<Vec<SignatureReference>> {
        let Some(value) = signature.references.as_ref() else {
            return Ok(Vec::new());
        };
        let resolved = resolve(self, value).map_err(|error| malformed(error.to_string()))?;
        let items = match &resolved {
            Value::Array(array) => &array.0,
            _ => return Err(malformed("/Reference must be an array")),
        };
        let mut out = Vec::new();
        for item in items {
            let reference = resolve(self, item).map_err(|error| malformed(error.to_string()))?;
            let dict = reference
                .as_dict()
                .ok_or_else(|| malformed("a /Reference entry is not a dictionary"))?;
            let Some(method_name) = dict.get("TransformMethod").and_then(|v| v.as_name()) else {
                continue;
            };
            let Some(method) = TransformMethod::from_name(method_name) else {
                return Err(malformed(format!(
                    "unknown /TransformMethod /{method_name}"
                )));
            };
            let params_value = dict
                .get("TransformParams")
                .ok_or_else(|| malformed("signature reference lacks /TransformParams"))?;
            let params_dict = {
                let resolved_params =
                    resolve(self, params_value).map_err(|error| malformed(error.to_string()))?;
                resolved_params
                    .as_dict()
                    .ok_or_else(|| malformed("/TransformParams is not a dictionary"))?
                    .clone()
            };
            if let Some(item_type) = params_dict.get("Type")
                && item_type.as_name() != Some("TransformParams")
            {
                return Err(malformed(format!(
                    "/TransformParams has /Type /{}",
                    item_type.as_name().unwrap_or("?")
                )));
            }
            let params = parse_transform_params(method, &params_dict)?;
            out.push(SignatureReference {
                transform_method: Some(method),
                params: Some(params),
                data: dict.get("Data").cloned(),
                digest_value: dict.get("DigestValue").cloned(),
                digest_length: dict.get("DigestLength").and_then(|v| v.as_integer()),
                dict: dict.clone(),
            });
        }
        Ok(out)
    }

    /// DocMDP 权限级别（12.8.4.3）：首个 DocMDP 引用的 `/P`；无则 `None`。
    pub fn doc_modification_permission(
        &self,
        signature: &DocumentSignature,
    ) -> Result<Option<DocmdpPermission>> {
        for reference in self.signature_references(signature)? {
            if let Some(TransformParams::Docmdp { permission }) = reference.params {
                return Ok(Some(permission));
            }
        }
        Ok(None)
    }

    /// FieldMDP 参数（12.8.4.2）：首个 FieldMDP 引用；无则 `None`。
    pub fn field_modification_params(
        &self,
        signature: &DocumentSignature,
    ) -> Result<Option<TransformParams>> {
        for reference in self.signature_references(signature)? {
            if let Some(params @ TransformParams::Fieldmdp { .. }) = reference.params {
                return Ok(Some(params));
            }
        }
        Ok(None)
    }
}

fn parse_transform_params(method: TransformMethod, dict: &Dict) -> Result<TransformParams> {
    match method {
        TransformMethod::Docmdp => {
            let value = dict
                .get("P")
                .and_then(|item| item.as_integer())
                .ok_or_else(|| malformed("DocMDP /TransformParams lacks integer /P"))?;
            let permission = DocmdpPermission::from_i64(value)
                .ok_or_else(|| malformed(format!("DocMDP /P must be 1..3, got {value}")))?;
            Ok(TransformParams::Docmdp { permission })
        }
        TransformMethod::Fieldmdp => {
            let name = dict
                .get("P")
                .and_then(|item| item.as_name())
                .ok_or_else(|| malformed("FieldMDP /TransformParams lacks name /P"))?;
            let permission = match name {
                "All" => FieldMdPermission::All,
                "Include" => FieldMdPermission::Include,
                "Exclude" => FieldMdPermission::Exclude,
                other => {
                    return Err(malformed(format!(
                        "FieldMDP /P must be All/Include/Exclude, got /{other}"
                    )));
                }
            };
            let annotations = dict
                .get("Annots")
                .map(|item| {
                    bits_string_bytes(item)
                        .ok_or_else(|| malformed("FieldMDP /Annots must be a bits string"))
                })
                .transpose()?
                .unwrap_or_default();
            Ok(TransformParams::Fieldmdp {
                permission,
                annotations,
                dict: dict.clone(),
            })
        }
        TransformMethod::Ur => {
            let version = match dict.get("V") {
                Some(item) => ur_version_value(item)
                    .ok_or_else(|| malformed("UR /V must be a version number"))?,
                None => return Err(malformed("UR /TransformParams lacks /V")),
            };
            let scope_names = match dict.get("Scope") {
                Some(item) => {
                    let array = item
                        .as_array()
                        .ok_or_else(|| malformed("UR /Scope must be an array of names"))?;
                    array
                        .0
                        .iter()
                        .map(|entry| {
                            entry
                                .as_name()
                                .ok_or_else(|| malformed("UR /Scope entries must be names"))
                        })
                        .collect::<Result<Vec<_>>>()?
                }
                None => return Err(malformed("UR /TransformParams lacks /Scope")),
            };
            let mut scope = UrScope {
                doc_attachment: false,
                annotation: false,
                form: false,
            };
            for entry in scope_names {
                match entry {
                    "DocAttachment" => scope.doc_attachment = true,
                    "Annotation" => scope.annotation = true,
                    "Form" => scope.form = true,
                    other => {
                        return Err(malformed(format!("unknown UR /Scope entry /{other}")));
                    }
                }
            }
            let needs_appearances = dict
                .get("NeedAppearances")
                .and_then(|item| match item {
                    Value::Bool(flag) => Some(*flag),
                    _ => None,
                })
                .unwrap_or(false);
            Ok(TransformParams::Ur {
                version,
                scope,
                needs_appearances,
                reason: dict.get("Reason").and_then(text_value),
                permission: dict.get("P").and_then(|item| item.as_integer()),
                v2: dict.get("V2").and_then(|item| item.as_integer()),
                dict: dict.clone(),
            })
        }
    }
}

fn ur_version_value(item: &Value) -> Option<i64> {
    match item {
        Value::Integer(number) => Some(*number),
        Value::Float(number) => (*number == 1.2_f64).then_some(12),
        Value::Name(name) => match name.as_str() {
            "1.2" => Some(12),
            "1.9" => Some(19),
            _ => None,
        },
        _ => None,
    }
}

/// 位串字符串（字面/十六进制/ASCII85）解码为字节。
fn bits_string_bytes(item: &Value) -> Option<Vec<u8>> {
    match item {
        Value::String(text) => Some(text.as_bytes().to_vec()),
        Value::HexString(hex) => {
            let mut padded = hex.clone();
            if padded.len() % 2 == 1 {
                padded.push('0');
            }
            (0..padded.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&padded[i..i + 2], 16).ok())
                .collect()
        }
        Value::Ascii85String(raw) => {
            let dict = Dict::new().insert("Filter", Value::Name("ASCII85Decode".into()));
            crate::core::filters::decode_stream(&dict, raw.as_bytes()).ok()
        }
        _ => None,
    }
}

fn parse_byte_range(value: Option<&Value>) -> Result<[i64; 4]> {
    let items = value
        .and_then(|item| match item {
            Value::Array(array) => Some(&array.0),
            _ => None,
        })
        .ok_or_else(|| malformed("/ByteRange must be a 4-integer array"))?;
    if items.len() != 4 {
        return Err(malformed(format!(
            "/ByteRange must have exactly 4 entries, got {}",
            items.len()
        )));
    }
    let mut range = [0i64; 4];
    for (slot, item) in range.iter_mut().zip(items) {
        let number = item
            .as_integer()
            .ok_or_else(|| malformed("/ByteRange entries must be integers"))?;
        if number < 0 {
            return Err(malformed(format!(
                "/ByteRange entry must be non-negative, got {number}"
            )));
        }
        *slot = number;
    }
    Ok(range)
}

#[cfg(test)]
mod tests;
