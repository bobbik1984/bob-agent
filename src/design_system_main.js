import { createApp } from 'vue';
import App from './views/DesignSystem.vue';
import i18n from './i18n';
import './index.css';

const app = createApp(App);
app.use(i18n);
app.mount('#app');
