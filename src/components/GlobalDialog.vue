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
  border-radius: 16px;
  width: 90%;
  max-width: 340px;
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.15);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: none;
}

.modal-header {
  padding: 24px 20px 8px;
  border-bottom: none;
}

.modal-header h3 {
  margin: 0;
  font-size: 1.2rem;
  color: var(--text-primary, #111827);
  font-weight: 600;
  text-align: center;
}

.modal-body {
  padding: 8px 24px 24px;
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
  padding: 16px 20px 20px;
  display: flex;
  justify-content: center;
  gap: 12px;
  background-color: transparent;
  border-top: none;
}

button {
  padding: 10px 24px;
  border-radius: 8px;
  font-size: 0.95rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
  border: none;
  min-width: 100px;
}

.btn-secondary {
  background-color: var(--bg-tertiary, #f3f4f6);
  color: var(--text-secondary, #4b5563);
  border: none;
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
  }
  .modal-header h3 {
    color: var(--text-primary, #f9fafb);
  }
  .modal-body {
    color: var(--text-secondary, #d1d5db);
  }
  .btn-secondary {
}
</style>
