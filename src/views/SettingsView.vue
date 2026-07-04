<template>
  <div class="settings-view">
    <div class="settings-scroll">
      <div class="settings-content">
        <!-- 根据侧边栏设置导航选中项，渲染对应子面板 -->
        <SettingsModelPanel
          v-if="activePanel === 'model'"
          ref="modelPanelRef"
          :config="config"
          @config-changed="onConfigChanged"
        />
        <SettingsConnections
          v-else-if="activePanel === 'connections'"
          :config="config"
          @config-changed="onConfigChanged"
        />
        <SettingsWorkspace
          v-else-if="activePanel === 'workspace'"
          :config="config"
          @config-changed="onConfigChanged"
        />
        <SettingsAppearance
          v-else-if="activePanel === 'appearance'"
          :config="config"
          @config-changed="onConfigChanged"
        />
        <SettingsAbout
          v-else-if="activePanel === 'about'"
          :config="config"
          @config-changed="onConfigChanged"
        />
        <SettingsDailyRoutine
          v-else-if="activePanel === 'daily_routine'"
          :config="config"
          @config-changed="onConfigChanged"
        />
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, reactive, onMounted, watch } from 'vue';

import SettingsModelPanel from './settings/SettingsModelPanel.vue';
import SettingsConnections from './settings/SettingsConnections.vue';
import SettingsWorkspace from './settings/SettingsWorkspace.vue';
import SettingsAppearance from './settings/SettingsAppearance.vue';
import SettingsAbout from './settings/SettingsAbout.vue';
import SettingsDailyRoutine from './settings/SettingsDailyRoutine.vue';

const props = defineProps({
  activePanel: { type: String, default: 'model' }
});
const emit = defineEmits(['config-changed']);

// ── 共享配置状态 ────────────────────────────────────
const config = reactive({
  offlineModelPath: '',
  theme: 'dark',
  uiScale: '100',
  accentColor: '#2776BB',
  weatherCity: '',
});

const modelPanelRef = ref(null);

onMounted(async () => {
  // 加载配置
  const savedConfig = await window.appAPI.getAllConfig();
  if (savedConfig) {
    Object.assign(config, savedConfig);
  }
});

function onConfigChanged() {
  emit('config-changed');
}

// 暴露给 App.vue 用于刷新
defineExpose({
  refreshModel() {
    if (modelPanelRef.value?.modelHubRef) {
      modelPanelRef.value.modelHubRef.rescan?.();
    }
  }
});
</script>

<style scoped>
.settings-view {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--bg-root);
}

.settings-scroll {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  padding: var(--space-4) var(--space-5);
  scrollbar-width: thin;
  scrollbar-color: var(--border-subtle) transparent;
  scrollbar-gutter: stable;
}

.settings-scroll::-webkit-scrollbar {
  width: 6px;
}

.settings-scroll::-webkit-scrollbar-thumb {
  background: var(--border-subtle);
  border-radius: 3px;
}

.settings-content {
  max-width: 700px;
  margin: 0 auto;
}
@media (max-width: 768px) {
  .settings-scroll {
    padding: 0 !important;
    padding-bottom: calc(60px + env(safe-area-inset-bottom, 16px)) !important;
    overflow-y: overlay;
  }
  /* 移动端彻底压平卡片，实现 Edge-to-Edge 沉浸式列表 */
  .settings-scroll :deep(.settings-section.card) {
    border-radius: 0;
    border-left: none;
    border-right: none;
    border-top: none;
    border-bottom: 1px solid var(--border-subtle);
    background: transparent;
    margin-bottom: 0;
    padding: 20px 16px;
  }
  .settings-scroll :deep(.settings-section.card:last-child) {
    border-bottom: none;
  }
  .settings-scroll::-webkit-scrollbar {
    width: 4px;
  }
  .settings-scroll::-webkit-scrollbar-track {
    background: transparent;
  }
  .settings-scroll::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.15);
    border-radius: 10px;
  }
  .settings-scroll::-webkit-scrollbar-thumb:hover {
    background: rgba(255, 255, 255, 0.3);
  }
}
</style>
