<template>
  <div class="kg-view" :class="isMobile ? 'kg-mobile-col' : 'layout-row'">
    <!-- 移动端二级导航 Tab 栏 -->
    <div v-if="isMobile" class="mobile-tab-grid">
      <button class="mobile-tab-item" :class="{ active: currentMode === 'graph' }" @click="currentMode = 'graph'">
        <Waypoints :size="20" class="tab-icon" />
        <span>图谱</span>
      </button>
      <button class="mobile-tab-item" :class="{ active: currentMode === 'notebook' }" @click="currentMode = 'notebook'">
        <FileText :size="20" class="tab-icon" />
        <span>笔记</span>
      </button>
      <button class="mobile-tab-item" :class="{ active: currentMode === 'ticket' }" @click="currentMode = 'ticket'">
        <Ticket :size="20" class="tab-icon" />
        <span>{{ $t('ticket.my_tickets') || '票夹' }}</span>
      </button>
    </div>

    <!-- 侧边栏传送门 -->
    <Teleport to="#kg-sidebar-portal" v-if="isMounted">
      <div class="kg-sidebar-wrapper">
        <div class="kg-sidebar-header">
          <div class="mode-toggle">
            <button :class="{ active: currentMode === 'graph' }" @click="currentMode = 'graph'">
              <Waypoints :size="16" /> {{ $t('kg.graph_view') }}
            </button>
            <button :class="{ active: currentMode === 'notebook' }" @click="currentMode = 'notebook'">
              <FileText :size="16" /> {{ $t('kg.notebook_view') }}
            </button>
            <button :class="{ active: currentMode === 'ticket' }" @click="currentMode = 'ticket'">
              <Ticket :size="16" /> {{ $t('ticket.my_tickets') || '票夹' }}
            </button>
          </div>
        </div>

        <!-- 图谱侧边栏内容 -->
        <div v-show="currentMode === 'graph'" class="kg-sidebar-content graph-sidebar">
          <div v-if="topProjects.length > 0" class="kg-project-list">
            <h3 class="kg-project-list-title">{{ $t('kg.top_projects') || '主要项目' }}</h3>
            <button
              v-for="proj in topProjects"
              :key="proj.id"
              class="kg-project-item"
              :class="{ active: selectedNode?.id === proj.id }"
              @click="focusNode(proj.id)"
            >
              <span class="project-icon" :style="{ color: kgColors['project'] || kgColors['Project'] || 'var(--user-accent, var(--accent-primary))' }">
                <Star :size="14" fill="currentColor" />
              </span>
              <span class="project-name">{{ proj.label }}</span>
              <span class="project-degree">{{ proj.degree }}</span>
            </button>
          </div>
          <div v-else class="kg-project-list-empty">
            <span v-if="loading">{{ te('kg.loading') ? t('kg.loading') : '加载中...' }}</span>
            <span v-else>{{ te('kg.no_projects') ? t('kg.no_projects') : '暂无项目节点' }}</span>
          </div>

          <!-- 来源批次列表 -->
          <div v-if="stats && stats.source_batches && stats.source_batches.length > 0" class="kg-project-list" style="margin-top: 16px;">
            <h3 class="kg-project-list-title">来源批次</h3>
            <button
              v-for="batch in stats.source_batches"
              :key="batch.batch_id"
              class="kg-project-item"
              :class="{ active: selectedNode?.id === 'source_' + batch.batch_id }"
              @click="focusNode('source_' + batch.batch_id)"
            >
              <span class="project-icon" :style="{ color: kgColors['source'] || 'var(--user-accent, var(--accent-primary))' }">
                <Package :size="14" />
              </span>
              <span class="project-name" :title="batch.folder_path">{{ batch.folder_name }}</span>
              <span class="project-degree">{{ batch.file_count }}</span>
            </button>
          </div>
          
          <div class="kg-sidebar-footer">
            <span v-if="stats" class="kg-stat-badge">
              {{ $t('kg.stats_summary', { nodes: stats.node_count, edges: stats.edge_count }) }}
            </span>
            <button class="kg-add-btn" @click="openFolderPicker" :title="$t('kg.add_folder') || '添加知识库'">
              <Plus :size="16" />
            </button>
          </div>
        </div>

        <!-- 笔记侧边栏内容 -->
        <div v-show="currentMode === 'notebook'" class="kg-sidebar-content notebook-sidebar-content">
          <NoteExplorer 
            v-if="!isMobile"
            ref="noteExplorerRef"
            :selectedNoteId="selectedNoteId"
            @select="handleNoteSelect"
          />
        </div>
      </div>
    </Teleport>

    <!-- 右侧主体区域 -->
    <main class="kg-main-content">
      <!-- 主体：图谱画布 + Inspector -->
      <div v-show="currentMode === 'graph'" class="kg-body">
        
        <!-- Search Overlay -->
        <div class="kg-overlay-search" :class="{ expanded: kgSearchExpanded }">
          <div class="kg-search-box">
            <button v-show="!kgSearchExpanded" class="btn-icon" @click="expandSearch" title="搜索">
              <Search :size="16" />
            </button>
            <div v-show="kgSearchExpanded" style="display: flex; align-items: center; gap: 8px; width: 100%;">
              <Search :size="14" style="color: var(--text-muted); flex-shrink: 0;" />
              <input
                v-model="searchTerm"
                :placeholder="$t('kg.search_placeholder') || '节点...'"
                @keyup.enter="doSearch"
                @blur="kgSearchExpanded = false"
                @keydown.esc="kgSearchExpanded = false"
                ref="searchInputRef"
              />
            </div>
          </div>
        </div>

        <!-- Legend Overlay -->
        <div class="kg-overlay-legend">
          <div class="kg-type-filters">
            <button
              v-for="t in typeFilters"
              :key="t.type"
              class="kg-filter-chip"
              :class="{ active: activeTypes.has(t.type) }"
              @click="toggleType(t.type)"
            >
              <span style="display: flex; align-items: center; gap: 6px;">
                <span class="chip-shape" :style="{ color: kgColors[t.type] || 'var(--text-muted)' }">
                  {{ getTypeShapeIcon(t.type) }}
                </span>
                <span>{{ getTypeName(t.type) }}</span>
              </span>
              <span style="font-variant-numeric: tabular-nums; opacity: 0.8;">{{ t.count }}</span>
            </button>
          </div>
        </div>

        <!-- vis.js 画布 + 拖拽覆盖层 -->
        <div
        ref="networkContainer"
        class="kg-canvas"
        @dragover.prevent="onDragOver"
        @dragleave="onDragLeave"
        @drop.prevent="onDrop"
        :class="{ 'drag-over': isDragOver }"
      ></div>

      <!-- Inspector 面板 -->
      <div 
        v-show="selectedNode" 
        class="inspector-resizer" 
        @mousedown="startResizeInspector"
        :style="{ right: inspectorWidth + 'px' }"
      ></div>
      <aside v-if="selectedNode" class="kg-inspector" :style="{ width: inspectorWidth + 'px' }">
        <div class="inspector-header">
          <span class="inspector-type-badge" :style="{ background: kgColors[selectedNode.type] || 'var(--text-muted)' }">
            {{ getTypeShapeIcon(selectedNode.type) }} {{ getTypeName(selectedNode.type) }}
          </span>
          <div style="display:flex; gap: 4px;">
            <button class="btn-icon inspector-merge" :class="{ active: mergeMode }" @click="toggleMergeMode" title="关联/合并至...">
              <Link :size="14" />
            </button>
            <button class="btn-icon inspector-close" @click="selectedNode = null">
              <X :size="14" />
            </button>
          </div>
        </div>
        <h3 class="inspector-title">{{ selectedNode.label }}</h3>
        <p v-if="selectedNode.summary" class="inspector-summary">{{ selectedNode.summary }}</p>
        <div v-if="selectedNode.source" style="display: flex; align-items: flex-start; justify-content: space-between; gap: 8px; margin-bottom: var(--space-4);">
          <p class="inspector-source" style="margin: 0; flex: 1;">
            <FileText :size="12" style="flex-shrink: 0; margin-top: 2px;" />
            <span :title="selectedNode.source">{{ selectedNode.source }}</span>
          </p>
          <button class="btn btn-ghost" style="padding: 2px 6px; font-size: 0.8em; flex-shrink: 0; display: flex; align-items: center; gap: 4px;" @click="openSourceFile(selectedNode.source)" title="打开原始文件">
            <ExternalLink :size="12" /> 打开
          </button>
        </div>

        <div v-if="projectIndexPath" style="margin-bottom: var(--space-4);">
          <button class="btn" style="width: 100%; display: flex; justify-content: center; align-items: center; gap: 6px; background-color: var(--user-accent); color: var(--text-inverse); border: none;" @click="openProjectPortal">
            <ExternalLink :size="14" /> 进入项目协作门户
          </button>
        </div>

        <div v-if="selectedNode.type === 'source'" style="margin-bottom: var(--space-4);">
          <button class="btn btn-danger" style="width: 100%; display: flex; justify-content: center; align-items: center; gap: 6px;" @click="removeSourceBatch(selectedNode)">
            <Trash2 :size="14" /> 彻底清除该来源批次
          </button>
        </div>

        <!-- 合并操作区 -->
        <div v-if="mergeMode" class="inspector-merge-panel">
          <div class="merge-prompt">
            <strong>关联/合并节点</strong>
            <p>将当前节点(被删除)合并至目标节点。</p>
            <p>请在下方选择，或在图谱上点击目标：</p>
          </div>
          <div class="merge-filters">
            <input type="text" v-model="mergeSearchTerm" placeholder="搜索目标节点..." class="merge-search-input" />
            <label class="merge-checkbox">
              <input type="checkbox" v-model="mergeFilterSameType" />
              仅同类型 ({{ getTypeName(selectedNode.type) }})
            </label>
          </div>
          
          <div class="merge-list">
            <div 
              v-for="node in filteredMergeNodes" 
              :key="node.id"
              class="merge-list-item"
              :class="{ selected: mergeTargetId === node.id }"
              @click="mergeTargetId = node.id"
            >
              <span class="merge-item-icon" :style="{ color: kgColors[node.type] || 'var(--text-muted)' }">
                {{ getTypeShapeIcon(node.type) }}
              </span>
              <span class="merge-item-label">{{ node.label }}</span>
            </div>
            <div v-if="filteredMergeNodes.length === 0" class="merge-list-empty">无匹配节点</div>
          </div>
          <div class="merge-actions">
            <button class="btn-primary" :disabled="!mergeTargetId" @click="confirmMerge">确认合并</button>
            <button class="btn-secondary" @click="mergeMode = false">取消</button>
          </div>
        </div>

        <div class="inspector-section">
          <h4>关联节点</h4>
          <div class="relation-item" v-for="rel in selectedRelations" :key="rel.id" @click="focusNode(rel.id)">
            <div class="relation-icon" :style="{ color: kgColors[rel.type] || 'var(--text-muted)' }">
              {{ getTypeShapeIcon(rel.type) }}
            </div>
            <div class="relation-info">
              <span class="relation-label">{{ rel.label }}</span>
              <span class="relation-type">{{ rel.relation }}</span>
            </div>
          </div>
          <div v-if="selectedRelations.length === 0" class="inspector-empty">
            暂无关联
          </div>
        </div>

        <!-- P1-4: 相关笔记 -->
        <div class="inspector-section" v-if="inspectorRelatedNotes.length > 0">
          <h4>📓 {{ $t('kg.notebook_view') }}</h4>
          <div class="relation-item" v-for="note in inspectorRelatedNotes" :key="note.path"
               @click="openRelatedNote(note.path)">
            <div class="relation-icon" style="color: var(--user-accent);">📄</div>
            <div class="relation-info">
              <span class="relation-label">{{ note.title || note.path }}</span>
              <span class="relation-type" v-if="note.snippet">{{ note.snippet }}</span>
            </div>
          </div>
        </div>
      </aside>
    </div>


    <!-- 笔记工作台 -->
    <div v-if="currentMode === 'notebook'" class="notebook-body">
      <!-- 移动端侧边栏 (抽屉+工具栏) -->
      <NoteExplorer 
        v-if="isMobile"
        ref="noteExplorerRefMobile"
        :selectedNoteId="selectedNoteId"
        @select="handleNoteSelect"
      />
      
      <div class="notebook-editor-area">
        <div v-if="!selectedNoteId" class="notebook-empty-state">
          {{ isMobile ? '请在菜单中选择或新建一篇笔记' : '请在左侧选择或新建一篇笔记' }}
        </div>
        <template v-else>
          <div v-if="isLoadingNote" class="notebook-empty-state">
            <RefreshCw :size="32" class="animate-spin" style="opacity: 0.4; margin-bottom: 12px;" />
            <p style="color: var(--text-secondary); font-size: 14px;">正在全速解析，请稍候...</p>
          </div>
          <TiptapEditor 
            v-show="!isLoadingNote"
            v-model="currentNoteContent"
            :saveStatus="saveStatus"
            :tags="currentNoteTags"
            @save="saveCurrentNote"
            @update:tags="handleTagsUpdate"
            @wikilink-click="handleWikilinkClick"
          />
        </template>

        <!-- P2-2: 反向链接面板 -->
        <div v-if="selectedNoteId && backlinks.length > 0" class="backlinks-panel">
          <div class="backlinks-header" @click="showBacklinks = !showBacklinks">
            <ChevronRight :size="14" class="caret" :class="{ open: showBacklinks }" />
            🔗 {{ t('notebook.tags') === 'Tags' ? 'Backlinks' : '反向链接' }} ({{ backlinks.length }})
          </div>
          <div v-show="showBacklinks" class="backlinks-list">
            <div v-for="bl in backlinks" :key="bl.path" class="backlink-item" @click="handleNoteSelect(bl.path)">
              <span class="backlink-title">{{ bl.title }}</span>
              <span class="backlink-context" v-if="bl.context">{{ bl.context }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>

    <div v-if="currentMode === 'ticket'" class="ticket-body" style="flex:1; overflow-y:auto; padding:24px; background-color: var(--bg-primary);">
      <div v-if="ticketNodes.length === 0" class="notebook-empty-state">
        {{ $t('ticket.empty') || '票夹为空' }}
      </div>
      <div v-else style="display:grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 16px; align-items: stretch;">
        <TicketCard v-for="node in ticketNodes" :key="node.id" :node="node" />
      </div>
    </div>

    <!-- 图谱空状态 / 生成中 -->
    <div v-if="currentMode === 'graph' && !loading && stats && stats.node_count === 0 && !backfilling" class="kg-empty">
      <Waypoints :size="48" style="opacity: 0.2;" />
      <p>{{ $t('kg.empty') }}</p>
      <p class="kg-empty-hint">{{ $t('kg.empty_hint') }}</p>
    </div>
    <div v-if="currentMode === 'graph' && backfilling" class="kg-empty">
      <RefreshCw :size="32" class="animate-spin" style="opacity: 0.4;" />
      <p>{{ $t('kg.generating') }}</p>
    </div>
    </main>
  </div>
</template>

<script setup>
import { ref, onMounted, onBeforeUnmount, watch, computed, inject } from 'vue';
import NoteExplorer from '../components/NoteExplorer.vue';
import TicketCard from '../components/TicketCard.vue';
import TiptapEditor from '../components/TiptapEditor.vue';
import { useI18n } from 'vue-i18n';
import { useDialog } from '../composables/useDialog';
import { Network } from 'vis-network';
import { DataSet } from 'vis-data';
import { Waypoints, Search, X, FileText, RefreshCw, Plus, Link, ExternalLink, Trash2, ChevronRight, Menu, ChevronDown, Star, Package, Ticket } from 'lucide-vue-next';

const emit = defineEmits(['toggle-sidebar']);

const isMobile = inject('isMobile');
const showMobileMenu = ref(false);

// ── 笔记模式逻辑 ─────────────────────────────────────
const isMounted = ref(false);

onMounted(() => {
  isMounted.value = true;
});

let saveTimeout = null;
let pendingSaveFn = null;

async function handleNoteSelect(id) {
  // Flush any pending save for the previous note synchronously
  if (saveTimeout && pendingSaveFn) {
    clearTimeout(saveTimeout);
    saveTimeout = null;
    await pendingSaveFn();
  }

  selectedNoteId.value = id;
  if (!id) {
    currentNoteContent.value = '';
    currentNoteTags.value = [];
    backlinks.value = [];
    currentNoteFrontmatter = null;
    return;
  }
  
  try {
    isLoadingNote.value = true;
    currentNoteContent.value = '';
    
    // Let Vue and browser update the UI (highlight the clicked note and show loader immediately)
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    
    // Abort if user clicked another note while yielding
    if (selectedNoteId.value !== id) {
      if (selectedNoteId.value === id) isLoadingNote.value = false; // only reset if we are the last one, wait, actually let the new click handle it
      return;
    }

    const res = await window.appAPI.notebookReadNote(id);
    
    // Abort if user clicked another note while reading from disk
    if (selectedNoteId.value !== id) return;

    if (res.ok) {
      currentNoteContent.value = res.content;
      currentNoteFrontmatter = res.frontmatter;
      currentNoteTags.value = res.frontmatter?.tags || [];
      // P2-2: Load backlinks asynchronously
      loadBacklinks(id);
      // Yield to browser one more time so Vue updates Tiptap editor props and Tiptap parses it
      // BEFORE we remove the loading spinner. This ensures the spinner stays ON while Tiptap parses!
      await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    } else {
      console.error("Read note failed:", res.error);
    }
  } catch (e) {
    console.error("Read note error:", e);
  } finally {
    if (selectedNoteId.value === id) {
      isLoadingNote.value = false;
    }
  }
}

async function saveCurrentNote(markdown) {
  if (!selectedNoteId.value) return;
  const id = selectedNoteId.value;
  
  if (saveTimeout) clearTimeout(saveTimeout);
  saveStatus.value = '保存中...';

  pendingSaveFn = async () => {
    try {
      await window.appAPI.notebookSaveNote(id, markdown);
      saveStatus.value = '已自动保存';
      setTimeout(() => {
        if (saveStatus.value === '已自动保存') saveStatus.value = '';
      }, 3000);
    } catch (e) {
      console.error("Save note failed:", e);
      saveStatus.value = '保存出错';
    }
  };

  saveTimeout = setTimeout(async () => {
    if (pendingSaveFn) {
      await pendingSaveFn();
      pendingSaveFn = null;
    }
  }, 1000);
}

async function handleTagsUpdate(newTags) {
  if (!selectedNoteId.value) return;
  currentNoteTags.value = newTags;
  try {
    await window.appAPI.notebookUpdateTags(selectedNoteId.value, newTags);
  } catch (e) {
    console.error('Failed to update tags:', e);
  }
}

// P2-2: Backlinks
const backlinks = ref([]);
const showBacklinks = ref(true);

async function loadBacklinks(notePath) {
  try {
    const res = await window.appAPI.notebookGetBacklinks(notePath);
    if (res && res.ok) {
      backlinks.value = res.backlinks || [];
    }
  } catch (e) {
    console.error('Failed to load backlinks:', e);
    backlinks.value = [];
  }
}

// P2-1: Wikilink click → navigate to target note
async function handleWikilinkClick(targetTitle) {
  // Search for the note by title
  try {
    const res = await window.appAPI.notebookSearch(targetTitle);
    if (res && res.ok && res.results && res.results.length > 0) {
      // Found existing note — navigate to it
      handleNoteSelect(res.results[0].path);
    } else {
      // Note doesn't exist — create it
      const createRes = await window.appAPI.notebookCreateNote(targetTitle, []);
      if (createRes.ok) {
        if (noteExplorerRef.value) noteExplorerRef.value.refresh();
        handleNoteSelect(createRes.path);
      }
    }
  } catch (e) {
    console.error('Wikilink navigate failed:', e);
  }
}

// ── 状态 ────────────────────────────────────────────────
const { t, te } = useI18n();

const currentMode = ref('graph'); // 'graph' or 'notebook'
const noteExplorerRef = ref(null);
const selectedNoteId = ref(null);
const currentNoteContent = ref('');
const currentNoteTags = ref([]);
const isLoadingNote = ref(false);
const saveStatus = ref('');
let currentNoteFrontmatter = null;

const networkContainer = ref(null);
const stats = ref(null);
const searchTerm = ref('');
const kgSearchExpanded = ref(false);
const searchInputRef = ref(null);
function expandSearch() {
  kgSearchExpanded.value = true;
  setTimeout(() => {
    if (searchInputRef.value) searchInputRef.value.focus();
  }, 150); // slight delay to allow transition to start/finish
}
const selectedNode = ref(null);
const selectedRelations = ref([]);
const projectIndexPath = ref(null);
const loading = ref(true);
const activeTypes = ref(new Set());
const backfilling = ref(false);
const isDragOver = ref(false);

const inspectorRelatedNotes = ref([]);

watch(() => selectedNode.value, async (newVal) => {
  projectIndexPath.value = null;
  inspectorRelatedNotes.value = [];
  if (!newVal) return;

  // Check project index
  if (newVal.type === 'Project' || newVal.type === 'project') {
    try {
      const path = await window.appAPI.checkProjectIndex(newVal.label);
      if (path) projectIndexPath.value = path;
    } catch (e) { console.error(e); }
  }

  // P1-4: Search for related notes by node label
  try {
    const res = await window.appAPI.notebookSearch(newVal.label);
    if (res && res.ok && res.results) {
      inspectorRelatedNotes.value = res.results.slice(0, 5);
    }
  } catch (e) { console.error('Related notes search failed:', e); }
});

function openRelatedNote(notePath) {
  currentMode.value = 'notebook';
  handleNoteSelect(notePath);
}

const mergeMode = ref(false);
const mergeTargetId = ref('');
const mergeSearchTerm = ref('');
const mergeFilterSameType = ref(true);

const inspectorWidth = ref(280);
const isResizingInspector = ref(false);

let network = null;
let nodesDataSet = null;
let edgesDataSet = null;
import { shallowRef } from 'vue';
const allGraphData = shallowRef(null);

const allNodesList = computed(() => {
  const _trigger = stats.value;
  if (!allGraphData.value) return [];
  return allGraphData.value.nodes
    .filter(n => n.id !== selectedNode.value?.id)
    .sort((a,b) => a.label.localeCompare(b.label));
});

const ticketNodes = computed(() => {
  const _trigger = stats.value;
  if (!allGraphData.value || !allGraphData.value.nodes) return [];
  return allGraphData.value.nodes
    .filter(n => n.node_type === 'ticket' || n.type === 'ticket' || n.type === 'Ticket')
    .sort((a,b) => {
      let aMeta = {};
      let bMeta = {};
      try { aMeta = typeof a.metadata === 'string' && a.metadata ? JSON.parse(a.metadata) : (a.metadata || {}); } catch(e) {}
      try { bMeta = typeof b.metadata === 'string' && b.metadata ? JSON.parse(b.metadata) : (b.metadata || {}); } catch(e) {}
      
      const now = new Date();
      const dateA = aMeta.start_time ? new Date(aMeta.start_time) : null;
      const dateB = bMeta.start_time ? new Date(bMeta.start_time) : null;
      
      if (!dateA && !dateB) return 0;
      if (!dateA) return 1;
      if (!dateB) return -1;
      
      const isAExpired = dateA < now;
      const isBExpired = dateB < now;
      
      if (!isAExpired && isBExpired) return -1;
      if (isAExpired && !isBExpired) return 1;
      
      if (!isAExpired && !isBExpired) {
        return dateA - dateB; // Upcoming: closer to today first (ascending)
      }
      
      return dateB - dateA; // Expired: closer to today first (descending)
    });
});

const topProjects = computed(() => {
  const _trigger = stats.value; // Force reactivity since allGraphData is not a ref
  if (!allGraphData.value || !allGraphData.value.nodes) return [];
  const projectNodes = allGraphData.value.nodes.filter(n => n.type === 'Project' || n.type === 'project');
  const degreeMap = new Map();
  if (allGraphData.value.edges) {
    allGraphData.value.edges.forEach(edge => {
      degreeMap.set(edge.source, (degreeMap.get(edge.source) || 0) + 1);
      degreeMap.set(edge.target, (degreeMap.get(edge.target) || 0) + 1);
    });
  }
  return projectNodes
    .map(n => ({ ...n, degree: degreeMap.get(n.id) || 0 }))
    .sort((a, b) => b.degree - a.degree)
    .slice(0, 10); // Display top 10 projects
});

const filteredMergeNodes = computed(() => {
  let list = allNodesList.value;
  if (mergeFilterSameType.value && selectedNode.value) {
    list = list.filter(n => n.type === selectedNode.value.type);
  }
  if (mergeSearchTerm.value) {
    const term = mergeSearchTerm.value.toLowerCase();
    list = list.filter(n => n.label.toLowerCase().includes(term));
  }
  return list;
});

function toggleMergeMode() {
  mergeMode.value = !mergeMode.value;
  mergeTargetId.value = '';
  mergeSearchTerm.value = '';
  mergeFilterSameType.value = true;
}

// ── Inspector Resizing ──
function startResizeInspector(e) {
  isResizingInspector.value = true;
  document.addEventListener('mousemove', handleResizeInspector);
  document.addEventListener('mouseup', stopResizeInspector);
  document.body.style.cursor = 'col-resize';
  document.body.style.userSelect = 'none';
}

function handleResizeInspector(e) {
  if (!isResizingInspector.value) return;
  // Panel is on the right, so width is (screen width - mouse X)
  let newWidth = window.innerWidth - e.clientX;
  if (newWidth < 280) newWidth = 280;
  if (newWidth > 800) newWidth = 800;
  inspectorWidth.value = newWidth;
}

function stopResizeInspector() {
  isResizingInspector.value = false;
  document.removeEventListener('mousemove', handleResizeInspector);
  document.removeEventListener('mouseup', stopResizeInspector);
  document.body.style.cursor = '';
  document.body.style.userSelect = '';
}

async function confirmMerge() {
  if (!mergeTargetId.value || !selectedNode.value) return;
  const targetNode = allNodesList.value.find(n => n.id === mergeTargetId.value);
  if (!targetNode) return;
  
  const yes = window.confirm(`确定要将【${selectedNode.value.label}】合并至【${targetNode.label}】吗？\n\n合并后，当前节点将被删除，其所有关联关系将转移到目标节点上。`);
  if (!yes) return;
  
  try {
    const res = await window.appAPI.invoke('kg_merge_nodes', {
      payload: {
        primary_id: targetNode.id,
        alias_id: selectedNode.value.id
      }
    });
    if (!res.ok) throw new Error(res.error || 'Unknown error');
    
    mergeMode.value = false;
    selectedNode.value = null;
    await loadGraph();
  } catch (e) {
    alert("合并失败: " + e);
  }
}

// ── 颜色与形状定义 ─────────────────────────────────────
const kgColors = ref({});

function updateKgColors() {
  const s = getComputedStyle(document.documentElement);
  const get = (v) => s.getPropertyValue(v).trim();
  kgColors.value = {
    tag: get('--kg-node-tag') || '#0891b2',
    project: get('--kg-node-project') || '#16a34a',
    file: get('--kg-node-file') || '#64748b',
    concept: get('--kg-node-concept') || '#4f46e5',
    person: get('--kg-node-person') || '#d97706',
    topic: get('--kg-node-topic') || '#9333ea',
    note: get('--kg-node-note') || '#eab308',
    edge: get('--kg-edge') || 'rgba(100, 116, 139, 0.25)',
    edgeHl: get('--kg-edge-highlight') || get('--accent-primary') || '#6366f1',
    font: get('--kg-font') || '#64748b',
  };
}

const typeFilters = computed(() => {
  if (!stats.value?.type_distribution) return [];
  return stats.value.type_distribution;
});

const typeShapes = {
  concept: { vis: 'dot', icon: '●' },
  project: { vis: 'star', icon: '★' },
  file: { vis: 'square', icon: '■' },
  tag: { vis: 'diamond', icon: '◆' },
  person: { vis: 'triangleDown', icon: '▼' },
  topic: { vis: 'hexagon', icon: '⬢' },
  note: { vis: 'box', icon: '▤' },
};

function getTypeShapeIcon(type) {
  return typeShapes[type]?.icon || '●';
}

function getTypeVisShape(type) {
  return typeShapes[type]?.vis || 'dot';
}

function getTypeName(type) {
  const key = `kg.type_${type}`;
  return te(key) ? t(key) : type;
}

function buildNetworkOptions() {
  const colors = kgColors.value;
  return {
    nodes: {
      size: 14,
      font: { size: 11, color: colors.font, face: 'Inter, system-ui, sans-serif' },
      borderWidth: 1.5,
      shadow: false,
    },
    edges: {
      width: 0.6,
      color: { color: colors.edge, highlight: colors.edgeHl, hover: colors.edgeHl, opacity: 1.0 },
      font: { size: 0, color: 'transparent' },
      arrows: { to: { enabled: true, scaleFactor: 0.35 } },
      smooth: { enabled: true, type: 'continuous', roundness: 0.5 },
      hoverWidth: 0.3,
    },
    physics: {
      solver: 'barnesHut',
      barnesHut: { 
        gravitationalConstant: -6000,  // 稍微减少排斥力使图谱更紧凑 (之前是-12000，默认-2000)
        centralGravity: 0.08,          // 稍微增加向心力
        springLength: 150,             // 缩短连线基本长度 (之前是250，默认100)
        springConstant: 0.05,          // 稍微增强连线拉力
        damping: 0.2 
      },
      stabilization: { enabled: true, iterations: 80, fit: true },
      maxVelocity: 50,
      minVelocity: 0.75,
      timestep: 0.5,
    },
    interaction: {
      hover: true,
      tooltipDelay: 300,
      zoomView: true,
      dragView: true,
      zoomSpeed: 1.0, // 加大缩放速度，去除迟滞感
      multiselect: false,
    },
  };
}

let tauriDragUnlistens = [];

function openSourceFile(path) {
  if (!path) return;
  if (window.appAPI && window.appAPI.openFile) {
    window.appAPI.openFile(path).catch(err => {
      console.error('Failed to open file:', err);
    });
  } else {
    alert(`无法直接打开文件: ${path}`);
  }
}

function openProjectPortal() {
  if (projectIndexPath.value) {
    window.appAPI.openFile(projectIndexPath.value);
  }
}

// ── 初始化 ──────────────────────────────────────────────
function onAndroidBackPressed(e) {
  if (showMobileMenu.value) {
    showMobileMenu.value = false;
    e.preventDefault();
  } else if (mergeMode.value) {
    mergeMode.value = false;
    e.preventDefault();
  } else if (selectedNode.value) {
    selectedNode.value = null;
    e.preventDefault();
  } else if (currentMode.value === 'notebook' && selectedNoteId.value) {
    selectedNoteId.value = null;
    e.preventDefault();
  }
}

function resizeNetwork() {
  if (network) {
    network.redraw();
    network.fit();
  }
}

onMounted(async () => {
  window.addEventListener('resize', resizeNetwork);
  
  window.addEventListener('open-ticket-view', (e) => {
    currentMode.value = 'ticket';
    if (e.detail) {
      setTimeout(() => {
        window.dispatchEvent(new CustomEvent('ticket-card-open', { detail: e.detail }));
      }, 100);
    }
  });

  // Auto-refresh ticket list when a new ticket is created from chat
  window.addEventListener('ticket-created', async () => {
    try {
      const [graphData, statsData] = await Promise.all([
        window.appAPI.kgGetFullGraph(),
        window.appAPI.kgStats(),
      ]);
      stats.value = statsData;
      allGraphData.value = graphData;
    } catch (e) {
      console.warn('[KG] ticket-created refresh failed:', e);
    }
  });

  window.addEventListener('android-back-pressed', onAndroidBackPressed);
  updateKgColors();
  await loadGraph();
  loading.value = false;

  // 注册 Tauri 原生拖拽监听 (因为全局已接管 native OS drop)
  if (window.appAPI.onDragEnter) {
    window.appAPI.onDragEnter(async () => {
      isDragOver.value = true;
    }).then(u => tauriDragUnlistens.push(u));

    window.appAPI.onDragLeave(async () => {
      isDragOver.value = false;
    }).then(u => tauriDragUnlistens.push(u));

    window.appAPI.onDragDrop(async (e) => {
      isDragOver.value = false;
      const kgView = document.querySelector('.kg-view');
      if (kgView && kgView.offsetParent === null) return;
      if (e.payload && e.payload.paths && e.payload.paths.length > 0) {
        const path = e.payload.paths[0];
        let yes = false;
        try {
          // using useDialog
          
          // 先预估成本
          const estimate = await window.appAPI.estimateKB(path);
          let msg = `是否要从该路径提取知识点并加入图谱？\n\n${path}\n\n`;
          
          if (estimate && !estimate.error) {
            msg += `【扫描结果】\n`;
            msg += `- 支持的文件: ${estimate.convertable_files} 个\n`;
            msg += `- Token 规模: 约 ${estimate.estimated_tokens}\n`;
            msg += `- 预计耗时: 约 ${estimate.estimated_minutes} 分钟\n`;
            msg += `- 预估成本: 约 ¥${(estimate.estimated_cost_cheap_rmb || 0).toFixed(4)} (基础模型)\n`;
          } else if (estimate && estimate.error) {
            msg += `【预估失败】\n${estimate.error}\n`;
          }

          yes = await useDialog().showConfirm({
            title: '提取前确认 (包含成本预估)',
            message: msg
          });
        } catch (err) {
          yes = window.confirm(`是否要从该路径提取知识点并加入图谱？\n\n${path}`);
        }
        
        if (yes) {
          await buildKBAndRefresh(path);
        }
      }
    }).then(u => tauriDragUnlistens.push(u));
  }
});

onBeforeUnmount(() => {
  window.removeEventListener('android-back-pressed', onAndroidBackPressed);
  if (network) {
    network.destroy();
    network = null;
  }
  tauriDragUnlistens.forEach(u => typeof u === 'function' && u());
});

async function loadGraph() {
  try {
    // 加载统计和完整图谱
    const [graphData, statsData] = await Promise.all([
      window.appAPI.kgGetFullGraph(),
      window.appAPI.kgStats(),
    ]);
    stats.value = statsData;
    allGraphData.value = graphData;

    if (graphData.nodes?.length > 0) {
      renderNetwork(graphData);
    }
  } catch (e) {
    console.error('KG load failed:', e);
  }
}

function renderNetwork(data) {
  if (!networkContainer.value) return;

  nodesDataSet = new DataSet(data.nodes.map(n => {
    const color = kgColors.value[n.type] || '#64748b';
    return {
      id: n.id,
      label: n.label,
      shape: getTypeVisShape(n.type),
      color: {
        background: color,
        border: color,
        highlight: { background: color, border: 'var(--text-inverse)' },
      },
      title: `${n.label} (${n.type})${n.summary ? '\n' + n.summary : ''}`,
      opacity: 1.0,
      hidden: false,
      _raw: n,
    };
  }));

  edgesDataSet = new DataSet(data.edges.map((e, i) => ({
    id: `e${i}`,
    from: e.source,
    to: e.target,
    label: e.relation,
    hidden: false,
    _raw: e,
  })));

  network = new Network(networkContainer.value, { nodes: nodesDataSet, edges: edgesDataSet }, buildNetworkOptions());

  // 点击节点 → 显示 Inspector，并高亮邻居
  network.on('selectNode', (params) => {
    if (params.nodes.length > 0) {
      const nodeId = params.nodes[0];
      
      if (mergeMode.value) {
        if (nodeId !== selectedNode.value?.id) {
          mergeTargetId.value = nodeId;
          // 恢复之前选中的主节点视觉效果
          network.setSelection({ nodes: [selectedNode.value.id, nodeId] });
        }
        return;
      }

      const node = nodesDataSet.get(nodeId);
      if (node?._raw) {
        selectedNode.value = node._raw;
        loadRelations(nodeId);
        focusNeighbors(nodeId);
      }
    }
  });

  network.on('deselectNode', () => {
    if (mergeMode.value) {
      // 如果在合并模式下取消选择，保留主节点的选择状态
      if (selectedNode.value) {
        network.setSelection({ nodes: [selectedNode.value.id] });
      }
      return;
    }
    
    selectedNode.value = null;
    selectedRelations.value = [];
    resetFocus();
  });

  // CAD-like 缩放：保持连线视觉粗细一致
  network.on('zoom', () => {
    updateEdgeWidths();
  });

  // 初始时调整一次
  setTimeout(() => updateEdgeWidths(), 500);
}

function updateEdgeWidths() {
  if (!network || !edgesDataSet) return;
  const scale = network.getScale();
  // 当画布缩小 (scale < 1) 时，增加基础线宽；当画布放大 (scale > 1) 时，减小基础线宽
  const normalWidth = Math.max(0.6 / scale, 0.1);
  const highlightWidth = Math.max(1.5 / scale, 0.2);
  const dimWidth = Math.max(0.3 / scale, 0.05);

  // 更新全局配置
  network.setOptions({
    edges: {
      width: normalWidth,
      hoverWidth: normalWidth * 0.5,
    }
  });

  // 如果有选中的节点，或者之前有独立设置过线宽，需要同步更新
  const edgeUpdates = [];
  edgesDataSet.forEach(edge => {
    if (selectedNode.value) {
      edgeUpdates.push({
        id: edge.id,
        width: edge._isRelevant ? highlightWidth : dimWidth
      });
    } else {
      // 没选中节点时，如果曾经被高亮/淡化过，重置回基础缩放宽度
      if (edge.width !== undefined && Math.abs(edge.width - normalWidth) > 0.001) {
         edgeUpdates.push({
           id: edge.id,
           width: normalWidth
         });
      }
    }
  });
  if (edgeUpdates.length > 0) {
    edgesDataSet.update(edgeUpdates);
  }
}

function focusNeighbors(nodeId) {
  if (!network) return;
  const neighborIds = new Set(network.getConnectedNodes(nodeId));
  neighborIds.add(nodeId);

  const nodeUpdates = [];
  nodesDataSet.forEach(node => {
    const isRelevant = neighborIds.has(node.id);
    nodeUpdates.push({ id: node.id, opacity: isRelevant ? 1.0 : 0.2 });
  });
  nodesDataSet.update(nodeUpdates);

  const scale = network.getScale();
  const highlightWidth = Math.max(1.5 / scale, 0.2);
  const dimWidth = Math.max(0.3 / scale, 0.05);

  const edgeUpdates = [];
  const edgeHl = kgColors.value.edgeHl;
  const edgeBase = kgColors.value.edge;
  edgesDataSet.forEach(edge => {
    const isRelevant = edge.from === nodeId || edge.to === nodeId;
    edgeUpdates.push({
      id: edge.id,
      _isRelevant: isRelevant,
      color: { color: isRelevant ? edgeHl : edgeBase, opacity: isRelevant ? 1.0 : 0.08 },
      width: isRelevant ? highlightWidth : dimWidth,
    });
  });
  edgesDataSet.update(edgeUpdates);
}

function resetFocus() {
  const nodeUpdates = [];
  nodesDataSet.forEach(node => {
    nodeUpdates.push({ id: node.id, opacity: 1.0 });
  });
  nodesDataSet.update(nodeUpdates);

  const scale = network.getScale();
  const normalWidth = Math.max(0.6 / scale, 0.1);

  const edgeUpdates = [];
  const edgeBase = kgColors.value.edge;
  edgesDataSet.forEach(edge => {
    edgeUpdates.push({
      id: edge.id,
      _isRelevant: false,
      color: { color: edgeBase, opacity: 1.0 },
      width: normalWidth,
    });
  });
  edgesDataSet.update(edgeUpdates);
}

function loadRelations(nodeId) {
  if (!allGraphData.value) return;
  const rels = [];
  for (const e of allGraphData.value.edges) {
    const neighborId = e.source === nodeId ? e.target : e.target === nodeId ? e.source : null;
    if (!neighborId) continue;
    const neighborNode = allGraphData.value.nodes.find(n => n.id === neighborId);
    if (neighborNode) {
      rels.push({
        id: neighborNode.id,
        label: neighborNode.label,
        type: neighborNode.type,
        relation: e.relation,
      });
    }
  }
  selectedRelations.value = rels;
}

// ── 交互 ────────────────────────────────────────────────
function doSearch() {
  if (!searchTerm.value.trim() || !network || !nodesDataSet) return;
  const term = searchTerm.value.toLowerCase();
  const matchIds = nodesDataSet.getIds({
    filter: (item) => item.label?.toLowerCase().includes(term),
  });
  if (matchIds.length > 0) {
    focusNode(matchIds[0]);
  }
}

function focusNode(nodeId) {
  if (!network) return;
  network.selectNodes([nodeId]);
  
  // 考虑到 Inspector 使用了 absolute 定位会遮挡右侧，相机往左偏移 inspector 宽度的一半
  network.focus(nodeId, { 
    scale: 1.2, 
    offset: { x: -(inspectorWidth.value / 2), y: 0 },
    animation: true 
  });
  
  const node = nodesDataSet.get(nodeId);
  if (node?._raw) {
    selectedNode.value = node._raw;
    loadRelations(nodeId);
    focusNeighbors(nodeId);
  }
}

function toggleType(type) {
  const s = new Set(activeTypes.value);
  if (s.has(type)) s.delete(type); else s.add(type);
  activeTypes.value = s;
  applyTypeFilter();
}

function applyTypeFilter() {
  if (!nodesDataSet || !edgesDataSet) return;

  const showAll = activeTypes.value.size === 0;
  
  const nodeUpdates = [];
  nodesDataSet.forEach(node => {
    const shouldHide = !showAll && !activeTypes.value.has(node._raw.type);
    if (node.hidden !== shouldHide) {
      nodeUpdates.push({ id: node.id, hidden: shouldHide });
    }
  });
  if (nodeUpdates.length > 0) nodesDataSet.update(nodeUpdates);

  const edgeUpdates = [];
  edgesDataSet.forEach(edge => {
    const srcHidden = nodesDataSet.get(edge.from)?.hidden;
    const tgtHidden = nodesDataSet.get(edge.to)?.hidden;
    const shouldHide = srcHidden || tgtHidden;
    if (edge.hidden !== shouldHide) {
      edgeUpdates.push({ id: edge.id, hidden: shouldHide });
    }
  });
  if (edgeUpdates.length > 0) edgesDataSet.update(edgeUpdates);
}

async function doBackfill() {
  backfilling.value = true;
  try {
    const result = await window.appAPI.kgBackfill();
    console.log('KG backfill result:', result);
    // 直接获取并渲染，不调 loadGraph (避免递归)
    const [graphData, statsData] = await Promise.all([
      window.appAPI.kgGetFullGraph(),
      window.appAPI.kgStats(),
    ]);
    stats.value = statsData;
    allGraphData.value = graphData;
    if (graphData.nodes?.length > 0) {
      renderNetwork(graphData);
    }
  } catch (e) {
    console.error('KG backfill failed:', e);
  } finally {
    backfilling.value = false;
  }
}

// ── 拖拽添加 + 文件夹选择 ────────────────────────────────
function onDragOver(e) {
  isDragOver.value = true;
}

function onDragLeave() {
  isDragOver.value = false;
}

async function confirmExtract(path) {
  try {
    const est = await window.appAPI.invoke('system_estimate_kb', { folderPath: path });
    
    let msg = `是否要从该路径提取知识点并加入图谱？\n\n路径: ${path}`;
    if (est) {
      msg += `\n\n【成本估算】`;
      msg += `\n有效文件: ${est.convertable_files} 个`;
      msg += `\n有效容量: ${(est.convertable_bytes / 1024).toFixed(2)} KB`;
      msg += `\n预估消耗: ${est.estimated_tokens} Tokens`;
      msg += `\n预估费用: 约 ￥${est.estimated_cost_core_rmb.toFixed(4)}`;
    }

    const yes = await useDialog().showConfirm({
      title: '提取前确认及成本预估',
      message: msg
    });
    
    if (yes) {
      await buildKBAndRefresh(path);
    }
  } catch (err) {
    console.error(err);
    const yes = window.confirm(`是否要从该路径提取知识点并加入图谱？\n\n${path}`);
    if (yes) {
      await buildKBAndRefresh(path);
    }
  }
}

async function onDrop(e) {
  isDragOver.value = false;
  const files = e.dataTransfer?.files;
  if (!files || files.length === 0) return;
  // 获取第一个拖入的路径
  const path = files[0].path || files[0].name;
  if (path) {
    await confirmExtract(path);
  }
}

async function openFolderPicker() {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const selected = await open({ directory: true, multiple: false, title: '选择知识库文件夹' });
    if (selected) {
      await confirmExtract(selected);
    }
  } catch (e) {
    // 降级: 使用 invoke 直接调用
    console.error('Folder picker failed:', e);
  }
}

async function buildKBAndRefresh(folderPath) {
  backfilling.value = true;
  try {
    // 调用现有的 KB 构建管线
    await window.appAPI.invoke('system_build_kb', { folderPath, plan: '' });
    // KB 构建完成后回填图谱并刷新
    await window.appAPI.kgBackfill();
    const [graphData, statsData] = await Promise.all([
      window.appAPI.kgGetFullGraph(),
      window.appAPI.kgStats(),
    ]);
    stats.value = statsData;
    allGraphData.value = graphData;
    if (graphData.nodes?.length > 0) {
      if (network) network.destroy();
      renderNetwork(graphData);
    }
  } catch (e) {
    console.error('KB build from KG view failed:', e);
  } finally {
    backfilling.value = false;
  }
}

async function removeSourceBatch(node) {
  if (!node || node.type !== 'source') return;
  // 从 source 节点取出 batch_id (通常是 node.source_batches 的内容之一，或者可以直接通过 node.id 的 source_ 前缀后获取)
  // 如果是 source 节点，它的 id 形式是 source_<batch_id>
  const batchId = node.id.replace('source_', '');
  
  if (confirm(`确定要彻底清除来源批次 "${node.label}" 及其相关联的所有知识点吗？\n警告：此操作不可逆！`)) {
    try {
      loading.value = true;
      const res = await window.appAPI.systemRemoveSource(batchId);
      if (res && res.ok) {
        console.log(`Successfully deleted ${res.nodes_deleted} nodes and ${res.edges_deleted} edges.`);
        selectedNode.value = null;
        // 刷新图谱
        const [graphData, statsData] = await Promise.all([
          window.appAPI.kgGetFullGraph(),
          window.appAPI.kgStats(),
        ]);
        stats.value = statsData;
        allGraphData.value = graphData;
        if (graphData.nodes?.length > 0) {
          if (network) network.destroy();
          renderNetwork(graphData);
        } else {
          if (network) {
            network.destroy();
            network = null;
          }
        }
      } else {
        alert(res?.message || '删除失败');
      }
    } catch (e) {
      console.error('Failed to remove source batch:', e);
      alert('删除发生错误');
    } finally {
      loading.value = false;
    }
  }
}
</script>

<style scoped>
.kg-mobile-col {
  flex-direction: column !important;
}

.kg-view {
  display: flex;
  flex-direction: row;
  height: 100%;
  background: var(--bg-primary);
  overflow: hidden;
}

/* ── 侧边栏 (Sidebar) ──────────────────────────────────── */
.kg-sidebar-wrapper {
  flex: 1;
  display: flex;
  flex-direction: column;
  background: transparent;
  height: 100%;
}

.kg-sidebar-header {
  padding: 16px 16px 12px;
  border-bottom: 1px solid var(--border-subtle);
}

.kg-sidebar-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow-y: auto;
}

.graph-sidebar {
  padding: 16px;
  gap: 16px;
}

.notebook-sidebar-content {
  padding: 0;
}

.kg-sidebar-footer {
  margin-top: auto;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px;
  border-top: 1px dashed var(--border-subtle);
}

.kg-main-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
  height: 100%;
  position: relative;
}

.kg-stat-badge {
  font-size: var(--text-xs);
  color: var(--text-muted);
  background: var(--bg-tertiary);
  padding: 2px 8px;
  border-radius: 99px;
}

.kg-overlay-search {
  position: absolute;
  top: 16px;
  left: 16px;
  z-index: 10;
}

.kg-overlay-legend {
  position: absolute;
  bottom: 16px;
  left: 16px;
  z-index: 10;
  max-width: 400px;
}

.kg-search-box {
  display: flex;
  align-items: center;
  background: var(--bg-primary);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-sm);
  padding: 6px 16px;
  width: 240px;
  height: 40px;
  box-sizing: border-box;
  box-shadow: var(--shadow-sm);
  transition: all 0.3s cubic-bezier(0.2, 0.8, 0.2, 1);
  overflow: hidden;
}

.kg-overlay-search:not(.expanded) .kg-search-box {
  width: 40px;
  padding: 0;
  justify-content: center;
}

.kg-overlay-search:not(.expanded) .btn-icon {
  width: 100%;
  height: 100%;
  padding: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius-sm);
}

.kg-search-box input {
  border: none;
  background: transparent;
  color: var(--text-primary);
  font-size: 12px;
  height: 28px;
  outline: none;
  flex: 1;
  min-width: 0;
}

.kg-type-filters {
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: var(--space-1);
}

.kg-filter-chip {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 4px 10px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-subtle);
  background: var(--bg-primary);
  color: var(--text-secondary);
  font-size: var(--text-xs);
  cursor: pointer;
  transition: all var(--duration-fast);
  box-shadow: var(--shadow-sm);
}

.kg-filter-chip:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.kg-filter-chip.active {
  background: var(--bg-secondary);
  border-color: var(--accent-primary);
  color: var(--text-primary);
}

/* Project List in Sidebar */
.kg-project-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.kg-project-list-title {
  font-size: var(--text-xs);
  color: var(--text-muted);
  text-transform: uppercase;
  margin-bottom: 8px;
  padding-left: 8px;
  font-weight: 600;
}

.kg-project-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border: none;
  background: transparent;
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.2s;
  text-align: left;
}

.kg-project-item:hover {
  background: var(--bg-hover);
}

.kg-project-item.active {
  background: var(--surface-input);
}

.project-icon {
  font-size: 14px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.project-name {
  flex: 1;
  font-size: var(--text-sm);
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.project-degree {
  font-size: 10px;
  color: var(--text-muted);
  background: var(--bg-tertiary);
  padding: 2px 6px;
  border-radius: 12px;
}

.kg-project-list-empty {
  color: var(--text-muted);
  font-size: var(--text-sm);
  padding: 16px 8px;
}

.chip-shape {
  font-size: 10px;
  line-height: 1;
  display: inline-flex;
  justify-content: center;
  align-items: center;
  width: 14px;
}

/* ── 主体 ──────────────────────────────────── */
.kg-body {
  flex: 1;
  display: flex;
  overflow: hidden;
  position: relative;
}

.kg-canvas {
  flex: 1;
  min-height: 400px;
  transition: outline var(--duration-fast);
}

.kg-canvas.drag-over {
  outline: 2px dashed var(--accent-primary);
  outline-offset: -12px;
  background: color-mix(in srgb, var(--bg-primary) 80%, transparent);
  backdrop-filter: blur(4px);
}

.kg-add-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-subtle);
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  transition: all var(--duration-fast);
  flex-shrink: 0;
}

.kg-add-btn:hover {
  background: var(--accent-primary);
  color: var(--text-inverse);
  border-color: var(--accent-primary);
}

/* ── Inspector ──────────────────────────────── */
.kg-inspector {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  z-index: 5;
  border-left: 1px solid var(--border-subtle);
  padding: var(--space-4);
  overflow-y: auto;
  background: var(--bg-secondary);
  animation: slideInRight var(--duration-normal) ease;
  box-shadow: -4px 0 24px rgba(0, 0, 0, 0.15);
}

.inspector-resizer {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 6px;
  background: transparent;
  cursor: col-resize;
  z-index: 10;
  transform: translateX(3px);
}

.inspector-resizer::after {
  content: '';
  position: absolute;
  top: 0;
  bottom: 0;
  right: 3px;
  width: 1px;
  background: transparent;
  transition: background var(--duration-fast);
}

.inspector-resizer:hover::after, .inspector-resizer:active::after {
  background: var(--accent-primary);
  width: 2px;
}

@keyframes slideInRight {
  from { transform: translateX(20px); opacity: 0; }
  to { transform: translateX(0); opacity: 1; }
}

.inspector-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: var(--space-2);
}

.inspector-merge.active {
  color: var(--user-accent);
  background: var(--bg-tertiary);
}

.inspector-merge-panel {
  background: var(--bg-tertiary);
  border-radius: var(--radius-md);
  padding: var(--space-3);
  margin-bottom: var(--space-4);
  border: 1px solid var(--border-subtle);
  animation: slideDown var(--duration-fast) ease;
}

.merge-prompt strong {
  display: block;
  font-size: var(--text-sm);
  color: var(--text-primary);
  margin-bottom: var(--space-1);
}

.merge-prompt p {
  font-size: var(--text-xs);
  color: var(--text-secondary);
  margin: 0 0 var(--space-1) 0;
  line-height: 1.4;
}

.merge-search-input {
  width: 100%;
  padding: 6px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-subtle);
  background: var(--bg-primary);
  color: var(--text-primary);
  font-size: var(--text-sm);
  outline: none;
  margin-bottom: var(--space-2);
}

.merge-checkbox {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: var(--text-xs);
  color: var(--text-secondary);
  margin-bottom: var(--space-3);
  cursor: pointer;
}

.merge-list {
  max-height: 150px;
  overflow-y: auto;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-sm);
  background: var(--bg-primary);
  margin-bottom: var(--space-3);
}

.merge-list-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 8px;
  font-size: var(--text-sm);
  color: var(--text-primary);
  cursor: pointer;
  border-bottom: 1px solid var(--border-subtle);
}

.merge-list-item:last-child {
  border-bottom: none;
}

.merge-list-item:hover {
  background: var(--bg-tertiary);
}

.merge-list-item.selected {
  background: var(--user-accent);
  color: var(--text-inverse);
}

.merge-item-icon {
  font-size: 10px;
}

.merge-list-empty {
  padding: 10px;
  text-align: center;
  font-size: var(--text-xs);
  color: var(--text-muted);
}

.merge-actions {
  display: flex;
  gap: var(--space-2);
}

.merge-actions button {
  flex: 1;
  padding: 4px 8px;
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  cursor: pointer;
  border: none;
  font-weight: 500;
}

.merge-actions .btn-primary {
  background: var(--user-accent);
  color: var(--text-inverse);
}

.merge-actions .btn-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.merge-actions .btn-secondary {
  background: var(--bg-primary);
  color: var(--text-secondary);
  border: 1px solid var(--border-subtle);
}

@keyframes slideDown {
  from { opacity: 0; transform: translateY(-10px); }
  to { opacity: 1; transform: translateY(0); }
}

.inspector-type-badge {
  font-size: var(--text-xs);
  color: var(--text-inverse);
  padding: 2px 10px;
  border-radius: 99px;
  font-weight: 500;
}

.inspector-close {
  color: var(--text-muted);
}

.inspector-title {
  font-size: var(--text-lg);
  font-weight: 600;
  color: var(--text-primary);
  margin: 0 0 var(--space-2);
}

.inspector-summary {
  font-size: var(--text-sm);
  color: var(--text-secondary);
  line-height: 1.6;
  margin-bottom: var(--space-3);
}

.inspector-source {
  font-size: var(--text-xs);
  color: var(--text-muted);
  display: flex;
  align-items: center;
  gap: 4px;
  margin-bottom: var(--space-4);
  word-break: break-all;
}

.inspector-section h4 {
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--text-primary);
  margin: 0 0 var(--space-2);
}

.relation-item {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: background var(--duration-fast);
}

.relation-item:hover {
  background: var(--bg-tertiary);
}

.relation-icon {
  font-size: 10px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 14px;
}

.relation-info {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.relation-label {
  font-size: var(--text-sm);
  color: var(--text-primary);
  flex: 1;
}

.relation-type {
  font-size: var(--text-xs);
  color: var(--text-muted);
  background: var(--bg-tertiary);
  padding: 1px 6px;
  border-radius: 4px;
}

.inspector-empty {
  font-size: var(--text-sm);
  color: var(--text-muted);
  text-align: center;
  padding: var(--space-4);
}

/* ── 空状态 ──────────────────────────────────── */
.kg-empty {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  text-align: center;
  color: var(--text-muted);
}

.kg-empty p {
  margin: var(--space-2) 0;
  font-size: var(--text-base);
}

.kg-empty-hint {
  font-size: var(--text-sm) !important;
  opacity: 0.6;
}

.kg-backfill-btn {
  margin-top: var(--space-4);
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-md);
  border: 1px solid var(--accent-primary);
  background: transparent;
  color: var(--accent-primary);
  font-size: var(--text-sm);
  cursor: pointer;
  transition: all var(--duration-fast);
}

.kg-backfill-btn:hover:not(:disabled) {
  background: var(--accent-primary);
  color: var(--text-inverse);
}

.kg-backfill-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}



.mode-toggle {
  display: flex;
  background-color: var(--bg-tertiary);
  border-radius: 6px;
  padding: 2px;
  gap: 2px;
}
.mode-toggle button {
  flex: 1;
  background: transparent;
  border: none;
  padding: 6px 12px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  border-radius: 4px;
  color: var(--text-secondary);
  cursor: pointer;
  font-weight: 500;
  transition: all 0.2s;
}
.mode-toggle button:hover {
  color: var(--text-primary);
}
.mode-toggle button.active {
  background-color: var(--bg-primary);
  color: var(--user-accent);
  box-shadow: 0 1px 3px rgba(0,0,0,0.1);
}

.notebook-body {
  flex: 1;
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.notebook-editor-area {
  flex: 1;
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 0;
  background-color: var(--bg-primary);
}

.notebook-empty-state {
  display: flex;
  align-items: center;
  justify-content: center;
  flex: 1;
  color: var(--text-muted);
  font-size: 1.1em;
}

/* ── P2-2: Backlinks Panel ── */
.backlinks-panel {
  border-top: 1px solid var(--border-subtle);
  padding: 8px 16px;
  max-height: 200px;
  overflow-y: auto;
}
.backlinks-header {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-family: var(--font-sans);
  color: var(--text-tertiary);
  cursor: pointer;
  padding: 4px 0;
  user-select: none;
}
.backlinks-header:hover { color: var(--text-secondary); }
.backlinks-header .caret {
  transition: transform 0.2s;
}
.backlinks-header .caret.open {
  transform: rotate(90deg);
}
.backlinks-list {
  margin-top: 6px;
}
.backlink-item {
  display: flex;
  flex-direction: column;
  padding: 6px 10px;
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.15s;
  margin-bottom: 2px;
}
.backlink-item:hover {
  background-color: var(--bg-tertiary);
}
.backlink-title {
  font-size: 13px;
  color: var(--user-accent);
  font-family: var(--font-sans);
}
.backlink-context {
  font-size: 11px;
  color: var(--text-tertiary);
  margin-top: 2px;
  white-space: nowrap;
  background: var(--bg-tertiary);
}

.relation-icon {
  font-size: 10px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 14px;
}

.relation-info {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.relation-label {
  font-size: var(--text-sm);
  color: var(--text-primary);
  flex: 1;
}

.relation-type {
  font-size: var(--text-xs);
  color: var(--text-muted);
  background: var(--bg-tertiary);
  padding: 1px 6px;
  border-radius: 4px;
}

.inspector-empty {
  font-size: var(--text-sm);
  color: var(--text-muted);
  text-align: center;
  padding: var(--space-4);
}

/* ── 空状态 ──────────────────────────────────── */
.kg-empty {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  text-align: center;
  color: var(--text-muted);
}

.kg-empty p {
  margin: var(--space-2) 0;
  font-size: var(--text-base);
}

.kg-empty-hint {
  font-size: var(--text-sm) !important;
  opacity: 0.6;
}

.kg-backfill-btn {
  margin-top: var(--space-4);
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-md);
  border: 1px solid var(--accent-primary);
  background: transparent;
  color: var(--accent-primary);
  font-size: var(--text-sm);
  cursor: pointer;
  transition: all var(--duration-fast);
}

.kg-backfill-btn:hover:not(:disabled) {
  background: var(--accent-primary);
  color: var(--text-inverse);
}

.kg-backfill-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}



.mode-toggle {
  display: flex;
  background-color: var(--bg-tertiary);
  border-radius: 6px;
  padding: 2px;
  gap: 2px;
}
.mode-toggle button {
  flex: 1;
  background: transparent;
  border: none;
  padding: 6px 12px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  border-radius: 4px;
  color: var(--text-secondary);
  cursor: pointer;
  font-weight: 500;
  transition: all 0.2s;
}
.mode-toggle button:hover {
  color: var(--text-primary);
}
.mode-toggle button.active {
  background-color: var(--bg-primary);
  color: var(--user-accent);
  box-shadow: 0 1px 3px rgba(0,0,0,0.1);
}

.notebook-body {
  flex: 1;
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.notebook-editor-area {
  flex: 1;
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 0;
  background-color: var(--bg-primary);
}

.notebook-empty-state {
  display: flex;
  align-items: center;
  justify-content: center;
  flex: 1;
  color: var(--text-muted);
  font-size: 1.1em;
}

/* ── P2-2: Backlinks Panel ── */
.backlinks-panel {
  border-top: 1px solid var(--border-subtle);
  padding: 8px 16px;
  max-height: 200px;
  overflow-y: auto;
}
.backlinks-header {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-family: var(--font-sans);
  color: var(--text-tertiary);
  cursor: pointer;
  padding: 4px 0;
  user-select: none;
}
.backlinks-header:hover { color: var(--text-secondary); }
.backlinks-header .caret {
  transition: transform 0.2s;
}
.backlinks-header .caret.open {
  transform: rotate(90deg);
}
.backlinks-list {
  margin-top: 6px;
}
.backlink-item {
  display: flex;
  flex-direction: column;
  padding: 6px 10px;
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.15s;
  margin-bottom: 2px;
}
.backlink-item:hover {
  background-color: var(--bg-tertiary);
}
.backlink-title {
  font-size: 13px;
  color: var(--user-accent);
  font-family: var(--font-sans);
}
.backlink-context {
  font-size: 11px;
  color: var(--text-tertiary);
  margin-top: 2px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* ── Mobile UI Adjustments ────────────────────────────────── */
.kg-mobile-col.kg-view {
  padding: 0;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

.kg-mobile-col .kg-overlay-legend {
  bottom: calc(80px + env(safe-area-inset-bottom, 20px)) !important;
  max-height: 40vh;
  overflow-y: auto;
  padding-bottom: 12px;
}

.kg-mobile-col .kg-overlay-search.expanded {
  left: 16px !important;
  right: 16px !important;
  top: 16px !important;
  width: auto !important;
}

.kg-mobile-col .kg-overlay-search.expanded .kg-search-box {
  width: 100% !important;
}
</style>
