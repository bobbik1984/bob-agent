"""Stage the offline camera scanner in generated Android sources, never build locally."""
from pathlib import Path
import shutil
import xml.etree.ElementTree as ET

DEPENDENCY = 'implementation("com.journeyapps:zxing-android-embedded:4.3.0")'
BEGIN = "// BOB_PAIRING_SCANNER_BEGIN"
END = "// BOB_PAIRING_SCANNER_END"


def stage(app_root: Path) -> None:
    android = app_root / "src-tauri/gen/android"
    generated = android / "app/src/main"
    gradle = android / "app/build.gradle.kts"
    manifest = generated / "AndroidManifest.xml"
    if not generated.is_dir() or not gradle.is_file() or not manifest.is_file():
        raise RuntimeError("Run Tauri Android init before staging the scanner")
    source = app_root / "src-tauri/android/PairingScannerPlugin.kt"
    destination = generated / "java/org/bobagent/scanner/PairingScannerPlugin.kt"
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    if source.read_bytes() != destination.read_bytes():
        raise RuntimeError("Android scanner source copy verification failed")
    content = gradle.read_text(encoding="utf-8")
    block = f'{BEGIN}\ndependencies {{\n    {DEPENDENCY}\n}}\n{END}'
    if BEGIN in content or END in content:
        if content.count(BEGIN) != 1 or content.count(END) != 1:
            raise RuntimeError("Ambiguous scanner dependency markers")
        start, end = content.index(BEGIN), content.index(END) + len(END)
        if start >= end:
            raise RuntimeError("Invalid scanner dependency markers")
        content = content[:start] + block + content[end:]
    elif "com.journeyapps:zxing-android-embedded" in content:
        raise RuntimeError("Scanner dependency already exists outside managed block")
    else:
        content = content.rstrip() + "\n\n" + block + "\n"
    gradle.write_text(content, encoding="utf-8")
    android_ns = "http://schemas.android.com/apk/res/android"
    tools_ns = "http://schemas.android.com/tools"
    ET.register_namespace("android", android_ns)
    ET.register_namespace("tools", tools_ns)
    document = ET.fromstring(manifest.read_text(encoding="utf-8"))
    application = document.find("application")
    if application is None:
        raise RuntimeError("Generated Android manifest has no application")
    capture_name = "com.journeyapps.barcodescanner.CaptureActivity"
    captures = [a for a in application.findall("activity") if a.get(f"{{{android_ns}}}name") == capture_name]
    if len(captures) > 1:
        raise RuntimeError("Ambiguous scanner Activity declarations")
    capture = captures[0] if captures else ET.SubElement(application, "activity")
    capture.set(f"{{{android_ns}}}name", capture_name)
    capture.set(f"{{{android_ns}}}screenOrientation", "fullSensor")
    capture.set(f"{{{tools_ns}}}replace", "android:screenOrientation")
    manifest.write_text(ET.tostring(document, encoding="unicode"), encoding="utf-8")
    for locale, prompt in [("values", "Scan the pairing code shown on your Home"), ("values-zh-rCN", "扫描 Home 上显示的配对二维码")]:
        resource = generated / "res" / locale / "bob_pairing_scanner.xml"
        resource.parent.mkdir(parents=True, exist_ok=True)
        root = ET.Element("resources")
        ET.SubElement(root, "string", {"name": "bob_pairing_scan_prompt"}).text = prompt
        resource.write_text(ET.tostring(root, encoding="unicode"), encoding="utf-8")
    # ZXing's merged manifest declares CAMERA; no startup permission request is added.


if __name__ == "__main__":
    stage(Path(__file__).resolve().parents[1])
    print("Android offline scanner sources, resources and pinned dependency staged")
