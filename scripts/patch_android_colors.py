import os
import re

def main():
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

if __name__ == "__main__":
    main()
