import { nextTick, reactive, ref, watch } from 'vue';

export function useQuickNote(ctx) {
  const {
    t,
    available,
    kernelBridge,
    clientState,
    saveClientState,
    loadPersonalOps,
    reload,
  } = ctx;

// Floating QuickNote Bubble (FAB) & QuickNote Overlay (ported from legacy bob-agent)
const quickNoteOpen = ref(false);
const quickNoteText = ref(clientState.quickNote?.text || '');
const quickNoteProject = ref(clientState.quickNote?.project || '');
const quickNoteProjectTitle = ref(clientState.quickNote?.title || '');
const quickNoteBusy = ref(false);
const quickNoteError = ref(false);
let quickNotePending = clientState.quickNote?.pending || null;

const quickNoteMediaId = ref(clientState.quickNote?.mediaId || '');
const quickNoteImage = ref('');
const quickNoteImageBusy = ref(false);
const quickNoteImageError = ref('');
const quickNoteSuggestion = ref(clientState.quickNote?.suggestion || '');
const quickNoteVisionTarget = ref(null);
const quickNoteRecognizing = ref(false);
const quickNoteFileRef = ref(null);
const quickNoteCameraRef = ref(null);
let quickNoteImageGeneration = 0;
const quickNoteSaved = ref(false);
const quickNoteInputRef = ref(null);
function persistQuickNote() {
  clientState.quickNote = { text: quickNoteText.value, project: quickNoteProject.value,
    title: quickNoteProjectTitle.value, pending: quickNotePending, mediaId: quickNoteMediaId.value, suggestion: quickNoteSuggestion.value };
  saveClientState(clientState);
}
watch([quickNoteText, quickNoteProject, quickNoteProjectTitle, quickNoteMediaId, quickNoteSuggestion], () => {
  quickNoteSaved.value = false;
  persistQuickNote();
}, { flush: 'sync' });
const fabPos = reactive({
  x: -1,
  y: 120,
  isRight: true,
});
const isDraggingFab = ref(false);
let fabDragMoved = false;
let fabStartX = 0;
let fabStartY = 0;
let fabInitialX = 0;
let fabInitialY = 0;

function onFabPointerDown(event) {
  isDraggingFab.value = true;
  fabDragMoved = false;
  fabStartX = event.clientX;
  fabStartY = event.clientY;
  const rect = event.currentTarget.getBoundingClientRect();
  fabInitialX = rect.left;
  fabInitialY = rect.top;
  try {
    event.currentTarget.setPointerCapture(event.pointerId);
  } catch {
    // ignore in synthetic pointer events
  }
}

function onFabPointerMove(event) {
  if (!isDraggingFab.value) return;
  const dx = event.clientX - fabStartX;
  const dy = event.clientY - fabStartY;
  if (!fabDragMoved && (Math.abs(dx) > 15 || Math.abs(dy) > 15)) {
    fabDragMoved = true;
  }
  if (fabDragMoved) {
    fabPos.x = fabInitialX + dx;
    fabPos.y = Math.max(48, Math.min(window.innerHeight - 96, fabInitialY + dy));
  }
}

function onFabPointerUp(event) {
  if (!isDraggingFab.value) return;
  isDraggingFab.value = false;
  try {
    event.currentTarget.releasePointerCapture(event.pointerId);
  } catch {
    // ignore
  }
  if (fabDragMoved) {
    const midX = window.innerWidth / 2;
    fabPos.isRight = fabPos.x + 22 >= midX;
    fabPos.x = -1;
  } else {
    void openQuickNoteOverlay();
  }
}

async function openQuickNoteOverlay() {
  quickNoteOpen.value = true;
  quickNoteSaved.value = false;
  await nextTick();
  quickNoteInputRef.value?.focus();
  if (quickNoteMediaId.value && !quickNoteImage.value) {
    try {
      const image = await kernelBridge.captureMediaGet(quickNoteMediaId.value);
      quickNoteImage.value = `data:${image.mime};base64,${image.base64}`;
    } catch { quickNoteImageError.value = 'captureImageMissing'; }
  }
  void loadCaptureVisionTarget();
}
async function loadCaptureVisionTarget() {
  try { quickNoteVisionTarget.value = await kernelBridge.captureVisionTarget(); }
  catch { quickNoteVisionTarget.value = null; }
}
async function selectCaptureImage(event) {
  const file = event.target.files?.[0];
  event.target.value = '';
  if (!file || quickNoteBusy.value || quickNoteImageBusy.value || quickNoteRecognizing.value) return;
  quickNoteImageError.value = '';
  if (!['image/png', 'image/jpeg', 'image/webp'].includes(file.type) || file.size > 8 * 1024 * 1024) {
    quickNoteImageError.value = 'captureImageInvalid'; return;
  }
  const generation = ++quickNoteImageGeneration;
  quickNoteImageBusy.value = true;
  try {
    const data = await new Promise((resolve, reject) => {
      const reader = new FileReader(); reader.onload = () => resolve(reader.result);
      reader.onerror = reject; reader.readAsDataURL(file);
    });
    await new Promise((resolve, reject) => {
      const image = new Image(); image.onload = () => image.naturalWidth * image.naturalHeight <= 40000000 ? resolve() : reject();
      image.onerror = reject; image.src = data;
    });
    const stored = await kernelBridge.captureMediaPut(file.type, data.split(',')[1]);
    if (generation !== quickNoteImageGeneration) return;
    quickNoteMediaId.value = stored.id;
    quickNoteImage.value = data;
    quickNoteSuggestion.value = '';
    quickNoteSaved.value = false;
  } catch { quickNoteImageError.value = 'captureImageSaveFailed'; }
  finally { quickNoteImageBusy.value = false; }
}
function removeCaptureImage() {
  if (quickNoteBusy.value || quickNoteImageBusy.value || quickNoteRecognizing.value) return;
  ++quickNoteImageGeneration;
  quickNoteMediaId.value = ''; quickNoteImage.value = ''; quickNoteSuggestion.value = '';
  quickNoteImageError.value = '';
}
async function recognizeCaptureImage() {
  if (!quickNoteMediaId.value || !quickNoteVisionTarget.value || quickNoteRecognizing.value || quickNoteBusy.value) return;
  const id = quickNoteMediaId.value;
  quickNoteRecognizing.value = true; quickNoteImageError.value = '';
  try {
    const result = await kernelBridge.captureRecognize(id, quickNoteVisionTarget.value.model_ref);
    if (id === quickNoteMediaId.value) quickNoteSuggestion.value = result.text;
  } catch { quickNoteImageError.value = 'captureRecognitionFailed'; }
  finally { quickNoteRecognizing.value = false; }
}
async function recognizeCaptureAsTicket() {
  if (!quickNoteMediaId.value || !quickNoteVisionTarget.value || quickNoteRecognizing.value || quickNoteBusy.value) return;
  const id = quickNoteMediaId.value;
  quickNoteRecognizing.value = true; quickNoteImageError.value = '';
  try {
    const result = await kernelBridge.ticketRecognize(id, quickNoteVisionTarget.value.model_ref);
    if (id === quickNoteMediaId.value) {
      quickNoteSuggestion.value = result?.text || t('ticketRecognizedSuccess');
      quickNoteSaved.value = true;
    }
    await loadPersonalOps();
  } catch { quickNoteImageError.value = 'captureRecognitionFailed'; }
  finally { quickNoteRecognizing.value = false; }
}
function applyCaptureSuggestion() {
  quickNoteText.value = [quickNoteText.value.trim(), quickNoteSuggestion.value.trim()].filter(Boolean).join('\n\n');
  quickNoteSuggestion.value = '';
}

async function submitQuickNote() {
  const content = quickNoteText.value.trim() || (quickNoteMediaId.value ? t('capturePhotoOnly') : '');
  if (!content || quickNoteBusy.value || quickNoteImageBusy.value || quickNoteRecognizing.value) return;
  quickNoteBusy.value = true;
  quickNoteError.value = false;
  const signature = JSON.stringify([content, quickNoteProject.value, quickNoteProjectTitle.value, quickNoteMediaId.value]);
  if (!quickNotePending || quickNotePending.signature !== signature) {
    quickNotePending = { signature, id: `cap-${crypto.randomUUID()}`, projectId:
      quickNoteProject.value === '__new' ? `proj-${crypto.randomUUID()}` : quickNoteProject.value };
  }
  try {
    if (!available) throw new Error('NATIVE_HOST_REQUIRED');
    if (quickNoteProject.value === '__new' && !quickNoteProjectTitle.value.trim()) {
      throw new Error('PROJECT_TITLE_REQUIRED');
    }
    const pending = quickNotePending;
    persistQuickNote();
    await kernelBridge.captureCreate({ id: pending.id, content, mediaId: quickNoteMediaId.value || null,
      projectId: pending.projectId || null,
      newProjectTitle: quickNoteProject.value === '__new' ? quickNoteProjectTitle.value.trim() : null });
    if (signature === JSON.stringify([quickNoteText.value.trim() || (quickNoteMediaId.value ? t('capturePhotoOnly') : ''), quickNoteProject.value, quickNoteProjectTitle.value, quickNoteMediaId.value])) {
      quickNoteMediaId.value = ''; quickNoteImage.value = ''; quickNoteSuggestion.value = '';
      quickNoteText.value = '';
      quickNoteProject.value = '';
      quickNoteProjectTitle.value = '';
      quickNotePending = null;
    }
    persistQuickNote();
    quickNoteSaved.value = true;
    void loadPersonalOps();
    void reload();
  } catch {
    quickNoteError.value = true;
  } finally {
    quickNoteBusy.value = false;
  }
}

  return Object.assign(ctx, {
    quickNoteOpen,
    quickNoteText,
    quickNoteProject,
    quickNoteProjectTitle,
    quickNoteBusy,
    quickNoteError,
    quickNoteMediaId,
    quickNoteImage,
    quickNoteImageBusy,
    quickNoteImageError,
    quickNoteSuggestion,
    quickNoteVisionTarget,
    quickNoteRecognizing,
    quickNoteFileRef,
    quickNoteCameraRef,
    quickNoteSaved,
    quickNoteInputRef,
    persistQuickNote,
    fabPos,
    isDraggingFab,
    onFabPointerDown,
    onFabPointerMove,
    onFabPointerUp,
    openQuickNoteOverlay,
    loadCaptureVisionTarget,
    selectCaptureImage,
    removeCaptureImage,
    recognizeCaptureImage,
    recognizeCaptureAsTicket,
    applyCaptureSuggestion,
    submitQuickNote,
  });
}
