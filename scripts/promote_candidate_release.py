"""Promote same-commit Windows and Android CI artifacts to a GitHub prerelease.

This never updates the website or stable download aliases. It refuses to replace
an existing tag, Release, artifact, or local candidate file.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile


ROOT = Path(__file__).resolve().parents[1]
REPO = "bobbik1984/bob-agent"
API = f"https://api.github.com/repos/{REPO}"
EXPECTED = {
    "windows.yml": ("installer.exe", "portable.zip"),
    "android.yml": ("signed.apk",),
}


def credential():
    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    if token:
        return token.strip()
    result = subprocess.run(
        ["git", "credential", "fill"],
        input="protocol=https\nhost=github.com\n\n",
        capture_output=True,
        encoding="utf-8",
        timeout=20,
        check=False,
    )
    for line in result.stdout.splitlines():
        if line.startswith("password="):
            return line.partition("=")[2].strip()
    raise RuntimeError("GitHub credential unavailable; no Release was created")


def request(url, token, method="GET", payload=None, content_type="application/vnd.github+json"):
    data = None if payload is None else json.dumps(payload, ensure_ascii=False).encode("utf-8")
    headers = {
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {token}",
        "User-Agent": "Bob-Candidate-Promoter",
        "X-GitHub-Api-Version": "2022-11-28",
    }
    if data is not None:
        headers["Content-Type"] = content_type
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=60) as response:
            body = response.read()
            return json.loads(body.decode("utf-8")) if body else {}
    except urllib.error.HTTPError as error:
        detail = error.read().decode("utf-8", "replace")[:500]
        raise RuntimeError(f"GitHub API {method} failed ({error.code}): {detail}") from error


def get_optional(url, token):
    try:
        return request(url, token)
    except RuntimeError as error:
        if "failed (404)" in str(error):
            return None
        raise


def find_successful_run(workflow, sha, token, deadline):
    encoded = urllib.parse.urlencode({"branch": "mobile-pc-rebuild", "event": "push", "per_page": 50})
    url = f"{API}/actions/workflows/{workflow}/runs?{encoded}"
    while time.monotonic() < deadline:
        runs = request(url, token).get("workflow_runs", [])
        matches = [run for run in runs if run.get("head_sha") == sha]
        if matches:
            run = max(matches, key=lambda item: item["id"])
            if run.get("status") == "completed":
                if run.get("conclusion") != "success":
                    raise RuntimeError(f"{workflow} run {run['id']} ended: {run.get('conclusion')}")
                print(f"{workflow}: success, run={run['id']}", flush=True)
                return run
            print(f"{workflow}: {run.get('status')} (run={run['id']})", flush=True)
        else:
            print(f"{workflow}: waiting for push run on {sha[:12]}", flush=True)
        time.sleep(25)
    raise RuntimeError(f"Timed out waiting for {workflow} on {sha}")


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request_obj, fp, code, msg, headers, newurl):
        return None


def download_artifact_zip(artifact, token, destination):
    url = artifact["archive_download_url"]
    req = urllib.request.Request(url, headers={
        "Authorization": f"Bearer {token}", "Accept": "application/vnd.github+json",
        "User-Agent": "Bob-Candidate-Promoter",
    })
    opener = urllib.request.build_opener(NoRedirect)
    try:
        opener.open(req, timeout=60)
        raise RuntimeError("Artifact API did not redirect to a download URL")
    except urllib.error.HTTPError as error:
        if error.code not in (301, 302, 303, 307, 308):
            raise RuntimeError(f"Artifact download failed: HTTP {error.code}") from error
        download_url = error.headers.get("Location")
        if not download_url or not download_url.startswith("https://"):
            raise RuntimeError("Artifact redirect was missing or insecure")
    with urllib.request.urlopen(
        urllib.request.Request(download_url, headers={"User-Agent": "Bob-Candidate-Promoter"}),
        timeout=120,
    ) as response, destination.open("wb") as output:
        shutil.copyfileobj(response, output)
    digest = artifact.get("digest", "")
    if digest.startswith("sha256:") and file_sha256(destination) != digest.partition(":")[2]:
        raise RuntimeError(f"Artifact ZIP digest mismatch: {artifact['name']}")
    if not zipfile.is_zipfile(destination):
        raise RuntimeError(f"Artifact is not a ZIP: {artifact['name']}")


def file_sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def extract_expected(zip_path, expected_names, destination):
    with zipfile.ZipFile(zip_path) as archive:
        if archive.testzip() is not None:
            raise RuntimeError(f"Corrupt Artifact ZIP: {zip_path.name}")
        for name in expected_names:
            members = [member for member in archive.infolist() if Path(member.filename).name == name]
            if len(members) != 1:
                raise RuntimeError(f"Expected exactly one {name} in {zip_path.name}, found {len(members)}")
            target = destination / name
            if target.exists():
                raise RuntimeError(f"Refusing to overwrite existing candidate: {target}")
            with archive.open(members[0]) as source, target.open("xb") as output:
                shutil.copyfileobj(source, output)
            if target.stat().st_size < 1024 * 1024:
                raise RuntimeError(f"Unexpectedly small candidate: {name}")
            with target.open("rb") as source:
                magic = source.read(2)
            if name.endswith(".exe") and magic != b"MZ":
                raise RuntimeError(f"Invalid Windows executable: {name}")
            if name.endswith((".zip", ".apk")) and not zipfile.is_zipfile(target):
                raise RuntimeError(f"Invalid ZIP/APK: {name}")
            print(f"Verified {name}: {target.stat().st_size} bytes, sha256={file_sha256(target)}", flush=True)


def release_notes(version, sha):
    changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    heading = f"## [{version}]"
    if heading not in changelog:
        raise RuntimeError(f"Missing {heading} in CHANGELOG.md")
    section = changelog.split(heading, 1)[1].split("\n## [", 1)[0].strip()
    return f"# Bob Agent {version} 候选预发布\n\n代码提交：`{sha}`。\n\n{section}\n"


def upload_asset(release, path, token):
    url = release["upload_url"].split("{", 1)[0]
    url += "?" + urllib.parse.urlencode({"name": path.name})
    req = urllib.request.Request(url, data=path.read_bytes(), headers={
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/octet-stream",
        "User-Agent": "Bob-Candidate-Promoter",
        "X-GitHub-Api-Version": "2022-11-28",
    }, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=180) as response:
            uploaded = json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        raise RuntimeError(f"Upload {path.name} failed ({error.code})") from error
    if uploaded.get("name") != path.name or uploaded.get("size") != path.stat().st_size:
        raise RuntimeError(f"Uploaded asset differs from local file: {path.name}")
    print(f"Uploaded {path.name}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sha", required=True, help="Exact source commit from both CI runs")
    parser.add_argument("--timeout-minutes", type=int, default=60)
    args = parser.parse_args()
    sha = args.sha.lower()
    if len(sha) != 40 or any(char not in "0123456789abcdef" for char in sha):
        raise RuntimeError("--sha must be a full 40-character commit hash")
    version = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))["version"]
    tag = f"v{version}-rc.{sha[:7]}"
    token = credential()
    if get_optional(f"{API}/git/ref/tags/{urllib.parse.quote(tag)}", token):
        raise RuntimeError(f"Tag already exists; refusing to replace {tag}")
    if get_optional(f"{API}/releases/tags/{urllib.parse.quote(tag)}", token):
        raise RuntimeError(f"Release already exists; refusing to replace {tag}")
    deadline = time.monotonic() + args.timeout_minutes * 60
    runs = {workflow: find_successful_run(workflow, sha, token, deadline) for workflow in EXPECTED}
    destination = ROOT / "dist-release" / f"candidate-v{version}-{sha[:12]}"
    if destination.exists():
        raise RuntimeError(f"Refusing to overwrite existing candidate directory: {destination}")
    destination.mkdir(parents=True)
    expected_files = []
    for workflow, suffixes in EXPECTED.items():
        run = runs[workflow]
        artifacts = request(f"{API}/actions/runs/{run['id']}/artifacts?per_page=100", token)["artifacts"]
        prefix = "bob-desktop-windows" if workflow == "windows.yml" else "bob-mobile-apk"
        expected_artifact = f"{prefix}-v{version}-{sha}"
        matches = [artifact for artifact in artifacts if artifact["name"] == expected_artifact and not artifact["expired"]]
        if len(matches) != 1:
            raise RuntimeError(f"Expected one unexpired {expected_artifact}, found {len(matches)}")
        archive = destination / f"{prefix}.artifact.zip"
        download_artifact_zip(matches[0], token, archive)
        names = [f"bob-v{version}-{suffix}" for suffix in suffixes]
        extract_expected(archive, names, destination)
        expected_files.extend(destination / name for name in names)
        archive.unlink()
    checksum = destination / "SHA256SUMS.txt"
    checksum.write_text(
        "".join(f"{file_sha256(path)}  {path.name}\n" for path in expected_files),
        encoding="utf-8",
    )
    release = request(f"{API}/releases", token, "POST", {
        "tag_name": tag,
        "target_commitish": sha,
        "name": f"Bob Agent {version} candidate ({sha[:7]})",
        "body": release_notes(version, sha),
        "draft": True,
        "prerelease": True,
        "generate_release_notes": False,
    })
    print(f"Draft Release created: {tag}", flush=True)
    for path in [*expected_files, checksum]:
        upload_asset(release, path, token)
    verified = request(f"{API}/releases/{release['id']}", token)
    assets = {asset["name"]: asset["size"] for asset in verified.get("assets", [])}
    for path in [*expected_files, checksum]:
        if assets.get(path.name) != path.stat().st_size:
            raise RuntimeError(f"Draft Release asset verification failed: {path.name}")
    published = request(f"{API}/releases/{release['id']}", token, "PATCH", {
        "draft": False, "prerelease": True,
    })
    if published.get("draft") or not published.get("prerelease"):
        raise RuntimeError("Release publication state was not confirmed")
    print(f"Candidate prerelease: {published['html_url']}", flush=True)
    print(f"Local candidate: {destination}", flush=True)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"Candidate promotion stopped: {error}", file=sys.stderr)
        sys.exit(1)
