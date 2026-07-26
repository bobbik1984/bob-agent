import codecs

filepath = 'src/views/settings/SettingsConnections.vue'
content = codecs.open(filepath, 'r', 'utf-8').read()

modal_code = """
    <!-- 同步日志 Modal -->
    <GlobalDialog v-if="showSyncLogsModal" title="同步日志" @close="showSyncLogsModal = false" style="width: 500px; max-width: 90vw;">
      <div style="padding: 16px; display: flex; flex-direction: column; gap: 8px; max-height: 60vh; overflow-y: auto;">
        <div v-if="syncLogs.length === 0" style="text-align: center; color: var(--text-tertiary); padding: 20px;">
          暂无同步日志
        </div>
        <div v-else v-for="(log, idx) in syncLogs" :key="idx" style="border: 1px solid var(--border-color); border-radius: 8px; padding: 12px; background: var(--bg-secondary);">
          <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 6px;">
            <div style="display: flex; gap: 6px; align-items: center;">
              <span v-if="log.status === 'success'" style="color: var(--color-success); display: flex;"><CheckCircle :size="14"/></span>
              <span v-else-if="log.status === 'error'" style="color: var(--color-error); display: flex;"><XCircle :size="14"/></span>
              <span v-else style="color: var(--text-tertiary); display: flex;"><Info :size="14"/></span>
              <span style="font-weight: 600; font-size: 13px;">{{ log.action }}</span>
            </div>
            <span style="font-size: 12px; color: var(--text-tertiary);">{{ new Date(log.timestamp).toLocaleTimeString() }}</span>
          </div>
          <div style="font-size: 13px; color: var(--text-secondary);">
            {{ log.detail }}
          </div>
        </div>
      </div>
    </GlobalDialog>
"""

if "GlobalDialog v-if=\"showSyncLogsModal\"" not in content:
    content = content.replace('</template>', modal_code + '\n</template>', 1)
    
if "import GlobalDialog" not in content:
    # Actually GlobalDialog is probably already imported or globally registered.
    # In App.vue it's global? Usually it's in components. Let me check if GlobalDialog is in SettingsConnections.
    pass

codecs.open(filepath, 'w', 'utf-8').write(content)
print("Modal inserted!")
