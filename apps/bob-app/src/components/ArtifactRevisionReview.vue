<script>
export function computeLineDiff(oldText = '', newText = '') {
  const oldStr = oldText == null ? '' : String(oldText);
  const newStr = newText == null ? '' : String(newText);
  if (oldStr === '' && newStr === '') return [];

  const oldLines = oldStr ? oldStr.split(/\r?\n/) : [];
  const newLines = newStr ? newStr.split(/\r?\n/) : [];

  if (oldLines.length === 0) {
    return newLines.map((text, i) => ({
      type: 'added',
      oldLine: null,
      newLine: i + 1,
      text,
    }));
  }
  if (newLines.length === 0) {
    return oldLines.map((text, i) => ({
      type: 'removed',
      oldLine: i + 1,
      newLine: null,
      text,
    }));
  }

  const m = oldLines.length;
  const n = newLines.length;
  const dp = Array.from({ length: m + 1 }, () => new Uint32Array(n + 1));

  for (let i = 0; i < m; i++) {
    for (let j = 0; j < n; j++) {
      if (oldLines[i] === newLines[j]) {
        dp[i + 1][j + 1] = dp[i][j] + 1;
      } else {
        dp[i + 1][j + 1] = Math.max(dp[i + 1][j], dp[i][j + 1]);
      }
    }
  }

  const result = [];
  let i = m;
  let j = n;
  while (i > 0 || j > 0) {
    if (i > 0 && j > 0 && oldLines[i - 1] === newLines[j - 1]) {
      result.push({
        type: 'unchanged',
        oldLine: i,
        newLine: j,
        text: oldLines[i - 1],
      });
      i--;
      j--;
    } else if (j > 0 && (i === 0 || dp[i][j - 1] >= dp[i - 1][j])) {
      result.push({
        type: 'added',
        oldLine: null,
        newLine: j,
        text: newLines[j - 1],
      });
      j--;
    } else if (i > 0 && (j === 0 || dp[i][j - 1] < dp[i - 1][j])) {
      result.push({
        type: 'removed',
        oldLine: i,
        newLine: null,
        text: oldLines[i - 1],
      });
      i--;
    }
  }

  return result.reverse();
}

export function buildRevisionDispatchPayload({
  artifactPath = '',
  parentVersionId = 'V1',
  annotations = [],
  summaryInstruction = '',
} = {}) {
  const normParent = parentVersionId || 'V1';
  const normPath = artifactPath || 'deliverable';
  const normSummary = (summaryInstruction || '').trim();
  const normAnnotations = (Array.isArray(annotations) ? annotations : []).map((ann, idx) => ({
    id: ann.id || `ann-${idx + 1}`,
    targetType: ann.targetType || (ann.rect ? 'region' : 'line'),
    lineStart: typeof ann.lineStart === 'number' ? ann.lineStart : null,
    lineEnd: typeof ann.lineEnd === 'number' ? ann.lineEnd : null,
    rect: ann.rect
      ? {
          xPct: Math.round(Number(ann.rect.xPct) || 0),
          yPct: Math.round(Number(ann.rect.yPct) || 0),
          wPct: Math.round(Number(ann.rect.wPct) || 0),
          hPct: Math.round(Number(ann.rect.hPct) || 0),
        }
      : null,
    comment: (ann.comment || '').trim(),
  }));

  let prompt = `[Revision Request for ${normPath} (Base: ${normParent})]`;
  if (normSummary) {
    prompt += `\n\nSummary Instruction:\n${normSummary}`;
  }
  if (normAnnotations.length > 0) {
    prompt += `\n\nInline Annotations:`;
    normAnnotations.forEach((ann, idx) => {
      if (ann.targetType === 'region' && ann.rect) {
        prompt += `\n${idx + 1}. [Region: x=${ann.rect.xPct}%, y=${ann.rect.yPct}%, w=${ann.rect.wPct}%, h=${ann.rect.hPct}%] ${ann.comment}`;
      } else if (ann.lineStart != null) {
        const lineSpan =
          ann.lineEnd != null && ann.lineEnd !== ann.lineStart
            ? `Lines ${ann.lineStart}-${ann.lineEnd}`
            : `Line ${ann.lineStart}`;
        prompt += `\n${idx + 1}. [${lineSpan}] ${ann.comment}`;
      } else {
        prompt += `\n${idx + 1}. ${ann.comment}`;
      }
    });
  }

  return {
    parentVersionId: normParent,
    artifactPath: normPath,
    instruction: prompt,
    summaryInstruction: normSummary,
    annotations: normAnnotations,
  };
}
</script>

<script setup>
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

const props = defineProps({
  versions: {
    type: Array,
    default: () => [],
  },
  activeVersionId: {
    type: String,
    default: null,
  },
  busy: {
    type: Boolean,
    default: false,
  },
});

const emit = defineEmits(['dispatch-revision', 'approve-version', 'close']);

const { t } = useI18n();

const currentMode = ref(props.versions?.length >= 2 ? 'side-by-side' : 'single');
const baseVersionId = ref(props.versions?.[0]?.id || 'V1');
const targetVersionId = ref(props.versions?.[props.versions.length - 1]?.id || 'V2');
const quickFlipTarget = ref('target');

watch(
  () => props.versions,
  newVersions => {
    if (newVersions?.length) {
      if (!newVersions.some(v => v.id === baseVersionId.value)) {
        baseVersionId.value = newVersions[0].id;
      }
      if (!newVersions.some(v => v.id === targetVersionId.value)) {
        targetVersionId.value = newVersions[newVersions.length - 1].id;
      }
    }
  },
  { immediate: true },
);

watch(
  () => props.activeVersionId,
  newId => {
    if (newId && props.versions?.some(v => v.id === newId)) {
      targetVersionId.value = newId;
    }
  },
);

const baseVersion = computed(() => {
  return props.versions.find(v => v.id === baseVersionId.value) || props.versions[0] || null;
});

const targetVersion = computed(() => {
  return (
    props.versions.find(v => v.id === targetVersionId.value) ||
    props.versions[props.versions.length - 1] ||
    baseVersion.value ||
    null
  );
});

const currentDisplayVersion = computed(() => {
  if (currentMode.value === 'single') {
    return baseVersion.value;
  }
  if (currentMode.value === 'toggle') {
    return quickFlipTarget.value === 'base' ? baseVersion.value : targetVersion.value;
  }
  return targetVersion.value;
});

const isImage = computed(() => {
  const v = currentDisplayVersion.value || baseVersion.value || targetVersion.value;
  if (!v) return false;
  if (v.imageUrl) return true;
  if (v.mimeType && v.mimeType.startsWith('image/')) return true;
  const path = (v.path || '').toLowerCase();
  return (
    path.endsWith('.png') ||
    path.endsWith('.jpg') ||
    path.endsWith('.jpeg') ||
    path.endsWith('.webp') ||
    path.endsWith('.svg')
  );
});

const annotations = ref([]);
const summaryInstruction = ref('');
const approvalNote = ref('');

const activeLineStart = ref(null);
const activeLineEnd = ref(null);
const lineCommentInput = ref('');
const showLineComposer = ref(false);

const imageStageRef = ref(null);
const isDrawingRegion = ref(false);
const dragStart = ref(null);
const draftRect = ref(null);
const regionCommentInput = ref('');
const showRegionComposer = ref(false);

const lineDiff = computed(() => {
  return computeLineDiff(
    baseVersion.value?.textContent || '',
    targetVersion.value?.textContent || '',
  );
});

const baseNumberedLines = computed(() => {
  const text = baseVersion.value?.textContent || '';
  return text ? text.split(/\r?\n/) : [];
});

const targetNumberedLines = computed(() => {
  const text = targetVersion.value?.textContent || '';
  return text ? text.split(/\r?\n/) : [];
});

function handleLineClick(lineNum, event) {
  if (event?.shiftKey && activeLineStart.value != null) {
    activeLineEnd.value = Math.max(activeLineStart.value, lineNum);
    activeLineStart.value = Math.min(activeLineStart.value, lineNum);
  } else {
    activeLineStart.value = lineNum;
    activeLineEnd.value = lineNum;
  }
  showLineComposer.value = true;
}

function isLineSelected(lineNum) {
  if (activeLineStart.value == null) return false;
  const start = Math.min(activeLineStart.value, activeLineEnd.value ?? activeLineStart.value);
  const end = Math.max(activeLineStart.value, activeLineEnd.value ?? activeLineStart.value);
  return lineNum >= start && lineNum <= end;
}

function getAnnotationsForLine(lineNum) {
  return annotations.value.filter(
    ann =>
      ann.targetType === 'line' &&
      ann.lineStart != null &&
      lineNum >= ann.lineStart &&
      lineNum <= (ann.lineEnd || ann.lineStart),
  );
}

function addLineAnnotation() {
  if (!lineCommentInput.value.trim() || activeLineStart.value == null) return;
  const start = Math.min(activeLineStart.value, activeLineEnd.value ?? activeLineStart.value);
  const end = Math.max(activeLineStart.value, activeLineEnd.value ?? activeLineStart.value);
  annotations.value.push({
    id: `ann-${Date.now()}-${annotations.value.length + 1}`,
    targetType: 'line',
    lineStart: start,
    lineEnd: end,
    rect: null,
    comment: lineCommentInput.value.trim(),
    createdAt: new Date().toISOString(),
  });
  lineCommentInput.value = '';
  showLineComposer.value = false;
  activeLineStart.value = null;
  activeLineEnd.value = null;
}

function cancelLineAnnotation() {
  lineCommentInput.value = '';
  showLineComposer.value = false;
  activeLineStart.value = null;
  activeLineEnd.value = null;
}

function handleImagePointerDown(event) {
  const stage = imageStageRef.value;
  if (!stage) return;
  const rect = stage.getBoundingClientRect();
  if (rect.width === 0 || rect.height === 0) return;
  const x = Math.max(0, Math.min(100, ((event.clientX - rect.left) / rect.width) * 100));
  const y = Math.max(0, Math.min(100, ((event.clientY - rect.top) / rect.height) * 100));
  dragStart.value = { x, y };
  isDrawingRegion.value = true;
  draftRect.value = { xPct: Math.round(x), yPct: Math.round(y), wPct: 0, hPct: 0 };
}

function handleImagePointerMove(event) {
  if (!isDrawingRegion.value || !dragStart.value) return;
  const stage = imageStageRef.value;
  if (!stage) return;
  const rect = stage.getBoundingClientRect();
  const currentX = Math.max(0, Math.min(100, ((event.clientX - rect.left) / rect.width) * 100));
  const currentY = Math.max(0, Math.min(100, ((event.clientY - rect.top) / rect.height) * 100));
  const minX = Math.min(dragStart.value.x, currentX);
  const minY = Math.min(dragStart.value.y, currentY);
  const w = Math.abs(currentX - dragStart.value.x);
  const h = Math.abs(currentY - dragStart.value.y);
  draftRect.value = {
    xPct: Math.round(minX),
    yPct: Math.round(minY),
    wPct: Math.max(2, Math.round(w)),
    hPct: Math.max(2, Math.round(h)),
  };
}

function handleImagePointerUp() {
  if (!isDrawingRegion.value) return;
  isDrawingRegion.value = false;
  if (draftRect.value) {
    if (draftRect.value.wPct < 4 && draftRect.value.hPct < 4) {
      draftRect.value.wPct = 8;
      draftRect.value.hPct = 8;
      draftRect.value.xPct = Math.max(0, draftRect.value.xPct - 4);
      draftRect.value.yPct = Math.max(0, draftRect.value.yPct - 4);
    }
    showRegionComposer.value = true;
  }
}

function addRegionAnnotation() {
  if (!regionCommentInput.value.trim() || !draftRect.value) return;
  annotations.value.push({
    id: `ann-${Date.now()}-${annotations.value.length + 1}`,
    targetType: 'region',
    lineStart: null,
    lineEnd: null,
    rect: { ...draftRect.value },
    comment: regionCommentInput.value.trim(),
    createdAt: new Date().toISOString(),
  });
  regionCommentInput.value = '';
  showRegionComposer.value = false;
  draftRect.value = null;
}

function cancelRegionAnnotation() {
  regionCommentInput.value = '';
  showRegionComposer.value = false;
  draftRect.value = null;
}

function removeAnnotation(id) {
  annotations.value = annotations.value.filter(a => a.id !== id);
}

function toggleQuickFlip() {
  quickFlipTarget.value = quickFlipTarget.value === 'base' ? 'target' : 'base';
}

const canDispatchRevision = computed(() => {
  return !props.busy && (Boolean(summaryInstruction.value.trim()) || annotations.value.length > 0);
});

function onDispatchRevision() {
  if (!canDispatchRevision.value) return;
  const payload = buildRevisionDispatchPayload({
    artifactPath: currentDisplayVersion.value?.path || baseVersion.value?.path || 'artifact',
    parentVersionId: baseVersion.value?.id || 'V1',
    annotations: annotations.value,
    summaryInstruction: summaryInstruction.value,
  });
  emit('dispatch-revision', payload);
}

function onApproveVersion() {
  if (props.busy) return;
  emit('approve-version', {
    versionId: (targetVersion.value || baseVersion.value)?.id || 'V1',
    note: summaryInstruction.value.trim() || approvalNote.value.trim(),
  });
}
</script>

<template>
  <div class="artifact-review-backdrop" role="dialog" :aria-label="t('artifactRevisionTitle')">
    <div class="artifact-review-dialog">
      <!-- Header -->
      <header class="artifact-review-header">
        <div class="header-main-info">
          <div class="header-title-row">
            <svg class="header-svg" viewBox="0 0 24 24" aria-hidden="true">
              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
              <polyline points="14 2 14 8 20 8" />
              <line x1="16" y1="13" x2="8" y2="13" />
              <line x1="16" y1="17" x2="8" y2="17" />
              <polyline points="10 9 9 9 8 9" />
            </svg>
            <h2>{{ t('artifactRevisionTitle') }}</h2>
            <span v-if="currentDisplayVersion?.path" class="artifact-path-badge">
              {{ currentDisplayVersion.path }}
            </span>
          </div>

          <div class="header-controls-row">
            <!-- Mode Switcher -->
            <div class="mode-switcher-group" role="tablist">
              <button
                type="button"
                role="tab"
                class="mode-btn"
                :class="{ active: currentMode === 'single' }"
                :aria-selected="currentMode === 'single'"
                @click="currentMode = 'single'"
              >
                <svg class="control-svg" viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
                  <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" />
                </svg>
                <span>{{ t('artifactModeSingle') }}</span>
              </button>

              <button
                type="button"
                role="tab"
                class="mode-btn"
                :class="{ active: currentMode === 'side-by-side' }"
                :aria-selected="currentMode === 'side-by-side'"
                @click="currentMode = 'side-by-side'"
              >
                <svg class="control-svg" viewBox="0 0 24 24" aria-hidden="true">
                  <rect x="3" y="3" width="18" height="18" rx="2" />
                  <path d="M12 3v18" />
                </svg>
                <span>{{ t('artifactModeSideBySide') }}</span>
              </button>

              <button
                type="button"
                role="tab"
                class="mode-btn"
                :class="{ active: currentMode === 'toggle' }"
                :aria-selected="currentMode === 'toggle'"
                @click="currentMode = 'toggle'"
              >
                <svg class="control-svg" viewBox="0 0 24 24" aria-hidden="true">
                  <path d="m16 3 4 4-4 4" />
                  <path d="M20 7H4" />
                  <path d="m8 21-4-4 4-4" />
                  <path d="M4 17h16" />
                </svg>
                <span>{{ t('artifactModeToggle') }}</span>
              </button>
            </div>

            <div v-if="versions.length > 1" class="version-select-group">
              <span class="version-label-tag">{{ t('artifactBaseVersion') }}: {{ baseVersion?.versionLabel || baseVersion?.id || 'V1' }}</span>
              <span class="version-arrow">→</span>
              <span class="version-label-tag active">{{ t('artifactTargetVersion') }}: {{ targetVersion?.versionLabel || targetVersion?.id || 'V2' }}</span>
            </div>
          </div>
        </div>

        <button
          type="button"
          class="dialog-close-btn"
          :aria-label="t('artifactCloseBtn')"
          @click="emit('close')"
        >
          <svg class="close-svg" viewBox="0 0 24 24" aria-hidden="true">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </header>

      <!-- Main Stage -->
      <main class="artifact-review-body">
        <div v-if="!baseVersion && !targetVersion" class="empty-state">
          <p>{{ t('artifactReviewEmpty') }}</p>
        </div>

        <!-- Mode 1: Single / Annotation Mode -->
        <section v-else-if="currentMode === 'single'" class="stage-single">
          <!-- Text Annotation View -->
          <div v-if="!isImage" class="text-annotation-panel">
            <div class="line-list-container">
              <div
                v-for="(lineText, idx) in baseNumberedLines"
                :key="idx"
                class="numbered-line-row"
                :class="{
                  'is-selected': isLineSelected(idx + 1),
                  'has-annotation': getAnnotationsForLine(idx + 1).length > 0,
                }"
                @click="handleLineClick(idx + 1, $event)"
              >
                <span class="line-number-cell">{{ idx + 1 }}</span>
                <span class="line-code-cell">{{ lineText || ' ' }}</span>
                <div v-if="getAnnotationsForLine(idx + 1).length" class="line-annotation-chips">
                  <span
                    v-for="ann in getAnnotationsForLine(idx + 1)"
                    :key="ann.id"
                    class="annotation-chip"
                    :title="ann.comment"
                  >
                    {{ ann.comment }}
                    <button
                      type="button"
                      class="chip-delete-btn"
                      :aria-label="t('artifactDeleteAnnotation')"
                      @click.stop="removeAnnotation(ann.id)"
                    >
                      ×
                    </button>
                  </span>
                </div>
              </div>
            </div>

            <!-- Inline Line Annotation Composer Popup -->
            <div v-if="showLineComposer" class="inline-composer-popover">
              <div class="composer-header">
                <strong>
                  {{
                    activeLineStart === activeLineEnd
                      ? t('artifactLine', { line: activeLineStart })
                      : t('artifactLines', { start: activeLineStart, end: activeLineEnd })
                  }}
                </strong>
              </div>
              <input
                v-model="lineCommentInput"
                type="text"
                class="composer-input"
                :placeholder="t('artifactAnnotationPlaceholder')"
                autofocus
                @keydown.enter.prevent="addLineAnnotation"
              />
              <div class="composer-actions">
                <button
                  type="button"
                  class="primary-action-btn"
                  :disabled="!lineCommentInput.trim()"
                  @click="addLineAnnotation"
                >
                  {{ t('artifactAddAnnotationBtn') }}
                </button>
                <button
                  type="button"
                  class="secondary-action-btn"
                  @click="cancelLineAnnotation"
                >
                  {{ t('artifactCancelBtn') }}
                </button>
              </div>
            </div>
          </div>

          <!-- Image Annotation Stage -->
          <div v-else class="image-annotation-wrapper">
            <p class="stage-instruction-hint">{{ t('artifactDragToAnnotate') }}</p>
            <div
              ref="imageStageRef"
              class="image-stage-container"
              @pointerdown="handleImagePointerDown"
              @pointermove="handleImagePointerMove"
              @pointerup="handleImagePointerUp"
            >
              <img
                :src="currentDisplayVersion?.imageUrl || currentDisplayVersion?.textContent"
                :alt="currentDisplayVersion?.path || 'Artifact Image'"
                class="stage-main-image"
                draggable="false"
              />

              <!-- Pinned Region Annotations -->
              <div
                v-for="(ann, idx) in annotations.filter(a => a.targetType === 'region' && a.rect)"
                :key="ann.id"
                class="pinned-region-box"
                :style="{
                  left: ann.rect.xPct + '%',
                  top: ann.rect.yPct + '%',
                  width: ann.rect.wPct + '%',
                  height: ann.rect.hPct + '%',
                }"
              >
                <span class="region-badge-index">{{ idx + 1 }}</span>
                <span class="region-tooltip-text">{{ ann.comment }}</span>
                <button
                  type="button"
                  class="chip-delete-btn"
                  :aria-label="t('artifactDeleteAnnotation')"
                  @click.stop="removeAnnotation(ann.id)"
                >
                  ×
                </button>
              </div>

              <!-- In-progress Drag Box -->
              <div
                v-if="draftRect"
                class="draft-region-box"
                :style="{
                  left: draftRect.xPct + '%',
                  top: draftRect.yPct + '%',
                  width: draftRect.wPct + '%',
                  height: draftRect.hPct + '%',
                }"
              ></div>
            </div>

            <!-- Inline Region Composer Popup -->
            <div v-if="showRegionComposer" class="inline-composer-popover region-composer">
              <div class="composer-header">
                <strong>{{ t('artifactRegion') }}</strong>
                <small v-if="draftRect">
                  ({{ draftRect.xPct }}%, {{ draftRect.yPct }}%, {{ draftRect.wPct }}% × {{ draftRect.hPct }}%)
                </small>
              </div>
              <input
                v-model="regionCommentInput"
                type="text"
                class="composer-input"
                :placeholder="t('artifactAnnotationPlaceholder')"
                autofocus
                @keydown.enter.prevent="addRegionAnnotation"
              />
              <div class="composer-actions">
                <button
                  type="button"
                  class="primary-action-btn"
                  :disabled="!regionCommentInput.trim()"
                  @click="addRegionAnnotation"
                >
                  {{ t('artifactAddAnnotationBtn') }}
                </button>
                <button
                  type="button"
                  class="secondary-action-btn"
                  @click="cancelRegionAnnotation"
                >
                  {{ t('artifactCancelBtn') }}
                </button>
              </div>
            </div>
          </div>
        </section>

        <!-- Mode 2: Side-by-Side Compare -->
        <section v-else-if="currentMode === 'side-by-side'" class="stage-side-by-side">
          <!-- Text Split Pane -->
          <div v-if="!isImage" class="side-by-side-grid">
            <div class="split-pane base-pane">
              <div class="split-pane-header">
                <span class="pane-badge">{{ baseVersion?.versionLabel || 'V1' }}</span>
                <span class="pane-title">{{ baseVersion?.path || t('artifactBaseVersion') }}</span>
              </div>
              <div class="split-pane-content">
                <div
                  v-for="(item, idx) in lineDiff"
                  :key="'left-' + idx"
                  class="split-diff-row"
                  :class="{
                    'diff-removed': item.type === 'removed',
                    'diff-spacer': item.type === 'added',
                  }"
                >
                  <span class="split-line-num">{{ item.oldLine ?? '' }}</span>
                  <span class="split-line-code">{{ item.type === 'added' ? '' : item.text }}</span>
                </div>
              </div>
            </div>

            <div class="split-pane target-pane">
              <div class="split-pane-header">
                <span class="pane-badge active">{{ targetVersion?.versionLabel || 'V2' }}</span>
                <span class="pane-title">{{ targetVersion?.path || t('artifactTargetVersion') }}</span>
              </div>
              <div class="split-pane-content">
                <div
                  v-for="(item, idx) in lineDiff"
                  :key="'right-' + idx"
                  class="split-diff-row"
                  :class="{
                    'diff-added': item.type === 'added',
                    'diff-spacer': item.type === 'removed',
                  }"
                >
                  <span class="split-line-num">{{ item.newLine ?? '' }}</span>
                  <span class="split-line-code">{{ item.type === 'removed' ? '' : item.text }}</span>
                </div>
              </div>
            </div>
          </div>

          <!-- Image Side-by-Side Compare -->
          <div v-else class="side-by-side-image-grid">
            <div class="image-pane">
              <div class="split-pane-header">
                <span class="pane-badge">{{ baseVersion?.versionLabel || 'V1' }}</span>
                <span>{{ baseVersion?.path || t('artifactBaseVersion') }}</span>
              </div>
              <div class="image-pane-body">
                <img
                  :src="baseVersion?.imageUrl || baseVersion?.textContent"
                  :alt="baseVersion?.path || 'Base Image'"
                  class="compare-img"
                />
              </div>
            </div>

            <div class="image-pane">
              <div class="split-pane-header">
                <span class="pane-badge active">{{ targetVersion?.versionLabel || 'V2' }}</span>
                <span>{{ targetVersion?.path || t('artifactTargetVersion') }}</span>
              </div>
              <div class="image-pane-body">
                <img
                  :src="targetVersion?.imageUrl || targetVersion?.textContent"
                  :alt="targetVersion?.path || 'Target Image'"
                  class="compare-img"
                />
              </div>
            </div>
          </div>
        </section>

        <!-- Mode 3: Toggle / Quick Flip / Unified Diff -->
        <section v-else class="stage-toggle">
          <div class="toggle-toolbar">
            <button
              type="button"
              class="quick-flip-btn"
              @click="toggleQuickFlip"
            >
              <svg class="control-svg" viewBox="0 0 24 24" aria-hidden="true">
                <path d="m16 3 4 4-4 4" />
                <path d="M20 7H4" />
                <path d="m8 21-4-4 4-4" />
                <path d="M4 17h16" />
              </svg>
              <span>{{ t('artifactFlipAB') }}</span>
            </button>
            <span class="flip-current-badge">
              {{ quickFlipTarget === 'base' ? (baseVersion?.versionLabel || 'V1') : (targetVersion?.versionLabel || 'V2') }}
            </span>
          </div>

          <!-- Image Flip Stage -->
          <div v-if="isImage" class="toggle-image-stage">
            <img
              :src="
                quickFlipTarget === 'base'
                  ? (baseVersion?.imageUrl || baseVersion?.textContent)
                  : (targetVersion?.imageUrl || targetVersion?.textContent)
              "
              :alt="quickFlipTarget"
              class="toggle-active-image"
            />
          </div>

          <!-- Unified Diff List -->
          <div v-else class="unified-diff-container">
            <div
              v-for="(item, idx) in lineDiff"
              :key="'diff-' + idx"
              class="unified-diff-row"
              :class="'diff-' + item.type"
            >
              <span class="unified-prefix">
                {{ item.type === 'added' ? '+' : item.type === 'removed' ? '-' : ' ' }}
              </span>
              <span class="unified-old-num">{{ item.oldLine ?? ' ' }}</span>
              <span class="unified-new-num">{{ item.newLine ?? ' ' }}</span>
              <span class="unified-line-code">{{ item.text || ' ' }}</span>
            </div>
          </div>
        </section>
      </main>

      <!-- Footer Actions -->
      <footer class="artifact-review-footer">
        <div class="footer-composer-block">
          <div class="footer-prompt-row">
            <textarea
              v-model="summaryInstruction"
              class="summary-instruction-input"
              rows="2"
              :placeholder="t('artifactRevisionPromptPlaceholder')"
            ></textarea>
          </div>

          <!-- Annotation Summary Chips -->
          <div v-if="annotations.length" class="footer-annotations-summary">
            <span class="annotations-count-pill">
              {{ annotations.length }} {{ t('artifactAnnotationsCount') }}
            </span>
            <div class="summary-chips-list">
              <span
                v-for="ann in annotations"
                :key="ann.id"
                class="summary-chip-item"
              >
                <small v-if="ann.targetType === 'region'">[Region]</small>
                <small v-else>[L{{ ann.lineStart }}{{ ann.lineEnd && ann.lineEnd !== ann.lineStart ? '-' + ann.lineEnd : '' }}]</small>
                <span class="summary-chip-text">{{ ann.comment }}</span>
                <button
                  type="button"
                  class="chip-delete-btn"
                  :aria-label="t('artifactDeleteAnnotation')"
                  @click="removeAnnotation(ann.id)"
                >
                  ×
                </button>
              </span>
            </div>
          </div>
          <div v-else class="footer-no-annotations-hint">
            <small>{{ t('artifactNoAnnotations') }}</small>
          </div>
        </div>

        <div class="footer-action-buttons">
          <button
            type="button"
            class="secondary-action-btn"
            @click="emit('close')"
          >
            {{ t('artifactCloseBtn') }}
          </button>

          <button
            type="button"
            class="request-revision-btn"
            :disabled="!canDispatchRevision"
            @click="onDispatchRevision"
          >
            <svg class="action-btn-svg" viewBox="0 0 24 24" aria-hidden="true">
              <path d="m22 2-7 20-4-9-9-4Z" />
              <path d="M22 2 11 13" />
            </svg>
            <span>{{ t('artifactDispatchRevisionBtn') }}</span>
          </button>

          <button
            type="button"
            class="approve-version-btn"
            :disabled="busy"
            @click="onApproveVersion"
          >
            <svg class="action-btn-svg" viewBox="0 0 24 24" aria-hidden="true">
              <polyline points="20 6 9 17 4 12" />
            </svg>
            <span>{{ t('artifactApproveVersionBtn') }}</span>
          </button>
        </div>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.artifact-review-backdrop {
  position: fixed;
  inset: 0;
  z-index: 1000;
  background: color-mix(in srgb, var(--bg-root) 82%, transparent);
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 1rem;
  scrollbar-gutter: stable;
}

.artifact-review-dialog {
  display: flex;
  flex-direction: column;
  width: 100%;
  max-width: var(--content-max-width, 68rem);
  height: 90vh;
  max-height: 56rem;
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-modal);
  box-shadow: var(--shadow-lg);
  overflow: hidden;
}

/* Header */
.artifact-review-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  padding: 0.875rem 1.25rem;
  border-bottom: 1px solid var(--border-color);
  background: var(--bg-secondary);
  gap: 1rem;
}

.header-main-info {
  display: flex;
  flex-direction: column;
  gap: 0.625rem;
  flex: 1;
  min-width: 0;
}

.header-title-row {
  display: flex;
  align-items: center;
  gap: 0.625rem;
  flex-wrap: wrap;
}

.header-title-row h2 {
  font-size: var(--text-base);
  font-weight: 600;
  margin: 0;
  color: var(--text-primary);
}

.header-svg {
  width: 1.25rem;
  height: 1.25rem;
  stroke: var(--accent-primary);
  stroke-width: 2;
  fill: none;
  stroke-linecap: round;
  stroke-linejoin: round;
  flex-shrink: 0;
}

.artifact-path-badge {
  font-size: var(--text-xs);
  color: var(--text-secondary);
  background: var(--bg-tertiary);
  padding: 0.125rem 0.5rem;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-color);
  font-family: monospace;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 18rem;
}

.header-controls-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  flex-wrap: wrap;
}

.mode-switcher-group {
  display: inline-flex;
  background: var(--bg-tertiary);
  border-radius: var(--radius-btn);
  padding: 0.1875rem;
  border: 1px solid var(--border-color);
  gap: 0.125rem;
}

.mode-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0.25rem 0.625rem;
  font-size: var(--text-xs);
  font-weight: 500;
  color: var(--text-secondary);
  background: transparent;
  border: none;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: all 0.15s ease;
}

.mode-btn.active {
  background: var(--bg-primary);
  color: var(--text-primary);
  box-shadow: 0 1px 2px color-mix(in srgb, var(--text-primary) 8%, transparent);
}

.control-svg {
  width: 0.875rem;
  height: 0.875rem;
  stroke: currentColor;
  stroke-width: 2;
  fill: none;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.version-select-group {
  display: inline-flex;
  align-items: center;
  gap: 0.375rem;
  font-size: var(--text-xs);
}

.version-label-tag {
  padding: 0.125rem 0.375rem;
  border-radius: var(--radius-xs);
  background: var(--bg-tertiary);
  color: var(--text-secondary);
}

.version-label-tag.active {
  background: var(--accent-soft);
  color: var(--accent-primary);
  font-weight: 600;
}

.version-arrow {
  color: var(--text-tertiary);
}

.dialog-close-btn {
  background: transparent;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 0.25rem;
  border-radius: var(--radius-sm);
  display: flex;
  align-items: center;
  justify-content: center;
}

.dialog-close-btn:hover {
  background: var(--bg-tertiary);
  color: var(--text-primary);
}

.close-svg {
  width: 1.25rem;
  height: 1.25rem;
  stroke: currentColor;
  stroke-width: 2;
  fill: none;
  stroke-linecap: round;
  stroke-linejoin: round;
}

/* Body / Main Stage */
.artifact-review-body {
  flex: 1;
  overflow: auto;
  position: relative;
  background: var(--bg-root);
  padding: 0.5rem;
}

.empty-state {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: var(--text-secondary);
  font-size: var(--text-sm);
}

/* Single Mode Text */
.text-annotation-panel {
  position: relative;
  background: var(--bg-primary);
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-color);
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: var(--text-xs);
  line-height: 1.6;
}

.line-list-container {
  display: flex;
  flex-direction: column;
}

.numbered-line-row {
  display: flex;
  align-items: flex-start;
  padding: 0.0625rem 0.5rem;
  cursor: pointer;
  border-bottom: 1px solid color-mix(in srgb, var(--border-color) 40%, transparent);
  transition: background-color 0.1s ease;
}

.numbered-line-row:hover {
  background: var(--bg-tertiary);
}

.numbered-line-row.is-selected {
  background: color-mix(in srgb, var(--accent-primary) 15%, transparent);
}

.numbered-line-row.has-annotation {
  border-left: 3px solid var(--accent-primary);
}

.line-number-cell {
  width: 3rem;
  flex-shrink: 0;
  text-align: right;
  padding-right: 0.75rem;
  color: var(--text-tertiary);
  user-select: none;
}

.line-code-cell {
  flex: 1;
  white-space: pre-wrap;
  word-break: break-all;
  color: var(--text-primary);
}

.line-annotation-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 0.25rem;
  margin-left: 0.5rem;
}

.annotation-chip {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  font-size: var(--text-2xs);
  background: var(--accent-soft);
  color: var(--accent-primary);
  border: 1px solid var(--accent-primary);
  border-radius: var(--radius-xs);
  padding: 0.0625rem 0.375rem;
  max-width: 14rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.chip-delete-btn {
  background: transparent;
  border: none;
  color: inherit;
  cursor: pointer;
  padding: 0 0.125rem;
  font-weight: bold;
}

/* Popover for inline annotation */
.inline-composer-popover {
  position: sticky;
  bottom: 1rem;
  margin: 0.75rem auto;
  width: min(100%, 28rem);
  background: var(--bg-secondary);
  border: 1px solid var(--accent-primary);
  border-radius: var(--radius-card);
  box-shadow: var(--shadow-lg);
  padding: 0.75rem;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  z-index: 10;
}

.composer-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: var(--text-xs);
  color: var(--text-primary);
}

.composer-input {
  width: 100%;
  padding: 0.375rem 0.5rem;
  font-size: var(--text-xs);
  color: var(--text-primary);
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  outline: none;
  box-sizing: border-box;
}

.composer-input:focus {
  border-color: var(--accent-primary);
}

.composer-actions {
  display: flex;
  justify-content: flex-end;
  gap: 0.5rem;
}

/* Image Annotation Stage */
.image-annotation-wrapper {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.5rem;
  min-height: 100%;
}

.stage-instruction-hint {
  font-size: var(--text-xs);
  color: var(--text-secondary);
  margin: 0;
}

.image-stage-container {
  position: relative;
  display: inline-block;
  max-width: 100%;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  background: var(--bg-tertiary);
  cursor: crosshair;
  user-select: none;
  touch-action: none;
}

.stage-main-image {
  display: block;
  max-width: 100%;
  max-height: 60vh;
  object-fit: contain;
  border-radius: var(--radius-sm);
  pointer-events: none;
}

.pinned-region-box {
  position: absolute;
  border: 2px dashed var(--accent-primary);
  background: color-mix(in srgb, var(--accent-primary) 18%, transparent);
  border-radius: var(--radius-xs);
  pointer-events: auto;
  cursor: pointer;
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  padding: 0.125rem;
}

.region-badge-index {
  background: var(--accent-primary);
  color: var(--text-on-accent);
  font-size: var(--text-2xs);
  font-weight: 700;
  border-radius: var(--radius-xs);
  padding: 0 0.25rem;
}

.region-tooltip-text {
  display: none;
  position: absolute;
  top: 100%;
  left: 0;
  background: var(--bg-primary);
  color: var(--text-primary);
  border: 1px solid var(--border-color);
  padding: 0.25rem 0.5rem;
  border-radius: var(--radius-xs);
  font-size: var(--text-2xs);
  white-space: nowrap;
  box-shadow: var(--shadow);
  z-index: 5;
}

.pinned-region-box:hover .region-tooltip-text {
  display: block;
}

.draft-region-box {
  position: absolute;
  border: 2px solid var(--accent-primary);
  background: color-mix(in srgb, var(--accent-primary) 25%, transparent);
  border-radius: var(--radius-xs);
  pointer-events: none;
}

/* Side-by-Side Diff */
.side-by-side-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0.5rem;
  height: 100%;
}

.split-pane {
  display: flex;
  flex-direction: column;
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  overflow: hidden;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: var(--text-xs);
}

.split-pane-header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.375rem 0.75rem;
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--border-color);
  font-size: var(--text-xs);
  font-weight: 600;
}

.pane-badge {
  padding: 0.0625rem 0.375rem;
  border-radius: var(--radius-xs);
  background: var(--bg-tertiary);
  color: var(--text-secondary);
  font-size: var(--text-2xs);
}

.pane-badge.active {
  background: var(--accent-soft);
  color: var(--accent-primary);
}

.split-pane-content {
  flex: 1;
  overflow: auto;
  line-height: 1.5;
}

.split-diff-row {
  display: flex;
  align-items: flex-start;
  padding: 0.0625rem 0.375rem;
  min-height: 1.25rem;
}

.split-line-num {
  width: 2.5rem;
  flex-shrink: 0;
  text-align: right;
  padding-right: 0.5rem;
  color: var(--text-tertiary);
  user-select: none;
}

.split-line-code {
  flex: 1;
  white-space: pre-wrap;
  word-break: break-all;
}

.diff-removed {
  background: color-mix(in srgb, var(--color-error) 12%, transparent);
  color: var(--color-error);
}

.diff-added {
  background: color-mix(in srgb, var(--color-success) 12%, transparent);
  color: var(--color-success);
}

.diff-spacer {
  background: color-mix(in srgb, var(--bg-tertiary) 50%, transparent);
}

/* Image Compare Grid */
.side-by-side-image-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0.5rem;
  height: 100%;
}

.image-pane {
  display: flex;
  flex-direction: column;
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  overflow: hidden;
}

.image-pane-body {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0.5rem;
  background: var(--bg-tertiary);
}

.compare-img {
  max-width: 100%;
  max-height: 55vh;
  object-fit: contain;
  border-radius: var(--radius-xs);
}

/* Toggle / Flip */
.stage-toggle {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  height: 100%;
}

.toggle-toolbar {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.25rem 0.5rem;
  background: var(--bg-secondary);
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-color);
}

.quick-flip-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0.25rem 0.625rem;
  font-size: var(--text-xs);
  font-weight: 500;
  color: var(--text-primary);
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-btn);
  cursor: pointer;
}

.flip-current-badge {
  font-size: var(--text-xs);
  font-weight: 600;
  color: var(--accent-primary);
  background: var(--accent-soft);
  padding: 0.125rem 0.5rem;
  border-radius: var(--radius-pill);
}

.toggle-image-stage {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  padding: 0.5rem;
}

.toggle-active-image {
  max-width: 100%;
  max-height: 60vh;
  object-fit: contain;
  border-radius: var(--radius-sm);
}

.unified-diff-container {
  flex: 1;
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  overflow: auto;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: var(--text-xs);
  line-height: 1.5;
}

.unified-diff-row {
  display: flex;
  align-items: flex-start;
  padding: 0.0625rem 0.5rem;
}

.unified-prefix {
  width: 1.25rem;
  flex-shrink: 0;
  font-weight: 700;
}

.unified-old-num,
.unified-new-num {
  width: 2.25rem;
  flex-shrink: 0;
  text-align: right;
  padding-right: 0.375rem;
  color: var(--text-tertiary);
  user-select: none;
}

.unified-line-code {
  flex: 1;
  white-space: pre-wrap;
  word-break: break-all;
}

/* Footer */
.artifact-review-footer {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  padding: 0.75rem 1.25rem;
  border-top: 1px solid var(--border-color);
  background: var(--bg-secondary);
}

.footer-composer-block {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.summary-instruction-input {
  width: 100%;
  box-sizing: border-box;
  padding: 0.5rem;
  font-size: var(--text-xs);
  color: var(--text-primary);
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  resize: vertical;
  outline: none;
  font-family: inherit;
}

.summary-instruction-input:focus {
  border-color: var(--accent-primary);
}

.footer-annotations-summary {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
}

.annotations-count-pill {
  font-size: var(--text-2xs);
  font-weight: 600;
  background: var(--accent-soft);
  color: var(--accent-primary);
  padding: 0.125rem 0.375rem;
  border-radius: var(--radius-xs);
}

.summary-chips-list {
  display: flex;
  flex-wrap: wrap;
  gap: 0.25rem;
}

.summary-chip-item {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  font-size: var(--text-2xs);
  background: var(--bg-primary);
  color: var(--text-primary);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-xs);
  padding: 0.0625rem 0.375rem;
  max-width: 16rem;
}

.summary-chip-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.footer-no-annotations-hint {
  font-size: var(--text-2xs);
  color: var(--text-tertiary);
}

.footer-action-buttons {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 0.625rem;
  flex-wrap: wrap;
}

.primary-action-btn,
.secondary-action-btn,
.request-revision-btn,
.approve-version-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0.375rem 0.875rem;
  font-size: var(--text-xs);
  font-weight: 500;
  border-radius: var(--radius-btn);
  cursor: pointer;
  border: 1px solid transparent;
  transition: all 0.15s ease;
}

.secondary-action-btn {
  background: var(--bg-primary);
  color: var(--text-secondary);
  border-color: var(--border-color);
}

.secondary-action-btn:hover {
  color: var(--text-primary);
  background: var(--bg-tertiary);
}

.primary-action-btn {
  background: var(--accent-primary);
  color: var(--text-on-accent);
}

.primary-action-btn:disabled,
.request-revision-btn:disabled,
.approve-version-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.request-revision-btn {
  background: var(--accent-primary);
  color: var(--text-on-accent);
}

.request-revision-btn:hover:not(:disabled) {
  filter: brightness(1.08);
}

.approve-version-btn {
  background: color-mix(in srgb, var(--color-success) 14%, var(--bg-primary));
  color: var(--color-success);
  border-color: var(--color-success);
}

.approve-version-btn:hover:not(:disabled) {
  background: color-mix(in srgb, var(--color-success) 24%, var(--bg-primary));
}

.action-btn-svg {
  width: 0.875rem;
  height: 0.875rem;
  stroke: currentColor;
  stroke-width: 2;
  fill: none;
  stroke-linecap: round;
  stroke-linejoin: round;
}

@media (max-width: 640px) {
  .side-by-side-grid,
  .side-by-side-image-grid {
    grid-template-columns: 1fr;
  }
}
</style>
