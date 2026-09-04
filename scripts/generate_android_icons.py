"""
generate_android_icons.py
─────────────────────────
以 public/bob_white.png 为唯一真理源 (SSOT)，
自动为 Android 生成完整的自适应图标 (adaptive icon) 和旧版图标。

设计规范：
1. 自适应图标 (Adaptive Icons, API 26+)：
   - 画布总宽: 108dp (xxxhdpi 对应 432px)
   - 系统可见圆盘遮罩: 直径 72dp (288px)
   - 核心安全区: 直径 66dp
   - LOGO 占比: 锁定为画布的 50% (例如 432px 画布上，Logo 宽 216px，高约 141px)，
     在圆形遮罩下留出充裕的呼吸留白，与 ChatGPT / Claude 视觉比例完美对齐。
2. 背景层：纯白不透明 (#FFFFFF)。
3. 旧版单层图标 (Legacy Icons)：纯白底座 + 62% 居中 Logo。
"""
import os
import numpy as np
from PIL import Image, ImageDraw

PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ANDROID_DIR = os.path.join(PROJECT_ROOT, "src-tauri", "icons", "android")
SOURCE_IMAGE = os.path.join(PROJECT_ROOT, "public", "bob_white.png")
FALLBACK_ICON = os.path.join(PROJECT_ROOT, "src-tauri", "icons", "icon.png")

# Android 各密度的尺寸规范
DENSITIES = {
    "mipmap-mdpi":    {"legacy": 48,  "adaptive": 108},
    "mipmap-hdpi":    {"legacy": 72,  "adaptive": 162},
    "mipmap-xhdpi":   {"legacy": 96,  "adaptive": 216},
    "mipmap-xxhdpi":  {"legacy": 144, "adaptive": 324},
    "mipmap-xxxhdpi": {"legacy": 192, "adaptive": 432},
}

# 比例锁死：保证自适应图标在任何圆形/异形遮罩下绝不撑爆、绝不切断
LOGO_SCALE_ADAPTIVE = 0.50
LOGO_SCALE_LEGACY = 0.62


def load_and_crop_logo():
    """从母图提取蓝色 Logo 主体并剔除透明/白色边缘，以实现严格的居中与安全区计算"""
    if os.path.exists(SOURCE_IMAGE):
        raw = Image.open(SOURCE_IMAGE).convert("RGBA")
        arr = np.array(raw)
        # 寻找非白色的蓝色像素主体 (r < 240 或 g < 240 或 b < 240)
        is_blue = (arr[:, :, 0] < 240) | (arr[:, :, 1] < 240) | (arr[:, :, 2] < 240)
        arr[:, :, 3] = np.where(is_blue, 255, 0)
        img = Image.fromarray(arr)
        bbox = img.getbbox()
        if bbox:
            cropped = img.crop(bbox)
            print(f"Loaded from {SOURCE_IMAGE}")
            print(f"Extracted blue logo bbox: {bbox} -> size: {cropped.size[0]}x{cropped.size[1]}")
            return cropped

    print(f"Fallback to {FALLBACK_ICON}")
    img = Image.open(FALLBACK_ICON).convert("RGBA")
    bbox = img.getbbox()
    if bbox:
        return img.crop(bbox)
    return img


def resize_logo(logo, max_dim):
    """等比缩放 Logo 使其最长边 == max_dim"""
    ratio = max_dim / max(logo.width, logo.height)
    new_w = int(logo.width * ratio)
    new_h = int(logo.height * ratio)
    return logo.resize((new_w, new_h), Image.Resampling.LANCZOS)


def center_offset(canvas_size, item_size):
    """计算居中粘贴偏移量"""
    return ((canvas_size - item_size[0]) // 2,
            (canvas_size - item_size[1]) // 2)


def generate_adaptive_background(size, out_path):
    """纯白不透明背景层"""
    bg = Image.new("RGBA", (size, size), (255, 255, 255, 255))
    bg.save(out_path)


def generate_adaptive_foreground(logo, size, out_path):
    """透明画布 + 居中 Logo (严格控制在 LOGO_SCALE_ADAPTIVE 安全区内)"""
    fg = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    logo_max = int(size * LOGO_SCALE_ADAPTIVE)
    resized = resize_logo(logo, logo_max)
    offset = center_offset(size, resized.size)
    fg.paste(resized, offset, resized)
    fg.save(out_path)


def generate_legacy_square(logo, size, out_path):
    """旧版方形图标：纯白底座 + 居中 Logo"""
    canvas = Image.new("RGBA", (size, size), (255, 255, 255, 255))
    logo_max = int(size * LOGO_SCALE_LEGACY)
    resized = resize_logo(logo, logo_max)
    offset = center_offset(size, resized.size)
    canvas.paste(resized, offset, resized)
    canvas.save(out_path)


def generate_legacy_round(logo, size, out_path):
    """旧版圆形图标：白色圆盘 + 居中 Logo，四角透明"""
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(canvas)
    draw.ellipse((0, 0, size - 1, size - 1), fill=(255, 255, 255, 255))
    logo_max = int(size * LOGO_SCALE_LEGACY)
    resized = resize_logo(logo, logo_max)
    offset = center_offset(size, resized.size)
    canvas.paste(resized, offset, resized)
    canvas.save(out_path)


def generate_xml_files():
    """生成自适应图标的 XML 声明文件"""
    xml_dir = os.path.join(ANDROID_DIR, "mipmap-anydpi-v26")
    os.makedirs(xml_dir, exist_ok=True)

    xml_content = '<?xml version="1.0" encoding="utf-8"?>\n' \
                  '<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">\n' \
                  '  <foreground android:drawable="@mipmap/ic_launcher_foreground"/>\n' \
                  '  <background android:drawable="@mipmap/ic_launcher_background"/>\n' \
                  '</adaptive-icon>'

    for fname in ("ic_launcher.xml", "ic_launcher_round.xml"):
        path = os.path.join(xml_dir, fname)
        with open(path, "w", encoding="utf-8") as f:
            f.write(xml_content)
        print(f"  Written: {path}")

    # 同时提供 values/ 下的白色背景颜色定义作为后备
    values_dir = os.path.join(ANDROID_DIR, "values")
    os.makedirs(values_dir, exist_ok=True)
    color_xml = '<?xml version="1.0" encoding="utf-8"?>\n' \
                '<resources>\n' \
                '  <color name="ic_launcher_background">#FFFFFF</color>\n' \
                '</resources>'
    color_path = os.path.join(values_dir, "ic_launcher_background.xml")
    with open(color_path, "w", encoding="utf-8") as f:
        f.write(color_xml)
    print(f"  Written: {color_path}")


def main():
    logo = load_and_crop_logo()

    print("\nGenerating Android icons with locked Safe-Zone ratios...")
    for density_name, dims in DENSITIES.items():
        out_dir = os.path.join(ANDROID_DIR, density_name)
        os.makedirs(out_dir, exist_ok=True)

        adaptive_size = dims["adaptive"]
        legacy_size = dims["legacy"]

        print(f"  [{density_name}] adaptive={adaptive_size}px (logo: {int(adaptive_size*LOGO_SCALE_ADAPTIVE)}px), legacy={legacy_size}px")

        generate_adaptive_background(
            adaptive_size, os.path.join(out_dir, "ic_launcher_background.png"))
        generate_adaptive_foreground(
            logo, adaptive_size, os.path.join(out_dir, "ic_launcher_foreground.png"))
        generate_legacy_square(
            logo, legacy_size, os.path.join(out_dir, "ic_launcher.png"))
        generate_legacy_round(
            logo, legacy_size, os.path.join(out_dir, "ic_launcher_round.png"))

    print("\nGenerating XML configuration files...")
    generate_xml_files()

    print("\n✅ All Android icons regenerated successfully with SSOT Safe-Zone!")


if __name__ == "__main__":
    main()
