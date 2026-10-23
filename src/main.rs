//! demo —— PDF 解析/生成示例程序。
//!
//! 用法：
//! ```text
//! demo parse <file.pdf>                解析并打印概要（含版本与回退警告）
//! demo gen   <out.pdf> [--version X]   生成最小 PDF（默认 1.7）
//! ```
//!
//! 不传参数时打印帮助。

use pdf_engine::core;
use std::env;
use std::fs;
use std::process::ExitCode;

fn cmd_parse(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let file = fs::File::open(path)?;
    let mut parser = core::reader::Parser::new(file)?;
    let doc = parser.parse_document()?;
    // 懒加载：此处只解析了文件级结构，未解析任何对象。
    println!("declared version: {}", doc.source_version());
    if !doc.warnings().is_empty() {
        println!("parse warnings:");
        for w in doc.warnings() {
            println!("  - {w}");
        }
    }
    let xref = doc.xref();
    println!("xref sections: {}", xref.0.len());
    println!("xref entries (max object no + 1): {}", xref.max_size());
    let in_use = xref
        .0
        .iter()
        .flat_map(|s| &s.entries)
        .filter(|e| e.in_use())
        .count();
    println!("xref in-use entries: {in_use}");
    let cached_before = doc.cached_object_count();
    println!("objects parsed before root resolution: {cached_before}");
    if let Some(root) = doc.trailer().root() {
        println!("trailer /Root: {} {} R", root.0, root.1);
        // 触发懒加载：解析 Root 对象（仅此一个，不递归整本）
        match doc.get_object(root.0) {
            Ok(Some(o)) => {
                println!(
                    "root object resolved: {} {} obj (value: {})",
                    o.num,
                    o.generation,
                    o.value.type_name()
                );
            }
            Ok(None) => println!("root object not in xref"),
            Err(e) => eprintln!("root object parse error: {e}"),
        }
        println!(
            "objects parsed after root resolution: {}",
            doc.cached_object_count()
        );
    }
    if let Some(size) = doc.trailer().size() {
        println!("trailer /Size: {size}");
    }
    println!("startxref: {}", doc.startxref().0);
    Ok(())
}

fn cmd_gen(path: &str, version: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    use core::builder::{Font, PageSize, PdfBuilder};
    use core::types::PdfVersion;

    // 版本解析复用 `types` 的集中实现（唯一事实源），CLI 不再维护一份
    // 版本表；只接受规范写法（如 "1.5"）。2.0 虽可解析，但会被 writer
    // 的输出区间校验（SUPPORTED_OUTPUT_VERSIONS）拒绝。
    let v = match version {
        Some(s) => PdfVersion::from_name(s)
            .filter(|v| v.as_str() == s)
            .ok_or_else(|| format!("unsupported version '{s}'"))?,
        None => PdfVersion::default(),
    };

    // 逻辑层：声明「一页 A4 + 一段文字」，由 PdfBuilder 翻译为
    // Catalog/Pages/Page/字体/内容流等底层对象，无需手工拼 Dict。
    let mut builder = PdfBuilder::with_version(v);
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(24.0), 72.0, 720.0, "Hello, PDF!");

    let mut out = Vec::new();
    builder.build(&mut out)?;
    fs::write(path, out)?;
    println!("wrote {path}");
    Ok(())
}

fn print_help() {
    eprintln!(
        "demo —— PDF 解析/生成示例\n\n\
         用法:\n  \
         demo parse <file.pdf>              解析并打印概要\n  \
         demo gen   <out.pdf> [--version X] 生成最小 PDF（默认 1.7）\n"
    );
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_help();
        return ExitCode::from(2);
    }
    let res = match args[1].as_str() {
        "parse" if args.len() >= 3 => cmd_parse(&args[2]).map_err(|e| e.to_string()),
        "gen" if args.len() >= 3 => {
            // 解析可选 --version X
            let mut version = None;
            let mut i = 3;
            while i < args.len() {
                if args[i] == "--version" && i + 1 < args.len() {
                    version = Some(args[i + 1].as_str());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            cmd_gen(&args[2], version).map_err(|e| e.to_string())
        }
        _ => {
            print_help();
            return ExitCode::from(2);
        }
    };
    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}
