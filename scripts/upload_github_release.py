import os
import sys
import json
import urllib.request
import urllib.error

# Configuration
REPO = "bobbik1984/bob-agent"  # This should be the public repo
VERSION = "v0.6.0"
FILES_TO_UPLOAD = [
    r"dist-release\bob-installer.exe",
    r"dist-release\bob-agent-portable.zip",
    r"dist-release\bob_v0.6.0.apk"
]

def main():
    print(f"--- GitHub Release Uploader for {REPO} {VERSION} ---")
    token = input("Please enter your GitHub Personal Access Token (classic or fine-grained with repo access): ").strip()
    if not token:
        print("Error: Token cannot be empty.")
        sys.exit(1)

    headers = {
        "Accept": "application/vnd.github.v3+json",
        "Authorization": f"token {token}",
        "User-Agent": "BobAgent-Uploader"
    }

    # 1. Check if release already exists
    print(f"\n[1/3] Checking if release {VERSION} already exists...")
    check_url = f"https://api.github.com/repos/{REPO}/releases/tags/{VERSION}"
    req = urllib.request.Request(check_url, headers=headers)
    release_id = None
    
    try:
        with urllib.request.urlopen(req) as response:
            data = json.loads(response.read().decode())
            release_id = data.get("id")
            print(f"Release {VERSION} found (ID: {release_id}).")
    except urllib.error.HTTPError as e:
        if e.code == 404:
            print(f"Release {VERSION} not found. Creating it now...")
        else:
            print(f"Error checking release: {e.read().decode()}")
            sys.exit(1)

    # 2. Create release if not exists
    if not release_id:
        create_url = f"https://api.github.com/repos/{REPO}/releases"
        payload = {
            "tag_name": VERSION,
            "target_commitish": "main",
            "name": f"Bob Agent {VERSION}",
            "body": "Automated release via Bob AI Agent.",
            "draft": False,
            "prerelease": False,
            "generate_release_notes": True
        }
        req = urllib.request.Request(create_url, data=json.dumps(payload).encode(), headers=headers, method="POST")
        try:
            with urllib.request.urlopen(req) as response:
                data = json.loads(response.read().decode())
                release_id = data.get("id")
                print(f"Successfully created release {VERSION} (ID: {release_id}).")
        except urllib.error.HTTPError as e:
            print(f"Error creating release: {e.read().decode()}")
            sys.exit(1)

    # 3. Upload assets
    print("\n[3/3] Uploading assets...")
    for filepath in FILES_TO_UPLOAD:
        if not os.path.exists(filepath):
            print(f"Error: File {filepath} does not exist. Skipping.")
            continue
            
        filename = os.path.basename(filepath)
        print(f"Uploading {filename}...")
        
        # We need to construct the upload URL. 
        # GitHub upload URL template looks like: https://uploads.github.com/repos/.../releases/123/assets{?name,label}
        upload_url = f"https://uploads.github.com/repos/{REPO}/releases/{release_id}/assets?name={urllib.parse.quote(filename)}"
        
        with open(filepath, 'rb') as f:
            file_data = f.read()

        upload_headers = headers.copy()
        if filename.endswith(".zip"):
            upload_headers["Content-Type"] = "application/zip"
        elif filename.endswith(".exe"):
            upload_headers["Content-Type"] = "application/vnd.microsoft.portable-executable"
        else:
            upload_headers["Content-Type"] = "application/octet-stream"

        req = urllib.request.Request(upload_url, data=file_data, headers=upload_headers, method="POST")
        try:
            with urllib.request.urlopen(req) as response:
                if response.status == 201:
                    print(f"✓ {filename} uploaded successfully!")
                else:
                    print(f"Warning: Unexpected status code {response.status} when uploading {filename}.")
        except urllib.error.HTTPError as e:
            err_body = e.read().decode()
            if "already_exists" in err_body:
                print(f"- {filename} already exists in this release. Skipping.")
            else:
                print(f"Error uploading {filename}: {err_body}")

    print("\n🎉 All done! You can view your release at: https://github.com/bobbik1984/bob-agent/releases")

if __name__ == "__main__":
    main()
