#!/bin/bash
# 生成测试用图片素材与 zip 压缩包
# 用法: ./scripts/make-testdata.sh [输出目录]
set -euo pipefail

OUT="${1:-/tmp/ic-testdata}"
rm -rf "$OUT"
mkdir -p "$OUT/images/sub/deep"

python3 - "$OUT" << 'PY'
import sys, random
from PIL import Image, ImageDraw

out = sys.argv[1]
random.seed(42)

def noisy(size, mode="RGB"):
    img = Image.new(mode, size)
    px = img.load()
    for y in range(size[1]):
        for x in range(size[0]):
            if mode == "RGB":
                px[x, y] = (random.randint(0,255), random.randint(0,255), random.randint(0,255))
            else:
                px[x, y] = random.randint(0, 255)
    return img

base = f"{out}/images"

# PNG（含 alpha），分布于多层子目录
noisy((300, 200), "RGBA").save(f"{base}/photo1.png")
noisy((300, 200), "RGBA").save(f"{base}/sub/photo2.png")
noisy((300, 200), "RGBA").save(f"{base}/sub/deep/photo3.png")

# JPEG（高质量，便于有损压缩）
for p in [f"{base}/photo1.png", f"{base}/sub/photo2.png"]:
    Image.open(p).convert("RGB").save(p.replace(".png", ".jpg"), quality=98)

# WebP
noisy((250, 250), "RGB").save(f"{base}/anim.webp", quality=95)

# 多帧 GIF（20 帧，20fps）
frames = []
for i in range(20):
    f = Image.new("RGB", (200, 150))
    d = ImageDraw.Draw(f)
    d.rectangle([i*8, 0, i*8+40, 150], fill=(255, 50, 50))
    d.ellipse([100+i, 60, 140+i, 100], fill=(50, 120, 255))
    frames.append(f)
frames[0].save(f"{base}/anim.gif", save_all=True, append_images=frames[1:], duration=50, loop=0)

# 非图片文件（应被跳过）
open(f"{base}/readme.txt", "w").write("not an image")
open(f"{base}/sub/notes.md", "w").write("# notes")
open(f"{base}/data.json", "w").write('{"a": 1}')
print("test images created")
PY

# 打包
(cd "$OUT/images" && zip -r -q "$OUT/input.zip" . -x '.DS_Store')

echo ""
echo "✅ 测试素材已生成:"
echo "   图片目录: $OUT/images"
echo "   输入压缩包: $OUT/input.zip"
find "$OUT/images" -type f | sed "s|$OUT/||" | sort | sed 's/^/     /'
