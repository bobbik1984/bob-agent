<script setup>
import { computed, inject, onUnmounted, ref, watch } from 'vue';
import DshSessions from './DshSessions.vue';
import {createChatScroll} from '../chat-scroll.mjs';
import MessageContent from './MessageContent.vue';
import { TERMINAL_ROD_FRAMES } from '../thinking-process.mjs';
import JarvisOrb from './JarvisOrb.vue';
import ActionCard from './ActionCard.vue';
import JarvisWidgetGrid from './JarvisWidgetGrid.vue';
import FeatureTrialBanner from './FeatureTrialBanner.vue';
import VirtualBrowserModal from './VirtualBrowserModal.vue';

const {
  t,
  brandLogo,
  getProviderLogo,
  canCancelExecution,
  remoteConversation,
  activeDrawer,
  settingsOpen,
  modelHub,
  readiness,
  executionEnabled,
  activeConversationItem,
  fault,
  sessionId,
  assistantConversationId,
  jarvisHome,
  reload,
  startup,
  homeFault,
  executionFault,
  historyRegion,
  loading,
  onHistoryScroll,
  messages,
  directProviderEnabled,
  homeConfigured,
  assistantLink,
  openAssistantTarget,
  assistantPollFailed,
  formatMessageTime,
  copyReply,
  copyState,
  messageStatus,
  messageExecution,
  cancelPending,
  cancel,
  send,
  quickSwitcherOpen,
  quickPopoverRef,
  assistantAutoRouting,
  toggleAutoRouting,
  quickSwitcherSearchQuery,
  discoveringModels,
  discoverLatestModels,
  quickSwitcherProviderId,
  selectorProviders,
  isKeyConfigured,
  quickSwitcherModels,
  activeMainRole,
  selectRoleModel,
  quickSwitcherProvider,
  quickSwitcherHiddenMatches,
  input,
  draft,
  updateDraft,
  focusComposer,
  keydown,
  quickPillRef,
  dispatchPending,
  saving,
  executionBlocked,
  activeExecution,
  canSend,
  composerAttachments,
  composerFileInput,
  openComposerFilePicker,
  handleComposerFiles,
  removeComposerAttachment,
  historyPullUpDistance,
  onHistoryTouchStart,
  onHistoryTouchMove,
  onHistoryTouchEnd,
  onHistoryWheel,
} = inject('bobAppShell');

const awayFromLatest=ref(false),chatContent=ref(null);
const chatScroll=createChatScroll(value=>{awayFromLatest.value=value;});
watch([historyRegion,chatContent],()=>chatScroll.bind(historyRegion.value),{flush:'post'});
watch(sessionId,()=>chatScroll.reset());
function onChatScroll(){chatScroll.scroll();onHistoryScroll();}
function jumpLatest(){chatScroll.bottom();}
onUnmounted(()=>chatScroll.dispose());
const rodIdx = ref(0);
let rodTimer = null;
const isBusy = computed(() => Boolean(loading.value || dispatchPending.value || saving.value || activeExecution.value));
const rodFrame = computed(() => `\u2060${TERMINAL_ROD_FRAMES[rodIdx.value % TERMINAL_ROD_FRAMES.length]}\u2060`);
const stripDots = s => String(s ?? '').replace(/\s*(?:…|\.{2,})\s*$/, '');
const streamingMsgId = ref(null);
let initializedSessionId = null;
watch(() => [sessionId.value, (messages.value || []).length], ([sid]) => {
  const list = messages.value || [];
  const last = list[list.length - 1];
  if (sid !== initializedSessionId) {
    initializedSessionId = sid;
    streamingMsgId.value = null;
    return;
  }
  if (last && last.role === 'assistant') {
    streamingMsgId.value = last.id;
  }
}, { immediate: true });
watch(isBusy, val => {
  if (typeof setInterval !== 'function') return;
  if (val && !rodTimer) {
    rodTimer = setInterval(() => { rodIdx.value = (rodIdx.value + 1) % TERMINAL_ROD_FRAMES.length; }, 120);
  } else if (!val && rodTimer) {
    clearInterval(rodTimer);
    rodTimer = null;
  }
}, { immediate: true });
onUnmounted(() => { if (rodTimer) clearInterval(rodTimer); });

const jarvisOrbMood = computed(() => jarvisHome?.orbMood?.value || 'idle');
const pendingApprovalCount = computed(() => jarvisHome?.pendingApprovalCount?.value || 0);
const isMainJarvisSession = computed(() => !sessionId.value || sessionId.value === assistantConversationId?.value);
</script>

<template>
    <!-- Primary View 1: 对话 (Chat) -->
    <div v-show="remoteConversation && activeDrawer === 'chat' && !settingsOpen" class="remote-session-host"><DshSessions v-if="remoteConversation" :key="remoteConversation.node.id + remoteConversation.session.id" :target="remoteConversation" :nodes="modelHub.state.remote_nodes || []" /></div>
    <main v-show="activeDrawer === 'chat' && !settingsOpen && !remoteConversation" class="main">
      <!-- Jarvis Sticky Bar & Widget Sheet when conversation has messages -->
      <div v-if="messages?.length && isMainJarvisSession" class="jarvis-sticky-bar">
        <JarvisOrb
          size="compact"
          :mood="jarvisOrbMood"
          :badge-count="pendingApprovalCount"
          :interactive="true"
          @activate="jarvisHome && (jarvisHome.widgetSheetOpen.value = !jarvisHome.widgetSheetOpen.value)"
        />
        <button
          type="button"
          class="jarvis-sheet-toggle-btn"
          :aria-expanded="jarvisHome?.widgetSheetOpen?.value"
          @click="jarvisHome && (jarvisHome.widgetSheetOpen.value = !jarvisHome.widgetSheetOpen.value)"
        >
          <span>{{ t(jarvisHome?.widgetSheetOpen?.value ? 'jarvisCloseWidgetsSheet' : 'jarvisOpenWidgetsSheet') }}</span>
          <svg class="chevron-svg" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2">
            <path :d="jarvisHome?.widgetSheetOpen?.value ? 'm18 15-6-6-6 6' : 'm6 9 6 6 6-6'" />
          </svg>
        </button>
      </div>
      <div v-if="messages?.length && isMainJarvisSession && jarvisHome?.widgetSheetOpen?.value" class="jarvis-widget-sheet">
        <JarvisWidgetGrid />
      </div>
      <div
        v-if="readiness && readiness.pending_messages > 0"
        class="connection-note"
        role="status"
      >
        <span class="storage-summary">
          {{
            t(executionEnabled ? 'localExecutionStatus' : 'localStatus', {
              saved: readiness.saved_messages,
              pending: readiness.pending_messages,
            })
          }}
        </span>
      </div>
      <div
        v-if="activeConversationItem && !activeConversationItem.target_device_online"
        class="notice"
        role="status"
      >
        <span>{{ activeConversationItem.status_reason || t('offlineImmediateDisabled') }}</span>
      </div>
      <div v-if="fault" class="notice" role="alert">
        <span>{{ t(fault) }}</span>
        <button type="button" @click="sessionId ? reload() : startup()">{{ t('reload') }}</button>
      </div>
      <div v-if="homeFault" class="notice" role="status">{{ t('homeSyncUnconfirmed') }}</div>
      <div v-if="executionFault" class="notice" role="alert">
        <span>{{ t(executionFault) }}</span>
        <button type="button" @click="reload">{{ t('checkStatus') }}</button>
      </div>
      <section
        ref="historyRegion"
        class="history"
        tabindex="0"
        :aria-label="t('conversation')"
        aria-live="polite"
        :aria-busy="loading"
        @scroll.passive="onChatScroll"
        @touchstart.passive="onHistoryTouchStart"
        @touchmove.passive="onHistoryTouchMove"
        @touchend.passive="onHistoryTouchEnd"
        @touchcancel.passive="onHistoryTouchEnd"
        @wheel.passive="onHistoryWheel"
      >
        <div ref="chatContent" data-chat-content>
        <div v-if="loading && !messages.length" class="empty loading-brand">
          <img :src="brandLogo" :alt="t('brand')" />
          <p>{{ t('loading') }}</p>
        </div>
        <div v-else-if="!messages.length" class="empty jarvis-home-stage">
          <JarvisOrb
            size="hero"
            :mood="jarvisOrbMood"
            :badge-count="pendingApprovalCount"
            @activate="focusComposer"
          />
          <h1>{{ t('startTitle') }}</h1>
          <p>
            {{
              t(
                directProviderEnabled
                  ? 'startDirectHint'
                  : homeConfigured
                    ? 'homeStartHint'
                    : executionEnabled
                      ? 'startExecutionHint'
                      : 'startHint',
              )
            }}
          </p>
          <JarvisWidgetGrid />
        </div>
        <article
          v-for="(message, msgIdx) in messages"
          :key="message.id" :data-chat-anchor="message.id"
          class="message"
          :class="{ 'assistant-message': message.role === 'assistant' }"
        >
          <span v-if="message.role === 'assistant'" class="message-state">
            {{
              t(
                message.source === 'native_session_result' ? 'assistantNativeReply' : ['direct_provider', 'replica_direct_provider'].includes(message.source)
                  ? 'assistantDirectReply'
                  : ['codex_execution', 'home_codex_execution'].includes(message.source)
                    ? 'assistantLiveReply'
                    : 'assistantReply',
              )
            }}
          </span>
          <span v-if="message.source === 'native_session_result'" class="message-state">{{ t('assistantNativeResult') }}</span>
          <MessageContent :content="message.content" :plain="message.role === 'user'" :stream="message.role === 'assistant' && msgIdx === messages.length - 1 && message.id === streamingMsgId" />
          <div v-if="jarvisHome?.cardsForMessage(message)?.length" class="message-action-cards">
            <ActionCard
              v-for="c in jarvisHome.cardsForMessage(message)"
              :key="c.id"
              :card="c"
              @approve="jarvisHome.approveAction"
              @reject="jarvisHome.rejectAction"
              @open-takeover="jarvisHome.openTakeover"
              @save-routine="jarvisHome.saveRoutine"
            />
          </div>
          <div v-if="assistantLink(message)" class="message-state" role="status">
            <span>{{ t('assistantDispatch_' + assistantLink(message).status) }}</span>
            <button type="button" @click="openAssistantTarget(assistantLink(message))">{{ assistantLink(message).target_title }} · {{ t('assistantOpenOriginal') }}</button>
            <span v-if="assistantPollFailed">{{ t('assistantResultReadFailed') }}</span>
          </div>
          <div class="message-meta-bar">
            <time
              v-if="formatMessageTime(message.created_at || message.createdAt || message.timestamp)"
              class="message-state message-time"
            >
              {{ formatMessageTime(message.created_at || message.createdAt || message.timestamp) }}
            </time>
            <button
              type="button"
              class="copy-reply"
              @click="copyReply(message)"
            >
              {{ t(copyState[message.id] || (message.role === 'assistant' ? 'copyReply' : 'copyMessage')) }}
            </button>
            <span class="message-state">{{ t(messageStatus(message)) }}</span>
          </div>
          <button
            v-if="canCancelExecution(messageExecution(message))"
            type="button"
            class="cancel-execution"
            :disabled="cancelPending.has(messageExecution(message).id)"
            @click="cancel(messageExecution(message))"
          >
            {{
              t(
                cancelPending.has(messageExecution(message).id)
                  ? 'requestingStop'
                  : 'stopExecution',
              )
            }}
          </button>
          <span v-if="messageExecution(message)?.status === 'unknown'" class="message-state">
            {{ t('noAutomaticReplay') }}
          </span>
        </article>
        <article
          v-if="jarvisHome?.jarvisStreaming?.value && jarvisHome?.streamingSessionId?.value === sessionId && jarvisHome?.streamingReplyText?.value"
          class="message assistant-message streaming-live-bubble"
        >
          <span class="message-state">{{ t('assistantReply') }}</span>
          <MessageContent :content="jarvisHome.streamingReplyText.value" :stream="true" />
        </article>
        <div v-if="jarvisHome?.pendingApprovals?.value?.length" class="pending-approvals-stream">
          <ActionCard
            v-for="card in jarvisHome.pendingApprovals.value"
            :key="card.id"
            :card="card"
            @approve="jarvisHome.approveAction"
            @reject="jarvisHome.rejectAction"
            @open-takeover="jarvisHome.openTakeover"
            @save-routine="jarvisHome.saveRoutine"
          />
        </div>
        <div v-if="dispatchPending || saving || activeExecution" class="live-status-container" role="status" aria-live="polite">
          <div v-if="isMainJarvisSession" class="jarvis-live-status-card">
            <JarvisOrb size="compact" :mood="jarvisOrbMood" :interactive="false" />
            <span class="jarvis-live-label">{{ stripDots(t(dispatchPending ? 'requestingExecution' : saving ? 'saving' : 'agentLiveThinking')) }}</span>
          </div>
          <div v-else class="dsh-live-terminal-card">
            <span class="terminal-rod" aria-hidden="true">[{{ rodFrame }}]</span>
            <span>{{ stripDots(t(dispatchPending ? 'requestingExecution' : saving ? 'saving' : 'agentLiveThinking')) }}</span>
            <span class="terminal-dots" aria-hidden="true"><span>.</span><span>.</span><span>.</span></span>
          </div>
        </div>
        <div v-if="historyPullUpDistance > 10" class="pull-up-refresh-hint" :class="{ ready: historyPullUpDistance >= 36 }" role="status" aria-live="polite">
          <span class="terminal-rod" aria-hidden="true">[{{ rodFrame }}]</span>
          <span>{{ t(historyPullUpDistance >= 36 ? 'releaseToRefresh' : 'pullUpToRefresh') }}</span>
        </div>
        </div>
      </section>
      <div v-if="awayFromLatest" class="chat-jump-row"><button type="button" class="chat-jump-latest" @click="jumpLatest" :aria-label="t('chatJumpLatest')" :title="t('chatJumpLatest')"><svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M12 4v16m-6-6 6 6 6-6"/></svg></button></div>

      <!-- Persistent 3-Minute Trial Bar above composer during active chat -->
      <FeatureTrialBanner v-if="messages?.length" />

      <form class="composer" @submit.prevent="send">
        <!-- Floating Quick Model Switcher Popover (Anchored cleanly above .composer) -->
        <div
          v-if="quickSwitcherOpen"
          ref="quickPopoverRef"
          class="quick-model-popover"
          @click.stop
        >
          <div class="quick-auto-routing-bar">
            <button
              type="button"
              class="quick-auto-btn"
              :class="{ active: assistantAutoRouting }"
              @click="toggleAutoRouting(); if (assistantAutoRouting) quickSwitcherOpen = false;"
            >
              <div class="auto-btn-content">
                <span class="auto-btn-title">
                  <svg class="inline-svg-xs" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"></polygon>
                  </svg>
                  {{ t('modelAutoRoute') }}
                </span>
                <span class="auto-btn-desc">{{ t('modelAutoRouteDesc') }}</span>
              </div>
              <span v-if="assistantAutoRouting" class="auto-active-badge">{{ t('enabled') }}</span>
            </button>
          </div>
          <div class="quick-popover-header">
            <input
              v-model="quickSwitcherSearchQuery"
              type="search"
              class="quick-model-search"
              :placeholder="t('quickModelFilterPlaceholder')"
            />
            <button
              type="button"
              class="mini-ghost-btn"
              :disabled="discoveringModels"
              @click="discoverLatestModels(quickSwitcherProviderId)"
            >
              {{ discoveringModels ? t('discoveringModelsBtn') : t('discoverLatestModelsBtn') }}
            </button>
          </div>
          <div class="quick-popover-columns">
            <div class="quick-provider-col">
              <button
                v-for="prov in selectorProviders"
                :key="prov.id"
                type="button"
                class="quick-provider-item"
                :class="{ active: quickSwitcherProviderId === prov.id }"
                @click="quickSwitcherProviderId = prov.id"
              >
                <img :src="getProviderLogo(prov.id)" alt="" class="chip-logo" />
                <span>{{ prov.name }}</span>
                <span
                  v-if="isKeyConfigured(prov.id)"
                  class="key-status-dot configured"
                ></span>
              </button>
            </div>
            <div class="quick-model-col">
              <button
                v-for="mdl in quickSwitcherModels"
                :key="mdl.id"
                type="button"
                class="quick-model-item"
                :class="{
                  selected:
                    activeMainRole.model_id === mdl.id &&
                    activeMainRole.provider_id === quickSwitcherProviderId,
                }"
                @click="selectRoleModel('main', quickSwitcherProvider, mdl)"
              >
                <strong class="quick-model-title-row">
                  <span>{{ mdl.name || mdl.id }}</span>
                  <span v-if="mdl.discovered" class="model-tag-pill">{{
                    t('discoveredModelBadge')
                  }}</span>
                </strong>
                <small>{{ mdl.id }}</small>
              </button>
              <template v-if="quickSwitcherHiddenMatches.length">
                <div class="catalog-matches-label">{{ t('catalogHiddenMatchesLabel') }}</div>
                <button
                  v-for="mdl in quickSwitcherHiddenMatches"
                  :key="`hidden-${mdl.id}`"
                  type="button"
                  class="quick-model-item catalog-hidden-chip"
                  @click="selectRoleModel('main', quickSwitcherProvider, mdl)"
                >
                  <strong class="quick-model-title-row">
                    <span>{{ mdl.name || mdl.id }}</span>
                    <span class="model-tag-pill">{{ t('enableCatalogModelBadge') }}</span>
                  </strong>
                  <small>{{ mdl.id }}</small>
                </button>
              </template>
            </div>
          </div>
        </div>

        <label class="visually-hidden" for="message-input">{{ t('message') }}</label>
        <textarea
          id="message-input"
          ref="input"
          :value="draft"
          :disabled="!sessionId"
          :placeholder="t('placeholder')"
          rows="2"
          @input="updateDraft"
          @focus="focusComposer"
          @keydown="keydown"
        />
        <div v-if="composerAttachments.length" class="composer-attachments">
          <span v-for="att in composerAttachments" :key="att.id" class="composer-attachment-chip">
            <span class="chip-file-name">{{ att.name }}</span>
            <button type="button" class="chip-remove-btn" :aria-label="t('removeAttachment')" @click="removeComposerAttachment(att.id)">×</button>
          </span>
        </div>
        <div class="composer-actions">
          <div class="composer-left-tools">
            <input ref="composerFileInput" type="file" multiple class="visually-hidden" @change="handleComposerFiles" />
            <button
              type="button"
              class="composer-tool-btn"
              :disabled="!sessionId || saving"
              :title="t('attachFileBtn')"
              :aria-label="t('attachFileBtn')"
              @click="openComposerFilePicker"
            >
              <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48" />
              </svg>
            </button>
            <button
              ref="quickPillRef"
              type="button"
              class="composer-model-pill"
              :aria-label="t('quickSwitchModel')"
              :aria-expanded="quickSwitcherOpen"
              @click.stop="
                quickSwitcherProviderId = activeMainRole.provider_id || 'deepseek';
                quickSwitcherOpen = !quickSwitcherOpen;
                if (!quickSwitcherOpen) quickSwitcherSearchQuery = '';
              "
            >
              <template v-if="assistantAutoRouting">
                <svg class="inline-svg-xs" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"></polygon>
                </svg>
                <span class="pill-model-name">{{ t('modelModePillAuto') }}</span>
                <span class="pill-auto-tag">{{ activeMainRole.model_name || activeMainRole.model_id }}</span>
              </template>
              <template v-else>
                <img
                  :src="getProviderLogo(activeMainRole.provider_id)"
                  alt=""
                  class="pill-logo"
                />
                <span class="pill-model-name">
                  {{ activeMainRole.model_name || activeMainRole.model_id }}
                </span>
              </template>
              <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
                <path d="m6 9 6 6 6-6" />
              </svg>
            </button>
            <span class="composer-hint-text">
              {{
                t(
                  dispatchPending
                    ? 'requestingExecution'
                    : saving
                      ? 'saving'
                      : executionBlocked
                        ? 'executionBusyHint'
                        : homeConfigured
                          ? 'homeSendHint'
                          : executionEnabled
                            ? 'executeHint'
                            : 'localFirstHint',
                )
              }}
            </span>
          </div>
          <button
            v-if="activeExecution"
            type="button"
            class="composer-submit-btn"
            :disabled="cancelPending.has(activeExecution.id)"
            @click="cancel(activeExecution)"
          >
            {{ t(cancelPending.has(activeExecution.id) ? 'requestingStop' : 'stopExecution') }}
          </button>
          <button v-else type="submit" class="composer-submit-btn" :disabled="!canSend">
            {{ t('send') }}
          </button>
        </div>
      </form>
    </main>
    <VirtualBrowserModal />
</template>

<style scoped>
.remote-session-host {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  min-width: 0;
  width: 100%;
  margin: 0 auto;
  position: relative;
  overflow: hidden;
}
</style>

<style scoped>
.chat-jump-row{height:0;position:relative;z-index:2;display:flex;justify-content:flex-end;padding:0 20px;transform:translateY(-46px)}
.chat-jump-latest{display:grid;place-items:center;width:36px;height:36px;border-radius:50%;border:1px solid var(--border-color);background:var(--bg-secondary);color:var(--accent-primary);cursor:pointer}

.jarvis-home-stage {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  padding: var(--gap-lg) var(--gap-md);
  gap: var(--gap-sm);
  width: 100%;
  max-width: 900px;
  margin: 0 auto;
}

.jarvis-sticky-bar {
  position: sticky;
  top: 0;
  z-index: 3;
  display: flex;
  align-items: center;
  gap: var(--gap-sm);
  padding: var(--gap-2xs) var(--gap-md);
  background: var(--bg-primary);
  border-bottom: 1px solid var(--line);
  backdrop-filter: blur(8px);
}

.jarvis-sheet-toggle-btn {
  display: inline-flex;
  align-items: center;
  gap: var(--gap-3xs);
  background: var(--bg-secondary);
  border: 1px solid var(--line);
  border-radius: var(--radius-pill);
  padding: 0.2rem 0.6rem;
  font-size: var(--text-xs);
  color: var(--text-secondary);
  cursor: pointer;
  transition: color 0.15s ease, background 0.15s ease;
}

.jarvis-sheet-toggle-btn:hover {
  background: var(--bg-tertiary);
  color: var(--text-primary);
}

.jarvis-widget-sheet {
  padding: var(--gap-sm) var(--gap-md);
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--line);
  max-height: 50vh;
  overflow-y: auto;
}

.message-action-cards,
.pending-approvals-stream {
  display: flex;
  flex-direction: column;
  gap: var(--gap-sm);
  margin-top: var(--gap-xs);
  width: 100%;
}

.streaming-live-bubble {
  border-left: 3px solid var(--accent-primary);
  background: var(--bg-secondary);
}

.live-status-container {
  display: flex;
  align-items: center;
  margin: var(--gap-xs) 0;
}

.jarvis-live-status-card {
  display: inline-flex;
  align-items: center;
  gap: var(--gap-sm);
  padding: 0.35rem 0.75rem;
  background: var(--bg-secondary);
  border: 1px solid var(--line);
  border-radius: var(--radius-pill);
}

.jarvis-live-label {
  font-size: var(--text-xs);
  color: var(--text-secondary);
}
</style>
