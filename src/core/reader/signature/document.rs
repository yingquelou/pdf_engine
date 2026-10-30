//! `Document` 上的签名发现、ByteRange 校验、分段摘要与 `/Reference`/`/Perms` 读取。

use crate::core::reader::content::text_value;
use crate::core::reader::form::FieldType;
use crate::core::reader::navigation::{catalog, resolve};
use crate::core::types::{Document, Value};

use super::entries::{DigestAlgorithm, DocumentSignature, parse_byte_range};
use super::perms::{PermsEntry, PermsEntryName};
use super::reference::{
    DocmdpPermission, SignatureReference, TransformMethod, TransformParams, parse_transform_params,
};
use super::{DIGEST_CHUNK, Result, SignatureBackend, SignatureError, malformed};

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
            Some(item) => Some(super::entries::SignatureSubFilter::from_name(
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

    /// 目录 `/Perms`（12.8.5 Table 268）中指定条目的解析结果；
    /// 无 `/Perms` 或无该键返回 `None`。值经一层间接解析；可解析为
    /// 字典时校验 `/Type /Sig` 并附带 DocMDP 整数 `/P`（仅 `/DocMDP`
    /// 条目），否则签名字段为 `None`（如 `/SigFlags` 的整数值）。
    pub fn perms_entry(&self, name: PermsEntryName) -> Result<Option<PermsEntry>> {
        let cat = catalog(self).map_err(|_| SignatureError::CatalogMissing)?;
        let Some(perms_value) = cat.get("Perms") else {
            return Ok(None);
        };
        let perms_resolved =
            resolve(self, perms_value).map_err(|error| malformed(error.to_string()))?;
        let perms = perms_resolved
            .as_dict()
            .ok_or_else(|| malformed("/Perms is not a dictionary"))?;
        let key = name.as_str();
        let Some(raw) = perms.get(key) else {
            return Ok(None);
        };
        let resolved = resolve(self, raw).map_err(|error| malformed(error.to_string()))?;
        let mut signature = None;
        let mut permission = None;
        if let Some(dict) = resolved.as_dict()
            && dict.get("Type").and_then(|item| item.as_name()) == Some("Sig")
        {
            signature = Some(self.signature(raw)?);
            if name == PermsEntryName::DocumentPermissions
                && let Some(value) = dict.get("P").and_then(|item| item.as_integer())
            {
                permission = Some(DocmdpPermission::from_i64(value).ok_or_else(|| {
                    malformed(format!("/Perms /DocMDP /P must be 1..3, got {value}"))
                })?);
            }
        }
        Ok(Some(PermsEntry {
            name,
            raw: raw.clone(),
            signature,
            permission,
        }))
    }

    /// 目录 `/Perms /DocMDP` 的文档修改防护级别（12.8.4.3）；无则 `None`。
    pub fn perms_document_modification_permission(&self) -> Result<Option<DocmdpPermission>> {
        Ok(self
            .perms_entry(PermsEntryName::DocumentPermissions)?
            .and_then(|entry| entry.permission))
    }

    /// 目录 `/Perms /Sig` 数组（Table 268）的全部签名字典；无键返回空表。
    pub fn perms_signatures(&self) -> Result<Vec<DocumentSignature>> {
        let cat = catalog(self).map_err(|_| SignatureError::CatalogMissing)?;
        let Some(perms_value) = cat.get("Perms") else {
            return Ok(Vec::new());
        };
        let perms_resolved =
            resolve(self, perms_value).map_err(|error| malformed(error.to_string()))?;
        let perms = perms_resolved
            .as_dict()
            .ok_or_else(|| malformed("/Perms is not a dictionary"))?;
        let Some(raw) = perms.get("Sig") else {
            return Ok(Vec::new());
        };
        let resolved = resolve(self, raw).map_err(|error| malformed(error.to_string()))?;
        let array = resolved
            .as_array()
            .ok_or_else(|| malformed("/Perms /Sig must be an array of signature dictionaries"))?;
        let mut out = Vec::new();
        for item in &array.0 {
            out.push(self.signature(item)?);
        }
        Ok(out)
    }
}
