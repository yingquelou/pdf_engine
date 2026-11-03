//! FDF 写端值类型（ISO 32000-1 12.7.7，PDF 1.2）：把读端类型化模型
//! （[`FdfDocument`] / [`FdfDictionary`] / [`FdfField`]，Table 242/243/246）
//! 转回可直接序列化的目录字典。
//!
//! 口径：未建模的原始条目按 [`FdfField::dict`] / [`FdfDictionary::dict`]
//! 原样保留；已建模条目以类型化字段为准——`Some` 覆盖原值，`None`
//! 删除字典里的同名条目。文本串（`/T`、`/Status`、类型化的 `/Target`
//! 与 Table 245 的脚本正文）统一写成十六进制串
//! （[`PdfTextString::encode`]：PDFDocEncoding 优先，
//! 含不可映射字符时回退 UTF-16BE 带 BOM），与读端
//! `content::text_value` 的解码口径配对。
//!
//! Table 248–251（`/Pages` 的四层结构 + `/Annots` 的 `/Page`）另有一组
//! **独立**的写入口径（[`pages_value`] / [`annots_value`] 一族）：它们只
//! 覆盖类型化命中的条目、**不动**类型化为 `None` 的原值（`keep_raw`），
//! 因为这几张表里的可选条目（Table 248 `/Info`、Table 249 `/Rename`
//! 「Default value: true」、Table 250 `/F`）都不该物化缺省；无法确证为
//! 字典的数组项整体按原值透传。之所以是独立函数而不是
//! [`catalog_dict`] 的一部分：`FdfDictionary` 的 `pages`/`annots` 是
//! **原值**字段（类型化视图以方法暴露，见 `reader/fdf/pages.rs` 缺口
//! 说明），把类型化页写回 `FdfDocument` 需要给该结构加字段，而它的
//! 全字段字面构造点在本车道写集之外。对外入口是
//! `PdfBuilder::fdf_pages_value` / `PdfBuilder::fdf_annots_value`
//! （`builder/navigation/fdf/mod.rs`），产出的值写回上述原值字段后照常走
//! [`catalog_dict`] + `export_fdf`；这几张表自身的装配前校验在子模块
//! `builder/navigation/fdf/pages.rs`。
//!
//! 版本门禁与文件装配在 `builder/navigation/fdf/mod.rs`。`PdfFeature` 新
//! 变体需要同步 `types/version/feature_display.rs` 的穷尽 `Display`
//! match（在本车道写集之外），故门禁直接比较 [`PdfVersion`] 常量，
//! 与 `navigation/mod.rs` 既有门禁口径一致。

use crate::core::reader::fdf::{
    FdfAnnotation, FdfDictionary, FdfDocument, FdfField, FdfJavaScript, FdfNamedPage, FdfPage,
    FdfTemplate,
};
use crate::core::types::{Array, Dict, PdfTextString, PdfVersion, Value};

/// 文本串值：写端一律输出十六进制串（Table 246 `/T`、Table 243
/// `/Status` 的文本串形态；编码见模块头的口径说明）。
pub(crate) fn text_string_value(text: &str) -> Value {
    Value::HexString(
        PdfTextString::new(text)
            .encode()
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect(),
    )
}

/// 把类型化条目覆盖写回原始字典：`Some` 插入（覆盖同名原值），
/// `None` 删除原值（类型化字段是唯一事实源）。返回新字典。
fn overlay(mut dict: Dict, entries: &[(&str, Option<Value>)]) -> Dict {
    for (key, value) in entries {
        match value {
            Some(value) => dict = dict.insert(*key, value.clone()),
            None => {
                dict.0.remove(*key);
            }
        }
    }
    dict
}

/// FDF 字段字典（Table 246）→ 直接对象值；`/Kids` 子字段同样写成
/// 直接对象（12.7.7.3.2：FDF 字段允许直接对象，与读端宽容解析对齐）。
pub(crate) fn field_value(field: &FdfField) -> Value {
    // Table 246 — `/T` 部分字段名（必填文本串；写端无条件写出）。
    let mut dict = overlay(
        field.dict.clone(),
        &[
            ("T", Some(text_string_value(&field.partial_name))),
            // Table 246 — `/V` 字段值（原值透传）。
            ("V", field.value.clone()),
            // Table 246 — `/Ff` 字段标志位（整型）。
            ("Ff", field.field_flags.map(Value::Integer)),
            // Table 246 — `/SetFf` 导入时置位的 `/Ff` 位（整型）。
            ("SetFf", field.set_field_flags.map(Value::Integer)),
            // Table 246 — `/ClrFf` 导入时清位的 `/Ff` 位（整型）。
            ("ClrFf", field.clear_field_flags.map(Value::Integer)),
            // Table 246 — `/F` widget 注解标志位（整型，12.5.3）。
            ("F", field.widget_flags.map(Value::Integer)),
            // Table 246 — `/SetF` 导入时置位的 `/F` 位（整型）。
            ("SetF", field.set_widget_flags.map(Value::Integer)),
            // Table 246 — `/ClrF` 导入时清位的 `/F` 位（整型）。
            ("ClrF", field.clear_widget_flags.map(Value::Integer)),
            // Table 246 — `/Opt` 选择字段选项数组（原值）。
            ("Opt", field.options.clone()),
            // Table 246 — `/AP` 外观字典（原值；N/R/D 均为流）。
            ("AP", field.appearance.clone()),
            // Table 246 — `/APRef` 外部页引用外观字典（原值；PDF 1.3）。
            ("APRef", field.appearance_reference.clone()),
            // Table 246 — `/IF` 图标适配字典（原值；PDF 1.3，Table 247）。
            ("IF", field.icon_fit.clone()),
            // Table 246 — `/A` 激活动作字典（原值；12.6）。
            ("A", field.action.clone()),
            // Table 246 — `/AA` 附加动作字典（原值；12.6.3）。
            ("AA", field.additional_actions.clone()),
            // Table 246 — `/RV` 富文本值（原值；PDF 1.5）。
            ("RV", field.rich_value.clone()),
        ],
    );
    // Table 246 — `/Kids` 直接子字段数组（声明序；子字段写成直接对象）。
    if field.kids.is_empty() {
        dict.0.remove("Kids");
    } else {
        dict = dict.insert(
            "Kids",
            Value::Array(Array(
                field.kids.iter().map(field_value).collect::<Vec<_>>(),
            )),
        );
    }
    Value::Dict(dict)
}

/// FDF `/JavaScript` 脚本字典（Table 245，raw L24951–L24981）→ 直接
/// 字典值。类型化的 `/Before`、`/After`、`/AfterPermsReady` 写成十六进制
/// 文本串（与读端 `text_value` 配对）并覆盖原值；类型化为 `None` 时保留
/// 字典里的原值——那必然是文本流/间接引用等就地不可确证的形态，不丢弃。
/// `/Doc` 在原数组**已被完整解码**或原数组缺失时按类型化配对写出，否则
/// 保留原数组（含流形态的配对整体留档）。
pub(crate) fn javascript_value(entry: &FdfJavaScript) -> Value {
    let mut dict = entry.dict.clone();
    for (key, text) in [
        ("Before", entry.before.as_deref()),
        ("After", entry.after.as_deref()),
        ("AfterPermsReady", entry.after_perms_ready.as_deref()),
    ] {
        if let Some(text) = text {
            dict = dict.insert(key, text_string_value(text));
        }
    }
    // 原数组里未进入类型化列表的配对（流形态）只能整体保留；完整解码
    // 或原数组缺失时才以类型化配对为准（可与读端逐字节往返）。
    let keep_raw_doc = matches!(dict.get("Doc"), Some(Value::Array(items))
        if entry.doc_scripts.len() * 2 != items.0.len());
    if !keep_raw_doc && !entry.doc_scripts.is_empty() {
        let mut pairs = Vec::with_capacity(entry.doc_scripts.len() * 2);
        for script in &entry.doc_scripts {
            pairs.push(Value::Name(script.name.clone()));
            pairs.push(text_string_value(&script.script));
        }
        dict = dict.insert("Doc", Value::Array(Array(pairs)));
    }
    Value::Dict(dict)
}

/// FDF 字典（Table 243）→ 直接对象值。
pub(crate) fn fdf_dictionary_value(fdf: &FdfDictionary) -> Value {
    // Table 243 续表 — `/Target`（string；PDF 1.4；raw L24927–L24929）：
    // 类型化字段为准，写成十六进制文本串（与读端 `text_value` 解码
    // 配对）；`target_frame` 为 None 时透传原值，非串形态不丢弃、
    // 由 `navigation/fdf/mod.rs` 在装配前按 Table 243 的 string 类型拒绝。
    let target = fdf
        .target_frame
        .as_deref()
        .map(text_string_value)
        .or_else(|| fdf.target.clone());
    // Table 243 续表 — `/JavaScript`（dictionary；PDF 1.4；raw
    // L24933–L24934）：类型化脚本字典（Table 245）优先，`None` 时透传
    // 原值（含非字典形态，由 `navigation/fdf/mod.rs` 装配前按 Table 243
    // 的 dictionary 类型拒绝）。
    let javascript = fdf
        .javascript_scripts
        .as_ref()
        .map(javascript_value)
        .or_else(|| fdf.javascript.clone());
    let dict = overlay(
        fdf.dict.clone(),
        &[
            // Table 243 — `/F` 源/目标 PDF 文件规范（原值）。
            ("F", fdf.source_file.clone()),
            // Table 243 — `/ID` 文件标识符数组（原值，14.4）。
            ("ID", fdf.identifier.clone()),
            // Table 243 — `/Fields` 根字段数组（Table 246 字段字典）。
            (
                "Fields",
                fdf.fields
                    .as_ref()
                    .map(|fields| Value::Array(Array(fields.iter().map(field_value).collect()))),
            ),
            // Table 243 — `/Status` 状态文本串（PDFDocEncoding）。
            ("Status", fdf.status.as_deref().map(text_string_value)),
            // Table 243 — `/Pages` 页字典数组（原值；PDF 1.3，Table 248）。
            ("Pages", fdf.pages.clone()),
            // Table 243 — `/Encoding` 名字（PDF 1.3；缺省 PDFDocEncoding）。
            (
                "Encoding",
                fdf.encoding
                    .as_deref()
                    .map(|name| Value::Name(name.to_string())),
            ),
            // Table 243 — `/Annots` 注解字典数组（原值；PDF 1.3，Table 251）。
            ("Annots", fdf.annots.clone()),
            // Table 243 — `/Differences` 增量更新流（原值；PDF 1.4）。
            ("Differences", fdf.differences.clone()),
            // Table 243 — `/Target` 浏览器框架名（类型化优先，原值兜底；PDF 1.4）。
            ("Target", target),
            // Table 243 — `/EmbeddedFDFs` 内嵌 FDF 文件规范数组（原值；PDF 1.4）。
            ("EmbeddedFDFs", fdf.embedded_fdfs.clone()),
            // Table 243 — `/JavaScript` 文档级脚本字典（类型化优先，原值兜底；PDF 1.4）。
            ("JavaScript", javascript),
        ],
    );
    Value::Dict(dict)
}

/// FDF 目录字典（Table 242）：`/FDF` 必填（直接字典），`/Version` 按
/// 声明版本补写（Table 242 — `/Version` 名字，Optional；PDF 1.4）。
pub(crate) fn catalog_dict(fdf: &FdfDocument, version: PdfVersion) -> Dict {
    let mut catalog = fdf.catalog.clone();
    catalog = catalog.insert("FDF", fdf_dictionary_value(&fdf.fdf));
    match fdf.catalog_version.as_deref() {
        // Table 242 — `/Version` 名字：模型已有原值时按原值写回。
        Some(name) => catalog = catalog.insert("Version", Value::Name(name.to_string())),
        None => {
            catalog.0.remove("Version");
            // Table 242 — `/Version` 自 PDF 1.4 起可用；1.2/1.3 的版别
            // 由文件头 `%FDF-<ver>` 承载（12.7.7.2.2）。
            if version >= PdfVersion::V1_4 {
                catalog = catalog.insert("Version", Value::Name(version.as_str()));
            }
        }
    }
    catalog
}

/// FDF 页字典数组（Table 248，raw L25085–L25090）→ 直接对象数组值
/// （Table 243 `/Pages`，PDF 1.3）。
pub(crate) fn pages_value(pages: &[FdfPage]) -> Value {
    Value::Array(Array(pages.iter().map(page_value).collect()))
}

/// 单个 FDF 页字典（Table 248）→ 直接字典值。
///
/// 写入口径与 [`field_value`] 一致：类型化条目覆盖写回 `page.dict`，
/// 未建模条目（含 Adobe 扩展键）原样保留。与 `overlay` 的
/// 「`None` 即删除」不同，这里用 [`keep_raw`]：类型化为 `None` 时**不动**
/// 原值——Table 248/249/250/251 的可选条目本就不该物化缺省，而形态不
/// 可确证的原值（如 `/Rename` 写成整数）也不能被静默删掉。
///
/// 无法类型化为字典的数组项（[`FdfPage::dict`] 为空且 `raw` 非字典）
/// 整体按原值透传，不伪造一个空页。
pub(crate) fn page_value(page: &FdfPage) -> Value {
    if !is_dict_form(&page.raw) && page.dict.0.is_empty() {
        return page.raw.clone();
    }
    let templates = Value::Array(Array(page.templates.iter().map(template_value).collect()));
    let mut dict = keep_raw(page.dict.clone(), "Templates", Some(templates));
    // Table 248 — `/Info`（Optional；「FDF page information dictionary」在
    // 32000-1 无条目表）：原值透传，缺席不物化。
    if let Some(info) = &page.info {
        dict = dict.insert("Info", info.clone());
    }
    Value::Dict(dict)
}

/// FDF 模板字典（Table 249，raw L25113–L25123）→ 直接字典值。
pub(crate) fn template_value(template: &FdfTemplate) -> Value {
    if !is_dict_form(&template.raw) && template.dict.0.is_empty() {
        return template.raw.clone();
    }
    let mut dict = template.dict.clone();
    // Table 249 — `/TRef`（Required；named page reference dictionary，
    // Table 250）。类型化缺失时不伪造：原值（若有）留在 dict 里，
    // 由 `navigation/fdf/pages.rs` 的装配前校验拒绝缺 Required 的产物。
    if let Some(reference) = &template.named_page {
        dict = dict.insert("TRef", named_page_value(reference));
    }
    // Table 249 — `/Fields`（Optional；指向 Table 246 字段字典的引用）。
    // 只有原数组被**完整**类型化时才以类型化为准，否则整体保留原值
    // （与 [`javascript_value`] 的 `/Doc` 同一策略，畸形项不丢）。
    let fully_typed = match dict.get("Fields") {
        None => true,
        Some(Value::Array(items)) => items.0.len() == template.fields.len(),
        Some(_) => false,
    };
    if fully_typed && !template.fields.is_empty() {
        dict = dict.insert(
            "Fields",
            Value::Array(Array(template.fields.iter().map(field_value).collect())),
        );
    }
    // Table 249 — `/Rename`（Optional; boolean; **Default value: true**）：
    // 缺省**不物化**，条目缺席就不写；非布尔形态（类型化为 None）原值保留。
    if let Some(flag) = template.rename {
        dict = dict.insert("Rename", Value::Bool(flag));
    }
    Value::Dict(dict)
}

/// FDF 命名页引用字典（Table 250，raw L25124–L25128）→ 直接字典值。
pub(crate) fn named_page_value(reference: &FdfNamedPage) -> Value {
    if !is_dict_form(&reference.raw) && reference.dict.0.is_empty() {
        return reference.raw.clone();
    }
    let mut dict = reference.dict.clone();
    // Table 250 — `/Name`（Required; string）：类型化命中才覆盖写成
    // 十六进制文本串（与读端 `text_value` 配对）；形态不可确证时原值保留。
    if let Some(name) = reference.name.as_deref() {
        dict = dict.insert("Name", text_string_value(name));
    }
    // Table 250 — `/F`（Optional; file specification）：原值透传；缺席即
    // 规范推定「该页在关联的 PDF 文件里」（raw L25127–L25128），不伪造。
    if let Some(file) = &reference.file {
        dict = dict.insert("F", file.clone());
    }
    Value::Dict(dict)
}

/// FDF 注解字典数组（Table 243 `/Annots` + Table 251，raw L25129–L25132）
/// → 直接对象数组值。
pub(crate) fn annots_value(annots: &[FdfAnnotation]) -> Value {
    Value::Array(Array(annots.iter().map(annotation_value).collect()))
}

/// 单个 FDF 注解字典 → 直接字典值。Table 251 只新增 `/Page`（integer，
/// Required for annotations in FDF files），其余条目（12.5.2 公共条目与
/// Table 171 起的子类型条目）一律原样保留。
pub(crate) fn annotation_value(annotation: &FdfAnnotation) -> Value {
    if !is_dict_form(&annotation.raw) && annotation.dict.0.is_empty() {
        return annotation.raw.clone();
    }
    // Table 251 — `/Page` 序数（0 起）；类型化为 None（缺失或非整型）时
    // 原值留在字典里不删。
    let mut dict = annotation.dict.clone();
    if let Some(page) = annotation.page {
        dict = dict.insert("Page", Value::Integer(page));
    }
    Value::Dict(dict)
}

/// 只在类型化给出值时覆盖，否则保留原字典条目（缺省不物化）。
fn keep_raw(mut dict: Dict, key: &str, value: Option<Value>) -> Dict {
    if let Some(value) = value {
        dict = dict.insert(key, value);
    }
    dict
}

/// 该原值形态是否就是字典（决定要不要整体透传）。
fn is_dict_form(value: &Value) -> bool {
    matches!(value, Value::Dict(_) | Value::Stream(_, _))
}
