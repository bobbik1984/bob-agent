import re
with open('src/tauri-bridge.js', 'r', encoding='utf-8') as f:
    content = f.read()

content = content.replace(
    '''  isSetupComplete: () => invoke('system_is_setup_complete'),''',
    '''  openExternal: (url) => invoke('plugin:shell|open', { path: url }),
  isSetupComplete: () => invoke('system_is_setup_complete'),'''
)

with open('src/tauri-bridge.js', 'w', encoding='utf-8') as f:
    f.write(content)
