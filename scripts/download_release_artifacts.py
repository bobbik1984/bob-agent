# -*- coding: utf-8 -*-
"""
Download v0.9.11 release assets from GitHub Releases directly to dist-release/
"""
import os
import sys
import json
import time
import subprocess
import urllib.request
import urllib.error

ROOT_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
DIST_DIR = os.path.join(ROOT_DIR, "dist-release")
VERSION = "0.9.12"

def get_token():
    try:
        p = subprocess.Popen(['git', 'credential', 'fill'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        out, _ = p.communicate('protocol=https\nhost=github.com\n\n')
        for line in out.splitlines():
            if line.startswith('password='):
                return line.split('=', 1)[1].strip()
    except Exception:
        pass
    return None

import zipfile

def verify_file(path):
    if not os.path.exists(path) or os.path.getsize(path) == 0:
        return False
    if path.endswith('.zip') or path.endswith('.apk'):
        return zipfile.is_zipfile(path)
    if path.endswith('.exe'):
        with open(path, 'rb') as f:
            return f.read(2) == b'MZ' and os.path.getsize(path) > 1024 * 1024
    return True

def download_file(url, out_path, max_retries=5):
    temp_path = out_path + ".tmp"
    for attempt in range(1, max_retries + 1):
        print(f"Downloading {url} -> {out_path} (attempt {attempt}/{max_retries})...")
        if os.path.exists(temp_path):
            try:
                os.remove(temp_path)
            except Exception:
                pass
        try:
            req = urllib.request.Request(url)
            req.add_header('User-Agent', 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36')
            with urllib.request.urlopen(req, timeout=30) as resp, open(temp_path, 'wb') as out_file:
                total_size = int(resp.headers.get('content-length', 0))
                downloaded = 0
                chunk_size = 1024 * 1024
                while True:
                    chunk = resp.read(chunk_size)
                    if not chunk:
                        break
                    out_file.write(chunk)
                    downloaded += len(chunk)
                    if total_size > 0:
                        percent = (downloaded / total_size) * 100
                        sys.stdout.write(f"\r  {downloaded / (1024*1024):.1f}MB / {total_size / (1024*1024):.1f}MB ({percent:.1f}%)")
                        sys.stdout.flush()

            print()
            if total_size > 0 and downloaded < total_size:
                print(f"  Warning: Incomplete download ({downloaded}/{total_size}), retrying...")
                time.sleep(2)
                continue

            if not verify_file(temp_path):
                print(f"  Warning: File verification failed for {temp_path}, retrying...")
                time.sleep(2)
                continue

            if os.path.exists(out_path):
                os.remove(out_path)
            os.replace(temp_path, out_path)
            print(f"  Download verified and completed: {out_path} ({os.path.getsize(out_path)} bytes)")
            return True
        except Exception as e:
            print(f"\n  Attempt {attempt} failed: {e}")
            time.sleep(2)

    return False

def check_and_download():
    token = get_token()
    headers = {'User-Agent': 'BobAgent'}
    if token:
        headers['Authorization'] = f'Bearer {token}'

    # Check releases
    desktop_done = False
    mobile_done = False

    desktop_installer = os.path.join(DIST_DIR, f"bob-v{VERSION}-installer.exe")
    desktop_portable = os.path.join(DIST_DIR, f"bob-v{VERSION}-portable.zip")
    mobile_apk = os.path.join(DIST_DIR, f"bob-v{VERSION}-signed.apk")

    req = urllib.request.Request("https://api.github.com/repos/bobbik1984/bob-agent/releases", headers=headers)
    try:
        with urllib.request.urlopen(req) as resp:
            releases = json.loads(resp.read().decode('utf-8'))
            for rel in releases:
                tag = rel.get('tag_name')
                name = rel.get('name', '')
                assets = rel.get('assets', [])
                if tag == 'latest-desktop' and f"v{VERSION}" in name:
                    for a in assets:
                        aname = a['name']
                        dl_url = a.get('browser_download_url') or a['url']
                        if aname == f"bob-v{VERSION}-installer.exe" and not verify_file(desktop_installer):
                            download_file(dl_url, desktop_installer)
                        elif aname == f"bob-v{VERSION}-portable.zip" and not verify_file(desktop_portable):
                            download_file(dl_url, desktop_portable)
                    if verify_file(desktop_installer) and verify_file(desktop_portable):
                        desktop_done = True
                elif tag == 'latest-mobile' and f"v{VERSION}" in name:
                    for a in assets:
                        aname = a['name']
                        dl_url = a.get('browser_download_url') or a['url']
                        if aname == f"bob-v{VERSION}-signed.apk" and not verify_file(mobile_apk):
                            download_file(dl_url, mobile_apk)
                    if verify_file(mobile_apk):
                        mobile_done = True
    except Exception as e:
        print(f"Error checking releases: {e}")

    return desktop_done, mobile_done

if __name__ == '__main__':
    d_done, m_done = check_and_download()
    print(f"Desktop done: {d_done}, Mobile done: {m_done}")
