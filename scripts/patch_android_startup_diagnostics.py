"""Enable WebView debugging only in the isolated Android startup diagnostic CI build."""

from pathlib import Path
import re


ROOT = Path(__file__).resolve().parents[1]
JAVA_ROOT = ROOT / "src-tauri" / "gen" / "android" / "app" / "src" / "main" / "java"
MARKER = "native-activity-oncreate"
METHOD = """    override fun onCreate(savedInstanceState: android.os.Bundle?) {
        android.webkit.WebView.setWebContentsDebuggingEnabled(true)
        android.util.Log.i("BOB_BOOT_DIAG", "native-activity-oncreate")
        super.onCreate(savedInstanceState)
    }
"""


def patch_main_activity(source: str) -> str:
    if MARKER in source:
        return source
    existing_on_create = re.search(
        r"override\s+fun\s+onCreate\s*\([^)]*\)\s*\{", source
    )
    if existing_on_create is not None:
        diagnostic_lines = (
            "\n        android.webkit.WebView.setWebContentsDebuggingEnabled(true)"
            '\n        android.util.Log.i("BOB_BOOT_DIAG", "native-activity-oncreate")'
        )
        return (
            source[: existing_on_create.end()]
            + diagnostic_lines
            + source[existing_on_create.end() :]
        )
    pattern = r"class\s+MainActivity\s*:\s*TauriActivity\(\)\s*(\{)?"
    match = re.search(pattern, source)
    if match is None:
        raise ValueError("Generated MainActivity does not extend TauriActivity")
    if match.group(1):
        return source[: match.end()] + "\n" + METHOD + source[match.end() :]
    return source[: match.end()] + " {\n" + METHOD + "}\n" + source[match.end() :]


def main() -> None:
    candidates = list(JAVA_ROOT.rglob("MainActivity.kt"))
    if len(candidates) != 1:
        raise RuntimeError(f"Expected exactly one generated MainActivity.kt; found {len(candidates)}")
    target = candidates[0]
    original = target.read_text(encoding="utf-8")
    patched = patch_main_activity(original)
    target.write_text(patched, encoding="utf-8")
    print(f"BOB_BOOT_DIAG patched {target.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
