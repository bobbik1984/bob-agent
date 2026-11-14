<script setup>
import { computed, inject, nextTick, onUnmounted, ref, watch } from 'vue';
import { nodeCreationKey, nodeLabel } from '../conversation-directory.mjs';

const {
  t,
  remoteCreationEnabled,
  catalog,
  creatingKind,
  creatingForProjectId,
  newItemTitle,
  remoteHarnesses,
  firstSessionMessage,
  nativeSetupReply,
  creationMode,
  creationDeviceId,
  creationConnectionId,
  creationWorkspace,
  creationNodes,
  creationWorkspaces,
  creationAvailableTools,
  creationLoading,
  creationNodeLoading,
  creationBusy,
  creationError,
  remoteCreation,
  selectCreationDevice,
  selectCreationTool,
  cancelCreateItem,
  submitCreateItem,
} = inject('bobAppShell');

const titleInputRef = ref(null);
const projectSelectRef = ref(null);

const isRemoteMode = computed(() => Boolean(remoteHarnesses[creationMode.value]));

function formatNodeMeta(node) {
  if (!node) return '';
  const remark = String(node.remark || '').trim();
  if (remark && remark !== 'web_host') return remark;
  if (node.isLocal || node.id === 'local') return t('createHostNodeTag');
  const rawUrl = String(node.baseUrl || node.url || '').trim();
  if (rawUrl) {
    try {
      const host = new URL(rawUrl).host;
      if (host && !host.startsWith('127.0.0.1') && !host.startsWith('localhost')) return host;
    } catch {}
  }
  return t('createRemoteNodeTag');
}

const canSubmit = computed(() => {
  if (creationBusy.value || creationLoading.value || creationNodeLoading.value) return false;
  if (creatingKind.value === 'project' && !newItemTitle.value.trim()) return false;
  const mode = creationMode.value;
  if (!['none', 'local'].includes(mode) && !Object.hasOwn(remoteHarnesses, mode)) return false;
  if (Object.hasOwn(remoteHarnesses, mode)) {
    if (!creationConnectionId.value || !creationWorkspace.value) return false;
    if (mode !== 'dsh' && !firstSessionMessage.value.trim()) return false;
  }
  return true;
});

function handleKeydown(event) {
  if (event.key === 'Escape' && creatingKind.value && !creationBusy.value) {
    event.preventDefault();
    cancelCreateItem();
  }
}

watch(
  creatingKind,
  async (kind) => {
    if (typeof window !== 'undefined') {
      window.removeEventListener('keydown', handleKeydown);
      if (kind) window.addEventListener('keydown', handleKeydown);
    }
    if (!kind) return;
    await nextTick();
    if (kind === 'conversation' && projectSelectRef.value) {
      projectSelectRef.value.focus();
    } else {
      titleInputRef.value?.focus();
    }
  },
  { immediate: true },
);

onUnmounted(() => {
  if (typeof window !== 'undefined') {
    window.removeEventListener('keydown', handleKeydown);
  }
});

function onBackdropClick() {
  if (!creationBusy.value) cancelCreateItem();
}

function submitLabelKey() {
  if (creationBusy.value) return 'createItemPending';
  if (remoteCreation.value) return 'createRetry';
  if (isRemoteMode.value && creationMode.value !== 'dsh') return 'createAndSend';
  return 'create';
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="creatingKind"
      class="create-modal-backdrop"
      role="dialog"
      aria-modal="true"
      :aria-label="t(creatingKind === 'project' ? 'createModalProjectTitle' : 'createModalConversationTitle')"
      @click.self="onBackdropClick"
    >
      <form class="create-modal-card" @submit.prevent="submitCreateItem()">
        <header class="create-modal-header">
          <div class="create-modal-heading">
            <h2>{{ t(creatingKind === 'project' ? 'createModalProjectTitle' : 'createModalConversationTitle') }}</h2>
            <p class="create-modal-subtitle">
              {{ t(creatingKind === 'project' ? 'createModalProjectSubtitle' : 'createModalConversationSubtitle') }}
            </p>
          </div>
          <button
            type="button"
            class="create-modal-close"
            :disabled="creationBusy"
            :aria-label="t('cancelAction')"
            @click="cancelCreateItem"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
              <path d="M18 6 6 18M6 6l12 12" />
            </svg>
          </button>
        </header>

        <div class="create-modal-body">
          <fieldset class="create-modal-fieldset" :disabled="creationBusy || !!remoteCreation">
            <!-- Step 1: Project association & Name -->
            <section class="create-step-block">
              <div class="create-step-title">
                <span class="step-index">1</span>
                <span>{{ t('createStepProjectTitle') }}</span>
              </div>
              <div class="step-fields" :class="{ 'step-fields-two-col': creatingKind === 'conversation' }">
                <label v-if="creatingKind === 'conversation'" class="field-label">
                  <span>{{ t('projectLinkLabel') }}</span>
                  <select ref="projectSelectRef" v-model="creatingForProjectId" class="modal-control">
                    <option :value="null">{{ t('projectLinkNone') }}</option>
                    <option v-for="project in catalog?.projects || []" :key="project.id" :value="project.id">
                      {{ project.title }}
                    </option>
                  </select>
                </label>
                <label class="field-label">
                  <span>{{ t(creatingKind === 'project' ? 'projectTitlePlaceholder' : 'conversationTitlePlaceholder') }}</span>
                  <input
                    ref="titleInputRef"
                    v-model="newItemTitle"
                    type="text"
                    maxlength="256"
                    class="modal-control"
                    :placeholder="t(creatingKind === 'project' ? 'createProjectTitleHint' : 'createConversationTitleHint')"
                    :required="creatingKind === 'project'"
                  />
                </label>
              </div>
            </section>

            <!-- Step 2: Execution Device -->
            <section class="create-step-block">
              <div class="create-step-title">
                <span class="step-index">2</span>
                <span>{{ t('createStepDevice') }}</span>
              </div>
              <div class="device-grid" role="radiogroup" :aria-label="t('createStepDevice')">
                <button
                  v-if="creatingKind === 'project'"
                  type="button"
                  class="choice-card"
                  :class="{ active: creationDeviceId === 'none' }"
                  :aria-pressed="creationDeviceId === 'none'"
                  @click="selectCreationDevice('none')"
                >
                  <span class="choice-title-row">
                    <span class="choice-dot muted" />
                    <span class="choice-name">{{ t('createProjectOnlyDevice') }}</span>
                    <span class="choice-radio" aria-hidden="true" />
                  </span>
                  <span class="choice-meta">{{ t('createProjectOnlyTag') }}</span>
                </button>

                <button
                  type="button"
                  class="choice-card"
                  :class="{ active: creationDeviceId === 'local' }"
                  :aria-pressed="creationDeviceId === 'local'"
                  @click="selectCreationDevice('local')"
                >
                  <span class="choice-title-row">
                    <span class="choice-dot online" />
                    <span class="choice-name">{{ t('createLocalDeviceTitle') }}</span>
                    <span class="choice-radio" aria-hidden="true" />
                  </span>
                  <span class="choice-meta">{{ t('createLocalDeviceTag') }}</span>
                </button>

                <template v-if="remoteCreationEnabled">
                  <button
                    v-for="node in creationNodes"
                    :key="nodeCreationKey(node)"
                    type="button"
                    class="choice-card"
                    :class="{ active: creationDeviceId === nodeCreationKey(node) }"
                    :aria-pressed="creationDeviceId === nodeCreationKey(node)"
                    @click="selectCreationDevice(nodeCreationKey(node))"
                  >
                    <span class="choice-title-row">
                      <span class="choice-dot" :class="node.online === false ? 'offline' : 'online'" />
                      <span class="choice-name">{{ nodeLabel(node) }}</span>
                      <span class="choice-radio" aria-hidden="true" />
                    </span>
                    <span class="choice-meta">{{ formatNodeMeta(node) }}</span>
                  </button>
                </template>
              </div>
            </section>

            <!-- Step 3: Available Tools on Selected Device -->
            <section class="create-step-block">
              <div class="create-step-title">
                <span class="step-index">3</span>
                <span>{{ t('createStepTool') }}</span>
              </div>

              <p v-if="creationDeviceId === 'none'" class="step-note">
                {{ t('createProjectOnlyNote') }}
              </p>
              <p v-else-if="creationDeviceId === 'local'" class="step-note">
                {{ t('createLocalToolNote') }}
              </p>
              <p v-else-if="creationNodeLoading" class="step-note">
                {{ t('createLoadingDeviceTools') }}
              </p>
              <div v-else class="tool-grid" role="radiogroup" :aria-label="t('createStepTool')">
                <button
                  v-for="tool in creationAvailableTools"
                  :key="tool.id"
                  type="button"
                  class="tool-chip"
                  :class="{ active: creationMode === tool.id, unavailable: tool.loaded && !tool.available }"
                  :disabled="tool.loaded && !tool.available"
                  :aria-pressed="creationMode === tool.id"
                  @click="selectCreationTool(tool.id)"
                >
                  <span class="tool-chip-name">{{ tool.label || t(tool.labelKey) }}</span>
                  <span class="tool-chip-count">
                    <template v-if="!tool.loaded">{{ t('dshLoading') }}</template>
                    <template v-else-if="tool.available">{{ tool.count }}</template>
                    <template v-else>{{ t('createToolNoWorkspaceShort') }}</template>
                  </span>
                </button>
              </div>

              <div v-if="isRemoteMode" class="daemon-hint">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
                  <circle cx="12" cy="12" r="10" />
                  <path d="M12 16v-4M12 8h.01" />
                </svg>
                <span>{{ t('createDetachedDaemonHint') }}</span>
              </div>
            </section>

            <!-- Step 4: Workspace & Initial Instruction -->
            <section v-if="isRemoteMode" class="create-step-block">
              <div class="create-step-title">
                <span class="step-index">4</span>
                <span>{{ t('createStepWorkspace') }}</span>
              </div>

              <label class="field-label">
                <span>{{ t('createSessionWorkspace') }}</span>
                <select
                  v-model="creationWorkspace"
                  class="modal-control"
                  required
                  :disabled="creationLoading || !creationWorkspaces.length"
                >
                  <option disabled value="">{{ t(creationLoading ? 'dshLoading' : 'createSelectWorkspace') }}</option>
                  <option v-for="workspace in creationWorkspaces" :key="workspace" :value="workspace">
                    {{ workspace }}
                  </option>
                </select>
              </label>

              <label v-if="creationMode !== 'dsh' && creationWorkspace" class="field-label">
                <span>{{ t('createFirstMessage') }}</span>
                <textarea
                  v-model="firstSessionMessage"
                  rows="3"
                  class="modal-control modal-textarea"
                  :placeholder="t('createFirstMessageHint')"
                  required
                />
                <small class="field-hint">{{ t('createFirstMessagePermission') }}</small>
              </label>
            </section>
          </fieldset>

          <pre v-if="nativeSetupReply" class="setup-reply">{{ nativeSetupReply }}</pre>
          <p v-if="creationError" class="create-error" role="status">{{ t(creationError) }}</p>
        </div>

        <footer class="create-modal-footer">
          <button type="button" class="btn-secondary" :disabled="creationBusy" @click="cancelCreateItem">
            {{ t('cancelAction') }}
          </button>
          <button type="submit" class="btn-primary" :disabled="!canSubmit">
            {{ t(submitLabelKey()) }}
          </button>
        </footer>
      </form>
    </div>
  </Teleport>
</template>

<style scoped>
.create-modal-backdrop {
  position: fixed;
  inset: 0;
  z-index: 90;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 1rem;
  background: color-mix(in srgb, var(--bg-root) 62%, transparent);
  backdrop-filter: blur(14px);
  -webkit-backdrop-filter: blur(14px);
}

.create-modal-card {
  width: min(560px, 100%);
  max-height: min(88vh, 760px);
  display: flex;
  flex-direction: column;
  background: var(--bg-primary);
  color: var(--text-primary);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-modal, 18px);
  box-shadow: var(--shadow-lg);
  overflow: hidden;
}

.create-modal-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.75rem;
  padding: 1.05rem 1.25rem 0.9rem;
  border-bottom: 1px solid var(--line);
}

.create-modal-heading h2 {
  margin: 0;
  font-size: var(--text-lg, 1.05rem);
  font-weight: 600;
  color: var(--text-primary);
}

.create-modal-subtitle {
  margin: 0.25rem 0 0;
  font-size: var(--text-xs, 0.78rem);
  color: var(--text-secondary);
}

.create-modal-close {
  flex: 0 0 auto;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 2rem;
  height: 2rem;
  border-radius: var(--radius-btn, 10px);
  border: 1px solid transparent;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
}

.create-modal-close:hover:not(:disabled) {
  background: var(--bg-tertiary);
  color: var(--text-primary);
}

.create-modal-body {
  padding: 1rem 1.25rem;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 0.9rem;
}

.create-modal-fieldset {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  min-width: 0;
  padding: 0;
  margin: 0;
  border: 0;
}

.create-step-block {
  display: flex;
  flex-direction: column;
  gap: 0.55rem;
  padding: 0.8rem 0.9rem;
  border-radius: var(--radius-card, 14px);
  background: var(--bg-secondary);
  border: 1px solid var(--line);
}

.create-step-title {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: var(--text-xs, 0.8rem);
  font-weight: 600;
  color: var(--text-primary);
}

.step-index {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.25rem;
  height: 1.25rem;
  border-radius: 999px;
  font-size: 0.72rem;
  background: var(--accent-soft);
  color: var(--accent-primary);
}

.step-fields {
  display: grid;
  gap: 0.65rem;
}

.step-fields-two-col {
  grid-template-columns: repeat(2, minmax(0, 1fr));
}

@media (max-width: 560px) {
  .step-fields-two-col {
    grid-template-columns: 1fr;
  }
}

.field-label {
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
  min-width: 0;
  font-size: var(--text-xs, 0.78rem);
  color: var(--text-secondary);
}

.modal-control {
  width: 100%;
  min-width: 0;
  padding: 0.5rem 0.65rem;
  border-radius: var(--radius-btn, 10px);
  border: 1px solid var(--border-color);
  background: var(--bg-primary);
  color: var(--text-primary);
  font-size: var(--text-sm, 0.88rem);
}

.modal-control:focus {
  outline: none;
  border-color: var(--accent-primary);
  box-shadow: 0 0 0 2px var(--accent-soft);
}

.modal-textarea {
  resize: vertical;
  min-height: 4.5rem;
}

.field-hint {
  color: var(--text-tertiary);
  font-size: 0.74rem;
}

.device-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 0.5rem;
}

.choice-card {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 0.2rem;
  padding: 0.55rem 0.7rem;
  border-radius: var(--radius-btn, 10px);
  border: 1px solid color-mix(in srgb, var(--text-primary) 24%, var(--bg-primary));
  background: var(--bg-secondary);
  color: var(--text-primary);
  text-align: left;
  cursor: pointer;
  transition: border-color 0.15s ease, background-color 0.15s ease, box-shadow 0.15s ease;
}

.choice-card:hover:not(:disabled) {
  border-color: color-mix(in srgb, var(--text-primary) 38%, var(--bg-primary));
  background: var(--bg-tertiary);
}

.choice-card.active {
  border-color: var(--accent-primary);
  background: var(--accent-soft);
  box-shadow: 0 0 0 1px var(--accent-primary);
}

.choice-title-row {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  width: 100%;
  min-width: 0;
}

.choice-dot {
  width: 0.48rem;
  height: 0.48rem;
  border-radius: 999px;
  flex: 0 0 auto;
  background: var(--text-tertiary);
}

.choice-dot.online {
  background: var(--color-success);
}

.choice-dot.offline {
  background: var(--text-tertiary);
}

.choice-dot.muted {
  background: var(--text-secondary);
}

.choice-name {
  flex: 1 1 auto;
  min-width: 0;
  font-size: var(--text-sm, 0.86rem);
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.choice-radio {
  width: 0.78rem;
  height: 0.78rem;
  border-radius: 999px;
  flex: 0 0 auto;
  border: 1.5px solid color-mix(in srgb, var(--text-primary) 34%, var(--bg-primary));
  background: transparent;
}

.choice-card.active .choice-radio {
  border-color: var(--accent-primary);
  background: radial-gradient(circle, var(--accent-primary) 44%, transparent 48%);
}

.choice-meta {
  font-size: 0.72rem;
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 100%;
}

.tool-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 0.45rem;
}

.tool-chip {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.42rem 0.72rem;
  border-radius: 999px;
  border: 1px solid var(--border-color);
  background: var(--bg-primary);
  color: var(--text-primary);
  font-size: var(--text-xs, 0.82rem);
  cursor: pointer;
}

.tool-chip:hover:not(:disabled) {
  background: var(--bg-tertiary);
}

.tool-chip.active {
  border-color: var(--accent-primary);
  background: var(--accent-soft);
  color: var(--accent-primary);
  font-weight: 600;
}

.tool-chip.unavailable,
.tool-chip:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.tool-chip-count {
  font-size: 0.7rem;
  padding: 0.08rem 0.38rem;
  border-radius: 999px;
  background: var(--bg-secondary);
  color: var(--text-secondary);
}

.step-note {
  margin: 0;
  font-size: var(--text-xs, 0.8rem);
  color: var(--text-secondary);
}

.daemon-hint {
  display: flex;
  align-items: flex-start;
  gap: 0.45rem;
  padding: 0.5rem 0.65rem;
  border-radius: var(--radius-btn, 10px);
  background: var(--bg-primary);
  border: 1px dashed var(--border-color);
  font-size: 0.74rem;
  color: var(--text-secondary);
  line-height: 1.45;
}

.daemon-hint svg {
  flex: 0 0 auto;
  margin-top: 0.1rem;
  color: var(--accent-primary);
}

.setup-reply {
  margin: 0;
  max-height: 8.5rem;
  overflow: auto;
  padding: 0.65rem 0.75rem;
  border-radius: var(--radius-btn, 10px);
  background: var(--bg-secondary);
  border: 1px solid var(--line);
  white-space: pre-wrap;
  font-size: 0.76rem;
  color: var(--text-secondary);
}

.create-error {
  margin: 0;
  padding: 0.5rem 0.75rem;
  border-radius: var(--radius-btn, 10px);
  background: color-mix(in srgb, var(--color-error) 12%, transparent);
  color: var(--color-error);
  font-size: var(--text-xs, 0.8rem);
  overflow-wrap: anywhere;
}

.create-modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: 0.6rem;
  padding: 0.85rem 1.25rem;
  border-top: 1px solid var(--line);
  background: var(--bg-secondary);
}

.btn-secondary,
.btn-primary {
  padding: 0.48rem 1rem;
  border-radius: var(--radius-btn, 10px);
  font-size: var(--text-sm, 0.86rem);
  font-weight: 500;
  cursor: pointer;
}

.btn-secondary {
  border: 1px solid var(--border-color);
  background: var(--bg-primary);
  color: var(--text-primary);
}

.btn-secondary:hover:not(:disabled) {
  background: var(--bg-tertiary);
}

.btn-primary {
  border: 1px solid var(--accent-primary);
  background: var(--accent-primary);
  color: var(--text-on-accent);
}

.btn-primary:disabled,
.btn-secondary:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}
</style>
