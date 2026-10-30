<script setup>
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';

const props = defineProps({
  readiness: {
    type: Object,
    default: null,
  },
  catalog: {
    type: Object,
    default: null,
  },
  activeSessionId: {
    type: String,
    default: null,
  },
  activeMainRole: {
    type: Object,
    default: () => ({}),
  },
  onlineState: {
    type: Object,
    default: () => ({}),
  },
});

const emit = defineEmits([
  'open-conversation',
  'create-project',
  'create-conversation',
  'open-settings-tab',
  'online-begin-pairing',
  'online-exchange-pairing',
  'online-refresh',
  'online-revoke',
  'online-select-project',
  'online-update-origin',
]);

const { t } = useI18n();
const newProjectTitle = ref('');

const projects = computed(() => props.catalog?.projects || []);
const conversations = computed(() => props.catalog?.conversations || []);
const deviceGroups = computed(() => props.catalog?.device_groups || []);

const directProviderReady = computed(
  () =>
    props.readiness?.mode === 'direct_provider' &&
    props.readiness?.execution_available === true,
);
const homePaired = computed(() => props.readiness?.mode === 'home_configured');
const onlineConnected = computed(() => Boolean(props.onlineState?.connected));
const onlineProjects = computed(() => props.onlineState?.projects || []);
const onlineTasks = computed(() => props.onlineState?.tasks || []);
const onlineMessages = computed(() => props.onlineState?.messages || []);

function projectConversations(projectId) {
  return conversations.value.filter(c => c.project_id === projectId);
}

function submitProject() {
  const title = newProjectTitle.value.trim();
  if (!title) return;
  emit('create-project', title);
  newProjectTitle.value = '';
}
</script>

<template>
  <section class="primary-view work-view" :aria-label="t('workViewTitle')">
    <header class="view-hero">
      <div class="view-hero-titles">
        <h2>{{ t('workViewTitle') }}</h2>
        <p>{{ t('workViewSubtitle') }}</p>
      </div>
      <div class="view-hero-actions">
        <button
          type="button"
          class="mini-ghost-btn"
          @click="emit('open-settings-tab', 'connection')"
        >
          🔌 {{ t('workPairNodeBtn') }}
        </button>
        <button
          type="button"
          class="mini-ghost-btn"
          @click="emit('open-settings-tab', 'model')"
        >
          🔑 {{ t('workConfigureModelBtn') }}
        </button>
      </div>
    </header>

    <div class="view-content-body">
      <!-- Execution Nodes Matrix -->
      <section class="hub-card">
        <div class="hub-card-header">
          <div class="hub-card-title-group">
            <h3>{{ t('workNodeMatrixTitle') }}</h3>
          </div>
        </div>

        <div class="work-nodes-grid">
          <div class="work-node-card" :class="{ online: directProviderReady }">
            <div class="work-node-head">
              <strong>{{ t('workLocalNodeTitle') }}</strong>
              <span class="device-status-pill" :class="{ online: directProviderReady }">
                {{ directProviderReady ? t('deviceOnline') : t('keyInputUnconfigured') }}
              </span>
            </div>
            <p class="work-node-meta">
              {{ activeMainRole?.provider_name || 'DeepSeek' }} ·
              {{ activeMainRole?.model_name || activeMainRole?.model_id || 'deepseek-chat' }}
            </p>
            <button
              type="button"
              class="mini-ghost-btn"
              @click="emit('open-settings-tab', 'model')"
            >
              {{ t('workConfigureModelBtn') }}
            </button>
          </div>

          <div class="work-node-card" :class="{ online: homePaired }">
            <div class="work-node-head">
              <strong>{{ t('workHomeNodeTitle') }}</strong>
              <span class="device-status-pill" :class="{ online: homePaired }">
                {{ homePaired ? t('deviceOnline') : t('deviceOffline') }}
              </span>
            </div>
            <p class="work-node-meta">
              {{ homePaired ? t('homeConfiguredHint') : t('deviceSubpageDesc') }}
            </p>
            <button
              type="button"
              class="mini-ghost-btn"
              @click="emit('open-settings-tab', 'connection')"
            >
              {{ t('workPairNodeBtn') }}
            </button>
          </div>

          <div class="work-node-card" :class="{ online: onlineConnected }">
            <div class="work-node-head">
              <strong>☁ {{ t('workOnlineNodeTitle') }}</strong>
              <span class="device-status-pill" :class="{ online: onlineConnected }">
                {{ onlineConnected ? t('onlineReadOnlyBadge') : t('keyInputUnconfigured') }}
              </span>
            </div>
            <p class="work-node-meta">
              {{ onlineState?.origin || 'https://agent.bobbik.org' }} · bob-core-client/1.0
            </p>
            <div class="work-node-actions">
              <button
                v-if="!onlineConnected"
                type="button"
                class="mini-ghost-btn"
                :disabled="onlineState?.busy"
                @click="emit('online-begin-pairing')"
              >
                {{ t('onlineConnectBtn') }}
              </button>
              <button
                v-else
                type="button"
                class="mini-ghost-btn"
                :disabled="onlineState?.busy"
                @click="emit('online-refresh')"
              >
                ↻ {{ t('onlineRefreshBtn') }}
              </button>
              <button
                v-if="onlineConnected"
                type="button"
                class="mini-ghost-btn"
                :disabled="onlineState?.busy"
                @click="emit('online-revoke')"
              >
                {{ t('onlineDisconnectBtn') }}
              </button>
            </div>
          </div>
        </div>
      </section>

      <!-- Cloud Online Work Service (agent.bobbik.org / bob-core-client/1.0) -->
      <section class="hub-card">
        <div class="hub-card-header">
          <div class="hub-card-title-group">
            <h3>☁ {{ t('onlineSectionTitle') }}</h3>
            <span class="hub-count-pill">bob-core-client/1.0</span>
          </div>
        </div>
        <p class="view-card-desc">{{ t('onlineSectionDesc') }}</p>

        <div class="quick-project-form">
          <input
            :value="onlineState?.origin || 'https://agent.bobbik.org'"
            type="url"
            :placeholder="t('onlineOriginPlaceholder')"
            @input="emit('online-update-origin', $event.target.value)"
          />
          <button
            v-if="!onlineConnected"
            type="button"
            class="primary-action-btn"
            :disabled="onlineState?.busy"
            @click="emit('online-begin-pairing')"
          >
            {{ t('onlineConnectBtn') }}
          </button>
          <button
            v-else
            type="button"
            class="primary-action-btn"
            :disabled="onlineState?.busy"
            @click="emit('online-refresh')"
          >
            {{ t('onlineRefreshBtn') }}
          </button>
        </div>

        <div v-if="onlineState?.pairing && !onlineConnected" class="routine-tag-box">
          <code>{{ onlineState.pairing.user_code }}</code>
          <span>{{ onlineState.pairing.verification_uri }}</span>
          <button
            type="button"
            class="mini-ghost-btn"
            :disabled="onlineState?.busy"
            @click="emit('online-exchange-pairing')"
          >
            {{ t('onlineCompletePairingBtn') }}
          </button>
        </div>

        <p v-if="onlineState?.error" class="view-card-desc" role="alert">
          {{ onlineState.error }}
        </p>

        <div v-if="onlineConnected" class="work-projects-grid">
          <article v-for="op in onlineProjects" :key="op.id" class="work-project-item">
            <div class="work-project-head">
              <strong>☁ {{ op.title || op.name || op.id }}</strong>
              <button
                type="button"
                class="mini-ghost-btn"
                @click="emit('online-select-project', op.id)"
              >
                {{ t('onlineInspectProjectBtn') }}
              </button>
            </div>
            <div v-if="onlineState?.selectedProjectId === op.id" class="work-session-list">
              <div v-for="task in onlineTasks" :key="task.id" class="work-session-row">
                <span class="work-session-title">{{ task.title || task.id }}</span>
                <span class="schedule-kind-badge">{{ task.status || 'observed' }}</span>
              </div>
              <div v-for="msg in onlineMessages" :key="msg.sequence" class="work-session-row">
                <span class="work-session-title">
                  #{{ msg.sequence }} · {{ msg.text || msg.content || msg.role }}
                </span>
              </div>
            </div>
          </article>
        </div>
      </section>

      <!-- Project Workspaces -->
      <section class="hub-card">
        <div class="hub-card-header">
          <div class="hub-card-title-group">
            <h3>{{ t('workProjectsSectionTitle') }}</h3>
            <span class="hub-count-pill">{{ projects.length }}</span>
          </div>
        </div>

        <form class="quick-project-form" @submit.prevent="submitProject">
          <input
            v-model="newProjectTitle"
            type="text"
            :placeholder="t('projectTitlePlaceholder')"
          />
          <button type="submit" class="primary-action-btn" :disabled="!newProjectTitle.trim()">
            + {{ t('workNewProjectBtn') }}
          </button>
        </form>

        <p v-if="!projects.length" class="view-card-desc">{{ t('noProjectsHint') }}</p>

        <div v-else class="work-projects-grid">
          <article v-for="proj in projects" :key="proj.id" class="work-project-item">
            <div class="work-project-head">
              <strong>📁 {{ proj.title }}</strong>
              <button
                type="button"
                class="mini-ghost-btn"
                @click="emit('create-conversation', proj.id)"
              >
                + {{ t('workNewSessionBtn') }}
              </button>
            </div>
            <div class="work-session-list">
              <div
                v-for="conv in projectConversations(proj.id)"
                :key="conv.id"
                class="work-session-row"
              >
                <span class="work-session-title">{{ conv.title }}</span>
                <button
                  type="button"
                  class="mini-ghost-btn"
                  @click="emit('open-conversation', conv.id)"
                >
                  {{ t('workOpenSessionBtn') }} →
                </button>
              </div>
            </div>
          </article>
        </div>
      </section>

      <!-- Device Task Groups -->
      <section v-if="deviceGroups.length" class="hub-card">
        <div class="hub-card-header">
          <div class="hub-card-title-group">
            <h3>{{ t('workDeviceGroupsTitle') }}</h3>
          </div>
        </div>

        <div class="work-projects-grid">
          <article v-for="group in deviceGroups" :key="group.device_id" class="work-project-item">
            <div class="work-project-head">
              <strong>🖥 {{ group.label }}</strong>
              <span
                class="device-status-pill"
                :class="{ online: group.immediate_execution_available }"
              >
                {{
                  t(
                    group.availability_state === 'online'
                      ? 'deviceOnline'
                      : group.availability_state === 'sleeping'
                        ? 'deviceSleeping'
                        : 'deviceOffline',
                  )
                }}
              </span>
            </div>
            <p class="view-card-desc">{{ group.status_reason }}</p>
            <div class="work-session-list">
              <div
                v-for="conv in group.conversations"
                :key="conv.id"
                class="work-session-row"
              >
                <span class="work-session-title">{{ conv.title }}</span>
                <button
                  type="button"
                  class="mini-ghost-btn"
                  @click="emit('open-conversation', conv.id)"
                >
                  {{ t('workOpenSessionBtn') }} →
                </button>
              </div>
            </div>
          </article>
        </div>
      </section>
    </div>
  </section>
</template>
