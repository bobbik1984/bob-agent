"""Offline regression checks for resumable, hash-verified candidate downloads."""
import hashlib
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import urllib.error
import zipfile

import promote_candidate_release as release


class DownloadTests(unittest.TestCase):
    def test_resume_validated_ranges_and_reject_corruption(self):
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, "w") as archive:
            archive.writestr("sample.txt", b"sample" * 100)
        body = buffer.getvalue()
        artifact = {"archive_download_url": "https://api.example.invalid/artifact",
                    "digest": "sha256:" + hashlib.sha256(body).hexdigest(),
                    "size_in_bytes": len(body), "name": "fixture"}
        class Redirect:
            def open(self, *args, **kwargs):
                raise urllib.error.HTTPError("fixture", 302, "redirect", {"Location": "https://download.example.invalid/file"}, None)
        class Response(io.BytesIO):
            status = 206
        def fetch(req, **kwargs):
            self.assertIsNone(req.get_header("Authorization"))
            start, end = map(int, req.get_header("Range").removeprefix("bytes=").split("-"))
            response = Response(body[start:end + 1])
            response.headers = {"Content-Range": f"bytes {start}-{end}/{len(body)}"}
            return response
        with tempfile.TemporaryDirectory() as directory, patch.object(release.urllib.request, "build_opener", return_value=Redirect()), patch.object(release.urllib.request, "urlopen", side_effect=fetch):
            target = Path(directory) / "artifact.zip"
            target.write_bytes(body[:30])
            release.download_artifact_zip(artifact, "not-a-real-token", target)
            self.assertEqual(target.read_bytes(), body)
            release.download_artifact_zip(artifact, "not-a-real-token", target)
            target.write_bytes(b"X" + body[1:])
            with self.assertRaisesRegex(RuntimeError, "digest mismatch"):
                release.download_artifact_zip(artifact, "not-a-real-token", target)

    def test_frozen_version_is_read_from_commit(self):
        self.assertIn('"version": "0.9.28"', release.source_text("1f23bb58f57bfb4a65082c51ad76db8f9e3541bb", "package.json"))


if __name__ == "__main__":
    unittest.main()
