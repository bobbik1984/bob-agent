"""
generate_android_icons.py
从 src-tauri/icons/icon.png (由 npx tauri icon 生成) 出发，
为 Android 生成完整的自适应图标 (adaptive icon) 和旧版图标。

本脚本在本地运行，生成的文件提交到 git。
CI 中由 patch_android_icons.py 负责将这些文件复制到 gen/ 目录。
"""
import os
from PIL import Image, ImageDraw

ANDROID_DIR = "src-tauri/icons/android"
SOURCE_ICON = "src-tauri/icons/icon.png"

# Android 各密度的尺寸规范
DENSITIES = {
    "mipmap-mdpi":    {"legacy": 48,  "adaptive": 108},
    "mipmap-hdpi":    {"legacy": 72,  "adaptive": 162},
    "mipmap-xhdpi":   {"legacy": 96,  "adaptive": 216},
    "mipmap-xxhdpi":  {"legacy": 144, "adaptive": 324},
    "mipmap-xxxhdpi": {"legacy": 192, "adaptive": 432},
}

# Logo 在自适应图标安全区中的占比
# Android 安全区 = 自适应画布的 66.7%，此值是相对于整个画布的比例
# 0.60 意味着 Logo 占安全区的 ~90%，视觉饱满且不会被裁切
LOGO_SCALE = 0.60


def load_and_crop_logo():
    """加载源图标并裁剪掉四周的透明留白"""
    img = Image.open(SOURCE_ICON).convert("RGBA")
    # 用 alpha 通道的 bbox 精确裁剪
    alpha_bbox = img.split()[-1].getbbox()
    if alpha_bbox:
        img = img.crop(alpha_bbox)
    print(f"Source logo cropped to: {img.size[0]}x{img.size[1]}")
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
    """透明画布 + 居中 Logo（占画布 LOGO_SCALE 比例）"""
    fg = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    logo_max = int(size * LOGO_SCALE)
    resized = resize_logo(logo, logo_max)
    offset = center_offset(size, resized.size)
    fg.paste(resized, offset, resized)
    fg.save(out_path)


def generate_legacy_square(logo, size, out_path):
    """旧版方形图标：白底 + 居中 Logo"""
    canvas = Image.new("RGBA", (size, size), (255, 255, 255, 255))
    logo_max = int(size * LOGO_SCALE)
    resized = resize_logo(logo, logo_max)
    offset = center_offset(size, resized.size)
    canvas.paste(resized, offset, resized)
    canvas.save(out_path)


def generate_legacy_round(logo, size, out_path):
    """旧版圆形图标：白色圆盘 + 居中 Logo，四角透明"""
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    # 画白色圆盘
    draw = ImageDraw.Draw(canvas)
    draw.ellipse((0, 0, size - 1, size - 1), fill=(255, 255, 255, 255))
    # 放置 Logo
    logo_max = int(size * LOGO_SCALE)
    resized = resize_logo(logo, logo_max)
    offset = center_offset(size, resized.size)
    canvas.paste(resized, offset, resized)
    canvas.save(out_path)


def generate_xml_files():
    """生成自适应图标的 XML 声明文件"""
    xml_dir = os.path.join(ANDROID_DIR, "mipmap-anydpi-v26")
    os.makedirs(xml_dir, exist_ok=True)

    # 使用 @mipmap/ 引用 PNG 背景图，而不是 @color/ 引用颜色值
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

    # 同时提供 values/ 下的颜色定义作为后备
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
    if not os.path.exists(SOURCE_ICON):
        print(f"Error: Source icon not found at {SOURCE_ICON}")
        print("Please run 'npx tauri icon public/bob_logo_square.svg' first.")
        return

    logo = load_and_crop_logo()

    print("\nGenerating Android icons...")
    for density_name, dims in DENSITIES.items():
        out_dir = os.path.join(ANDROID_DIR, density_name)
        os.makedirs(out_dir, exist_ok=True)

        adaptive_size = dims["adaptive"]
        legacy_size = dims["legacy"]

        print(f"\n  [{density_name}] adaptive={adaptive_size}dp, legacy={legacy_size}dp")

        generate_adaptive_background(
            adaptive_size, os.path.join(out_dir, "ic_launcher_background.png"))
        generate_adaptive_foreground(
            logo, adaptive_size, os.path.join(out_dir, "ic_launcher_foreground.png"))
        generate_legacy_square(
            logo, legacy_size, os.path.join(out_dir, "ic_launcher.png"))
        generate_legacy_round(
            logo, legacy_size, os.path.join(out_dir, "ic_launcher_round.png"))

    print("\nGenerating XML files...")
    generate_xml_files()

    print("\n✅ All Android icons generated successfully!")
    print("Next: commit and push, then patch_android_icons.py will handle CI deployment.")


if __name__ == "__main__":
    main()
