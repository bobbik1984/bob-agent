import os

manifest_path = "src-tauri/gen/android/app/src/main/AndroidManifest.xml"

if not os.path.exists(manifest_path):
    print(f"Error: {manifest_path} not found.")
    exit(1)

with open(manifest_path, "r", encoding="utf-8") as f:
    content = f.read()

activity_xml = """
        <activity
            android:name="bob.agent.ShareActivity"
            android:exported="true"
            android:theme="@android:style/Theme.Translucent.NoTitleBar"
            android:excludeFromRecents="true">
            <intent-filter>
                <action android:name="android.intent.action.SEND" />
                <category android:name="android.intent.category.DEFAULT" />
                <data android:mimeType="text/plain" />
                <data android:mimeType="image/*" />
            </intent-filter>
        </activity>
"""

# Inject before the closing </application> tag
if "bob.agent.ShareActivity" not in content:
    content = content.replace("</application>", activity_xml + "</application>")
    with open(manifest_path, "w", encoding="utf-8") as f:
        f.write(content)
    print("Successfully injected ShareActivity into AndroidManifest.xml")
else:
    print("ShareActivity already exists in AndroidManifest.xml")
