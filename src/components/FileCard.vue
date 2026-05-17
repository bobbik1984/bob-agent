<template>
  <div
    class="bob-card-inline file-card"
    :class="{ 'file-card--missing': !meta.exists, 'file-card--loading': loading }"
    @click="openFile"
    :title="hoverTooltip"
  >
    <!-- 左侧：缩略图 或 文件类型图标 -->
    <div class="bob-card-inline__icon">
      <img v-if="meta.thumbnail" :src="meta.thumbnail" class="file-card__thumb" alt="" />
      <component v-else :is="fileIcon" :size="14" class="file-card__type-icon" />
    </div>

    <!-- 右侧：文件名 -->
    <div class="bob-card-inline__info">
      <div class="bob-card-inline__title">{{ meta.name || fileName }}</div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, markRaw } from 'vue';
import {
  File, FileText, FileCode, FileSpreadsheet, FileArchive,
  Folder, FolderOpen, Image, Film, ExternalLink
} from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';

const { t } = useI18n();

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
  if (meta.value.isDir) return null; // 文件夹不显示大小
  const bytes = meta.value.size;
  if (!bytes || bytes === 0) return null;
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

  if (diffMin < 1) return t('file_card.just_now');
  if (diffMin < 60) return t('file_card.mins_ago', { n: diffMin });
  const diffHour = Math.floor(diffMin / 60);
  if (diffHour < 24) return t('file_card.hours_ago', { n: diffHour });
  const diffDay = Math.floor(diffHour / 24);
  if (diffDay < 7) return t('file_card.days_ago', { n: diffDay });

  // 超过 7 天显示日期
  const month = d.getMonth() + 1;
  const day = d.getDate();
  return `${month}/${day}`;
});

const hoverTooltip = computed(() => {
  if (!meta.value.exists) return `${t('file_card.file_missing')}\n${t('file_card.label_path')}: ${props.filePath}`;
  
  let lines = [
    `${t('file_card.label_name')}: ${meta.value.name || fileName.value}`,
    `${t('file_card.label_path')}: ${props.filePath}`,
    `${t('file_card.label_modified')}: ${formattedTime.value || t('file_card.unknown')}`
  ];
  
  if (!meta.value.isDir && formattedSize.value) {
    lines.push(`${t('file_card.label_size')}: ${formattedSize.value}`);
  }
  
  return lines.join('\n');
});

function openFile() {
  if (!meta.value.exists) return;
  window.electronAPI.openFile(props.filePath).catch(err => {
    console.error('[FileCard] 打开文件失败:', err);
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
  max-width: 420px;
}

.file-card--missing {
  opacity: 0.5;
  cursor: not-allowed;
}

.file-card--loading {
  opacity: 0.6;
}

.file-card__thumb {
  width: 16px;
  height: 16px;
  object-fit: cover;
  border-radius: 2px;
}

.file-card__type-icon {
  opacity: 0.85;
}
</style>


