<template>
  <section class="remote-panel" :aria-label="t('remote_agent.title')">
    <p class="remote-hint">{{ t('remote_agent.scope') }}</p>
    <form v-if="!state.connection" class="remote-form" @submit.prevent="connect">
      <label>{{ t('remote_agent.endpoint') }}<input v-model="endpoint" type="url" required autocomplete="off" spellcheck="false" /></label>
      <label>{{ t('remote_agent.token') }}<input v-model="token" type="password" autocomplete="off" spellcheck="false" /></label>
      <p class="remote-hint">{{ t('remote_agent.privacy') }}</p>
      <button type="submit" :disabled="state.busy"><LoaderCircle v-if="state.busy" :size="16" /><Plug v-else :size="16" />{{ t(state.busy ? 'remote_agent.connecting' : 'remote_agent.connect') }}</button>
    </form>
    <template v-else>
      <header class="remote-toolbar">
        <span class="remote-endpoint">{{ state.connection.endpoint }}</span>
        <button type="button" :disabled="state.busy" @click="controller.refresh()"><RefreshCw :size="16" />{{ t('remote_agent.refresh') }}</button>
        <button type="button" @click="controller.disconnect()"><Unplug :size="16" />{{ t('remote_agent.disconnect') }}</button>
      </header>
      <div class="remote-workspace">
        <details class="remote-sidebar" :open="!compact || !state.sessionId">
          <summary><PanelLeft :size="16" />{{ t('remote_agent.sessions') }}</summary>
          <label>{{ t('remote_agent.project') }}
            <select v-model="workspaceId"><option value="">{{ t('remote_agent.all_sessions') }}</option><option v-for="project in state.catalog?.workspaces || []" :key="project.workspaceId" :value="project.workspaceId">{{ project.title }}</option></select>
          </label>
          <div class="remote-session-list">
            <button v-for="session in sessions" :key="session.sessionId" type="button" :aria-pressed="state.sessionId === session.sessionId" @click="controller.select(session.sessionId)">
              <MessageSquare :size="16" /><span>{{ session.title }}</span><small v-if="session.running">{{ t('remote_agent.running') }}</small>
            </button>
            <p v-if="!sessions.length" class="remote-hint">{{ t('remote_agent.empty') }}</p>
          </div>
        </details>
        <section class="remote-transcript" aria-live="polite">
          <p v-if="state.historyBusy">{{ t('remote_agent.loading') }}</p>
          <p v-else-if="!state.history" class="remote-hint">{{ t('remote_agent.choose') }}</p>
          <template v-else>
            <p v-if="state.history.hasMore" class="remote-hint">{{ t('remote_agent.recent_only') }}</p>
            <article v-for="message in state.history.messages" :key="message.id" class="remote-message" :class="message.role">
              <small>{{ t(`remote_agent.role_${message.role}`) }}</small>
              <!-- Remote text is untrusted: no v-html, local file links or automatic image fetches. -->
              <div>{{ message.text }}</div>
            </article>
            <p v-if="!state.history.messages.length" class="remote-hint">{{ t('remote_agent.no_text') }}</p>
            <p v-if="state.history.omittedRecords" class="remote-hint">{{ t('remote_agent.omitted', { count: state.history.omittedRecords }) }}</p>
            <button type="button" :disabled="state.historyBusy" @click="controller.select(state.sessionId)"><RefreshCw :size="16" />{{ t('remote_agent.refresh_history') }}</button>
          </template>
        </section>
      </div>
    </template>
    <p v-if="state.error" class="remote-error" role="alert">{{ t(`remote_agent.errors.${knownError}`) }}</p>
  </section>
</template>

<script setup>
import { computed, inject, onBeforeUnmount, reactive, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { LoaderCircle, MessageSquare, PanelLeft, Plug, RefreshCw, Unplug } from 'lucide-vue-next';
import { createRemoteAgentController } from '../../work/remote-agent-state.js';

const { t, te } = useI18n();
const compact = inject('isMobile', ref(false));
const endpoint = ref('');
const token = ref('');
const workspaceId = ref('');
const state = reactive({ connection: null, catalog: null, history: null, sessionId: '', busy: false, historyBusy: false, error: '' });
const controller = createRemoteAgentController(window.appAPI, patch => Object.assign(state, patch));
const knownError = computed(() => te(`remote_agent.errors.${state.error}`) ? state.error : 'REMOTE_UNKNOWN');
const sessions = computed(() => {
  const project = state.catalog?.workspaces.find(item => item.workspaceId === workspaceId.value);
  return (state.catalog?.sessions || []).filter(item => !project || project.sessionIds.includes(item.sessionId));
});
function connect() {
  const input = { adapter: 'dsh', endpoint: endpoint.value, token: token.value };
  token.value = '';
  // Strip pasted query credentials from the visible field immediately, even on failure.
  try { const url = new URL(endpoint.value); url.search = ''; url.hash = ''; endpoint.value = url.toString(); }
  catch { endpoint.value = ''; }
  workspaceId.value = '';
  void controller.connect(input);
}
onBeforeUnmount(() => { token.value = ''; void controller.dispose(); });
</script>

<style scoped>
.remote-panel { padding: 16px; border: 1px solid var(--border-color); border-radius: 16px; background: var(--bg-secondary); color: var(--text-primary); min-width: 0; }
.remote-form, .remote-sidebar label { display: grid; gap: 12px; }
.remote-form label, .remote-sidebar label { display: grid; gap: 6px; color: var(--text-secondary); }
input, select { width: 100%; box-sizing: border-box; font-size: 16px; min-height: 44px; padding: 10px; border-radius: 10px; border: 1px solid var(--border-color); color: var(--text-primary); background: var(--bg-primary); }
button, summary { display: flex; gap: 8px; align-items: center; min-height: 44px; cursor: pointer; }
button { padding: 8px 12px; border: 1px solid var(--border-color); border-radius: 10px; background: var(--bg-primary); color: var(--text-primary); font: inherit; }
button:disabled { opacity: .55; cursor: wait; }
button[aria-pressed="true"] { border-color: var(--accent-primary); background: var(--bg-tertiary); }
button:focus-visible, summary:focus-visible { outline: 2px solid var(--accent-primary); outline-offset: 2px; }
svg { flex-shrink: 0; }
.remote-hint { color: var(--text-secondary); font-size: .875rem; line-height: 1.6; }
.remote-toolbar { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
.remote-endpoint { flex: 1 1 180px; overflow-wrap: anywhere; font-size: .875rem; }
.remote-workspace { display: grid; grid-template-columns: minmax(180px, 30%) minmax(0, 1fr); gap: 16px; margin-top: 16px; }
.remote-sidebar { min-width: 0; }
.remote-session-list { display: grid; gap: 6px; max-height: 480px; overflow-y: auto; margin-top: 12px; scrollbar-gutter: stable; }
.remote-session-list button { text-align: start; width: 100%; }
.remote-session-list span { overflow-wrap: anywhere; flex: 1; min-width: 0; }
.remote-transcript { min-width: 0; max-height: 65dvh; overflow-y: auto; scrollbar-gutter: stable; }
.remote-message { border: 1px solid var(--border-color); background: var(--bg-primary); border-radius: 12px; padding: 14px; margin-block: 10px; }
.remote-message.user { background: var(--bg-tertiary); }
.remote-message small { color: var(--text-secondary); }
.remote-message div { margin-top: 8px; white-space: pre-wrap; overflow-wrap: anywhere; line-height: 1.65; }
.remote-error { color: var(--color-error); overflow-wrap: anywhere; }
@media (max-width: 720px) { .remote-workspace { grid-template-columns: minmax(0, 1fr); } .remote-panel { padding: 12px; } }
</style>
