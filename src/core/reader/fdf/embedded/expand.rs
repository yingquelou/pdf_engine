//! `/Differences` 与 `/EmbeddedFDFs` 视图的**装配**部分：解引用、形态
//! 判定、7.11 文件规范复用、递归子 FDF 文档解析、Table 244
//! `/EncryptionRevision` 识别与**按 12.7.7.3 解密**（密钥派生与 RC4 见
//! [`super::crypto`]；未给口令或 revision ≠ 1 时一律「不解密 + 记诊断 +
//! 保留原值」）、PDF 1.4 版本语义报告，以及层数/单数组条目/跨层全局
//! 预算/环检测四道防护。
//!
//! 产出的形态类型在 [`super::view`]；本模块只负责把不可信输入降级成
//! 「类型化 + 诊断 + 原值保留」，任何畸形都不向上抛错（与
//! `/JavaScript`、`/Target` 的读端宽容风格一致）。

use std::collections::HashSet;
use zeroize::Zeroizing;

use crate::core::reader::embedded::{EmbeddedFile, Filespec};
use crate::core::reader::navigation::resolve;
use crate::core::types::{Document, PdfVersion, Value};

use super::super::{FdfDocument, parse};
use super::ExpansionBudget;
use super::crypto;
use super::scan;
use super::view::{FdfDifferences, FdfEmbeddedFdf, FdfEmbeddedFdfs};

/// 装配 `/Differences` 视图；条目缺失返回 `None`。
pub(super) fn differences_view(document: &Document, fdf: &FdfDocument) -> Option<FdfDifferences> {
    let raw = fdf.fdf.dict.get("Differences")?.clone();
    let reference = raw.as_ref();
    let mut issues = Vec::new();
    if let Some(problem) = pdf14_entry_problem(document, fdf, "Differences") {
        issues.push(problem);
    }
    let stream = match resolve(document, &raw) {
        Ok(value @ Value::Stream(_, _)) => Some(value),
        Ok(other) => {
            issues.push(format!(
                "FDF /Differences resolved to {} but Table 243 declares a stream; raw value kept",
                other.type_name()
            ));
            None
        }
        Err(error) => {
            issues.push(format!(
                "FDF /Differences reference could not be resolved: {error}; raw value kept"
            ));
            None
        }
    };
    Some(FdfDifferences {
        raw,
        reference,
        stream,
        issues,
    })
}

/// 装配 `/EmbeddedFDFs` 视图；条目缺失返回 `None`。`depth` 是递归层数
/// （上限 [`super::MAX_FDF_EMBEDDED_DEPTH`]），单次数组展开另有
/// [`super::MAX_FDF_EMBEDDED_ENTRIES`] 条目上限，整棵树共享 `budget` 的
/// [`super::MAX_FDF_EMBEDDED_TOTAL_ENTRIES`] 全局名额。
///
/// `password` 是**用户口令**（12.7.7.3 的 FDF 侧解密输入）：`None`
/// （旧入口 [`Document::fdf_embedded_fdfs`]）时加密条目一律不解密、只记
/// 诊断，行为与本切片之前完全一致；`Some` 时 `/EncryptionRevision 1`
/// 的载体流按 [`super::crypto`] 解扰后再走既有类型化路径，且口令沿递归
/// 逐层下传（子文档自己的加密条目同样可解）。
///
/// 环检测的作用域是**一份文档**：对象号只在其所属文件里唯一，父子两层
/// 各自出现 `2 0 R` 不是环路，故 `visited` 每次装配一份新数组时新建；
/// 同一数组里重复引用同一文件规范/载体流才判为环路。跨文档的自嵌套由
/// 层数上限与全局条目预算共同兜底（单靠层数只约束一条链，扇出相乘仍会
/// 放大）。
pub(super) fn embedded_view(
    document: &Document,
    fdf: &FdfDocument,
    depth: usize,
    budget: &mut ExpansionBudget,
    password: Option<&[u8]>,
) -> Option<FdfEmbeddedFdfs> {
    let raw = fdf.fdf.dict.get("EmbeddedFDFs")?.clone();
    let reference = raw.as_ref();
    let mut issues = Vec::new();
    if let Some(problem) = pdf14_entry_problem(document, fdf, "EmbeddedFDFs") {
        issues.push(problem);
    }
    let array = match resolve(document, &raw) {
        Ok(value) => match value {
            Value::Array(items) => items.0,
            other => {
                issues.push(format!(
                    "FDF /EmbeddedFDFs resolved to {} but Table 243 declares an array; raw value kept",
                    other.type_name()
                ));
                return Some(FdfEmbeddedFdfs {
                    raw: Some(raw),
                    reference,
                    entries: Vec::new(),
                    issues,
                });
            }
        },
        Err(error) => {
            issues.push(format!(
                "FDF /EmbeddedFDFs reference could not be resolved: {error}; raw value kept"
            ));
            return Some(FdfEmbeddedFdfs {
                raw: Some(raw),
                reference,
                entries: Vec::new(),
                issues,
            });
        }
    };
    // 不 with_capacity：`array.len()` 是不可信输入。
    let mut visited = HashSet::new();
    let mut entries = Vec::new();
    for item in array {
        if entries.len() >= super::MAX_FDF_EMBEDDED_ENTRIES {
            issues.push(format!(
                "FDF /EmbeddedFDFs holds more than {} entries; the tail was skipped",
                super::MAX_FDF_EMBEDDED_ENTRIES
            ));
            break;
        }
        if !budget.admit() {
            issues.push(format!(
                "FDF /EmbeddedFDFs expansion stopped: this call already typed \
                 {} entries across all recursion levels (global budget); the rest of this \
                 array is not typed and its raw value is kept",
                super::MAX_FDF_EMBEDDED_TOTAL_ENTRIES
            ));
            break;
        }
        entries.push(embedded_entry(
            document,
            item,
            depth,
            &mut visited,
            budget,
            password,
        ));
    }
    Some(FdfEmbeddedFdfs {
        raw: Some(raw),
        reference,
        entries,
        issues,
    })
}

/// 装配数组的一项：文件规范 → `/EF` 载体流 → Table 244
/// `/EncryptionRevision`（必要时按 12.7.7.3 解密，见
/// [`carrier_plaintext`]）→ 递归子 FDF 文档（连同子文档自己的
/// `/Differences` 视图，见 [`FdfEmbeddedFdf::differences`]）。畸形与超限
/// 一律降级为 `issues` 记录，`raw` 原值保留。
fn embedded_entry(
    document: &Document,
    raw: Value,
    depth: usize,
    visited: &mut HashSet<(i64, i64)>,
    budget: &mut ExpansionBudget,
    password: Option<&[u8]>,
) -> FdfEmbeddedFdf {
    let mut issues = Vec::new();
    let filespec = match document.filespec(&raw) {
        Ok(spec) => Some(spec),
        Err(error) => {
            issues.push(format!(
                "FDF /EmbeddedFDFs entry is not a readable file specification: {error}; raw value kept"
            ));
            None
        }
    };
    let carrier = filespec.as_ref().and_then(Filespec::primary);
    let encryption_revision = carrier_revision(carrier, &mut issues);
    let mut typed = TypedChild::default();
    let mut decrypted = false;
    if let Some(carrier) = carrier {
        if let Err(note) = recursion_guard(&filespec, carrier, depth, visited) {
            issues.push(note);
        } else {
            match carrier_plaintext(carrier, encryption_revision, password, &mut issues) {
                CarrierBytes::Plain(bytes) => {
                    typed = type_plaintext(bytes, depth, budget, password, &mut issues);
                }
                CarrierBytes::Decrypted(plain) => {
                    decrypted = true;
                    typed = type_plaintext(&plain, depth, budget, password, &mut issues);
                    if typed.document.is_none() {
                        // RC4 是流密码，错口令同样「解密成功」，只是明文
                        // 不可解析 ⇒ 额外给一条可操作的提示。
                        issues.push(
                            "FDF embedded FDF stream was unscrambled with the revision 1 key \
                             (12.7.7.3) but the plaintext does not type as an FDF file; the \
                             supplied user password may be incorrect"
                                .to_string(),
                        );
                    }
                }
                // 加密且未经解密：原因诊断已由 [`carrier_plaintext`] 记好，
                // 内容不当作明文递归（载体字节仍在 [`FdfEmbeddedFdf::data`]）。
                CarrierBytes::Undecrypted => {}
            }
        }
    } else {
        issues.push(
            "FDF /EmbeddedFDFs entry carries no /EF embedded file stream; only the file \
             specification is typed"
                .to_string(),
        );
    }
    FdfEmbeddedFdf {
        raw,
        filespec,
        encryption_revision,
        decrypted,
        document: typed.document,
        differences: typed.differences,
        nested: typed.nested,
        issues,
    }
}

/// 一个条目在**明文字节**可用时的类型化产物（子文档 + 就地装配的
/// `/Differences` 与递归 `/EmbeddedFDFs`）。
#[derive(Default)]
struct TypedChild {
    document: Option<Box<FdfDocument>>,
    differences: Option<Box<FdfDifferences>>,
    nested: Option<Box<FdfEmbeddedFdfs>>,
}

/// 明文字节的既有类型化路径（与未加密条目**完全同一条**）：宽容扫描 →
/// 子文档组装 → [`parse::fdf_document`] → 逐层递归与 `/Differences`。
/// 失败一律记诊断并返回空 [`TypedChild`]，绝不 panic。
///
/// 子文档完全由内层字节自证（[`scan::build_child_document`]），故本函数
/// 不需要外层 `Document`，也不再进入外层的 `visited` 环检测集合。
fn type_plaintext(
    bytes: &[u8],
    depth: usize,
    budget: &mut ExpansionBudget,
    password: Option<&[u8]>,
    issues: &mut Vec<String>,
) -> TypedChild {
    let scanned = scan::scan_objects(bytes);
    if scanned.truncated {
        issues.push(format!(
            "FDF embedded FDF stream holds more than {} indirect objects; the tail was \
             skipped",
            scan::MAX_FDF_EMBEDDED_OBJECTS
        ));
    }
    if scanned.objects.is_empty() {
        issues.push(
            "FDF embedded FDF stream yields no indirect objects; child document not parsed"
                .to_string(),
        );
        return TypedChild::default();
    }
    let Some(child) = scan::build_child_document(scanned, bytes, issues) else {
        issues.push(
            "FDF embedded FDF stream holds no catalog dictionary with /FDF (Table 242); \
             child document not parsed"
                .to_string(),
        );
        return TypedChild::default();
    };
    match parse::fdf_document(&child) {
        Ok(Some(view)) => {
            // 子文档的 `/Differences` 只能**就地**类型化：子
            // `Document` 不落进视图（它非 `Clone`，且
            // `FdfEmbeddedFdf` 要维持 `Clone + PartialEq`），
            // 否则调用方只拿到一个无处解引用的对象号。
            TypedChild {
                differences: differences_view(&child, &view).map(Box::new),
                nested: embedded_view(&child, &view, depth + 1, budget, password).map(Box::new),
                document: Some(Box::new(view)),
            }
        }
        Ok(None) => {
            issues.push(
                "FDF embedded FDF stream parses as a file without an /FDF catalog (Table 242); \
                 child document not parsed"
                    .to_string(),
            );
            TypedChild::default()
        }
        Err(error) => {
            issues.push(format!(
                "FDF embedded FDF stream could not be parsed as an FDF file: {error}; \
                 raw file specification kept"
            ));
            TypedChild::default()
        }
    }
}

/// 载体流（[`EmbeddedFile`]）字节与 Table 244 `/EncryptionRevision` 的
/// 关系判定，决定本条目是否**有明文字节可类型化**。
enum CarrierBytes<'a> {
    /// 流字典未声明 `/EncryptionRevision` ⇒ 未加密，载体字节即明文。
    Plain(&'a [u8]),
    /// `/EncryptionRevision 1` 且调用方给了口令 ⇒ 已按 12.7.7.3
    /// （MD5(步 (a) 填充口令) 前 5 字节 → RC4）解扰出的明文字节。
    Decrypted(Zeroizing<Vec<u8>>),
    /// 加密内容**未经解密**（未给口令、revision ≠ 1、或解扰失败）：
    /// 不产子文档，原因诊断已记入 `issues`，载体原字节保留。
    Undecrypted,
}

/// 见 [`CarrierBytes`]。`/Differences` 之类未声明 `/EncryptionRevision`
/// 的条目走 [`CarrierBytes::Plain`]，行为与本切片之前逐字一致。
fn carrier_plaintext<'a>(
    carrier: &'a EmbeddedFile,
    revision: Option<i64>,
    password: Option<&[u8]>,
    issues: &mut Vec<String>,
) -> CarrierBytes<'a> {
    let Some(revision) = revision else {
        return CarrierBytes::Plain(&carrier.data);
    };
    if revision != 1 {
        issues.push(format!(
            "FDF embedded FDF stream /EncryptionRevision {revision} is not the only \
             defined value 1 (Table 244 raw L24937-L24939)"
        ));
        issues.push(format!(
            "FDF embedded FDF stream is encrypted with /EncryptionRevision {revision} \
             (Table 244) but revision 1 is the only defined algorithm (12.7.7.3 raw \
             L24894-L24896), so its bytes are returned undecrypted and no child document \
             is parsed"
        ));
        return CarrierBytes::Undecrypted;
    }
    let Some(password) = password else {
        issues.push(
            "FDF embedded FDF stream is encrypted with /EncryptionRevision 1 (Table 244) \
             but this call supplied no user password, so its bytes are returned undecrypted \
             and no child document is parsed (see Document::fdf_embedded_fdfs_with_password)"
                .to_string(),
        );
        return CarrierBytes::Undecrypted;
    };
    match crypto::decrypt(&carrier.data, password) {
        Some(plain) => CarrierBytes::Decrypted(plain),
        None => {
            issues.push(
                "FDF embedded FDF stream could not be unscrambled with the revision 1 key \
                 (12.7.7.3), so its bytes are returned undecrypted and no child document \
                 is parsed"
                    .to_string(),
            );
            CarrierBytes::Undecrypted
        }
    }
}

/// 递归防护：环检测（同一对象号不得重复进入）+ 层数上限；通过后把本条
/// 目的文件规范与载体流对象号登记进 `visited`。返回 `Err(诊断)` 表示
/// 本条目不再向下展开（上层类型化结果不受影响）。
fn recursion_guard(
    filespec: &Option<Filespec>,
    carrier: &EmbeddedFile,
    depth: usize,
    visited: &mut HashSet<(i64, i64)>,
) -> std::result::Result<(), String> {
    // 环检测：文件规范与载体流的对象号都登记。
    let mut keys = Vec::new();
    if let Some(key) = filespec.as_ref().and_then(|spec| spec.reference) {
        keys.push(key);
    }
    if let Some(key) = carrier.reference {
        keys.push(key);
    }
    if keys.iter().any(|key| visited.contains(key)) {
        return Err(
            "FDF /EmbeddedFDFs references an embedded file already visited; recursion skipped"
                .to_string(),
        );
    }
    if depth >= super::MAX_FDF_EMBEDDED_DEPTH {
        return Err(format!(
            "FDF embedded recursion limit {} reached; deeper levels are \
             not expanded",
            super::MAX_FDF_EMBEDDED_DEPTH
        ));
    }
    visited.extend(keys);
    Ok(())
}

/// Table 244 `/EncryptionRevision`：存在且为整数才类型化，非整数记诊断。
fn carrier_revision(carrier: Option<&EmbeddedFile>, issues: &mut Vec<String>) -> Option<i64> {
    let dict = carrier.map(|file| &file.dict)?;
    match dict.get("EncryptionRevision") {
        None => None,
        Some(Value::Integer(revision)) => Some(*revision),
        Some(other) => {
            issues.push(format!(
                "FDF embedded FDF stream /EncryptionRevision is {} but Table 244 declares an \
                 integer; raw stream dictionary entry kept",
                other.type_name()
            ));
            None
        }
    }
}

/// 读端版本语义（**不拒绝**）：`/Differences` 与 `/EmbeddedFDFs` 都是
/// PDF 1.4 条目，声明版本低于 1.4 时仍照常类型化，只额外记一条诊断。
/// 声明版本取值优先序：FDF 目录 `/Version`（Table 242）→ `%FDF-x.y`
/// 文件头 → [`Document::source_version`]。禁止新增 `PdfFeature` 变体，
/// 故这里直接比较 [`PdfVersion`] 常量。
fn pdf14_entry_problem(document: &Document, fdf: &FdfDocument, key: &str) -> Option<String> {
    let declared = declared_version(document, fdf);
    if declared >= PdfVersion::V1_4 {
        return None;
    }
    Some(format!(
        "FDF /{key} is a PDF 1.4 entry (Table 243 continued) but the file declares {}; it is \
         still read leniently",
        declared.as_str()
    ))
}

/// 本文件的 FDF 声明版本。
fn declared_version(document: &Document, fdf: &FdfDocument) -> PdfVersion {
    fdf.catalog_version
        .as_deref()
        .and_then(PdfVersion::from_name)
        .or_else(|| scan::fdf_header_version(document))
        .unwrap_or_else(|| document.source_version())
}
