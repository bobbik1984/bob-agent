<template>
  <!-- Bob 的工作间（目录管理） -->
  <details class="settings-section card custom-model-override">
    <summary class="section-title">
      <div style="display: flex; align-items: center; gap: 8px;">
        <HardDrive :size="16" class="section-icon" style="opacity: 0.6;" />
        {{ $t('settings.bob_workspace') }}
      </div>
      <ChevronDown :size="16" class="details-chevron" />
    </summary>


    <!-- 工作目录 (workspaceDir) -->
    <div v-if="!isNativeMobile" class="details-section">
      <label class="form-label" style="font-size: 0.85em; margin-bottom: 6px; display: flex; align-items: center; gap: 6px;">
        <FolderOpen :size="14" style="opacity: 0.6;" />
        {{ $t('settings.workspace') }}
      </label>

      <div class="form-group workspace-group">
        <input
          v-model="config.workspaceDir"
          class="input"
          :placeholder="$t('settings.workspace_placeholder')"
          readonly
        />
        <button class="btn btn-primary browse-btn" @click="selectWorkspaceDir">
          <FolderOpen :size="14" />
          <span>{{ $t('settings.browse') }}</span>
        </button>
      </div>
      <button
        v-if="config.workspaceDir"
        class="btn-clear"
        @click="clearWorkspaceDir"
      >
        {{ $t('settings.clear_workspace') }}
      </button>
    </div>

    <div v-if="!isNativeMobile" class="details-section">
      <!-- 关注的文件夹 -->
      <label class="form-label" style="font-size: 0.85em; margin-bottom: 6px; display: flex; align-items: center; gap: 6px;">
        <FolderHeart :size="14" style="opacity: 0.6;" />
        {{ $t('settings.tracked_folders') }}
      </label>


      <div v-if="trackedFolders.length > 0" class="tracked-folders-list">
        <div
          v-for="folder in trackedFolders"
          :key="folder.id"
          class="tracked-folder-item"
        >
          <div class="folder-info">
            <span class="folder-name">{{ folder.name }}</span>
            <span class="folder-path">{{ folder.path }}</span>
          </div>
          <button class="btn-icon btn-remove-folder" @click="removeFolder(folder.path)" title="取消关注">
            <X :size="14" />
          </button>
        </div>
      </div>
      <div v-else class="empty-folders">
        <span>{{ $t('settings.tracked_folders_empty') }}</span>
      </div>

      <button class="btn btn-primary" @click="addFolder" style="margin-top: 12px;">
        <Plus :size="14" />
        <span>{{ $t('settings.add_folder') }}</span>
      </button>
    </div>

    <div class="details-section">
      <!-- 晨报天气城市 -->
      <label class="form-label" style="font-size: 0.85em; margin-bottom: 6px; display: flex; align-items: center; gap: 6px; margin-top: 24px;">
        <Cloud :size="14" style="opacity: 0.6;" />
        晨报天气城市
      </label>

      <div class="form-group workspace-group">
        <div class="path-display" style="display: flex; gap: 8px;">
          <input 
            v-model="config.weatherCity" 
            class="input" 
            placeholder="例如: 上海 (留空则自动定位)" 
            @change="saveConfig('weatherCity', config.weatherCity)"
            style="flex: 1;"
          />
        </div>
      </div>

      <!-- 设备名称 (deviceName) -->
      <label class="form-label" style="font-size: 0.85em; margin-bottom: 6px; display: flex; align-items: center; gap: 6px; margin-top: 24px;">
        <Smartphone :size="14" style="opacity: 0.6;" />
        设备名称
      </label>

      <div class="form-group workspace-group">
        <div class="path-display" style="display: flex; gap: 8px;">
          <input 
            v-model="config.deviceName" 
            class="input" 
            placeholder="例如: ThinkPad X1 (留空则使用设备 ID)" 
            @change="saveConfig('deviceName', config.deviceName)"
            style="flex: 1;"
          />
        </div>
      </div>

      <!-- 知识库目录 (wikiDir) -->
      <template v-if="!isNativeMobile">
        <label class="form-label" style="font-size: 0.85em; margin-bottom: 6px; display: flex; align-items: center; gap: 6px; margin-top: 24px;">
          <FileText :size="14" style="opacity: 0.6;" />
          {{ $t('settings.wiki_dir') }}
        </label>

        <div class="form-group workspace-group">
          <input
            v-model="config.wikiDir"
            class="input"
            :placeholder="$t('settings.wiki_dir_placeholder')"
            readonly
          />
          <button class="btn btn-primary browse-btn" @click="selectWikiDir">
            <FolderOpen :size="14" />
            <span>{{ $t('settings.browse') }}</span>
          </button>
        </div>
        <button
          v-if="config.wikiDir"
          class="btn-clear"
          @click="clearWikiDir"
        >
          {{ $t('settings.clear_wiki') }}
        </button>
      </template>
    </div>

    <div class="details-section">
      <!-- 技能目录 (externalSkillsDir) -->
      <template v-if="!isNativeMobile">
        <label class="form-label" style="font-size: 0.85em; margin-bottom: 6px; display: flex; align-items: center; gap: 6px;">
          <Puzzle :size="14" style="opacity: 0.6;" />
          {{ $t('settings.skills') }}
        </label>

        <div class="form-group workspace-group">
          <input
            v-model="config.externalSkillsDir"
            class="input"
            :placeholder="$t('settings.skills_placeholder')"
            readonly
          />
          <button class="btn btn-primary browse-btn" @click="selectExternalSkillsDir">
            <FolderOpen :size="14" />
            <span>{{ $t('settings.browse') }}</span>
          </button>
        </div>
        <button
          v-if="config.externalSkillsDir"
          class="btn-clear"
          @click="clearExternalSkillsDir"
        >
          {{ $t('settings.clear_skills') }}
        </button>
      </template>

      <div class="plugin-manager-entry details-section">

        <button class="btn btn-primary" @click="showPluginManager = true" style="display: flex; align-items: center; gap: 8px;">
          <Layers :size="16" />
          <span>{{ $t('settings.open_plugin_center') }}</span>
        </button>
      </div>
    </div>
  </details>

  <!-- 插件管理弹窗 -->
  <PluginManager :isOpen="showPluginManager" @close="showPluginManager = false" />

  <!-- T-1302: 记忆管理 -->
  <details class="settings-section card custom-model-override">
    <summary class="section-title">
      <div style="display: flex; align-items: center; gap: 8px;">
        <Brain :size="16" class="section-icon" />
        {{ $t('settings.memory_title') }}
      </div>
      <ChevronDown :size="16" class="details-chevron" />
    </summary>


    <div v-if="memoryLoading" style="display: flex; align-items: center; gap: 8px; color: var(--text-tertiary); padding: 12px 0;">
      <Loader2 :size="16" class="spin" />
      <span style="font-size: 0.85em;">{{ $t('inbox.loading') }}</span>
    </div>

    <div v-else-if="memoryEntries.length === 0" class="empty-folders">
      <span>{{ $t('settings.memory_empty') }}</span>
    </div>

    <div v-else class="memory-list">
      <div
        v-for="entry in memoryEntries"
        :key="entry.type + '/' + entry.id"
        class="memory-entry"
      >
        <div class="memory-entry-info">
          <div style="display: flex; align-items: center; gap: 8px;">
            <BookOpen v-if="entry.type === 'wiki'" :size="14" class="memory-entry-icon wiki" />
            <Brain v-else :size="14" class="memory-entry-icon session" />
            <span class="memory-entry-title" :title="entry.title || entry.id">{{ formatMemoryTitle(entry.title || entry.id) }}</span>
          </div>
          <div class="memory-entry-meta">
            {{ formatMemoryTime(entry.modified) }}
          </div>
        </div>
        <button
          class="btn-icon btn-remove-folder"
          @click="deleteMemoryEntry(entry)"
          :title="$t('settings.delete')"
        >
          <Trash2 :size="14" />
        </button>
      </div>
    </div>
  </details>

  <!-- 进化引擎看板 -->
  <details class="settings-section card custom-model-override">
    <summary class="section-title">
      <div style="display: flex; align-items: center; gap: 8px;">
        <Dna :size="16" class="section-icon" />
        {{ $t('settings.evolution_title') || '进化引擎' }}
      </div>
      <ChevronDown :size="16" class="details-chevron" />
    </summary>


    <div v-if="evoLoading" style="display: flex; align-items: center; gap: 8px; color: var(--text-tertiary); padding: 12px 0;">
      <Loader2 :size="16" class="spin" />
      <span style="font-size: 0.85em;">{{ $t('settings.evo_loading') }}</span>
    </div>

    <div v-else class="evo-dashboard">
      <!-- 统计卡片行 -->
      <div class="evo-stats-grid">
        <div class="evo-stat-card">
          <div class="evo-stat-value">{{ evoStats.observations?.total_conversations || 0 }}</div>
          <div class="evo-stat-label">{{ $t('settings.evo_obs_conversations') }}</div>
        </div>
        <div class="evo-stat-card">
          <div class="evo-stat-value">{{ evoStats.learned_facts_count || 0 }}</div>
          <div class="evo-stat-label">{{ $t('settings.evo_learned_facts') }}</div>
        </div>
        <div class="evo-stat-card">
          <div class="evo-stat-value">{{ evoStats.observations?.total_tool_calls || 0 }}</div>
          <div class="evo-stat-label">{{ $t('settings.evo_tool_calls') }}</div>
        </div>
        <div class="evo-stat-card">
          <div class="evo-stat-value">{{ formatTokenCount(evoStats.observations?.total_tokens_in, evoStats.observations?.total_tokens_out) }}</div>
          <div class="evo-stat-label">{{ $t('settings.evo_token_usage') }}</div>
        </div>
      </div>

      <!-- 最近做梦记录 -->
      <div v-if="evoStats.dream_history?.length > 0" class="evo-dream-section">
        <div class="evo-dream-header">
          <Moon :size="14" style="color: var(--user-accent);" />
          <span>{{ $t('settings.evo_dream_log') }}</span>
          <span v-if="evoStats.last_dream_at" class="evo-dream-time">{{ $t('settings.evo_last_time') }}{{ formatEvoTime(evoStats.last_dream_at) }}</span>
        </div>
        <div class="evo-dream-timeline">
          <div v-for="(dream, idx) in evoStats.dream_history.slice(0, 5)" :key="idx" class="evo-dream-entry">
            <div class="evo-dream-dot" :class="{ refined: dream.soul_refined }"></div>
            <div class="evo-dream-content">
              <span class="evo-dream-report">{{ dream.report || $t('settings.evo_no_update') }}</span>
              <span class="evo-dream-meta">{{ formatEvoTime(dream.created_at) }}</span>
            </div>
          </div>
        </div>
      </div>

      <div v-else class="empty-folders" style="margin-top: 8px;">
        <span>{{ $t('settings.evo_empty') }}</span>
      </div>
    </div>
  </details>

  <!-- 知识库目录迁移向导弹窗 -->
  <Transition name="briefing-fade">
    <div v-if="showWikiMigrationModal" class="wechat-modal-overlay" @click.self="cancelWikiMigration">
      <div class="help-modal" style="width: 500px;">
        <div class="briefing-header">
          <div class="briefing-icon"><Brain :size="18" /></div>
          <div class="briefing-title" style="flex: 1; font-size: 14px; font-weight: 600; color: var(--text-primary);">{{ $t('settings.wiki_migrate_title') }}</div>
          <button class="briefing-close" @click="cancelWikiMigration" style="background: none; border: none; color: var(--text-tertiary); cursor: pointer; padding: 4px; border-radius: 4px; display: flex; align-items: center; justify-content: center;">
            <X :size="14" />
          </button>
        </div>
        <div class="briefing-body" style="padding: 24px; display: flex; flex-direction: column; gap: 16px;">
          <p style="font-size: 0.9em; color: var(--text-secondary); line-height: 1.5;">
            {{ $t('settings.wiki_migrate_desc') }}<br/>
            <code style="display: block; padding: 8px; background: var(--bg-primary); border-radius: var(--radius-sm); margin-top: 6px; font-size: 0.85em; border: 1px solid var(--border-subtle); word-break: break-all; color: var(--text-primary);">{{ pendingWikiDir }}</code>
          </p>
          
          <div style="display: flex; flex-direction: column; gap: 10px;">
            <label style="font-size: 0.95em; font-weight: 600; color: var(--text-primary);">{{ $t('settings.wiki_migrate_select_mode') }}</label>
            
            <div style="display: flex; flex-direction: column; gap: 8px;">
              <label style="display: flex; align-items: flex-start; gap: 8px; padding: 12px; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); cursor: pointer; transition: all 0.2s;" :style="{ borderColor: migrationMode === 'copy_merge' ? 'var(--accent-primary)' : 'var(--border-subtle)', background: migrationMode === 'copy_merge' ? 'color-mix(in srgb, var(--accent-primary) 5%, transparent)' : 'transparent' }">
                <input type="radio" v-model="migrationMode" value="copy_merge" style="margin-top: 3px;" />
                <div>
                  <div style="font-weight: 600; font-size: 0.9em; color: var(--text-primary);">{{ $t('settings.wiki_migrate_mode_merge') }}</div>
                  <div style="font-size: 0.8em; color: var(--text-secondary); margin-top: 2px;">{{ $t('settings.wiki_migrate_mode_merge_desc') }}</div>
                </div>
              </label>

              <label style="display: flex; align-items: flex-start; gap: 8px; padding: 12px; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); cursor: pointer; transition: all 0.2s;" :style="{ borderColor: migrationMode === 'copy_overwrite' ? 'var(--accent-primary)' : 'var(--border-subtle)', background: migrationMode === 'copy_overwrite' ? 'color-mix(in srgb, var(--accent-primary) 5%, transparent)' : 'transparent' }">
                <input type="radio" v-model="migrationMode" value="copy_overwrite" style="margin-top: 3px;" />
                <div>
                  <div style="font-weight: 600; font-size: 0.9em; color: var(--text-primary);">{{ $t('settings.wiki_migrate_mode_overwrite') }}</div>
                  <div style="font-size: 0.8em; color: var(--text-secondary); margin-top: 2px;">{{ $t('settings.wiki_migrate_mode_overwrite_desc') }}</div>
                </div>
              </label>

              <label style="display: flex; align-items: flex-start; gap: 8px; padding: 12px; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); cursor: pointer; transition: all 0.2s;" :style="{ borderColor: migrationMode === 'link_only' ? 'var(--accent-primary)' : 'var(--border-subtle)', background: migrationMode === 'link_only' ? 'color-mix(in srgb, var(--accent-primary) 5%, transparent)' : 'transparent' }">
                <input type="radio" v-model="migrationMode" value="link_only" style="margin-top: 3px;" />
                <div>
                  <div style="font-weight: 600; font-size: 0.9em; color: var(--text-primary);">{{ $t('settings.wiki_migrate_mode_link') }}</div>
                  <div style="font-size: 0.8em; color: var(--text-secondary); margin-top: 2px;">{{ $t('settings.wiki_migrate_mode_link_desc') }}</div>
                </div>
              </label>
            </div>
          </div>

          <div v-if="migrationError" style="padding: 10px 12px; background: var(--color-error-bg); border: 1px solid var(--color-error); border-radius: var(--radius-sm); font-size: 0.85em; color: var(--color-error); line-height: 1.4;">
            {{ migrationError }}
          </div>

          <div style="display: flex; justify-content: flex-end; gap: 10px; margin-top: 8px;">
            <button class="btn btn-ghost" :disabled="isMigrating" @click="cancelWikiMigration">{{ $t('modal.cancel') }}</button>
            <button class="btn btn-primary" style="display: flex; align-items: center; gap: 6px;" :disabled="isMigrating" @click="confirmWikiMigration">
              <Loader2 v-if="isMigrating" class="spin" :size="14" />
              <span>{{ isMigrating ? $t('settings.wiki_migrate_processing') : $t('settings.wiki_migrate_confirm') }}</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  </Transition>
</template>

<script setup>
import { ref, onMounted, inject } from 'vue';
import { useI18n } from 'vue-i18n';
import { formatDateTime as formatMemoryTime, formatFuzzyTime as formatEvoTime } from '@/utils/date';
import { HardDrive, FolderOpen, FolderHeart, FileText, Puzzle, Layers, X, Plus, ChevronDown, Trash2, Brain, BookOpen, Loader2, Dna, Moon, Cloud, Smartphone } from 'lucide-vue-next';
import PluginManager from '../../components/PluginManager.vue';

const isNativeMobile = inject('isNativeMobile', false);

const props = defineProps({
  config: { type: Object, required: true },
});
const emit = defineEmits(['config-changed']);
const { t } = useI18n();

// ── Workspace ──
const showPluginManager = ref(false);
const trackedFolders = ref([]);

async function saveConfig(key, value) {
  await window.appAPI.setConfig(key, value);
  emit('config-changed');
}

async function selectWorkspaceDir() {
  const dirPath = await window.appAPI.selectWorkspaceDir();
  if (dirPath) {
    props.config.workspaceDir = dirPath;
    await saveConfig('workspaceDir', dirPath);
  }
}

async function clearWorkspaceDir() {
  props.config.workspaceDir = '';
  await saveConfig('workspaceDir', '');
}

// ── Wiki Dir + Migration ──
const showWikiMigrationModal = ref(false);
const pendingWikiDir = ref('');
const migrationMode = ref('copy_merge');
const isMigrating = ref(false);
const migrationError = ref('');

function cancelWikiMigration() {
  if (isMigrating.value) return;
  showWikiMigrationModal.value = false;
  pendingWikiDir.value = '';
  migrationError.value = '';
}

async function confirmWikiMigration() {
  isMigrating.value = true;
  migrationError.value = '';
  try {
    const res = await window.appAPI.migrateWikiDir(
      props.config.wikiDir || '',
      pendingWikiDir.value,
      migrationMode.value
    );
    if (res && res.ok) {
      props.config.wikiDir = pendingWikiDir.value;
      await saveConfig('wikiDir', pendingWikiDir.value);
      showWikiMigrationModal.value = false;
      pendingWikiDir.value = '';
      alert(t('settings.wiki_migrate_success'));
    } else {
      migrationError.value = res?.error || t('settings.wiki_migrate_error_unknown');
    }
  } catch (err) {
    migrationError.value = t('settings.wiki_migrate_failed') + err;
  } finally {
    isMigrating.value = false;
  }
}

async function selectWikiDir() {
  const dirPath = await window.appAPI.selectDir();
  if (dirPath) {
    if (dirPath === props.config.wikiDir) return;
    pendingWikiDir.value = dirPath;
    showWikiMigrationModal.value = true;
  }
}

async function clearWikiDir() {
  props.config.wikiDir = '';
  await saveConfig('wikiDir', '');
}

// ── External Skills Dir ──
async function selectExternalSkillsDir() {
  const dirPath = await window.appAPI.selectDir();
  if (dirPath) {
    props.config.externalSkillsDir = dirPath;
    await saveConfig('externalSkillsDir', dirPath);
  }
}

async function clearExternalSkillsDir() {
  props.config.externalSkillsDir = '';
  await saveConfig('externalSkillsDir', '');
}

// ── 文件夹跟踪 ──
async function loadTrackedFolders() {
  trackedFolders.value = await window.appAPI.getTrackedFolders();
}

async function addFolder() {
  const dirPath = await window.appAPI.selectFolderToTrack();
  if (dirPath) {
    await window.appAPI.addTrackedFolder(dirPath);
    await loadTrackedFolders();
  }
}

async function removeFolder(folderPath) {
  await window.appAPI.removeTrackedFolder(folderPath);
  await loadTrackedFolders();
}

// ── T-1302: 记忆管理 ──
const memoryLoading = ref(false);
const memoryEntries = ref([]);

async function loadMemoryEntries() {
  if (!window.appAPI.getMemoryEntries) return;
  memoryLoading.value = true;
  try {
    const entries = await window.appAPI.getMemoryEntries();
    memoryEntries.value = entries || [];
  } catch (e) {
    console.error('Failed to load memory entries', e);
    memoryEntries.value = [];
  } finally {
    memoryLoading.value = false;
  }
}

async function deleteMemoryEntry(entry) {
  if (!confirm(t('settings.memory_delete_confirm'))) return;
  try {
    await window.appAPI.deleteMemoryEntry(entry.type, entry.id);
    memoryEntries.value = memoryEntries.value.filter(
      e => !(e.type === entry.type && e.id === entry.id)
    );
  } catch (e) {
    console.error('Failed to delete memory entry', e);
  }
}

function formatMemoryTitle(title) {
  if (!title) return '';
  let cleaned = title.replace(/^(对话摘要|对话记忆|知识条目|知识库)[:：]\s*/g, '');
  if (cleaned.startsWith('conv-')) {
    return t('chat.conversation') + ' ' + cleaned.replace('conv-', '');
  }
  return cleaned;
}



// ── 进化引擎看板 ──
const evoLoading = ref(false);
const evoStats = ref({});

async function loadEvolutionStats() {
  if (!window.appAPI.getEvolutionStats) return;
  evoLoading.value = true;
  try {
    const data = await window.appAPI.getEvolutionStats();
    evoStats.value = data || {};
  } catch (e) {
    console.error('Failed to load evolution stats', e);
    evoStats.value = {};
  } finally {
    evoLoading.value = false;
  }
}

function formatTokenCount(tokIn, tokOut) {
  const total = (tokIn || 0) + (tokOut || 0);
  if (total < 1000) return String(total);
  if (total < 1_000_000) return (total / 1000).toFixed(1) + 'K';
  return (total / 1_000_000).toFixed(2) + 'M';
}



// ── Init ──
onMounted(async () => {
  await loadTrackedFolders();
  await loadMemoryEntries();
  loadEvolutionStats(); // 不 await，后台加载不阻塞
});
</script>

<style scoped>
.settings-section {
  margin-bottom: var(--space-5);
}

.section-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: var(--text-lg);
  font-weight: 500;
  margin-bottom: var(--space-4);
  color: var(--text-primary);
}

.section-desc {
  font-size: var(--text-sm);
  color: var(--text-tertiary);
  margin-bottom: var(--space-4);
}

.form-group {
  margin-bottom: var(--space-4);
  position: relative;
}

.form-label {
  display: block;
  font-size: var(--text-sm);
  color: var(--text-secondary);
  margin-bottom: var(--space-2);
  font-weight: 500;
}

/* 统一间距和折叠样式 */
.details-section {
  border-top: 1px solid var(--border-subtle);
  padding-top: var(--space-4);
  margin-top: var(--space-4);
}
.details-section:first-of-type {
  border-top: none;
  padding-top: 0;
  margin-top: 0;
}

details > summary {
  list-style: none;
}
details > summary::-webkit-details-marker {
  display: none;
}
.details-chevron {
  transition: transform 0.2s ease;
  color: var(--text-tertiary);
}
details[open] > summary .details-chevron {
  transform: rotate(180deg);
}

.workspace-group {
  display: flex;
  gap: var(--space-2);
  align-items: center;
}

.workspace-group .input {
  flex: 1;
  min-width: 0;
  text-overflow: ellipsis;
  cursor: default;
}

.browse-btn {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  white-space: nowrap;
  flex-shrink: 0;
}

.btn-clear {
  background: transparent;
  border: none;
  color: var(--text-tertiary);
  font-size: var(--text-xs);
  cursor: pointer;
  padding: var(--space-1) 0;
  margin-top: var(--space-2);
  font-family: var(--font-sans);
  transition: color var(--duration-fast);
}

.btn-clear:hover {
  color: var(--color-error);
}

/* ── 关注的文件夹 ── */
.tracked-folders-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 12px;
}

.tracked-folder-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  background: var(--bg-secondary);
  border-radius: 8px;
  border: 1px solid var(--border-subtle);
  transition: border-color var(--duration-fast);
}

.tracked-folder-item:hover {
  border-color: var(--border-default);
}

.folder-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.folder-name {
  font-weight: 600;
  font-size: 13px;
  color: var(--text-primary);
}

.folder-path {
  font-size: 11px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.btn-remove-folder {
  flex-shrink: 0;
  opacity: 0.4;
  transition: opacity var(--duration-fast), color var(--duration-fast);
}

.btn-remove-folder:hover {
  opacity: 1;
  color: var(--color-error);
}

.empty-folders {
  padding: 16px;
  text-align: center;
  color: var(--text-tertiary);
  font-size: 13px;
  border: 1px dashed var(--border-subtle);
  border-radius: 8px;
  margin-top: 12px;
}

.spin {
  animation: spin 1s linear infinite;
}
@keyframes spin { 100% { transform: rotate(360deg); } }

/* ── T-1302: Memory Management ── */
.memory-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.memory-entry {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  border-radius: var(--radius-lg);
  background: var(--bg-secondary);
  border: 1px solid var(--border-subtle);
  transition: background 0.15s;
}
.memory-entry:hover {
  background: var(--bg-tertiary);
}
.memory-entry-info {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}
.memory-entry-title {
  font-size: var(--text-sm);
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.memory-entry-meta {
  font-size: var(--text-xs);
  color: var(--text-tertiary);
  margin-left: 22px;
}
.memory-entry-icon {
  flex-shrink: 0;
}
.memory-entry-icon.wiki,
.memory-entry-icon.wiki :deep(svg) {
  color: var(--accent-secondary);
}
.memory-entry-icon.session,
.memory-entry-icon.session :deep(svg) {
  color: var(--accent-secondary);
  opacity: 0.6;
}

/* ── Evolution Dashboard ── */
.evo-dashboard {
  margin-top: 8px;
}
.evo-stats-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 10px;
  margin-bottom: 16px;
}
@media (max-width: 600px) {
  .evo-stats-grid { grid-template-columns: repeat(2, 1fr); }
}
.evo-stat-card {
  background: var(--bg-tertiary);
  border-radius: var(--radius-md);
  padding: 12px 14px;
  text-align: center;
  border: 1px solid var(--border-subtle);
  transition: border-color 0.2s;
}
.evo-stat-card:hover {
  border-color: var(--user-accent);
}
.evo-stat-value {
  font-size: 1.3em;
  font-weight: 700;
  color: var(--text-primary);
  letter-spacing: -0.02em;
}
.evo-stat-label {
  font-size: 0.75em;
  color: var(--text-tertiary);
  margin-top: 4px;
}
.evo-dream-section {
  margin-top: 4px;
}
.evo-dream-header {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 0.85em;
  font-weight: 600;
  color: var(--text-secondary);
  margin-bottom: 10px;
}
.evo-dream-time {
  margin-left: auto;
  font-weight: 400;
  font-size: 0.85em;
  color: var(--text-tertiary);
}
.evo-dream-timeline {
  position: relative;
  padding-left: 16px;
  border-left: 2px solid var(--border-subtle);
}
.evo-dream-entry {
  position: relative;
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 6px 0;
}
.evo-dream-dot {
  position: absolute;
  left: -21px;
  top: 10px;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--text-tertiary);
  border: 2px solid var(--bg-secondary);
  flex-shrink: 0;
}
.evo-dream-dot.refined {
  background: var(--user-accent);
  box-shadow: 0 0 6px var(--user-accent);
}
.evo-dream-content {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.evo-dream-report {
  font-size: 0.85em;
  color: var(--text-secondary);
  line-height: 1.4;
}
.evo-dream-meta {
  font-size: 0.75em;
  color: var(--text-tertiary);
}

/* ── Wiki Migration Modal ── */
.wechat-modal-overlay {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.4);
  backdrop-filter: blur(4px);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
}

.help-modal {
  width: 580px;
  max-height: 80vh;
  background: var(--bg-secondary);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-lg);
  display: flex;
  flex-direction: column;
}

.briefing-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--border-subtle);
}

.briefing-close:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

/* Transition */
.briefing-fade-enter-active {
  transition: all 0.3s ease;
}
.briefing-fade-leave-active {
  transition: all 0.2s ease;
}
.briefing-fade-enter-from {
  opacity: 0;
  transform: scale(0.95);
}
.briefing-fade-leave-to {
  opacity: 0;
  transform: scale(0.95);
}
</style>
