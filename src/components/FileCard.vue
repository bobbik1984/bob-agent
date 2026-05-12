<template>
  <div
    class="file-card"
    :class="{ 'file-card--missing': !meta.exists, 'file-card--loading': loading }"
    @click="openFile"
  >
    <!-- 左侧：缩略图 或 文件类型图标 -->
    <div class="file-card__icon">
      <img v-if="meta.thumbnail" :src="meta.thumbnail" class="file-card__thumb" alt="" />
      <component v-else :is="fileIcon" :size="24" class="file-card__type-icon" />
    </div>

    <!-- 中间：文件名 + 元信息 -->
    <div class="file-card__info">
      <div class="file-card__name" :title="filePath">{{ meta.name || fileName }}</div>
      <div class="file-card__meta" v-if="meta.exists">
        <span>{{ formattedSize }}</span>
        <span class="file-card__dot">·</span>
        <span>{{ formattedTime }}</span>
      </div>
      <div class="file-card__meta file-card__meta--missing" v-else>
        文件不存在
      </div>
    </div>

    <!-- 右侧：操作按钮 -->
    <div class="file-card__actions" @click.stop>
      <button
        class="file-card__btn"
        title="在文件夹中显示"
        @click="showInFolder"
        v-if="meta.exists"
      >
        <FolderOpen :size="14" />
      </button>
      <button
        class="file-card__btn"
        title="打开文件"
        @click="openFile"
        v-if="meta.exists"
      >
        <ExternalLink :size="14" />
      </button>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, markRaw } from 'vue';
import {
  File, FileText, FileCode, FileSpreadsheet, FileArchive,
  Folder, FolderOpen, Image, Film, ExternalLink
} from 'lucide-vue-next';

const props = defineProps({
  filePath: { type: String, required: true },
});

const loading = ref(true);
const meta = ref({
  name: '',
  ext: '',
  type: 'file',
  size: 0,
  mtime: null,
  isDir: false,
  thumbnail: null,
  exists: true,
});

// 从路径中提取文件名作为 fallback
const fileName = computed(() => {
  const parts = props.filePath.replace(/\\/g, '/').split('/');
  return parts[parts.length - 1] || props.filePath;
});

// 文件类型 → Lucide 图标映射
const ICON_MAP = {
  folder: markRaw(Folder),
  document: markRaw(FileText),
  spreadsheet: markRaw(FileSpreadsheet),
  code: markRaw(FileCode),
  archive: markRaw(FileArchive),
  image: markRaw(Image),
  video: markRaw(Film),
  file: markRaw(File),
};

const fileIcon = computed(() => ICON_MAP[meta.value.type] || ICON_MAP.file);

// 格式化文件大小
const formattedSize = computed(() => {
  const bytes = meta.value.size;
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const val = (bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0);
  return `${val} ${units[i]}`;
});

// 格式化修改时间
const formattedTime = computed(() => {
  if (!meta.value.mtime) return '';
  const d = new Date(meta.value.mtime);
  const now = new Date();
  const diffMs = now - d;
  const diffMin = Math.floor(diffMs / 60000);

  if (diffMin < 1) return '刚刚';
  if (diffMin < 60) return `${diffMin} 分钟前`;
  const diffHour = Math.floor(diffMin / 60);
  if (diffHour < 24) return `${diffHour} 小时前`;
  const diffDay = Math.floor(diffHour / 24);
  if (diffDay < 7) return `${diffDay} 天前`;

  // 超过 7 天显示日期
  const month = d.getMonth() + 1;
  const day = d.getDate();
  return `${month}/${day}`;
});

function openFile() {
  if (!meta.value.exists) return;
  window.electronAPI.openFile(props.filePath).catch(err => {
    console.error('[FileCard] 打开文件失败:', err);
  });
}

function showInFolder() {
  window.electronAPI.showInFolder(props.filePath).catch(err => {
    console.error('[FileCard] 显示文件夹失败:', err);
  });
}

onMounted(async () => {
  try {
    const result = await window.electronAPI.getFileMeta(props.filePath);
    meta.value = result;
  } catch (err) {
    console.error('[FileCard] 获取文件元数据失败:', err);
    meta.value.exists = false;
  } finally {
    loading.value = false;
  }
});
</script>

<style scoped>
.file-card {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  margin: 6px 0;
  border-radius: 8px;
  background: var(--bg-hover);
  border: 1px solid var(--border-subtle);
  cursor: pointer;
  transition: all 0.15s ease;
  max-width: 420px;
  width: fit-content;
}

.file-card:hover {
  background: var(--bg-tertiary);
  border-color: var(--border-default);
  transform: translateY(-1px);
  box-shadow: var(--shadow-sm);
}

.file-card--missing {
  opacity: 0.5;
  cursor: not-allowed;
}

.file-card--loading {
  opacity: 0.6;
}

/* 图标区域 */
.file-card__icon {
  flex-shrink: 0;
  width: 40px;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 6px;
  background: rgba(var(--accent-rgb, 79, 139, 255), 0.1);
  color: var(--accent-primary);
}

.file-card__thumb {
  width: 40px;
  height: 40px;
  object-fit: cover;
  border-radius: 6px;
}

.file-card__type-icon {
  opacity: 0.85;
}

/* 信息区域 */
.file-card__info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.file-card__name {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  line-height: 1.3;
}

.file-card__meta {
  font-size: 11px;
  color: var(--text-tertiary);
  display: flex;
  align-items: center;
  gap: 4px;
  line-height: 1.2;
}

.file-card__meta--missing {
  color: var(--color-error);
}

.file-card__dot {
  opacity: 0.4;
}

/* 操作按钮 */
.file-card__actions {
  display: flex;
  gap: 2px;
  flex-shrink: 0;
  opacity: 0;
  transition: opacity 0.15s ease;
}

.file-card:hover .file-card__actions {
  opacity: 1;
}

.file-card__btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.12s ease;
}

.file-card__btn:hover {
  background: var(--bg-hover);
  color: var(--accent-primary);
}
</style>
