"""Contract tests for generated Android Activity instrumentation."""

import unittest

from scripts.patch_android_startup_diagnostics import MARKER, patch_main_activity


class PatchMainActivityTests(unittest.TestCase):
    def test_patches_existing_on_create_without_duplicate(self) -> None:
        source = (
            "package bob.agent\nclass MainActivity : TauriActivity() {\n"
            "    override fun onCreate(savedInstanceState: android.os.Bundle?) {\n"
            "        super.onCreate(savedInstanceState)\n    }\n}\n"
        )
        patched = patch_main_activity(source)
        self.assertIn(MARKER, patched)
        self.assertEqual(patched.count("override fun onCreate"), 1)
        self.assertEqual(patch_main_activity(patched), patched)

    def test_patches_activity_with_body_once(self) -> None:
        source = "package bob.agent\nclass MainActivity : TauriActivity() {\n}\n"
        patched = patch_main_activity(source)
        self.assertIn(MARKER, patched)
        self.assertIn("setWebContentsDebuggingEnabled(true)", patched)
        self.assertEqual(patch_main_activity(patched), patched)

    def test_patches_activity_without_body(self) -> None:
        source = "package bob.agent\nclass MainActivity : TauriActivity()\n"
        patched = patch_main_activity(source)
        self.assertIn(MARKER, patched)
        self.assertIn("super.onCreate(savedInstanceState)", patched)

    def test_rejects_unexpected_activity(self) -> None:
        with self.assertRaises(ValueError):
            patch_main_activity("class MainActivity : Activity() {}")


if __name__ == "__main__":
    unittest.main()
