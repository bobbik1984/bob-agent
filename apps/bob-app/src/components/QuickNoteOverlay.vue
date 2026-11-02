<script setup>
import { inject } from 'vue';

const {
  t,
  catalog,
  isDraggingFab,
  fabPos,
  onFabPointerDown,
  onFabPointerMove,
  onFabPointerUp,
  quickNoteOpen,
  submitQuickNote,
  quickNoteInputRef,
  quickNoteText,
  quickNoteBusy,
  quickNoteSaved,
  quickNoteError,
  quickNoteMediaId,
  quickNoteProject,
  quickNoteProjectTitle,
  quickNoteFileRef,
  quickNoteCameraRef,
  selectCaptureImage,
  quickNoteImageBusy,
  quickNoteRecognizing,
  quickNoteImage,
  removeCaptureImage,
  quickNoteVisionTarget,
  recognizeCaptureImage,
  recognizeCaptureAsTicket,
  quickNoteSuggestion,
  applyCaptureSuggestion,
  quickNoteImageError,
} = inject('bobAppShell');
</script>

<template>
    <!-- Draggable Edge-Snapping QuickNote Floating Bubble (Restored from legacy bob-agent) -->
    <button
      type="button"
      class="mobile-fab"
      :class="{
        dragging: isDraggingFab,
        'snap-right': fabPos.isRight && fabPos.x === -1,
        'snap-left': !fabPos.isRight && fabPos.x === -1,
      }"
      :style="
        fabPos.x !== -1
          ? { left: `${fabPos.x}px`, top: `${fabPos.y}px` }
          : { top: `${fabPos.y}px` }
      "
      :aria-label="t('quickNoteOpenAria')"
      :title="t('quickNoteOpenAria')"
      @pointerdown="onFabPointerDown"
      @pointermove="onFabPointerMove"
      @pointerup="onFabPointerUp"
      @pointercancel="onFabPointerUp"
    >
      <svg class="fab-svg" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 152.85 99.94" aria-hidden="true">
        <path fill="currentColor" d="M166.3,82.45a29.91,29.91,0,0,1-52.92,19.11,29.91,29.91,0,0,1-46,0A29.91,29.91,0,0,1,14.45,82.45V15.72a2.3,2.3,0,0,1,2.3-2.3H26a2.3,2.3,0,0,1,2.3,2.3V57.24a29.92,29.92,0,0,1,39.12,6.09,29.91,29.91,0,0,1,39.11-6.09V15.72a2.3,2.3,0,0,1,2.3-2.3H118a2.3,2.3,0,0,1,2.3,2.3V57.24a29.92,29.92,0,0,1,46,25.21Zm-13.8,0a16.11,16.11,0,1,0-16.11,16.1A16.11,16.11,0,0,0,152.5,82.45Zm-46,0a16.11,16.11,0,1,0-16.1,16.1A16.1,16.1,0,0,0,106.48,82.45Zm-46,0a16.11,16.11,0,1,0-16.11,16.1A16.11,16.11,0,0,0,60.47,82.45Z" transform="translate(-13.95 -12.92)" />
      </svg>
    </button>

    <!-- QuickNote Glassmorphism Overlay (Restored from legacy QuickNoteOverlay.vue) -->
    <div
      v-if="quickNoteOpen"
      class="quicknote-overlay"
      @click.self="quickNoteOpen = false"
    >
      <div class="quicknote-card">
        <form class="quicknote-input-row" @submit.prevent="submitQuickNote">
          <textarea ref="quickNoteInputRef" v-model="quickNoteText" class="quicknote-input" rows="2"
            :disabled="quickNoteBusy" :aria-label="t('quickNotePlaceholder')" :placeholder="t('quickNotePlaceholder')"
            @keydown.enter.exact.prevent="!$event.isComposing && submitQuickNote()"></textarea>
          <span v-if="quickNoteSaved" class="quicknote-saved-badge">
            {{ t('quickNoteSavedHint') }}
          </span>
        </form>
        <p v-if="quickNoteError" role="alert">{{ t('quickNoteSaveError') }}</p>
        <div v-if="quickNoteText.trim() || quickNoteMediaId" class="quicknote-project-row">
          <label for="quicknote-project">{{ t('quickNoteProject') }}</label>
          <select id="quicknote-project" v-model="quickNoteProject" :disabled="quickNoteBusy">
            <option value="">{{ t('quickNoteUnassigned') }}</option>
            <option v-for="project in catalog?.projects || []" :key="project.id" :value="project.id">{{ project.title }}</option>
            <option value="__new">{{ t('quickNoteNewProject') }}</option>
          </select>
          <input v-if="quickNoteProject === '__new'" v-model="quickNoteProjectTitle" :disabled="quickNoteBusy" :aria-label="t('quickNoteProjectTitle')" :placeholder="t('quickNoteProjectTitle')" />
          <button type="button" :disabled="quickNoteBusy" @click="submitQuickNote">{{ t(quickNoteBusy ? 'saving' : 'quickNoteSave') }}</button>
        </div>
        <div class="quicknote-actions-row">
          <input ref="quickNoteFileRef" class="capture-file-input" type="file" accept="image/png,image/jpeg,image/webp" @change="selectCaptureImage" />
          <input ref="quickNoteCameraRef" class="capture-file-input" type="file" accept="image/png,image/jpeg,image/webp" capture="environment" @change="selectCaptureImage" />
          <button class="quicknote-action-pill" type="button" :disabled="quickNoteBusy || quickNoteImageBusy || quickNoteRecognizing" @click="quickNoteCameraRef?.click()">
            <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 6h4l2-3h4l2 3h4v15H4z"/><circle cx="12" cy="13" r="4"/></svg>{{ t('captureCamera') }}
          </button>
          <button class="quicknote-action-pill" type="button" :disabled="quickNoteBusy || quickNoteImageBusy || quickNoteRecognizing" @click="quickNoteFileRef?.click()">
            <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="3" width="18" height="18" rx="2"/><path d="m3 17 6-6 4 4 3-3 5 5"/><circle cx="16" cy="7" r="1"/></svg>{{ t('captureChooseImage') }}
          </button>
          <span v-if="quickNoteImageBusy" role="status">{{ t('saving') }}</span>
        </div>
        <div v-if="quickNoteMediaId" class="capture-photo-panel">
          <img v-if="quickNoteImage" :src="quickNoteImage" :alt="t('captureOriginal')" />
          <p>{{ t('captureImageStored') }}</p>
          <button type="button" :disabled="quickNoteBusy || quickNoteRecognizing" @click="removeCaptureImage">{{ t('captureRemove') }}</button>
          <template v-if="quickNoteVisionTarget">
            <p>{{ t('captureRecognitionDestination', { model: quickNoteVisionTarget.model_ref, endpoint: quickNoteVisionTarget.base_url }) }}</p>
            <div style="display:flex;gap:8px;flex-wrap:wrap;">
              <button type="button" :disabled="quickNoteBusy || quickNoteRecognizing" @click="recognizeCaptureImage">{{ t(quickNoteRecognizing ? 'captureRecognizing' : 'captureRecognize') }}</button>
              <button type="button" :disabled="quickNoteBusy || quickNoteRecognizing" @click="recognizeCaptureAsTicket">{{ t(quickNoteRecognizing ? 'ticketRecognizingBtn' : 'ticketRecognizeBtn') }}</button>
            </div>
          </template>
          <p v-else>{{ t('captureVisionUnconfigured') }}</p>
        </div>
        <div v-if="quickNoteSuggestion" class="capture-suggestion">
          <label for="capture-suggestion">{{ t('captureSuggestion') }}</label>
          <textarea id="capture-suggestion" v-model="quickNoteSuggestion" rows="4" />
          <button type="button" :disabled="quickNoteBusy" @click="applyCaptureSuggestion">{{ t('captureUseSuggestion') }}</button>
        </div>
        <p v-if="quickNoteImageError" role="alert">{{ t(quickNoteImageError) }}</p>
      </div>
    </div>
</template>
