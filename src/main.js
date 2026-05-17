import { createApp } from 'vue';
import App from './App.vue';
import i18n from './i18n';
import './tauri-bridge.js'; // V2 Tauri Bridge Adapter
import './index.css';

const app = createApp(App);
app.use(i18n);
app.mount('#app');
