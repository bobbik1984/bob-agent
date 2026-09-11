# -*- coding: utf-8 -*-
"""
Bob Agent 云端产物一键同步与官网发布工具
- 从 GitHub Releases (latest-desktop 与 latest-mobile) 拉取最新构建好的带版本号二进制资产
- 存入本地 dist-release/ 目录 (保留带版本号的文件名，如 bob-v0.9.5-installer.exe)
- 自动触发 website/sync_deploy.py，改名并发布至官网 VPS1 (https://bob.bobbik.org)
"""

import os
import sys
import json
import urllib.request
import urllib.parse
import ssl
import subprocess

ROOT_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
DIST_DIR = os.path.join(ROOT_DIR, "dist-release")
REPO = "bobbik1984/bob-agent"

def download_file(url, target_path):
    print(f"  正在下载: {os.path.basename(target_path)} ...")
    ctx = ssl.create_default_context()
    req = urllib.request.Request(url, headers={"User-Agent": "Bob-Artifact-Puller"})
    
    with urllib.request.urlopen(req, context=ctx) as resp:
        total_length = resp.headers.get('content-length')
        total_size = int(total_length) if total_length else None
        downloaded = 0
        block_size = 1024 * 1024  # 1MB
        
        with open(target_path, 'wb') as f:
            while True:
                chunk = resp.read(block_size)
                if not chunk:
                    break
                f.write(chunk)
                downloaded += len(chunk)
                if total_size:
                    pct = (downloaded / total_size) * 100
                    mb_done = downloaded / (1024 * 1024)
                    mb_total = total_size / (1024 * 1024)
                    sys.stdout.write(f"\r    -> 进度: {pct:.1f}% ({mb_done:.1f}MB / {mb_total:.1f}MB)")
                    sys.stdout.flush()
    print("\n    ✓ 下载完成。")

def fetch_release_assets(tag):
    url = f"https://api.github.com/repos/{REPO}/releases/tags/{tag}"
    ctx = ssl.create_default_context()
    req = urllib.request.Request(url, headers={"User-Agent": "Bob-Artifact-Puller"})
    try:
        with urllib.request.urlopen(req, context=ctx) as resp:
            data = json.loads(resp.read().decode("utf-8"))
            return data.get("assets", [])
    except Exception as e:
        print(f"  [WARN] 获取 Release {tag} 失败: {e}")
        return []

def main():
    print("===========================================================")
    print("   Bob Agent 云端全平台构建产物同步与分发中心")
    print("===========================================================")
    os.makedirs(DIST_DIR, exist_ok=True)

    # 1. 获取 Windows 桌面端产物
    print("\n[1/3] 检索 Windows 桌面端云端最新构建产物 (latest-desktop)...")
    desktop_assets = fetch_release_assets("latest-desktop")
    # 优先下载带版本号的文件，防止冗余
    downloaded_any = False
    for a in desktop_assets:
        name = a["name"]
        if name.startswith("bob-v") and (name.endswith("-installer.exe") or name.endswith("-portable.zip")):
            target = os.path.join(DIST_DIR, name)
            download_file(a["browser_download_url"], target)
            downloaded_any = True
    
    # 若无带版本号的文件，降级下载固定命名文件
    if not downloaded_any:
        for a in desktop_assets:
            name = a["name"]
            if name in ("bob-installer.exe", "bob-agent-portable.zip"):
                target = os.path.join(DIST_DIR, name)
                download_file(a["browser_download_url"], target)

    # 2. 获取 Android 移动端产物
    print("\n[2/3] 检索 Android 移动端云端最新构建产物 (latest-mobile)...")
    mobile_assets = fetch_release_assets("latest-mobile")
    downloaded_mobile = False
    for a in mobile_assets:
        name = a["name"]
        if name.startswith("bob-v") and name.endswith("-signed.apk"):
            target = os.path.join(DIST_DIR, name)
            download_file(a["browser_download_url"], target)
            downloaded_mobile = True
    
    if not downloaded_mobile:
        for a in mobile_assets:
            name = a["name"]
            if name == "bob-mobile-latest.apk":
                target = os.path.join(DIST_DIR, name)
                download_file(a["browser_download_url"], target)

    print("\n[3/3] 本地 dist-release/ 产物就绪，开始同步至官网...")
    sync_script = os.path.join(ROOT_DIR, "website", "sync_deploy.py")
    if os.path.exists(sync_script):
        ret = subprocess.call([sys.executable, sync_script])
        if ret == 0:
            print("\n🎉 官网发布成功！新版本已同步至 VPS1 静态下载中心。")
        else:
            print("\n[WARN] 官网部署脚本返回异常。")

if __name__ == "__main__":
    main()
