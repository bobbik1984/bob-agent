/**
 * useModelSwitcher — 模型切换逻辑 composable
 *
 * 职责:
 *   - 模型列表管理 (availableModels, modelProviderList, switcherModels)
 *   - 当前模型状态 (currentModelRaw, currentModelName, currentModelLogo)
 *   - 模型切换交互 (toggleModelSwitcher, switchModel)
 *   - 模型 logo 匹配 (getModelLogo)
 */

import { ref, computed } from 'vue';

export function useModelSwitcher() {
  const currentModelRaw = ref('');
  const showModelSwitcher = ref(false);
  const availableModels = ref([]);
  const switcherProvider = ref('');

  // 从 availableModels 分组出供应商列表
  const modelProviderList = computed(() => {
    const map = {};
    for (const m of availableModels.value) {
      const prov = m.provider || 'unknown';
      if (!map[prov]) map[prov] = { id: prov, name: m.providerName || prov, count: 0 };
      map[prov].count++;
    }
    return Object.values(map);
  });

  // 当前供应商下的模型列表
  const switcherModels = computed(() => {
    if (!switcherProvider.value) return [];
    const models = availableModels.value.filter(m => (m.provider || 'unknown') === switcherProvider.value);
    return models.sort((a, b) => {
      const nameA = a.displayName || a.id || '';
      const nameB = b.displayName || b.id || '';
      return nameA.localeCompare(nameB);
    });
  });

  // ── Logo 匹配 ──
  function getModelLogo(modelId) {
    const name = (modelId || '').toLowerCase();
    if (name.includes('deepseek')) return '/logos/deepseek.png';
    if (name.includes('gpt') || name.includes('openai')) return '/logos/openai.png';
    if (name.includes('gemini') || name.includes('google') || name.includes('gemma')) return '/logos/google.png';
    if (name.includes('qwen') || name.includes('dashscope')) return '/logos/qwen.png';
    if (name.includes('glm') || name.includes('zhipu')) return '/logos/glm.svg';
    if (name.includes('kimi') || name.includes('moonshot')) return '/logos/kimi.png';
    if (name.includes('doubao') || name.includes('seed')) return '/logos/doubao.png';
    if (name.includes('minimax')) return '/logos/minimax.png';
    if (name.includes('mimo')) return '/logos/mimo.png';
    if (name.includes('modelscope')) return '/logos/modelscope.png';
    if (name.includes('claude') || name.includes('anthropic')) return '/logos/claude.png';
    if (name.includes('grok') || name.includes('xai')) return '/logos/grok.png';
    if (name.includes('openrouter')) return '/logos/openrouter.png';
    return null;
  }

  const currentModelName = computed(() => {
    const found = availableModels.value.find(m => m.id === currentModelRaw.value);
    if (found) return found.displayName || found.label;
    if (currentModelRaw.value && currentModelRaw.value.includes('::')) {
      return currentModelRaw.value.split('::')[1];
    }
    return currentModelRaw.value || '';
  });

  const currentModelLogo = computed(() => {
    return getModelLogo(currentModelRaw.value);
  });

  async function toggleModelSwitcher() {
    if (!showModelSwitcher.value) {
      try {
        const pool = await window.electronAPI.getModelPool();
        let keys = {};
        if (window.electronAPI.getApiKeys) {
          keys = await window.electronAPI.getApiKeys() || {};
        }
        availableModels.value = (pool || [])
          .filter(m => !!keys[m.provider])
          .map(m => ({
            id: m.id,
            provider: m.provider,
            providerName: m.providerName,
            displayName: m.displayName,
          }));
        if (currentModelRaw.value && currentModelRaw.value.includes('::')) {
          switcherProvider.value = currentModelRaw.value.split('::')[0];
        } else if (modelProviderList.value.length > 0) {
          switcherProvider.value = modelProviderList.value[0].id;
        }
      } catch (e) {
        availableModels.value = [];
      }
    }
    showModelSwitcher.value = !showModelSwitcher.value;
  }

  async function switchModel(modelId) {
    await window.electronAPI.assignModelRole(modelId, 'main');
    currentModelRaw.value = modelId;
    showModelSwitcher.value = false;
  }

  // 预加载模型列表
  async function initModels() {
    try {
      const pool = await window.electronAPI.getModelPool();
      availableModels.value = (pool || []).map(m => ({
        id: m.id,
        provider: m.provider,
        providerName: m.providerName,
        displayName: m.displayName,
      }));
      const active = await window.electronAPI.getActiveModels();
      currentModelRaw.value = active?.main || '';
    } catch (e) { /* ignore */ }
  }

  async function refreshModel() {
    try {
      const active = await window.electronAPI.getActiveModels();
      if (active && active.main) {
        currentModelRaw.value = active.main;
      }
    } catch (e) { /* ignore */ }
  }

  return {
    currentModelRaw,
    showModelSwitcher,
    availableModels,
    switcherProvider,
    modelProviderList,
    switcherModels,
    currentModelName,
    currentModelLogo,
    getModelLogo,
    toggleModelSwitcher,
    switchModel,
    initModels,
    refreshModel,
  };
}
