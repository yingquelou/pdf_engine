//! reader 模块测试。

use super::*;
use std::io::Write;
use std::process::id;

mod basic;
mod document;
mod linearized;
mod support;

/// 生成唯一临时文件路径（无 tempfile 依赖）。
///
/// 沙箱策略按镜像路径拦截：target 目录内的二进制禁止写系统 %TEMP%。
/// 因此临时文件统一落在构建目录 target/test-tmp/ 下；无法定位 target
/// 时回退系统临时目录。
fn tmp_path(name: &str) -> std::path::PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!(
        "demo-pdf-test-{}-{}-{}",
        id(),
        name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ))
}

/// 把字节写入临时文件并返回其 File。
pub(super) fn write_tmp(bytes: &[u8], name: &str) -> (std::fs::File, std::path::PathBuf) {
    let path = tmp_path(name);
    let mut f = std::fs::File::create(&path).expect("create tmp");
    f.write_all(bytes).expect("write tmp");
    f.flush().expect("flush");
    // 重新打开只读
    let f = std::fs::OpenOptions::new()
        .read(true)
        .open(&path)
        .expect("reopen tmp");
    (f, path)
}

pub(super) struct SliceSource<'a> {
    data: &'a [u8],
    len: u64,
}
impl<'a> ObjectSource for SliceSource<'a> {
    fn read_at(&mut self, offset: u64, len: u64) -> Option<Vec<u8>> {
        let start = offset as usize;
        let end = (start + len as usize).min(self.data.len());
        if start > self.data.len() {
            return None;
        }
        Some(self.data[start..end].to_vec())
    }
    fn file_len(&self) -> u64 {
        self.len
    }
}

pub(super) fn slice_source(data: &[u8]) -> SliceSource<'_> {
    SliceSource {
        data,
        len: data.len() as u64,
    }
}
