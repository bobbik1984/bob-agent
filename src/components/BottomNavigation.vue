<template>
  <nav class="bottom-nav">
    <button 
      class="bottom-nav-item" 
      :class="{ active: activeDrawer === 'chat' }" 
      @click="$emit('update:activeDrawer', 'chat')"
    >
      <MessageSquare :size="20" />
      <span>{{ $t('nav.conversations') || '对话' }}</span>
    </button>
    <button 
      class="bottom-nav-item" 
      :class="{ active: activeDrawer === 'schedule' }" 
      @click="$emit('update:activeDrawer', 'schedule')"
    >
      <CalendarDays :size="20" />
      <span>{{ $t('nav.schedule') || '日程' }}</span>
    </button>
    <button 
      class="bottom-nav-item" 
      :class="{ active: activeDrawer === 'settings' }" 
      @click="$emit('update:activeDrawer', 'settings')"
    >
      <Settings :size="20" />
      <span>{{ $t('nav.settings') || '设置' }}</span>
    </button>
  </nav>
</template>

<script setup>
import { MessageSquare, CalendarDays, Settings } from 'lucide-vue-next';

defineProps({
  activeDrawer: {
    type: String,
    required: true
  }
});
defineEmits(['update:activeDrawer']);
</script>

<style scoped>
.bottom-nav {
  position: fixed;
  bottom: 0;
  left: 0;
  right: 0;
  height: calc(60px + env(safe-area-inset-bottom, 0px));
  padding-bottom: env(safe-area-inset-bottom, 0px);
  background: var(--bg-secondary);
  border-top: 1px solid var(--border-subtle);
  display: flex;
  justify-content: space-around;
  align-items: center;
  z-index: 100;
}

.bottom-nav-item {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 60px;
  gap: 4px;
  background: transparent;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  transition: all 0.2s ease;
  -webkit-tap-highlight-color: transparent;
}

.bottom-nav-item:active {
  transform: scale(0.95);
}

.bottom-nav-item.active {
  color: var(--accent-primary);
}

.bottom-nav-item span {
  font-size: 10px;
  font-weight: 500;
}
</style>
