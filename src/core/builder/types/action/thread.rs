//! Thread 动作写端（12.6.4.6 Table 205）。
//!
//! 写端以索引形式给出目标：`/D` 为线程在目录 `/Threads` 数组中的 0 基
//! 索引（即 [`PdfBuilder::add_article_thread`] 的注册顺序），`/B` 为
//! bead 在线程内的 0 基索引。引用形式需要在动作字典写出前分配线程与
//! bead 的对象号，而线程对象在目录后处理阶段才分配，故不建模。

use crate::core::types::{Dict, Value};
use crate::core::writer::WriteError;

/// Thread 动作（12.6.4.6 Table 205）：跳转到文章线上的指定 bead。
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadAction {
    thread: usize,
    bead: Option<usize>,
}

impl ThreadAction {
    /// 以线程索引（`add_article_thread` 的注册顺序）构造。
    pub fn new(thread: usize) -> Self {
        Self { thread, bead: None }
    }

    /// 指定线程内的 bead 索引；缺省时查看器从线程首 bead 开始。
    pub fn with_bead(mut self, bead: usize) -> Self {
        self.bead = Some(bead);
        self
    }

    /// 目标线程索引。
    pub fn thread(&self) -> usize {
        self.thread
    }

    /// 目标 bead 索引。
    pub fn bead(&self) -> Option<usize> {
        self.bead
    }

    /// 校验索引对已注册文章线可解析；`bead_counts` 按线程注册顺序给出
    /// 各自的 bead 数。
    pub(crate) fn validate(&self, bead_counts: &[usize]) -> Result<(), WriteError> {
        let Some(count) = bead_counts.get(self.thread) else {
            return Err(WriteError::InvalidObject(format!(
                "Thread action /D references article thread index {} but only {} threads are registered",
                self.thread,
                bead_counts.len()
            )));
        };
        if let Some(bead) = self.bead
            && bead >= *count
        {
            return Err(WriteError::InvalidObject(format!(
                "Thread action /B bead index {bead} is out of range (thread {} has {count} beads)",
                self.thread
            )));
        }
        Ok(())
    }

    /// 动作专有条目（`/S` 由外层统一插入）。
    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new().insert("D", Value::Integer(self.thread as i64));
        if let Some(bead) = self.bead {
            dict = dict.insert("B", Value::Integer(bead as i64));
        }
        dict
    }
}
