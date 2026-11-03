//! 对象流（`/ObjStm`）内对象的解包与 §7.5.7 结构违例诊断。
//!
//! 自 [`super`]（原 `reader/document.rs`）拆出以守 600 行 / 25600 字节文件上限。
//!
//! ## 规范立场（本轮分支的依据）
//!
//! ISO 32000-1:2008 §7.5.7「Object Streams」（本仓库
//! `docs/PDF32000_2008.pdf`，`pdftotext -raw` 抽取的第 2233–2244 行）：
//!
//! > 第 2233 行 `7.5.7 Object Streams`
//! > 第 2236–2237 行 NOTE 1「The purpose of object streams is to allow
//! > indirect objects **other than streams** to be stored more compactly …」
//! > 第 2240 行「The following objects **shall not** be stored in an object stream:」
//! > 第 2241 行「▪ Stream objects」
//! > 第 2242 行「▪ Objects with a generation number other than zero」
//! > 第 2243 行「▪ A document's encryption dictionary (see 7.6, "Encryption")」
//! > 第 2244 行「▪ An object representing the value of the Length entry in an
//! > object stream dictionary」
//! > 第 2254–2255 行「In linearized files … the document catalog, the
//! > linearization dictionary, and page objects shall not appear in an object stream.」
//!
//! 即：**规范禁止**流对象被打包进对象流（不是许可）。因此本轮取
//! **「拒收 + 诊断」分支**，而非「按许可修复内联流语义」：
//! - 读端（本模块）维持既有的「宽进」姿态：违例文档里的内联流对象**不再被
//!   静默降级成字典**，正文从解码后的对象流载荷里切成
//!   [`crate::core::types::StreamData::Owned`] 原样保住，同时登记
//!   [`ObjectStreamViolation`]（见 [`Document::object_stream_violations`]）；
//! - 写端**不得**产出违例文件：打包选择显式排除流对象
//!   （`crate::core::writer::serialize::ObjectStreamLayout`），且
//!   `crate::core::writer::context` 在真正写入对象流前对混入的流对象
//!   返回类型化错误、零字节，绝不静默丢正文。
//!
//! 注：第 2254–2255 行的「线性化文件不得打包 catalog / 线性化字典 / 页面
//! 对象」需要知道文档是否线性化，属另一条独立判定，本轮不纳入
//! [`ObjectStreamViolation`]（写端不产出线性化文件，读端已有
//! [`crate::core::reader::linearization`] 通道）。

use crate::core::reader::Result;
use crate::core::reader::lexer::Lexer;
use crate::core::reader::object::{parse_int_token, parse_object, parse_stream_tail};
use crate::core::types::{Document, Object, Value, XrefKind, scan_value_features};

/// 对象流（§7.5.7）结构违例：一条对应一个可判定的 shall。
///
/// 与 [`crate::core::reader::linearization::LinearizationIssue`]、
/// [`crate::core::reader::navigation::requirements::RequirementsIssue`]
/// 同形——类型化、非 panic、`Display` 携带条款文本，供调用方直接转述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectStreamViolation {
    /// 7.5.7「The following objects shall not be stored in an object stream:
    /// ▪ Stream objects」——被打包的对象自己带 `stream … endstream` 正文。
    ///
    /// `recovered` 为真表示读端已把正文从对象流载荷里还原成
    /// [`crate::core::types::Value::Stream`]（内容不丢）；为假表示内联
    /// `stream` 尾部无法定位（畸形长度 / 缺 `endstream`），对象退回普通
    /// 字典、正文不可恢复。
    StreamObjectInObjectStream {
        /// 违例对象的对象号。
        object_number: i64,
        /// 承载它的对象流对象号。
        stream_number: i64,
        /// 该对象在对象流载荷内的索引（xref type-2 条目的第三字段）。
        index: i64,
        /// 正文是否已被还原。
        recovered: bool,
    },
}

impl std::fmt::Display for ObjectStreamViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObjectStreamViolation::StreamObjectInObjectStream {
                object_number,
                stream_number,
                index,
                recovered,
            } => write!(
                f,
                "7.5.7 violation: object {object_number} (index {index}) stored inside object \
                 stream {stream_number} is a stream object, which shall not be stored in an \
                 object stream; body {}",
                if *recovered {
                    "recovered from the object stream payload"
                } else {
                    "unrecoverable (kept as a plain dictionary)"
                }
            ),
        }
    }
}

impl std::error::Error for ObjectStreamViolation {}

impl Document {
    /// 解析「位于对象流（ObjStm）内」的第 `index` 个对象，并把结果填入缓存
    /// 与特性画像；解包细节见 [`Document::unpack_obj_stream_entry`]。
    ///
    /// 违例（7.5.7 的内联流对象）除进入 [`Document::object_stream_violations`]
    /// 外，也追加一条同文本 warning，保证 `warnings()` 通道不哑。
    pub(super) fn get_object_from_obj_stream(
        &self,
        num: i64,
        stream_num: i64,
        index: i64,
    ) -> Result<Option<Object>> {
        let Some((obj, violation)) = self.unpack_obj_stream_entry(num, stream_num, index)? else {
            return Ok(None);
        };
        if let Some(violation) = &violation {
            self.push_warning(violation.to_string());
        }
        // 顺带扫描特性，与 Normal 路径保持一致。
        scan_value_features(&mut self.features.borrow_mut(), &obj.value);
        self.cache.borrow_mut().insert(obj.num, obj.clone());
        Ok(Some(obj))
    }

    /// 扫描全文档 xref 的 type-2（对象流内）条目，汇总 §7.5.7 结构违例。
    ///
    /// 只读诊断：不写缓存、不改特性画像；但对确实损坏的条目会像
    /// [`Document::get_object`] 一样追加说明性 warning。写端据此（以及自身
    /// 的显式排除）保证不再产出违例文件。
    pub fn object_stream_violations(&self) -> Vec<ObjectStreamViolation> {
        if self.source.is_none() {
            // 写入端构造的 Document：对象全在缓存、无对象流载荷可扫。
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut seen: std::collections::HashSet<(i64, i64)> = std::collections::HashSet::new();
        for sub in &self.xref.0 {
            for (i, e) in sub.entries.iter().enumerate() {
                let (stream_num, index) = match &e.kind {
                    XrefKind::InObjStream { stream_num, index } => (*stream_num, *index),
                    _ => continue,
                };
                // /Prev 链可能重复登记同一 (对象流, 索引)，只判一次。
                if !seen.insert((stream_num, index)) {
                    continue;
                }
                let num = sub.start + i as i64;
                if let Ok(Some((_, Some(violation)))) =
                    self.unpack_obj_stream_entry(num, stream_num, index)
                {
                    out.push(violation);
                }
            }
        }
        out
    }

    /// 对象流解包的纯函数部分：不填缓存、不扫特性、不报违例 warning，
    /// 只返回对象与其（若有）§7.5.7 违例。
    ///
    /// 流程：先取出对象流对象本身（其 xref 条目应是可直接定位的
    /// `Normal` 条目），取其原始字节，按 `/Filter` + `/DecodeParms`
    /// （含 Flate 与 Predictor）**解码**，再读字典 `/N`（流内对象个数）
    /// 与 `/First`（首个对象数据的偏移）；流字节开头是 N 对
    /// `对象号 相对偏移`（ASCII 数字，空白分隔），第 `index` 对给出目标
    /// 对象号与其在流内的相对偏移，对象本体位于 `First + 相对偏移`。
    /// 对象流内对象没有 `N G obj` 包裹，直接是对象本体。
    ///
    /// 无法解码（不支持的过滤器 / 数据损坏）或结构异常时记 warning 并
    /// 返回 `Ok(None)`，绝不 panic。
    pub(super) fn unpack_obj_stream_entry(
        &self,
        num: i64,
        stream_num: i64,
        index: i64,
    ) -> Result<Option<(Object, Option<ObjectStreamViolation>)>> {
        // 对象流自身必须是可直接定位的普通条目；若它也位于另一个对象流
        // 内，则放弃，避免递归取对象流造成无限递归。
        match self.xref.get(stream_num).map(|e| &e.kind) {
            Some(XrefKind::Normal) => {}
            Some(XrefKind::InObjStream { .. }) => {
                self.push_warning(format!(
                    "object {num}: object stream {stream_num} is itself inside an object stream; \
                     nested object streams are not supported"
                ));
                return Ok(None);
            }
            _ => return Ok(None),
        }
        let stream_obj = match self.get_object(stream_num)? {
            Some(o) => o,
            None => return Ok(None),
        };
        let (dict, data) = match &stream_obj.value {
            Value::Stream(d, sd) => (d, sd),
            _ => {
                self.push_warning(format!(
                    "object {num}: referenced object {stream_num} for object stream is not a stream"
                ));
                return Ok(None);
            }
        };
        // 版本门控：对象流自 PDF 1.5 引入。声明版本更低仍尝试解析（读取
        // 结果与版本无关），仅记说明性 warning，保持宽容。
        if !self.source_version.supports_object_streams() {
            self.push_warning(format!(
                "object stream {stream_num} present but declared version {} < 1.5; \
                 attempting parse anyway",
                self.source_version
            ));
        }
        // 按 /Filter + /DecodeParms 解码（对象流常见 /FlateDecode）。不支持的
        // 过滤器或数据损坏 → 记 warning 优雅失败（返回 None），绝不 panic。
        let bytes = match self.decode_stream_bytes(&stream_obj.value) {
            Ok(b) => b,
            Err(e) => {
                self.push_warning(format!(
                    "object {num} (index {index} in object stream {stream_num}) is compressed \
                     (filter {:?}); decoding failed: {e}; giving up",
                    dict.get("Filter")
                ));
                return Ok(None);
            }
        };
        let (n, first) = match (
            dict.get("N").and_then(|v| v.as_integer()),
            dict.get("First").and_then(|v| v.as_integer()),
        ) {
            (Some(n), Some(first)) if n > 0 && first >= 0 => (n, first as usize),
            _ => {
                self.push_warning(format!(
                    "object stream {stream_num} missing/invalid /N or /First"
                ));
                return Ok(None);
            }
        };
        if index < 0 || index >= n {
            return Ok(None);
        }
        // 流字节是对象流自身内容的窗口，词法器 base 取其文件绝对偏移
        // （仅用于错误定位；Owned 字节无文件偏移，按 0 计）。
        let base = match data {
            crate::core::types::StreamData::Ref { offset, .. }
            | crate::core::types::StreamData::EncryptedRef { offset, .. } => *offset,
            crate::core::types::StreamData::Owned(_) => 0,
        };
        // 头部：N 对「对象号 相对偏移」。读第 index 对（0 基）。
        let mut hlex = Lexer::at(&bytes, base);
        let mut inner_num = 0i64;
        let mut rel = 0i64;
        for _ in 0..=index {
            inner_num = parse_int_token(&mut hlex, "ObjStm header object number")?;
            rel = parse_int_token(&mut hlex, "ObjStm header offset")?;
        }
        let start = match usize::try_from(rel).ok().and_then(|r| first.checked_add(r)) {
            Some(s) if s < bytes.len() => s,
            _ => {
                self.push_warning(format!(
                    "object stream {stream_num}: object {num} offset out of range"
                ));
                return Ok(None);
            }
        };
        let lex_base = base + start as u64;
        let mut lex = Lexer::at(&bytes[start..], lex_base);
        let mut value = parse_object(&mut lex)?;
        // §7.5.7 禁止把流对象存进对象流（模块文档引用的第 2240–2241 行）。
        // 违例文档里被打包对象仍可能自带 `stream … endstream`：修复前这里只
        // 把字典交给调用方、正文被**静默丢弃**，写端随后把它当普通字典重新
        // 打包 → 一次往返即丢内容。现在读端保住正文（切成 `Owned`，因为载荷
        // 是解码后的内存副本、其偏移在源文件里不可寻址，不能用 `Ref`），
        // 并登记类型化违例交写端拒收。
        let mut violation = None;
        if let Value::Dict(d) = &value
            && lex.peek_is_stream_keyword()
        {
            // 内联流的 `/Length` 若写成间接引用，指向的是载荷内的对象、与
            // 文件偏移不可对应，故不解析（resolver 恒 `None` → 回退扫描
            // `endstream`）。尾部畸形时不升级为错误：该文件已违例，把整个
            // 对象判成不可解比保留字典更糟。
            let recovered = match parse_stream_tail(&mut lex, d, &mut |_, _| Ok(None)) {
                Ok((abs, len)) => abs
                    .checked_sub(lex_base)
                    .and_then(|rel| usize::try_from(rel).ok())
                    .and_then(|rel| {
                        let end = rel.checked_add(len as usize)?;
                        bytes[start..].get(rel..end).map(<[u8]>::to_vec)
                    }),
                Err(_) => None,
            };
            violation = Some(ObjectStreamViolation::StreamObjectInObjectStream {
                object_number: inner_num,
                stream_number: stream_num,
                index,
                recovered: recovered.is_some(),
            });
            if let Some(body) = recovered {
                value = Value::Stream(d.clone(), crate::core::types::StreamData::Owned(body));
            }
        }
        // 对象流内对象不带 `N G obj` 包裹，其 generation 规范上为 0。
        let obj = Object::new(inner_num, 0, value);
        Ok(Some((obj, violation)))
    }
}
