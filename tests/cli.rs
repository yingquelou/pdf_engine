//! CLI（src/main.rs）集成测试：经 `CARGO_BIN_EXE_demo` 调用二进制，
//! 锁定参数解析、错误路径与退出码契约。

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

/// 在临时目录下生成唯一路径（含 pid + 纳秒 + 序号，避免并行冲突）。
///
/// 沙箱策略按镜像路径拦截：target 目录内的二进制禁止写系统 %TEMP%，
/// 临时路径统一落在构建目录 target/test-tmp/ 下；无法定位 target 时
/// 回退系统临时目录。
fn unique_path(tag: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!(
        "demo-cli-{}-{}-{}-{tag}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos(),
        seq
    ))
}

fn demo_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_demo"))
}

/// `gen --version 9.9`：非法版本应失败，stderr 说明原因，且不产出文件。
#[test]
fn gen_with_invalid_version_fails() {
    let out = unique_path("bad");
    let res = demo_bin()
        .args(["gen", out.to_str().expect("utf-8 path"), "--version", "9.9"])
        .output()
        .expect("spawn demo");
    assert!(!res.status.success(), "非法版本应使 gen 失败");
    let stderr = String::from_utf8_lossy(&res.stderr);
    assert!(
        stderr.contains("unsupported version"),
        "stderr 应说明 unsupported version：{stderr}"
    );
    assert!(!out.exists(), "gen 失败时不应产出文件");
}

/// `gen` 默认 1.7 → 产出文件 → `parse` 该文件成功且报出 1.7。
#[test]
fn gen_then_parse_roundtrip_via_cli() {
    let out = unique_path("gen");
    let res = demo_bin()
        .args(["gen", out.to_str().expect("utf-8 path")])
        .output()
        .expect("spawn demo");
    assert!(
        res.status.success(),
        "gen 应成功：{}",
        String::from_utf8_lossy(&res.stderr)
    );
    assert!(out.exists(), "gen 应产出文件");

    let res = demo_bin()
        .args(["parse", out.to_str().expect("utf-8 path")])
        .output()
        .expect("spawn demo");
    assert!(res.status.success(), "parse 生成的文件应成功");
    let stdout = String::from_utf8_lossy(&res.stdout);
    assert!(
        stdout.contains("declared version: 1.7"),
        "parse 应报出 writer 默认 1.7：{stdout}"
    );
    let _ = std::fs::remove_file(&out);
}

/// `parse` 不存在的文件：应失败并输出 error。
#[test]
fn parse_missing_file_fails() {
    let missing = unique_path("missing");
    let res = demo_bin()
        .args(["parse", missing.to_str().expect("utf-8 path")])
        .output()
        .expect("spawn demo");
    assert!(!res.status.success(), "文件不存在应使 parse 失败");
    let stderr = String::from_utf8_lossy(&res.stderr);
    assert!(stderr.contains("error"), "stderr 应含 error：{stderr}");
}
