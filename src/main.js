import { createApp } from 'vue';
import App from './App.vue';
import i18n from './i18n';
import './tauri-bridge.js'; // V2 Tauri Bridge Adapter
import './index.css';

window.__bobBootDiag?.('main-module-evaluated');
const app = createApp(App);
app.use(i18n);
window.__bobBootDiag?.('vue-mount-begin');

// 异常安全边界：捕获 Vue 组件树中的所有未捕获异常，绝不允许抛错导致整树卸载/白屏
app.config.errorHandler = (err, instance, info) => {
  console.error('[Vue Global ErrorHandler]', err, info);
};

app.mount('#app');
window.__bobBootDiag?.('vue-mount-complete');

// 窗口亮相：Vue 已挂载，native-splash 已覆盖全屏
// 此时 WebView2 的白色底板被完全遮住，可以安全显示窗口
if (window.appAPI?.showWindow) {
  window.appAPI.showWindow().catch(e => {
    console.warn('[main.js] window.show() failed:', e);
  });
}
