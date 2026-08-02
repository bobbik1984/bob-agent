import { reactive, readonly } from 'vue';

const state = reactive({
  isVisible: false,
  title: '',
  message: '',
  type: 'confirm', // 'confirm' or 'alert'
  confirmText: '确定',
  cancelText: '取消',
  inputValue: '',
  inputPlaceholder: '',
  resolvePromise: null,
});

export function useDialog() {
  const showConfirm = (options) => {
    return new Promise((resolve) => {
      // Support passing just a string for simple alerts
      if (typeof options === 'string') {
        options = { message: options };
      }
      state.title = options.title || '提示';
      state.message = options.message || '';
      state.type = 'confirm';
      state.confirmText = options.confirmText || '确定';
      state.cancelText = options.cancelText || '取消';
      state.resolvePromise = resolve;
      state.isVisible = true;
    });
  };

  const showAlert = (options) => {
    return new Promise((resolve) => {
      // Support passing just a string
      if (typeof options === 'string') {
        options = { message: options };
      }
      state.title = options.title || '提示';
      state.message = options.message || '';
      state.type = 'alert';
      state.confirmText = options.confirmText || '确定';
      state.resolvePromise = resolve;
      state.isVisible = true;
    });
  };

  const showPrompt = (options) => {
    return new Promise((resolve) => {
      if (typeof options === 'string') {
        options = { message: options };
      }
      state.title = options.title || '提示';
      state.message = options.message || '';
      state.type = 'prompt';
      state.inputValue = options.defaultValue || '';
      state.inputPlaceholder = options.placeholder || '';
      state.confirmText = options.confirmText || '确定';
      state.cancelText = options.cancelText || '取消';
      state.resolvePromise = resolve;
      state.isVisible = true;
    });
  };

  const confirm = () => {
    state.isVisible = false;
    if (state.resolvePromise) {
      if (state.type === 'prompt') {
        state.resolvePromise(state.inputValue);
      } else {
        state.resolvePromise(true);
      }
      state.resolvePromise = null;
    }
  };

  const cancel = () => {
    state.isVisible = false;
    if (state.resolvePromise) {
      if (state.type === 'prompt') {
        state.resolvePromise(null);
      } else {
        state.resolvePromise(false);
      }
      state.resolvePromise = null;
    }
  };

  return {
    state: readonly(state),
    showConfirm,
    showAlert,
    showPrompt,
    confirm,
    cancel,
  };
}
