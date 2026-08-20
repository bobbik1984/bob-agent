<template>
  <Teleport to="body">
    <Transition name="today-layer">
      <div v-if="visible" class="today-layer-backdrop" @click.self="$emit('close')">
        <section
          ref="panelRef"
          class="today-layer-panel"
          role="dialog"
          aria-modal="true"
          :aria-label="t('today.title')"
          tabindex="-1"
          @keydown="handleKeydown"
        >
          <header class="today-layer-header">
            <div class="today-layer-title">
              <CalendarRange :size="17" />
              <span>{{ t('today.layer_title') }}</span>
            </div>
            <div class="today-layer-tools">
              <button type="button" class="today-layer-icon" :title="t('today.refresh')" :disabled="refreshing" @click="$emit('refresh')">
                <RefreshCw :size="14" :class="{ spinning: refreshing }" />
              </button>
              <button ref="closeRef" type="button" class="today-layer-icon" :title="t('today.close')" @click="$emit('close')">
                <X :size="15" />
              </button>
            </div>
          </header>

          <div class="today-layer-scroll">
            <TodayBriefCard
              :snapshot="snapshot"
              :loading="loading"
              :error-code="errorCode"
              @action="$emit('action', $event)"
              @expand="detailsOpen = true"
              @refresh="$emit('refresh')"
            />

            <section v-if="detailsOpen && snapshot?.detailItems?.length" class="today-details">
              <header class="today-details-header">
                <span>{{ t('today.details') }}</span>
                <button type="button" class="today-layer-icon" :title="t('today.collapse')" @click="detailsOpen = false">
                  <ChevronUp :size="14" />
                </button>
              </header>
              <button
                v-for="item in snapshot.detailItems"
                :key="item.itemId"
                type="button"
                class="today-detail-row"
                @click="$emit('action', item)"
              >
                <span class="today-detail-title">{{ itemTitle(item) }}</span>
                <span class="today-detail-source">{{ sourceLabel(item.source) }}</span>
              </button>
            </section>
          </div>
        </section>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup>
import { nextTick, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { CalendarRange, ChevronUp, RefreshCw, X } from 'lucide-vue-next';
import TodayBriefCard from './TodayBriefCard.vue';

const props = defineProps({
  visible: { type: Boolean, default: false },
  snapshot: { type: Object, default: null },
  loading: { type: Boolean, default: false },
  refreshing: { type: Boolean, default: false },
  errorCode: { type: String, default: '' },
});
const emit = defineEmits(['ready', 'close', 'refresh', 'action']);
const { t, te } = useI18n();
const panelRef = ref(null);
const closeRef = ref(null);
const detailsOpen = ref(false);

watch(() => props.visible, async (visible) => {
  if (!visible) {
    detailsOpen.value = false;
    return;
  }
  await nextTick();
  panelRef.value?.focus({ preventScroll: true });
  emit('ready');
}, { immediate: true });

function itemTitle(item) {
  if (item.title) return item.title;
  if (item.titleKey && te(item.titleKey)) return t(item.titleKey, item.messageArgs || {});
  return t('today.untitled');
}

function sourceLabel(source) {
  const key = `today.source.${source}`;
  return te(key) ? t(key) : '';
}

function handleKeydown(event) {
  if (event.key === 'Escape') {
    event.preventDefault();
    emit('close');
    return;
  }
  if (event.key !== 'Tab') return;
  const focusable = Array.from(panelRef.value?.querySelectorAll(
    'button:not([disabled]), [href], input:not([disabled]), [tabindex]:not([tabindex="-1"])',
  ) || []);
  if (!focusable.length) {
    event.preventDefault();
    panelRef.value?.focus();
    return;
  }
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}
</script>

<style scoped>
.today-layer-backdrop {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: grid;
  place-items: center;
  padding: var(--space-6);
  background: rgba(0, 0, 0, 0.42);
}

.today-layer-panel {
  width: min(520px, calc(100vw - 48px));
  max-height: min(680px, calc(100dvh - 64px));
  display: flex;
  flex-direction: column;
  overflow: hidden;
  color: var(--text-primary);
  background: var(--bg-secondary);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-xl);
  box-shadow: var(--shadow-lg);
  outline: none;
}

.today-layer-header,
.today-layer-title,
.today-layer-tools,
.today-details-header {
  display: flex;
  align-items: center;
}

.today-layer-header {
  min-height: 48px;
  padding: 0 var(--space-4);
  justify-content: space-between;
  border-bottom: 1px solid var(--border-subtle);
}

.today-layer-title {
  gap: var(--space-2);
  font-size: var(--text-sm);
  font-weight: 600;
}

.today-layer-tools { gap: var(--space-1); }

.today-layer-icon {
  width: 30px;
  height: 30px;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  color: var(--text-secondary);
  background: transparent;
  border: 0;
  border-radius: var(--radius-md);
  cursor: pointer;
}

.today-layer-icon:hover { color: var(--text-primary); background: var(--bg-hover); }
.today-layer-icon:disabled { opacity: 0.5; cursor: default; }

.today-layer-scroll {
  min-height: 0;
  padding: var(--space-4);
  overflow-y: auto;
  scrollbar-gutter: stable;
}

.today-details {
  margin-top: var(--space-4);
  background: var(--bg-primary);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-lg);
  overflow: hidden;
}

.today-details-header {
  min-height: 38px;
  padding: 0 var(--space-3) 0 var(--space-4);
  justify-content: space-between;
  color: var(--text-secondary);
  border-bottom: 1px solid var(--border-subtle);
  font-size: var(--text-xs);
  font-weight: 600;
}

.today-detail-row {
  width: 100%;
  min-height: 42px;
  padding: 0 var(--space-4);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  color: var(--text-primary);
  background: transparent;
  border: 0;
  border-bottom: 1px solid var(--border-subtle);
  cursor: pointer;
}

.today-detail-row:last-child { border-bottom: 0; }
.today-detail-row:hover { background: var(--bg-hover); }

.today-detail-title {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--text-sm);
}

.today-detail-source {
  flex-shrink: 0;
  color: var(--text-tertiary);
  font-size: var(--text-xs);
}

.today-layer-enter-active,
.today-layer-leave-active { transition: opacity 0.16s ease; }
.today-layer-enter-active .today-layer-panel,
.today-layer-leave-active .today-layer-panel { transition: transform 0.16s ease, opacity 0.16s ease; }
.today-layer-enter-from,
.today-layer-leave-to { opacity: 0; }
.today-layer-enter-from .today-layer-panel,
.today-layer-leave-to .today-layer-panel { opacity: 0; transform: translateY(8px); }

.spinning { animation: today-spin 0.8s linear infinite; }
@keyframes today-spin { to { transform: rotate(360deg); } }

@media (max-width: 700px) {
  .today-layer-backdrop {
    padding: 16px;
    align-items: center;
  }

  .today-layer-panel {
    width: 100%;
    max-height: calc(100dvh - 48px - env(safe-area-inset-top) - env(safe-area-inset-bottom));
    border-radius: var(--radius-lg);
  }

  .today-layer-scroll { padding: var(--space-3); }
}

@media (prefers-reduced-motion: reduce) {
  .today-layer-enter-active,
  .today-layer-leave-active,
  .today-layer-enter-active .today-layer-panel,
  .today-layer-leave-active .today-layer-panel { transition: none; }
  .spinning { animation: none; }
}
</style>
