<template>
  <section class="remote-panel" :aria-label="t('remote_agent.title')">
    <header class="remote-toolbar">
      <strong>{{ t('remote_agent.work_title') }}</strong>
      <select v-if="state.profiles.length" :value="state.selectedProfileId" :aria-label="t('remote_agent.connections')" :disabled="state.busy" @change="controller.connectProfile($event.target.value)"><option value="" disabled>{{ t('remote_agent.choose_connection') }}</option><option v-for="p in state.profiles" :key="p.id" :value="p.id">{{ p.name }}</option></select>
      <button @click="openConnections"><Settings2 :size="16" />{{ t('remote_agent.manage') }}</button>
    </header>
    <p v-if="!state.connection" class="remote-hint">{{ t(state.unlocked ? 'remote_agent.choose_connection' : 'remote_agent.unlock_in_settings') }}</p>
    <p v-if="state.reconnecting || state.busy" role="status">{{ t('remote_agent.connecting') }}</p>
    <button v-if="state.selectedProfileId && state.error && !state.busy" @click="controller.connectProfile(state.selectedProfileId)">{{ t('remote_agent.retry') }}</button>
    <template v-if="state.connection">
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
            <details v-if="state.live?.reasoning" class="remote-message"><summary>{{ t('remote_agent.reasoning') }}</summary><div>{{ state.live.reasoning }}</div></details>
            <article v-if="state.live?.draft" class="remote-message assistant"><small>{{ t('remote_agent.role_assistant') }}</small><div>{{ state.live.draft }}</div></article>
            <p v-if="!state.history.messages.length" class="remote-hint">{{ t('remote_agent.no_text') }}</p>
            <p v-if="state.history.omittedRecords" class="remote-hint">{{ t('remote_agent.omitted', { count: state.history.omittedRecords }) }}</p>
            <button type="button" :disabled="state.historyBusy" @click="controller.select(state.sessionId)"><RefreshCw :size="16" />{{ t('remote_agent.refresh_history') }}</button>
          </template>
        </section>
      </div>
      <section v-if="state.live" class="remote-controls">
        <aside v-if="controller.needsReview()" class="remote-review"><p>{{ t('remote_agent.review_uncertain') }}</p><button @click="controller.reviewed()">{{ t('remote_agent.reviewed') }}</button></aside>
        <p role="status">{{ t(`remote_agent.phase_${state.live.phase}`) }}</p>
        <p class="remote-hint">{{ t('remote_agent.execution_scope') }}</p>
        <article v-for="item in state.live.interactions" :key="item.eventId" class="remote-message">
          <template v-if="item.kind === 'approval'">
            <strong>{{ t('remote_agent.approval') }} · {{ item.request.toolName }}</strong>
            <div>{{ item.request.reason }}</div>
            <div class="remote-actions">
              <button :disabled="!canAnswer(item)" @click="controller.answer(item.eventId, 'allowed-once')">{{ t('remote_agent.allow_once') }}</button>
              <button :disabled="!canAnswer(item)" @click="controller.answer(item.eventId, 'rejected')">{{ t('remote_agent.reject') }}</button>
            </div>
          </template>
          <form v-else @submit.prevent="answerQuestions(item)">
            <fieldset v-for="q in item.request.questions" :key="q.id" :disabled="!canAnswer(item)">
              <legend>{{ q.question }}</legend><p v-if="q.detail" class="remote-detail">{{ q.detail }}</p>
              <label v-for="option in q.options || []" :key="option.label" class="remote-option">
                <input type="checkbox" :checked="selected(item, q).includes(option.label)" @change="choose(item, q, option.label, $event.target.checked)" />
                <span>{{ option.label }}<small v-if="option.description"> — {{ option.description }}</small></span>
              </label>
              <label>{{ t('remote_agent.custom_answer') }}<input :value="answers[`${item.eventId}:${q.id}:text`] || ''" @input="answers[`${item.eventId}:${q.id}:text`] = $event.target.value" /></label>
            </fieldset>
            <button :disabled="!canAnswer(item)">{{ t('remote_agent.submit_answer') }}</button>
          </form>
          <small>{{ t(`remote_agent.interaction_${item.status}`) }}</small>
        </article>
        <form class="remote-composer" @submit.prevent="send">
          <label>{{ t('remote_agent.message') }}<textarea v-model="message" rows="3" maxlength="64000" :disabled="!canSend" /></label>
          <div class="remote-actions">
            <button type="submit" :disabled="!canSend || !message.trim()"><Send :size="16" />{{ t('remote_agent.send') }}</button>
            <button type="button" :disabled="!state.live.ready || state.actionBusy || state.live.phase !== 'running'" @click="controller.cancel()"><Square :size="16" />{{ t('remote_agent.cancel') }}</button>
          </div>
        </form>
      </section>
    </template>
    <p v-if="state.error" class="remote-error" role="alert">{{ t(`remote_agent.errors.${knownError}`) }}</p>
  </section>
</template>

<script setup>
import { computed, inject, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { MessageSquare, PanelLeft, Settings2, RefreshCw, Unplug, Send, Square } from 'lucide-vue-next';
import { getRemoteAgentService } from '../../work/remote-agent-service.js';

const { t, te } = useI18n();
const compact = inject('isMobile', ref(false));
const openConnections = inject('openRemoteConnections', () => {});
const workspaceId = ref('');
const message = ref('');
const answers = reactive({});
const drafts = new Map();
const controller = getRemoteAgentService(window.appAPI);
const state = controller.state;
watch(() => state.connection && state.sessionId ? `${state.connection.endpoint}:${state.sessionId}` : '', (key, previous) => {
  if (previous) drafts.set(previous, message.value);
  message.value = key ? (drafts.get(key) || '') : '';
  for (const field of Object.keys(answers)) delete answers[field];
}, { flush: 'sync' });
watch(() => state.live?.watchId, () => { for (const field of Object.keys(answers)) delete answers[field]; });
const canSend = computed(() => state.live?.ready && !state.error && !state.actionBusy && !controller.needsReview() && ['idle', 'completed', 'stopped', 'blocked', 'failed'].includes(state.live.phase));
const canAnswer = item => state.live?.ready && !state.error && !state.actionBusy && !controller.needsReview() && item.status === 'pending';
const selected = (item, q) => answers[`${item.eventId}:${q.id}`] || [];
function choose(item, q, label, checked) {
  const old = selected(item, q);
  answers[`${item.eventId}:${q.id}`] = checked ? (q.multiSelect ? [...new Set([...old, label])] : [label]) : old.filter(x => x !== label);
}
async function send() {
  if (!canSend.value || !message.value.trim()) return;
  const text = message.value;
  if (await controller.send(text, crypto.randomUUID())) message.value = '';
}
function answerQuestions(item) {
  void controller.answer(item.eventId, { answers: item.request.questions.map(q => ({ id: q.id, selected: selected(item, q), custom: answers[`${item.eventId}:${q.id}:text`] || '' })) });
}
const knownError = computed(() => te(`remote_agent.errors.${state.error}`) ? state.error : 'REMOTE_UNKNOWN');
const sessions = computed(() => {
  const project = state.catalog?.workspaces.find(item => item.workspaceId === workspaceId.value);
  return (state.catalog?.sessions || []).filter(item => !project || project.sessionIds.includes(item.sessionId));
});
// Restore the selected session's workspace after reconnect; never retain another host's filter.
watch(() => state.connection?.connectionId, () => { workspaceId.value = ''; });
watch(() => state.sessionId, id => {
  const project = state.catalog?.workspaces.find(item => item.sessionIds.includes(id));
  if (project) workspaceId.value = project.workspaceId;
});
const activeDrawer = inject('activeDrawer', ref('work'));
watch(activeDrawer, value => { if (value === 'work') void controller.resume(); });
function onForeground() { if (document.visibilityState === 'visible' && activeDrawer.value === 'work') void controller.resume(); }
onMounted(() => { if (activeDrawer.value === 'work') void controller.resume(); document.addEventListener('visibilitychange', onForeground); });
onBeforeUnmount(() => document.removeEventListener('visibilitychange', onForeground));
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
.remote-toolbar select { flex: 1; min-width: 140px; width: auto; }
.remote-review { padding: 12px; border: 1px solid var(--color-warning); border-radius: 10px; }
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
.remote-controls { margin-top: 16px; border-top: 1px solid var(--border-color); }
.remote-actions { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 12px; }
.remote-composer label { display: grid; gap: 8px; }
textarea { box-sizing: border-box; width: 100%; resize: vertical; min-height: 80px; font: inherit; font-size: 16px; padding: 12px; border: 1px solid var(--border-color); border-radius: 12px; background: var(--bg-primary); color: var(--text-primary); }
fieldset { min-width: 0; border: 1px solid var(--border-color); border-radius: 8px; margin-block: 12px; }
.remote-option { display: flex; align-items: center; min-height: 44px; gap: 8px; }
.remote-option input { width: 20px; min-height: 20px; accent-color: var(--accent-primary); }
.remote-detail { white-space: pre-wrap; overflow-wrap: anywhere; }
@media (max-width: 720px) { .remote-workspace { grid-template-columns: minmax(0, 1fr); } .remote-panel { padding: 12px; } }
</style>
