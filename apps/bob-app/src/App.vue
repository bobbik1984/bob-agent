<script setup>
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import HomeConnection from './HomeConnection.vue';
import { bindViewport } from './viewport.mjs';
import brandLogo from './assets/bob-logo.svg';
import { useI18n } from 'vue-i18n';
import { kernelBridge } from './native-bridge.mjs';
import { createSendController } from './send-controller.mjs';
import { createExecutionController, canCancelExecution, executionStatusKey, singleFlight, blocksNewExecution } from './execution-controller.mjs';
const { t, locale } = useI18n();
const sessionId = ref(null);
const messages = ref([]);
const readiness = ref(null);
const executions = ref([]);
const executionFault = ref(null);
const homeFault = ref(false);
const cancelPending = ref(new Set());
const dispatchPending = ref(false);
let poll = null;
let disposed = false;
let releaseViewport = () => {};
const locallyAccepted = new Map();
let unconfirmedRequest = null;
const executionController = createExecutionController(kernelBridge);
const executionEnabled = computed(() => readiness.value?.mode === 'codex_readonly' && readiness.value?.execution_available === true);
const homeConfigured = computed(() => readiness.value?.mode === 'home_configured');
const sendLabel = computed(() => executionEnabled.value || homeConfigured.value);
const byMessage = computed(() => new Map(executions.value.map(run => [run.message_id, run])));
function messageExecution(message) { return byMessage.value.get(message.id) || executions.value.find(run => run.request_id === message.request_id && !!message.request_id); }
function messageStatus(message) {
  if (message.role === 'assistant') return ['codex_execution', 'home_codex_execution'].includes(message.source) ? 'replySaved' : 'probeReplySaved';
  return executionStatusKey(messageExecution(message)) || statusKeys[message.send_state] || 'unknown';
}
const connectionKey = computed(() => !available ? 'nativeRequired' : executionEnabled.value ? 'codexReadOnlyReady' : homeConfigured.value ? 'homeConfiguredHint' : readiness.value ? 'providerUnconfigured' : 'loading');
const settingsOpen = ref(false);
const navigationOpen = ref(false);
const devicePanel = ref(null);
const deviceCapabilities = ref(null);
const currentConversation = computed(() => messages.value.find(m => m.role === 'user')?.content?.slice(0, 60) || t('conversation'));
async function openDevices() { settingsOpen.value = true; await nextTick(); await devicePanel.value?.openDetails(); }

const settingsPanel = ref(null);
let settingsTrigger = null;
watch(settingsOpen, async open => {
  if (open) settingsTrigger = document.activeElement;
  await nextTick();
  if (open) settingsPanel.value?.querySelector('button')?.focus();
  else settingsTrigger?.focus();
});
function settingsKeydown(event) {
  if (event.key === 'Escape') { event.preventDefault(); settingsOpen.value = false; return; }
  if (event.key !== 'Tab') return;
  const controls = [...settingsPanel.value.querySelectorAll('button,select,input,textarea,summary,[tabindex="0"]')].filter(el => !el.disabled && el.getClientRects().length);
  const first = controls[0], last = controls.at(-1);
  if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
  else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
}
const copyState = ref({});
async function copyReply(message) {
  try { await navigator.clipboard.writeText(message.content); copyState.value = {...copyState.value, [message.id]: 'copied'}; }
  catch { copyState.value = {...copyState.value, [message.id]: 'copyFailed'}; }
}
const draft = ref('');
const saving = ref(false);
const loading = ref(true);
const fault = ref(null);
const input = ref(null);
const historyRegion = ref(null);
const available = kernelBridge.available();
const executionBlocked = computed(() => blocksNewExecution(executions.value));
const activeExecution = computed(() => executions.value.find(run => canCancelExecution(run)));
const canSend = computed(() => !!sessionId.value && !!draft.value.trim() && !loading.value && !saving.value && !executionBlocked.value);
watch(locale, value => { document.documentElement.lang = value; });
const controller = createSendController({ bridge: kernelBridge,
  createIntent: content => ({request_id: crypto.randomUUID(), message_id: crypto.randomUUID(), session_id: sessionId.value, content}),
  onAccepted: () => {},
});
function updateDraft(event) {
  draft.value = event.target.value;
  controller.setDraft(draft.value);
  if (sessionId.value) void controller.prewarm();
}
function focusComposer() { if (sessionId.value) void controller.prewarm(); }
function nearBottom() {
  const region = historyRegion.value;
  return !region || region.scrollHeight - region.scrollTop - region.clientHeight < 48;
}
const reload = singleFlight(async () => {
  if (!sessionId.value || disposed) return;
  void kernelBridge.syncHome().then(status => { if (status !== 'sync_in_progress') homeFault.value = false; }).catch(() => { homeFault.value = true; });
  const results = await Promise.allSettled([
    kernelBridge.history(sessionId.value), kernelBridge.readiness(), kernelBridge.executions(sessionId.value),
  ]);
  if (disposed) return;
  // Check position after I/O, so a user who scrolls while polling is not pulled back.
  const follow = nearBottom();
  if (results[0].status === 'fulfilled') {
    const stored = results[0].value;
    for (const message of stored) locallyAccepted.delete(message.id);
    messages.value = [...stored, ...locallyAccepted.values()];
    fault.value = null;
  }
  else fault.value = 'historyFailed';
  if (results[1].status === 'fulfilled') readiness.value = results[1].value;
  else { readiness.value = null; fault.value ||= 'statusFailed'; }
  if (results[2].status === 'fulfilled') {
    executions.value = results[2].value;
    if (unconfirmedRequest && executions.value.some(run => run.request_id === unconfirmedRequest)) {
      unconfirmedRequest = null; executionFault.value = null;
    }
  }
  else fault.value ||= 'executionReadFailed';
  await nextTick();
  if (follow && historyRegion.value) historyRegion.value.scrollTop = historyRegion.value.scrollHeight;
});
async function cancel(execution) {
  if (!canCancelExecution(execution) || cancelPending.value.has(execution.id)) return;
  cancelPending.value = new Set([...cancelPending.value, execution.id]);
  try {
    const receipt = await executionController.cancel(execution);
    executions.value = executions.value.map(run => run.id === receipt.id ? receipt : run);
    executionFault.value = null;
  } catch { executionFault.value = 'cancelUnconfirmed'; }
  finally {
    const pending = new Set(cancelPending.value); pending.delete(execution.id); cancelPending.value = pending;
    await reload();
  }
}
async function focusPairedComposer(){await reload();await nextTick();input.value?.focus();}
async function startup() {
  loading.value = true; fault.value = null;
  if (!available) { loading.value = false; return; }
  try {
    const state = await kernelBridge.bootstrap();
    sessionId.value = state.session_id;
    await reload();
  } catch { fault.value = 'startupFailed'; }
  finally { loading.value = false; }
}
async function send() {
  if (!canSend.value) return;
  saving.value = true; fault.value = null;
  const sentText = draft.value;
  const shouldExecute = executionEnabled.value;
  try {
    const receipt = await controller.send();
    if (receipt && !messages.value.some(message => message.id === receipt.message_id)) {
      const message = { id: receipt.message_id, request_id: receipt.request_id, role: 'user', content: sentText, send_state: 'local_saved' };
      locallyAccepted.set(message.id, message);
      messages.value.push(message);
    }
    draft.value = controller.snapshot().draft;
    await nextTick();
    input.value?.focus();
    if (receipt && shouldExecute) {
      dispatchPending.value = true;
      executionFault.value = null;
      try {
        const execution = await executionController.dispatch(receipt);
        executions.value = [...executions.value.filter(run => run.id !== execution.id), execution];
      } catch { unconfirmedRequest = receipt.request_id; executionFault.value = 'executionStartUnconfirmed'; }
      finally { dispatchPending.value = false; }
    }
    await reload();
  } catch { draft.value = controller.snapshot().draft; fault.value = 'saveFailed'; }
  finally { saving.value = false; }
}
function keydown(event) {
  if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) { event.preventDefault(); void send(); }
}
const statusKeys = { reply_saved:'probeReplySaved', local_saved:'localSaved',queued_for_wakeup:'waitingHome',not_dispatched:'waitingHome',dispatching:'sending',dispatched:'waitingReceipt',unknown_send:'unknown',peer_accepted:'homeReceived',rejected:'rejected',cancelled_local:'cancelled',needs_attention:'attention' };
onMounted(async () => {
  releaseViewport = bindViewport(window, document.documentElement);
  await startup();
  if (available) deviceCapabilities.value = await kernelBridge.pairingCapabilities().catch(() => null);
  if (!disposed && available) poll = setInterval(() => { void reload(); }, 1500);
});
onUnmounted(() => { disposed = true; releaseViewport(); if (poll) clearInterval(poll); });
</script>
<template>
  <div class="app-shell">
    <header class="topbar">
      <button type="button" class="icon-button" :aria-label="t('openNavigation')" :aria-expanded="navigationOpen" @click="navigationOpen=true">☰</button>
      <span class="page-name">{{ t('conversation') }}</span>
      <button type="button" class="device-indicator" :class="{paired:homeConfigured,unconfirmed:homeFault}" :aria-label="t(homeFault ? 'deviceStatusUnknown' : homeConfigured ? 'devicePairedDetails' : 'connectDeviceDetails')" @click="openDevices">
        <svg v-if="deviceCapabilities?.can_scan" viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="3" width="18" height="13" rx="2"/><path d="M8 21h8M12 16v5"/></svg>
        <svg v-else viewBox="0 0 24 24" aria-hidden="true"><rect x="6" y="2" width="12" height="20" rx="2"/><path d="M10 18h4"/></svg>
        <span class="device-dot" aria-hidden="true"></span>
      </button>
      <button type="button" class="icon-button settings-toggle" :aria-label="t('settings')" :aria-expanded="settingsOpen" @click="settingsOpen=!settingsOpen">⋯</button>
    </header>
    <section v-show="settingsOpen" ref="settingsPanel" class="settings-panel" role="dialog" aria-modal="true" :aria-label="t('settings')" @keydown="settingsKeydown">
      <div class="settings-heading"><h2>{{ t('settings') }}</h2><button type="button" :aria-label="t('closeSettings')" @click="settingsOpen=false">×</button></div>
      <label class="language"><span class="visually-hidden">{{ t('language') }}</span><select v-model="locale"><option value="zh-CN">{{ t('chinese') }}</option><option value="en-US">{{ t('english') }}</option></select></label>
      <HomeConnection ref="devicePanel" v-if="sessionId" :paired="homeConfigured" @changed="reload" @connected="settingsOpen=false;focusPairedComposer()" />
    </section>
    <div v-if="navigationOpen" class="navigation-overlay" @keydown.esc="navigationOpen=false">
      <button type="button" class="navigation-backdrop" :aria-label="t('closeNavigation')" @click="navigationOpen=false"></button>
      <nav class="navigation-drawer" :aria-label="t('conversations')">
        <div class="settings-heading"><h2>{{ t('conversations') }}</h2><button type="button" :aria-label="t('closeNavigation')" @click="navigationOpen=false">×</button></div>
        <button type="button" class="conversation-link" aria-current="page" @click="navigationOpen=false">{{ currentConversation }}</button>
        <h2>{{ t('projects') }}</h2><p class="navigation-note">{{ t('projectsNotConnected') }}</p>
      </nav>
    </div>
    <main class="main">
      <div v-if="!homeConfigured || readiness?.pending_messages > 0" class="connection-note" :class="{'home-paired':homeConfigured}" role="status">
        <span>{{ t(connectionKey) }}</span>
        <span v-if="readiness && readiness.pending_messages > 0" class="storage-summary">{{ t(executionEnabled ? 'localExecutionStatus' : 'localStatus', { saved: readiness.saved_messages, pending: readiness.pending_messages }) }}</span>
      </div>
      <div v-if="fault" class="notice" role="alert"><span>{{ t(fault) }}</span><button type="button" @click="sessionId ? reload() : startup()">{{ t('reload') }}</button></div>
      <div v-if="homeFault" class="notice" role="status">{{ t('homeSyncUnconfirmed') }}</div>
      <div v-if="executionFault" class="notice" role="alert"><span>{{ t(executionFault) }}</span><button type="button" @click="reload">{{ t('checkStatus') }}</button></div>
      <section ref="historyRegion" class="history" :aria-label="t('conversation')" aria-live="polite" :aria-busy="loading">
        <div v-if="loading" class="empty loading-brand"><img :src="brandLogo" :alt="t('brand')" /><p>{{ t('loading') }}</p></div>
        <div v-else-if="!messages.length" class="empty"><h1>{{ t('startTitle') }}</h1><p>{{ t(homeConfigured ? 'homeStartHint' : executionEnabled ? 'startExecutionHint' : 'startHint') }}</p></div>
        <article v-for="message in messages" :key="message.id" class="message" :class="{ 'assistant-message': message.role === 'assistant' }">
          <span v-if="message.role === 'assistant'" class="message-state">{{ t(['codex_execution', 'home_codex_execution'].includes(message.source) ? 'assistantLiveReply' : 'assistantReply') }}</span>
          <p class="message-body">{{ message.content }}</p>
          <button v-if="message.role === 'assistant'" type="button" class="copy-reply" @click="copyReply(message)">{{ t(copyState[message.id] || 'copyReply') }}</button>
          <span class="message-state">{{ t(messageStatus(message)) }}</span>
          <button v-if="canCancelExecution(messageExecution(message))" type="button" class="cancel-execution" :disabled="cancelPending.has(messageExecution(message).id)" @click="cancel(messageExecution(message))">{{ t(cancelPending.has(messageExecution(message).id) ? 'requestingStop' : 'stopExecution') }}</button>
          <span v-if="messageExecution(message)?.status === 'unknown'" class="message-state">{{ t('noAutomaticReplay') }}</span>
        </article>
      </section>
      <form class="composer" @submit.prevent="send">
        <label class="visually-hidden" for="message-input">{{ t('message') }}</label>
        <textarea id="message-input" ref="input" :value="draft" :disabled="!sessionId" :placeholder="t('placeholder')" rows="2" @input="updateDraft" @focus="focusComposer" @keydown="keydown" />
        <div class="composer-actions"><span>{{ t(dispatchPending ? 'requestingExecution' : saving ? 'saving' : executionBlocked ? 'executionBusyHint' : homeConfigured ? 'homeSendHint' : executionEnabled ? 'executeHint' : 'localFirstHint') }}</span><button v-if="activeExecution" type="button" :disabled="cancelPending.has(activeExecution.id)" @click="cancel(activeExecution)">{{ t(cancelPending.has(activeExecution.id) ? 'requestingStop' : 'stopExecution') }}</button><button v-else type="submit" :disabled="!canSend">{{ t(sendLabel ? 'sendAndExecute' : 'send') }}</button></div>
      </form>
    </main>
  </div>
</template>
