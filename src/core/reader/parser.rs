//! 文件级 `Parser`：窗口读取与 xref 策略链装配。

use crate::core::types::{Document, ObjectSecurity, PdfVersion, Trailer, Value, Xref};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::sync::Arc;
use zeroize::Zeroizing;

use super::lexer::Lexer;
use super::object::parse_startxref;
use super::security::{RustCryptoBackend, SecurityHandler};
use super::xref::{XrefChain, XrefContext, merge_xref, parse_xref_stream};
use super::{DEFAULT_WINDOW, Result, TAIL_WINDOW, err, find_subslice};

// ── 对象源 trait ─────────────────────────────────────────────────────

/// 对象源：从底层文件按绝对偏移读取窗口字节。
///
/// 文件读取抽象（按偏移 Seek+Read + 文件长度）。
///
/// [`ParserSource`] 实现此 trait（持 `&mut BufReader<File>`），供 xref
/// 策略链与全文扫描按需读取流字节/对象窗口，无需把整个文件加载到内存。
pub(crate) trait ObjectSource {
    fn read_at(&mut self, offset: u64, len: u64) -> Option<Vec<u8>>;
    /// 文件总长度（字节）。
    fn file_len(&self) -> u64;
}

// ── 文件级 Parser：持 BufReader<File> ───────────────────────────────

/// 递归下降语法分析器的文件级入口。
///
/// 持有 `BufReader<File>`（而非整个文件字节），按需 `seek+read` 局部
/// 窗口构造 [`Lexer`]，调用自由解析函数与 xref 策略链装配 [`Document`]。
/// 绝不把整个文件读入内存。
pub struct Parser {
    /// 源文件读取器。`parse_document` 会 `take()` 后移交给 [`Document`]，
    /// 移交后为 `None`（`Parser` 为一次性使用）。
    reader: Option<BufReader<File>>,
    file_len: u64,
    password: Option<Zeroizing<Vec<u8>>>,
}

impl Parser {
    /// 打开文件构造解析器。读取文件长度用于末尾窗口计算。
    pub fn new(file: File) -> io::Result<Self> {
        let file_len = file.metadata()?.len();
        Ok(Self {
            reader: Some(BufReader::new(file)),
            file_len,
            password: None,
        })
    }

    /// Configure the password used to open an encrypted PDF.
    ///
    /// Password bytes are truncated according to the selected security-handler
    /// revision. Without this option the parser tries the empty password.
    pub fn with_password(mut self, password: impl AsRef<[u8]>) -> Self {
        self.password = Some(Zeroizing::new(password.as_ref().to_vec()));
        self
    }

    /// 借用底层读取器。仅在 `parse_document` 已移交 reader 后才返回错误。
    fn reader_mut(&mut self) -> Result<&mut BufReader<File>> {
        self.reader
            .as_mut()
            .ok_or_else(|| err(0, "file reader already consumed by parse_document"))
    }

    /// 从绝对偏移 `offset` 读取 `len` 字节窗口。越界部分截到文件末尾。
    fn read_window(&mut self, offset: u64, len: usize) -> Result<Vec<u8>> {
        if offset >= self.file_len {
            return Ok(Vec::new());
        }
        let avail = self.file_len - offset;
        let take = (len as u64).min(avail) as usize;
        let reader = self.reader_mut()?;
        reader
            .seek(SeekFrom::Start(offset))
            .map_err(|e| err(offset, format!("seek failed: {e}")))?;
        let mut buf = vec![0u8; take];
        let mut filled = 0usize;
        while filled < take {
            match reader.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(err(offset, format!("read failed: {e}"))),
            }
        }
        buf.truncate(filled);
        Ok(buf)
    }

    /// 读取文件末尾 `len` 字节窗口（不足则返回全部）。
    fn read_tail(&mut self, len: usize) -> Result<Vec<u8>> {
        let start = self.file_len.saturating_sub(len as u64);
        self.read_window(start, len)
    }

    /// 解析整个 PDF 文件的**文件级结构**：读文件头版本、定位末尾
    /// `startxref`、回溯 xref + trailer，装配为 [`Document`]。
    ///
    /// **懒加载**：此处**不**解析任何间接对象（除回退所需）。对象按
    /// 需由 [`Document::get_object`] 触发。
    ///
    /// **窗口式**：只读取末尾 2048 字节定位 startxref，再从 startxref
    /// 偏移读 64KB 窗口装下 xref + trailer；全文扫描回退走分块流式。
    /// 整个过程不把文件全量读入内存。
    ///
    /// **兼容/回退**：当 startxref 指向位置不是经典 `xref` 关键字
    /// 时（PDF 1.5+ XRef 流），按三层策略回退：
    /// 1. 经典 `xref` 表（所有版本合法）
    /// 2. PDF 1.5 XRef 流（未压缩时可解析；压缩则跳过此层）
    /// 3. 全文扫描所有 `N G obj ... endobj` 自建 xref + 取最后 trailer
    ///
    /// 每层失败转 warning，不中断；最终仍失败才返回 `Err`。
    pub fn parse_document(&mut self) -> Result<Document> {
        let mut warnings = Vec::new();

        // 0. 读文件头版本（读头部 1KB 窗口）。识别 1.0–1.7 与 2.0：
        //    2.0 与 1.x 一视同仁地正常解析（不因版本偏高而拒绝读取），
        //    仅当头部完全缺失/不可解析时才回退为 1.0 并记 warning。
        let head = self.read_window(0, 1024)?;
        let mut version = match PdfVersion::parse(&head) {
            Some(v) => v,
            None => {
                warnings.push("no valid %PDF-x.y header found; assuming version 1.0".to_string());
                PdfVersion::V1_0
            }
        };

        // 1. 末尾扫描 startxref（取最后一个出现位置）
        let tail = self.read_tail(TAIL_WINDOW)?;
        let tail_base = self.file_len.saturating_sub(tail.len() as u64);
        let needle = b"startxref";
        let mut sx_off: Option<u64> = None;
        let mut from = 0usize;
        while let Some(p) = find_subslice(&tail, needle, from) {
            sx_off = Some(tail_base + p as u64);
            from = p + needle.len();
        }
        let sx_off = sx_off.ok_or_else(|| err(0, "startxref not found"))?;

        // 2. 从 startxref 起解析 startxref 值（在 tail 窗口内构造 Lexer）
        let sx_rel = sx_off.saturating_sub(tail_base) as usize;
        let sx_win = &tail[sx_rel..];
        let startxref = {
            let mut lex = Lexer::at(sx_win, sx_off);
            parse_startxref(&mut lex)?
        };

        // 3. xref + trailer：三层回退（对应**最新修订**）
        let xref_off = startxref.0 as u64;
        let (mut xref, trailer) = self.resolve_xref_at(xref_off, version, &mut warnings)?;

        let security = self.authenticate_encryption(&xref, &trailer, &startxref, version)?;

        // 3.5 有效版本：根 Catalog 字典的 /Version（形如 /1.5）可覆盖
        //     文件头声明版本（PDF 1.4+），取两者较大者。Catalog 是间接
        //     对象，且**可能被打包在对象流内**（那时 xref 条目没有文件
        //     偏移，只有 `stream_num`/`index`），故用一个临时 Document
        //     （克隆的文件句柄 + 已解析的 xref/trailer）复用 get_object
        //     的定位逻辑，普通条目与对象流内条目一并覆盖。解析失败或
        //     版本不支持则忽略覆盖。
        if let Some((root_num, _)) = trailer.root()
            && let Ok(cloned) = File::try_clone(self.reader_mut()?.get_ref())
        {
            let probe = Document::from_source(
                BufReader::new(cloned),
                xref.clone(),
                trailer.clone(),
                startxref.clone(),
                version,
                Vec::new(),
                None,
            );
            if let Ok(Some(root)) = probe.get_object(root_num)
                && let Some(v) = root
                    .value
                    .as_dict()
                    .and_then(|d| d.get("Version"))
                    .and_then(|v| v.as_name())
                    .and_then(PdfVersion::from_name)
                && v > version
            {
                version = v;
            }
        }

        // 3.6 增量更新（incremental update）：最新修订的 trailer（或 XRef 流
        //     字典）可含 `/Prev <整数偏移>`，指向上一个修订的 xref 偏移，
        //     形成一条回溯到文件最初版本的链。沿链回溯并把历史修订的条目
        //     **追加到 `xref` 末尾**——[`Xref::get`] 取首个命中子段，故最新
        //     修订始终优先，历史条目只补充新修订未提及的对象号（含「最新
        //     修订把对象标为 free」→ `get_object` 返回 None，不回退历史旧值）。
        //     此处用已确定的**有效版本** `version` 解析历史偏移，避免版本
        //     门控把历史的 XRef 流挡掉。
        self.follow_prev_chain(&mut xref, &trailer, xref_off, version, &mut warnings);

        // 4. 装配 Document：把 Parser 持有的读取器**移交**给 Document。
        //    `parse_document` 是一次性语义，移交后 `Parser` 不可再用
        //    （`reader` 变为 `None`），因此无需克隆文件句柄或留占位。
        let reader = self
            .reader
            .take()
            .ok_or_else(|| err(0, "file reader already consumed by parse_document"))?;
        Ok(Document::from_source(
            reader, xref, trailer, startxref, version, warnings, security,
        ))
    }

    fn authenticate_encryption(
        &mut self,
        xref: &Xref,
        trailer: &Trailer,
        startxref: &crate::core::types::StartXref,
        version: PdfVersion,
    ) -> Result<Option<Arc<dyn ObjectSecurity>>> {
        let Some(encrypt_value) = trailer.dict.get("Encrypt") else {
            return Ok(None);
        };
        if matches!(encrypt_value, Value::Null) {
            return Ok(None);
        }

        let (encrypt_dict, encrypt_dict_ref) = match encrypt_value {
            Value::Dict(dict) => (dict.clone(), None),
            Value::Ref(num, generation) => {
                let file =
                    self.reader_mut()?.get_ref().try_clone().map_err(|e| {
                        err(0, format!("failed to read encryption dictionary: {e}"))
                    })?;
                let document = Document::from_source(
                    BufReader::new(file),
                    xref.clone(),
                    trailer.clone(),
                    startxref.clone(),
                    version,
                    Vec::new(),
                    None,
                );
                let object = document
                    .get_object(*num)?
                    .filter(|object| object.generation == *generation)
                    .ok_or_else(|| err(0, "invalid trailer /Encrypt reference"))?;
                let dict = object
                    .value
                    .as_dict()
                    .ok_or_else(|| err(0, "trailer /Encrypt does not reference a dictionary"))?
                    .clone();
                (dict, Some((*num, *generation)))
            }
            _ => return Err(err(0, "invalid trailer /Encrypt value")),
        };

        let id = trailer
            .dict
            .get("ID")
            .ok_or_else(|| err(0, "encrypted PDF is missing trailer /ID"))?;
        let password = self.password.take().unwrap_or_default();
        let security = SecurityHandler::authenticate(
            &encrypt_dict,
            id,
            &password,
            encrypt_dict_ref,
            &RustCryptoBackend,
        )
        .map_err(|_| err(0, "invalid password or unsupported PDF encryption"))?;
        Ok(Some(Arc::new(security)))
    }

    /// 在**任意文件偏移** `xref_off` 处解析一份 `(xref, trailer)`。
    ///
    /// 三层回退：经典 xref 表 → XRef 流 → 全文扫描。实现为
    /// [`XrefStrategy`] 责任链：每个策略判断能否处理当前上下文并尝试解析，
    /// 失败则记 warning 传递给下一策略。新增 xref 解析方式只需实现 trait
    /// 并加入链，不改本方法主干。
    ///
    /// 抽取为可复用于任意偏移的独立步骤：既用于 startxref 指向的**最新
    /// 修订**，也用于沿 `/Prev` 链回溯到的**历史修订**（历史修订可能是经典
    /// 表，也可能是 XRef 流）。版本门控用传入的 `version`——回溯历史时传
    /// 文件已确定的有效版本，避免把历史 XRef 流挡掉。若解析出的 trailer 含
    /// 混合引用的 `/XRefStm`，也在此一并解析并合并。
    fn resolve_xref_at(
        &mut self,
        xref_off: u64,
        version: PdfVersion,
        warnings: &mut Vec<String>,
    ) -> Result<(Xref, Trailer)> {
        // 读 xref 窗口：从 startxref 偏移起 64KB，足够装下 xref + trailer。
        let xref_win = self.read_window(xref_off, DEFAULT_WINDOW)?;
        let ctx = XrefContext { xref_off, version };
        let chain = XrefChain::default();
        // 策略链需要的 ObjectSource 用一个临时适配器，它持 &mut self.reader。
        let file_len = self.file_len;
        let mut src = ParserSource {
            reader: self.reader_mut()?,
            file_len,
        };
        let (mut xref, mut trailer) =
            chain.resolve(&xref_win, xref_off, &mut src, &ctx, warnings)?;

        // 混合引用文件（hybrid-reference）：经典 trailer 含 /XRefStm，
        // 指向一个补充 XRef 流，其条目对经典表不可见。按规范解析该流并
        // 并入结果：同号以 XRef 流条目为准；流字典中经典 trailer 缺失的
        // 字段（如 /Root /Info）一并补齐。该路径不依赖头部版本门控。
        let xref_stm_off = trailer
            .dict
            .get("XRefStm")
            .and_then(|v| v.as_integer())
            .map(|v| v.max(0) as u64);
        if let Some(stm_off) = xref_stm_off {
            match src.read_at(stm_off, DEFAULT_WINDOW as u64) {
                Some(win) if !win.is_empty() => {
                    let mut lex = Lexer::at(&win, stm_off);
                    match parse_xref_stream(&mut lex, stm_off, &mut src) {
                        Ok((sx, st)) => {
                            merge_xref(&mut xref, &sx);
                            for (k, v) in st.dict.0.iter() {
                                if trailer.dict.get(k).is_none() {
                                    trailer.dict.0.insert(k.clone(), v.clone());
                                }
                            }
                        }
                        // 压缩流等失败不致命，记 warning 继续用经典表。
                        Err(e) => warnings.push(format!("/XRefStm stream parse failed: {e}")),
                    }
                }
                _ => warnings.push(format!("/XRefStm read failed at offset {stm_off}")),
            }
        }

        Ok((xref, trailer))
    }

    /// 沿 `/Prev` 链回溯增量更新（incremental update）的历史修订，并把历史
    /// 条目并入 `xref`（追加到末尾）。
    ///
    /// 从**最新修订**的 `newest_trailer` 出发，取其字典里的正整数 `/Prev`
    /// 偏移，用 [`Parser::resolve_xref_at`] 在历史偏移解析出该修订的
    /// `(xref_prev, trailer_prev)`（经典表与 XRef 流均由策略链覆盖，含各自
    /// 的 `/XRefStm` 补充流），把 `xref_prev` 的条目**追加到 `xref` 末尾**：
    /// [`Xref::get`] 按顺序取首个命中子段，最新修订因此始终优先，历史条目
    /// 只补充新修订未提及的对象号。随后以 `trailer_prev` 继续回溯其 `/Prev`；
    /// `newest_trailer` 保持为文档 trailer，不被覆盖。
    ///
    /// **防死循环**：用已访问偏移集合拦截自引用/成环，另设迭代次数上限，
    /// 命中重复偏移或超限即记 warning 并停止。回溯中任一步失败（窗口读不到 /
    /// 策略链全失败）不致命：记 warning 后停止，保留已解析到的部分。
    fn follow_prev_chain(
        &mut self,
        xref: &mut Xref,
        newest_trailer: &Trailer,
        start_off: u64,
        version: PdfVersion,
        warnings: &mut Vec<String>,
    ) {
        // 回溯迭代次数上限：防御损坏文件构造的极长 `/Prev` 链。
        const MAX_PREV_DEPTH: usize = 64;
        // 已访问偏移：拦截自引用 / 成环导致的死循环。
        let mut visited: BTreeSet<u64> = BTreeSet::new();
        visited.insert(start_off);

        // 当前修订的 trailer：首轮为最新修订，其后为上一层历史修订。
        let mut cursor: Trailer = newest_trailer.clone();
        for _ in 0..MAX_PREV_DEPTH {
            let prev_off = match cursor.dict.get("Prev").and_then(|v| v.as_integer()) {
                Some(v) if v > 0 => v as u64,
                _ => break, // 链已到头：无 /Prev 或非正偏移
            };
            if visited.contains(&prev_off) {
                warnings.push(format!(
                    "incremental update: /Prev offset {prev_off} revisited; stopping chain"
                ));
                break;
            }
            visited.insert(prev_off);
            match self.resolve_xref_at(prev_off, version, warnings) {
                Ok((xref_prev, trailer_prev)) => {
                    // 历史条目追加到末尾：新修订优先，历史仅补未覆盖的号。
                    xref.0.extend(xref_prev.0);
                    // 以本层 trailer 继续回溯其 /Prev。
                    cursor = trailer_prev;
                }
                Err(e) => {
                    warnings.push(format!(
                        "incremental update: /Prev xref parse failed at offset {prev_off}: {e}; \
                         keeping partial xref"
                    ));
                    break;
                }
            }
        }
    }
}

/// `Parser` 借用期间的对象源适配器：把 `&mut BufReader<File>` 实现
/// [`ObjectSource`]，供策略链读取流字节 / 倒找 xref 窗口。
struct ParserSource<'a> {
    reader: &'a mut BufReader<File>,
    file_len: u64,
}

impl<'a> ObjectSource for ParserSource<'a> {
    fn read_at(&mut self, offset: u64, len: u64) -> Option<Vec<u8>> {
        if offset >= self.file_len {
            return None;
        }
        if self.reader.seek(SeekFrom::Start(offset)).is_err() {
            return None;
        }
        let take = (len as usize).min((self.file_len - offset) as usize);
        let mut buf = vec![0u8; take];
        let mut filled = 0;
        while filled < take {
            match self.reader.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(_) => return None,
            }
        }
        buf.truncate(filled);
        if buf.is_empty() && take > 0 {
            return None;
        }
        Some(buf)
    }

    fn file_len(&self) -> u64 {
        self.file_len
    }
}
