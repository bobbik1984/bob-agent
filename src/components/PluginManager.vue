<template>
  <div v-if="isOpen" class="modal-overlay" @click.self="close">
    <div class="pm-modal">
      <div class="pm-header">
        <h2>{{ $t('plugin.title') }}</h2>
        <button class="pm-close" @click="close"><X :size="16" /></button>
      </div>

      <div class="pm-body">
        <div v-if="loading" class="pm-loading">
          <Loader2 class="pm-spinner" :size="18" />
          <span>{{ $t('plugin.loading') }}</span>
        </div>

        <template v-else>
          <div v-for="group in pluginGroups" :key="group.title" class="pm-group">
            <div class="pm-group-header">
              <h3>{{ group.title }}</h3>
              <span class="pm-group-count">{{ group.items.length }}</span>
            </div>
            
            <div
              v-for="plugin in group.items"
              :key="plugin.id"
              class="pm-row"
              :class="{ expanded: expandedId === plugin.id }"
              @click="expandedId = expandedId === plugin.id ? null : plugin.id"
            >
              <!-- 主行：名称 + 类型 + 状态 -->
              <div class="pm-row-main">
                <span class="pm-name">{{ plugin.name }}</span>
                <span class="pm-type">{{ plugin.typeLabel }}</span>
                <span class="pm-spacer"></span>
                
                <!-- 安装按钮 / 状态 -->
                <button
                  v-if="plugin.type === 'engine' && !plugin.installed && !installing[plugin.id]"
                  class="pm-install-btn"
                  @click.stop="installPlugin(plugin.id)"
                >{{ $t('plugin.install') }}</button>
                <span v-else-if="installing[plugin.id]" class="pm-status installing">
                  <Loader2 class="pm-spinner" :size="12" /> {{ $t('plugin.installing') }}
                </span>
                <span v-else class="pm-status ready">{{ $t('plugin.ready') }}</span>
                
                <ChevronRight :size="14" class="pm-chevron" />
              </div>

              <!-- 展开详情 -->
              <div v-if="expandedId === plugin.id" class="pm-detail" @click.stop>
                <p>{{ plugin.description }}</p>
                <div v-if="plugin.dependencies && plugin.dependencies.length" class="pm-deps">
                  <span class="pm-dep" v-for="dep in plugin.dependencies" :key="dep">{{ dep }}</span>
                </div>
                <div v-if="installing[plugin.id] && progressLogs[plugin.id]" class="pm-terminal">
                  <code>{{ progressLogs[plugin.id] }}</code>
                </div>
              </div>
            </div>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, watch, onMounted, onUnmounted } from 'vue';
import { X, Loader2, ChevronRight } from 'lucide-vue-next';

const { t } = useI18n();

const props = defineProps({ isOpen: Boolean });
const emit = defineEmits(['close']);

const plugins = ref([]);
const loading = ref(true);
const installing = ref({});
const progressLogs = ref({});
const expandedId = ref(null);
let removeListener = null;
let removeUpdatedListener = null;

const pluginGroups = computed(() => {
  const groups = [
    { title: '下载引擎 (Engines)', items: plugins.value.filter(p => p.type === 'engine') },
    { title: '内置能力 (Native Tools)', items: plugins.value.filter(p => p.type === 'tool') },
    { title: '外部认知技能 (External Skills)', items: plugins.value.filter(p => p.type === 'skill') }
  ];
  return groups.filter(g => g.items.length > 0);
});

const close = () => emit('close');

const fetchPlugins = async () => {
  loading.value = true;
  try {
    plugins.value = await window.electronAPI.getPlugins();
  } catch (err) {
    console.error("Failed to load plugins:", err);
  } finally {
    loading.value = false;
  }
};

const installPlugin = async (id) => {
  installing.value[id] = true;
  progressLogs.value[id] = '';
  expandedId.value = id;
  try {
    await window.electronAPI.installPlugin(id);
    await fetchPlugins();
  } catch (err) {
    progressLogs.value[id] += '\n安装失败: ' + err.message;
  } finally {
    installing.value[id] = false;
  }
};

watch(() => props.isOpen, (v) => { if (v) fetchPlugins(); }, { immediate: true });

onMounted(() => {
  removeListener = window.electronAPI.onPluginProgress(({ id, msg }) => {
    if (!progressLogs.value[id]) progressLogs.value[id] = '';
    progressLogs.value[id] += msg;
    const lines = progressLogs.value[id].split('\n');
    if (lines.length > 8) progressLogs.value[id] = lines.slice(-8).join('\n');
  });
  
  if (window.electronAPI.onPluginUpdated) {
    removeUpdatedListener = window.electronAPI.onPluginUpdated((updatedPlugins) => {
      plugins.value = updatedPlugins;
    });
  }
});

onUnmounted(() => { 
  if (removeListener) removeListener(); 
  if (removeUpdatedListener) removeUpdatedListener();
});
</script>

<style scoped>
/* ── Modal Shell ─────────────────────────────────── */
.modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  backdrop-filter: blur(4px);
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
}

.pm-modal {
  width: 520px;
  max-width: 90vw;
  max-height: 75vh;
  background: var(--bg-primary);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-lg);
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.6);
  display: flex;
  flex-direction: column;
}

/* ── Header ──────────────────────────────────────── */
.pm-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid var(--border-subtle);
}

.pm-header h2 {
  margin: 0;
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--text-primary);
}

.pm-close {
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  padding: 4px;
  border-radius: var(--radius-sm);
  display: flex;
}
.pm-close:hover { color: var(--text-primary); background: var(--surface-glass); }

/* ── Body ────────────────────────────────────────── */
.pm-body {
  overflow-y: auto;
  padding: 4px 0;
}

.pm-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 32px;
  color: var(--text-tertiary);
  font-size: var(--text-sm);
}

.pm-spinner { animation: spin 1s linear infinite; }
@keyframes spin { 100% { transform: rotate(360deg); } }

/* ── Grouping ────────────────────────────────────── */
.pm-group {
  margin-bottom: 12px;
}
.pm-group:last-child {
  margin-bottom: 0;
}

.pm-group-header {
  padding: 8px 16px 4px 16px;
  display: flex;
  align-items: center;
  gap: 8px;
  position: sticky;
  top: 0;
  background: var(--bg-primary);
  z-index: 10;
}

.pm-group-header h3 {
  margin: 0;
  font-size: 11px;
  color: var(--text-tertiary);
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.pm-group-count {
  font-size: 10px;
  background: var(--surface-card);
  color: var(--text-tertiary);
  padding: 1px 6px;
  border-radius: var(--radius-full);
}

/* ── Row ─────────────────────────────────────────── */
.pm-row {
  cursor: pointer;
  border-bottom: 1px solid var(--border-subtle);
  transition: background var(--duration-fast);
}
.pm-row:last-child { border-bottom: none; }
.pm-row:hover { background: var(--surface-glass); }

.pm-row-main {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 16px;
  min-height: 36px;
}

.pm-name {
  font-size: var(--text-sm);
  color: var(--text-primary);
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 220px;
}

.pm-type {
  font-size: 10px;
  padding: 1px 5px;
  border-radius: 3px;
  font-weight: 500;
  background: var(--surface-card);
  color: var(--text-tertiary);
  border: 1px solid var(--border-subtle);
  flex-shrink: 0;
}

.pm-spacer { flex: 1; }

.pm-status {
  font-size: 11px;
  flex-shrink: 0;
}
.pm-status.ready { color: var(--text-tertiary); }
.pm-status.installing {
  color: var(--text-secondary);
  display: flex;
  align-items: center;
  gap: 4px;
}

.pm-install-btn {
  font-size: 11px;
  padding: 2px 10px;
  border-radius: var(--radius-sm);
  border: none;
  cursor: pointer;
  font-weight: 600;
  font-family: var(--font-sans);
  /* 高对比度：纯白底黑字，和 btn-primary 保持一致 */
  background: var(--text-primary);
  color: var(--bg-root);
  transition: all var(--duration-fast);
}
.pm-install-btn:hover {
  background: #fff;
}

.pm-chevron {
  color: var(--text-tertiary);
  flex-shrink: 0;
  transition: transform var(--duration-fast);
}
.pm-row.expanded .pm-chevron {
  transform: rotate(90deg);
}

/* ── Detail Panel ────────────────────────────────── */
.pm-detail {
  padding: 0 16px 10px 16px;
  animation: fadeIn 150ms ease-out;
}

.pm-detail p {
  margin: 0 0 6px 0;
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.5;
}

.pm-deps {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-bottom: 6px;
}

.pm-dep {
  font-size: 10px;
  font-family: var(--font-mono);
  padding: 1px 5px;
  border-radius: 3px;
  background: var(--bg-tertiary);
  color: var(--text-tertiary);
  border: 1px solid var(--border-subtle);
}

.pm-terminal {
  background: var(--bg-root);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-sm);
  padding: 6px 8px;
  max-height: 80px;
  overflow-y: auto;
}

.pm-terminal code {
  font-family: var(--font-mono);
  font-size: 10px;
  color: var(--text-secondary);
  white-space: pre-wrap;
  line-height: 1.4;
}
</style>
