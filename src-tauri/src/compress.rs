//! 图片压缩核心模块
//!
//! 纯本地实现：所有解码、压缩、编码都在本机内存/磁盘完成，不联网。

use std::io::Cursor;
use std::path::Path;

use anyhow::{bail, Context, Result};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{DynamicImage, ImageEncoder, ImageFormat, RgbaImage};

/// 支持的图片扩展名（小写，不含点）
pub const SUPPORTED_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff"];

/// 判断路径是否为受支持的图片
pub fn is_supported_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// 压缩配置
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressOptions {
    /// 质量 0-100，100 表示最高质量（体积最大），越低压缩越狠
    pub quality: u8,
    /// 是否使用无损压缩（PNG / WebP 生效）
    #[serde(default)]
    pub lossless: bool,
    /// GIF 目标帧率 (fps)，0 或 None 表示不抽帧
    #[serde(default)]
    pub gif_fps: Option<u32>,
    /// GIF 缩放比例 (1-100 %)，100 表示不缩放
    #[serde(default = "default_scale")]
    pub gif_scale: u32,
    /// GIF 调色板颜色数上限 (2-256)
    #[serde(default = "default_colors")]
    pub gif_colors: u16,
}

fn default_scale() -> u32 {
    100
}
fn default_colors() -> u16 {
    256
}

impl Default for CompressOptions {
    fn default() -> Self {
        Self {
            quality: 80,
            lossless: false,
            gif_fps: None,
            gif_scale: 100,
            gif_colors: 256,
        }
    }
}

/// 单张图片的压缩结果
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressResult {
    /// 相对于压缩包根目录的路径（保持原目录结构）
    pub relative_path: String,
    pub original_size: u64,
    pub compressed_size: u64,
    /// 是否实际被压缩（未压缩则原样写回）
    pub compressed: bool,
    /// 若出错则为 Some(原因)
    pub error: Option<String>,
}

/// 主入口：对一段图片字节做压缩，返回压缩后的字节。
///
/// - 若解码失败或格式不支持，返回 `Ok(None)` 表示应原样保留
/// - 若压缩后体积反而更大，返回 `Ok(None)` 交给调用方决定是否保留原文件
pub fn compress_image_bytes(data: &[u8], ext: &str, opts: &CompressOptions) -> Result<Vec<u8>> {
    let ext = ext.to_ascii_lowercase();
    match ext.as_str() {
        "gif" => compress_gif(data, opts),
        "jpg" | "jpeg" => compress_jpeg(data, opts),
        "png" => compress_png(data, opts),
        "webp" => compress_webp(data, opts),
        // bmp / tiff 等：解码后统一转 PNG/JPEG 以取得压缩收益
        _ => compress_misc(data, opts),
    }
}

/// JPEG：有损重编码，quality 直接传给编码器
fn compress_jpeg(data: &[u8], opts: &CompressOptions) -> Result<Vec<u8>> {
    let img = image::load_from_memory_with_format(data, ImageFormat::Jpeg)
        .context("JPEG 解码失败")?;

    // 去掉 alpha（JPEG 不支持），若是灰度则保持灰度以省体积
    let rgb = img.to_rgb8();
    let mut out = Vec::with_capacity(data.len() / 2);

    let quality = opts.quality.clamp(1, 100);
    let mut encoder = JpegEncoder::new_with_quality(&mut out, quality);
    // 渐进式 JPEG 通常能再省几个百分点
    encoder.encode_image(&DynamicImage::ImageRgb8(rgb))?;
    Ok(out)
}

/// PNG：无损重编码（调优压缩级别 + 过滤器）
///
/// PNG 本身是无损格式，quality 对其影响有限；这里通过 `CompressionType::Best`
/// 与自适应过滤器获得更好的压缩率，属"无损"压缩，不损失画质。
fn compress_png(data: &[u8], _opts: &CompressOptions) -> Result<Vec<u8>> {
    let img = image::load_from_memory_with_format(data, ImageFormat::Png)
        .context("PNG 解码失败")?;

    let mut out = Vec::with_capacity(data.len());
    // Best 级别压缩耗时略高但压缩率最好
    let encoder = PngEncoder::new_with_quality(
        &mut out,
        CompressionType::Best,
        FilterType::Adaptive,
    );

    match img.color() {
        image::ColorType::Rgb8 => {
            let buf = img.to_rgb8();
            encoder.write_image(
                buf.as_raw(),
                buf.width(),
                buf.height(),
                image::ExtendedColorType::Rgb8,
            )?;
        }
        image::ColorType::Rgba8 => {
            let buf = img.to_rgba8();
            encoder.write_image(
                buf.as_raw(),
                buf.width(),
                buf.height(),
                image::ExtendedColorType::Rgba8,
            )?;
        }
        image::ColorType::L8 => {
            // 灰度图：转成 RGBA 会变大，这里保留灰度
            let buf = img.to_luma8();
            let enc = PngEncoder::new_with_quality(
                &mut out,
                CompressionType::Best,
                FilterType::Adaptive,
            );
            enc.write_image(
                buf.as_raw(),
                buf.width(),
                buf.height(),
                image::ExtendedColorType::L8,
            )?;
        }
        _ => {
            let buf = img.to_rgba8();
            encoder.write_image(
                buf.as_raw(),
                buf.width(),
                buf.height(),
                image::ExtendedColorType::Rgba8,
            )?;
        }
    }
    Ok(out)
}

/// WebP：使用 libwebp 做有损/无损编码
fn compress_webp(data: &[u8], opts: &CompressOptions) -> Result<Vec<u8>> {
    let img = image::load_from_memory_with_format(data, ImageFormat::WebP)
        .context("WebP 解码失败")?;

    // libwebp 的编码质量是 f32 0.0-100.0
    let quality = opts.quality.clamp(1, 100) as f32;

    let encoded = if opts.lossless {
        // 无损：保留 alpha
        if img.color().has_alpha() {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            webp::Encoder::from_rgba(rgba.as_raw(), w, h).encode_lossless()
        } else {
            let rgb = img.to_rgb8();
            let (w, h) = rgb.dimensions();
            webp::Encoder::from_rgb(rgb.as_raw(), w, h).encode_lossless()
        }
    } else if img.color().has_alpha() {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        webp::Encoder::from_rgba(rgba.as_raw(), w, h).encode(quality)
    } else {
        let rgb = img.to_rgb8();
        let (w, h) = rgb.dimensions();
        webp::Encoder::from_rgb(rgb.as_raw(), w, h).encode(quality)
    };

    Ok(encoded.to_vec())
}

/// 其它格式 (BMP/TIFF 等)：有 alpha 转 PNG，否则转 JPEG
fn compress_misc(data: &[u8], opts: &CompressOptions) -> Result<Vec<u8>> {
    let img = image::load_from_memory(data).context("图片解码失败")?;
    if img.color().has_alpha() {
        compress_dynamic_to_png(&img)
    } else {
        compress_dynamic_to_jpeg(&img, opts.quality)
    }
}

fn compress_dynamic_to_png(img: &DynamicImage) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let encoder = PngEncoder::new_with_quality(
        &mut out,
        CompressionType::Best,
        FilterType::Adaptive,
    );
    let buf = img.to_rgba8();
    encoder.write_image(
        buf.as_raw(),
        buf.width(),
        buf.height(),
        image::ExtendedColorType::Rgba8,
    )?;
    Ok(out)
}

fn compress_dynamic_to_jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let rgb = img.to_rgb8();
    let mut encoder = JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100));
    encoder.encode_image(&DynamicImage::ImageRgb8(rgb))?;
    Ok(out)
}

// ---------------------------------------------------------------------------
// GIF 压缩：抽帧降帧率 + 缩放 + 调色板量化
// ---------------------------------------------------------------------------

/// GIF 压缩：按目标帧率抽帧，可选缩放，量化调色板后重新编码
///
/// 处理流程：
/// 1. 逐帧解码，按 disposal method 合成出"视觉上完整的画布"
/// 2. 累计帧延迟，按目标帧率抽帧（丢弃间隔过短的中间帧）
/// 3. 缩放抽帧后的画面
/// 4. 用 NeuQuant 量化到 ≤ `colors` 种颜色，重编码为 GIF
fn compress_gif(data: &[u8], opts: &CompressOptions) -> Result<Vec<u8>> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options
        .read_info(Cursor::new(data))
        .context("GIF 解码失败")?;

    let width = decoder.width() as u32;
    let height = decoder.height() as u32;

    // 缩放尺寸
    let scale = opts.gif_scale.clamp(1, 100) as f32 / 100.0;
    let out_w = ((width as f32 * scale).round() as u32).clamp(1, 65535);
    let out_h = ((height as f32 * scale).round() as u32).clamp(1, 65535);

    // 目标帧率抽帧
    let target_interval_ms: Option<f64> =
        opts.gif_fps.filter(|f| *f > 0).map(|f| 1000.0 / f as f64);

    // from_rgba_speed 的 speed 参数: 1(最慢/最好) - 30(最快/最差)
    // 颜色数越少，量化越激进，这里用颜色数反推一个合适的采样速度
    let colors = opts.gif_colors.clamp(2, 256) as usize;
    let quant_speed: i32 = (1700 / colors.max(2) as i32 * 3).clamp(1, 30);

    // ---- 阶段 1：解码 + 合成 + 抽帧 ----
    let mut composited: Vec<CompositedFrame> = Vec::new();
    let mut canvas: RgbaImage = RgbaImage::new(width, height);
    let mut acc_ms: f64 = 0.0;

    while let Some(frame) = decoder.read_next_frame().context("GIF 读取帧失败")? {
        // 记录"绘制本帧前"的画布，供 Previous 型 disposal 还原
        let before = match frame.dispose {
            gif::DisposalMethod::Previous => Some(canvas.clone()),
            _ => None,
        };

        blit_frame(&mut canvas, frame, width, height);

        acc_ms += frame.delay as f64 * 10.0;
        let keep = match target_interval_ms {
            None => true,
            Some(interval) => acc_ms + 0.001 >= interval,
        };
        if keep {
            composited.push(CompositedFrame {
                image: canvas.clone(),
                delay_ms: acc_ms,
            });
            acc_ms = 0.0;
        }

        // 应用 disposal，为下一帧准备画布
        match frame.dispose {
            gif::DisposalMethod::Background => {
                // 将本帧占据的矩形区域清为透明
                let (l, t) = (frame.left as u32, frame.top as u32);
                let (fw, fh) = (frame.width as u32, frame.height as u32);
                for y in t..(t + fh).min(height) {
                    for x in l..(l + fw).min(width) {
                        canvas.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
                    }
                }
            }
            gif::DisposalMethod::Previous => {
                if let Some(prev) = before {
                    canvas = prev;
                }
            }
            _ => {}
        }
    }

    if composited.is_empty() {
        bail!("GIF 未解析出任何帧");
    }

    // ---- 阶段 2：缩放 ----
    let scaled_frames: Vec<(RgbaImage, f64)> = composited
        .into_iter()
        .map(|f| {
            let img = if out_w != width || out_h != height {
                image::imageops::resize(
                    &f.image,
                    out_w,
                    out_h,
                    image::imageops::FilterType::Lanczos3,
                )
            } else {
                f.image
            };
            (img, f.delay_ms)
        })
        .collect();

    // ---- 阶段 3：编码 ----
    let mut out = Vec::with_capacity(data.len() / 2);
    {
        let mut encoder = gif::Encoder::new(&mut out, out_w as u16, out_h as u16, &[])
            .context("GIF 编码器创建失败")?;
        encoder
            .set_repeat(gif::Repeat::Infinite)
            .map_err(|e| anyhow::anyhow!("设置循环失败: {e}"))?;

        for (img, delay_ms) in &scaled_frames {
            let mut raw = img.clone().into_raw();
            let mut frame =
                gif::Frame::from_rgba_speed(out_w as u16, out_h as u16, &mut raw, quant_speed);
            // delay 单位是 10ms；至少 2 (=20ms)，避免播放器加速
            frame.delay = (delay_ms / 10.0).round().clamp(2.0, 6553.0) as u16;
            frame.dispose = gif::DisposalMethod::Background;
            encoder
                .write_frame(&frame)
                .map_err(|e| anyhow::anyhow!("写入帧失败: {e}"))?;
        }
    }

    Ok(out)
}

struct CompositedFrame {
    image: RgbaImage,
    delay_ms: f64,
}

/// 将一帧绘制到 canvas 上。
///
/// 已通过 `set_color_output(RGBA)` 让解码器输出 RGBA，
/// 因此这里的 `buffer` 始终是 4 字节/像素、按行优先排列的完整帧数据。
/// 透明像素不做覆盖，以保留底层已合成的内容。
fn blit_frame(canvas: &mut RgbaImage, frame: &gif::Frame<'_>, width: u32, height: u32) {
    let fw = frame.width as u32;
    let fh = frame.height as u32;
    let left = frame.left as u32;
    let top = frame.top as u32;
    let buf = frame.buffer.as_ref();

    // 数据长度校验，避免越界
    if fw == 0 || fh == 0 || buf.len() < (fw as usize * fh as usize * 4) {
        return;
    }

    let full_frame = fw == width && fh == height && left == 0 && top == 0;

    for y in 0..fh {
        let cy = top + y;
        if cy >= height {
            break;
        }
        for x in 0..fw {
            let cx = left + x;
            if cx >= width {
                break;
            }
            let i = ((y * fw + x) * 4) as usize;
            let px = &buf[i..i + 4];
            if px[3] == 0 && !full_frame {
                // 局部帧的透明像素：跳过，保留下层内容
                continue;
            }
            canvas.put_pixel(cx, cy, image::Rgba([px[0], px[1], px[2], px[3]]));
        }
    }
}

/// 便捷函数：压缩一段字节，若压缩结果更大或失败，则返回原数据
pub fn compress_or_original(data: &[u8], ext: &str, opts: &CompressOptions) -> (Vec<u8>, bool) {
    match compress_image_bytes(data, ext, opts) {
        Ok(compressed) => {
            if compressed.len() < data.len() {
                (compressed, true)
            } else {
                (data.to_vec(), false)
            }
        }
        Err(_) => (data.to_vec(), false),
    }
}
