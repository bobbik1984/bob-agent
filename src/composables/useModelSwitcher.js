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

// ── 静态资源与名称映射 SSOT (供整个项目复用) ──
export function getModelMeta(id) {
  const name = (id || '').toLowerCase();
  
  if (name.includes('deepseek')) return { name: 'DeepSeek', logo: '/logos/deepseek.png' };
  if (name.includes('gpt') || name.includes('o3') || name.includes('o4') || name.includes('openai')) return { name: 'OpenAI', logo: '/logos/openai.png' };
  if (name.includes('gemini') || name.includes('google') || name.includes('gemma') || name.includes('vertex')) return { name: 'Gemini', logo: '/logos/google.png' };
  if (name.includes('qwen') || name.includes('dashscope') || name.includes('aliyun')) return { name: 'Qwen', logo: '/logos/qwen.png' };
  if (name.includes('glm') || name.includes('zhipu')) return { name: 'GLM', logo: '/logos/glm.svg' };
  if (name.includes('kimi') || name.includes('moonshot')) return { name: 'Kimi', logo: '/logos/kimi.png' };
  if (name.includes('doubao') || name.includes('seed') || name.includes('volcengine')) return { name: 'Doubao', logo: '/logos/doubao.png' };
  if (name.includes('minimax')) return { name: 'MiniMax', logo: '/logos/minimax.png' };
  if (name.includes('mimo')) return { name: 'Mimo', logo: '/logos/mimo.png' };
  if (name.includes('modelscope')) return { name: 'ModelScope', logo: '/logos/modelscope.png' };
  if (name.includes('claude') || name.includes('anthropic')) return { name: 'Claude', logo: '/logos/claude.png' };
  if (name.includes('grok') || name.includes('xai')) return { name: 'Grok', logo: '/logos/grok.png' };
  if (name.includes('llama') || name.includes('local-')) return { name: 'Local', logo: null };
  
  return { name: id, logo: null };
}

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
    return getModelMeta(modelId).logo;
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
        const pool = await window.appAPI.getModelPool();
        let keys = {};
        if (window.appAPI.getApiKeys) {
          keys = await window.appAPI.getApiKeys() || {};
        }
        availableModels.value = (pool || [])
          .filter(m => !!keys[m.provider] || m.provider === 'offline')
          .map(m => ({
            id: m.id,
            provider: m.provider,
            providerName: m.providerName,
            displayName: m.displayName || m.id,
            vision: !!m.vision,
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
    await window.appAPI.assignModelRole(modelId, 'main');
    currentModelRaw.value = modelId;
    showModelSwitcher.value = false;
  }

  // 预加载模型列表
  async function initModels() {
    try {
      const pool = await window.appAPI.getModelPool();
      availableModels.value = (pool || []).map(m => ({
        id: m.id,
        provider: m.provider,
        providerName: m.providerName,
        displayName: m.displayName || m.id,
        vision: !!m.vision,
      }));
      const active = await window.appAPI.getActiveModels();
      currentModelRaw.value = active?.main || '';
    } catch (e) { /* ignore */ }
  }

  async function refreshModel() {
    try {
      const active = await window.appAPI.getActiveModels();
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
