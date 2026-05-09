<template>
  <div class="inbox-view">
    <div class="inbox-header">
      <h2 class="inbox-title">📅 智能收件箱</h2>
      <p class="inbox-subtitle">日程管理 + 待办清单</p>
    </div>
    <div class="inbox-placeholder">
      <div class="placeholder-icon">🚧</div>
      <p>Sprint 2 开发中</p>
      <p class="placeholder-hint">将支持自然语言录入日程、周时间轴、待办清单</p>
    </div>
  </div>
</template>

<script setup>
</script>

<style scoped>
.inbox-view {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-4);
}

.inbox-header {
  text-align: center;
}

.inbox-title {
  font-size: var(--text-2xl);
  font-weight: 600;
  color: var(--text-primary);
}

.inbox-subtitle {
  color: var(--text-tertiary);
  margin-top: var(--space-2);
}

.inbox-placeholder {
  text-align: center;
  color: var(--text-secondary);
}

.placeholder-icon {
  font-size: 3rem;
  margin-bottom: var(--space-3);
}

.placeholder-hint {
  font-size: var(--text-sm);
  color: var(--text-tertiary);
  margin-top: var(--space-2);
}
</style>
