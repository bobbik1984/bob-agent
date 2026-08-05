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
          class="btn-secondary" 
          @click="cancel"
        >
          {{ state.cancelText }}
        </button>
        <button 
          :class="state.confirmClass || 'btn-primary'" 
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
  background-color: var(--bg-primary, #1a1a2e);
  border-radius: var(--radius-lg, 12px);
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
  color: var(--text-primary, #111827);
  font-weight: 600;
  text-align: center;
}

.modal-body {
  padding: 8px 20px 16px;
  color: var(--text-secondary, #4b5563);
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

button {
  padding: 8px 20px;
  border-radius: var(--radius-sm, 6px);
  font-size: 0.9rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
  border: none;
  min-width: 80px;
}

.btn-secondary {
  background-color: var(--bg-tertiary, #f3f4f6);
  color: var(--text-secondary, #4b5563);
  border: 1px solid var(--border-primary, transparent);
}

.btn-secondary:hover {
  background-color: var(--surface-secondary, #e5e7eb);
}

.btn-primary {
  background-color: var(--user-accent, var(--accent-primary, #3b82f6));
  color: white;
}

.btn-primary:hover {
  filter: brightness(1.1);
}

.btn-danger {
  background-color: var(--color-error, #ef4444);
  color: white;
}

.btn-danger:hover {
  filter: brightness(1.1);
}

.prompt-input-wrapper {
  margin-top: 12px;
}

.prompt-input {
  width: 100%;
  padding: 8px 12px;
  border-radius: var(--radius-sm, 6px);
  border: 1px solid var(--border-subtle, #e5e7eb);
  background-color: var(--surface-input, var(--bg-secondary, #f9fafb));
  color: var(--text-primary, #111827);
  font-size: 0.95rem;
  outline: none;
  box-sizing: border-box;
  transition: border-color 0.2s;
}

.prompt-input:focus {
  border-color: var(--user-accent, var(--accent-primary, #3b82f6));
}

.prompt-textarea {
  width: 100%;
  padding: 8px 12px;
  border-radius: var(--radius-sm, 6px);
  border: 1px solid var(--border-subtle, #e5e7eb);
  background-color: var(--surface-input, var(--bg-secondary, #f9fafb));
  color: var(--text-primary, #111827);
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
  border-color: var(--user-accent, var(--accent-primary, #3b82f6));
}

/* Dark mode overrides if variables are not fully set */
@media (prefers-color-scheme: dark) {
  .modal-content {
    background-color: var(--bg-primary, #1f2937);
  }
  .modal-header h3 {
    color: var(--text-primary, #f9fafb);
  }
  .modal-body {
    color: var(--text-secondary, #d1d5db);
  }
  .btn-secondary {
    background-color: var(--bg-tertiary, #374151);
    color: var(--text-secondary, #9ca3af);
  }
  .prompt-input {
    background-color: var(--surface-input, var(--bg-secondary, #374151));
    border-color: var(--border-subtle, #4b5563);
    color: var(--text-primary, #f9fafb);
  }
  .prompt-textarea {
    background-color: var(--surface-input, var(--bg-secondary, #374151));
    border-color: var(--border-subtle, #4b5563);
    color: var(--text-primary, #f9fafb);
  }
}
</style>
