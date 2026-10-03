import { createApp } from 'vue';
import App from './App.vue';
import i18n from './i18n';
import { startPostMountBridgeListeners } from './tauri-bridge.js'; // V2 Tauri Bridge Adapter
import './index.css';

const app = createApp(App);
app.use(i18n);
app.mount('#app');

// 非首屏必需的原生事件监听只在 Vue 挂载成功后注册。
startPostMountBridgeListeners().catch(e => {
  console.error('[main.js] post-mount bridge listener failed:', e);
});

// 窗口亮相：Vue 已挂载，native-splash 已覆盖全屏
// 此时 WebView2 的白色底板被完全遮住，可以安全显示窗口
window.appAPI.showWindow().catch(e => {
  console.warn('[main.js] window.show() failed:', e);
});
