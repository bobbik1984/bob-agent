"""Copy reviewed native source into the generated Tauri shell; no SDK operations."""
from pathlib import Path
import shutil
import xml.etree.ElementTree as ET

REQUIRED_PERMISSIONS = (
    "android.permission.USE_BIOMETRIC",
    "android.permission.USE_FINGERPRINT",
)


def _ensure_manifest_permissions(manifest: Path) -> None:
    if not manifest.is_file():
        return
    android_ns = "http://schemas.android.com/apk/res/android"
    ET.register_namespace("android", android_ns)
    document = ET.fromstring(manifest.read_text(encoding="utf-8"))
    existing = {
        el.get(f"{{{android_ns}}}name")
        for el in document.findall("uses-permission")
    }
    changed = False
    for perm in REQUIRED_PERMISSIONS:
        if perm not in existing:
            el = ET.SubElement(document, "uses-permission")
            el.set(f"{{{android_ns}}}name", perm)
            existing.add(perm)
            changed = True
    if changed:
        manifest.write_text(ET.tostring(document, encoding="unicode"), encoding="utf-8")


def stage(app_root: Path) -> Path:
    source = app_root / "src-tauri/android/DeviceVaultPlugin.kt"
    generated = app_root / "src-tauri/gen/android/app/src/main"
    if not generated.is_dir():
        raise RuntimeError("Run Tauri Android init before staging the native vault")
    destination = generated / "java/org/bobagent/vault/DeviceVaultPlugin.kt"
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    if source.read_bytes() != destination.read_bytes():
        raise RuntimeError("Android vault source copy verification failed")
    _ensure_manifest_permissions(generated / "AndroidManifest.xml")
    return destination


if __name__ == "__main__":
    stage(Path(__file__).resolve().parents[1])
    print("Android native vault source staged and verified")
