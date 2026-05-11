import { createI18n } from 'vue-i18n';
import zhCN from './locales/zh-CN.json';
import en from './locales/en.json';

const i18n = createI18n({
  legacy: false,          // 使用 Composition API 模式
  locale: 'zh-CN',        // 默认语言
  fallbackLocale: 'en',   // 找不到 key 时降级到英文
  messages: {
    'zh-CN': zhCN,
    'en': en,
  },
});

export default i18n;
