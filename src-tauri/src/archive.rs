//! 压缩包处理：解析 zip、递归读取子目录图片、压缩后重新打包

use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rayon::prelude::*;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::compress::{
    compress_or_original, is_supported_image, CompressOptions, CompressResult, SUPPORTED_EXTS,
};

/// 压缩任务汇总
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveSummary {
    /// 输出 zip 的绝对路径
    pub output_path: String,
    pub output_name: String,
    /// 总图片数（受支持格式）
    pub total_images: usize,
    /// 成功压缩的图片数
    pub compressed_images: usize,
    /// 跳过（非图片或不支持）的文件数
    pub skipped_files: usize,
    /// 出错的文件数
    pub failed_images: usize,
    pub original_total: u64,
    pub compressed_total: u64,
    /// 输出 zip 文件的实际大小
    pub zip_size: u64,
    /// 逐文件明细
    pub details: Vec<CompressResult>,
}

/// 从 zip 文件解压并压缩所有图片，最后打包为新的 zip。
///
/// `progress` 回调用于向前端推送进度 (已处理数, 总数, 当前文件名)。
pub fn process_zip<F>(
    input_zip: &Path,
    output_zip: &Path,
    opts: &CompressOptions,
    progress: F,
) -> Result<ArchiveSummary>
where
    F: Fn(usize, usize, &str) + Sync + Send,
{
    let file = File::open(input_zip)
        .with_context(|| format!("无法打开压缩包: {}", input_zip.display()))?;
    let mut archive = ZipArchive::new(file).context("压缩包格式无效或已损坏")?;

    // ---- 第 1 步：扫描出所有条目（递归，天然包含子目录）----
    struct Entry {
        /// 清理后的相对路径
        relative: PathBuf,
        is_dir: bool,
        is_image: bool,
        /// 系统生成的元数据（__MACOSX / ._xxx / .DS_Store），既不算图片也不算用户文件
        is_system_meta: bool,
    }

    let mut entries: Vec<Entry> = Vec::new();
    for i in 0..archive.len() {
        let (is_dir, relative) = {
            let f = archive.by_index(i)?;
            (f.is_dir(), normalize_zip_path(f.name()))
        };
        // 过滤 macOS 的 __MACOSX 与隐藏文件
        let is_macos_meta = relative
            .components()
            .any(|c| c.as_os_str() == "__MACOSX" || c.as_os_str().to_string_lossy().starts_with("._"))
            || relative
                .file_name()
                .map(|n| n == ".DS_Store")
                .unwrap_or(false);

        entries.push(Entry {
            relative: relative.clone(),
            is_dir,
            is_image: !is_dir && !is_macos_meta && is_supported_image(&relative),
            is_system_meta: is_macos_meta,
        });
    }

    let image_entries: Vec<&Entry> = entries.iter().filter(|e| e.is_image).collect();
    let total_images = image_entries.len();
    // 只统计"用户文件"里被跳过的（不含系统元数据，避免误导用户）
    let skipped_files = entries
        .iter()
        .filter(|e| !e.is_dir && !e.is_image && !e.is_system_meta)
        .count();

    if total_images == 0 {
        anyhow::bail!("压缩包内没有找到支持的图片文件（{}）", SUPPORTED_EXTS.join(", "));
    }

    // ---- 第 2 步：并行读取 + 压缩 ----
    // 逐个读取 zip 条目有 IO 争用，这里先串行读出字节（内存友好：读一个处理一个太慢，
    // ---- 第 2 步：读取 + 压缩 ----
    // 先收集需要处理的索引与路径（此时不持有 ZipFile，避免借用冲突）
    let mut targets: Vec<(usize, String, PathBuf, u64)> = Vec::with_capacity(total_images);
    for i in 0..archive.len() {
        let (name, relative, is_dir, size) = {
            let f = archive.by_index(i)?;
            (
                f.name().to_string(),
                normalize_zip_path(f.name()),
                f.is_dir(),
                f.size(),
            )
        };
        if is_dir || !is_supported_image(&relative) {
            continue;
        }
        let is_macos_meta = relative
            .components()
            .any(|c| c.as_os_str() == "__MACOSX" || c.as_os_str().to_string_lossy().starts_with("._"));
        if is_macos_meta {
            continue;
        }
        targets.push((i, name, relative, size));
    }

    // 逐个读取内容（借用作用域受限，安全）
    let mut tasks: Vec<(String, PathBuf, Vec<u8>, u64)> = Vec::with_capacity(targets.len());
    for (i, name, relative, size) in targets {
        let mut buf = Vec::with_capacity(size as usize);
        {
            let mut f = archive.by_index(i)?;
            f.read_to_end(&mut buf)
                .with_context(|| format!("读取 {name} 失败"))?;
        }
        tasks.push((name, relative, buf, size));
    }

    // 并行压缩
    let done = std::sync::atomic::AtomicUsize::new(0);
    let results: Vec<(String, PathBuf, Vec<u8>, CompressResult)> = tasks
        .par_iter()
        .map(|(name, relative, buf, _size)| {
            let ext = relative
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string();
            let orig_size = buf.len() as u64;

            let (out_bytes, compressed) = compress_or_original(buf, &ext, opts);

            let rel_str = relative.to_string_lossy().replace('\\', "/");
            let result = CompressResult {
                relative_path: rel_str,
                original_size: orig_size,
                compressed_size: out_bytes.len() as u64,
                compressed,
                error: None,
            };

            let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            progress(n, total_images, name);

            (name.clone(), relative.clone(), out_bytes, result)
        })
        .collect();

    // ---- 第 3 步：写入新 zip，保持原有目录结构 ----
    let out_file = File::create(output_zip)
        .with_context(|| format!("无法创建输出文件: {}", output_zip.display()))?;
    let mut zip_writer = ZipWriter::new(out_file);

    // 保留目录条目（保持空目录结构）
    let mut written_dirs: std::collections::HashSet<String> = std::collections::HashSet::new();
    for e in &entries {
        if e.is_dir {
            let dir_name = ensure_trailing_slash(&e.relative.to_string_lossy());
            if written_dirs.insert(dir_name.clone()) {
                let _ = zip_writer.add_directory(dir_name, SimpleFileOptions::default());
            }
        }
    }

    let mut details: Vec<CompressResult> = Vec::with_capacity(results.len());
    let mut compressed_images = 0usize;
    let mut original_total = 0u64;
    let mut compressed_total = 0u64;

    let file_opts = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    for (_name, relative, bytes, result) in results {
        let rel = relative.to_string_lossy().replace('\\', "/");
        // 确保父目录条目存在
        if let Some(parent) = relative.parent() {
            let p = parent.to_string_lossy().replace('\\', "/");
            if !p.is_empty() {
                let mut acc = String::new();
                for seg in p.split('/') {
                    if seg.is_empty() {
                        continue;
                    }
                    acc.push_str(seg);
                    acc.push('/');
                    if written_dirs.insert(acc.clone()) {
                        let _ = zip_writer.add_directory(acc.clone(), SimpleFileOptions::default());
                    }
                }
            }
        }

        zip_writer
            .start_file(rel.clone(), file_opts)
            .with_context(|| format!("写入 {rel} 失败"))?;
        zip_writer.write_all(&bytes)?;

        if result.compressed {
            compressed_images += 1;
        }
        original_total += result.original_size;
        compressed_total += result.compressed_size;
        details.push(result);
    }

    zip_writer.finish().context("完成 zip 写入失败")?;

    let zip_size = std::fs::metadata(output_zip).map(|m| m.len()).unwrap_or(0);

    Ok(ArchiveSummary {
        output_path: output_zip.to_string_lossy().to_string(),
        output_name: output_zip
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "compressed.zip".into()),
        total_images,
        compressed_images,
        skipped_files,
        failed_images: 0,
        original_total,
        compressed_total,
        zip_size,
        details,
    })
}

/// 处理一个本地目录：遍历（递归）所有图片，压缩后打包为 zip
pub fn process_folder<F>(
    input_dir: &Path,
    output_zip: &Path,
    opts: &CompressOptions,
    progress: F,
) -> Result<ArchiveSummary>
where
    F: Fn(usize, usize, &str) + Sync + Send,
{
    let mut files: Vec<PathBuf> = Vec::new();
    let mut skipped = 0usize;

    for entry in WalkDir::new(input_dir).follow_links(false) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if entry.file_type().is_dir() {
            continue;
        }
        let path = entry.path();
        if is_supported_image(path) {
            files.push(path.to_path_buf());
        } else {
            skipped += 1;
        }
    }

    let total = files.len();
    if total == 0 {
        anyhow::bail!("目录内没有找到支持的图片文件");
    }

    let done = std::sync::atomic::AtomicUsize::new(0);
    let results: Vec<(PathBuf, Vec<u8>, CompressResult)> = files
        .par_iter()
        .map(|path| {
            let data = std::fs::read(path).unwrap_or_default();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string();
            let (out_bytes, compressed) = compress_or_original(&data, &ext, opts);

            let rel = path
                .strip_prefix(input_dir)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");

            let result = CompressResult {
                relative_path: rel,
                original_size: data.len() as u64,
                compressed_size: out_bytes.len() as u64,
                compressed,
                error: None,
            };

            let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            progress(n, total, &path.to_string_lossy());

            (path.to_path_buf(), out_bytes, result)
        })
        .collect();

    let out_file = File::create(output_zip).context("无法创建输出文件")?;
    let mut zip_writer = ZipWriter::new(out_file);
    let file_opts = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let mut details = Vec::with_capacity(results.len());
    let mut compressed_images = 0usize;
    let mut original_total = 0u64;
    let mut compressed_total = 0u64;
    let mut written_dirs: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (_p, bytes, result) in results {
        let rel = result.relative_path.clone();
        if let Some(parent) = Path::new(&rel).parent() {
            let p = parent.to_string_lossy().replace('\\', "/");
            if !p.is_empty() {
                let mut acc = String::new();
                for seg in p.split('/') {
                    if seg.is_empty() {
                        continue;
                    }
                    acc.push_str(seg);
                    acc.push('/');
                    if written_dirs.insert(acc.clone()) {
                        let _ = zip_writer.add_directory(acc.clone(), SimpleFileOptions::default());
                    }
                }
            }
        }
        zip_writer.start_file(rel, file_opts)?;
        zip_writer.write_all(&bytes)?;

        if result.compressed {
            compressed_images += 1;
        }
        original_total += result.original_size;
        compressed_total += result.compressed_size;
        details.push(result);
    }
    zip_writer.finish()?;

    let zip_size = std::fs::metadata(output_zip).map(|m| m.len()).unwrap_or(0);

    Ok(ArchiveSummary {
        output_path: output_zip.to_string_lossy().to_string(),
        output_name: output_zip
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "compressed.zip".into()),
        total_images: total,
        compressed_images,
        skipped_files: skipped,
        failed_images: 0,
        original_total,
        compressed_total,
        zip_size,
        details,
    })
}

/// 规范化 zip 内部路径：去掉 "./"、开头的 "/"，统一为 `/`
fn normalize_zip_path(name: &str) -> PathBuf {
    let cleaned = name.replace('\\', "/");
    let cleaned = cleaned.trim_start_matches("./");
    let cleaned = cleaned.trim_start_matches('/');
    PathBuf::from(cleaned)
}

fn ensure_trailing_slash(s: &str) -> String {
    if s.ends_with('/') {
        s.to_string()
    } else {
        format!("{s}/")
    }
}

/// 读取 zip 内所有图片的清单（不解压内容），用于预览
pub fn list_zip_images(input_zip: &Path) -> Result<Vec<(String, u64)>> {
    let file = File::open(input_zip)?;
    let mut archive = ZipArchive::new(file)?;
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let f = archive.by_index(i)?;
        if f.is_dir() {
            continue;
        }
        let rel = normalize_zip_path(f.name());
        let is_meta = rel
            .components()
            .any(|c| c.as_os_str() == "__MACOSX" || c.as_os_str().to_string_lossy().starts_with("._"));
        if is_meta || !is_supported_image(&rel) {
            continue;
        }
        out.push((rel.to_string_lossy().replace('\\', "/"), f.size()));
    }
    Ok(out)
}

/// 判断给定路径是否是 zip
pub fn is_zip(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
}

/// 读取文件头部魔数，判断是否为 zip（拖拽时可能没扩展名）
pub fn sniff_is_zip(path: &Path) -> bool {
    if is_zip(path) {
        return true;
    }
    let mut f = match File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut magic = [0u8; 4];
    if f.seek(std::io::SeekFrom::Start(0)).is_err() {
        return false;
    }
    if f.read_exact(&mut magic).is_err() {
        return false;
    }
    magic == [0x50, 0x4B, 0x03, 0x04] || magic == [0x50, 0x4B, 0x05, 0x06]
}
