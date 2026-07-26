import codecs
import re

c = codecs.open('src/views/settings/SettingsConnections.vue', 'r', 'utf-8').read()

header_replace = '''          <div style="display: flex; align-items: center; gap: 8px;">
            <button 
              class="device-indicator-btn"
              @click.stop="openSyncLogs" 
              title="查看同步日志"
            >
              <Info :size="12" />
            </button>
            <button 
              v-if="connectedDevices.length > 0"'''

# Using regex to account for whitespace
c = re.sub(r'<\s*div\s+style="display:\s*flex;\s*align-items:\s*center;\s*gap:\s*8px;"\s*>\s*<\s*button\s+v-if="connectedDevices\.length\s*>\s*0"', header_replace, c, count=1)


old_btn_regex = r'<\s*button[^>]*@click="openSyncLogs"[^>]*>[\s\S]*?<\s*/\s*button\s*>'
c = re.sub(old_btn_regex, '', c)


codecs.open('src/views/settings/SettingsConnections.vue', 'w', 'utf-8').write(c)
print('UI Patched!')
