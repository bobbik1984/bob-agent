<template>
  <section class="work-view" :class="`layout-${layoutMode}`">
    <header class="work-header">
      <div>
        <p class="work-eyebrow">{{ t('work.eyebrow') }}</p>
        <h1>{{ t('work.title') }}</h1>
        <p>{{ t('work.subtitle') }}</p>
      </div>
      <button class="btn btn-primary" type="button" @click="beginProject">
        <Plus :size="16" />
        {{ t('work.new_project') }}
      </button>
    </header>

    <div v-if="errorMessage" class="work-notice error" role="alert">
      <CircleAlert :size="16" />
      <span>{{ errorMessage }}</span>
      <button type="button" :aria-label="t('common.close')" @click="errorMessage = ''"><X :size="14" /></button>
    </div>

    <section v-if="pendingLinks.length" class="assignment-panel" aria-live="polite">
      <div class="assignment-heading">
        <div>
          <p class="section-kicker">{{ t('work.assignment_kicker') }}</p>
          <h2><Link2 :size="18" />{{ t('work.assignment_title') }}</h2>
          <p>{{ t('work.assignment_description') }}</p>
        </div>
        <span class="assignment-count">{{ pendingLinks.length }}</span>
      </div>
      <div class="assignment-list">
        <article v-for="candidate in pendingLinks" :key="candidate.id" class="assignment-card">
          <div class="assignment-copy">
            <strong>{{ candidate.title }}</strong>
            <span>{{ candidate.projectHint || t('work.assignment_no_hint') }}</span>
            <small>{{ candidateReason(candidate.reasonCode) }}</small>
          </div>
          <div class="assignment-controls">
            <select v-model="candidateDrafts[candidate.id].projectId" :aria-label="t('work.assignment_project')">
              <option value="" disabled>{{ t('work.assignment_select') }}</option>
              <option v-for="project in projects" :key="project.id" :value="project.id">{{ project.title }}</option>
            </select>
            <input
              v-if="candidate.intent === 'decision' && candidate.reasonCode === 'missing_decision_reason'"
              v-model.trim="candidateDrafts[candidate.id].reason"
              :placeholder="t('work.assignment_reason_hint')"
            />
            <input
              v-if="candidate.intent === 'commitment' && candidate.reasonCode === 'missing_commitment_owner'"
              v-model.trim="candidateDrafts[candidate.id].owner"
              :placeholder="t('work.assignment_owner_hint')"
            />
            <input
              v-if="candidate.intent === 'commitment' && candidate.reasonCode === 'missing_commitment_due_at'"
              v-model.trim="candidateDrafts[candidate.id].dueAt"
              :placeholder="t('work.assignment_due_hint')"
            />
            <button class="btn btn-primary btn-compact" type="button" :disabled="saving || !candidateDrafts[candidate.id].projectId" @click="resolveCandidate(candidate)">
              <Check :size="15" />{{ t('work.assignment_confirm') }}
            </button>
            <button class="btn btn-secondary btn-compact" type="button" :disabled="saving" @click="dismissCandidate(candidate)">
              <X :size="15" />{{ t('work.assignment_dismiss') }}
            </button>
          </div>
        </article>
      </div>
    </section>

    <section v-if="pendingChangeReviews.length" class="assignment-panel change-review-panel" aria-live="polite">
      <div class="assignment-heading">
        <div>
          <p class="section-kicker">{{ t('work.change_review_kicker') }}</p>
          <h2><GitCompareArrows :size="18" />{{ t('work.change_review_title') }}</h2>
          <p>{{ t('work.change_review_description') }}</p>
        </div>
        <span class="assignment-count">{{ pendingChangeReviews.length }}</span>
      </div>
      <div class="assignment-list">
        <article v-for="review in pendingChangeReviews" :key="review.id" class="assignment-card change-review-card">
          <div class="assignment-copy">
            <strong>{{ review.changeTitle }}</strong>
            <span>{{ t('work.change_review_affects', { kind: targetKindLabel(review.targetKind), title: review.targetTitle }) }}</span>
            <small>{{ changeReason(review.reasonCode) }}</small>
          </div>
          <div class="assignment-controls change-review-controls">
            <input
              v-model.trim="changeReviewDrafts[review.id]"
              :placeholder="t('work.change_review_note_hint')"
            />
            <button class="btn btn-primary btn-compact" type="button" :disabled="saving" @click="handleChangeReview(review, 'accept')">
              <Check :size="15" />{{ t('work.change_review_accept') }}
            </button>
            <button class="btn btn-secondary btn-compact" type="button" :disabled="saving" @click="handleChangeReview(review, 'reject')">
              <X :size="15" />{{ t('work.change_review_reject') }}
            </button>
            <button class="btn btn-secondary btn-compact" type="button" :disabled="saving" @click="handleChangeReview(review, 'defer')">
              <Clock3 :size="15" />{{ t('work.change_review_defer') }}
            </button>
          </div>
        </article>
      </div>
    </section>

    <details v-if="deferredChangeReviews.length" class="assignment-panel deferred-review-panel">
      <summary>
        <span><Clock3 :size="16" />{{ t('work.change_review_deferred_title') }}</span>
        <span class="assignment-count">{{ deferredChangeReviews.length }}</span>
      </summary>
      <div class="assignment-list">
        <article v-for="review in deferredChangeReviews" :key="review.id" class="assignment-card change-review-card">
          <div class="assignment-copy">
            <strong>{{ review.changeTitle }}</strong>
            <span>{{ t('work.change_review_affects', { kind: targetKindLabel(review.targetKind), title: review.targetTitle }) }}</span>
            <small>{{ changeReason(review.reasonCode) }}</small>
          </div>
          <div class="assignment-controls deferred-review-controls">
            <button class="btn btn-secondary btn-compact" type="button" :disabled="saving" @click="handleChangeReview(review, 'reopen')">
              <RefreshCw :size="15" />{{ t('work.change_review_reopen') }}
            </button>
          </div>
        </article>
      </div>
    </details>

    <nav v-if="projects.length && terminalKind !== 'native-mobile'" class="project-switcher" :aria-label="t('work.projects')">
      <button
        v-for="project in projects"
        :key="project.id"
        class="project-option"
        :class="{ active: project.id === activeProjectId }"
        type="button"
        :aria-current="project.id === activeProjectId ? 'page' : undefined"
        @click="selectProject(project.id)"
      >
        <span class="project-state-dot" aria-hidden="true"></span>
        <span>{{ project.title }}</span>
      </button>
    </nav>

    <label v-else-if="projects.length" class="project-select-wrap">
      <span class="sr-only">{{ t('work.projects') }}</span>
      <span class="project-state-dot active" aria-hidden="true"></span>
      <select :value="activeProjectId" @change="selectProject($event.target.value)">
        <option v-for="project in projects" :key="project.id" :value="project.id">{{ project.title }}</option>
      </select>
    </label>

    <main class="work-main">
        <form v-if="creatingProject" class="create-project-card" @submit.prevent="createProject">
          <div class="section-heading">
            <div>
              <p class="section-kicker">{{ t('work.project_definition') }}</p>
              <h2>{{ t('work.create_title') }}</h2>
            </div>
            <button class="btn btn-icon" type="button" :aria-label="t('common.close')" @click="creatingProject = false">
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
            <button class="btn btn-secondary" type="button" @click="creatingProject = false">{{ t('common.cancel') }}</button>
            <button class="btn btn-primary" type="submit" :disabled="saving || !projectDraft.title">
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
            <button class="btn btn-secondary" type="button" @click="exportSnapshot">
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
            <button class="btn btn-primary btn-compact" type="submit" :disabled="saving || !itemDraft.title">
              <Plus :size="16" />
              {{ t('work.add') }}
            </button>
          </form>

          <div class="board-grid">
            <section class="work-column">
              <div class="column-heading"><Target :size="16" /><h3>{{ t('work.goals') }}</h3></div>
              <article v-for="goal in aggregate.goals" :key="goal.id" class="work-card" :data-work-object-id="goal.id">
                <strong>{{ goal.title }}</strong>
                <p>{{ goal.data?.outcome || goal.description }}</p>
                <span class="mini-status">{{ statusLabel(goal.status) }}</span>
                <div v-if="runtimeByGoal[goal.id]" class="runtime-card">
                  <div class="runtime-state-line">
                    <span class="runtime-dot" :class="runtimeByGoal[goal.id].run.status"></span>
                    <strong>{{ runtimeStatusLabel(runtimeByGoal[goal.id].run.status) }}</strong>
                    <small>{{ runtimePhaseLabel(runtimeByGoal[goal.id].run.phase) }}</small>
                  </div>
                  <p v-if="runtimeByGoal[goal.id].run.nextAction" class="runtime-next">
                    {{ runtimeText(runtimeByGoal[goal.id].run.nextAction) }}
                  </p>
                  <div class="runtime-meta">
                    <span>{{ t('goal.verification') }} · {{ verificationLabel(runtimeByGoal[goal.id].run.verificationState) }}</span>
                    <span>{{ t('goal.risk') }} · {{ String(runtimeByGoal[goal.id].run.risk).toUpperCase() }}</span>
                    <span>{{ t('goal.calls', { model: runtimeByGoal[goal.id].run.modelCallsUsed, tools: runtimeByGoal[goal.id].run.toolCallsUsed }) }}</span>
                    <span v-if="runtimeByGoal[goal.id].run.latestCheckpointId">{{ t('goal.checkpoint_saved') }}</span>
                  </div>
                  <p v-if="runtimeByGoal[goal.id].run.lastErrorCode" class="runtime-error-detail">
                    {{ runtimeByGoal[goal.id].run.lastErrorCode }} · {{ runtimeErrorText(runtimeByGoal[goal.id].run) }}
                  </p>
                  <div v-if="runtimeByGoal[goal.id].pendingApproval" class="runtime-approval">
                    <p>{{ runtimeText(runtimeByGoal[goal.id].pendingApproval.summary) }}</p>
                    <div class="runtime-actions">
                      <button
                        v-for="choice in runtimeByGoal[goal.id].pendingApproval.choices"
                        :key="choice.choiceId"
                        type="button"
                        class="btn btn-secondary btn-compact"
                        :class="{ 'btn-selected': choice.semantic === 'approve' || choice.semantic === 'select_option' }"
                        :disabled="runtimeBusy === runtimeByGoal[goal.id].run.runId"
                        @click="handleApproval(runtimeByGoal[goal.id], choice)"
                      >
                        {{ t(choice.labelKey) }}
                      </button>
                    </div>
                  </div>
                  <div v-else-if="!isTerminalRuntime(runtimeByGoal[goal.id].run.status)" class="runtime-actions">
                    <button
                      v-if="['ready', 'blocked', 'waiting_user'].includes(runtimeByGoal[goal.id].run.status)"
                      type="button" class="btn btn-secondary btn-compact btn-selected"
                      :disabled="runtimeBusy === runtimeByGoal[goal.id].run.runId"
                      @click="handleRuntimeAction(runtimeByGoal[goal.id], 'continue')"
                    >{{ t('goal.continue') }}</button>
                    <button
                      v-if="!['waiting_user'].includes(runtimeByGoal[goal.id].run.status)"
                      type="button" class="btn btn-secondary btn-compact"
                      :disabled="runtimeBusy === runtimeByGoal[goal.id].run.runId"
                      @click="handleRuntimeAction(runtimeByGoal[goal.id], 'defer')"
                    >{{ t('goal.defer') }}</button>
                    <button type="button" class="btn btn-secondary btn-compact"
                      :disabled="runtimeBusy === runtimeByGoal[goal.id].run.runId"
                      @click="handleRuntimeAction(runtimeByGoal[goal.id], 'cancel')"
                    >{{ t('goal.cancel') }}</button>
                  </div>
                </div>
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
                <div v-if="hasDecisionDetails(decision)" class="decision-details">
                  <span v-if="decision.data?.owner">{{ t('work.decision_owner', { value: decision.data.owner }) }}</span>
                  <span v-if="decision.data?.participants?.length">{{ t('work.decision_participants', { value: decision.data.participants.join('、') }) }}</span>
                  <span v-if="decision.data?.alternatives?.length">{{ t('work.decision_alternatives', { value: decision.data.alternatives.join('；') }) }}</span>
                  <span v-if="decision.data?.evidence?.length">{{ t('work.decision_evidence', { count: decision.data.evidence.length }) }}</span>
                  <span v-if="decision.data?.revisitCondition">{{ t('work.decision_revisit', { value: decision.data.revisitCondition }) }}</span>
                </div>
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
          <button class="btn btn-primary" type="button" @click="beginProject">
            <Plus :size="16" />{{ t('work.new_project') }}
          </button>
        </div>
    </main>
  </section>
</template>

<script setup>
import { computed, inject, onBeforeUnmount, onMounted, reactive, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import {
  Check, CheckCircle2, Circle, CircleAlert, FileDown,
  Clock3, GitCompareArrows, History, Link2, ListChecks, LoaderCircle, Milestone, Plus, RefreshCw, Route, Scale,
  Target, X,
} from 'lucide-vue-next';

const { t, locale } = useI18n();
const layoutMode = inject('layoutMode', ref('desktop-wide'));
const terminalKind = inject('terminalKind', 'desktop');
const projects = ref([]);
const aggregate = ref(null);
const activeProjectId = ref('');
const loading = ref(false);
const saving = ref(false);
const creatingProject = ref(false);
const errorMessage = ref('');
const pendingLinks = ref([]);
const candidateDrafts = reactive({});
const pendingChangeReviews = ref([]);
const deferredChangeReviews = ref([]);
const changeReviewDrafts = reactive({});
const runtimeRuns = ref([]);
const runtimeBusy = ref('');
let stopRuntimeListener = null;

const projectDraft = reactive({ title: '', mission: '', currentPhase: '' });
const itemDraft = reactive({ kind: 'task', title: '', reason: '' });

const openTasks = computed(() => (aggregate.value?.tasks || []).filter(task => !['done', 'cancelled', 'archived'].includes(task.status)));
const runtimeByGoal = computed(() => Object.fromEntries(runtimeRuns.value.map(item => [item.run.goalId, item])));
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
    await loadPendingLinks();
    await loadChangeReviews();
    if (!activeProjectId.value && projects.value.length) {
      await selectProject(projects.value[0].id);
    }
  } catch (error) {
    errorMessage.value = String(error);
  } finally {
    loading.value = false;
  }
}

async function loadChangeReviews() {
  [pendingChangeReviews.value, deferredChangeReviews.value] = await Promise.all([
    window.appAPI.workChangeReviewList({ status: 'pending', limit: 50 }),
    window.appAPI.workChangeReviewList({ status: 'deferred', limit: 50 }),
  ]);
  for (const review of pendingChangeReviews.value) {
    changeReviewDrafts[review.id] ||= '';
  }
}

async function handleChangeReview(review, action) {
  saving.value = true;
  errorMessage.value = '';
  try {
    await window.appAPI.workChangeReviewAction({
      reviewId: review.id,
      action,
      expectedRevision: review.revision,
      note: changeReviewDrafts[review.id] || null,
    });
    delete changeReviewDrafts[review.id];
    await loadChangeReviews();
    await loadProjects();
    if (activeProjectId.value) await selectProject(activeProjectId.value);
  } catch (error) {
    errorMessage.value = String(error);
    await loadChangeReviews();
  } finally {
    saving.value = false;
  }
}

async function loadPendingLinks() {
  pendingLinks.value = await window.appAPI.workProjectLinkListPending(50);
  for (const candidate of pendingLinks.value) {
    const firstCandidate = candidate.selectedProjectId || candidate.candidateProjectIds?.[0] || '';
    candidateDrafts[candidate.id] ||= { projectId: firstCandidate, reason: '', owner: '', dueAt: '' };
  }
}

async function resolveCandidate(candidate) {
  const draft = candidateDrafts[candidate.id];
  saving.value = true;
  errorMessage.value = '';
  try {
    const outcome = await window.appAPI.workProjectLinkResolve({
      candidateId: candidate.id,
      projectId: draft.projectId,
      expectedRevision: candidate.revision,
      reason: draft.reason || null,
      owner: draft.owner || null,
      dueAt: draft.dueAt || null,
    });
    delete candidateDrafts[candidate.id];
    await loadPendingLinks();
    await loadProjects();
    if (outcome.candidate?.selectedProjectId) await selectProject(outcome.candidate.selectedProjectId);
  } catch (error) {
    errorMessage.value = String(error);
    await loadPendingLinks();
  } finally {
    saving.value = false;
  }
}

async function dismissCandidate(candidate) {
  saving.value = true;
  errorMessage.value = '';
  try {
    await window.appAPI.workProjectLinkDismiss({ candidateId: candidate.id, expectedRevision: candidate.revision });
    delete candidateDrafts[candidate.id];
    await loadPendingLinks();
  } catch (error) {
    errorMessage.value = String(error);
    await loadPendingLinks();
  } finally {
    saving.value = false;
  }
}

function candidateReason(reasonCode) {
  return t(`work.assignment_${reasonCode}`, t('work.assignment_unknown'));
}

function changeReason(reasonCode) {
  return t(`work.change_review_reason_${reasonCode}`, t('work.change_review_reason_unknown'));
}

function targetKindLabel(kind) {
  return t(`work.kind_${kind}`, kind);
}

function hasDecisionDetails(decision) {
  const data = decision?.data || {};
  return Boolean(data.owner || data.revisitCondition || data.participants?.length || data.alternatives?.length || data.evidence?.length);
}

async function selectProject(projectId) {
  activeProjectId.value = projectId;
  creatingProject.value = false;
  errorMessage.value = '';
  try {
    [aggregate.value, runtimeRuns.value] = await Promise.all([
      window.appAPI.workProjectGet(projectId),
      window.appAPI.goalRuntimeList({ projectId, limit: 50 }),
    ]);
  } catch (error) {
    aggregate.value = null;
    errorMessage.value = String(error);
  }
}

async function refreshRuntime() {
  if (!activeProjectId.value) return;
  runtimeRuns.value = await window.appAPI.goalRuntimeList({ projectId: activeProjectId.value, limit: 50 });
}

async function handleApproval(runtime, choice) {
  runtimeBusy.value = runtime.run.runId;
  errorMessage.value = '';
  try {
    const outcome = await window.appAPI.goalRuntimeDecideApproval({
      approvalId: runtime.pendingApproval.approvalId,
      choiceId: choice.choiceId,
      expectedRevision: runtime.pendingApproval.revision,
      actor: 'user',
      deviceId: 'desktop',
      inputModality: 'pointer',
      trustedDevice: true,
      idempotencyKey: idempotencyKey('goal-approval'),
    });
    if (outcome.run?.status === 'ready' && ['approve', 'select_option'].includes(choice.semantic)) {
      await window.appAPI.goalRuntimeContinue({
        runId: outcome.run.runId,
        expectedRevision: outcome.run.revision,
        idempotencyKey: idempotencyKey('goal-approved-continue'),
      });
    }
    await refreshRuntime();
    await selectProject(activeProjectId.value);
  } catch (error) {
    errorMessage.value = String(error);
    await refreshRuntime();
  } finally {
    runtimeBusy.value = '';
  }
}

async function handleRuntimeAction(runtime, action) {
  runtimeBusy.value = runtime.run.runId;
  errorMessage.value = '';
  try {
    const input = {
      runId: runtime.run.runId,
      expectedRevision: runtime.run.revision,
      idempotencyKey: idempotencyKey(`goal-${action}`),
    };
    if (action === 'continue') await window.appAPI.goalRuntimeContinue(input);
    if (action === 'defer') await window.appAPI.goalRuntimeDefer(input);
    if (action === 'cancel') await window.appAPI.goalRuntimeCancel(input);
    await refreshRuntime();
    await selectProject(activeProjectId.value);
  } catch (error) {
    errorMessage.value = String(error);
    await refreshRuntime();
  } finally {
    runtimeBusy.value = '';
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

function runtimeStatusLabel(status) { return t(`goal.status_${status}`, status); }
function runtimePhaseLabel(phase) { return t(`goal.phase_${phase}`, phase); }
function verificationLabel(state) { return t(`goal.verification_${state}`, state); }
function isTerminalRuntime(status) { return ['done', 'failed', 'cancelled'].includes(status); }
function runtimeText(value) { return value?.startsWith?.('goal.') ? t(value) : value; }

const runtimeErrorKeys = {
  'GOAL-EVIDENCE-UNVERIFIED': 'goal.error_evidence_unverified',
  'GOAL-SLICE-TIMEOUT': 'goal.error_slice_timeout',
  'GOAL-BUDGET-MODEL': 'goal.error_budget_model',
  'GOAL-BUDGET-TOOL': 'goal.error_budget_tool',
  'GOAL-BUDGET-RUNTIME': 'goal.error_budget_runtime',
  'GOAL-BUDGET-REPAIR': 'goal.error_budget_repair',
  'GOAL-RUNTIME-BUSY': 'goal.error_runtime_busy',
};

function runtimeErrorText(run) {
  const key = runtimeErrorKeys[run.lastErrorCode];
  return key ? t(key) : (run.lastErrorDetail || t('goal.error_unknown'));
}

function eventLabel(event) {
  const type = event.eventType || '';
  if (type === 'project.created') return t('work.event_project_created');
  if (type.endsWith('.created')) return t('work.event_item_created');
  if (type.endsWith('.status_changed')) return t('work.event_status_changed');
  if (type === 'relation.created') return t('work.event_relation_created');
  if (type === 'external_link.recorded') return t('work.event_external_link_recorded');
  if (type.startsWith('change_review.')) return t(`work.event_${type.replace('.', '_')}`, type);
  return type;
}

function formatTime(timestamp) {
  if (!timestamp) return '';
  return new Intl.DateTimeFormat(locale.value, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }).format(new Date(timestamp));
}

async function handleTodayBriefNavigation(event) {
  const item = event.detail;
  const projectId = item?.action?.payload?.projectId || item?.messageArgs?.projectId;
  const objectId = item?.action?.targetId || item?.action?.payload?.goalId;
  if (projectId) await selectProject(projectId);
  requestAnimationFrame(() => {
    const target = objectId ? document.querySelector(`[data-work-object-id="${CSS.escape(objectId)}"]`) : null;
    target?.scrollIntoView({ behavior: 'smooth', block: 'center' });
  });
}

onMounted(async () => {
  await loadProjects();
  stopRuntimeListener = await window.appAPI.listenEvent('goal:runtime-state', () => refreshRuntime());
  window.addEventListener('today-brief-action', handleTodayBriefNavigation);
});
onBeforeUnmount(() => {
  stopRuntimeListener?.();
  window.removeEventListener('today-brief-action', handleTodayBriefNavigation);
});
</script>

<style scoped>
.work-view { height: 100%; overflow: auto; box-sizing: border-box; background: var(--bg-primary); color: var(--text-primary); padding: clamp(20px, 3vw, 36px); }
.work-header, .project-summary, .section-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: 20px; }
.work-header { max-width: 1000px; margin: 0 auto 18px; }
.work-header h1, .project-summary h2, .create-project-card h2 { margin: 3px 0 6px; font-size: clamp(22px, 2.2vw, 30px); letter-spacing: -0.03em; }
.work-header p, .project-summary p { margin: 0; color: var(--text-tertiary); }
.work-eyebrow, .section-kicker { color: var(--user-accent, var(--accent-primary)) !important; font-size: 12px; font-weight: 650; letter-spacing: .08em; text-transform: uppercase; }
.assignment-panel { max-width: 1000px; margin: 0 auto 16px; border: 1px solid var(--border-subtle); border-radius: 14px; padding: 15px; background: var(--surface-card); }
.assignment-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; }
.assignment-heading h2 { display: flex; align-items: center; gap: 8px; margin: 3px 0 4px; font-size: 16px; }
.assignment-heading p { margin: 0; color: var(--text-tertiary); font-size: 12px; }
.assignment-count { min-width: 25px; height: 25px; display: inline-grid; place-items: center; border: 1px solid var(--border-subtle); border-radius: 999px; color: var(--user-accent, var(--accent-primary)); font-size: 11px; }
.assignment-list { display: grid; gap: 8px; margin-top: 13px; }
.assignment-card { display: grid; grid-template-columns: minmax(180px, .8fr) minmax(320px, 1.2fr); gap: 12px; align-items: center; border-top: 1px solid var(--border-subtle); padding-top: 10px; }
.assignment-card:first-child { border-top: 0; padding-top: 0; }
.assignment-copy { min-width: 0; display: grid; gap: 3px; }
.assignment-copy strong, .assignment-copy span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.assignment-copy strong { font-size: 13px; }
.assignment-copy span, .assignment-copy small { color: var(--text-tertiary); font-size: 11px; }
.assignment-controls { display: flex; gap: 7px; align-items: center; }
.assignment-controls select, .assignment-controls input { min-width: 0; padding: 8px 10px; }
.assignment-controls select { flex: 1; }
.assignment-controls input { flex: 1.2; }
.change-review-panel { border-color: color-mix(in srgb, var(--user-accent, var(--accent-primary)) 24%, var(--border-subtle)); }
.change-review-controls input { flex: 1; }
.change-review-controls .btn { white-space: nowrap; }
.deferred-review-panel { padding: 11px 15px; }
.deferred-review-panel summary { display: flex; align-items: center; justify-content: space-between; gap: 12px; color: var(--text-secondary); cursor: pointer; list-style: none; font-size: 12px; }
.deferred-review-panel summary::-webkit-details-marker { display: none; }
.deferred-review-panel summary > span:first-child { display: inline-flex; align-items: center; gap: 7px; }
.deferred-review-controls { justify-content: flex-end; }
.project-switcher { max-width: 1000px; margin: 0 auto 12px; display: flex; align-items: center; gap: 4px; overflow-x: auto; border-bottom: 1px solid var(--border-subtle); padding: 0 0 8px; scrollbar-width: thin; }
.project-option { min-height: 30px; max-width: 240px; display: inline-flex; flex: 0 0 auto; align-items: center; gap: 8px; border: 1px solid transparent; border-radius: 8px; padding: 0 10px; color: var(--text-secondary); background: transparent; font: inherit; font-size: 12px; cursor: pointer; }
.project-option > span:last-child { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.project-option:hover { color: var(--text-primary); background: var(--surface-glass); }
.project-option.active { color: var(--user-accent, var(--accent-primary)); border-color: color-mix(in srgb, var(--user-accent, var(--accent-primary)) 24%, transparent); background: color-mix(in srgb, var(--user-accent, var(--accent-primary)) 8%, transparent); }
.project-state-dot { width: 7px; height: 7px; flex: 0 0 auto; box-sizing: border-box; border: 1.5px solid var(--text-muted); border-radius: 50%; background: transparent; }
.project-option.active .project-state-dot, .project-state-dot.active { border-color: var(--user-accent, var(--accent-primary)); background: var(--user-accent, var(--accent-primary)); }
.project-select-wrap { max-width: 1000px; min-height: 38px; margin: 0 auto 12px; display: flex; align-items: center; gap: 8px; border-bottom: 1px solid var(--border-subtle); padding: 0 2px 8px; }
.project-select-wrap select { min-width: 0; border: 0; padding: 7px 28px 7px 0; background: transparent; font-weight: 600; }
.project-select-wrap select:focus { border: 0; }
.project-board, .create-project-card, .work-empty-state { border: 1px solid var(--border-subtle); border-radius: 16px; background: var(--surface-card); }
.work-empty-state { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; color: var(--text-muted); text-align: center; }
.work-main { min-width: 0; max-width: 1000px; margin: 0 auto; }
.project-board, .create-project-card { padding: clamp(18px, 2.5vw, 28px); }
.create-project-card { max-width: 680px; }
.create-project-card label { display: grid; gap: 7px; margin-top: 18px; color: var(--text-secondary); font-size: 13px; }
input, textarea, select { width: 100%; box-sizing: border-box; border: 1px solid var(--border-subtle); border-radius: 9px; background: var(--surface-input); color: var(--text-primary); padding: 10px 12px; font: inherit; outline: none; }
input:focus, textarea:focus, select:focus { border-color: var(--user-accent, var(--accent-primary)); }
textarea { resize: vertical; }
.form-actions { display: flex; justify-content: flex-end; gap: 9px; margin-top: 20px; }
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
.runtime-card { margin-top: 10px; padding-top: 9px; border-top: 1px solid var(--border-subtle); }
.runtime-state-line { display: grid; grid-template-columns: 7px minmax(0, 1fr) auto; gap: 7px; align-items: center; }
.runtime-state-line strong { color: var(--text-secondary); font-size: 11px; }
.runtime-state-line small { color: var(--text-muted); font-size: 10px; }
.runtime-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--text-muted); }
.runtime-dot.running, .runtime-dot.verifying, .runtime-dot.ready { background: var(--user-accent, var(--accent-primary)); }
.runtime-dot.done { background: var(--accent-primary); }
.runtime-dot.blocked, .runtime-dot.failed { background: var(--text-muted); }
.work-card .runtime-next { margin: 5px 0; }
.runtime-meta { display: flex; flex-wrap: wrap; gap: 5px 9px; color: var(--text-muted); font-size: 10px; }
.work-card .runtime-error-detail { margin: 7px 0 0; font-family: var(--font-mono); color: var(--text-secondary); }
.runtime-approval { margin-top: 8px; padding-top: 8px; border-top: 1px solid var(--border-subtle); }
.work-card .runtime-approval p { margin: 0 0 7px; color: var(--text-secondary); }
.runtime-actions { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 8px; }
.runtime-actions .btn { min-height: 28px; padding: 0 9px; font-size: 10px; }
.decision-details { display: grid; gap: 3px; margin: -2px 0 9px; color: var(--text-muted); font-size: 10px; line-height: 1.45; }
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
.work-notice { max-width: 1000px; margin: 0 auto 12px; display: flex; align-items: center; gap: 8px; border: 1px solid color-mix(in srgb, var(--error) 35%, var(--border-subtle)); border-radius: 10px; padding: 9px 12px; color: var(--error); background: color-mix(in srgb, var(--error) 7%, transparent); font-size: 12px; }
.work-notice span { flex: 1; }
.work-notice button { border: 0; color: inherit; background: transparent; cursor: pointer; }
.spin { animation: spin .8s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }

.work-view.layout-desktop-compact,
.work-view.layout-mobile-native { padding: 16px 14px 84px; }
.layout-desktop-compact .work-header,
.layout-mobile-native .work-header { align-items: center; }
.layout-desktop-compact .work-header p:not(.work-eyebrow),
.layout-mobile-native .work-header p:not(.work-eyebrow) { display: none; }
.layout-desktop-compact .assignment-card,
.layout-mobile-native .assignment-card { grid-template-columns: 1fr; }
.layout-mobile-native .assignment-card:nth-child(n+4) { display: none; }
.layout-desktop-compact .assignment-controls,
.layout-mobile-native .assignment-controls { display: grid; grid-template-columns: 1fr 1fr; }
.layout-desktop-compact .assignment-controls select,
.layout-desktop-compact .assignment-controls input,
.layout-mobile-native .assignment-controls select,
.layout-mobile-native .assignment-controls input { grid-column: 1 / -1; }
.layout-desktop-compact .change-review-controls,
.layout-mobile-native .change-review-controls { grid-template-columns: repeat(3, 1fr); }
.layout-desktop-compact .project-summary,
.layout-mobile-native .project-summary { flex-direction: column; }
.layout-desktop-compact .continuity-strip,
.layout-mobile-native .continuity-strip { grid-template-columns: repeat(2, 1fr); }
.layout-desktop-compact .continuity-strip > div:nth-child(2),
.layout-mobile-native .continuity-strip > div:nth-child(2) { border-right: 0; }
.layout-desktop-compact .continuity-strip > div:nth-child(-n+2),
.layout-mobile-native .continuity-strip > div:nth-child(-n+2) { border-bottom: 1px solid var(--border-subtle); }
.layout-desktop-compact .quick-add,
.layout-desktop-compact .quick-add.has-reason,
.layout-mobile-native .quick-add,
.layout-mobile-native .quick-add.has-reason { grid-template-columns: 1fr; }
.layout-desktop-compact .board-grid,
.layout-mobile-native .board-grid { grid-template-columns: 1fr; }
</style>
