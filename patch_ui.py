import codecs

filepath = 'src/views/settings/SettingsConnections.vue'
content = codecs.open(filepath, 'r', 'utf-8').read()

# 1. Add "日志" button
btn_code = """<button class="btn btn-secondary-outline btn-sm" style="padding: 5px 8px; height: 28px; flex-shrink: 0;" @click="openSyncLogs" title="查看同步日志">
              <Info :size="13" /> 日志
            </button>
            <button class="btn btn-danger-outline btn-sm" style="padding: 5px 8px; height: 28px; flex-shrink: 0;" @click="handleReset" :title="$t('settings.p2p_btn_destroy')">"""

if "openSyncLogs" not in content:
    content = content.replace("""<button class="btn btn-danger-outline btn-sm" style="padding: 5px 8px; height: 28px; flex-shrink: 0;" @click="handleReset" :title="$t('settings.p2p_btn_destroy')">""", btn_code)


# 2. Add Modal Dialog
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
            <span style="font-size: 12px; color: var(--text-tertiary);">{{ formatTime(log.timestamp) }}</span>
          </div>
          <div style="font-size: 13px; color: var(--text-secondary);">
            {{ log.detail }}
          </div>
        </div>
      </div>
    </GlobalDialog>
"""
if "showSyncLogsModal" not in content:
    # insert before <style scoped>
    content = content.replace('</div>\n</template>', modal_code + '\n  </div>\n</template>')


# 3. Add script logic
script_code = """
const showSyncLogsModal = ref(false);
const syncLogs = ref([]);

const openSyncLogs = async () => {
  try {
    syncLogs.value = await invoke('get_sync_logs');
  } catch (e) {
    console.error("Failed to load sync logs:", e);
  }
  showSyncLogsModal.value = true;
};
"""
if "const openSyncLogs = async ()" not in content:
    # insert after const showDevicesModal = ref(false);
    content = content.replace("const showDevicesModal = ref(false);", "const showDevicesModal = ref(false);\n" + script_code)

# 4. Import icons CheckCircle, XCircle, Info if missing
if "CheckCircle" not in content:
    content = content.replace("import { Trash2, Unlink", "import { Trash2, Unlink, CheckCircle, XCircle")

codecs.open(filepath, 'w', 'utf-8').write(content)
print("SettingsConnections.vue patched!")
