//! 独立测试：验证压缩核心在真实图片上的行为
//!
//! 用法: cargo run --bin compress_test -- <input.zip> <output.zip>

use std::path::PathBuf;

use image_compressor_lib::test_harness::{process_zip_for_test, CompressOptions};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/test_input.zip"));
    let output = args
        .get(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/test_output.zip"));

    let opts = CompressOptions {
        quality: 75,
        lossless: false,
        gif_fps: Some(10),
        gif_scale: 100,
        gif_colors: 200,
    };

    println!("输入: {}", input.display());
    println!("输出: {}", output.display());
    println!("配置: {opts:?}\n");

    match process_zip_for_test(&input, &output, &opts) {
        Ok(s) => {
            println!("=== 压缩完成 ===");
            println!("总图片数:     {}", s.total_images);
            println!("成功压缩:     {}", s.compressed_images);
            println!("跳过非图片:   {}", s.skipped_files);
            println!("原图总大小:   {} bytes", s.original_total);
            println!("压缩后总大小: {} bytes", s.compressed_total);
            println!("输出 zip:     {} bytes", s.zip_size);
            let pct = (s.original_total as f64 - s.compressed_total as f64)
                / s.original_total as f64
                * 100.0;
            println!("节省:         {pct:.1}%");
            println!("\n--- 逐文件明细 ---");
            for d in &s.details {
                let p = (d.original_size as f64 - d.compressed_size as f64)
                    / d.original_size as f64
                    * 100.0;
                println!(
                    "{:<28} {:>8} -> {:>8}  {}{}",
                    d.relative_path,
                    d.original_size,
                    d.compressed_size,
                    if d.compressed {
                        format!("↓{p:.0}%")
                    } else {
                        "原样".to_string()
                    },
                    d.error
                        .as_ref()
                        .map(|e| format!("  ERR: {e}"))
                        .unwrap_or_default()
                );
            }
        }
        Err(e) => {
            eprintln!("失败: {e:#}");
            std::process::exit(1);
        }
    }
}
