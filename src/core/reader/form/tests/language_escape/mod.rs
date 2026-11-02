//! 表单字段 Unicode 语言转义读端测试（ISO 32000-1 7.9.2.2、12.7.3.2、
//! 12.7.3.3、14.9.2.1 与 14.9.3）：`/TU` 替代名在 `alternate_name.rs`，
//! `/RV` 富文本值在 `rich_value.rs`，共用夹具留在本文件。
//!
//! Table 220（raw L23645–L23650）把 `/TU` 定为「替代字段名」，并写明它在
//! 提取内容供无障碍使用时有用（见 14.9.3）；14.9.3（raw L32111–L32114）
//! 要求替代名「shall be used in place of the actual field name」，NOTE 2
//! （raw L32114）指出它「useful for vocalization purposes」，NOTE 3
//! （raw L32117–L32120）要求「Unicode escape sequences specifying language
//! shall override the prevailing Lang entry」。字段字典按 Table 220 没有
//! `/Lang` 条目，故 prevailing 即目录 `/Lang`（14.9.2.3 raw
//! L31955–L31956）。
//!
//! Table 222（raw L23767–L23771）另把 `/RV` 定为「A rich text string, as
//! described in 12.7.3.4, "Rich Text Strings."」（text string or text stream，
//! PDF 1.5）；本库不建模该条目，值留在 `Field::dict`，故 `/RV` 的测试同时
//! 钉住三种值形态的口径：文本串可展开转义、名字恒判无转义且不报错、文本流
//! 形态不解引用（同注解 `/DS` 的先例）。
//!
//! 12.7.3.2（raw L23696–L23697）只要求各表示的 `/FT`、`/V`、`/DV` 一致，
//! **没有**规定 `/TU` 一致；Table 222 只把 `/DA`、`/Q` 标为 inheritable，
//! `/RV` 没有该标注。因此两处都按节点自身声明解析：节点 `Field` 与归并视图
//! `LogicalField` 各自解析自己暴露的那一条，不做祖先回落。
//!
//! 7.9.2.2 a) 把转义写成「the byte sequence 0 followed by 27」，即 UTF-16BE
//! 的 `00 1B`，故夹具用 `PdfTextString` 编出的 UTF-16BE 十六进制串。

mod alternate_name;
mod rich_value;

use crate::core::reader::accessibility::{LANGUAGE_ESCAPE, LanguageError, LanguageIdentifier};

use super::*;

/// 7.9.2.2 a)–d) 的一个完整语言序列（U+001B + 码 + U+001B）。
fn escape(code: &str) -> String {
    format!("{LANGUAGE_ESCAPE}{code}{LANGUAGE_ESCAPE}")
}

/// 只有开界符、没有闭界符的残缺转义。
fn unterminated_escape(code: &str) -> String {
    format!("{LANGUAGE_ESCAPE}{code}")
}

fn identifier(value: &str) -> LanguageIdentifier {
    LanguageIdentifier::parse(value).unwrap_or_else(|error| panic!("{value:?}: {error}"))
}

/// 语言结论写成可读的标签串。
fn language_tag(language: Option<&LanguageIdentifier>) -> Option<&str> {
    language.map(LanguageIdentifier::as_str)
}

/// 单字段的 `/AcroForm`，`/TU` 取给定文本串。
fn field_of(alternate_name: Value) -> Field {
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new()
                .insert("T", text("name"))
                .insert("FT", name("Tx"))
                .insert("TU", alternate_name),
        )]),
    )));
    let mut form = doc.acro_form().expect("parse").expect("form present");
    form.fields.remove(0)
}
