"""Copy reviewed native source into the generated Tauri shell; no SDK operations."""
from pathlib import Path
import shutil


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
    return destination


if __name__ == "__main__":
    stage(Path(__file__).resolve().parents[1])
    print("Android native vault source staged and verified")
