<template>
  <div class="note-explorer">
    <div class="explorer-header">
      <div class="view-toggles">
        <button :class="{ active: viewMode === 'tree' }" @click="viewMode = 'tree'" :title="$t('notebook.tree_view')"><Folder :size="16" /></button>
        <button :class="{ active: viewMode === 'timeline' }" @click="viewMode = 'timeline'" :title="$t('notebook.timeline_view')"><CalendarDays :size="16" /></button>
      </div>
      <div class="header-actions">
        <button class="new-note-btn" @click="loadNotes" :title="$t('notebook.refresh')">
          <RefreshCw :size="16" />
        </button>
        <button class="new-note-btn" @click="createNewNote" :title="$t('notebook.new_note')">
          <Plus :size="16" />
        </button>
      </div>
    </div>

    <!-- 树状视图 -->
    <div v-if="viewMode === 'tree'" class="explorer-content">
      <div class="section">
        <div class="section-title" @click="toggleSection('daily')">
          <ChevronRight :size="16" class="caret" :class="{ open: expanded.daily }" /> {{ $t('notebook.daily') }}
        </div>
        <ul v-show="expanded.daily" class="note-list">
          <li v-for="note in notes.daily" :key="note.id" 
              :class="{ active: selectedNoteId === note.id }"
              @click="selectNote(note)">
            <CalendarDays :size="14" class="icon" /> {{ note.title || formatDailyName(note.id) }}
            <button class="del-btn" @click.stop="deleteNote(note)" title="删除"><Trash2 :size="14" /></button>
          </li>
          <li v-if="notes.daily.length === 0" class="empty-text">{{ $t('notebook.empty_daily') }}</li>
        </ul>
      </div>

      <div class="section" @dragover.prevent @dragenter.prevent @drop="onDrop($event, 'topics')">
        <div class="section-title" @click="toggleSection('topics')">
          <ChevronRight :size="16" class="caret" :class="{ open: expanded.topics }" /> {{ $t('notebook.topics') }}
        </div>
        <ul v-show="expanded.topics" class="note-list">
          <li v-for="note in notes.topics" :key="note.id" 
              draggable="true"
              @dragstart="onDragStart($event, note, 'topics')"
              :class="{ active: selectedNoteId === note.id }"
              @click="selectNote(note)">
            <FileText :size="14" class="icon" /> 
            <span class="title-text">{{ note.title || formatTopicName(note.id) }}</span>
            <button class="del-btn" @click.stop="deleteNote(note)" title="删除"><Trash2 :size="14" /></button>
          </li>
          <li v-if="notes.topics.length === 0" class="empty-text">{{ $t('notebook.empty_topics') }}</li>
        </ul>
      </div>

      <!-- 知识文献 -->
      <div class="section" @dragover.prevent @dragenter.prevent @drop="onDrop($event, 'wiki/sources')">
        <div class="section-title" @click="toggleSection('sources')">
          <ChevronRight :size="16" class="caret" :class="{ open: expanded.sources }" /> {{ $t('notebook.sources') }}
        </div>
        <ul v-show="expanded.sources" class="note-list">
          <li v-for="note in notes.sources" :key="note.id" 
              draggable="true"
              @dragstart="onDragStart($event, note, 'wiki/sources')"
              :class="{ active: selectedNoteId === note.id }"
              @click="selectNote(note)">
            <FileText :size="14" class="icon" /> 
            <span class="title-text">{{ note.title || formatTopicName(note.id.replace('wiki/sources/', '')) }}</span>
            <button class="del-btn" @click.stop="deleteNote(note)" title="删除"><Trash2 :size="14" /></button>
          </li>
          <li v-if="!notes.sources || notes.sources.length === 0" class="empty-text">{{ $t('notebook.empty_sources') }}</li>
        </ul>
      </div>
    </div>

    <!-- 时间线视图 -->
    <div v-else class="explorer-content">
      <div class="timeline-list">
        <!-- 将所有笔记按照修改/创建时间排序显示 -->
        <div v-for="note in sortedTimelineNotes" :key="note.id" 
             class="timeline-item"
             :class="{ active: selectedNoteId === note.id }"
             @click="selectNote(note)">
          <div class="timeline-meta" v-if="formatTimelineDate(note)">{{ formatTimelineDate(note) }}</div>
          <div class="timeline-title">{{ note.title || formatAnyName(note.id) }}</div>
          <div class="timeline-dot"></div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted, computed, defineEmits, defineProps } from 'vue';
import { Folder, CalendarDays, ChevronRight, FileText, Plus, Trash2, RefreshCw } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';

const { t } = useI18n();


const props = defineProps({
  selectedNoteId: {
    type: String,
    default: null
  }
});

const emit = defineEmits(['select', 'create']);

const viewMode = ref('tree'); // 'tree' or 'timeline'
const notes = ref({ daily: [], topics: [] });
const expanded = ref({ daily: true, topics: true,
  sources: true });

const loadNotes = async () => {
  try {
    const res = await window.electronAPI.notebookListNotes();
    if (res && res.daily) {
      notes.value = res;
    if (!notes.value.sources) notes.value.sources = [];
    }
  } catch (e) {
    console.error("Failed to load notes", e);
  }
};

onMounted(() => {
  loadNotes();
});

const toggleSection = (sec) => {
  expanded.value[sec] = !expanded.value[sec];
};

const formatDailyName = (id) => {
  return id.replace('daily/', '').replace('.md', '');
};

const formatTopicName = (id) => {
  return id.replace('topics/', '').replace('.md', '');
};

const formatAnyName = (id) => {
  return id.split('/').pop().replace('.md', '');
};

const formatTimelineDate = (note) => {
  const t = note.updated || note.created;
  
  if (!t) {
    const name = note.title || note.id || '';
    const match = name.match(/(\d{4})[-_]?(\d{2})[-_]?(\d{2})/);
    if (match) {
      return `${match[1]}-${match[2]}-${match[3]}`;
    }
    return '';
  }

  // Attempt to parse '2026-06-29 15:30:00' to '06-29 15:30'
  try {
    const parts = t.split(' ');
    if (parts.length === 2) {
      const dateParts = parts[0].split('-');
      const datePart = dateParts.length === 3 ? `${dateParts[1]}-${dateParts[2]}` : parts[0];
      const timePart = parts[1].substring(0, 5); // '15:30'
      return `${datePart} ${timePart}`;
    }
    return t;
  } catch (e) {
    return t;
  }
};

const sortedTimelineNotes = computed(() => {
  const allNotes = [
    ...(notes.value.daily || []),
    ...(notes.value.topics || []),
    ...(notes.value.sources || [])
  ];
  return allNotes.sort((a, b) => {
    // If we have updated or created fields, use them for sorting
    const timeA = a.updated || a.created || a.id;
    const timeB = b.updated || b.created || b.id;
    return timeB.localeCompare(timeA);
  });
});

const selectNote = (note) => {
  emit('select', note.id);
};

const createNewNote = async () => {
  const title = prompt("请输入新笔记标题:");
  if (!title) return;
  try {
    const res = await window.electronAPI.notebookCreateNote(title, []);
    if (res.ok) {
      await loadNotes();
      emit('select', res.path);
    } else {
      alert("创建失败: " + res.error);
    }
  } catch (e) {
    alert("创建失败: " + e.message);
  }
};

const deleteNote = async (note) => {
  if (!confirm(`确定要删除笔记 "${formatTopicName(note.id) || formatDailyName(note.id)}" 吗？`)) return;
  try {
    const res = await window.electronAPI.notebookDeleteNote(note.id);
    if (res.ok) {
      await loadNotes();
      if (props.selectedNoteId === note.id) {
        emit('select', null);
      }
    } else {
      alert("删除失败: " + res.error);
    }
  } catch (e) {
    alert("删除失败: " + e.message);
  }
};

const onDragStart = (e, note, category) => {
  e.dataTransfer.setData('noteId', note.id);
  e.dataTransfer.setData('category', category);
  e.dataTransfer.effectAllowed = 'move';
};

const onDrop = async (e, targetCategory) => {
  const noteId = e.dataTransfer.getData('noteId');
  const sourceCategory = e.dataTransfer.getData('category');
  
  if (!noteId || !sourceCategory || sourceCategory === targetCategory) return;
  
  try {
    const res = await window.electronAPI.notebookMoveNote(noteId, targetCategory);
    if (res.ok) {
      await loadNotes();
      if (props.selectedNoteId === noteId) {
        emit('select', res.new_id); // update selected note to the new path
      }
    } else {
      alert("移动失败: " + res.error);
    }
  } catch (err) {
    alert("移动失败: " + err.message);
  }
};

// 暴露刷新方法给父组件
defineExpose({
  refresh: loadNotes
});
</script>

<style scoped>
.note-explorer {
  display: flex;
  flex-direction: column;
  height: 100%;
  background-color: transparent;
  overflow: hidden;
}

.explorer-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  height: 53px;
  box-sizing: border-box;
  padding: 0 16px;
  border-bottom: 1px solid var(--border-subtle);
  background-color: transparent;
}

.header-actions {
  display: flex;
  gap: 4px;
}

.view-toggles {
  display: flex;
  gap: 4px;
  background-color: var(--bg-tertiary);
  padding: 2px;
  border-radius: 6px;
}
.view-toggles button {
  background: transparent;
  border: none;
  cursor: pointer;
  color: var(--text-tertiary);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 6px;
  border-radius: 4px;
  transition: all 0.2s;
}
.view-toggles button.active, .view-toggles button:hover {
  color: var(--text-primary);
  background-color: var(--bg-primary);
  box-shadow: 0 1px 2px rgba(0,0,0,0.05);
}

.new-note-btn {
  background: transparent;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 6px;
  border-radius: 6px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.2s;
}
.new-note-btn:hover {
  background-color: var(--bg-tertiary);
  color: var(--text-primary);
}

.explorer-content {
  flex: 1;
  overflow-y: auto;
  padding: 8px 0;
}

.section-title {
  padding: 8px 16px;
  font-size: var(--text-sm);
  font-family: var(--font-sans);
  font-weight: 500;
  color: var(--text-tertiary);
  cursor: pointer;
  user-select: none;
  display: flex;
  align-items: center;
  gap: 6px;
  transition: color 0.2s;
}
.section-title:hover {
  color: var(--text-secondary);
}

.section-title:hover {
  color: var(--text-primary);
}

.caret {
  transition: transform 0.2s cubic-bezier(0.4, 0, 0.2, 1);
  color: var(--text-tertiary);
}

.caret.open {
  transform: rotate(90deg);
}

.note-list {
  list-style: none;
  padding: 0 8px;
  margin: 2px 0 16px 0;
}

.note-list li {
  padding: 8px 12px;
  margin-bottom: 2px;
  border-radius: 6px;
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 10px;
  color: var(--text-secondary);
  font-size: var(--text-sm);
  font-family: var(--font-sans);
  position: relative;
  transition: all 0.2s ease;
}

.note-list li:hover {
  background-color: var(--bg-tertiary);
  color: var(--text-primary);
}

.note-list li.active {
  background-color: var(--user-accent);
  color: #ffffff;
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.1);
}
.note-list li .icon {
  color: var(--text-tertiary);
}
.note-list li:hover .icon {
  color: var(--text-secondary);
}
.note-list li.active .icon {
  color: rgba(255, 255, 255, 0.8);
}

.title-text {
  flex: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.del-btn {
  background: transparent;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  opacity: 0;
  padding: 6px;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.2s;
}
.note-list li.active .del-btn {
  color: rgba(255, 255, 255, 0.7);
}
.note-list li.active:hover .del-btn {
  color: rgba(255, 255, 255, 1);
}

.note-list li:hover .del-btn {
  opacity: 0.7;
}

.del-btn:hover {
  opacity: 1 !important;
  background-color: rgba(239, 68, 68, 0.1);
}

.empty-text {
  color: var(--text-tertiary);
  font-size: var(--text-sm);
  font-family: var(--font-sans);
  font-style: italic;
  padding: 4px 24px !important;
  pointer-events: none;
}

.timeline-list {
  padding: 12px;
  position: relative;
}

.timeline-item {
  padding: 8px 12px 8px 24px;
  cursor: pointer;
  position: relative;
  border-radius: 4px;
  font-family: var(--font-sans);
  transition: all 0.2s ease;
}

.timeline-meta {
  font-size: 11px;
  color: var(--text-tertiary);
  margin-bottom: 2px;
}

.timeline-title {
  color: var(--text-secondary);
  font-size: var(--text-sm);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.timeline-item:hover {
  background-color: var(--bg-tertiary);
}
.timeline-item:hover .timeline-title {
  color: var(--text-primary);
}

.timeline-item.active {
  background-color: var(--user-accent, #3b82f6);
}
.timeline-item.active .timeline-title,
.timeline-item.active .timeline-meta {
  color: #ffffff;
}

.timeline-dot {
  position: absolute;
  left: 8px;
  top: 18px;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background-color: var(--bg-secondary);
  border: 2px solid var(--border-strong, var(--text-tertiary));
}

.timeline-item.active .timeline-dot {
  background-color: #ffffff;
  border-color: #ffffff;
}

.timeline-list::before {
  content: '';
  position: absolute;
  left: 11px;
  top: 12px;
  bottom: 12px;
  width: 2px;
  background-color: var(--border-light);
  z-index: 0;
}
</style>
