<template>
  <div v-if="state.isVisible" class="modal-overlay" @click.self="handleOverlayClick">
    <div class="modal-content">
      <div class="modal-header">
        <h3>{{ state.title }}</h3>
      </div>
      <div class="modal-body">
        <p>{{ state.message }}</p>
      </div>
      <div class="modal-footer">
        <button 
          v-if="state.type === 'confirm'" 
          class="btn-secondary" 
          @click="cancel"
        >
          {{ state.cancelText }}
        </button>
        <button 
          class="btn-primary" 
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
  if (state.type === 'confirm') {
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
  background-color: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9999;
  backdrop-filter: blur(2px);
}

.modal-content {
  background-color: var(--bg-primary, #ffffff);
  border-radius: 12px;
  width: 90%;
  max-width: 400px;
  box-shadow: 0 10px 25px rgba(0, 0, 0, 0.2);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--border-color, #e5e7eb);
}

.modal-header {
  padding: 16px 20px;
  border-bottom: 1px solid var(--border-color, #e5e7eb);
}

.modal-header h3 {
  margin: 0;
  font-size: 1.1rem;
  color: var(--text-primary, #111827);
  font-weight: 600;
}

.modal-body {
  padding: 20px;
  color: var(--text-secondary, #4b5563);
  font-size: 0.95rem;
  line-height: 1.5;
}

.modal-body p {
  margin: 0;
  white-space: pre-wrap;
}

.modal-footer {
  padding: 16px 20px;
  display: flex;
  justify-content: flex-end;
  gap: 12px;
  background-color: var(--bg-secondary, #f9fafb);
  border-top: 1px solid var(--border-color, #e5e7eb);
}

button {
  padding: 8px 16px;
  border-radius: 6px;
  font-size: 0.9rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
  border: none;
}

.btn-secondary {
  background-color: transparent;
  color: var(--text-secondary, #4b5563);
  border: 1px solid var(--border-color, #d1d5db);
}

.btn-secondary:hover {
  background-color: var(--bg-tertiary, #f3f4f6);
}

.btn-primary {
  background-color: var(--user-accent, var(--accent-primary, #3b82f6));
  color: white;
}

.btn-primary:hover {
  filter: brightness(1.1);
}

/* Dark mode overrides if variables are not fully set */
@media (prefers-color-scheme: dark) {
  .modal-content {
    background-color: var(--bg-primary, #1f2937);
    border-color: var(--border-color, #374151);
  }
  .modal-header {
    border-color: var(--border-color, #374151);
  }
  .modal-header h3 {
    color: var(--text-primary, #f9fafb);
  }
  .modal-body {
    color: var(--text-secondary, #d1d5db);
  }
  .modal-footer {
    background-color: var(--bg-secondary, #111827);
    border-color: var(--border-color, #374151);
  }
  .btn-secondary {
    border-color: var(--border-color, #4b5563);
    color: var(--text-secondary, #d1d5db);
  }
  .btn-secondary:hover {
    background-color: var(--bg-tertiary, #374151);
  }
}
</style>
