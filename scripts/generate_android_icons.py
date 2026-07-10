import os
from PIL import Image, ImageDraw

def generate_icons():
    base_dir = "src-tauri/icons/android"
    src_icon_path = "src-tauri/icons/icon.png"
    
    if not os.path.exists(src_icon_path):
        print("Source icon not found!")
        return

    # Load source logo and crop to exact bounding box
    logo = Image.open(src_icon_path).convert("RGBA")
    bbox = logo.getbbox()
    logo = logo.crop(bbox)
    
    sizes = {
        "mdpi": {"legacy": 48, "adaptive": 108},
        "hdpi": {"legacy": 72, "adaptive": 162},
        "xhdpi": {"legacy": 96, "adaptive": 216},
        "xxhdpi": {"legacy": 144, "adaptive": 324},
        "xxxhdpi": {"legacy": 192, "adaptive": 432}
    }
    
    for dpi, dims in sizes.items():
        legacy_size = dims["legacy"]
        adaptive_size = dims["adaptive"]
        out_dir = os.path.join(base_dir, f"mipmap-{dpi}")
        os.makedirs(out_dir, exist_ok=True)
        
        # 1. Adaptive Background (Solid White)
        bg = Image.new("RGBA", (adaptive_size, adaptive_size), (255, 255, 255, 255))
        bg.save(os.path.join(out_dir, "ic_launcher_background.png"))
        
        # 2. Adaptive Foreground (Transparent with logo in 60% safe zone)
        fg = Image.new("RGBA", (adaptive_size, adaptive_size), (0, 0, 0, 0))
        logo_max_dim = int(adaptive_size * 0.60)
        # Calculate resize ratio
        ratio = logo_max_dim / max(logo.width, logo.height)
        new_size = (int(logo.width * ratio), int(logo.height * ratio))
        resized_logo = logo.resize(new_size, Image.Resampling.LANCZOS)
        # Paste in center
        offset = ((adaptive_size - new_size[0]) // 2, (adaptive_size - new_size[1]) // 2)
        fg.paste(resized_logo, offset, resized_logo)
        fg.save(os.path.join(out_dir, "ic_launcher_foreground.png"))
        
        # 3. Legacy Round Icon (White circle on transparent background with logo)
        round_icon = Image.new("RGBA", (legacy_size, legacy_size), (0, 0, 0, 0))
        draw = ImageDraw.Draw(round_icon)
        # Draw white circle
        draw.ellipse((0, 0, legacy_size-1, legacy_size-1), fill=(255, 255, 255, 255))
        # Add logo
        logo_legacy_dim = int(legacy_size * 0.60)
        ratio_legacy = logo_legacy_dim / max(logo.width, logo.height)
        new_size_legacy = (int(logo.width * ratio_legacy), int(logo.height * ratio_legacy))
        resized_logo_legacy = logo.resize(new_size_legacy, Image.Resampling.LANCZOS)
        offset_legacy = ((legacy_size - new_size_legacy[0]) // 2, (legacy_size - new_size_legacy[1]) // 2)
        round_icon.paste(resized_logo_legacy, offset_legacy, resized_logo_legacy)
        round_icon.save(os.path.join(out_dir, "ic_launcher_round.png"))
        
        # 4. Legacy Icon (White square with slightly rounded corners (optional), or just white square)
        # We will use a white circle just like the round icon, because modern Android launchers prefer circles.
        # But if they force square, we will provide a solid white square. Wait, standard is square for legacy.
        legacy_icon = Image.new("RGBA", (legacy_size, legacy_size), (255, 255, 255, 255))
        legacy_icon.paste(resized_logo_legacy, offset_legacy, resized_logo_legacy)
        legacy_icon.save(os.path.join(out_dir, "ic_launcher.png"))

    # Generate XMLs
    xml_dir = os.path.join(base_dir, "mipmap-anydpi-v26")
    os.makedirs(xml_dir, exist_ok=True)
    xml_content = '''<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
  <foreground android:drawable="@mipmap/ic_launcher_foreground"/>
  <background android:drawable="@mipmap/ic_launcher_background"/>
</adaptive-icon>'''
    with open(os.path.join(xml_dir, "ic_launcher.xml"), "w") as f:
        f.write(xml_content)
    with open(os.path.join(xml_dir, "ic_launcher_round.xml"), "w") as f:
        f.write(xml_content)

if __name__ == "__main__":
    generate_icons()
