<script setup>
import { computed, reactive, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

const props = defineProps({
  notes: {
    type: Array,
    default: () => [],
  },
  captures: {
    type: Array,
    default: () => [],
  },
  memories: {
    type: Array,
    default: () => [],
  },
  decisions: {
    type: Array,
    default: () => [],
  },
  busy: {
    type: Boolean,
    default: false,
  },
});

const emit = defineEmits([
  'save-note',
  'create-capture',
  'update-memory',
  'create-decision',
  'update-decision',
]);

const { t } = useI18n();
const activeSubtab = ref('notes'); // 'notes' | 'captures' | 'memories' | 'decisions'
const searchQuery = ref('');
const selectedNoteId = ref(null);

const noteEditor = reactive({
  id: '',
  title: '',
  tagsText: '',
  body: '',
});

const captureDraft = ref('');
const decisionDraft = ref('');

const filteredNotes = computed(() => {
  const q = searchQuery.value.trim().toLowerCase();
  const list = props.notes || [];
  if (!q) return list;
  return list.filter(n => {
    const inTitle = (n.title || '').toLowerCase().includes(q);
    const inBody = (n.body_markdown || n.body || '').toLowerCase().includes(q);
    const inTags = (n.tags || []).some(tag => tag.toLowerCase().includes(q));
    return inTitle || inBody || inTags;
  });
});

watch(
  () => props.notes,
  list => {
    if (!selectedNoteId.value && Array.isArray(list) && list.length > 0) {
      selectNote(list[0]);
    }
  },
  { immediate: true },
);

function selectNote(note) {
  if (!note) return;
  selectedNoteId.value = note.id;
  noteEditor.id = note.id;
  noteEditor.title = note.title || '';
  noteEditor.tagsText = Array.isArray(note.tags) ? note.tags.join(', ') : '';
  noteEditor.body = note.body_markdown || note.body || '';
}

function startNewNote() {
  const id = `note-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
  selectedNoteId.value = id;
  noteEditor.id = id;
  noteEditor.title = '';
  noteEditor.tagsText = '';
  noteEditor.body = '';
}

function submitNote() {
  const title = noteEditor.title.trim();
  if (!title) return;
  const id =
    noteEditor.id || `note-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
  const tags = noteEditor.tagsText
    .split(',')
    .map(s => s.trim())
    .filter(Boolean);
  emit('save-note', {
    id,
    title,
    body: noteEditor.body,
    tags,
  });
}

function submitCapture() {
  const content = captureDraft.value.trim();
  if (!content) return;
  emit('create-capture', {
    id: `cap-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
    content,
  });
  captureDraft.value = '';
}

function submitDecision() {
  const body = decisionDraft.value.trim();
  if (!body) return;
  emit('create-decision', {
    id: `dec-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
    body,
    status: 'accepted',
  });
  decisionDraft.value = '';
}
</script>

<template>
  <section class="primary-view knowledge-view" :aria-label="t('knowledgeViewTitle')">
    <header class="view-hero">
      <div class="view-hero-titles">
        <h2>{{ t('knowledgeViewTitle') }}</h2>
        <p>{{ t('knowledgeViewSubtitle') }}</p>
      </div>
      <div class="view-subtabs" role="tablist">
        <button
          type="button"
          class="view-subtab-btn"
          :class="{ active: activeSubtab === 'notes' }"
          @click="activeSubtab = 'notes'"
        >
          📝 {{ t('knowledgeTabNotes') }}
          <span class="subtab-count">{{ notes.length }}</span>
        </button>
        <button
          type="button"
          class="view-subtab-btn"
          :class="{ active: activeSubtab === 'captures' }"
          @click="activeSubtab = 'captures'"
        >
          ⚡ {{ t('knowledgeTabCaptures') }}
          <span class="subtab-count">{{ captures.length }}</span>
        </button>
        <button
          type="button"
          class="view-subtab-btn"
          :class="{ active: activeSubtab === 'memories' }"
          @click="activeSubtab = 'memories'"
        >
          🧠 {{ t('knowledgeTabMemories') }}
          <span class="subtab-count">{{ memories.length }}</span>
        </button>
        <button
          type="button"
          class="view-subtab-btn"
          :class="{ active: activeSubtab === 'decisions' }"
          @click="activeSubtab = 'decisions'"
        >
          ⚖ {{ t('knowledgeTabDecisions') }}
          <span class="subtab-count">{{ decisions.length }}</span>
        </button>
      </div>
    </header>

    <div class="view-content-body">
      <template v-if="activeSubtab === 'notes'">
        <div class="knowledge-toolbar">
          <input
            v-model="searchQuery"
            type="search"
            class="knowledge-search-input"
            :placeholder="t('knowledgeSearchPlaceholder')"
          />
          <button type="button" class="primary-action-btn" @click="startNewNote">
            + {{ t('knowledgeNewNoteBtn') }}
          </button>
        </div>

        <div class="knowledge-split-pane">
          <aside class="knowledge-note-list hub-card">
            <p v-if="!filteredNotes.length" class="view-card-desc">
              {{ t('knowledgeEmptyNotesHint') }}
            </p>
            <button
              v-for="note in filteredNotes"
              :key="note.id"
              type="button"
              class="knowledge-note-item"
              :class="{ active: selectedNoteId === note.id }"
              @click="selectNote(note)"
            >
              <strong>{{ note.title }}</strong>
              <div v-if="(note.tags || []).length" class="knowledge-tag-row">
                <span v-for="tag in note.tags" :key="tag" class="knowledge-tag-chip">
                  #{{ tag }}
                </span>
              </div>
              <small class="knowledge-note-preview">
                {{ (note.body_markdown || note.body || '').slice(0, 72) }}
              </small>
            </button>
          </aside>

          <form class="knowledge-note-editor hub-card" @submit.prevent="submitNote">
            <input
              v-model="noteEditor.title"
              type="text"
              class="note-editor-title"
              :placeholder="t('knowledgeNoteTitlePlaceholder')"
              required
            />
            <input
              v-model="noteEditor.tagsText"
              type="text"
              class="note-editor-tags"
              :placeholder="t('knowledgeNoteTagsPlaceholder')"
            />
            <textarea
              v-model="noteEditor.body"
              rows="8"
              class="note-editor-body"
              :placeholder="t('knowledgeNoteBodyPlaceholder')"
            ></textarea>
            <div class="note-editor-actions">
              <button
                type="submit"
                class="primary-action-btn"
                :disabled="busy || !noteEditor.title.trim()"
              >
                {{ t('knowledgeSaveNoteBtn') }}
              </button>
            </div>
          </form>
        </div>
      </template>

      <template v-else-if="activeSubtab === 'captures'">
        <form class="quick-capture-form hub-card" @submit.prevent="submitCapture">
          <input
            v-model="captureDraft"
            type="text"
            :placeholder="t('knowledgeCapturePlaceholder')"
            required
          />
          <button
            type="submit"
            class="primary-action-btn"
            :disabled="busy || !captureDraft.trim()"
          >
            {{ t('knowledgeAddCaptureBtn') }}
          </button>
        </form>

        <div v-if="!captures.length" class="view-empty-card hub-card">
          <p>{{ t('knowledgeEmptyNotesHint') }}</p>
        </div>
        <div v-else class="captures-list">
          <article v-for="cap in captures" :key="cap.id" class="hub-card capture-item-card">
            <p>{{ cap.content_ref || cap.content }}</p>
          </article>
        </div>
      </template>

      <template v-else-if="activeSubtab === 'memories'">
        <div v-if="!memories.length" class="view-empty-card hub-card">
          <p>{{ t('knowledgeEmptyMemoriesHint') }}</p>
        </div>
        <div v-else class="captures-list">
          <article v-for="mem in memories" :key="mem.id" class="hub-card memory-item-card">
            <div class="memory-item-main">
              <strong>{{ mem.body }}</strong>
              <span class="schedule-kind-badge">
                {{ mem.state }}
                <template v-if="mem.source_capture_id || mem.source_message_id">
                  · {{ mem.source_capture_id || mem.source_message_id }}
                </template>
              </span>
            </div>
            <div class="memory-item-actions">
              <button
                v-if="mem.state !== 'active'"
                type="button"
                class="mini-ghost-btn"
                @click="emit('update-memory', { id: mem.id, state: 'active' })"
              >
                {{ t('knowledgeMemoryActivateBtn') }}
              </button>
              <button
                v-if="mem.state !== 'forgotten'"
                type="button"
                class="mini-ghost-btn"
                @click="emit('update-memory', { id: mem.id, state: 'forgotten' })"
              >
                {{ t('knowledgeMemoryForgetBtn') }}
              </button>
            </div>
          </article>
        </div>
      </template>

      <template v-else>
        <form class="quick-capture-form hub-card" @submit.prevent="submitDecision">
          <input
            v-model="decisionDraft"
            type="text"
            :placeholder="t('knowledgeDecisionPlaceholder')"
            required
          />
          <button
            type="submit"
            class="primary-action-btn"
            :disabled="busy || !decisionDraft.trim()"
          >
            {{ t('knowledgeAddDecisionBtn') }}
          </button>
        </form>

        <div v-if="!decisions.length" class="view-empty-card hub-card">
          <p>{{ t('knowledgeEmptyDecisionsHint') }}</p>
        </div>
        <div v-else class="captures-list">
          <article v-for="dec in decisions" :key="dec.id" class="hub-card memory-item-card">
            <div class="memory-item-main">
              <strong>{{ dec.body }}</strong>
              <span class="schedule-kind-badge">{{ dec.project_id }} · {{ dec.status }}</span>
            </div>
            <div class="memory-item-actions">
              <button
                v-if="dec.status !== 'accepted'"
                type="button"
                class="mini-ghost-btn"
                @click="emit('update-decision', { id: dec.id, status: 'accepted' })"
              >
                {{ t('decisionAcceptBtn') }}
              </button>
              <button
                v-if="dec.status !== 'rejected'"
                type="button"
                class="mini-ghost-btn"
                @click="emit('update-decision', { id: dec.id, status: 'rejected' })"
              >
                {{ t('decisionRejectBtn') }}
              </button>
            </div>
          </article>
        </div>
      </template>
    </div>
  </section>
</template>
