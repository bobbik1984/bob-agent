import re

with open('src/views/settings/SettingsConnections.vue', 'r', encoding='utf-8') as f:
    content = f.read()

new_steps = '''  pairingSteps.value = [
    { id: 'parse',         label: '二维码解码',         status: 'pending', detail: '' },
    { id: 'save_config',   label: '保存配对配置',       status: 'pending', detail: '' },
    { id: 'lan_sync',      label: '尝试局域网直连同步',   status: 'pending', detail: '' },
    { id: 'relay_handshake', label: '尝试外网隧道穿透',  status: 'pending', detail: '' },
    { id: 'relay_sync',    label: '外网隧道数据同步',     status: 'pending', detail: '' },
  ];'''

# Replace steps safely using regex
content = re.sub(r'pairingSteps\.value\s*=\s*\[(.*?)\];', new_steps, content, flags=re.DOTALL)

# Replace the try block for Steps 3, 4, 5
old_logic_pattern = r'// Step 3: Relay Handshake.*?finally \{\s*if \(unlistenProgress\) unlistenProgress\(\);\s*\}'

with open('patch.js', 'r', encoding='utf-8') as f:
    new_logic = f.read()

new_logic_full = new_logic + '\n    } catch (e) {\n      pairingDone.value = true;\n      pairingError.value = true;\n    } finally {\n      if (unlistenProgress) unlistenProgress();\n    }'

content = re.sub(old_logic_pattern, new_logic_full, content, flags=re.DOTALL)

with open('src/views/settings/SettingsConnections.vue', 'w', encoding='utf-8') as f:
    f.write(content)

print("Patched successfully")
