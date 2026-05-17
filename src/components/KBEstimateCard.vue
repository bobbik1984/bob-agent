<template>
  <div class="confirm-card estimate-card">
    <div class="card-header">
      <div class="card-icon estimate-icon">
        <Calculator :size="24" />
      </div>
      <div class="card-title-area">
        <div class="card-type">{{ $t('kb_estimate.title') }}</div>
        <div class="card-title">{{ folderName || $t('kb_estimate.unknown_folder') }}</div>
      </div>
    </div>

    <div class="card-body">
      <div class="info-row" v-if="!estimateResult">
        <div class="loading-state">
          <Loader2 class="animate-spin" :size="20" />
          <span>{{ $t('kb_estimate.analyzing') }}</span>
        </div>
      </div>

      <template v-else-if="estimateResult.error">
        <div class="error-state">
          <AlertTriangle :size="20" style="color: var(--error);" />
          <span>{{ estimateResult.error }}</span>
        </div>
      </template>

      <template v-else>
        <div class="stats-grid">
          <div class="stat-item">
            <div class="stat-value">{{ estimateResult.convertable_files }}</div>
            <div class="stat-label">{{ $t('kb_estimate.convertable_files') }}</div>
          </div>
          <div class="stat-item">
            <div class="stat-value">{{ formatSize(estimateResult.convertable_bytes) }}</div>
            <div class="stat-label">{{ $t('kb_estimate.data_size') }}</div>
          </div>
          <div class="stat-item">
            <div class="stat-value">{{ formatTokens(estimateResult.estimated_tokens) }}</div>
            <div class="stat-label">{{ $t('kb_estimate.estimated_tokens') }}</div>
          </div>
        </div>

        <div class="cost-section">
          <div class="cost-title">{{ $t('kb_estimate.cost_title') }}</div>
          
          <div class="cost-option" :class="{ selected: selectedPlan === 'cheap' }" @click="selectedPlan = 'cheap'">
            <div class="cost-option-header">
              <span class="plan-name"><Zap :size="14" /> {{ $t('kb_estimate.plan_cheap') }}</span>
              <span class="plan-price">~ ¥{{ estimateResult.estimated_cost_cheap_rmb.toFixed(4) }}</span>
            </div>
            <div class="cost-option-desc">{{ $t('kb_estimate.plan_cheap_desc') }}</div>
          </div>

          <div class="cost-option" :class="{ selected: selectedPlan === 'core' }" @click="selectedPlan = 'core'">
            <div class="cost-option-header">
              <span class="plan-name"><Cpu :size="14" /> {{ $t('kb_estimate.plan_core') }}</span>
              <span class="plan-price">~ ¥{{ estimateResult.estimated_cost_core_rmb.toFixed(4) }}</span>
            </div>
            <div class="cost-option-desc">{{ $t('kb_estimate.plan_core_desc') }}</div>
          </div>
        </div>
        
        <div class="warning-text">
          <Info :size="14" />
          {{ $t('kb_estimate.cost_warning') }}
        </div>
      </template>
    </div>

    <div class="card-footer">
      <button class="btn btn-ghost" @click="$emit('cancel')">{{ $t('kb_estimate.later') }}</button>
      <button 
        class="btn btn-primary" 
        :disabled="!estimateResult || estimateResult.error || estimateResult.convertable_files === 0"
        @click="$emit('confirm', selectedPlan)"
      >
        <Play :size="16" />
        {{ $t('kb_estimate.start_build') }}
      </button>
    </div>
  </div>
</template>

<script setup>
import { ref } from 'vue';
import { Calculator, Loader2, AlertTriangle, Info, Zap, Cpu, Play } from 'lucide-vue-next';

const props = defineProps({
  folderName: {
    type: String,
    default: ''
  },
  estimateResult: {
    type: Object,
    default: null
  }
});

defineEmits(['confirm', 'cancel']);

const selectedPlan = ref('cheap');

const formatSize = (bytes) => {
  if (bytes < 1024) return bytes + ' B';
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
  return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
};

const formatTokens = (tokens) => {
  if (tokens < 1000) return tokens;
  if (tokens < 1000000) return (tokens / 1000).toFixed(1) + 'k';
  return (tokens / 1000000).toFixed(2) + 'M';
};
</script>

<style scoped>
.confirm-card {
  background-color: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  overflow: hidden;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.2);
  width: 100%;
  max-width: 450px;
  margin: 10px 0;
}

.card-header {
  display: flex;
  align-items: center;
  padding: 16px;
  border-bottom: 1px solid var(--border-color);
  background-color: var(--bg-secondary);
}

.card-icon.estimate-icon {
  background-color: rgba(var(--accent-primary-rgb), 0.15);
  color: var(--accent-primary);
  width: 48px;
  height: 48px;
  border-radius: 12px;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-right: 16px;
}

.card-title-area {
  flex: 1;
  min-width: 0;
}

.card-type {
  font-size: 12px;
  color: var(--text-secondary);
  margin-bottom: 4px;
}

.card-title {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.card-body {
  padding: 16px;
}

.loading-state, .error-state {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 20px;
  color: var(--text-secondary);
  justify-content: center;
}

.stats-grid {
  display: flex;
  background-color: var(--bg-secondary);
  border-radius: 8px;
  padding: 12px;
  margin-bottom: 16px;
  border: 1px solid var(--border-color);
}

.stat-item {
  flex: 1;
  text-align: center;
  border-right: 1px solid var(--border-color);
}

.stat-item:last-child {
  border-right: none;
}

.stat-value {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
  line-height: 1.2;
  margin-bottom: 4px;
}

.stat-label {
  font-size: 11px;
  color: var(--text-secondary);
}

.cost-section {
  margin-bottom: 16px;
}

.cost-title {
  font-size: 13px;
  color: var(--text-secondary);
  margin-bottom: 8px;
  font-weight: 500;
}

.cost-option {
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 12px;
  margin-bottom: 8px;
  cursor: pointer;
  transition: all 0.2s;
  background-color: transparent;
}

.cost-option:hover {
  background-color: var(--bg-hover);
}

.cost-option.selected {
  border-color: var(--accent-primary);
  background-color: rgba(var(--accent-primary-rgb), 0.05);
}

.cost-option-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 4px;
}

.plan-name {
  display: flex;
  align-items: center;
  gap: 6px;
  font-weight: 500;
  font-size: 14px;
  color: var(--text-primary);
}

.plan-price {
  font-weight: 600;
  color: var(--accent-primary);
  font-size: 14px;
}

.cost-option-desc {
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.4;
}

.warning-text {
  display: flex;
  gap: 8px;
  font-size: 12px;
  color: var(--text-tertiary);
  background-color: var(--bg-secondary);
  padding: 10px;
  border-radius: 8px;
  line-height: 1.4;
}

.card-footer {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
  padding: 16px;
  border-top: 1px solid var(--border-color);
  background-color: var(--bg-secondary);
}

.btn {
  padding: 8px 16px;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  transition: all 0.2s;
  border: none;
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-ghost {
  background: transparent;
  color: var(--text-secondary);
}

.btn-ghost:not(:disabled):hover {
  background-color: var(--bg-hover);
  color: var(--text-primary);
}

.btn-primary {
  background-color: var(--accent-primary);
  color: white;
}

.btn-primary:not(:disabled):hover {
  background-color: var(--accent-hover);
}
</style>
