<script setup>
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

const props = defineProps({
  card: {
    type: Object,
    required: true,
  },
  busy: {
    type: Boolean,
    default: false,
  },
});

const emit = defineEmits(['approve', 'reject', 'open-takeover', 'save-routine']);

const { t } = useI18n();

const kind = computed(() => props.card?.kind || 'info');
const status = computed(() => props.card?.status || 'pending');
const isPending = computed(() => !props.card?.status || props.card?.status === 'pending');

const resolvedIcon = computed(() => {
  if (props.card?.icon) {
    const raw = String(props.card.icon).toLowerCase();
    if (['calendar', 'weather', 'browser', 'shield', 'file', 'terminal', 'routine', 'spark'].includes(raw)) {
      return raw;
    }
  }
  switch (kind.value) {
    case 'takeover':
      return 'browser';
    case 'approval':
      return 'shield';
    case 'routine_draft':
      return 'routine';
    case 'info':
    default:
      return 'spark';
  }
});

const normalizedItems = computed(() => {
  if (!Array.isArray(props.card?.items)) return [];
  return props.card.items.map(item => {
    if (typeof item === 'string') {
      return { title: item, detail: '', status: '' };
    }
    return {
      title: item?.title || '',
      detail: item?.detail || '',
      status: item?.status || '',
    };
  });
});

const riskBadgeLabel = computed(() => {
  switch (props.card?.riskLevel) {
    case 'side_effect':
      return t('cardRiskSideEffect');
    case 'irreversible':
      return t('cardRiskIrreversible');
    case 'settings':
      return t('cardRiskSettings');
    default:
      return '';
  }
});

const statusBadgeLabel = computed(() => {
  switch (status.value) {
    case 'approved':
      return t('cardApproved');
    case 'rejected':
      return t('cardRejected');
    case 'expired':
      return t('cardExpired');
    default:
      return '';
  }
});
</script>

<template>
  <article
    class="action-card"
    :class="[`card-${kind}`, `status-${status}`]"
    :data-card-id="card.id"
  >
    <!-- Header -->
    <header class="card-header">
      <div class="card-icon-chip" aria-hidden="true">
        <!-- SVG icon map -->
        <svg
          v-if="resolvedIcon === 'calendar'"
          viewBox="0 0 24 24"
          width="18"
          height="18"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <rect x="3" y="4" width="18" height="18" rx="2" ry="2"/>
          <line x1="16" y1="2" x2="16" y2="6"/>
          <line x1="8" y1="2" x2="8" y2="6"/>
          <line x1="3" y1="10" x2="21" y2="10"/>
        </svg>

        <svg
          v-else-if="resolvedIcon === 'weather'"
          viewBox="0 0 24 24"
          width="18"
          height="18"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z"/>
        </svg>

        <svg
          v-else-if="resolvedIcon === 'browser'"
          viewBox="0 0 24 24"
          width="18"
          height="18"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <rect width="20" height="16" x="2" y="4" rx="2"/>
          <path d="M2 8h20"/>
          <circle cx="5" cy="6" r="0.75" fill="currentColor"/>
          <circle cx="8" cy="6" r="0.75" fill="currentColor"/>
        </svg>

        <svg
          v-else-if="resolvedIcon === 'shield'"
          viewBox="0 0 24 24"
          width="18"
          height="18"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>
        </svg>

        <svg
          v-else-if="resolvedIcon === 'file'"
          viewBox="0 0 24 24"
          width="18"
          height="18"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
          <polyline points="14 2 14 8 20 8"/>
        </svg>

        <svg
          v-else-if="resolvedIcon === 'terminal'"
          viewBox="0 0 24 24"
          width="18"
          height="18"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <polyline points="4 17 10 11 4 5"/>
          <line x1="12" y1="19" x2="20" y2="19"/>
        </svg>

        <svg
          v-else-if="resolvedIcon === 'routine'"
          viewBox="0 0 24 24"
          width="18"
          height="18"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <polyline points="23 4 23 10 17 10"/>
          <polyline points="1 20 1 14 7 14"/>
          <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/>
        </svg>

        <svg
          v-else
          viewBox="0 0 24 24"
          width="18"
          height="18"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="m12 3-1.912 5.813a2 2 0 0 1-1.275 1.275L3 12l5.813 1.912a2 2 0 0 1 1.275 1.275L12 21l1.912-5.813a2 2 0 0 1 1.275-1.275L21 12l-5.813-1.912a2 2 0 0 1-1.275-1.275L12 3Z"/>
        </svg>
      </div>

      <div class="card-header-main">
        <div class="card-title-row">
          <h3 class="card-title">{{ card.title }}</h3>
          <span
            v-if="riskBadgeLabel"
            class="risk-badge"
            :class="`risk-${card.riskLevel}`"
          >{{ riskBadgeLabel }}</span>
        </div>
        <p v-if="card.summary" class="card-summary">{{ card.summary }}</p>
      </div>

      <span
        v-if="statusBadgeLabel"
        class="status-badge"
        :class="`status-${status}`"
      >{{ statusBadgeLabel }}</span>
    </header>

    <!-- Key-Value Fields -->
    <div v-if="card.fields && card.fields.length > 0" class="card-fields">
      <div v-for="(field, index) in card.fields" :key="index" class="kv-row">
        <span class="kv-label">{{ field.label }}</span>
        <span class="kv-value">{{ field.value }}</span>
      </div>
    </div>

    <!-- Structured Items List -->
    <div v-if="normalizedItems.length > 0" class="card-items">
      <div
        v-for="(item, index) in normalizedItems"
        :key="index"
        class="card-item-entry"
      >
        <span class="item-bullet" aria-hidden="true"></span>
        <div class="item-content">
          <span class="item-title">{{ item.title }}</span>
          <span v-if="item.detail" class="item-detail">{{ item.detail }}</span>
        </div>
        <span v-if="item.status" class="item-status">{{ item.status }}</span>
      </div>
    </div>

    <!-- Progressive Disclosure: Collapsible Technical Detail -->
    <details v-if="card.technicalDetail" class="card-technical">
      <summary class="technical-summary">{{ t('cardTechnicalDetails') }}</summary>
      <pre class="technical-body"><code>{{ card.technicalDetail }}</code></pre>
    </details>

    <!-- Actions Section -->
    <footer v-if="isPending && (kind === 'approval' || kind === 'takeover' || kind === 'routine_draft')" class="card-actions">
      <!-- Secondary dismiss/reject button -->
      <button
        type="button"
        class="btn btn-secondary"
        :disabled="busy"
        @click="emit('reject', card.id)"
      >
        {{ t('cardReject') }}
      </button>

      <!-- Primary action button per kind -->
      <button
        v-if="kind === 'approval'"
        type="button"
        class="btn btn-primary"
        :disabled="busy"
        @click="emit('approve', card.id)"
      >
        {{ t('cardAllowOnce') }}
      </button>

      <button
        v-else-if="kind === 'takeover'"
        type="button"
        class="btn btn-primary btn-warning"
        :disabled="busy"
        @click="emit('open-takeover', card.id)"
      >
        {{ t('cardOpenTakeover') }}
      </button>

      <button
        v-else-if="kind === 'routine_draft'"
        type="button"
        class="btn btn-primary"
        :disabled="busy"
        @click="emit('save-routine', card.id)"
      >
        {{ t('cardSaveAsRoutine') }}
      </button>
    </footer>
  </article>
</template>

<style scoped>
.action-card {
  display: flex;
  flex-direction: column;
  gap: var(--gap-sm);
  background: var(--bg-primary);
  border: 1px solid var(--line);
  border-radius: var(--radius-card);
  padding: var(--gap-md) var(--gap-lg);
  box-shadow: var(--shadow);
  color: var(--text-primary);
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
}

.action-card.card-approval,
.action-card.card-takeover {
  border-left: 3px solid var(--color-warning);
}

.action-card.status-approved {
  border-left-color: var(--color-success);
}

.action-card.status-rejected {
  border-left-color: var(--line);
  opacity: 0.82;
}

.card-header {
  display: flex;
  align-items: flex-start;
  gap: var(--gap-sm);
}

.card-icon-chip {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 2.25rem;
  height: 2.25rem;
  border-radius: var(--radius-md);
  background: var(--accent-soft);
  color: var(--accent-primary);
  flex-shrink: 0;
}

.card-approval .card-icon-chip,
.card-takeover .card-icon-chip {
  background: var(--warning-soft);
  color: var(--warning-text);
}

.card-header-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: var(--gap-3xs);
}

.card-title-row {
  display: flex;
  align-items: center;
  gap: var(--gap-xs);
  flex-wrap: wrap;
}

.card-title {
  margin: 0;
  font-size: var(--text-md);
  font-weight: 600;
  line-height: 1.35;
  color: var(--text-primary);
  word-break: break-word;
}

.risk-badge {
  display: inline-flex;
  align-items: center;
  padding: 0.1rem 0.45rem;
  border-radius: var(--radius-pill);
  font-size: var(--text-2xs);
  font-weight: 600;
  line-height: 1.2;
}

.risk-badge.risk-side_effect,
.risk-badge.risk-side-effect {
  background: var(--warning-soft);
  color: var(--warning-text);
  border: 1px solid color-mix(in srgb, var(--color-warning) 30%, transparent);
}

.risk-badge.risk-irreversible {
  background: var(--error-soft);
  color: var(--error-text);
  border: 1px solid color-mix(in srgb, var(--color-error) 30%, transparent);
}

.risk-badge.risk-settings {
  background: var(--accent-soft);
  color: var(--accent-primary);
  border: 1px solid color-mix(in srgb, var(--accent-primary) 30%, transparent);
}

.status-badge {
  display: inline-flex;
  align-items: center;
  padding: 0.15rem 0.5rem;
  border-radius: var(--radius-pill);
  font-size: var(--text-xs);
  font-weight: 600;
  align-self: flex-start;
  margin-left: auto;
  flex-shrink: 0;
}

.status-badge.status-approved {
  background: var(--success-soft);
  color: var(--success-text);
}

.status-badge.status-rejected {
  background: color-mix(in srgb, var(--text-secondary) 15%, var(--bg-primary));
  color: var(--text-secondary);
}

.status-badge.status-expired {
  background: color-mix(in srgb, var(--text-tertiary) 15%, var(--bg-primary));
  color: var(--text-tertiary);
}

.card-summary {
  margin: 0;
  font-size: var(--text-sm);
  color: var(--text-secondary);
  line-height: 1.45;
}

.card-fields {
  display: flex;
  flex-direction: column;
  gap: var(--gap-xs);
  padding: var(--gap-xs) 0;
  border-top: 1px solid color-mix(in srgb, var(--line) 60%, transparent);
}

.kv-row {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: var(--gap-md);
  font-size: var(--text-sm);
}

.kv-label {
  color: var(--text-secondary);
  flex-shrink: 0;
}

.kv-value {
  color: var(--text-primary);
  font-weight: 500;
  text-align: right;
  word-break: break-word;
}

.card-items {
  display: flex;
  flex-direction: column;
  gap: var(--gap-xs);
  padding: var(--gap-xs) 0;
  border-top: 1px solid color-mix(in srgb, var(--line) 60%, transparent);
}

.card-item-entry {
  display: flex;
  align-items: flex-start;
  gap: var(--gap-xs);
  font-size: var(--text-sm);
}

.item-bullet {
  width: 5px;
  height: 5px;
  border-radius: var(--radius-pill);
  background: var(--accent-primary);
  margin-top: 0.55rem;
  flex-shrink: 0;
}

.item-content {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: var(--gap-3xs);
}

.item-title {
  color: var(--text-primary);
  font-weight: 500;
  word-break: break-word;
}

.item-detail {
  font-size: var(--text-xs);
  color: var(--text-secondary);
  word-break: break-word;
}

.item-status {
  font-size: var(--text-2xs);
  color: var(--text-secondary);
  background: var(--bg-secondary);
  padding: 0.1rem 0.35rem;
  border-radius: var(--radius-sm);
  flex-shrink: 0;
}

.card-technical {
  border-radius: var(--radius-sm);
  background: var(--bg-secondary);
  border: 1px solid var(--line);
  font-size: var(--text-xs);
  overflow: hidden;
}

.technical-summary {
  padding: var(--gap-xs) var(--gap-sm);
  cursor: pointer;
  color: var(--text-secondary);
  font-weight: 500;
  user-select: none;
}

.technical-summary:hover {
  color: var(--text-primary);
}

.technical-body {
  margin: 0;
  padding: var(--gap-xs) var(--gap-sm);
  border-top: 1px solid var(--line);
  background: var(--bg-tertiary);
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: var(--text-2xs);
  line-height: 1.45;
  color: var(--text-primary);
  overflow-x: auto;
  white-space: pre-wrap;
  word-break: break-word;
}

.card-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--gap-sm);
  margin-top: var(--gap-xs);
  padding-top: var(--gap-xs);
  border-top: 1px solid color-mix(in srgb, var(--line) 60%, transparent);
}

.card-actions .btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0.35rem 0.85rem;
  border-radius: var(--radius-btn);
  font-size: var(--text-sm);
  font-weight: 500;
  cursor: pointer;
  transition: opacity 0.15s ease, background 0.15s ease;
  min-height: 2rem;
}

.card-actions .btn:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.card-actions .btn-secondary {
  background: var(--bg-secondary);
  border: 1px solid var(--line);
  color: var(--text-primary);
}

.card-actions .btn-secondary:not(:disabled):hover {
  background: var(--bg-tertiary);
}

.card-actions .btn-primary {
  background: var(--accent-primary);
  border: 1px solid var(--accent-primary);
  color: var(--text-on-accent);
}

.card-actions .btn-primary:not(:disabled):hover {
  opacity: 0.92;
}

.card-actions .btn-warning {
  background: var(--color-warning);
  border-color: var(--color-warning);
  color: var(--text-on-accent);
}

.card-actions .btn-warning:not(:disabled):hover {
  opacity: 0.92;
}
</style>
