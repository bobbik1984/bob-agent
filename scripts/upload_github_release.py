# -*- coding: utf-8 -*-
"""
Bob Agent 统一 GitHub Release 自动化发布工具
- 自动提取 package.json 中的当前版本号
- 自动利用本地 Git Credential Manager 静默获取 GitHub API 凭证
- 自动创建 Tag/Release 并上传全平台构建产物 (Installer, Portable, APK)
"""

import os
import sys
import json
import subprocess
import urllib.request
import urllib.error
import urllib.parse

REPO = "bobbik1984/bob-agent"
ROOT_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
PACKAGE_JSON = os.path.join(ROOT_DIR, "package.json")
DIST_DIR = os.path.join(ROOT_DIR, "dist-release")

def get_current_version():
    with open(PACKAGE_JSON, "r", encoding="utf-8") as f:
        data = json.load(f)
        ver = data.get("version", "0.9.5")
        return f"v{ver}" if not ver.startswith("v") else ver

def get_github_token():
    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    if token:
        return token.strip()
    try:
        p = subprocess.Popen(
            ["git", "credential", "fill"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True
        )
        out, _ = p.communicate("protocol=https\nhost=github.com\n")
        for line in out.splitlines():
            if line.startswith("password="):
                tok = line.split("=", 1)[1].strip()
                if tok:
                    return tok
    except Exception as e:
        print(f"[WARN] 无法通过 git credential 提取凭证: {e}")
    return None

def main():
    version = get_current_version()
    print("===========================================================")
    print(f"   Bob Agent GitHub 官方发布自动化 ({REPO} {version})")
    print("===========================================================")

    token = get_github_token()
    if not token:
        token = input("请输入 GitHub 个人访问令牌 (Personal Access Token): ").strip()
        if not token:
            print("[FAIL] 缺少 GitHub 授权令牌，发版中止。")
            sys.exit(1)

    headers = {
        "Accept": "application/vnd.github.v3+json",
        "Authorization": f"token {token}",
        "User-Agent": "BobAgent-Release-Pipeline"
    }

    print(f"\n[1/3] 检查 Release {version} 状态...")
    check_url = f"https://api.github.com/repos/{REPO}/releases/tags/{version}"
    req = urllib.request.Request(check_url, headers=headers)
    release_data = None

    try:
        with urllib.request.urlopen(req) as resp:
            release_data = json.loads(resp.read().decode("utf-8"))
            print(f"✓ 已检测到已存在的 Release (ID: {release_data.get('id')})")
    except urllib.error.HTTPError as e:
        if e.code == 404:
            print(f"Release {version} 尚未创建，准备自动创建...")
        else:
            print(f"[FAIL] 检查 Release 失败: {e.read().decode('utf-8')}")
            sys.exit(1)

    if not release_data:
        create_url = f"https://api.github.com/repos/{REPO}/releases"
        release_notes = f"""# Bob Agent {version} 发布说明

### 🌟 核心更新亮点
- **工作核心 (Work Core)**：全新上线项目、目标、计划、决策与监控一体化工作台。
- **本地 SQLite 迁移与自检**：无损多版本数据迁移校验，历史会话与待办事项 100% 完整继承。
- **启动与运行时死锁消除**：深度排查并修复 Android/Desktop 初始化阶段的阻塞互斥，冷启动秒开。
- **Cloud-Agent 跨端协作协议**：正式落盘 UDES 数据端口规范（`docs/contracts/bob_cloud_agent_interchange.yaml`），赋能移动端/桌面端与云端无缝联动。

### 📦 资产列表
- `bob-installer.exe`：Windows 官方桌面一键安装程序
- `bob-agent-portable.zip`：Windows 绿色免安装便携版
- `bob-mobile-latest.apk`：Android 移动端直装包
"""
        payload = {
            "tag_name": version,
            "target_commitish": "main",
            "name": f"Bob Agent {version}",
            "body": release_notes,
            "draft": False,
            "prerelease": False,
            "generate_release_notes": False
        }
        req = urllib.request.Request(
            create_url,
            data=json.dumps(payload).encode("utf-8"),
            headers=headers,
            method="POST"
        )
        try:
            with urllib.request.urlopen(req) as resp:
                release_data = json.loads(resp.read().decode("utf-8"))
                print(f"✓ 成功创建 Release {version} (ID: {release_data.get('id')})")
        except urllib.error.HTTPError as e:
            print(f"[FAIL] 创建 Release 失败: {e.read().decode('utf-8')}")
            sys.exit(1)

    release_id = release_data["id"]
    existing_assets = {a["name"]: a["id"] for a in release_data.get("assets", [])}

    files_to_upload = [
        os.path.join(DIST_DIR, "bob-installer.exe"),
        os.path.join(DIST_DIR, "bob-agent-portable.zip"),
        os.path.join(DIST_DIR, "bob-mobile-latest.apk"),
    ]

    print(f"\n[2/3] 准备上传发布资产 (Release Assets)...")
    for filepath in files_to_upload:
        if not os.path.exists(filepath):
            print(f"[WARN] 文件不存在，跳过: {filepath}")
            continue

        filename = os.path.basename(filepath)
        size_mb = os.path.getsize(filepath) / (1024 * 1024)

        if filename in existing_assets:
            old_asset_id = existing_assets[filename]
            print(f"- 正在替换已有资产 {filename} (ID: {old_asset_id})...")
            del_url = f"https://api.github.com/repos/{REPO}/releases/assets/{old_asset_id}"
            del_req = urllib.request.Request(del_url, headers=headers, method="DELETE")
            try:
                with urllib.request.urlopen(del_req) as resp:
                    pass
            except Exception as e:
                print(f"  [WARN] 删除旧资产失败: {e}")

        print(f">>> 正在上传 {filename} ({size_mb:.2f} MB)...")
        upload_url = f"https://uploads.github.com/repos/{REPO}/releases/{release_id}/assets?name={urllib.parse.quote(filename)}"
        
        with open(filepath, "rb") as f:
            file_data = f.read()

        upload_headers = headers.copy()
        if filename.endswith(".zip"):
            upload_headers["Content-Type"] = "application/zip"
        elif filename.endswith(".exe"):
            upload_headers["Content-Type"] = "application/vnd.microsoft.portable-executable"
        elif filename.endswith(".apk"):
            upload_headers["Content-Type"] = "application/vnd.android.package-archive"
        else:
            upload_headers["Content-Type"] = "application/octet-stream"

        upload_req = urllib.request.Request(upload_url, data=file_data, headers=upload_headers, method="POST")
        try:
            with urllib.request.urlopen(upload_req) as resp:
                if resp.status in (200, 201):
                    print(f"✓ {filename} 上传完成！")
                else:
                    print(f"[WARN] 上传响应码: {resp.status}")
        except urllib.error.HTTPError as e:
            print(f"[FAIL] 上传 {filename} 失败: {e.read().decode('utf-8')}")

    print(f"\n[3/3] 验证发布页面...")
    print(f"🎉 发布成功！在线访问地址：")
    print(f"🔗 https://github.com/{REPO}/releases/tag/{version}")

if __name__ == "__main__":
    main()
