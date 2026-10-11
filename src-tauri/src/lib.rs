//! 图片压缩工具 - Tauri 后端
//!
//! 所有处理均在本机完成，不发起任何网络请求。

pub mod archive;
pub mod compress;

/// 供 `compress_test` 二进制复用的测试入口（不参与 Tauri 命令）
pub mod test_harness {
    pub use crate::archive::ArchiveSummary;
    pub use crate::compress::CompressOptions;

    /// 直接调用 zip 处理流程，无 Tauri 依赖
    pub fn process_zip_for_test(
        input: &std::path::Path,
        output: &std::path::Path,
        opts: &CompressOptions,
    ) -> anyhow::Result<ArchiveSummary> {
        crate::archive::process_zip(input, output, opts, |_c, _t, _n| {})
    }
}

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{Emitter, Window};

use archive::{process_folder, process_zip, ArchiveSummary};
use compress::CompressOptions;

/// 统一的命令返回
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CmdResult<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T> CmdResult<T> {
    fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }
    fn err(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

/// 输入源信息
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputInfo {
    pub path: String,
    pub name: String,
    pub kind: String, // "zip" | "folder" | "image"
    pub size: u64,
    /// 若为 zip，列出内部图片数量与总大小
    pub image_count: usize,
    pub image_total_size: u64,
}

/// 探测拖拽/选择进来的路径类型
#[tauri::command]
fn inspect_input(path: String) -> CmdResult<InputInfo> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return CmdResult::err("路径不存在");
    }

    if p.is_dir() {
        // 目录：递归统计图片
        let mut count = 0usize;
        let mut total = 0u64;
        for entry in walkdir::WalkDir::new(&p).follow_links(false) {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if entry.file_type().is_dir() {
                continue;
            }
            if compress::is_supported_image(entry.path()) {
                count += 1;
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
        let name = p
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "folder".into());
        return CmdResult::ok(InputInfo {
            path,
            name,
            kind: "folder".into(),
            size: total,
            image_count: count,
            image_total_size: total,
        });
    }

    let meta_size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
    let name = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "input".into());

    if archive::sniff_is_zip(&p) {
        match archive::list_zip_images(&p) {
            Ok(images) => {
                let total: u64 = images.iter().map(|(_, s)| *s).sum();
                CmdResult::ok(InputInfo {
                    path,
                    name,
                    kind: "zip".into(),
                    size: meta_size,
                    image_count: images.len(),
                    image_total_size: total,
                })
            }
            Err(e) => CmdResult::err(format!("读取压缩包失败: {e}")),
        }
    } else if compress::is_supported_image(&p) {
        CmdResult::ok(InputInfo {
            path,
            name,
            kind: "image".into(),
            size: meta_size,
            image_count: 1,
            image_total_size: meta_size,
        })
    } else {
        CmdResult::err("不支持的文件类型，请拖入 zip 压缩包或图片文件夹")
    }
}

/// 开始压缩任务
#[tauri::command]
async fn start_compress(
    window: Window,
    input_path: String,
    output_path: String,
    options: CompressOptions,
) -> CmdResult<ArchiveSummary> {
    let input = PathBuf::from(&input_path);
    let output = PathBuf::from(&output_path);

    if !input.exists() {
        return CmdResult::err("输入路径不存在");
    }

    if let Some(parent) = output.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return CmdResult::err(format!("无法创建输出目录: {e}"));
        }
    }

    let win = window.clone();
    let progress_cb = move |done: usize, total: usize, name: &str| {
        let payload = serde_json::json!({
            "current": done,
            "total": total,
            "name": name,
        });
        let _ = win.emit("compress-progress", payload);
    };

    let result = if input.is_dir() {
        process_folder(&input, &output, &options, progress_cb)
    } else if archive::sniff_is_zip(&input) {
        process_zip(&input, &output, &options, progress_cb)
    } else if compress::is_supported_image(&input) {
        // 单张图片：包一层 zip
        process_single_image(&input, &output, &options)
    } else {
        Err(anyhow::anyhow!("不支持的文件类型"))
    };

    match result {
        Ok(summary) => {
            let _ = window.emit("compress-done", &summary);
            CmdResult::ok(summary)
        }
        Err(e) => {
            let msg = format!("{e:#}");
            let _ = window.emit("compress-error", msg.clone());
            CmdResult::err(msg)
        }
    }
}

/// 单张图片：压缩后包装成 zip
fn process_single_image(
    input: &Path,
    output: &Path,
    opts: &CompressOptions,
) -> anyhow::Result<ArchiveSummary> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let data = std::fs::read(input)?;
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_string();
    let (bytes, compressed) = compress::compress_or_original(&data, &ext, opts);

    let file = std::fs::File::create(output)?;
    let mut zw = zip::ZipWriter::new(file);
    let name = input
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".into());
    zw.start_file(name.clone(), SimpleFileOptions::default())?;
    zw.write_all(&bytes)?;
    zw.finish()?;

    let zip_size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    let detail = compress::CompressResult {
        relative_path: name,
        original_size: data.len() as u64,
        compressed_size: bytes.len() as u64,
        compressed,
        error: None,
    };

    Ok(ArchiveSummary {
        output_path: output.to_string_lossy().to_string(),
        output_name: output
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "compressed.zip".into()),
        total_images: 1,
        compressed_images: if compressed { 1 } else { 0 },
        skipped_files: 0,
        failed_images: 0,
        original_total: data.len() as u64,
        compressed_total: bytes.len() as u64,
        zip_size,
        details: vec![detail],
    })
}

/// 用系统默认程序打开输出文件所在目录
#[tauri::command]
fn reveal_in_finder(path: String) -> CmdResult<()> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return CmdResult::err("文件不存在");
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("-R")
            .arg(&p)
            .spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .arg(format!("/select,{}", p.display()))
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let dir = p.parent().unwrap_or(Path::new("."));
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
    CmdResult::ok(())
}

/// 生成默认输出文件名：原名 + -compressed.zip，放在同目录
#[tauri::command]
fn default_output_path(input_path: String) -> CmdResult<String> {
    let p = PathBuf::from(&input_path);
    let parent = p.parent().unwrap_or(Path::new("."));
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".into());
    let out = parent.join(format!("{stem}-compressed.zip"));
    // 若已存在，加序号
    let mut final_path = out.clone();
    let mut i = 1;
    while final_path.exists() {
        final_path = parent.join(format!("{stem}-compressed-{i}.zip"));
        i += 1;
    }
    CmdResult::ok(final_path.to_string_lossy().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            inspect_input,
            start_compress,
            reveal_in_finder,
            default_output_path,
        ])
        .setup(|_app| {
            // 窗口尺寸/标题等已在 tauri.conf.json 中声明。
            // 此处保留 setup 钩子，便于后续扩展（托盘、菜单等）。
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}
