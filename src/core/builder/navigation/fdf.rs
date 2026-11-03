//! FDF 独立文件写端（ISO 32000-1 12.7.7.2，PDF 1.2）：
//! [`PdfBuilder::build_fdf`]（目录字典直接对象形态）与
//! [`PdfBuilder::export_fdf`]（读端 [`FdfDocument`] 模型）。
//!
//! 文件结构（12.7.7.2.1，Figure 65）：一行文件头 `%FDF-<ver>`
//! （12.7.7.2.2）、体部只写**一个**间接对象——FDF 目录（Table 241 要求
//! trailer `/Root` shall be an indirect reference）、经典交叉引用表
//! （规范允许省略，但本库读端以 `startxref` 为入口，故始终写出）、
//! trailer（`/Root` 必填）与 `%%EOF`。字段与 FDF 字典全部写成直接对象
//! （12.7.7.3.2：FDF 字段允许直接对象）。对象号恒为 0 代、互不重复
//! （12.7.7.1）。
//!
//! 版本门禁（超出**生效版本** → [`WriteError::UnsupportedVersion`]，零字节）：
//! 生效版本 = 声明的文件头版本与目录 `/Version` 的较大值（Table 242 —
//! `/Version` 只在「晚于文件头版本」时生效，否则以文件头为准）。
//! - 文件版本本身须在 1.2–1.7（12.7.7.1：FDF 自 PDF 1.2；2.0 档位无 FDF
//!   对应规范，保守拒绝）；
//! - Table 242 — 目录 `/Version`：PDF 1.4；
//! - Table 243 — FDF 字典 `/Encoding` `/Pages` `/Annots`：PDF 1.3；
//!   `/Differences` `/Target` `/EmbeddedFDFs` `/JavaScript`：PDF 1.4；
//! - Table 245 — JavaScript 字典 `/AfterPermsReady`：PDF 1.6；
//! - Table 246 — 字段 `/APRef` `/IF`：PDF 1.3；`/RV`：PDF 1.5。
//!
//! 结构非法（`/FDF` 缺失、`/Fields` 与 `/Pages` 或 `/Status` 同现、
//! `/Target` 非字符串（Table 243：string）、`/JavaScript` 非字典
//! （Table 243：dictionary）、Table 245 的脚本条目既非文本串又非文本流
//! 或 `/Doc` 非数组/配对不齐、字段缺 `/T`、整型标志非整数、
//! 超出读端深度/数量上限等）返回
//! [`WriteError::InvalidObject`]，同样零字节。
//!
//! `PdfFeature` 新变体需同步 `types/version/feature_display.rs` 的穷尽
//! `Display` match（本车道写集之外），故此处直接比较 [`PdfVersion`]
//! 常量，与 `navigation/mod.rs` 的既有门禁口径一致。

use std::io::Write as _;

use crate::core::builder::types::fdf::catalog_dict;
use crate::core::reader::fdf::{FdfDocument, MAX_FDF_FIELD_DEPTH, MAX_FDF_FIELD_NODES};
use crate::core::types::{Dict, PdfVersion, Value};
use crate::core::writer::{WriteError, write_value};

use super::PdfBuilder;
use super::invalid;

/// Table 243 — FDF 字典中 PDF 1.3 档位条目。
const GATED_1_3_FDF_KEYS: [&str; 3] = ["Encoding", "Pages", "Annots"];
/// Table 243 — FDF 字典中 PDF 1.4 档位条目。
const GATED_1_4_FDF_KEYS: [&str; 4] = ["Differences", "Target", "EmbeddedFDFs", "JavaScript"];
/// Table 246 — 字段字典中 PDF 1.3 档位条目。
const GATED_1_3_FIELD_KEYS: [&str; 2] = ["APRef", "IF"];
/// Table 246 — 整型标志条目。
const INTEGER_FIELD_KEYS: [&str; 6] = ["Ff", "SetFf", "ClrFf", "F", "SetF", "ClrF"];
/// Table 245 — JavaScript 字典中的脚本条目（`/AfterPermsReady` 另需 1.6 档位）。
const JAVASCRIPT_SCRIPT_KEYS: [&str; 3] = ["Before", "After", "AfterPermsReady"];

/// Table 245 脚本条目的可接受形态：文本串（字面/十六进制/ASCII85）或
/// **间接引用**的文本流（按原值写出，引用是否与单对象写出一致属剩余
/// 缺口）。直接内联流被拒绝：12.7.7.2 的写端只走一个间接对象，
/// `write_value` 无法把流字节带进字典文本，产出会是静默损坏的文件。
fn script_value_ok(value: &Value) -> bool {
    matches!(
        value,
        Value::String(_) | Value::HexString(_) | Value::Ascii85String(_) | Value::Ref(..)
    )
}

fn unsupported(entry: &str, minimum: PdfVersion, version: PdfVersion) -> WriteError {
    WriteError::UnsupportedVersion(format!(
        "FDF {entry} requires PDF {}, got {version}",
        minimum.as_str()
    ))
}

/// 字段树遍历防护：节点计数与读端 `MAX_FDF_FIELD_NODES` 同口径，
/// 保证写出的文件必然可被读端接收。
struct FieldCounter {
    nodes: usize,
}

/// 校验 FDF 目录字典与生效版本；任何失败都在装配字节之前返回。
///
/// 生效版本 = 声明的文件头版本与目录 `/Version` 的较大值（Table 242 —
/// `/Version`「The version … **if later than** the version specified in
/// the file's header. If the header specifies a later version, or if this
/// entry is absent, the document conforms to the version specified in the
/// header」）；所有条目档位门禁按生效版本判定。
pub(super) fn validate(catalog: &Dict, version: PdfVersion) -> Result<(), WriteError> {
    // 12.7.7.1：FDF 是 PDF 1.2 的文件格式；ISO 32000-1 无 2.0 档位 FDF。
    if version < PdfVersion::V1_2 || version > PdfVersion::V1_7 {
        return Err(unsupported("file version", PdfVersion::V1_2, version));
    }
    // Table 242 — `/Version` 名字（Optional; PDF 1.4）。
    let mut gate = version;
    if let Some(value) = catalog.get("Version") {
        if version < PdfVersion::V1_4 {
            return Err(unsupported(
                "catalog /Version (Table 242)",
                PdfVersion::V1_4,
                version,
            ));
        }
        let name = value
            .as_name()
            .ok_or_else(|| invalid("FDF catalog /Version must be a name object (Table 242)"))?;
        let Some(catalog_version) = PdfVersion::from_name(name) else {
            return Err(invalid(format!(
                "FDF catalog /Version {name:?} is not a parsable version name"
            )));
        };
        if catalog_version > PdfVersion::V1_7 {
            return Err(invalid(format!(
                "FDF catalog /Version {name:?} exceeds the FDF versions defined by ISO 32000-1 (1.2-1.7)"
            )));
        }
        if catalog_version > gate {
            gate = catalog_version;
        }
    }
    // Table 242 — `/FDF` 必填 FDF 字典；写端只输出单个间接对象，
    // 故此处要求直接字典形态。
    let Some(fdf) = catalog.get("FDF").and_then(Value::as_dict) else {
        return Err(invalid(
            "FDF catalog requires a direct-dictionary /FDF entry (Table 242)",
        ));
    };
    for key in GATED_1_3_FDF_KEYS {
        if fdf.get(key).is_some() && gate < PdfVersion::V1_3 {
            return Err(unsupported(
                "FDF dictionary entry (Table 243)",
                PdfVersion::V1_3,
                gate,
            ));
        }
    }
    for key in GATED_1_4_FDF_KEYS {
        if fdf.get(key).is_some() && gate < PdfVersion::V1_4 {
            return Err(unsupported(
                "FDF dictionary entry (Table 243)",
                PdfVersion::V1_4,
                gate,
            ));
        }
    }
    // Table 243 续表 — `/Target` 类型是 string（浏览器框架名；raw
    // L24927–L24929）：非串形态（含 Adobe FDF 2.0 的字典式 `/Target`，
    // 非 ISO 32000-1 基线，待核）在装配字节之前拒绝。档位门禁先于
    // 本校验，1.3 声明下的非串 `/Target` 仍报 UnsupportedVersion。
    if let Some(value) = fdf.get("Target")
        && !matches!(value, Value::String(_) | Value::HexString(_))
    {
        return Err(invalid(
            "FDF dictionary /Target must be a string (Table 243)",
        ));
    }
    // Table 243 续表 — `/JavaScript` 类型是 dictionary（PDF 1.4；raw
    // L24933–L24934「A JavaScript dictionary (see Table 245) defining
    // document-level JavaScript scripts」），指向 Table 245（raw
    // L24951–L24981）。字典形态按 Table 245 逐条目把关。
    if let Some(value) = fdf.get("JavaScript") {
        let Some(scripts) = value.as_dict() else {
            return Err(invalid(
                "FDF dictionary /JavaScript must be a dictionary (Table 243)",
            ));
        };
        // Table 245 — `/AfterPermsReady`（Optional; PDF 1.6；raw
        // L24961–L24969）：档位门禁先于类型校验，与 FDF 字典条目同口径。
        if scripts.get("AfterPermsReady").is_some() && gate < PdfVersion::V1_6 {
            return Err(unsupported(
                "FDF /JavaScript /AfterPermsReady (Table 245)",
                PdfVersion::V1_6,
                gate,
            ));
        }
        // Table 245 — `/Before` `/After` `/AfterPermsReady` 是
        // 「text string or text stream」；`/Doc` 是偶数长度的
        // 「name + text string or text stream」配对数组。
        for key in JAVASCRIPT_SCRIPT_KEYS {
            if let Some(entry) = scripts.get(key)
                && !script_value_ok(entry)
            {
                return Err(invalid(format!(
                    "FDF /JavaScript /{key} must be a text string or text stream (Table 245)"
                )));
            }
        }
        if let Some(entry) = scripts.get("Doc") {
            let array = entry
                .as_array()
                .ok_or_else(|| invalid("FDF /JavaScript /Doc must be an array (Table 245)"))?;
            if array.0.len() % 2 != 0 {
                return Err(invalid(
                    "FDF /JavaScript /Doc shall contain an even number of elements (Table 245)",
                ));
            }
            for pair in array.0.chunks(2) {
                // 偶数长度已在上面把关，此处配对必然齐全；不齐即拒。
                let [name, value] = pair else {
                    return Err(invalid(
                        "FDF /JavaScript /Doc pairs shall be complete (Table 245)",
                    ));
                };
                if name.as_name().is_none() {
                    return Err(invalid(
                        "FDF /JavaScript /Doc pairs shall start with a name object (Table 245)",
                    ));
                }
                if !script_value_ok(value) {
                    return Err(invalid(
                        "FDF /JavaScript /Doc pair values shall be text strings or text streams (Table 245)",
                    ));
                }
            }
        }
    }
    // Table 243：`/Fields` 与 `/Pages`、`/Status` 与 `/Pages` shall not
    // both be present。
    if fdf.get("Pages").is_some() && (fdf.get("Fields").is_some() || fdf.get("Status").is_some()) {
        return Err(invalid(
            "FDF dictionary /Pages shall not appear together with /Fields or /Status (Table 243)",
        ));
    }
    // Table 243 — `/Fields` 根字段数组；逐项走 Table 246 校验。
    let mut counter = FieldCounter { nodes: 0 };
    if let Some(value) = fdf.get("Fields") {
        let array = value
            .as_array()
            .ok_or_else(|| invalid("FDF dictionary /Fields must be an array (Table 243)"))?;
        walk_field_array(&array.0, 1, &mut counter, gate)?;
    }
    Ok(())
}

/// 走一个字段数组（Table 243 `/Fields` 或 Table 246 `/Kids`）。
fn walk_field_array(
    entries: &[Value],
    depth: usize,
    counter: &mut FieldCounter,
    effective: PdfVersion,
) -> Result<(), WriteError> {
    for entry in entries {
        // Table 246：写端字段字典只支持直接对象形态（间接字段需要
        // 多对象与对象号簿记，超出本切片）。
        let Some(dict) = entry.as_dict() else {
            return Err(invalid(
                "FDF field must be a direct dictionary (Table 246 writer policy)",
            ));
        };
        walk_field(dict, depth, counter, effective)?;
    }
    Ok(())
}

/// 走单个字段字典（Table 246）。
fn walk_field(
    dict: &Dict,
    depth: usize,
    counter: &mut FieldCounter,
    effective: PdfVersion,
) -> Result<(), WriteError> {
    if depth > MAX_FDF_FIELD_DEPTH {
        return Err(invalid(format!(
            "FDF field tree exceeds depth limit {MAX_FDF_FIELD_DEPTH} (reader parity)"
        )));
    }
    if counter.nodes >= MAX_FDF_FIELD_NODES {
        return Err(invalid(format!(
            "FDF field tree exceeds node limit {MAX_FDF_FIELD_NODES} (reader parity)"
        )));
    }
    counter.nodes += 1;
    // Table 246 — `/T` 部分字段名（必填文本串）。
    match dict.get("T") {
        Some(Value::String(_) | Value::HexString(_)) => {}
        _ => return Err(invalid("FDF field requires /T text string (Table 246)")),
    }
    // Table 246 — 整型标志条目出现时必须为整数（与读端口径一致）。
    for key in INTEGER_FIELD_KEYS {
        if let Some(value) = dict.get(key)
            && !matches!(value, Value::Integer(_))
        {
            return Err(invalid(format!(
                "FDF field /{key} must be an integer (Table 246)"
            )));
        }
    }
    // Table 246 — `/APRef` `/IF`（PDF 1.3）与 `/RV`（PDF 1.5）门禁。
    for key in GATED_1_3_FIELD_KEYS {
        if dict.get(key).is_some() && effective < PdfVersion::V1_3 {
            return Err(unsupported(
                "field /APRef or /IF (Table 246)",
                PdfVersion::V1_3,
                effective,
            ));
        }
    }
    if dict.get("RV").is_some() && effective < PdfVersion::V1_5 {
        return Err(unsupported(
            "field /RV (Table 246)",
            PdfVersion::V1_5,
            effective,
        ));
    }
    // Table 246 — `/Kids` 递归。
    if let Some(value) = dict.get("Kids") {
        let array = value
            .as_array()
            .ok_or_else(|| invalid("FDF field /Kids must be an array (Table 246)"))?;
        walk_field_array(&array.0, depth + 1, counter, effective)?;
    }
    Ok(())
}

/// 校验并装配独立 FDF 字节；失败时不返回任何字节。
pub(super) fn emit(catalog: &Dict, version: PdfVersion) -> Result<Vec<u8>, WriteError> {
    validate(catalog, version)?;
    let mut body = String::new();
    write_value(&Value::Dict(catalog.clone()), &mut body);
    let mut out: Vec<u8> = Vec::with_capacity(body.len() + 128);
    // 12.7.7.2.2 — 一行文件头 `%FDF-<ver>`。
    let _ = writeln!(&mut out, "%FDF-{version}");
    // 12.7.7.2.4 — trailer `/Root` 指向的目录写成唯一间接对象（Table 241）。
    let object_at = out.len();
    let _ = write!(&mut out, "1 0 obj\n{body}\nendobj\n");
    let xref_at = out.len();
    out.extend_from_slice(b"xref\n0 2\n0000000000 65535 f \n");
    let _ = writeln!(&mut out, "{object_at:010} 00000 n ");
    out.extend_from_slice(b"trailer\n<< /Root 1 0 R /Size 2 >>\n");
    let _ = writeln!(&mut out, "startxref\n{xref_at}\n%%EOF");
    Ok(out)
}

impl PdfBuilder {
    /// 序列化一个已按 Table 242 组装的 FDF 目录字典为独立 FDF 文件字节
    /// （12.7.7.2）。`version` 写进文件头 `%FDF-<ver>`（1.2–1.7）；目录
    /// 若自带 `/Version` 且晚于该版本，则按 Table 242 以 `/Version` 为准
    /// （本入口只校验，不补写：补写由
    /// [`export_fdf`](Self::export_fdf) 的模型路径负责）。超出**生效版本**
    /// 的条目返回 [`WriteError::UnsupportedVersion`]、结构非法返回
    /// [`WriteError::InvalidObject`]，均零字节输出。
    pub fn build_fdf(&self, catalog: &Dict, version: PdfVersion) -> Result<Vec<u8>, WriteError> {
        emit(catalog, version)
    }

    /// 序列化读端 FDF 类型化模型（`crate::core::reader::fdf::FdfDocument`，
    /// Table 242/243/246）为独立 FDF 字节：未建模原始条目原样保留，
    /// 已建模条目以类型化字段为准；`/Version` 按模型原值写回，模型缺
    /// 该值且 `version >= 1.4` 时按声明版本补写（Table 242）。版本门禁
    /// 与校验同 [`build_fdf`](Self::build_fdf)（生效版本 = 声明版本与
    /// 目录 `/Version` 的较大值），失败零字节输出。
    pub fn export_fdf(
        &self,
        fdf: &FdfDocument,
        version: PdfVersion,
    ) -> Result<Vec<u8>, WriteError> {
        emit(&catalog_dict(fdf, version), version)
    }
}
