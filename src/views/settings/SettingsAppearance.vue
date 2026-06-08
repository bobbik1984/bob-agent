<template>
  <!-- 外观 -->
  <section class="settings-section card">
    <h3 class="section-title">
      <Palette :size="16" class="section-icon" />
      {{ $t('settings.appearance') }}
    </h3>
    <div class="form-group">
      <label class="form-label">{{ $t('settings.theme') }}</label>
      <CustomSelect
        v-model="config.theme"
        :options="themeOptions"
        @change="applyTheme(config.theme)"
      />
    </div>
    <div class="form-group">
      <label class="form-label">{{ $t('settings.ui_scale') }}</label>
      <CustomSelect
        v-model="config.uiScale"
        :options="uiScaleOptions"
        @change="applyUiScale(config.uiScale)"
      />
    </div>
    <div class="form-group" style="margin-top: 12px;">
      <label class="form-label">{{ $t('settings.accent_color') }}</label>
      <div style="display: flex; gap: 12px; flex-wrap: wrap; margin-top: 8px;">
        <button 
          v-for="color in accentColors" 
          :key="color.value"
          class="accent-circle"
          :class="{ active: config.accentColor === color.value }"
          :style="{ backgroundColor: color.value }"
          @click="applyAccentColor(color.value)"
          :title="color.name"
        ></button>
      </div>
    </div>
  </section>

  <!-- 语言 (直接嵌入外观区后面) -->
  <section class="settings-section card">
    <h3 class="section-title">
      <Globe :size="16" class="section-icon" />
      {{ $t('settings.language') }}
    </h3>
    <div class="form-group">
      <CustomSelect
        v-model="currentLocale"
        :options="languageOptions"
        @change="switchLanguage"
      />
    </div>
  </section>
</template>

<script setup>
import { ref, computed, inject, onMounted, onUnmounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { Palette, Globe } from 'lucide-vue-next';
import CustomSelect from '../../components/CustomSelect.vue';
import { ACCENT_COLORS, PREMIUM_THEMES } from '@/constants/theme.js';

const props = defineProps({
  config: { type: Object, required: true },
});
const emit = defineEmits(['config-changed']);
const { locale, t } = useI18n();
const injectedTheme = inject('currentTheme', null);

const currentLocale = ref('zh-CN');

const languageOptions = [
  { label: '简体中文', value: 'zh-CN' },
  { label: 'English', value: 'en-US' },
];

function switchLanguage(val) {
  locale.value = val || currentLocale.value;
  saveConfig('language', locale.value);
}

const accentColors = computed(() => ACCENT_COLORS.map(c => ({
  value: c.value,
  name: c.nameKey ? (t(c.nameKey) || c.name) : c.name,
})));

const themeOptions = computed(() => {
  const baseOptions = [
    { label: t('settings.theme_dark'), value: 'dark' },
    { label: t('settings.theme_light'), value: 'light' },
  ];
  const premiumOptions = PREMIUM_THEMES.map(theme => ({
    label: theme.nameKey ? (t(theme.nameKey) || theme.name) : theme.name,
    value: theme.id
  }));
  return [...baseOptions, ...premiumOptions];
});

const uiScaleOptions = computed(() => [
  { label: t('settings.scale_compact'), value: 'compact' },
  { label: t('settings.scale_comfortable'), value: 'comfortable' },
]);

function applyUiScale(scale, persist = true) {
  document.documentElement.setAttribute('data-ui-scale', scale);
  if (persist) {
    saveConfig('uiScale', scale);
  }
}

function applyTheme(theme, persist = true) {
  document.documentElement.classList.add('theme-transitioning');
  
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      document.documentElement.setAttribute('data-theme', theme);
      setTimeout(() => {
        document.documentElement.classList.remove('theme-transitioning');
      }, 850);
    });
  });

  if (persist) {
    saveConfig('theme', theme);
  }
  if (window.electronAPI.updateTheme) {
    window.electronAPI.updateTheme(theme);
  }
}

function applyAccentColor(color) {
  props.config.accentColor = color;
  localStorage.setItem('bob-accent', color);
  document.documentElement.style.setProperty('--user-accent', color);
  const hex = color.replace('#', '');
  const r = parseInt(hex.substring(0, 2), 16);
  const g = parseInt(hex.substring(2, 4), 16);
  const b = parseInt(hex.substring(4, 6), 16);
  document.documentElement.style.setProperty('--user-accent-rgb', `${r}, ${g}, ${b}`);
  saveConfig('accentColor', color);
  emit('config-changed');
}

async function saveConfig(key, value) {
  await window.electronAPI.setConfig(key, value);
  emit('config-changed');
}

// Listen for external theme changes
const handleThemeChange = (e) => {
  if (props.config.theme !== e.detail) {
    props.config.theme = e.detail;
  }
};
window.addEventListener('bob-theme-changed', handleThemeChange);

onMounted(() => {
  // 恢复用户选择的语言
  currentLocale.value = props.config.language || 'zh-CN';
  locale.value = currentLocale.value;

  // 强制同步 DOM 当前的主题（修复标题栏快速切换导致的状态不同步）
  const domTheme = document.documentElement.getAttribute('data-theme');
  if (domTheme && props.config.theme !== domTheme) {
    props.config.theme = domTheme;
  }
});

onUnmounted(() => {
  window.removeEventListener('bob-theme-changed', handleThemeChange);
});
</script>

<style scoped>
.settings-section {
  margin-bottom: var(--space-5);
}

.section-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: var(--text-lg);
  font-weight: 500;
  margin-bottom: var(--space-4);
  color: var(--text-primary);
}

.form-group {
  margin-bottom: var(--space-4);
  position: relative;
}

.form-label {
  display: block;
  font-size: var(--text-sm);
  color: var(--text-secondary);
  margin-bottom: var(--space-2);
  font-weight: 500;
}

.accent-circle {
  width: 24px;
  height: 24px;
  border-radius: 12px;
  border: 2px solid transparent;
  cursor: pointer;
  transition: transform 0.2s, box-shadow 0.2s;
}

.accent-circle:hover {
  transform: scale(1.1);
}

.accent-circle.active {
  border-color: var(--text-primary);
  transform: scale(1.1);
  box-shadow: 0 0 0 2px var(--bg-primary), 0 0 0 4px var(--text-primary);
}
</style>
