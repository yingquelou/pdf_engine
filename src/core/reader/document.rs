//! `Document` 的懒加载对象解析（xref → 窗口 → 间接对象/对象流）。

use crate::core::filters::decode_stream;
use crate::core::types::{Document, Object, Value, XrefKind, scan_value_features};

use super::lexer::Lexer;
use super::object::{parse_indirect_object_with_length_resolver, parse_int_token, parse_object};
use super::{DEFAULT_WINDOW, Result, err};

impl Document {
    /// 按 `num` 懒加载对象：查缓存 → 查 xref → 按条目种类定位并解析，
    /// 结果存入缓存后返回克隆。
    ///
    /// 条目种类决定路径：
    /// - [`XrefKind::Normal`]：按 `offset` 从源文件读 64KB 窗口解析
    ///   `N G obj ... endobj`；
    /// - [`XrefKind::InObjStream`]：对象打包在对象流（ObjStm）内，由内部
    ///   `get_object_from_obj_stream` 解包；
    /// - [`XrefKind::Free`]：自由条目，对象不存在。
    ///
    /// 返回 `Ok(None)` 表示 xref 中无此对象号、对象为自由条目，或对象流
    /// 内对象因（压缩等）无法解析而放弃；返回 `Err` 表示偏移越界或对象
    /// 语法错误。
    ///
    /// 命中缓存直接克隆返回，不重复解析。写入端构造的 Document
    /// （source 为 None）直接查缓存返回。
    pub fn get_object(&self, num: i64) -> Result<Option<Object>> {
        // 写入端构造（source 为 None）时直接查缓存
        if self.source.is_none() {
            return Ok(self.cache.borrow().get(&num).cloned());
        }
        // 读端：先查缓存（以对象号为键，O(1) 命中）
        if let Some(obj) = self.cache.borrow().get(&num) {
            return Ok(Some(obj.clone()));
        }
        // 查 xref 拿条目
        let entry = match self.xref.get(num) {
            Some(e) => e,
            None => return Ok(None),
        };
        match &entry.kind {
            XrefKind::Free => Ok(None),
            XrefKind::Normal => {
                // 按偏移从文件读窗口解析并填入缓存
                let offset = entry.offset as u64;
                let win = self
                    .read_at(offset, DEFAULT_WINDOW)
                    .ok_or_else(|| err(offset, "read object window failed"))?;
                if win.is_empty() {
                    return Err(err(offset, "empty read at object offset"));
                }
                let mut lex = Lexer::at(&win, offset);
                let mut obj = parse_indirect_object_with_length_resolver(
                    &mut lex,
                    |length_num, generation| self.resolve_stream_length(length_num, generation),
                )?;
                if let Some(security) = &self.security {
                    security.decrypt_object(&mut obj).map_err(|message| {
                        err(offset, format!("object decryption failed: {message}"))
                    })?;
                }
                // 顺带从刚解析出的对象里扫描特性，填充 FeatureProfile。
                // 这使得懒加载路径也能自然累积特性——writer 后续调
                // load_all_objects 只是强制把所有对象都解析一遍。
                scan_value_features(&mut self.features.borrow_mut(), &obj.value);
                self.cache.borrow_mut().insert(obj.num, obj.clone());
                Ok(Some(obj))
            }
            XrefKind::InObjStream { stream_num, index } => {
                self.get_object_from_obj_stream(num, *stream_num, *index)
            }
        }
    }

    fn resolve_stream_length(&self, num: i64, generation: i64) -> Result<Option<u64>> {
        {
            let mut stack = self.stream_length_stack.borrow_mut();
            if !stack.insert((num, generation)) {
                return Err(err(
                    0,
                    format!("cyclic indirect stream /Length reference {num} {generation} R"),
                ));
            }
        }

        let result = (|| {
            self.get_object(num)?
                .filter(|object| object.generation == generation)
                .ok_or_else(|| {
                    err(
                        0,
                        format!("invalid indirect stream /Length {num} {generation} R"),
                    )
                })
                .and_then(|object| match object.value {
                    Value::Integer(length) => u64::try_from(length)
                        .map(Some)
                        .map_err(|_| err(0, "invalid negative indirect stream /Length")),
                    _ => Err(err(0, "indirect stream /Length object is not an integer")),
                })
        })();
        self.stream_length_stack
            .borrow_mut()
            .remove(&(num, generation));
        result
    }

    /// Read and decode a stream, including the document's standard `/Crypt`
    /// filter when opened with a password.
    pub fn decode_stream_bytes(&self, value: &Value) -> std::result::Result<Vec<u8>, String> {
        let Value::Stream(dict, _) = value else {
            return Err("value is not a PDF stream".to_string());
        };
        let raw = self.try_stream_bytes(value)?;
        let dict = Self::remove_crypt_filter(dict, self.is_encrypted())?;
        decode_stream(&dict, &raw).map_err(|error| error.to_string())
    }

    fn remove_crypt_filter(
        dict: &crate::core::types::Dict,
        has_security_context: bool,
    ) -> std::result::Result<crate::core::types::Dict, String> {
        let Some(filter_value) = dict.get("Filter") else {
            return Ok(dict.clone());
        };
        let (filters, was_array) = match filter_value {
            Value::Name(name) => (vec![name.as_str()], false),
            Value::Array(array) => (
                array
                    .iter()
                    .map(|value| {
                        value
                            .as_name()
                            .ok_or_else(|| "invalid PDF stream /Filter array".to_string())
                    })
                    .collect::<std::result::Result<Vec<_>, _>>()?,
                true,
            ),
            _ => return Err("invalid PDF stream /Filter".to_string()),
        };
        let Some(crypt_index) = filters.iter().position(|name| *name == "Crypt") else {
            return Ok(dict.clone());
        };
        if crypt_index != 0 {
            return Err("PDF /Crypt filter must be the first stream filter".to_string());
        }
        if !has_security_context {
            let params = match dict.get("DecodeParms").or_else(|| dict.get("DP")) {
                Some(Value::Dict(params)) => Some(params),
                Some(Value::Array(params)) => match params.0.first() {
                    Some(Value::Dict(params)) => Some(params),
                    Some(Value::Null) | None => None,
                    Some(_) => return Err("invalid /Crypt DecodeParms entry".to_string()),
                },
                None | Some(Value::Null) => None,
                Some(_) => return Err("invalid PDF stream /DecodeParms".to_string()),
            };
            let crypt_name = match params.and_then(|params| params.get("Name")) {
                Some(Value::Name(name)) => name.as_str(),
                None => "Identity",
                Some(_) => return Err("invalid /Crypt DecodeParms /Name".to_string()),
            };
            if crypt_name != "Identity" {
                return Err("/Crypt filter requires an authenticated PDF document".to_string());
            }
        }

        let mut result = dict.clone();
        let remaining: Vec<Value> = filters
            .iter()
            .enumerate()
            .filter(|&(index, _)| index != crypt_index)
            .map(|(_, name)| Value::Name(name.to_string()))
            .collect();
        if remaining.is_empty() {
            result.0.remove("Filter");
        } else if was_array {
            result.0.insert(
                "Filter".to_string(),
                Value::Array(crate::core::types::Array(remaining)),
            );
        } else {
            result.0.insert(
                "Filter".to_string(),
                remaining.into_iter().next().expect("one filter remains"),
            );
        }

        let parms_key = if dict.0.contains_key("DecodeParms") {
            "DecodeParms"
        } else {
            "DP"
        };
        match dict.get("DecodeParms").or_else(|| dict.get("DP")) {
            Some(Value::Dict(_)) if filters.len() == 1 => {
                result.0.remove("DecodeParms");
                result.0.remove("DP");
            }
            Some(Value::Dict(_)) => {
                return Err("PDF /DecodeParms must be an array for multiple filters".to_string());
            }
            Some(Value::Array(parms)) => {
                let remaining: Vec<Value> = parms
                    .iter()
                    .enumerate()
                    .filter(|&(index, _)| index != crypt_index)
                    .map(|(_, value)| value.clone())
                    .collect();
                if remaining.is_empty()
                    || remaining.iter().all(|value| matches!(value, Value::Null))
                {
                    result.0.remove(parms_key);
                    result.0.remove(if parms_key == "DecodeParms" {
                        "DP"
                    } else {
                        "DecodeParms"
                    });
                } else {
                    result.0.insert(
                        "DecodeParms".to_string(),
                        Value::Array(crate::core::types::Array(remaining)),
                    );
                    result.0.remove("DP");
                }
            }
            None | Some(Value::Null) => {}
            Some(_) => return Err("invalid PDF stream /DecodeParms".to_string()),
        }
        Ok(result)
    }

    /// 解析「位于对象流（ObjStm）内」的第 `index` 个对象。
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
    fn get_object_from_obj_stream(
        &self,
        num: i64,
        stream_num: i64,
        index: i64,
    ) -> Result<Option<Object>> {
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
        let mut lex = Lexer::at(&bytes[start..], base + start as u64);
        let value = parse_object(&mut lex)?;
        // 对象流内对象不带 `N G obj` 包裹，其 generation 规范上为 0。
        let obj = Object::new(inner_num, 0, value);
        // 顺带扫描特性，与 Normal 路径保持一致。
        scan_value_features(&mut self.features.borrow_mut(), &obj.value);
        self.cache.borrow_mut().insert(obj.num, obj.clone());
        Ok(Some(obj))
    }

    /// 追加一条解析警告（`&self` 下经 `Document::warnings` 的 `RefCell`
    /// 内部可变）。对整个 crate 可见（writer 在预加载失败时也需追加）。
    pub(crate) fn push_warning(&self, msg: String) {
        self.warnings.borrow_mut().push(msg);
    }

    /// 已缓存对象的数量（用于测试与诊断）。不触发新的解析。
    pub fn cached_object_count(&self) -> usize {
        self.cache.borrow().len()
    }

    /// 预加载全部对象到缓存，并填充完整的 FeatureProfile。
    ///
    /// 遍历 `xref` 所有子段的所有条目：
    /// - `XrefKind::Normal` → 调 `get_object(num)?`
    /// - `XrefKind::InObjStream` → 调 `get_object_from_obj_stream`（同样经 `get_object`）
    /// - `XrefKind::Free` → 跳过
    ///
    /// 异常条目记 warning 后跳过，不中断。无 `source`（写入端构造的
    /// Document）时直接返回 Ok(())——对象已在缓存、特性已由 `from_objects`
    /// 填充。
    ///
    /// 读端宽解压后的流字节（`StreamData::Owned`）会完整驻留内存——
    /// 这是「宽进严出」的必要代价，writer 写出时再按目标版本重新表达。
    pub fn load_all_objects(&self) -> Result<()> {
        if self.source.is_none() {
            // 写入端构造：对象已全在 cache，features 已填充
            return Ok(());
        }
        for sub in &self.xref.0 {
            for (i, e) in sub.entries.iter().enumerate() {
                let num = sub.start + i as i64;
                // `/Prev` 链按「最新段在前」排列；已在缓存的对象即最新版本，
                // 历史段的旧条目（尤其是直接走 get_object_from_obj_stream 的
                // InObjStream 路径）不得覆盖，否则全量重写会回退到旧版本。
                if self.cache.borrow().contains_key(&num) {
                    continue;
                }
                match &e.kind {
                    XrefKind::Free => continue,
                    XrefKind::Normal => {
                        if let Err(err) = self.get_object(num) {
                            self.push_warning(format!(
                                "load_all_objects: object {num} failed: {err}"
                            ));
                        }
                    }
                    XrefKind::InObjStream { stream_num, index } => {
                        if let Err(err) = self.get_object_from_obj_stream(num, *stream_num, *index)
                        {
                            self.push_warning(format!(
                                "load_all_objects: object {num} (in obj stream) failed: {err}"
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
