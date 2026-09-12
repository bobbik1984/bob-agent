# -*- coding: utf-8 -*-
"""
Bob Agent GitHub Actions CI 监控探针
- 自动查询 Windows 桌面端与 Android 移动端最新构建进度
- 显示各编译步骤详细状态与执行耗时
- 支持轮询模式（--watch / -w）直到全部成功或失败
"""

import sys
import time
import json
import ssl
import urllib.request

import subprocess
import os

REPO = "bobbik1984/bob-agent"

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
        out, _ = p.communicate("protocol=https\nhost=github.com\n\n")
        for line in out.splitlines():
            if line.startswith("password="):
                tok = line.split("=", 1)[1].strip()
                if tok:
                    return tok
    except Exception:
        pass
    return None

HEADERS = {"User-Agent": "Bob-CI-Monitor"}
_token = get_github_token()
if _token:
    HEADERS["Authorization"] = f"token {_token}"


def get_latest_runs():
    ctx = ssl.create_default_context()
    url = f"https://api.github.com/repos/{REPO}/actions/runs?per_page=5"
    req = urllib.request.Request(url, headers=HEADERS)
    with urllib.request.urlopen(req, context=ctx) as resp:
        data = json.loads(resp.read().decode("utf-8"))
    
    # 提取最近的 Windows 和 Android 各一次构建
    runs = {}
    for r in data.get("workflow_runs", []):
        name = r.get("name")
        if name in ("Bob Desktop Windows CI", "Bob-Mobile Android CI") and name not in runs:
            runs[name] = r
    return runs

def inspect_run_steps(jobs_url):
    ctx = ssl.create_default_context()
    req = urllib.request.Request(jobs_url, headers=HEADERS)
    try:
        with urllib.request.urlopen(req, context=ctx) as resp:
            data = json.loads(resp.read().decode("utf-8"))
            steps_info = []
            for job in data.get("jobs", []):
                for s in job.get("steps", []):
                    if s.get("status") in ("in_progress", "completed"):
                        status_str = "DONE" if s.get("status") == "completed" else "RUNNING"
                        conc = f" ({s.get('conclusion')})" if s.get('conclusion') else ""
                        steps_info.append(f"  [{status_str}{conc}] {s['name']}")
            return steps_info
    except Exception as e:
        return [f"  [WARN] 获取步骤失败: {e}"]

def check_once():
    runs = get_latest_runs()
    all_done = True
    all_success = True
    
    for name, r in runs.items():
        status = r.get("status")
        conclusion = r.get("conclusion")
        print(f"\n==========================================")
        print(f"工作流: {name}")
        print(f"状态:   {status} (结论: {conclusion})")
        print(f"链接:   {r.get('html_url')}")
        print(f"==========================================")
        steps = inspect_run_steps(r.get("jobs_url"))
        for s in steps:
            print(s)
            
        if status != "completed":
            all_done = False
        if conclusion != "success":
            all_success = False
            
    return all_done, all_success

def main():
    watch = "--watch" in sys.argv or "-w" in sys.argv
    print("===========================================================")
    print("   Bob Agent 云端 CI 构建监控探针")
    print("===========================================================")
    
    if not watch:
        all_done, all_success = check_once()
        if all_done and all_success:
            print("\n✓ 双端构建已全部成功完成！")
            sys.exit(0)
        elif all_done and not all_success:
            print("\n❌ 构建已结束，但存在失败任务。")
            sys.exit(1)
        else:
            print("\n⚡ 云端任务仍在进行中...")
            sys.exit(2)
            
    print("正在持续监听云端构建状态（按 Ctrl+C 退出）...\n")
    while True:
        all_done, all_success = check_once()
        if all_done:
            if all_success:
                print("\n🎉 双端云端构建全部成功通过！")
                sys.exit(0)
            else:
                print("\n❌ 云端构建失败，请检查日志。")
                sys.exit(1)
        time.sleep(15)

if __name__ == "__main__":
    main()
