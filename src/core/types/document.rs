//! 懒加载 `Document` 与源文件读取。

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

use super::features::{FeatureProfile, scan_dict_features, scan_value_features};
use super::value::{Object, StartXref, StreamData, Trailer, Value, Xref};
use super::version::PdfVersion;

pub(crate) trait ObjectSecurity: Send + Sync + std::fmt::Debug {
    fn decrypt_object(&self, object: &mut Object) -> Result<(), String>;
    fn decrypt_stream(
        &self,
        object_num: i64,
        generation: i64,
        dict: &super::value::Dict,
        bytes: &[u8],
    ) -> Result<Vec<u8>, String>;

    /// 加密字典 `/P` 的有符号原值与处理器 revision（`/R`），供读端
    /// 权限模型解释（7.6.3.2、Table 22；解码见
    /// `crate::core::reader::security::Permissions`）。
    ///
    /// 默认 `None`：该处理器不公开 `/P` 视图（公钥安全处理器 7.6.4
    /// 未实现）。这里只回传原始整数而不引入 reader 侧类型，以保持
    /// `types` 层的对象模型纯度（库策略）。
    fn permission_entry(&self) -> Option<(i32, u8)> {
        None
    }
}

/// 完整 PDF 文件在内存中的表示：源字节 + xref + trailer + startxref
/// + 已解析对象缓存。
///
/// **懒加载**：打开时只解析 xref/trailer/startxref，对象按需由
/// [`crate::core::reader`] 模块为 `Document` 实现的 `get_object` 触发
/// 解析并缓存。典型场景（渲染指定页面）只解析沿引用链上的少数对象，
/// 而非整本 PDF。
///
/// 写入端（程序构造而非从文件读）用 [`Document::from_objects`]，
/// 此时 source 为空、对象全部预置在缓存里。
///
/// `Document` 非线程安全（缓存用 `RefCell`），单线程使用。
/// `source` 用 `Arc<Mutex<BufReader<File>>>`，对象与流字节按偏移
/// Seek+Read 读取，流对象仅存偏移而非拷贝字节。
#[derive(Debug)]
pub struct Document {
    /// 源文件读取器（读端）；写入端构造时为 `None`。
    ///
    /// 对象与流按 xref/`StreamData::Ref` 偏移通过 Seek+Read
    /// **按需读取**，不把整个文件加载到内存。`Arc<Mutex>` 允许
    /// 多处借用并串行化访问（单线程使用，锁竞争为零）。
    pub source: Option<Arc<Mutex<BufReader<File>>>>,
    pub xref: Xref,
    pub trailer: Trailer,
    pub startxref: StartXref,
    /// 源文档声明的版本（读端填充），版本中立元数据。
    ///
    /// 只反映文件头 `%PDF-x.y`（与根 Catalog `/Version` 覆盖后的
    /// 最终值）；**不**约束输出特性——writer 默认用自己的目标版本
    /// （`PdfVersion::default()` = 1.7），完全与源版本解耦。
    pub source_version: PdfVersion,
    /// 文档实际用到的特性画像（读端填充、写端消费）。
    ///
    /// 读端在 `get_object` 解析对象时自动扫描并累计；
    /// writer 在构建前调用 [`Document::load_all_objects`] 预加载
    /// 全部对象后即覆盖完整画像，用于「不可降级特性 → 报错」与
    /// 「可降级结构（ObjStm/XRef 流）→ 自动退回经典形态」。
    ///
    /// 用 `RefCell` 包裹：与 `warnings`/`cache` 一样，`get_object`
    /// 是 `&self` 但需要内部可变填充。
    pub features: RefCell<FeatureProfile>,
    /// 解析过程中的非致命问题（回退、版本不符、对象流无法解码等）。
    /// reader 填充。用 `RefCell` 包裹：`get_object`（取 `&self`）在解析
    /// 对象流内对象失败时需追加说明性警告。
    pub warnings: RefCell<Vec<String>>,
    /// 已解析对象缓存：`num -> Object`。以对象号为键（O(1) 命中）；
    /// 同一对象号若存在多个 generation，以最新修订为准——与 xref
    /// 查找（同样只认最新条目）保持一致。
    pub(crate) cache: RefCell<HashMap<i64, Object>>,
    /// 标记删除的对象号（E7 增量更新）：写出时产出自由条目遮蔽历史条目。
    pub(crate) deleted: RefCell<HashSet<i64>>,
    pub(crate) security: Option<Arc<dyn ObjectSecurity>>,
    pub(crate) stream_length_stack: RefCell<HashSet<(i64, i64)>>,
}

impl Document {
    /// 空文档（写入端构造起点）。等价于 `Default::default()`。
    pub fn new() -> Self {
        Self::default()
    }

    /// 写入端构造：从一组预解析对象出发，无源文件、
    /// xref/trailer 由调用方另行提供。
    pub fn from_objects(
        objects: Vec<Object>,
        xref: Xref,
        trailer: Trailer,
        startxref: StartXref,
    ) -> Self {
        // 先扫描所有传入对象 + trailer.dict 的特性，构造初始 FeatureProfile。
        let mut features = FeatureProfile::new();
        for o in &objects {
            scan_value_features(&mut features, &o.value);
        }
        // trailer.dict 含 /Encrypt、XRef 流 trailer 结构等文件级特性，
        // 不从对象扫描路径经过，故需单独补充扫描。
        scan_dict_features(&mut features, &trailer.dict);
        let cache = objects.into_iter().map(|o| (o.num, o)).collect();
        Self {
            source: None,
            xref,
            trailer,
            startxref,
            // 写入端构造的 Document 无源文件，source_version 取默认 1.7
            // （版本中立元数据，不约束 writer 输出版本）。
            source_version: PdfVersion::default(),
            features: RefCell::new(features),
            warnings: RefCell::new(Vec::new()),
            cache: RefCell::new(cache),
            deleted: RefCell::new(HashSet::new()),
            security: None,
            stream_length_stack: RefCell::new(HashSet::new()),
        }
    }

    /// 读端构造：源文件读取器 + 文件级结构 + 声明版本，缓存初始为空。
    /// 对象/流按偏移从文件按需读取，不预加载。
    pub(crate) fn from_source(
        source: BufReader<File>,
        xref: Xref,
        trailer: Trailer,
        startxref: StartXref,
        version: PdfVersion,
        warnings: Vec<String>,
        security: Option<Arc<dyn ObjectSecurity>>,
    ) -> Self {
        // 从文件级结构（trailer/XRef 流字典）预扫一遍特性——
        // XRef 流的 /Type /XRef 与 trailer /Encrypt 能在不解析对象的情况下
        // 就被发现，这样懒加载 Document 在只解析根 Catalog 时也能知道
        // 文档用了 XRef 流。
        let mut features = FeatureProfile::new();
        scan_dict_features(&mut features, &trailer.dict);
        Self {
            source: Some(Arc::new(Mutex::new(source))),
            xref,
            trailer,
            startxref,
            source_version: version,
            features: RefCell::new(features),
            warnings: RefCell::new(warnings),
            cache: RefCell::new(HashMap::new()),
            deleted: RefCell::new(HashSet::new()),
            security,
            stream_length_stack: RefCell::new(HashSet::new()),
        }
    }

    pub fn xref(&self) -> &Xref {
        &self.xref
    }

    pub fn trailer(&self) -> &Trailer {
        &self.trailer
    }

    pub fn startxref(&self) -> &StartXref {
        &self.startxref
    }

    /// 源文档声明的版本（读端填充，版本中立元数据）。
    pub fn source_version(&self) -> PdfVersion {
        self.source_version
    }

    /// Whether this document was opened with an authenticated encryption context.
    pub fn is_encrypted(&self) -> bool {
        self.security.is_some()
    }

    /// 解析过程中的非致命问题副本。因 `warnings` 存于 `RefCell`，
    /// 内部借用不可外泄，这里返回克隆（量小，调用不频繁）。
    pub fn warnings(&self) -> Vec<String> {
        self.warnings.borrow().clone()
    }

    /// 插入或覆盖对象（主要用于写入端构造与测试）。
    pub fn set_object(&self, obj: Object) {
        // 插入时顺带扫描特性，保持 FeatureProfile 与 cache 同步。
        scan_value_features(&mut self.features.borrow_mut(), &obj.value);
        self.cache.borrow_mut().insert(obj.num, obj);
    }

    /// 标记删除对象号（7.5.6 增量更新）：从缓存移除并记录对象号，
    /// 增量段为该号写出自由条目，使读端（按最新条目优先）看不到旧版本。
    pub fn delete_object(&self, num: i64) {
        self.cache.borrow_mut().remove(&num);
        self.deleted.borrow_mut().insert(num);
    }

    /// 已标记删除的对象号（升序）。
    pub fn deleted_objects(&self) -> Vec<i64> {
        let mut v: Vec<i64> = self.deleted.borrow().iter().copied().collect();
        v.sort_unstable();
        v
    }

    /// 返回缓存中全部对象，按 `num` 升序。供 writer 顺序写出。
    /// 仅在写入端（对象已预置在缓存）调用；对读端懒加载 Document
    /// 只返回当前已解析的部分。
    pub fn objects_for_write(&self) -> Vec<Object> {
        let cache = self.cache.borrow();
        let mut v: Vec<Object> = cache.values().cloned().collect();
        v.sort_by_key(|o| o.num);
        v
    }

    /// 取流的实际字节。
    ///
    /// - [`StreamData::Ref`]：从源文件按偏移 Seek+Read 读取
    ///   （按需，不常驻内存）；
    /// - [`StreamData::Owned`]：直接克隆。
    ///
    /// 非 `Value::Stream` 返回 `None`。读端 `Ref` 但无源文件
    /// 返回 `None`（写端构造的 Document 无源）。I/O 错误返回 `None`。
    pub fn stream_bytes(&self, value: &Value) -> Option<Vec<u8>> {
        self.try_stream_bytes(value).ok()
    }

    /// Read a stream's raw bytes and decrypt them when the document is encrypted.
    ///
    /// Unlike [`Document::stream_bytes`], this preserves the decryption error
    /// instead of returning `None`.
    pub fn try_stream_bytes(&self, value: &Value) -> Result<Vec<u8>, String> {
        match value {
            Value::Stream(dict, data) => match data {
                StreamData::Owned(b) => Ok(b.clone()),
                StreamData::Ref { offset, len } => self
                    .read_at(*offset, *len as usize)
                    .ok_or_else(|| "failed to read stream bytes".to_string()),
                StreamData::EncryptedRef {
                    offset,
                    len,
                    object_num,
                    generation,
                } => {
                    let bytes = self
                        .read_at(*offset, *len as usize)
                        .ok_or_else(|| "failed to read encrypted stream bytes".to_string())?;
                    self.security
                        .as_ref()
                        .ok_or_else(|| "missing PDF decryption context".to_string())?
                        .decrypt_stream(*object_num, *generation, dict, &bytes)
                }
            },
            _ => Err("value is not a PDF stream".to_string()),
        }
    }

    /// 从源文件按偏移读取至多 `len` 字节（短读安全：到达 EOF 即停止，
    /// 返回实际读到的字节，不因未读满而失败）。无源或 I/O 错误返回 `None`。
    pub(crate) fn read_at(&self, offset: u64, len: usize) -> Option<Vec<u8>> {
        let src = self.source.as_ref()?;
        let mut guard = src.lock().ok()?;
        guard.seek(SeekFrom::Start(offset)).ok()?;
        let mut buf = vec![0u8; len];
        let mut filled = 0usize;
        while filled < buf.len() {
            match guard.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(_) => return None,
            }
        }
        buf.truncate(filled);
        Some(buf)
    }

    /// 源文件总字节数；无源或元数据查询失败返回 `None`。
    pub(crate) fn source_len(&self) -> Option<u64> {
        let src = self.source.as_ref()?;
        let guard = src.lock().ok()?;
        guard.get_ref().metadata().ok().map(|meta| meta.len())
    }
}

impl PartialEq for Document {
    fn eq(&self, other: &Self) -> bool {
        // 缓存内容可能因懒加载不同步而不同，故只比较文件级结构。
        self.xref == other.xref
            && self.trailer == other.trailer
            && self.startxref == other.startxref
            && self.source_version == other.source_version
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::from_objects(Vec::new(), Xref::new(), Trailer::new(), StartXref(0))
    }
}
