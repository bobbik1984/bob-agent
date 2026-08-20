<script setup>
import { ref, computed } from 'vue';
import { ClipboardCopy, Check } from 'lucide-vue-next';
import hljs from 'highlight.js';

const props = defineProps({
  code: { type: String, required: true },
  lang: { type: String, default: '' }
});

const isCopied = ref(false);

const highlightedCode = computed(() => {
  if (!props.code) return '';
  const language = hljs.getLanguage(props.lang) ? props.lang : 'plaintext';
  return hljs.highlight(props.code, { language }).value;
});

const copyCode = async () => {
  try {
    await navigator.clipboard.writeText(props.code);
    isCopied.value = true;
    setTimeout(() => {
      isCopied.value = false;
    }, 2000);
  } catch (e) {
    console.error('Failed to copy code:', e);
  }
};
</script>

<template>
  <div class="bob-code-block">
    <div class="code-header">
      <span class="code-lang">{{ lang || 'code' }}</span>
      <button class="copy-btn" @click="copyCode" :title="isCopied ? '已复制' : '复制代码'">
        <Check v-if="isCopied" :size="14" class="icon-success" />
        <ClipboardCopy v-else :size="14" class="icon-default" />
      </button>
    </div>
    <div class="code-body">
      <pre><code class="hljs" :class="'language-' + lang" v-html="highlightedCode"></code></pre>
    </div>
  </div>
</template>

<style scoped>
.bob-code-block {
  margin: var(--space-3) 0;
  border: 1px solid var(--border-subtle);
  border-radius: 8px;
  background: var(--bg-tertiary);
  overflow: hidden;
  contain: paint; /* 防止拖选越界 */
}

.code-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 4px 8px 4px 12px;
  background: rgba(255, 255, 255, 0.02);
  border-bottom: 1px solid var(--border-subtle);
}

.code-lang {
  font-family: var(--font-mono);
  font-size: 0.75rem;
  color: var(--text-muted);
  text-transform: lowercase;
}

.copy-btn {
  background: transparent;
  border: none;
  padding: 4px;
  border-radius: 4px;
  color: var(--text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all var(--duration-fast);
  opacity: 0; /* 默认隐藏，hover显示 */
}

.bob-code-block:hover .copy-btn {
  opacity: 1;
}

.copy-btn:hover {
  background: var(--surface-hover);
  color: var(--text-secondary);
}

.icon-success {
  color: var(--color-success, #10b981);
}

.code-body {
  margin: 0;
  padding: var(--space-3) var(--space-4);
  overflow-x: auto;
}

.code-body pre {
  margin: 0;
  padding: 0;
  background: transparent;
  border-radius: 0;
  border: none;
}

.code-body code {
  font-family: var(--font-mono);
  font-size: 0.9em;
  background: transparent;
  color: var(--text-primary);
}
</style>
