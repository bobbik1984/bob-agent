import sys
with open('src/views/settings/SettingsConnections.vue', 'r', encoding='utf-8') as f:
    content = f.read()

with open('temp_patch.html', 'r', encoding='utf-8') as f:
    replacement = f.read()

import re
pattern = re.compile(r'<div class="service-cards-grid">\s*<!-- 🔄 多端同步 \(P2P Sync\) -->.*?</div>\s*</section>', re.DOTALL)
new_content = pattern.sub(replacement + '\n  </section>', content)

if new_content != content:
    with open('src/views/settings/SettingsConnections.vue', 'w', encoding='utf-8') as f:
        f.write(new_content)
    print('Patched successfully.')
else:
    print('Regex failed to match.')
