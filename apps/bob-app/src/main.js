import { createApp } from 'vue';
import { createI18n } from 'vue-i18n';
import App from './App.vue';
import en from './locales/en-US.json';
import zh from './locales/zh-CN.json';
import './style.css';
import { registerApprovalServiceWorker } from './approval-notifications.mjs';

void registerApprovalServiceWorker();
const locale = navigator.language.startsWith('zh') ? 'zh-CN' : 'en-US';
const i18n = createI18n({ legacy: false, locale, fallbackLocale: 'en-US', messages: { 'en-US': en, 'zh-CN': zh } });
createApp(App).use(i18n).mount('#app');
