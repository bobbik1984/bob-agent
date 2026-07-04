<template>
  <div class="tiptap-editor-container" @dragover.prevent @drop.prevent="onDrop">
    <!-- Editor Toolbar -->
    <div v-if="editor" class="editor-toolbar">
      <button @click="editor.chain().focus().toggleBold().run()" :class="{ 'is-active': editor.isActive('bold') }" title="加粗 (Ctrl+B)">
        <strong>B</strong>
      </button>
      <button @click="editor.chain().focus().toggleItalic().run()" :class="{ 'is-active': editor.isActive('italic') }" title="斜体 (Ctrl+I)">
        <em>I</em>
      </button>
      <button @click="editor.chain().focus().toggleStrike().run()" :class="{ 'is-active': editor.isActive('strike') }" title="删除线">
        <s>S</s>
      </button>
      <button @click="editor.chain().focus().toggleCode().run()" :class="{ 'is-active': editor.isActive('code') }" title="行内代码">
        &lt;&gt;
      </button>
      <button @click="editor.chain().focus().toggleHeading({ level: 1 }).run()" :class="{ 'is-active': editor.isActive('heading', { level: 1 }) }" title="标题1">
        H1
      </button>
      <button @click="editor.chain().focus().toggleHeading({ level: 2 }).run()" :class="{ 'is-active': editor.isActive('heading', { level: 2 }) }" title="标题2">
        H2
      </button>
      <button @click="editor.chain().focus().toggleBulletList().run()" :class="{ 'is-active': editor.isActive('bulletList') }" title="无序列表">
        • 列表
      </button>
      <button @click="editor.chain().focus().toggleTaskList().run()" :class="{ 'is-active': editor.isActive('taskList') }" title="待办列表">
        ☐ 待办
      </button>
      <div class="toolbar-spacer" style="flex: 1;"></div>
      <div v-if="saveStatus" class="save-indicator">{{ saveStatus }}</div>
    </div>

    <!-- Tag Bar -->
    <div v-if="tags && tags.length > 0 || showTagInput" class="tag-bar">
      <span v-for="(tag, idx) in tags" :key="tag" class="tag-chip">
        {{ tag }}
        <button class="tag-remove" @click="removeTag(idx)">&times;</button>
      </span>
      <input v-if="showTagInput" ref="tagInputRef" v-model="newTagText"
             class="tag-input" :placeholder="$t('notebook.tag_placeholder')"
             @keydown.enter.prevent="addTag" @keydown.escape="showTagInput = false"
             @blur="addTag" />
      <button v-else class="tag-add-btn" @click="showTagInput = true" :title="$t('notebook.add_tag')">
        + {{ $t('notebook.tags') }}
      </button>
    </div>

    <!-- Editor Content -->
    <editor-content :editor="editor" class="editor-content" />
  </div>
</template>

<script setup>
import { ref, watch, onMounted, onBeforeUnmount, nextTick, defineProps, defineEmits } from 'vue';
import { Editor, EditorContent } from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import TaskList from '@tiptap/extension-task-list';
import TaskItem from '@tiptap/extension-task-item';
import Image from '@tiptap/extension-image';
import Link from '@tiptap/extension-link';
import Placeholder from '@tiptap/extension-placeholder';
import { Markdown } from 'tiptap-markdown';
import { WikilinkExtension, preprocessWikilinks } from '../extensions/WikilinkExtension.js';
import { useI18n } from 'vue-i18n';

const { t } = useI18n();

const props = defineProps({
  modelValue: {
    type: String,
    default: ''
  },
  placeholder: {
    type: String,
    default: '记录你的灵感与思考...'
  },
  saveStatus: {
    type: String,
    default: ''
  },
  tags: {
    type: Array,
    default: () => []
  }
});

const emit = defineEmits(['update:modelValue', 'save', 'update:tags', 'wikilink-click']);

const showTagInput = ref(false);
const newTagText = ref('');
const tagInputRef = ref(null);

const addTag = () => {
  const tag = newTagText.value.trim();
  if (tag && !props.tags.includes(tag)) {
    emit('update:tags', [...props.tags, tag]);
  }
  newTagText.value = '';
  showTagInput.value = false;
};

const removeTag = (idx) => {
  const updated = [...props.tags];
  updated.splice(idx, 1);
  emit('update:tags', updated);
};

watch(showTagInput, (val) => {
  if (val) nextTick(() => tagInputRef.value?.focus());
});

const editor = ref(null);
let internalUpdate = false;

onMounted(() => {
  editor.value = new Editor({
    extensions: [
      StarterKit,
      TaskList,
      TaskItem.configure({
        nested: true,
      }),
      Image,
      Link.configure({
        openOnClick: false,
      }),
      Placeholder.configure({
        placeholder: props.placeholder,
      }),
      Markdown.configure({
        html: true,  // Allow HTML for wikilink span tags
        transformPastedText: true,
        transformCopiedText: true,
      }),
      WikilinkExtension.configure({
        onWikilinkClick: (target) => emit('wikilink-click', target),
      }),
    ],
    content: preprocessWikilinks(props.modelValue),
    onUpdate: ({ editor }) => {
      if (window._ignoreNextTiptapUpdate) return;
      internalUpdate = true;
      const markdown = editor.storage.markdown.getMarkdown();
      emit('update:modelValue', markdown);
      emit('save', markdown); // Optional: Trigger save logic externally
    },
  });
});

watch(() => props.modelValue, (newVal) => {
  if (internalUpdate) {
    internalUpdate = false;
    return;
  }
  if (editor.value) {
    window._ignoreNextTiptapUpdate = true;
    editor.value.commands.setContent(preprocessWikilinks(newVal), false);
    // Reset flag immediately and also on nextTick in case of async updates
    window._ignoreNextTiptapUpdate = false;
    setTimeout(() => { window._ignoreNextTiptapUpdate = false; }, 0);
  }
});

onBeforeUnmount(() => {
  if (editor.value) {
    editor.value.destroy();
  }
});

const onDrop = async (e) => {
  const files = e.dataTransfer.files;
  if (!files || files.length === 0) return;
  
  for (let i = 0; i < files.length; i++) {
    const file = files[i];
    if (file.type.startsWith('image/')) {
      const arrayBuffer = await file.arrayBuffer();
      const bytes = new Uint8Array(arrayBuffer);
      
      const fileName = `img_${Date.now()}_${file.name.replace(/[^a-zA-Z0-9.\-]/g, '_')}`;
      
      try {
        const res = await window.appAPI.notebookSaveAsset(fileName, Array.from(bytes));
        if (res.ok) {
          // Note: Here we insert relative path or some path resolver depending on how the frontend serves assets
          // For now, we will insert the file path that is returned, assuming there is a protocol to serve them.
          // In Tauri, we can serve it using convertFileSrc or a custom protocol.
          const assetUrl = `assets/${fileName}`; 
          editor.value.chain().focus().setImage({ src: assetUrl }).run();
        } else {
          console.error("Failed to save asset:", res.error);
        }
      } catch (err) {
        console.error("Asset upload error:", err);
      }
    }
  }
};
</script>

<style scoped>
.tiptap-editor-container {
  display: flex;
  flex-direction: column;
  height: 100%;
  border: none;
  border-radius: 0;
  overflow: hidden;
  background-color: transparent;
}

.editor-toolbar {
  display: flex;
  gap: 4px;
  height: 53px;
  box-sizing: border-box;
  padding: 0 16px;
  background-color: transparent;
  border-bottom: 1px solid var(--border-subtle);
  flex-wrap: wrap;
  align-items: center;
}

/* ── Tag Bar ── */
.tag-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  padding: 6px 16px;
  border-bottom: 1px solid var(--border-subtle);
  min-height: 32px;
}
.tag-bar .tag-chip {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  padding: 2px 10px;
  border-radius: 999px;
  background-color: var(--bg-tertiary);
  color: var(--text-secondary);
  font-size: 12px;
  font-family: var(--font-sans);
}
.tag-remove {
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  font-size: 14px;
  padding: 0 2px;
  line-height: 1;
  transition: color 0.15s;
}
.tag-remove:hover {
  color: var(--color-error, #ef4444);
}
.tag-input {
  border: none;
  outline: none;
  background: transparent;
  color: var(--text-primary);
  font-size: 12px;
  font-family: var(--font-sans);
  padding: 2px 4px;
  min-width: 80px;
  max-width: 160px;
}
.tag-add-btn {
  background: none;
  border: 1px dashed var(--border-subtle);
  border-radius: 999px;
  padding: 2px 10px;
  color: var(--text-tertiary);
  font-size: 12px;
  font-family: var(--font-sans);
  cursor: pointer;
  transition: all 0.15s;
}
.tag-add-btn:hover {
  color: var(--user-accent);
  border-color: var(--user-accent);
}

.editor-toolbar button {
  background: transparent;
  border: 1px solid transparent;
  border-radius: 4px;
  padding: 4px 8px;
  cursor: pointer;
  color: var(--text-secondary);
  transition: all 0.2s;
  font-size: 0.9em;
}

.editor-toolbar button:hover {
  background-color: var(--bg-tertiary);
  color: var(--text-primary);
}

.editor-toolbar button.is-active {
  background-color: var(--user-accent);
  color: var(--text-inverse);
}

.save-indicator {
  font-size: 12px;
  color: var(--text-muted);
  display: flex;
  align-items: center;
  margin-right: 8px;
}

.editor-content {
  flex: 1;
  overflow-y: auto;
  padding: 24px 32px;
  color: var(--text-primary);
}

/* Tiptap specific styles */
:deep(.ProseMirror) {
  outline: none;
  min-height: 100%;
  line-height: 1.6;
}

:deep(.ProseMirror p) {
  margin-top: 0;
  margin-bottom: 0.8em;
}

:deep(.ProseMirror h1),
:deep(.ProseMirror h2),
:deep(.ProseMirror h3),
:deep(.ProseMirror h4) {
  margin-top: 1.2em;
  margin-bottom: 0.5em;
  line-height: 1.3;
  font-weight: 600;
}

:deep(.ProseMirror ul),
:deep(.ProseMirror ol) {
  margin-top: 0;
  margin-bottom: 0.8em;
  padding-left: 1.5em;
}

:deep(.ProseMirror p.is-editor-empty:first-child::before) {
  color: var(--text-tertiary);
  content: attr(data-placeholder);
  float: left;
  height: 0;
  pointer-events: none;
}

:deep(ul[data-type="taskList"]) {
  list-style: none;
  padding: 0;
}

:deep(ul[data-type="taskList"] li) {
  display: flex;
  align-items: flex-start;
}

:deep(ul[data-type="taskList"] li > label) {
  margin-right: 0.5rem;
  user-select: none;
}

:deep(ul[data-type="taskList"] li > div) {
  flex: 1;
}

:deep(img) {
  max-width: 100%;
  border-radius: 4px;
}

/* ── Wikilinks ── */
:deep(.wikilink) {
  color: var(--user-accent);
  cursor: pointer;
  border-bottom: 1px solid transparent;
  transition: all 0.15s;
  padding: 0 1px;
  border-radius: 2px;
}
:deep(.wikilink:hover) {
  border-bottom-color: var(--user-accent);
  background-color: color-mix(in srgb, var(--user-accent) 8%, transparent);
}
</style>
