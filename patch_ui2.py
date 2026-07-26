import codecs

c = codecs.open('src/views/settings/SettingsConnections.vue', 'r', 'utf-8').read()

header_target = '''          <div style="display: flex; align-items: center; gap: 8px;">
            <button 
              v-if="connectedDevices.length > 0"'''
              
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

c = c.replace(header_target, header_replace)

old_btn = '''<button class="btn btn-secondary-outline btn-sm" style="padding: 5px 8px; height: 28px; flex-shrink: 0;" @click="openSyncLogs" title="查看同步日志">
              <Info :size="13" /> 日志
            </button>
            '''
c = c.replace(old_btn, '')

codecs.open('src/views/settings/SettingsConnections.vue', 'w', 'utf-8').write(c)
print('UI Patched!')
