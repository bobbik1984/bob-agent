<template>
  <section class="work-view">
    <header class="work-header">
      <div>
        <p class="work-eyebrow">{{ t('work.eyebrow') }}</p>
        <h1>{{ t('work.title') }}</h1>
        <p>{{ t('work.subtitle') }}</p>
      </div>
      <button class="primary-button" type="button" @click="beginProject">
        <Plus :size="16" />
        {{ t('work.new_project') }}
      </button>
    </header>

    <div v-if="errorMessage" class="work-notice error" role="alert">
      <CircleAlert :size="16" />
      <span>{{ errorMessage }}</span>
      <button type="button" :aria-label="t('common.close')" @click="errorMessage = ''"><X :size="14" /></button>
    </div>

    <div class="work-layout">
      <aside class="project-rail">
        <div class="rail-heading">
          <span>{{ t('work.projects') }}</span>
          <span>{{ projects.length }}</span>
        </div>
        <button
          v-for="project in projects"
          :key="project.id"
          class="project-row"
          :class="{ active: project.id === activeProjectId }"
          type="button"
          @click="selectProject(project.id)"
        >
          <FolderKanban :size="17" />
          <span class="project-copy">
            <strong>{{ project.title }}</strong>
            <small>{{ project.currentPhase || t('work.phase_unset') }}</small>
          </span>
          <ChevronRight :size="15" />
        </button>
        <div v-if="!loading && projects.length === 0" class="rail-empty">
          <FolderKanban :size="24" />
          <span>{{ t('work.empty_projects') }}</span>
        </div>
      </aside>

      <main class="work-main">
        <form v-if="creatingProject" class="create-project-card" @submit.prevent="createProject">
          <div class="section-heading">
            <div>
              <p class="section-kicker">{{ t('work.project_definition') }}</p>
              <h2>{{ t('work.create_title') }}</h2>
            </div>
            <button class="icon-button" type="button" :aria-label="t('common.close')" @click="creatingProject = false">
              <X :size="17" />
            </button>
          </div>
          <label>
            <span>{{ t('work.project_name') }}</span>
            <input v-model.trim="projectDraft.title" required maxlength="200" :placeholder="t('work.project_name_hint')" />
          </label>
          <label>
            <span>{{ t('work.project_mission') }}</span>
            <textarea v-model.trim="projectDraft.mission" rows="3" :placeholder="t('work.project_mission_hint')"></textarea>
          </label>
          <label>
            <span>{{ t('work.current_phase') }}</span>
            <input v-model.trim="projectDraft.currentPhase" maxlength="120" :placeholder="t('work.current_phase_hint')" />
          </label>
          <div class="form-actions">
            <button class="secondary-button" type="button" @click="creatingProject = false">{{ t('common.cancel') }}</button>
            <button class="primary-button" type="submit" :disabled="saving || !projectDraft.title">
              <LoaderCircle v-if="saving" class="spin" :size="16" />
              <Check v-else :size="16" />
              {{ t('work.create_action') }}
            </button>
          </div>
        </form>

        <div v-else-if="aggregate" class="project-board">
          <header class="project-summary">
            <div class="project-summary-copy">
              <div class="project-title-line">
                <span class="status-dot"></span>
                <h2>{{ aggregate.project.title }}</h2>
                <span class="status-pill">{{ statusLabel(aggregate.project.status) }}</span>
              </div>
              <p>{{ aggregate.project.mission || t('work.no_mission') }}</p>
              <div class="project-meta">
                <span><Milestone :size="14" />{{ aggregate.project.currentPhase || t('work.phase_unset') }}</span>
                <span><RefreshCw :size="14" />{{ formatTime(aggregate.project.updatedAt) }}</span>
              </div>
            </div>
            <button class="secondary-button" type="button" @click="exportSnapshot">
              <FileDown :size="16" />
              {{ t('work.export_snapshot') }}
            </button>
          </header>

          <div class="continuity-strip">
            <div>
              <span>{{ aggregate.goals.length }}</span>
              <small>{{ t('work.goals') }}</small>
            </div>
            <div>
              <span>{{ openTasks.length }}</span>
              <small>{{ t('work.open_tasks') }}</small>
            </div>
            <div>
              <span>{{ aggregate.decisions.length }}</span>
              <small>{{ t('work.decisions') }}</small>
            </div>
            <div>
              <span>{{ aggregate.risks.length }}</span>
              <small>{{ t('work.risks') }}</small>
            </div>
          </div>

          <form class="quick-add" :class="{ 'has-reason': itemDraft.kind === 'decision' }" @submit.prevent="createWorkItem">
            <select v-model="itemDraft.kind" :aria-label="t('work.item_kind')">
              <option value="task">{{ t('work.kind_task') }}</option>
              <option value="goal">{{ t('work.kind_goal') }}</option>
              <option value="decision">{{ t('work.kind_decision') }}</option>
            </select>
            <input v-model.trim="itemDraft.title" required maxlength="200" :placeholder="itemPlaceholder" />
            <input
              v-if="itemDraft.kind === 'decision'"
              v-model.trim="itemDraft.reason"
              required
              maxlength="500"
              :placeholder="t('work.decision_reason_hint')"
            />
            <button class="primary-button compact" type="submit" :disabled="saving || !itemDraft.title">
              <Plus :size="16" />
              {{ t('work.add') }}
            </button>
          </form>

          <div class="board-grid">
            <section class="work-column">
              <div class="column-heading"><Target :size="16" /><h3>{{ t('work.goals') }}</h3></div>
              <article v-for="goal in aggregate.goals" :key="goal.id" class="work-card">
                <strong>{{ goal.title }}</strong>
                <p>{{ goal.data?.outcome || goal.description }}</p>
                <span class="mini-status">{{ statusLabel(goal.status) }}</span>
              </article>
              <p v-if="aggregate.goals.length === 0" class="column-empty">{{ t('work.empty_goals') }}</p>
            </section>

            <section class="work-column">
              <div class="column-heading"><ListChecks :size="16" /><h3>{{ t('work.tasks') }}</h3></div>
              <article v-for="task in aggregate.tasks" :key="task.id" class="work-card task-card" :class="{ complete: task.status === 'done' }">
                <button class="task-toggle" type="button" :disabled="task.status === 'done'" @click="completeTask(task)">
                  <CheckCircle2 v-if="task.status === 'done'" :size="18" />
                  <Circle v-else :size="18" />
                </button>
                <div>
                  <strong>{{ task.title }}</strong>
                  <p v-if="task.description">{{ task.description }}</p>
                  <span class="mini-status">{{ statusLabel(task.status) }}</span>
                </div>
              </article>
              <p v-if="aggregate.tasks.length === 0" class="column-empty">{{ t('work.empty_tasks') }}</p>
            </section>

            <section class="work-column">
              <div class="column-heading"><Scale :size="16" /><h3>{{ t('work.decisions') }}</h3></div>
              <article v-for="decision in aggregate.decisions" :key="decision.id" class="work-card">
                <strong>{{ decision.data?.decision || decision.title }}</strong>
                <p>{{ decision.data?.reason }}</p>
                <span class="mini-status">{{ statusLabel(decision.status) }}</span>
              </article>
              <p v-if="aggregate.decisions.length === 0" class="column-empty">{{ t('work.empty_decisions') }}</p>
            </section>
          </div>

          <section class="activity-panel">
            <div class="column-heading"><History :size="16" /><h3>{{ t('work.recent_activity') }}</h3></div>
            <div v-if="aggregate.recentEvents.length" class="activity-list">
              <div v-for="event in aggregate.recentEvents.slice(0, 8)" :key="event.id" class="activity-row">
                <span class="activity-mark"></span>
                <span>{{ eventLabel(event) }}</span>
                <time>{{ formatTime(event.createdAt) }}</time>
              </div>
            </div>
            <p v-else class="column-empty">{{ t('work.empty_activity') }}</p>
          </section>
        </div>

        <div v-else class="work-empty-state">
          <Route :size="34" />
          <h2>{{ t('work.empty_title') }}</h2>
          <p>{{ t('work.empty_description') }}</p>
          <button class="primary-button" type="button" @click="beginProject">
            <Plus :size="16" />{{ t('work.new_project') }}
          </button>
        </div>
      </main>
    </div>
  </section>
</template>

<script setup>
import { computed, onMounted, reactive, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import {
  Check, CheckCircle2, ChevronRight, Circle, CircleAlert, FileDown, FolderKanban,
  History, ListChecks, LoaderCircle, Milestone, Plus, RefreshCw, Route, Scale,
  Target, X,
} from 'lucide-vue-next';

const { t, locale } = useI18n();
const projects = ref([]);
const aggregate = ref(null);
const activeProjectId = ref('');
const loading = ref(false);
const saving = ref(false);
const creatingProject = ref(false);
const errorMessage = ref('');

const projectDraft = reactive({ title: '', mission: '', currentPhase: '' });
const itemDraft = reactive({ kind: 'task', title: '', reason: '' });

const openTasks = computed(() => (aggregate.value?.tasks || []).filter(task => !['done', 'cancelled', 'archived'].includes(task.status)));
const itemPlaceholder = computed(() => ({
  task: t('work.task_hint'),
  goal: t('work.goal_hint'),
  decision: t('work.decision_hint'),
}[itemDraft.kind]));

function idempotencyKey(scope) {
  const id = globalThis.crypto?.randomUUID?.() || `${Date.now()}-${Math.random().toString(16).slice(2)}`;
  return `ui:${scope}:${id}`;
}

function beginProject() {
  creatingProject.value = true;
  errorMessage.value = '';
}

async function loadProjects() {
  loading.value = true;
  errorMessage.value = '';
  try {
    projects.value = await window.appAPI.workProjectList();
    if (!activeProjectId.value && projects.value.length) {
      await selectProject(projects.value[0].id);
    }
  } catch (error) {
    errorMessage.value = String(error);
  } finally {
    loading.value = false;
  }
}

async function selectProject(projectId) {
  activeProjectId.value = projectId;
  creatingProject.value = false;
  errorMessage.value = '';
  try {
    aggregate.value = await window.appAPI.workProjectGet(projectId);
  } catch (error) {
    aggregate.value = null;
    errorMessage.value = String(error);
  }
}

async function createProject() {
  saving.value = true;
  errorMessage.value = '';
  try {
    const project = await window.appAPI.workProjectCreate({
      title: projectDraft.title,
      mission: projectDraft.mission,
      currentPhase: projectDraft.currentPhase || null,
      metadata: {},
      actor: 'user',
      idempotencyKey: idempotencyKey('project'),
    });
    projectDraft.title = '';
    projectDraft.mission = '';
    projectDraft.currentPhase = '';
    creatingProject.value = false;
    await loadProjects();
    await selectProject(project.id);
  } catch (error) {
    errorMessage.value = String(error);
  } finally {
    saving.value = false;
  }
}

async function createWorkItem() {
  if (!aggregate.value) return;
  saving.value = true;
  errorMessage.value = '';
  try {
    const data = itemDraft.kind === 'goal'
      ? { outcome: itemDraft.title }
      : itemDraft.kind === 'decision'
        ? { decision: itemDraft.title, reason: itemDraft.reason }
        : {};
    await window.appAPI.workObjectCreate({
      kind: itemDraft.kind,
      projectId: aggregate.value.project.id,
      title: itemDraft.title,
      data,
      actor: 'user',
      idempotencyKey: idempotencyKey(itemDraft.kind),
    });
    itemDraft.title = '';
    itemDraft.reason = '';
    await selectProject(activeProjectId.value);
    await loadProjects();
  } catch (error) {
    errorMessage.value = String(error);
  } finally {
    saving.value = false;
  }
}

async function completeTask(task) {
  errorMessage.value = '';
  try {
    await window.appAPI.workObjectUpdateStatus({
      objectId: task.id,
      status: 'done',
      expectedRevision: task.revision,
      actor: 'user',
      idempotencyKey: idempotencyKey('status'),
    });
    await selectProject(activeProjectId.value);
  } catch (error) {
    errorMessage.value = String(error);
  }
}

async function exportSnapshot() {
  if (!activeProjectId.value) return;
  try {
    await window.appAPI.workProjectExportSnapshot(activeProjectId.value);
  } catch (error) {
    errorMessage.value = String(error);
  }
}

function statusLabel(status) {
  return t(`work.status_${status}`, status);
}

function eventLabel(event) {
  const type = event.eventType || '';
  if (type === 'project.created') return t('work.event_project_created');
  if (type.endsWith('.created')) return t('work.event_item_created');
  if (type.endsWith('.status_changed')) return t('work.event_status_changed');
  if (type === 'relation.created') return t('work.event_relation_created');
  return type;
}

function formatTime(timestamp) {
  if (!timestamp) return '';
  return new Intl.DateTimeFormat(locale.value, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }).format(new Date(timestamp));
}

onMounted(loadProjects);
</script>

<style scoped>
.work-view { height: 100%; overflow: auto; background: var(--bg-primary); color: var(--text-primary); padding: clamp(20px, 3vw, 36px); }
.work-header, .project-summary, .section-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: 20px; }
.work-header { max-width: 1280px; margin: 0 auto 22px; }
.work-header h1, .project-summary h2, .create-project-card h2 { margin: 3px 0 6px; font-size: clamp(22px, 2.2vw, 30px); letter-spacing: -0.03em; }
.work-header p, .project-summary p { margin: 0; color: var(--text-tertiary); }
.work-eyebrow, .section-kicker { color: var(--user-accent, var(--accent-primary)) !important; font-size: 12px; font-weight: 650; letter-spacing: .08em; text-transform: uppercase; }
.work-layout { max-width: 1280px; min-height: calc(100% - 90px); margin: 0 auto; display: grid; grid-template-columns: 240px minmax(0, 1fr); gap: 16px; }
.project-rail, .project-board, .create-project-card, .work-empty-state { border: 1px solid var(--border-subtle); border-radius: 16px; background: var(--surface-card); }
.project-rail { padding: 10px; }
.rail-heading { display: flex; justify-content: space-between; padding: 8px 9px 12px; color: var(--text-tertiary); font-size: 12px; }
.project-row { width: 100%; display: flex; align-items: center; gap: 9px; border: 0; border-radius: 10px; padding: 10px; color: var(--text-secondary); background: transparent; text-align: left; cursor: pointer; }
.project-row:hover { background: var(--surface-glass); }
.project-row.active { color: var(--user-accent, var(--accent-primary)); background: color-mix(in srgb, var(--user-accent, var(--accent-primary)) 10%, transparent); }
.project-copy { min-width: 0; flex: 1; display: flex; flex-direction: column; gap: 3px; }
.project-copy strong, .project-copy small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.project-copy strong { font-size: 13px; font-weight: 600; color: var(--text-primary); }
.project-copy small { font-size: 11px; color: var(--text-tertiary); }
.rail-empty, .work-empty-state { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; color: var(--text-muted); text-align: center; }
.rail-empty { min-height: 180px; padding: 24px 12px; font-size: 12px; }
.work-main { min-width: 0; }
.project-board, .create-project-card { padding: clamp(18px, 2.5vw, 28px); }
.create-project-card { max-width: 680px; }
.create-project-card label { display: grid; gap: 7px; margin-top: 18px; color: var(--text-secondary); font-size: 13px; }
input, textarea, select { width: 100%; box-sizing: border-box; border: 1px solid var(--border-subtle); border-radius: 9px; background: var(--surface-input); color: var(--text-primary); padding: 10px 12px; font: inherit; outline: none; }
input:focus, textarea:focus, select:focus { border-color: var(--user-accent, var(--accent-primary)); }
textarea { resize: vertical; }
.form-actions { display: flex; justify-content: flex-end; gap: 9px; margin-top: 20px; }
.primary-button, .secondary-button, .icon-button { display: inline-flex; align-items: center; justify-content: center; gap: 7px; border-radius: 9px; min-height: 36px; padding: 0 13px; font: inherit; font-size: 13px; cursor: pointer; }
.primary-button { border: 1px solid var(--user-accent, var(--accent-primary)); color: white; background: var(--user-accent, var(--accent-primary)); }
.secondary-button, .icon-button { border: 1px solid var(--border-subtle); color: var(--text-secondary); background: var(--surface-glass); }
.icon-button { width: 36px; padding: 0; }
.primary-button:disabled { opacity: .55; cursor: default; }
.primary-button.compact { white-space: nowrap; }
.project-title-line { display: flex; align-items: center; gap: 9px; }
.project-title-line h2 { margin: 0; }
.status-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--user-accent, var(--accent-primary)); }
.status-pill, .mini-status { border: 1px solid var(--border-subtle); border-radius: 999px; color: var(--text-tertiary); padding: 3px 8px; font-size: 10px; }
.project-meta { display: flex; gap: 16px; margin-top: 13px; color: var(--text-muted); font-size: 11px; }
.project-meta span { display: inline-flex; align-items: center; gap: 5px; }
.continuity-strip { display: grid; grid-template-columns: repeat(4, 1fr); margin: 24px 0 16px; border: 1px solid var(--border-subtle); border-radius: 12px; overflow: hidden; }
.continuity-strip > div { display: grid; gap: 2px; padding: 14px 16px; border-right: 1px solid var(--border-subtle); }
.continuity-strip > div:last-child { border-right: 0; }
.continuity-strip span { font-size: 19px; font-weight: 650; }
.continuity-strip small { color: var(--text-tertiary); font-size: 11px; }
.quick-add { display: grid; grid-template-columns: 120px minmax(160px, 1fr) auto; gap: 8px; margin-bottom: 18px; }
.quick-add.has-reason { grid-template-columns: 120px minmax(150px, 1fr) minmax(150px, 1fr) auto; }
.board-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px; }
.work-column, .activity-panel { border: 1px solid var(--border-subtle); border-radius: 12px; padding: 13px; background: color-mix(in srgb, var(--surface-card) 75%, transparent); }
.column-heading { display: flex; align-items: center; gap: 7px; color: var(--text-secondary); }
.column-heading h3 { margin: 0; font-size: 13px; }
.work-card { position: relative; margin-top: 10px; border: 1px solid var(--border-subtle); border-radius: 9px; padding: 11px; background: var(--bg-primary); }
.work-card strong { display: block; font-size: 12px; line-height: 1.5; }
.work-card p { margin: 5px 0 9px; color: var(--text-tertiary); font-size: 11px; line-height: 1.5; }
.task-card { display: flex; gap: 8px; }
.task-card.complete strong { color: var(--text-muted); text-decoration: line-through; }
.task-toggle { border: 0; padding: 0; color: var(--user-accent, var(--accent-primary)); background: transparent; cursor: pointer; }
.column-empty { margin: 16px 2px 4px; color: var(--text-muted); font-size: 11px; }
.activity-panel { margin-top: 10px; }
.activity-list { margin-top: 9px; }
.activity-row { display: grid; grid-template-columns: 8px 1fr auto; gap: 8px; align-items: center; min-height: 30px; color: var(--text-secondary); font-size: 11px; border-top: 1px solid var(--border-subtle); }
.activity-row:first-child { border-top: 0; }
.activity-row time { color: var(--text-muted); }
.activity-mark { width: 5px; height: 5px; border-radius: 50%; background: var(--user-accent, var(--accent-primary)); }
.work-empty-state { min-height: 420px; padding: 30px; }
.work-empty-state h2 { margin: 4px 0 0; font-size: 20px; }
.work-empty-state p { max-width: 420px; margin: 0 0 8px; color: var(--text-tertiary); }
.work-notice { max-width: 1280px; margin: 0 auto 12px; display: flex; align-items: center; gap: 8px; border: 1px solid color-mix(in srgb, var(--error) 35%, var(--border-subtle)); border-radius: 10px; padding: 9px 12px; color: var(--error); background: color-mix(in srgb, var(--error) 7%, transparent); font-size: 12px; }
.work-notice span { flex: 1; }
.work-notice button { border: 0; color: inherit; background: transparent; cursor: pointer; }
.spin { animation: spin .8s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }

@media (max-width: 820px) {
  .work-view { padding: 16px 14px 84px; }
  .work-header { align-items: center; }
  .work-header p:not(.work-eyebrow) { display: none; }
  .work-layout { grid-template-columns: 1fr; }
  .project-rail { display: flex; gap: 6px; overflow-x: auto; }
  .rail-heading, .rail-empty { display: none; }
  .project-row { min-width: 160px; }
  .project-summary { flex-direction: column; }
  .continuity-strip { grid-template-columns: repeat(2, 1fr); }
  .continuity-strip > div:nth-child(2) { border-right: 0; }
  .continuity-strip > div:nth-child(-n+2) { border-bottom: 1px solid var(--border-subtle); }
  .quick-add, .quick-add.has-reason { grid-template-columns: 1fr; }
  .board-grid { grid-template-columns: 1fr; }
}
</style>
