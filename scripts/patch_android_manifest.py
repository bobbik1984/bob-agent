import os
import shutil

# Paths
base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
android_dir = os.path.join(base_dir, 'src-tauri', 'gen', 'android')
manifest_path = os.path.join(android_dir, 'app', 'src', 'main', 'AndroidManifest.xml')
main_activity_dir = os.path.join(android_dir, 'app', 'src', 'main', 'java', 'org', 'bobbik', 'bobagent')
main_activity_path = os.path.join(main_activity_dir, 'MainActivity.kt')
plugin_source = os.path.join(base_dir, 'scripts', 'SpeechRecognizerPlugin.kt')

# Ensure directories exist
if not os.path.exists(android_dir):
    print("Android directory not found. Have you run 'tauri android init'?")
    exit(1)

# 1. Patch AndroidManifest.xml
permissions = [
    '<uses-permission android:name="android.permission.RECORD_AUDIO" />',
    '<uses-permission android:name="android.permission.BLUETOOTH_CONNECT" />',
    '<uses-permission android:name="android.permission.VIBRATE" />',
    '<uses-permission android:name="android.permission.WAKE_LOCK" />'
]

print(f"Patching {manifest_path}...")
if os.path.exists(manifest_path):
    with open(manifest_path, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # Check if permissions already exist
    for perm in permissions:
        if perm not in content:
            # Insert before <application>
            content = content.replace('<application', perm + '\n    <application')
    
    with open(manifest_path, 'w', encoding='utf-8') as f:
        f.write(content)
    print("Manifest patched successfully.")
else:
    print(f"Error: Manifest not found at {manifest_path}")

# 2. Copy Plugin file
print(f"Copying {plugin_source} to {main_activity_dir}...")
if not os.path.exists(main_activity_dir):
    os.makedirs(main_activity_dir)
if os.path.exists(plugin_source):
    shutil.copy2(plugin_source, main_activity_dir)
    print("Plugin copied.")
else:
    print(f"Error: Plugin source not found at {plugin_source}")

# 3. Patch MainActivity.kt
print(f"Patching {main_activity_path}...")
if os.path.exists(main_activity_path):
    with open(main_activity_path, 'r', encoding='utf-8') as f:
        content = f.read()
    
    if "SpeechRecognizerPlugin" not in content:
        # Find the init { } block or create one
        if "init {" in content:
            content = content.replace("init {", "init {\n        registerPlugin(app.tauri.plugin.PluginManager(this).register(SpeechRecognizerPlugin::class.java))")
        else:
            # Replace class MainActivity : TauriActivity() { with init block
            replacement = """class MainActivity : TauriActivity() {
    init {
        app.tauri.plugin.PluginManager.getInstance().register(this, SpeechRecognizerPlugin::class.java)
    }"""
            content = content.replace("class MainActivity : TauriActivity() {", replacement)
            
        with open(main_activity_path, 'w', encoding='utf-8') as f:
            f.write(content)
        print("MainActivity patched.")
    else:
        print("MainActivity already patched.")
else:
    print(f"Error: MainActivity not found at {main_activity_path}. It might be generated later during build.")

print("Android Native patch completed successfully.")
