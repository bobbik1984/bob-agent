<template>
  <div v-if="state.isVisible" class="modal-overlay" @click.self="handleOverlayClick">
    <div class="modal-content">
      <div class="modal-header">
        <h3>{{ state.title }}</h3>
      </div>
      <div class="modal-body">
        <p v-if="state.message">{{ state.message }}</p>
        <div v-if="state.type === 'prompt'" class="prompt-input-wrapper">
          <input 
            type="text"
            v-model="state.inputValue" 
            :placeholder="state.inputPlaceholder"
            @keyup.enter="!state.showDescription && confirm()"
            class="prompt-input"
            autofocus
          />
          <textarea
            v-if="state.showDescription"
            v-model="state.descriptionValue"
            :placeholder="state.descriptionPlaceholder"
            class="prompt-textarea"
            rows="3"
          ></textarea>
        </div>
      </div>
      <div class="modal-footer">
        <button 
          v-if="state.type === 'confirm' || state.type === 'prompt'" 
          class="btn btn-secondary"
          @click="cancel"
        >
          {{ state.cancelText }}
        </button>
        <button 
          :class="['btn', state.confirmClass || 'btn-primary']"
          @click="confirm"
        >
          {{ state.confirmText }}
        </button>
      </div>
    </div>
  </div>
</template>

<script setup>
import { useDialog } from '../composables/useDialog.js';

const { state, confirm, cancel } = useDialog();

const handleOverlayClick = () => {
  if (state.type === 'confirm' || state.type === 'prompt') {
    cancel();
  } else {
    confirm();
  }
};
</script>

<style scoped>
.modal-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background-color: rgba(0, 0, 0, 0.4);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9999;
  backdrop-filter: blur(4px);
}

.modal-content {
  background-color: var(--bg-primary, #141414);
  border-radius: var(--radius-lg, 10px);
  width: 90%;
  max-width: 340px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--border-subtle);
}

.modal-header {
  padding: 20px 20px 8px;
  border-bottom: none;
}

.modal-header h3 {
  margin: 0;
  font-size: 1.1rem;
  color: var(--text-primary, #e8e8e8);
  font-weight: 600;
  text-align: center;
}

.modal-body {
  padding: 8px 20px 16px;
  color: var(--text-secondary, #a0a0a0);
  font-size: 0.95rem;
  line-height: 1.5;
  text-align: center;
}

.modal-body p {
  margin: 0;
  white-space: pre-wrap;
}

.modal-footer {
  padding: 12px 20px 20px;
  display: flex;
  justify-content: center;
  gap: 12px;
  background-color: transparent;
  border-top: none;
}

.modal-footer .btn { min-width: 80px; }

.prompt-input-wrapper {
  margin-top: 12px;
}

.prompt-input {
  width: 100%;
  padding: 8px 12px;
  border-radius: var(--radius-sm, 4px);
  border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
  background-color: var(--surface-input, rgba(255, 255, 255, 0.08));
  color: var(--text-primary, #e8e8e8);
  font-size: 0.95rem;
  outline: none;
  box-sizing: border-box;
  transition: border-color 0.2s;
}

.prompt-input:focus {
  border-color: var(--user-accent, var(--accent-primary, #e8e8e8));
}

.prompt-textarea {
  width: 100%;
  padding: 8px 12px;
  border-radius: var(--radius-sm, 4px);
  border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
  background-color: var(--surface-input, rgba(255, 255, 255, 0.08));
  color: var(--text-primary, #e8e8e8);
  font-size: 0.9rem;
  font-family: inherit;
  outline: none;
  box-sizing: border-box;
  resize: vertical;
  min-height: 60px;
  margin-top: 8px;
  transition: border-color 0.2s;
}

.prompt-textarea:focus {
  border-color: var(--user-accent, var(--accent-primary, #e8e8e8));
}
</style>
