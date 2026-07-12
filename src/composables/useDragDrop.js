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
  const pendingBoardingPass = ref(null); // rxing BCBP 自动识别结果

  // ── 附件选择 ──
  async function handleAttach() {
    try {
      const result = await window.appAPI.selectFile();
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
    const base64 = await window.appAPI.getClipboardImage();
    if (base64) {
      pendingImages.value.push(base64);
    }
  }

  // ── BCBP 儒略日 → 日期转换 ──
  function julianDayToDate(julianStr) {
    const day = parseInt(julianStr, 10);
    if (isNaN(day) || day < 1 || day > 366) return '';
    const year = new Date().getFullYear();
    const date = new Date(year, 0, day);
    const mm = String(date.getMonth() + 1).padStart(2, '0');
    const dd = String(date.getDate()).padStart(2, '0');
    return `${year}-${mm}-${dd}`;
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
        reader.onload = async (e) => {
          const base64 = e.target.result.replace(/^data:image\/\w+;base64,/, '');
          pendingImages.value.push(base64);
          if (window.appAPI?.systemDecodeBarcodeBase64) {
            try {
              const res = await window.appAPI.systemDecodeBarcodeBase64(base64);
              if (res && res.data) {
                // BCBP 格式检测：以 M1 开头的是标准登机牌
                if (res.bcbp_info) {
                  const info = res.bcbp_info;
                  const dateStr = julianDayToDate(info.date);
                  pendingBoardingPass.value = {
                    raw_data: res.data,
                    raw_base64: base64,
                    format: res.format,
                    passenger_name: info.passenger_name,
                    pnr: info.pnr,
                    origin: info.origin,
                    destination: info.destination,
                    carrier: info.carrier,
                    flight_number: info.flight_number,
                    date: dateStr,
                    seat: info.seat.replace(/^0+/, ''),
                  };
                  console.log('[DragDrop] BCBP 登机牌识别成功:', pendingBoardingPass.value);
                } else {
                  // 非 BCBP 的普通条码 → 追加文字给大模型
                  if (inputText.value.length > 0 && !inputText.value.endsWith('\n')) {
                    inputText.value += '\n';
                  }
                  inputText.value += `[系统自动提取的图片条码内容: ${res.data}]`;
                }
              }
            } catch (err) {
              // rxing 解码失败，静默忽略，让大模型 Vision 兜底
            }
          }
        };
        reader.readAsDataURL(file);
        return;
      }
    }
  }

  // ── 登机牌确认/忽略 ──
  async function confirmBoardingPass() {
    const bp = pendingBoardingPass.value;
    if (!bp) return;
    try {
      const args = {
        title: `${bp.carrier}${bp.flight_number} ${bp.origin}-${bp.destination}`,
        category: 'flight',
        start_time: bp.date ? `${bp.date} 00:00:00` : '',
        end_time: '',
        venue: `${bp.origin} - ${bp.destination}`,
        seat_info: bp.seat,
        barcode_data: bp.raw_data,
        barcode_type: 'qr',
        flight_info: {
          flight_number: `${bp.carrier}${bp.flight_number}`,
          carrier: bp.carrier,
          pnr: bp.pnr,
          origin: bp.origin,
          destination: bp.destination,
          seat: bp.seat,
        },
      };
      await window.appAPI.createTicketDirect(args);
      messages.value.push({
        role: 'assistant',
        content: `已将航班 ${bp.carrier}${bp.flight_number} (${bp.origin} → ${bp.destination}) 的登机牌存入票夹。`,
      });
      pendingImages.value = [];
    } catch (err) {
      console.error('[DragDrop] 票据创建失败:', err);
      messages.value.push({
        role: 'assistant',
        content: `票据创建失败: ${err}`,
        _isError: true,
      });
    }
    pendingBoardingPass.value = null;
    scrollToBottom();
  }

  function dismissBoardingPass() {
    pendingBoardingPass.value = null;
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
    const filePath = window.appAPI.getFilePath ? window.appAPI.getFilePath(file) : file.path;

    // 路由 1: 文件夹
    if (filePath) {
      try {
        const meta = await window.appAPI.getFileMeta(filePath);
        if (meta && meta.isDir) {
          const scanResult = await window.appAPI.scanFolder(filePath);
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
      reader.onload = async (e) => {
        const base64 = e.target.result.replace(/^data:image\/\w+;base64,/, '');
        pendingImages.value.push(base64);
        if (window.appAPI?.systemDecodeBarcodeBase64) {
          try {
            const res = await window.appAPI.systemDecodeBarcodeBase64(base64);
            if (res && res.data) {
              if (inputText.value.length > 0 && !inputText.value.endsWith('\n')) {
                inputText.value += '\n';
              }
              inputText.value += `[系统自动提取的图片条码内容: ${res.data}]`;
            }
          } catch (err) {}
        }
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
      const result = await window.appAPI.readFile(filePath);
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
        const meta = await window.appAPI.getFileMeta(filePath);
        if (meta && meta.isDir) {
          let scanResult;
          if (window.__preScannedFolder && window.__preScannedFolder.path === filePath) {
            scanResult = window.__preScannedFolder.scanResult;
            window.__preScannedFolder = null;
          } else {
            scanResult = await window.appAPI.scanFolder(filePath);
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
    await window.appAPI.addMessage(conversationId(), 'user', userContent, null);

    const systemMsgId = Date.now().toString();
    const systemMsg = { id: systemMsgId, role: 'assistant', content: '正在处理文件夹，请稍候...' };
    messages.value.push(systemMsg);
    scrollToBottom();

    try {
      const result = await window.appAPI.sendChat([{
        role: 'user',
        content: `Please execute the "track_folder" tool on this path: ${folder.path}`
      }], globalFileAccess.value, agentMode.value);

      const successContent = `✅ 已将「${folder.name}」收藏到目录列表。`;
      const index = messages.value.findIndex(m => m.id === systemMsgId);
      if (index !== -1) messages.value[index].content = successContent;
      await window.appAPI.addMessage(conversationId(), 'assistant', successContent, null);

      pendingKBEstimate.value = { name: folder.name, path: folder.path, result: null };
      scrollToBottom();

      const estimateResult = await window.appAPI.estimateKB(folder.path);
      if (pendingKBEstimate.value && pendingKBEstimate.value.path === folder.path) {
        pendingKBEstimate.value.result = estimateResult;
      }
    } catch (err) {
      const failContent = `❌ 文件夹收藏失败: ${err.message}`;
      const index = messages.value.findIndex(m => m.id === systemMsgId);
      if (index !== -1) messages.value[index].content = failContent;
      await window.appAPI.addMessage(conversationId(), 'assistant', failContent, null);
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

    const unlistenProgress = window.appAPI.onKBProgress?.((payload) => {
      const idx = messages.value.findIndex(m => m.id === progressMsgId);
      if (idx !== -1) {
        messages.value[idx].content = `📚 ${payload.message} (${payload.current}/${payload.total})`;
      }
      scrollToBottom();
    });
    if (unlistenProgress) kbUnlistens.push(unlistenProgress);

    const unlistenComplete = window.appAPI.onKBComplete?.((payload) => {
      const idx = messages.value.findIndex(m => m.id === progressMsgId);
      if (idx !== -1) {
        if (payload.failed > 0) {
          messages.value[idx].content = `⚠️ 知识库构建完成：${payload.success}/${payload.total} 成功，${payload.failed} 个文件处理失败。你现在可以向我提问关于「${payload.folder}」的内容。`;
        } else {
          messages.value[idx].content = `✅ 知识库构建完成！已成功处理 ${payload.total} 个文件。你现在可以向我提问关于「${payload.folder}」的内容。`;
        }
        window.appAPI.addMessage(conversationId(), 'assistant', messages.value[idx].content, null);
      }
      scrollToBottom();
      if (unlistenProgress) unlistenProgress();
      if (unlistenComplete) unlistenComplete();
    });
    if (unlistenComplete) kbUnlistens.push(unlistenComplete);

    window.appAPI.buildKB?.(folderPath, plan).then((result) => {
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
    if (!window.appAPI.onDragEnter) return;

    let currentPreScanPath = null;
    window.appAPI.onDragEnter(async (e) => {
      isDragging.value = true;
      if (e.payload && e.payload.paths && e.payload.paths.length > 0) {
        const filePath = e.payload.paths[0];
        if (currentPreScanPath === filePath) return;
        currentPreScanPath = filePath;
        try {
          const meta = await window.appAPI.getFileMeta(filePath);
          if (meta && meta.isDir) {
            window.appAPI.scanFolder(filePath).then(scanResult => {
              if (scanResult && !scanResult.error) {
                window.__preScannedFolder = { path: filePath, name: meta.name, scanResult };
              }
            });
          }
        } catch(err) {}
      }
    }).then(u => tauriDragUnlistens.push(u));

    window.appAPI.onDragLeave(async () => { isDragging.value = false; }).then(u => tauriDragUnlistens.push(u));

    window.appAPI.onDragDrop(async (e) => {
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
    pendingBoardingPass,
    handleAttach,
    handlePaste,
    onDragEnter,
    handleDrop,
    handleTauriDrop,
    cancelFolderTrack,
    confirmFolderTrack,
    cancelKBEstimate,
    startKBBuild,
    confirmBoardingPass,
    dismissBoardingPass,
    setupTauriDragListeners,
  };
}
