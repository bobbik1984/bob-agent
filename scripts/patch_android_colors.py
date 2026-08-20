import os
import re

def patch_resources():
    res_dir = "src-tauri/gen/android/app/src/main/res"
    if not os.path.exists(res_dir):
        print(f"Android resource directory not found at: {res_dir}")
        return

    print(f"Scanning and patching XML files in: {res_dir}")
    for root, dirs, files in os.walk(res_dir):
        for file in files:
            if file.endswith(".xml"):
                path = os.path.join(root, file)
                try:
                    with open(path, "r", encoding="utf-8") as f:
                        content = f.read()

                    # Replace default Tauri purple hex colors
                    new_content = content
                    new_content = new_content.replace("#FF6200EE", "#FFFFFFFF")
                    new_content = new_content.replace("#6200EE", "#FFFFFF")
                    new_content = new_content.replace("#FF3700B3", "#FFFFFFFF")
                    new_content = new_content.replace("#3700B3", "#FFFFFF")

                    # Force ic_launcher_background to be white
                    new_content = re.sub(
                        r'name="ic_launcher_background">[^<]+</color>',
                        'name="ic_launcher_background">#FFFFFF</color>',
                        new_content
                    )

                    if new_content != content:
                        with open(path, "w", encoding="utf-8") as f:
                            f.write(new_content)
                        print(f"Successfully patched color in: {path}")
                except Exception as e:
                    print(f"Error patching {path}: {e}")

def patch_gradle_sdk_versions():
    gen_dir = "src-tauri/gen/android"
    if not os.path.exists(gen_dir):
        print(f"Android gen directory not found at: {gen_dir}")
        return

    print(f"Scanning and patching Gradle files in: {gen_dir}")
    for root, dirs, files in os.walk(gen_dir):
        for file in files:
            if file.endswith((".gradle", ".gradle.kts")):
                path = os.path.join(root, file)
                try:
                    with open(path, "r", encoding="utf-8") as f:
                        content = f.read()

                    new_content = content
                    # Downgrade compileSdk and targetSdk from 36 to stable 35
                    new_content = re.sub(r'\bcompileSdk\s*=\s*36\b', 'compileSdk = 35', new_content)
                    new_content = re.sub(r'\btargetSdk\s*=\s*36\b', 'targetSdk = 35', new_content)
                    new_content = re.sub(r'\bcompileSdkVersion\s*\(\s*36\s*\)', 'compileSdkVersion(35)', new_content)
                    new_content = re.sub(r'\btargetSdkVersion\s*\(\s*36\s*\)', 'targetSdkVersion(35)', new_content)

                    if new_content != content:
                        with open(path, "w", encoding="utf-8") as f:
                            f.write(new_content)
                        print(f"Successfully patched compileSdk/targetSdk to 35 in Gradle file: {path}")
                except Exception as e:
                    print(f"Error patching Gradle file {path}: {e}")

def main():
    patch_resources()
    patch_gradle_sdk_versions()

if __name__ == "__main__":
    main()
