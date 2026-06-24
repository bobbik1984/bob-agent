import re
with open('src/views/ChatView.vue', 'r', encoding='utf-8') as f:
    content = f.read()

content = content.replace(
    'cdnUpload.value = { active: true, fileName: e.payload.file_name, percent: 0, bytesSent: 0, totalBytes: e.payload.total_bytes };',
    'cdnUpload.value = { active: true, fileName: e.payload.file_name, percent: 0, bytesSent: 0, totalBytes: e.payload.total_bytes, attempt: 1 };'
)

content = content.replace(
    '''      cdnUnlistens.push(await listen('cdn:upload-progress', (e) => {
        cdnUpload.value.percent = e.payload.percent;
        cdnUpload.value.bytesSent = e.payload.bytes_sent;
      }));''',
    '''      cdnUnlistens.push(await listen('cdn:upload-progress', (e) => {
        cdnUpload.value.percent = e.payload.percent;
        cdnUpload.value.bytesSent = e.payload.bytes_sent;
        if (e.payload.attempt) cdnUpload.value.attempt = e.payload.attempt;
      }));'''
)

content = content.replace(
    '''<span class="cdn-upload-name">{{ cdnUpload.fileName }}</span>''',
    '''<span class="cdn-upload-name">{{ cdnUpload.fileName }} <span v-if="cdnUpload.attempt && cdnUpload.attempt > 1" style="color: var(--user-accent); font-size: 0.9em;">(重试 {{ cdnUpload.attempt }}/3)</span></span>'''
)

with open('src/views/ChatView.vue', 'w', encoding='utf-8') as f:
    f.write(content)
