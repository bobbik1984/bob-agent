<script setup>
import { computed, inject, onUnmounted, ref, watch } from 'vue';
import DshSessions from './DshSessions.vue';
import MessageContent from './MessageContent.vue';
import { TERMINAL_ROD_FRAMES } from '../thinking-process.mjs';

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
} = inject('bobAppShell');

const rodIdx = ref(0);
let rodTimer = null;
const isBusy = computed(() => Boolean(loading.value || dispatchPending.value || saving.value || activeExecution.value));
const rodFrame = computed(() => TERMINAL_ROD_FRAMES[rodIdx.value % TERMINAL_ROD_FRAMES.length]);
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
</script>

<template>
    <!-- Primary View 1: 对话 (Chat) -->
    <div v-show="remoteConversation && activeDrawer === 'chat' && !settingsOpen" class="remote-session-host"><DshSessions v-if="remoteConversation" :key="remoteConversation.node.id + remoteConversation.session.id" :target="remoteConversation" :nodes="modelHub.state.remote_nodes || []" /></div>
    <main v-show="activeDrawer === 'chat' && !settingsOpen && !remoteConversation" class="main">
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
        @scroll.passive="onHistoryScroll"
      >
        <div v-if="loading" class="empty loading-brand">
          <img :src="brandLogo" :alt="t('brand')" />
          <p>{{ t('loading') }}</p>
        </div>
        <div v-else-if="!messages.length" class="empty">
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
        </div>
        <article
          v-for="(message, msgIdx) in messages"
          :key="message.id"
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
          <MessageContent :content="message.content" :plain="message.role === 'user'" :stream="message.role === 'assistant' && msgIdx === messages.length - 1" />
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
        <div v-if="dispatchPending || saving || activeExecution" class="dsh-live-terminal-card" role="status" aria-live="polite">
          <span class="terminal-rod" aria-hidden="true">[{{ rodFrame }}]</span>
          <span>{{ t(dispatchPending ? 'requestingExecution' : saving ? 'saving' : 'agentLiveThinking') }}</span>
          <span class="terminal-dots" aria-hidden="true"><span>.</span><span>.</span><span>.</span></span>
        </div>
      </section>

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
        <div class="composer-actions">
          <div class="composer-left-tools">
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
</template>

<style scoped>
.remote-session-host {
  display: flex;
  flex: 1;
  min-height: 0;
  min-width: 0;
  overflow: hidden;
}
</style>
