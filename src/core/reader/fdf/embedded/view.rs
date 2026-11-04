//! `/EmbeddedFDFs` 类型化视图的**形态**部分（Table 243 续表）：
//! [`FdfDifferences`]、[`FdfEmbeddedFdfs`]、[`FdfEmbeddedFdf`] 三个只读
//! 视图及其 accessor。装配（解引用、递归展开、降级、防护）在
//! [`super::expand`]，对外入口是 [`Document::fdf_differences`] 与
//! [`Document::fdf_embedded_fdfs`]（定义在 `fdf/embedded.rs`）。
//!
//! 所有视图都遵循同一读端策略：**原值永不丢弃**（[`FdfDifferences::raw`]、
//! [`FdfEmbeddedFdfs::raw`]、[`FdfEmbeddedFdf::raw`]），类型化失败一律降级
//! 为 `issues` 诊断，绝不让整份 FDF 解析失败。

use crate::core::reader::embedded::{EmbeddedFile, Filespec};
use crate::core::types::{Dict, Document, Value};

use super::super::{FdfDocument, FdfError, malformed};

type Result<T> = std::result::Result<T, FdfError>;

/// `/Differences`（Table 243 续表 raw L24906–L24926；stream；PDF 1.4）
/// 的类型化只读视图：确证「它是流」并原值保留其流字典条目
/// （`/Length`、`/Filter`、`/DecodeParms` 等）。
///
/// 直接形态（`Value::Stream`）与间接引用形态（`5 0 R`）都接：引用经
/// `reader/navigation::resolve` 解引用到流对象；解引用失败（悬空引用、
/// 对象号与本 crate 单对象写端产生的 `/Type /XRef` 流冲突等）**不报错**，
/// 只记诊断并保留原值（[`Self::raw`]）。
///
/// 形态说明：流在文件语法里只能是间接对象（7.3.7），故**文件级**读取到的
/// `/Differences` 实际都是引用形态；直接 `Value::Stream` 出现在内存构造的
/// [`Document`]（`Document::from_objects`）里，两条路径由单元测试覆盖。
#[derive(Debug, Clone, PartialEq)]
pub struct FdfDifferences {
    /// `/Differences` 条目原值（引用形态时仍是 `Value::Ref`，一条不丢）。
    pub raw: Value,
    /// 原值为间接引用时的 `(对象号, generation)`；直接流形态为 `None`。
    pub reference: Option<(i64, i64)>,
    /// 解引用后确证的流对象值；形态不是流或解引用失败时为 `None`。
    pub stream: Option<Value>,
    /// 降级与诊断记录（非致命；读端宽容，任何一条都不阻断解析）。
    pub issues: Vec<String>,
}

impl FdfDifferences {
    /// 是否已确证为流（Table 243 声明的类型）。
    pub fn is_stream(&self) -> bool {
        self.stream.is_some()
    }

    /// 流字典（`/Length`、`/Filter` 等条目**原值保留**，本切片不裁剪）。
    pub fn dict(&self) -> Option<&Dict> {
        match &self.stream {
            Some(Value::Stream(dict, _)) => Some(dict),
            _ => None,
        }
    }

    /// 按键取流字典条目原值。
    pub fn entry(&self, key: &str) -> Option<&Value> {
        self.dict().and_then(|dict| dict.get(key))
    }

    /// 声明的 `/Length`（整型）；缺失或非整型为 `None`。
    pub fn declared_length(&self) -> Option<i64> {
        self.entry("Length").and_then(Value::as_integer)
    }

    /// 经 [`Document::decode_stream_bytes`] 解出的增量更新字节：过滤器链
    /// 与 PDF 侧（7.6）解密照常应用。Table 244 的 FDF 侧 RC4 **只作用于
    /// `/EmbeddedFDFs` 的嵌入式 FDF 文件整体**（见
    /// [`Document::fdf_embedded_fdfs_with_password`]），`/Differences` 流
    /// 字典不声明 `/EncryptionRevision`，故此处无需 FDF 侧解扰。未确证为
    /// 流或解码失败报 [`FdfError::Malformed`]。
    pub fn decoded_bytes(&self, document: &Document) -> Result<Vec<u8>> {
        let stream = self
            .stream
            .as_ref()
            .ok_or_else(|| malformed("FDF /Differences does not resolve to a stream"))?;
        document.decode_stream_bytes(stream).map_err(|error| {
            malformed(format!(
                "FDF /Differences stream cannot be decoded: {error}"
            ))
        })
    }
}

/// `/EmbeddedFDFs`（Table 243 续表 raw L24930–L24932；array of file
/// specifications；PDF 1.4）的类型化只读视图。
#[derive(Debug, Clone, PartialEq)]
pub struct FdfEmbeddedFdfs {
    /// 条目原值（不丢弃；非数组形态时也在此保留）。
    pub raw: Option<Value>,
    /// 原值为间接引用（数组写成 `7 0 R`）时的 `(对象号, generation)`。
    pub reference: Option<(i64, i64)>,
    /// 逐项类型化结果，按数组声明序；单次数组展开的条目数上限
    /// [`super::MAX_FDF_EMBEDDED_ENTRIES`]，超出部分跳过并记入
    /// [`Self::issues`]。
    /// 整棵递归树另受 [`super::MAX_FDF_EMBEDDED_TOTAL_ENTRIES`] 的全局条目
    /// 预算约束（展开按声明序深度优先，故预算可能在**任一层**的数组中途
    /// 截断，含顶层），截断处同样记一条诊断、原值保留在 [`Self::raw`]。
    pub entries: Vec<FdfEmbeddedFdf>,
    /// 数组级降级与诊断记录（非数组、解引用失败、条目超限、
    /// 声明版本低于 PDF 1.4 等）。
    pub issues: Vec<String>,
}

impl FdfEmbeddedFdfs {
    /// 已类型化的条目数。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否没有任何已类型化条目。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 按文件规范展示名（`/UF` 优先、回退 `/F`）取首个**已解出子文档**
    /// 的条目视图。
    pub fn document(&self, name: &str) -> Option<&FdfDocument> {
        self.entries
            .iter()
            .find(|entry| entry.name() == Some(name))
            .and_then(|entry| entry.document.as_deref())
    }
}

/// `/EmbeddedFDFs` 数组的一项：一个 7.11 文件规范，其 `/EF` 载体流即
/// 「嵌入式 FDF 文件」（7.11.4）。
#[derive(Debug, Clone, PartialEq)]
pub struct FdfEmbeddedFdf {
    /// 数组项原值（畸形项整体保留，不丢弃）。
    pub raw: Value,
    /// 7.11 文件规范类型化视图（复用 `reader/embedded.rs`）；无法解析为
    /// 文件规范时为 `None`，原值仍在 [`Self::raw`]。
    pub filespec: Option<Filespec>,
    /// Table 244 — 载体流的 `/EncryptionRevision`（存在**且为整数**时
    /// `Some`；非整数形态记诊断，原值留在载体流字典里）。
    pub encryption_revision: Option<i64>,
    /// 本条目的内层字节是否**经 12.7.7.3 解密**得到：仅当
    /// `/EncryptionRevision == 1` 且调用方走
    /// [`Document::fdf_embedded_fdfs_with_password`] 给出口令时为 `true`
    /// （解扰本身失败时为 `false`）。未加密条目恒 `false`——它没有
    /// 需要解密的步骤，[`Self::data`] 即明文。
    /// 密钥字节永不进入本视图（见 `embedded/crypto.rs` 的安全说明）。
    pub decrypted: bool,
    /// 递归解析出的子 FDF 文档视图（Table 242 目录 + Table 243 字典）。
    /// `None`：无 `/EF` 载体、内容未经/无法 FDF 解析、加密且未经解密（未给
    /// 口令、`/EncryptionRevision` ≠ 1、或解扰失败）、超限或命中环路——
    /// 具体原因见 [`Self::issues`]。
    pub document: Option<Box<FdfDocument>>,
    /// 子文档自己 `/Differences` 的类型化视图（就地装配：子
    /// [`Document`] 不落进视图，[`FdfDocument`] 里保留的
    /// `differences` 原值只是一个调用方无法解引用的对象号）。子文档未
    /// 声明该条目时为 `None`；[`FdfDifferences::decoded_bytes`] 以**外层**
    /// 文档为上下文解释子流（内层字节已随载体流解码内联，解密后的明文
    /// 同样如此；PDF 侧 `/Crypt` 按外层加密状态裁剪）。
    pub differences: Option<Box<FdfDifferences>>,
    /// 子文档自己的 `/EmbeddedFDFs` 类型化结果（递归一层记一层，受
    /// [`super::MAX_FDF_EMBEDDED_DEPTH`]、环检测与
    /// [`super::MAX_FDF_EMBEDDED_TOTAL_ENTRIES`] 全局预算三重约束）。
    pub nested: Option<Box<FdfEmbeddedFdfs>>,
    /// 本条目的降级与诊断记录（非致命）。
    pub issues: Vec<String>,
}

impl FdfEmbeddedFdf {
    /// `/EF` 载体流（嵌入式文件流）；引用型/外部文件规范为 `None`。
    pub fn carrier(&self) -> Option<&EmbeddedFile> {
        self.filespec.as_ref().and_then(Filespec::primary)
    }

    /// 文件规范展示名（`/UF` 优先回退 `/F`）；两者皆缺时 `None`。
    pub fn name(&self) -> Option<&str> {
        self.filespec
            .as_ref()
            .map(Filespec::name)
            .filter(|name| !name.is_empty())
    }

    /// 载体流的（已按 PDF 侧过滤器与解密处理的）**文件字节原值**：
    /// FDF 侧 RC4 **不在此应用**，故加密条目的返回值仍是密文（读端策略是
    /// 原值永不丢弃）。12.7.7.3 解扰出的明文只在装配期存在（由
    /// `Zeroizing` 承载、随后归零），其类型化结果见 [`Self::document`] /
    /// [`Self::differences`] / [`Self::nested`]，是否解扰过见
    /// [`Self::is_decrypted`]。
    pub fn data(&self) -> Option<&[u8]> {
        self.carrier().map(|file| file.data.as_slice())
    }

    /// Table 244 — 是否声明了 `/EncryptionRevision`（存在即表示该嵌入式
    /// FDF 流按 FDF 机制加密）。
    pub fn is_encrypted(&self) -> bool {
        self.encryption_revision.is_some()
    }

    /// 本条目的内层字节是否由 **12.7.7.3 的 FDF 侧解密**得到（即
    /// [`Self::document`] 等类型化结果来自解密后的明文，而非文件里的
    /// 原字节）。未加密条目为 `false`（无需解密）。
    pub fn is_decrypted(&self) -> bool {
        self.decrypted
    }

    /// `/EncryptionRevision` 是否落在「publication 时唯一被定义的值 1」
    /// （Table 244 raw L24937–L24939）；未加密时也是 `true`。
    pub fn encryption_revision_supported(&self) -> bool {
        self.encryption_revision.is_none() || self.encryption_revision == Some(1)
    }
}
