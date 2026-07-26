import codecs

filepath = 'src/views/settings/SettingsConnections.vue'
c = codecs.open(filepath, 'r', 'utf-8').read()
c = c.replace('showSyncLogsModal.value = true;', 'alert("Click registered! Logs: " + syncLogs.value.length); showSyncLogsModal.value = true;')
codecs.open(filepath, 'w', 'utf-8').write(c)
print('Alert added!')
