"""Source-staging checks only: these are not an Android build or camera test."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("scanner_stage", Path(__file__).with_name("stage-android-scanner.py"))
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ScannerStageTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bob-scanner-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.generated = self.root / "src-tauri/gen/android/app/src/main"
        self.gradle = self.root / "src-tauri/gen/android/app/build.gradle.kts"
        self.generated.mkdir(parents=True)
        self.gradle.write_text("plugins {}\n", encoding="utf-8")
        (self.generated / "AndroidManifest.xml").write_text("<manifest><application /></manifest>", encoding="utf-8")
        source = self.root / "src-tauri/android/PairingScannerPlugin.kt"
        source.parent.mkdir(parents=True)
        source.write_bytes((Path(__file__).resolve().parents[1] / "src-tauri/android/PairingScannerPlugin.kt").read_bytes())

    def test_staging_is_exact_and_idempotent(self):
        MODULE.stage(self.root)
        before = self.gradle.read_bytes()
        manifest = (self.generated / "AndroidManifest.xml").read_bytes()
        MODULE.stage(self.root)
        self.assertEqual(before, self.gradle.read_bytes())
        self.assertEqual(manifest, (self.generated / "AndroidManifest.xml").read_bytes())
        self.assertEqual(self.gradle.read_text(encoding="utf-8").count(MODULE.DEPENDENCY), 1)
        self.assertTrue((self.generated / "java/org/bobagent/scanner/PairingScannerPlugin.kt").is_file())
        self.assertTrue((self.generated / "res/values/bob_pairing_scanner.xml").is_file())
        self.assertTrue((self.generated / "res/values-zh-rCN/bob_pairing_scanner.xml").is_file())

    def test_unmanaged_dependency_is_not_overwritten(self):
        text = "dependencies { " + MODULE.DEPENDENCY + " }"
        self.gradle.write_text(text, encoding="utf-8")
        with self.assertRaises(RuntimeError):
            MODULE.stage(self.root)
        self.assertEqual(self.gradle.read_text(encoding="utf-8"), text)

    def test_incomplete_generated_shell_is_rejected(self):
        self.gradle.unlink()
        with self.assertRaises(RuntimeError):
            MODULE.stage(self.root)

    def test_duplicate_managed_markers_are_rejected(self):
        self.gradle.write_text(MODULE.BEGIN + MODULE.BEGIN + MODULE.END, encoding="utf-8")
        with self.assertRaises(RuntimeError):
            MODULE.stage(self.root)


if __name__ == "__main__":
    unittest.main()
