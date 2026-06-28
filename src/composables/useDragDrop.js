/**
 * useDragDrop — 文件拖拽/粘贴/附件处理 composable
 *
 * 职责:
 *   - 拖拽状态 (isDragging)
 *   - 待发送文件/图片 (pendingImage, pendingFiles)
 *   - DOM 拖拽 + Tauri 原生拖拽处理
 *   - 文件夹识别 → FolderDropCard / KBEstimateCard 流程
 *   - 图片粘贴
 *   - 附件选择
 */

import { ref } from 'vue';

export function useDragDrop({ messages, inputText, scrollToBottom, globalFileAccess, agentMode, conversationId }) {
  const isDragging = ref(false);
  const pendingImages = ref([]);
  const pendingFiles = ref([]);
  const pendingFolderInfo = ref(null);
  const pendingKBEstimate = ref(null);

  // ── 附件选择 ──
  async function handleAttach() {
    try {
      const result = await window.electronAPI.selectFile();
      if (!result) return;
      if (typeof result === 'object' && result.type === 'image' && result.content) {
        pendingImages.value.push(result.content);
        return;
      }
      if (typeof result === 'object' && result.type === 'text' && result.content) {
        inputText.value = `请分析以下文件内容 (${result.name}):\n\n${result.content}`;
        return;
      }
      if (typeof result === 'string') {
        inputText.value = `用户选择了文件: ${result}`;
        return;
      }
    } catch (e) {
      console.error('[handleAttach]', e);
    }
    const base64 = await window.electronAPI.getClipboardImage();
    if (base64) {
      pendingImages.value.push(base64);
    }
  }

  // ── 图片粘贴 ──
  function handlePaste(event) {
    const items = event.clipboardData?.items;
    if (!items) return;
    for (const item of items) {
      if (item.type.startsWith('image/')) {
        event.preventDefault();
        const file = item.getAsFile();
        const reader = new FileReader();
        reader.onload = (e) => {
          const base64 = e.target.result.replace(/^data:image\/\w+;base64,/, '');
          pendingImages.value.push(base64);
        };
        reader.readAsDataURL(file);
        return;
      }
    }
  }

  // ── DOM 拖拽入口 ──
  function onDragEnter(e) {
    if (e.dataTransfer?.types?.includes('Files')) {
      isDragging.value = true;
    }
  }

  // ── DOM Drop 处理 ──
  async function handleDrop(event) {
    isDragging.value = false;
    const files = event.dataTransfer?.files;
    if (!files || files.length === 0) return;

    const file = files[0];
    const filePath = window.electronAPI.getFilePath ? window.electronAPI.getFilePath(file) : file.path;

    // 路由 1: 文件夹
    if (filePath) {
      try {
        const meta = await window.electronAPI.getFileMeta(filePath);
        if (meta && meta.isDir) {
          const scanResult = await window.electronAPI.scanFolder(filePath);
          if (scanResult && !scanResult.error) {
            pendingFolderInfo.value = { path: filePath, name: meta.name, scanResult };
            scrollToBottom();
            return;
          } else {
            inputText.value = `文件夹扫描失败: ${scanResult?.message || '未知错误'}`;
            return;
          }
        }
      } catch (err) {
        console.warn("Failed to check file meta", err);
      }
    }

    // 路由 2: 图片
    if (file.type.startsWith('image/')) {
      const reader = new FileReader();
      reader.onload = (e) => {
        const base64 = e.target.result.replace(/^data:image\/\w+;base64,/, '');
        pendingImages.value.push(base64);
      };
      reader.readAsDataURL(file);
      return;
    }

    // 路由 3: 文档
    if (!filePath) {
      inputText.value = `文件处理失败: 无法获取文件的本地路径。`;
      return;
    }
    try {
      const result = await window.electronAPI.readFile(filePath);
      if (result.error) {
        inputText.value = `文件读取失败: ${result.error}`;
      } else {
        inputText.value = `请分析以下文件内容 (${result.name}):\n\n${result.content}`;
      }
    } catch (err) {
      inputText.value = `文件处理失败: ${err.message}`;
    }
  }

  // ── Tauri 原生拖拽 ──
  async function handleTauriDrop(paths) {
    if (!paths || paths.length === 0) return;
    for (const filePath of paths) {
      if (filePath.match(/\.(png|jpg|jpeg|gif|webp)$/i)) continue;
      try {
        const meta = await window.electronAPI.getFileMeta(filePath);
        if (meta && meta.isDir) {
          let scanResult;
          if (window.__preScannedFolder && window.__preScannedFolder.path === filePath) {
            scanResult = window.__preScannedFolder.scanResult;
            window.__preScannedFolder = null;
          } else {
            scanResult = await window.electronAPI.scanFolder(filePath);
          }
          if (scanResult && !scanResult.error) {
            pendingFolderInfo.value = { path: filePath, name: meta.name, scanResult };
            scrollToBottom();
          } else {
            inputText.value = `文件夹扫描失败: ${scanResult?.message || '未知错误'}`;
          }
        } else {
          if (!pendingFiles.value.some(f => f.path === filePath)) {
            pendingFiles.value.push({
              path: filePath,
              name: meta ? meta.name : filePath.split(/[/\\]/).pop(),
              size: meta ? meta.size : 0
            });
          }
        }
      } catch (err) {
        console.warn(`原生拖拽处理错误: ${err.message}`);
      }
    }
  }

  // ── 文件夹确认/取消 ──
  function cancelFolderTrack() {
    pendingFolderInfo.value = null;
  }

  async function confirmFolderTrack() {
    const folder = pendingFolderInfo.value;
    pendingFolderInfo.value = null;
    if (!folder) return;

    const userContent = `我已经将文件夹「${folder.name}」拖入。`;
    messages.value.push({ role: 'user', content: userContent });
    await window.electronAPI.addMessage(conversationId(), 'user', userContent, null);

    const systemMsgId = Date.now().toString();
    const systemMsg = { id: systemMsgId, role: 'assistant', content: '正在处理文件夹，请稍候...' };
    messages.value.push(systemMsg);
    scrollToBottom();

    try {
      const result = await window.electronAPI.sendChat([{
        role: 'user',
        content: `Please execute the "track_folder" tool on this path: ${folder.path}`
      }], globalFileAccess.value, agentMode.value);

      const successContent = `✅ 已将「${folder.name}」收藏到目录列表。`;
      const index = messages.value.findIndex(m => m.id === systemMsgId);
      if (index !== -1) messages.value[index].content = successContent;
      await window.electronAPI.addMessage(conversationId(), 'assistant', successContent, null);

      pendingKBEstimate.value = { name: folder.name, path: folder.path, result: null };
      scrollToBottom();

      const estimateResult = await window.electronAPI.estimateKB(folder.path);
      if (pendingKBEstimate.value && pendingKBEstimate.value.path === folder.path) {
        pendingKBEstimate.value.result = estimateResult;
      }
    } catch (err) {
      const failContent = `❌ 文件夹收藏失败: ${err.message}`;
      const index = messages.value.findIndex(m => m.id === systemMsgId);
      if (index !== -1) messages.value[index].content = failContent;
      await window.electronAPI.addMessage(conversationId(), 'assistant', failContent, null);
    }
  }

  function cancelKBEstimate() {
    pendingKBEstimate.value = null;
  }

  function startKBBuild(folderPath, plan, kbUnlistens) {
    pendingKBEstimate.value = null;

    const progressMsgId = Date.now().toString();
    messages.value.push({ id: progressMsgId, role: 'assistant', content: '📚 正在启动知识库构建...' });
    scrollToBottom();

    const unlistenProgress = window.electronAPI.onKBProgress?.((payload) => {
      const idx = messages.value.findIndex(m => m.id === progressMsgId);
      if (idx !== -1) {
        messages.value[idx].content = `📚 ${payload.message} (${payload.current}/${payload.total})`;
      }
      scrollToBottom();
    });
    if (unlistenProgress) kbUnlistens.push(unlistenProgress);

    const unlistenComplete = window.electronAPI.onKBComplete?.((payload) => {
      const idx = messages.value.findIndex(m => m.id === progressMsgId);
      if (idx !== -1) {
        if (payload.failed > 0) {
          messages.value[idx].content = `⚠️ 知识库构建完成：${payload.success}/${payload.total} 成功，${payload.failed} 个文件处理失败。你现在可以向我提问关于「${payload.folder}」的内容。`;
        } else {
          messages.value[idx].content = `✅ 知识库构建完成！已成功处理 ${payload.total} 个文件。你现在可以向我提问关于「${payload.folder}」的内容。`;
        }
        window.electronAPI.addMessage(conversationId(), 'assistant', messages.value[idx].content, null);
      }
      scrollToBottom();
      if (unlistenProgress) unlistenProgress();
      if (unlistenComplete) unlistenComplete();
    });
    if (unlistenComplete) kbUnlistens.push(unlistenComplete);

    window.electronAPI.buildKB?.(folderPath, plan).then((result) => {
      if (result?.error) {
        const idx = messages.value.findIndex(m => m.id === progressMsgId);
        if (idx !== -1) messages.value[idx].content = `❌ ${result.message}`;
        if (unlistenProgress) unlistenProgress();
        if (unlistenComplete) unlistenComplete();
      }
    }).catch((err) => {
      const idx = messages.value.findIndex(m => m.id === progressMsgId);
      if (idx !== -1) messages.value[idx].content = `❌ 知识库构建失败: ${err.message || err}`;
      if (unlistenProgress) unlistenProgress();
      if (unlistenComplete) unlistenComplete();
    });
  }

  // ── Tauri 拖拽监听注册 ──
  function setupTauriDragListeners(tauriDragUnlistens) {
    if (!window.electronAPI.onDragEnter) return;

    let currentPreScanPath = null;
    window.electronAPI.onDragEnter(async (e) => {
      isDragging.value = true;
      if (e.payload && e.payload.paths && e.payload.paths.length > 0) {
        const filePath = e.payload.paths[0];
        if (currentPreScanPath === filePath) return;
        currentPreScanPath = filePath;
        try {
          const meta = await window.electronAPI.getFileMeta(filePath);
          if (meta && meta.isDir) {
            window.electronAPI.scanFolder(filePath).then(scanResult => {
              if (scanResult && !scanResult.error) {
                window.__preScannedFolder = { path: filePath, name: meta.name, scanResult };
              }
            });
          }
        } catch(err) {}
      }
    }).then(u => tauriDragUnlistens.push(u));

    window.electronAPI.onDragLeave(async () => { isDragging.value = false; }).then(u => tauriDragUnlistens.push(u));

    window.electronAPI.onDragDrop(async (e) => {
      // 避免在非 Chat 视图时意外触发 (因为 ChatView 是 v-show 并非 unmounted)
      const chatView = document.querySelector('.chat-view');
      if (chatView && chatView.offsetParent === null) return;

      isDragging.value = false;
      if (e.payload && e.payload.paths && e.payload.paths.length > 0) {
        await handleTauriDrop(e.payload.paths);
      }
    }).then(u => tauriDragUnlistens.push(u));
  }

  return {
    isDragging,
    pendingImages,
    pendingFiles,
    pendingFolderInfo,
    pendingKBEstimate,
    handleAttach,
    handlePaste,
    onDragEnter,
    handleDrop,
    handleTauriDrop,
    cancelFolderTrack,
    confirmFolderTrack,
    cancelKBEstimate,
    startKBBuild,
    setupTauriDragListeners,
  };
}
