//! 压缩核心与压缩包处理的单元测试
//!
//! 所有测试在内存中构造图片，不依赖外部文件。

use std::io::Write;

use image::codecs::jpeg::JpegEncoder;
#[allow(unused_imports)]
use image::ImageEncoder;
use image::{DynamicImage, RgbImage, RgbaImage};

use image_compressor_lib::compress::{
    compress_image_bytes, compress_or_original, is_supported_image, CompressOptions,
};

/// 生成一张高熵 RGB 图（压缩空间大，便于断言"压缩后更小"）
fn noisy_rgb(w: u32, h: u32) -> RgbImage {
    let mut img = RgbImage::new(w, h);
    let mut seed: u32 = 0x1234_5678;
    for px in img.pixels_mut() {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let r = (seed >> 24) as u8;
        let g = (seed >> 16) as u8;
        let b = (seed >> 8) as u8;
        *px = image::Rgb([r, g, b]);
    }
    img
}

fn png_bytes(img: &DynamicImage) -> Vec<u8> {
    let mut buf = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut buf),
        image::ImageFormat::Png,
    )
    .unwrap();
    buf
}

fn jpeg_bytes(img: &DynamicImage, q: u8) -> Vec<u8> {
    let mut buf = Vec::new();
    let rgb = img.to_rgb8();
    let mut enc = JpegEncoder::new_with_quality(&mut buf, q);
    enc.encode_image(&DynamicImage::ImageRgb8(rgb)).unwrap();
    buf
}

fn webp_bytes(img: &RgbImage, q: f32) -> Vec<u8> {
    let (w, h) = img.dimensions();
    webp::Encoder::from_rgb(img.as_raw(), w, h).encode(q).to_vec()
}

/// 生成多帧 GIF
fn gif_bytes(frames: usize, w: u16, h: u16, delay_cs: u16) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = gif::Encoder::new(&mut out, w, h, &[]).unwrap();
        enc.set_repeat(gif::Repeat::Infinite).unwrap();
        for i in 0..frames {
            let mut img = RgbaImage::new(w as u32, h as u32);
            // 每帧不同的图案，保证帧间有变化
            for y in 0..h as u32 {
                for x in 0..w as u32 {
                    let on = ((x + i as u32 * 5) % 40) < 20;
                    img.put_pixel(
                        x,
                        y,
                        if on {
                            image::Rgba([220, 40, 40, 255])
                        } else {
                            image::Rgba([30, 60, 200, 255])
                        },
                    );
                }
            }
            let mut raw = img.into_raw();
            let mut frame = gif::Frame::from_rgba_speed(w, h, &mut raw, 10);
            frame.delay = delay_cs;
            enc.write_frame(&frame).unwrap();
        }
    }
    out
}

fn count_gif_frames(data: &[u8]) -> usize {
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::RGBA);
    let mut dec = opts.read_info(std::io::Cursor::new(data)).unwrap();
    let mut n = 0;
    while dec.read_next_frame().unwrap().is_some() {
        n += 1;
    }
    n
}

#[test]
fn supported_extensions() {
    for name in ["a.png", "b.JPG", "c.Jpeg", "d.webp", "e.gif", "f.bmp", "g.TIFF"] {
        assert!(
            is_supported_image(std::path::Path::new(name)),
            "{name} 应被识别为支持"
        );
    }
    for name in ["a.txt", "b.md", "c.json", "d.mp4", "noext"] {
        assert!(
            !is_supported_image(std::path::Path::new(name)),
            "{name} 不应被识别为支持"
        );
    }
}

#[test]
fn jpeg_quality_affects_size() {
    // 必须喂真实 JPEG 数据（扩展名需与内容一致）
    let src = jpeg_bytes(&DynamicImage::ImageRgb8(noisy_rgb(400, 300)), 98);
    let opts_hi = CompressOptions {
        quality: 95,
        ..Default::default()
    };
    let opts_lo = CompressOptions {
        quality: 30,
        ..Default::default()
    };

    let hi = compress_image_bytes(&src, "jpg", &opts_hi).unwrap();
    let lo = compress_image_bytes(&src, "jpg", &opts_lo).unwrap();

    assert!(hi.len() < src.len(), "JPEG q95 应小于 q98 原图");
    assert!(
        lo.len() < hi.len(),
        "低质量 ({}) 应比高质量 ({}) 更小",
        lo.len(),
        hi.len()
    );

    // 输出必须是可解码的合法 JPEG
    let decoded = image::load_from_memory_with_format(&hi, image::ImageFormat::Jpeg).unwrap();
    assert_eq!(decoded.width(), 400);
    assert_eq!(decoded.height(), 300);
}

#[test]
fn png_is_lossless_and_valid() {
    let src = png_bytes(&DynamicImage::ImageRgb8(noisy_rgb(200, 150)));
    let opts = CompressOptions::default();
    let out = compress_image_bytes(&src, "png", &opts).unwrap();

    let a = image::load_from_memory_with_format(&src, image::ImageFormat::Png).unwrap();
    let b = image::load_from_memory_with_format(&out, image::ImageFormat::Png).unwrap();
    assert_eq!(a.width(), b.width());
    assert_eq!(a.height(), b.height());
    // PNG 无损：像素必须完全一致
    assert_eq!(
        a.to_rgba8().into_raw(),
        b.to_rgba8().into_raw(),
        "PNG 无损压缩后像素必须完全相同"
    );
}

#[test]
fn png_preserves_alpha() {
    let mut img = RgbaImage::new(64, 64);
    for (i, px) in img.pixels_mut().enumerate() {
        *px = image::Rgba([(i % 255) as u8, 100, 50, (i % 128) as u8]);
    }
    let src = png_bytes(&DynamicImage::ImageRgba8(img.clone()));
    let out = compress_image_bytes(&src, "png", &CompressOptions::default()).unwrap();
    let decoded = image::load_from_memory_with_format(&out, image::ImageFormat::Png).unwrap();
    assert!(decoded.color().has_alpha(), "PNG 应保留 alpha 通道");
    assert_eq!(decoded.to_rgba8().into_raw(), img.into_raw());
}

#[test]
fn webp_lossy_and_lossless() {
    let rgb = noisy_rgb(300, 200);
    let src = webp_bytes(&rgb, 95.0);

    let lossy = compress_image_bytes(
        &src,
        "webp",
        &CompressOptions {
            quality: 50,
            lossless: false,
            ..Default::default()
        },
    )
    .unwrap();

    let lossless = compress_image_bytes(
        &src,
        "webp",
        &CompressOptions {
            lossless: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert!(!lossy.is_empty());
    assert!(!lossless.is_empty());
    assert!(
        lossy.len() < lossless.len(),
        "有损 ({}) 应小于无损 ({})",
        lossy.len(),
        lossless.len()
    );

    // 两者都应可解码且尺寸一致
    for (label, data) in [("lossy", &lossy), ("lossless", &lossless)] {
        let d = image::load_from_memory_with_format(data, image::ImageFormat::WebP)
            .unwrap_or_else(|e| panic!("{label} webp 解码失败: {e}"));
        assert_eq!((d.width(), d.height()), (300, 200));
    }
}

#[test]
fn gif_frame_dropping_works() {
    // 20 帧，每帧 5cs (=50ms) -> 原始 20fps
    let src = gif_bytes(20, 120, 90, 5);
    assert_eq!(count_gif_frames(&src), 20);

    // 目标 10fps -> 应抽掉约一半帧
    let out = compress_image_bytes(
        &src,
        "gif",
        &CompressOptions {
            gif_fps: Some(10),
            gif_scale: 100,
            gif_colors: 256,
            ..Default::default()
        },
    )
    .unwrap();

    let frames = count_gif_frames(&out);
    assert!(
        frames < 20,
        "帧率压缩后帧数应减少，实际 {frames}"
    );
    assert!(frames >= 8, "不应过度抽帧，实际 {frames}");
    assert!(out.len() < src.len(), "GIF 应变小");
}

#[test]
fn gif_scaling_works() {
    let src = gif_bytes(6, 200, 100, 10);
    let out = compress_image_bytes(
        &src,
        "gif",
        &CompressOptions {
            gif_fps: None,
            gif_scale: 50,
            gif_colors: 256,
            ..Default::default()
        },
    )
    .unwrap();

    let opts = gif::DecodeOptions::new();
    let dec = opts.read_info(std::io::Cursor::new(&out)).unwrap();
    assert_eq!(dec.width(), 100, "50% 缩放后宽度应为 100");
    assert_eq!(dec.height(), 50, "50% 缩放后高度应为 50");
}

#[test]
fn gif_no_frame_drop_keeps_all_frames() {
    let src = gif_bytes(8, 80, 60, 10);
    let out = compress_image_bytes(
        &src,
        "gif",
        &CompressOptions {
            gif_fps: None,
            gif_scale: 100,
            gif_colors: 256,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(count_gif_frames(&out), 8, "不启用帧率压缩时应保留全部 8 帧");
}

#[test]
fn compress_or_original_never_grows() {
    // 一张已经高度优化的极小图，压缩后可能变大 -> 应返回原数据
    let tiny = png_bytes(&DynamicImage::ImageRgb8(RgbImage::new(2, 2)));
    let (out, compressed) = compress_or_original(&tiny, "png", &CompressOptions::default());
    assert!(
        out.len() <= tiny.len(),
        "compress_or_original 不应让体积变大 ({} > {})",
        out.len(),
        tiny.len()
    );
    if !compressed {
        assert_eq!(out, tiny, "未压缩时应返回原始字节");
    }
}

#[test]
fn corrupt_data_falls_back_to_original() {
    let garbage = b"this is definitely not an image";
    let (out, compressed) = compress_or_original(garbage, "jpg", &CompressOptions::default());
    assert!(!compressed, "垃圾数据不应被标记为已压缩");
    assert_eq!(out, garbage, "解码失败时应原样返回");
}

#[test]
fn quality_zero_is_clamped_not_panicking() {
    let src = jpeg_bytes(&DynamicImage::ImageRgb8(noisy_rgb(100, 100)), 95);
    // quality=0 必须被 clamp 到 1，不能 panic
    let out = compress_image_bytes(
        &src,
        "jpg",
        &CompressOptions {
            quality: 0,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!out.is_empty());
    image::load_from_memory_with_format(&out, image::ImageFormat::Jpeg).unwrap();
}

/// 扩展名与内容不符时，应报错而非产出损坏文件
/// （产品行为：`compress_or_original` 会因此保留原文件）
#[test]
fn mismatched_extension_is_rejected() {
    let png = png_bytes(&DynamicImage::ImageRgb8(noisy_rgb(50, 50)));
    let res = compress_image_bytes(&png, "jpg", &CompressOptions::default());
    assert!(res.is_err(), "PNG 数据标为 .jpg 应被拒绝");

    // 兜底路径应保留原文件
    let (out, compressed) = compress_or_original(&png, "jpg", &CompressOptions::default());
    assert!(!compressed);
    assert_eq!(out, png);
}

#[test]
fn quality_and_color_limits_are_clamped() {
    let src = gif_bytes(4, 60, 40, 10);
    // 越界参数不应 panic
    let out = compress_image_bytes(
        &src,
        "gif",
        &CompressOptions {
            gif_fps: Some(999),
            gif_scale: 0,
            gif_colors: 0,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!out.is_empty());
    count_gif_frames(&out);
}

// --------------------------- 压缩包流程测试 ---------------------------

use image_compressor_lib::archive::process_zip;
use std::io::Read;

#[test]
fn zip_preserves_structure_and_skips_non_images() {
    let dir = std::env::temp_dir().join(format!("ic_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("a/b/c")).unwrap();

    // 构造输入 zip：嵌套目录 + 非图片文件
    let in_zip = dir.join("in.zip");
    {
        let f = std::fs::File::create(&in_zip).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let opt = zip::write::SimpleFileOptions::default();

        let png = png_bytes(&DynamicImage::ImageRgb8(noisy_rgb(100, 80)));
        let jpg = jpeg_bytes(&DynamicImage::ImageRgb8(noisy_rgb(100, 80)), 95);

        zw.add_directory("a/", opt).unwrap();
        zw.add_directory("a/b/", opt).unwrap();
        zw.add_directory("a/b/c/", opt).unwrap();
        zw.start_file("a/top.png", opt).unwrap();
        zw.write_all(&png).unwrap();
        zw.start_file("a/b/mid.jpg", opt).unwrap();
        zw.write_all(&jpg).unwrap();
        zw.start_file("a/b/c/deep.png", opt).unwrap();
        zw.write_all(&png).unwrap();
        // 非图片，应被跳过
        zw.start_file("a/readme.txt", opt).unwrap();
        zw.write_all(b"hello").unwrap();
        zw.start_file("a/b/data.json", opt).unwrap();
        zw.write_all(b"{}").unwrap();
        zw.start_file("__MACOSX/a/._top.png", opt).unwrap();
        zw.write_all(b"junk").unwrap();
        zw.finish().unwrap();
    }

    let out_zip = dir.join("out.zip");
    let summary = process_zip(&in_zip, &out_zip, &CompressOptions::default(), |_, _, _| {})
        .expect("压缩应成功");

    assert_eq!(summary.total_images, 3, "应识别 3 张图片");
    assert_eq!(
        summary.skipped_files, 2,
        "应跳过 2 个用户非图片文件（txt/json）；__MACOSX 元数据不计入"
    );
    assert!(summary.compressed_total <= summary.original_total);
    assert!(out_zip.exists());

    // 校验输出结构
    let f = std::fs::File::open(&out_zip).unwrap();
    let mut ar = zip::ZipArchive::new(f).unwrap();
    let mut names: Vec<String> = (0..ar.len())
        .map(|i| ar.by_index(i).unwrap().name().to_string())
        .collect();
    names.sort();

    assert!(names.iter().any(|n| n == "a/top.png"), "应保留 a/top.png");
    assert!(
        names.iter().any(|n| n == "a/b/mid.jpg"),
        "应保留 a/b/mid.jpg（子目录）"
    );
    assert!(
        names.iter().any(|n| n == "a/b/c/deep.png"),
        "应保留 a/b/c/deep.png（深层子目录）"
    );
    assert!(
        !names.iter().any(|n| n.contains("txt") || n.contains("json")),
        "非图片文件不应出现在输出中: {names:?}"
    );
    assert!(
        !names.iter().any(|n| n.contains("__MACOSX")),
        "macOS 元数据应被过滤: {names:?}"
    );

    // 输出内容可解码
    for i in 0..ar.len() {
        let mut e = ar.by_index(i).unwrap();
        if e.is_dir() || !is_supported_image(std::path::Path::new(e.name())) {
            continue;
        }
        let mut buf = Vec::new();
        e.read_to_end(&mut buf).unwrap();
        image::load_from_memory(&buf)
            .unwrap_or_else(|err| panic!("输出 {} 无法解码: {err}", e.name()));
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn zip_with_no_images_errors() {
    let dir = std::env::temp_dir().join(format!("ic_test_noimg_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let in_zip = dir.join("in.zip");
    {
        let f = std::fs::File::create(&in_zip).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let opt = zip::write::SimpleFileOptions::default();
        zw.start_file("readme.txt", opt).unwrap();
        zw.write_all(b"no images here").unwrap();
        zw.finish().unwrap();
    }

    let res = process_zip(
        &in_zip,
        &dir.join("out.zip"),
        &CompressOptions::default(),
        |_, _, _| {},
    );
    assert!(res.is_err(), "无图片时应返回错误");
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("没有找到支持的图片"));

    let _ = std::fs::remove_dir_all(&dir);
}
