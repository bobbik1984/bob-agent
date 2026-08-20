#!/usr/bin/env python3
"""
sync_android_icons.py
────────────────────
将 src-tauri/icons/android/ 下的各密度图标同步到
src-tauri/gen/android/app/src/main/res/ 对应的 mipmap-* 目录。

使用场景：
  - 在 `tauri android build` 或 `tauri android dev` 之前运行
  - 或者将此脚本挂到 CI/CD 的 pre-build 步骤

运行方式：
  cd bob-agent
  python scripts/sync_android_icons.py
"""

import shutil
import sys
from pathlib import Path

# 项目根目录 (bob-agent/)
PROJECT_ROOT = Path(__file__).resolve().parent.parent
SRC_TAURI = PROJECT_ROOT / "src-tauri"

ICONS_DIR = SRC_TAURI / "icons" / "android"
GEN_RES_DIR = SRC_TAURI / "gen" / "android" / "app" / "src" / "main" / "res"

DENSITY_DIRS = [
    "mipmap-mdpi",
    "mipmap-hdpi",
    "mipmap-xhdpi",
    "mipmap-xxhdpi",
    "mipmap-xxxhdpi",
]

def main():
    if not ICONS_DIR.exists():
        print(f"❌ 图标源目录不存在: {ICONS_DIR}")
        sys.exit(1)

    if not GEN_RES_DIR.exists():
        print(f"⚠  gen/android/res 目录不存在，可能需要先运行 `npx tauri android init`")
        print(f"   目标路径: {GEN_RES_DIR}")
        sys.exit(1)

    synced = 0
    for density in DENSITY_DIRS:
        src_dir = ICONS_DIR / density
        dst_dir = GEN_RES_DIR / density

        if not src_dir.exists():
            print(f"  ⚠ 跳过 (源目录缺失): {density}")
            continue

        dst_dir.mkdir(parents=True, exist_ok=True)

        for icon_file in src_dir.iterdir():
            if icon_file.is_file() and icon_file.suffix == ".png":
                dst_file = dst_dir / icon_file.name
                shutil.copy2(icon_file, dst_file)
                synced += 1
                print(f"  ✅ {density}/{icon_file.name}")

    if synced > 0:
        print(f"\n🎉 同步完成: {synced} 个图标文件已覆写到 gen/android/res/")
    else:
        print(f"\n⚠ 没有找到任何图标文件需要同步")

if __name__ == "__main__":
    main()
