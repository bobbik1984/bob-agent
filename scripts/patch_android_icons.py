"""
patch_android_icons.py
CI 专用脚本：在 `npx tauri android init` 之后、`npx tauri android build` 之前执行。

功能：
1. 将 src-tauri/icons/android/ 下的所有图标资源强制复制到
   src-tauri/gen/android/app/src/main/res/ 对应目录
2. 强制覆写 ic_launcher.xml，确保使用 @mipmap/ 引用（而非 @color/）
3. 消灭 gen/ 中所有残留的紫色 (#6200EE) 颜色定义

此脚本不依赖 PIL，只使用标准库。
"""
import os
import re
import shutil

# 路径定义
ICONS_SRC = "src-tauri/icons/android"
GEN_RES = "src-tauri/gen/android/app/src/main/res"


def copy_icon_assets():
    """将 icons/android/ 中的所有文件复制到 gen/ 的 res/ 目录"""
    if not os.path.exists(ICONS_SRC):
        print(f"ERROR: Source icons directory not found: {ICONS_SRC}")
        return False

    if not os.path.exists(GEN_RES):
        print(f"ERROR: Gen res directory not found: {GEN_RES}")
        print("Has 'npx tauri android init' been run?")
        return False

    copied = 0
    for root, dirs, files in os.walk(ICONS_SRC):
        for fname in files:
            src_path = os.path.join(root, fname)
            # 计算相对路径，映射到 gen/res/ 下
            rel_path = os.path.relpath(src_path, ICONS_SRC)
            dst_path = os.path.join(GEN_RES, rel_path)

            os.makedirs(os.path.dirname(dst_path), exist_ok=True)
            shutil.copy2(src_path, dst_path)
            copied += 1
            print(f"  Copied: {rel_path}")

    print(f"Total files copied: {copied}")
    return True


def force_write_adaptive_xml():
    """确保 gen/ 中的 ic_launcher.xml 使用 @mipmap/ 引用"""
    xml_dir = os.path.join(GEN_RES, "mipmap-anydpi-v26")
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
        print(f"  Force-written: {path}")


def force_write_background_color():
    """在 gen/ 的 values/ 下写入白色背景颜色定义（作为后备）"""
    values_dir = os.path.join(GEN_RES, "values")
    os.makedirs(values_dir, exist_ok=True)

    color_xml = '<?xml version="1.0" encoding="utf-8"?>\n' \
                '<resources>\n' \
                '  <color name="ic_launcher_background">#FFFFFF</color>\n' \
                '</resources>'

    path = os.path.join(values_dir, "ic_launcher_background.xml")
    with open(path, "w", encoding="utf-8") as f:
        f.write(color_xml)
    print(f"  Force-written: {path}")


def kill_purple_everywhere():
    """扫描 gen/res/ 中所有 XML，将紫色 #6200EE 及相关色替换为白色"""
    purple_replacements = {
        "#FF6200EE": "#FFFFFFFF",
        "#6200EE": "#FFFFFF",
        "#FF3700B3": "#FFFFFFFF",
        "#3700B3": "#FFFFFF",
    }

    patched_files = 0
    for root, dirs, files in os.walk(GEN_RES):
        for fname in files:
            if not fname.endswith(".xml"):
                continue
            path = os.path.join(root, fname)
            try:
                with open(path, "r", encoding="utf-8") as f:
                    content = f.read()

                new_content = content
                for old, new in purple_replacements.items():
                    new_content = new_content.replace(old, new)

                # 额外：强制替换 ic_launcher_background 颜色定义
                new_content = re.sub(
                    r'name="ic_launcher_background">[^<]+</color>',
                    'name="ic_launcher_background">#FFFFFF</color>',
                    new_content
                )

                if new_content != content:
                    with open(path, "w", encoding="utf-8") as f:
                        f.write(new_content)
                    patched_files += 1
                    print(f"  Patched purple in: {os.path.relpath(path, GEN_RES)}")
            except Exception as e:
                print(f"  Warning: Could not patch {path}: {e}")

    print(f"Total XML files patched: {patched_files}")


def main():
    print("=" * 60)
    print("Android Icon Patcher — Fixing Tauri default purple icons")
    print("=" * 60)

    print("\n[Step 1] Copying icon assets from icons/ to gen/res/...")
    if not copy_icon_assets():
        print("ABORT: Cannot proceed without source icons.")
        return

    print("\n[Step 2] Force-writing adaptive icon XML (using @mipmap/)...")
    force_write_adaptive_xml()

    print("\n[Step 3] Force-writing white background color definition...")
    force_write_background_color()

    print("\n[Step 4] Killing all remaining purple color values...")
    kill_purple_everywhere()

    print("\n" + "=" * 60)
    print("✅ Android icon patching complete!")
    print("   All purple has been eliminated. All icons use @mipmap/ refs.")
    print("=" * 60)


if __name__ == "__main__":
    main()
