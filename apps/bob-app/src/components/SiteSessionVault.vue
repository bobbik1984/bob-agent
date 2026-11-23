<script setup>
import { computed, inject, ref } from 'vue';
import { useI18n } from 'vue-i18n';

const shell = inject('bobAppShell', {});
const jarvisHome = shell?.jarvisHome;
const { t } = useI18n();

const targetUrl = ref('');
const confirmingClearAll = ref(false);
const refreshing = ref(false);

const siteSessions = computed(() => {
  return jarvisHome?.siteSessions?.value || [];
});

async function handlePreLogin() {
  const raw = targetUrl.value.trim();
  if (!raw) return;
  let url = raw;
  if (!url.startsWith('http://') && !url.startsWith('https://')) {
    url = `https://${url}`;
  }
  if (typeof jarvisHome?.openSiteForLogin === 'function') {
    await jarvisHome.openSiteForLogin(url);
  }
  targetUrl.value = '';
}

async function handleClearSession(domain) {
  if (typeof jarvisHome?.clearSiteSession === 'function') {
    await jarvisHome.clearSiteSession(domain);
  }
}

async function handleClearAll() {
  const list = [...siteSessions.value];
  for (const item of list) {
    const domain = item.domain || item.host || item.id;
    if (domain && typeof jarvisHome?.clearSiteSession === 'function') {
      await jarvisHome.clearSiteSession(domain);
    }
  }
  confirmingClearAll.value = false;
}

async function handleRefresh() {
  refreshing.value = true;
  try {
    if (typeof jarvisHome?.refreshSiteSessions === 'function') {
      await jarvisHome.refreshSiteSessions();
    }
  } finally {
    refreshing.value = false;
  }
}

function formatLastUsed(ts) {
  if (!ts) return '';
  try {
    const d = new Date(ts);
    return d.toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return String(ts);
  }
}
</script>

<template>
  <section class="hub-card site-session-vault">
    <div class="hub-card-header">
      <div class="hub-card-title-group">
        <svg class="hub-icon" viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2">
          <rect x="2" y="4" width="20" height="16" rx="2" />
          <path d="M2 8h20" />
          <circle cx="6" cy="6" r="1" fill="currentColor" />
          <circle cx="10" cy="6" r="1" fill="currentColor" />
        </svg>
        <h3>{{ t('browserSessionVaultTitle') }}</h3>
        <span class="hub-count-pill">{{ siteSessions.length }}</span>
      </div>
      <button
        type="button"
        class="mini-ghost-btn"
        :disabled="refreshing"
        :title="t('browserReload')"
        @click="handleRefresh"
      >
        <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
          <path d="M3 3v5h5" />
          <path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16" />
          <path d="M16 21h5v-5" />
        </svg>
      </button>
    </div>

    <p class="view-card-desc">{{ t('browserSessionVaultDesc') }}</p>

    <!-- Pre-login URL Input Bar -->
    <div class="prelogin-input-row">
      <input
        v-model="targetUrl"
        type="url"
        class="prelogin-input"
        :placeholder="t('browserPreLoginPlaceholder')"
        @keydown.enter.prevent="handlePreLogin"
      />
      <button
        type="button"
        class="prelogin-btn"
        :disabled="!targetUrl.trim()"
        @click="handlePreLogin"
      >
        <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
          <polyline points="15 3 21 3 21 9" />
          <line x1="10" y1="14" x2="21" y2="3" />
        </svg>
        <span>{{ t('browserPreLoginSiteBtn') }}</span>
      </button>
    </div>

    <!-- Saved Site Sessions List -->
    <div v-if="siteSessions.length > 0" class="site-sessions-list">
      <div
        v-for="session in siteSessions"
        :key="session.domain || session.id"
        class="site-session-item"
      >
        <div class="session-domain-col">
          <div class="session-domain-title-row">
            <svg class="domain-icon" viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="12" cy="12" r="10" />
              <line x1="2" y1="12" x2="22" y2="12" />
              <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
            </svg>
            <strong class="session-domain-name">{{ session.domain || session.host || session.id }}</strong>
          </div>
          <div class="session-meta-row">
            <span class="session-cookie-indicator">
              <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2">
                <circle cx="12" cy="12" r="9" />
                <circle cx="8" cy="9" r="1" fill="currentColor" />
                <circle cx="15" cy="8" r="1" fill="currentColor" />
                <circle cx="14" cy="14" r="1" fill="currentColor" />
                <circle cx="9" cy="15" r="1" fill="currentColor" />
              </svg>
              <span>{{ t('browserCookiesStored') }}</span>
            </span>
            <span v-if="session.last_used || session.lastUsed" class="session-time-text">
              {{ t('browserLastUsed', { time: formatLastUsed(session.last_used || session.lastUsed) }) }}
            </span>
          </div>
        </div>

        <button
          type="button"
          class="clear-session-btn"
          :title="t('browserClearSession')"
          @click="handleClearSession(session.domain || session.id)"
        >
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="3 6 5 6 21 6" />
            <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
          </svg>
          <span>{{ t('browserClearSession') }}</span>
        </button>
      </div>

      <!-- Clear All Control -->
      <div class="clear-all-row">
        <template v-if="!confirmingClearAll">
          <button
            type="button"
            class="clear-all-btn"
            @click="confirmingClearAll = true"
          >
            {{ t('browserClearAllSessions') }}
          </button>
        </template>
        <template v-else>
          <span class="clear-all-confirm-label">{{ t('browserClearAllConfirm') }}</span>
          <div class="confirm-actions-row">
            <button
              type="button"
              class="clear-all-confirm-btn"
              @click="handleClearAll"
            >
              {{ t('confirm') || 'OK' }}
            </button>
            <button
              type="button"
              class="clear-all-cancel-btn"
              @click="confirmingClearAll = false"
            >
              {{ t('cancel') || 'Cancel' }}
            </button>
          </div>
        </template>
      </div>
    </div>

    <!-- Empty State -->
    <div v-else class="vault-empty-state">
      <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2">
        <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
        <path d="M7 11V7a5 5 0 0 1 10 0v4" />
      </svg>
      <span>{{ t('browserNoSavedSessions') }}</span>
    </div>
  </section>
</template>

<style scoped>
.site-session-vault {
  display: flex;
  flex-direction: column;
  gap: var(--gap-sm);
}

.prelogin-input-row {
  display: flex;
  align-items: center;
  gap: var(--gap-xs);
  margin-top: var(--gap-3xs);
}

.prelogin-input {
  flex: 1;
  min-width: 0;
  padding: 0.45rem 0.75rem;
  background: var(--bg-secondary);
  border: 1px solid var(--line);
  border-radius: var(--radius-md);
  color: var(--text-primary);
  font-size: var(--text-sm);
  outline: none;
  transition: border-color 0.15s ease;
}

.prelogin-input:focus {
  border-color: var(--accent-primary);
}

.prelogin-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  padding: 0.45rem 0.85rem;
  background: var(--accent-primary);
  border: 1px solid var(--accent-primary);
  border-radius: var(--radius-md);
  color: var(--text-on-accent);
  font-size: var(--text-xs);
  font-weight: 600;
  cursor: pointer;
  white-space: nowrap;
  transition: opacity 0.15s ease;
}

.prelogin-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.prelogin-btn:hover:not(:disabled) {
  opacity: 0.92;
}

.site-sessions-list {
  display: flex;
  flex-direction: column;
  gap: var(--gap-xs);
  margin-top: var(--gap-3xs);
}

.site-session-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--gap-sm);
  padding: var(--gap-sm) var(--gap-md);
  background: var(--bg-secondary);
  border: 1px solid var(--line);
  border-radius: var(--radius-card);
  transition: border-color 0.15s ease;
}

.session-domain-col {
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
  min-width: 0;
  flex: 1;
}

.session-domain-title-row {
  display: flex;
  align-items: center;
  gap: 0.4rem;
}

.domain-icon {
  color: var(--accent-primary);
  flex-shrink: 0;
}

.session-domain-name {
  font-size: var(--text-sm);
  color: var(--text-primary);
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.session-meta-row {
  display: flex;
  align-items: center;
  gap: var(--gap-sm);
  font-size: var(--text-2xs);
  color: var(--text-secondary);
  flex-wrap: wrap;
}

.session-cookie-indicator {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  color: var(--color-success);
}

.session-time-text {
  color: var(--text-tertiary);
}

.clear-session-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  padding: 0.3rem 0.6rem;
  background: var(--bg-tertiary);
  border: 1px solid var(--line);
  border-radius: var(--radius-md);
  color: var(--text-secondary);
  font-size: var(--text-xs);
  cursor: pointer;
  white-space: nowrap;
  flex-shrink: 0;
  transition: color 0.15s ease, background 0.15s ease;
}

.clear-session-btn:hover {
  background: color-mix(in srgb, var(--color-error) 15%, var(--bg-primary));
  color: var(--color-error);
  border-color: color-mix(in srgb, var(--color-error) 30%, transparent);
}

.clear-all-row {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--gap-sm);
  padding-top: var(--gap-xs);
  border-top: 1px solid var(--line);
}

.clear-all-btn {
  padding: 0.3rem 0.65rem;
  background: transparent;
  border: 1px solid var(--line);
  border-radius: var(--radius-md);
  color: var(--text-secondary);
  font-size: var(--text-xs);
  cursor: pointer;
  transition: color 0.15s ease;
}

.clear-all-btn:hover {
  color: var(--color-error);
}

.clear-all-confirm-label {
  font-size: var(--text-xs);
  color: var(--text-secondary);
}

.confirm-actions-row {
  display: flex;
  align-items: center;
  gap: var(--gap-xs);
}

.clear-all-confirm-btn {
  padding: 0.25rem 0.6rem;
  background: var(--color-error);
  border: 1px solid var(--color-error);
  border-radius: var(--radius-md);
  color: var(--text-on-accent);
  font-size: var(--text-xs);
  font-weight: 600;
  cursor: pointer;
}

.clear-all-cancel-btn {
  padding: 0.25rem 0.6rem;
  background: var(--bg-secondary);
  border: 1px solid var(--line);
  border-radius: var(--radius-md);
  color: var(--text-secondary);
  font-size: var(--text-xs);
  cursor: pointer;
}

.vault-empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--gap-xs);
  padding: var(--gap-lg);
  color: var(--text-tertiary);
  font-size: var(--text-sm);
  background: var(--bg-secondary);
  border: 1px dashed var(--line);
  border-radius: var(--radius-card);
}
</style>
