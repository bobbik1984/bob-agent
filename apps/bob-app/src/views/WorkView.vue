<script setup>
import { computed, reactive, ref } from 'vue';
import DshSessions from '../components/DshSessions.vue';
import MessageContent from '../components/MessageContent.vue';
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
  schedules: {
    type: Array,
    default: () => [],
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
  remoteNodes: {
    type: Array,
    default: () => [],
  },
  pairedOnline: {
    type: Boolean,
    default: false,
  },
});

const emit = defineEmits([
  'open-conversation',
  'create-project',
  'create-conversation',
  'open-settings-tab',
  'create-project-todo',
  'toggle-todo-status',
  'start-todo-session',
  'online-begin-pairing',
  'online-exchange-pairing',
  'online-refresh',
  'online-revoke',
  'online-select-project',
  'online-update-origin',
  'online-save-node',
  'online-load-more-messages',
  'online-submit-operation',
]);

const { t } = useI18n();
const newProjectTitle = ref('');
const showNewProjectForm = ref(false);
const showCloudConfig = ref(false);
const directNodeToken = ref('');
const projectTodoInputs = reactive({});
const onlineTaskPrompts = reactive({});

function submitOnlineOperation(projectId, task, operationKind = 'continue_task') {
  const key = task?.id || projectId;
  const prompt = (onlineTaskPrompts[key] || '').trim();
  if (!prompt && operationKind === 'continue_task') return;
  emit('online-submit-operation', {
    operationKind,
    projectId,
    taskId: task?.id || null,
    prompt: prompt || null,
    parentVersionId: task?.latest_artifact_version_id || null,
  });
  onlineTaskPrompts[key] = '';
}

function submitSaveNode() {
  emit('online-save-node', {
    origin: props.onlineState?.origin || 'https://agent.bobbik.org',
    token: directNodeToken.value.trim() || null,
  });
  directNodeToken.value = '';
}

function selectSyncedNode(node) {
  if (!node?.baseUrl) return;
  emit('online-save-node', {
    origin: node.baseUrl,
    token: node.token || null,
  });
}

const importedIds = computed(() => new Set(props.catalog?.imported_project_ids || []));
const projects = computed(() => (props.catalog?.projects || []).filter(p => !importedIds.value.has(p.id)));
const importedProjects = computed(() => (props.catalog?.projects || []).filter(p => importedIds.value.has(p.id)));
const conversations = computed(() => props.catalog?.conversations || []);
const deviceGroups = computed(() => (props.catalog?.device_groups || []).map(g => ({ ...g, conversations: g.conversations.filter(c => !importedIds.value.has(c.project_id)) })).filter(g => g.conversations.length));

const directProviderReady = computed(
  () =>
    props.readiness?.mode === 'direct_provider' &&
    props.readiness?.execution_available === true,
);
const homePaired = computed(
  () => props.pairedOnline || props.readiness?.mode === 'home_configured',
);
const onlineConnected = computed(() => Boolean(props.onlineState?.connected));
const onlineProjects = computed(() => props.onlineState?.projects || []);
const onlineTasks = computed(() => props.onlineState?.tasks || []);
const onlineMessages = computed(() => props.onlineState?.messages || []);

function projectConversations(projectId) {
  return conversations.value.filter(c => c.project_id === projectId);
}

function projectTodos(projectId) {
  return (props.schedules || []).filter(
    s => s.project_id === projectId && s.kind !== 'event' && s.status !== 'cancelled',
  );
}

function submitProjectTodo(projectId) {
  const text = (projectTodoInputs[projectId] || '').trim();
  if (!text) return;
  emit('create-project-todo', {
    projectId,
    title: text,
  });
  projectTodoInputs[projectId] = '';
}

function toggleProjectTodo(todo) {
  const nextStatus = todo.status === 'completed' ? 'scheduled' : 'completed';
  emit('toggle-todo-status', { id: todo.id, status: nextStatus });
}

function submitProject() {
  const title = newProjectTitle.value.trim();
  if (!title) return;
  emit('create-project', title);
  newProjectTitle.value = '';
  showNewProjectForm.value = false;
}
</script>

<template>
  <section class="primary-view work-view" :aria-label="t('workViewTitle')">
    <!-- Compact Single-Line Node Status Strip -->
    <header class="work-status-strip">
      <div class="work-status-pills">
        <button
          type="button"
          class="node-status-chip"
          :class="{ online: directProviderReady }"
          @click="emit('open-settings-tab', 'model')"
        >
          <span class="status-dot" :class="{ online: directProviderReady }"></span>
          <span>{{ activeMainRole?.model_name || activeMainRole?.model_id || 'DeepSeek' }}</span>
        </button>

        <button
          type="button"
          class="node-status-chip"
          :class="{ online: homePaired }"
          @click="emit('open-settings-tab', 'connection')"
        >
          <span class="status-dot" :class="{ online: homePaired }"></span>
          <span>Home · {{ homePaired ? t('deviceOnline') : t('deviceOffline') }}</span>
        </button>

        <button
          type="button"
          class="node-status-chip"
          :class="{ online: onlineConnected, active: showCloudConfig }"
          @click="showCloudConfig = !showCloudConfig"
        >
          <span class="status-dot" :class="{ online: onlineConnected }"></span>
          <span>bob-core-client/1.0</span>
        </button>
      </div>

      <button
        type="button"
        class="compact-add-btn"
        :class="{ active: showNewProjectForm }"
        @click="showNewProjectForm = !showNewProjectForm"
      >
        {{ t('workNewProjectBtn') }}
      </button>
    </header>

    <div class="view-content-body">
      <DshSessions :nodes="remoteNodes" />
      <!-- Collapsible New Project Drawer -->
      <form
        v-if="showNewProjectForm"
        class="quick-project-form hub-card"
        @submit.prevent="submitProject"
      >
        <input
          v-model="newProjectTitle"
          type="text"
          :placeholder="t('projectTitlePlaceholder')"
          required
        />
        <button type="submit" class="primary-action-btn" :disabled="!newProjectTitle.trim()">
          {{ t('create') }}
        </button>
      </form>

      <!-- Collapsible Cloud Node Pairing Drawer (Opened on demand from status strip) -->
      <section v-if="showCloudConfig || onlineState?.pairing" class="hub-card">
        <div class="hub-card-header">
          <div class="hub-card-title-group">
            <svg class="inline-svg" viewBox="0 0 24 24" aria-hidden="true">
              <path d="M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z" />
            </svg>
            <h3>{{ t('onlineSectionTitle') }}</h3>
          </div>
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

        <div class="quick-project-form">
          <input
            v-model="directNodeToken"
            type="password"
            autocomplete="off"
            :placeholder="t('onlineSaveTokenPlaceholder')"
          />
          <button
            type="button"
            class="mini-ghost-btn"
            :disabled="onlineState?.busy"
            @click="submitSaveNode"
          >
            {{ t('onlineSaveNodeBtn') }}
          </button>
        </div>

        <div v-if="remoteNodes?.length" class="work-session-list">
          <div
            v-for="node in remoteNodes"
            :key="node.id || node.baseUrl"
            class="work-session-row"
          >
            <span class="work-session-title">{{ node.label || node.baseUrl }} ({{ node.baseUrl }})</span>
            <button
              type="button"
              class="mini-ghost-btn"
              @click="selectSyncedNode(node)"
            >
              {{ t('onlineRefreshBtn') }}
            </button>
          </div>
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
        <p v-if="onlineState?.statusNotice" class="view-card-desc" role="status">
          {{ onlineState.statusNotice }}
        </p>
      </section>

      <!-- First-Screen Priority 1: Active Projects & Task Workspaces -->
      <section class="hub-card">
        <div class="hub-card-header">
          <div class="hub-card-title-group">
            <svg class="inline-svg" viewBox="0 0 24 24" aria-hidden="true">
              <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
            </svg>
            <h3>{{ t('workProjectsSectionTitle') }}</h3>
            <span class="hub-count-pill">{{ projects.length }}</span>
          </div>
        </div>

        <p v-if="!projects.length" class="view-card-desc">{{ t('noProjectsHint') }}</p>

        <div v-else class="work-projects-grid">
          <article v-for="proj in projects" :key="proj.id" class="work-project-item">
            <div class="work-project-head">
              <div class="work-item-title-group">
                <svg class="inline-svg" viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
                </svg>
                <strong>{{ proj.title }}</strong>
              </div>
              <button
                type="button"
                class="mini-ghost-btn"
                @click="emit('create-conversation', proj.id)"
              >
                {{ t('workNewSessionBtn') }}
              </button>
            </div>
            <p v-if="proj.directory_ref" class="work-node-meta">{{ proj.directory_ref }}</p>

            <!-- Project Sessions -->
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
                  {{ t('workOpenSessionBtn') }}
                </button>
              </div>
            </div>

            <!-- Closed-Loop Bound Project Todos (Synced with ScheduleView & todo.md) -->
            <div class="work-project-todos-section">
              <div class="work-todos-header">
                <span class="work-todos-label">{{ t('workProjectTodosTitle') }}</span>
                <span class="subtab-count">{{ projectTodos(proj.id).length }}</span>
              </div>

              <div v-if="projectTodos(proj.id).length" class="work-session-list">
                <div
                  v-for="todo in projectTodos(proj.id)"
                  :key="todo.id"
                  class="work-session-row work-todo-row"
                  :class="{ done: todo.status === 'completed' }"
                >
                  <button
                    type="button"
                    class="schedule-check-btn"
                    :class="{ checked: todo.status === 'completed' }"
                    @click="toggleProjectTodo(todo)"
                  >
                    <svg
                      v-if="todo.status === 'completed'"
                      class="check-svg"
                      viewBox="0 0 24 24"
                      aria-hidden="true"
                    >
                      <path d="M20 6 9 17l-5-5" />
                    </svg>
                  </button>
                  <span class="work-todo-title">{{ todo.title }}</span>
                  <button
                    type="button"
                    class="mini-ghost-btn"
                    @click="emit('start-todo-session', { projectId: proj.id, todo })"
                  >
                    {{ t('workProjectStartWithTodoBtn') }}
                  </button>
                </div>
              </div>

              <form class="work-inline-todo-form" @submit.prevent="submitProjectTodo(proj.id)">
                <input
                  v-model="projectTodoInputs[proj.id]"
                  type="text"
                  class="work-inline-todo-input"
                  :placeholder="t('workProjectAddTodoPlaceholder')"
                />
                <button
                  type="submit"
                  class="mini-ghost-btn"
                  :disabled="!(projectTodoInputs[proj.id] || '').trim()"
                >
                  + {{ t('workProjectAddTodoBtn') }}
                </button>
              </form>
            </div>
          </article>
        </div>
      </section>

      <!-- First-Screen Priority 2: Connected Cloud Online Projects (if connected) -->
      <section v-if="onlineConnected && onlineProjects.length" class="hub-card">
        <div class="hub-card-header">
          <div class="hub-card-title-group">
            <svg class="inline-svg" viewBox="0 0 24 24" aria-hidden="true">
              <path d="M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z" />
            </svg>
            <h3>{{ t('workOnlineNodeTitle') }}</h3>
            <span class="hub-count-pill">{{ onlineProjects.length }}</span>
          </div>
          <button
            type="button"
            class="mini-ghost-btn"
            :disabled="onlineState?.busy"
            @click="emit('online-refresh')"
          >
            {{ t('onlineRefreshBtn') }}
          </button>
        </div>

        <div class="work-projects-grid">
          <article
            v-for="op in onlineProjects"
            :key="op.id"
            class="work-project-item"
            :class="{ 'is-inspected': onlineState?.selectedProjectId === op.id }"
          >
            <div class="work-project-head">
              <strong>{{ op.title || op.name || op.id }}</strong>
              <button
                type="button"
                class="mini-ghost-btn"
                @click="emit('online-select-project', op.id)"
              >
                {{ t('onlineInspectProjectBtn') }}
              </button>
            </div>

            <!-- Bound Todos for Online Project -->
            <div v-if="projectTodos(op.id).length" class="work-session-list">
              <div
                v-for="todo in projectTodos(op.id)"
                :key="todo.id"
                class="work-session-row work-todo-row"
                :class="{ done: todo.status === 'completed' }"
              >
                <button
                  type="button"
                  class="schedule-check-btn"
                  :class="{ checked: todo.status === 'completed' }"
                  @click="toggleProjectTodo(todo)"
                >
                  <svg
                    v-if="todo.status === 'completed'"
                    class="check-svg"
                    viewBox="0 0 24 24"
                    aria-hidden="true"
                  >
                    <path d="M20 6 9 17l-5-5" />
                  </svg>
                </button>
                <span class="work-todo-title">{{ todo.title }}</span>
              </div>
            </div>

            <div v-if="onlineState?.selectedProjectId === op.id" class="online-inspector-workbench">
              <div v-if="onlineState?.offlineCached" class="routine-tag-box">
                <span>{{ t('onlineCachedBadge') }}</span>
              </div>
              <div class="online-workbench-split">
                <div class="online-workbench-pane">
                  <div class="work-session-list">
                    <div v-for="task in onlineTasks" :key="task.id" class="work-session-row">
                      <div class="memory-item-main">
                        <span class="work-session-title">{{ task.title || task.id }}</span>
                        <small v-if="onlineState?.taskSnapshots?.[task.id]" class="work-node-meta">
                          {{
                            onlineState.taskSnapshots[task.id].verification_status ||
                            onlineState.taskSnapshots[task.id].status ||
                            'verified'
                          }}
                        </small>
                      </div>
                      <span class="schedule-kind-badge">{{ task.status || 'observed' }}</span>
                      <button
                        v-if="onlineState?.capabilities?.capabilities?.submit"
                        type="button"
                        class="mini-ghost-btn"
                        @click="submitOnlineOperation(op.id, task, 'create_task_iteration')"
                      >
                        {{ t('onlineIterateV2Btn') }}
                      </button>
                    </div>
                  </div>
                </div>

                <div class="online-workbench-pane">
                  <form
                    v-if="onlineState?.capabilities?.capabilities?.submit"
                    class="work-inline-todo-form"
                    @submit.prevent="submitOnlineOperation(op.id, onlineTasks[0] || null, 'continue_task')"
                  >
                    <input
                      v-model="onlineTaskPrompts[onlineTasks[0]?.id || op.id]"
                      type="text"
                      class="work-inline-todo-input"
                      :placeholder="t('onlineTaskPromptPlaceholder')"
                    />
                    <button type="submit" class="mini-ghost-btn" :disabled="onlineState?.busy">
                      {{ t('onlineContinueTaskBtn') }}
                    </button>
                  </form>
                  <div class="work-session-list">
                    <div v-for="msg in onlineMessages" :key="msg.sequence" class="work-session-row work-message-row">
                      <small class="message-state">#{{ msg.sequence }} · {{ msg.role }}</small>
                      <MessageContent :content="msg.text || msg.content || ''" :plain="msg.role === 'user'" />
                    </div>
                  </div>
                  <button
                    v-if="onlineState?.messagesHasMore"
                    type="button"
                    class="mini-ghost-btn"
                    @click="
                      emit('online-load-more-messages', {
                        projectId: op.id,
                        after: onlineState.messagesNextAfter,
                      })
                    "
                  >
                    {{ t('onlineLoadMoreMessagesBtn') }}
                  </button>
                </div>
              </div>
            </div>
          </article>
        </div>
      </section>

      <details v-if="importedProjects.length" class="hub-card">
        <summary>{{ t('workImportedProjects') }} · {{ importedProjects.length }}</summary>
        <p class="view-card-desc">{{ t('workImportedProjectsHint') }}</p>
        <article v-for="project in importedProjects" :key="project.id" class="work-project-item">
          <strong>{{ project.title }}</strong>
          <div v-for="conv in projectConversations(project.id)" :key="conv.id" class="work-session-row">
            <span>{{ conv.title }}</span>
            <button class="mini-ghost-btn" @click="emit('open-conversation', conv.id)">{{ t('workOpenSessionBtn') }}</button>
          </div>
        </article>
      </details>

      <!-- First-Screen Priority 3: Active Device Task Groups -->
      <section v-if="deviceGroups.length" class="hub-card">
        <div class="hub-card-header">
          <div class="hub-card-title-group">
            <svg class="inline-svg" viewBox="0 0 24 24" aria-hidden="true">
              <rect x="2" y="3" width="20" height="14" rx="2" />
              <path d="M8 21h8M12 17v4" />
            </svg>
            <h3>{{ t('workDeviceGroupsTitle') }}</h3>
          </div>
        </div>

        <div class="work-projects-grid">
          <article v-for="group in deviceGroups" :key="group.device_id" class="work-project-item">
            <div class="work-project-head">
              <div class="work-item-title-group">
                <span
                  class="status-dot"
                  :class="{ online: group.immediate_execution_available }"
                ></span>
                <strong>{{ group.label }}</strong>
              </div>
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
                  {{ t('workOpenSessionBtn') }}
                </button>
              </div>
            </div>
          </article>
        </div>
      </section>
    </div>
  </section>
</template>
