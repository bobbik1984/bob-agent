<template>
  <div class="chat-view" :class="{ 'is-mobile': isMobile }" @touchstart="handleTouchStart" @touchend="handleTouchEnd">
    <!-- Lightbox for Image Zoom -->
    <div v-if="zoomedImage" class="image-lightbox" @click="zoomedImage = null; imageScale = 1; imageTranslateX = 0; imageTranslateY = 0;" @wheel.prevent="handleImageWheel" @mousemove="handleImageMouseMove" @mouseup="handleImageMouseUp" @mouseleave="handleImageMouseUp">
      <img :src="zoomedImage" :style="{ transform: `translate(${imageTranslateX}px, ${imageTranslateY}px) scale(${imageScale})` }" @click.stop @mousedown="handleImageMouseDown" />
      <button class="lightbox-close" @click="zoomedImage = null; imageScale = 1; imageTranslateX = 0; imageTranslateY = 0;">&times;</button>
    </div>

    <!-- T-1304: 系统健康横幅 -->
    <div v-if="healthBanner" class="health-banner" :class="healthBanner.severity">
      <span class="health-icon">{{ healthBanner.severity === 'error' ? '!' : 'i' }}</span>
      <span class="health-text">{{ healthBanner.message }}</span>
      <button v-if="healthBanner.fixable" class="health-fix-btn" @click="handleAutoFix(healthBanner.code)">修复</button>
      <button class="health-dismiss-btn" @click="dismissHealthBanner(healthBanner.code)">&times;</button>
    </div>
    <!-- 移动端专属顶部栏 -->
    <div v-if="isMobile" class="mobile-header">
      <button class="mobile-hamburger" @click="emit('open-drawer')">
        <Menu :size="24" />
      </button>
      <div class="mobile-header-center">
        <input 
          v-if="isEditingTitle"
          ref="titleInputRef"
          v-model="editTitleText"
          class="mobile-header-title-input"
          @blur="saveTitle"
          @keydown.enter="saveTitle"
          @keydown.esc="cancelTitleEdit"
        />
        <span 
          v-else
          class="mobile-header-title" 
          @click="startTitleEdit"
          title="点击修改标题"
        >
          {{ conversationTitle }}
        </span>
      </div>
      <div class="mobile-header-right">
        <span class="cost-indicator" v-if="sessionCost > 0" style="font-size: 12px; color: var(--text-tertiary);">¥{{ sessionCost.toFixed(4) }}</span>
      </div>
    </div>
    <!-- 消息区域 -->
    <div class="messages-area" ref="messagesArea">
      

      <!-- 空状态：背景 logo（绝对定位，不参与 flex 布局） -->
      <div v-if="messages.length === 0" class="empty-logo-wrapper">
        <div class="empty-bob-logo bob-clickable" @click="openQuickNote"></div>
      </div>

      <!-- 空状态：前景内容（晨间汇报等） -->
      <div v-if="messages.length === 0" class="empty-state animate-fade-in">
        <!-- 晨间汇报卡片 -->
        <MorningBriefing
          @chat="onBriefingChat"
          @dismiss="() => {}"
        />
      </div>

      <!-- 消息列表 -->
      <div
        v-for="(msg, idx) in displayMessages"
        :key="msg.id || idx"
        class="message-row animate-slide-up"
        :class="[`message-${msg.role}`]"
      >
        <!-- 头像 -->
        <div class="message-avatar" :class="msg.role === 'user' ? 'avatar-user' : 'avatar-bob'">
          <User v-if="msg.role === 'user'" :size="16" />
          <div v-else class="bob-avatar-icon bob-clickable" @click="openQuickNote"></div>
        </div>

        <!-- 内容 -->
        <div class="message-body">
          <!-- 解析出的事件卡片 -->
          <template v-if="msg.type === 'confirm-card'">
            <ConfirmCard
              :event="msg.event"
              @confirm="(e) => handleConfirmEvent(e, msg)"
              @cancel="() => handleCancelEvent(msg)"
            />
          
    
    </template>





          <!-- 思维链折叠 -->
          <div v-if="msg.thinking && !msg._isError && msg.type !== 'confirm-card'" class="thinking-card" :class="{ expanded: msg._thinkingExpanded }">
            <button class="thinking-toggle" @click="msg._thinkingExpanded = !msg._thinkingExpanded">
              <ChevronRight :size="14" class="thinking-arrow" :class="{ 'expanded': msg._thinkingExpanded }" />
              <span>Thought process</span>
            </button>
            <div v-if="msg._thinkingExpanded" class="thinking-content selectable">
              {{ msg.thinking }}
            </div>
          </div>
          <!-- 错误卡片 -->
          <div v-if="msg._isError" class="error-card">
            <div class="error-icon">!</div>
            <div class="error-body">
              <div class="error-title">{{ $t('chat.error_title') }}</div>
              <div class="error-detail">{{ msg.content }}</div>
            </div>
          </div>
          <!-- 消息内容（block 数组渲染：text + file-card 交替）-->
          <div v-if="!msg._isError && msg.type !== 'confirm-card' && msg.type !== 'action-item-card' && msg.content" class="message-content selectable">
            <template v-for="(block, bi) in renderMessageBlocks(msg.content)" :key="bi">
              <div v-if="block.type === 'html'" v-html="block.content"></div>
              <CodeBlock v-else-if="block.type === 'code'" :code="block.code" :lang="block.lang" />
              <FileCard v-else-if="block.type === 'file'" :filePath="block.path" />
            </template>
          </div>

          <!-- 图片预览 (多图数组优先) -->
          <div v-if="msg.image_base64s && msg.image_base64s.length > 0" class="message-images-grid" style="display: flex; gap: 8px; flex-wrap: wrap;">
            <div v-for="(b64, imgIdx) in msg.image_base64s" :key="imgIdx" class="message-image">
              <img 
                :src="'data:image/png;base64,' + b64" 
                alt="用户图片" 
                @click="zoomedImage = 'data:image/png;base64,' + b64; imageScale = 1; imageTranslateX = 0; imageTranslateY = 0;"
                style="cursor: zoom-in; max-height: 200px; border-radius: 4px;"
              />
            </div>
          </div>
          <!-- 图片预览 (单图旧版兼容，仅在没有 image_base64s 时显示) -->
          <div v-else-if="msg.image_base64" class="message-image">
            <img 
              :src="'data:image/png;base64,' + msg.image_base64" 
              alt="用户图片" 
              @click="zoomedImage = 'data:image/png;base64,' + msg.image_base64; imageScale = 1; imageTranslateX = 0; imageTranslateY = 0;"
              style="cursor: zoom-in;"
            />
          </div>
          <!-- 元数据标注：模型 & 来源 & 复制 & 记忆标志 -->
          <div class="message-meta-row" v-if="msg.role === 'assistant' || msg.from_channel">
            <div v-if="msg.content && msg.content.includes('<|mem|>')" class="memory-indicator" title="已自动提炼知识到脑库">
              🧬
            </div>
            <div v-if="msg.from_channel" class="source-label">
              <Smartphone v-if="msg.from_channel === 'wechat' || (msg.from_channel !== 'wechat' && isMobile)" :size="10" />
              <Monitor v-else :size="10" />
              <span>{{ msg.from_channel === 'wechat' ? 'WeChat' : (isMobile ? 'Mobile' : 'Desktop') }}</span>
            </div>
            <div v-if="msg.role === 'assistant' && msg._modelLabel" class="model-label">
              {{ msg._modelLabel }}
            </div>
            <button
              v-if="msg.role === 'assistant' && msg.content"
              class="copy-rich-btn"
              title="存为笔记"
              @click="clipMessageToNote(msg)"
            >
              <BookmarkPlus :size="12" />
            </button>
            <!-- T-1201: 富文本复制按钮 -->
            <button
              v-if="msg.role === 'assistant' && msg.content"
              class="copy-rich-btn"
              :title="msg._copyDone ? '已复制' : '复制为富文本'"
              @click="copyRichText(msg)"
            >
              <Check v-if="msg._copyDone" :size="12" class="copy-done-icon" />
              <ClipboardCopy v-else :size="12" />
            </button>
          </div>
        </div>
      </div>

      <!-- 流式输出中 -->
      <div v-if="isStreaming" class="message-row message-assistant animate-slide-up">
        <div class="message-avatar avatar-bob"><div class="bob-avatar-icon"></div></div>
        <div class="message-body">
          <!-- 等待响应指示器：回车后立即出现，思考期间持续显示，直到正文开始流入才消失 -->
          <div v-if="!streamContent && activeTools.length === 0 && !streamThinking" class="typing-indicator">
            <span class="dot"></span><span class="dot"></span><span class="dot"></span>
          </div>
          <!-- 流式思考过程（实时） -->
          <div v-if="streamThinking" class="thinking-card stream-thinking" :class="{ expanded: streamThinkingExpanded }">
            <button class="thinking-toggle" @click="streamThinkingExpanded = !streamThinkingExpanded">
              <span class="thinking-pulse"></span>
              <ChevronRight :size="14" class="thinking-arrow" :class="{ 'expanded': streamThinkingExpanded }" />
              <span>{{ $t('chat.thinking') || 'Thinking...' }}</span>
            </button>
            <div v-if="streamThinkingExpanded" ref="streamThinkingRef" class="thinking-content stream-thinking-content selectable">
              {{ streamThinking }}<span class="typing-cursor"></span>
            </div>
          </div>
          <!-- 工具调用状态 -->
          <div v-if="activeTools.length > 0" class="tool-calls-panel">
            <div
              v-for="(tool, idx) in activeTools"
              :key="idx"
              class="tool-call-item"
              :class="{ 'is-running': tool.status === 'running' }"
            >
              <div class="tool-call-header" @click="tool._expanded = !tool._expanded">
                <span class="tool-dot" :class="tool.status === 'running' ? 'dot-running' : 'dot-done'"></span>
                <span class="tool-name">{{ tool.name }}</span>
                <ChevronRight v-if="tool.result" :size="12" class="tool-expand-icon" :class="{ 'rotate-90': tool._expanded }" />
              </div>
              <div v-if="tool._expanded && tool.result" class="tool-result-preview">
                <template v-if="tool._searchResults && tool._searchResults.length > 0">
                  <SearchCard
                    v-for="(sr, si) in tool._searchResults"
                    :key="si"
                    :title="sr.title"
                    :url="sr.url"
                    :snippet="sr.snippet"
                  />
                </template>
                <template v-else-if="tool._browserEnable">
                  <BrowserEnableCard
                    :url="tool._browserEnable.url"
                    :browser-path="tool._browserEnable.browserPath"
                    :browser-detected="tool._browserEnable.browserDetected"
                    @confirm="(data) => handleBrowserEnable(data, tool)"
                    @cancel="() => { tool._browserEnable = null }"
                  />
                </template>
                <template v-else>
                  <div class="tool-result-text" :class="{ 'is-expanded': tool._resultExpanded }">
                    {{ tool.result }}
                  </div>
                  <button v-if="tool.result && tool.result.length > 300" class="btn-ghost btn-expand-result" @click.stop="tool._resultExpanded = !tool._resultExpanded">
                    {{ tool._resultExpanded ? '收起' : '展开更多' }}
                  </button>
                </template>
              </div>
            </div>
          </div>
          <!-- CDN 上传进度条 -->
          <div v-if="cdnUpload.active" class="cdn-upload-progress">
            <div class="cdn-upload-info">
              <FileUp :size="14" />
              <span class="cdn-upload-name">{{ cdnUpload.fileName }} <span v-if="cdnUpload.attempt && cdnUpload.attempt > 1" style="color: var(--user-accent); font-size: 0.9em;">(重试 {{ cdnUpload.attempt }}/3)</span></span>
              <span class="cdn-upload-percent">{{ cdnUpload.percent }}%</span>
            </div>
            <div class="cdn-upload-bar-track">
              <div class="cdn-upload-bar-fill" :style="{ width: cdnUpload.percent + '%' }"></div>
            </div>
            <div class="cdn-upload-detail">
              {{ formatBytes(cdnUpload.bytesSent) }} / {{ formatBytes(cdnUpload.totalBytes) }}
            </div>
          </div>
          <div v-if="streamContent" class="message-content selectable">
            <template v-for="(block, bi) in renderMessageBlocks(streamContent)" :key="bi">
              <div v-if="block.type === 'html'" v-html="block.content"></div>
              <CodeBlock v-else-if="block.type === 'code'" :code="block.code" :lang="block.lang" />
              <FileCard v-else-if="block.type === 'file'" :filePath="block.path" />
            </template>
          </div>
          <!-- 流式元数据：模型标注 + 记忆标志 -->
          <div class="message-meta-row" v-if="currentModelName || streamContent.includes('<|mem|>')">
            <div v-if="streamContent.includes('<|mem|>')" class="memory-indicator" title="已自动提炼知识到脑库">
              🧬
            </div>
            <div v-if="currentModelName" class="model-label">{{ currentModelName }}</div>
          </div>
        </div>
      </div>
    <!-- Pending Folder Drop Card -->
    <div v-if="pendingFolderInfo" class="message-row">
      <div class="message-bubble system-bubble">
        <FolderDropCard
          :folder-path="pendingFolderInfo.path"
          :folder-name="pendingFolderInfo.name"
          :scan-result="pendingFolderInfo.scanResult"
          @confirm="confirmFolderTrack"
          @cancel="cancelFolderTrack"
        />
      </div>
    </div>
    
    <!-- Pending KB Estimate Card -->
    <div v-if="pendingKBEstimate" class="message-row">
      <div class="message-bubble system-bubble">
        <KBEstimateCard
          :folder-name="pendingKBEstimate.name"
          :estimate-result="pendingKBEstimate.result"
          @confirm="(plan) => onStartKBBuild(pendingKBEstimate.path, plan)"
          @cancel="cancelKBEstimate"
        />
      </div>
    </div>
    </div>

    <!-- 文件拖拽遮罩 -->
    <div
      v-if="isDragging"
      class="drop-overlay animate-fade-in"
      @dragover.prevent
      @dragleave="isDragging = false"
      @drop.prevent="handleDrop"
    >
      <div class="drop-content">
        <FileUp :size="48" class="drop-icon" />
        <span>{{ $t('chat.drop_hint_full') }}</span>
      </div>
    </div>

    <!-- 输入区 -->
    <div class="input-area">
      <div class="quick-actions-bar" v-if="inputText.trim().length > 0 && !isMobile">
        <div class="actions-spacer"></div>
        <button
          class="btn-parse-event"
          @click="parseTextAsEvent"
          :disabled="isParsing"
        >
          <Calendar v-if="!isParsing" :size="14" />
          <Loader2 v-else :size="14" class="animate-spin" />
          <span>{{ isParsing ? $t('chat.parsing') : $t('chat.parse_event') }}</span>
        </button>
      </div>
      <div class="input-row">
        <!-- BCBP 登机牌自动识别确认卡片 -->
        <div v-if="pendingBoardingPass" class="boarding-pass-card">
          <div class="bp-header">
            <span class="bp-icon">✈</span>
            <span class="bp-title">检测到登机牌</span>
          </div>
          <div class="bp-body" style="display: flex; gap: 16px; align-items: center; margin-bottom: 12px;">
            <div class="bp-main" style="flex: 1;">
              <div class="bp-route">
                <div class="bp-airport">
                  <span class="bp-code" style="font-size: 2.2em; font-weight: 800; color: var(--text-primary);">{{ pendingBoardingPass.origin }}</span>
                </div>
                <div class="bp-flight-info">
                  <span class="bp-flight-number" style="font-size: 1.1em; font-weight: 700; color: var(--user-accent, #4f8cf7);">{{ pendingBoardingPass.carrier }}{{ pendingBoardingPass.flight_number }}</span>
                  <div class="bp-arrow">→</div>
                  <span class="bp-date" style="font-size: 0.9em; font-weight: 500;">{{ pendingBoardingPass.date }}</span>
                </div>
                <div class="bp-airport">
                  <span class="bp-code" style="font-size: 2.2em; font-weight: 800; color: var(--text-primary);">{{ pendingBoardingPass.destination }}</span>
                </div>
              </div>
              <div class="bp-details">
                <div class="bp-detail-item">
                  <span class="bp-label">旅客 / PASSENGER</span>
                  <span class="bp-value" style="font-weight: 600;">{{ pendingBoardingPass.passenger_name }}</span>
                </div>
                <div class="bp-detail-item">
                  <span class="bp-label">座位 / SEAT</span>
                  <span class="bp-value" style="font-size: 1.2em; font-weight: 700;">{{ pendingBoardingPass.seat }}</span>
                </div>
                <div class="bp-detail-item">
                  <span class="bp-label">PNR</span>
                  <span class="bp-value" style="font-family: monospace; font-size: 1.1em; font-weight: 600;">{{ pendingBoardingPass.pnr }}</span>
                </div>
              </div>
            </div>
            <div class="bp-qr" v-if="pendingBoardingPass.raw_base64" style="background: white; padding: 6px; border-radius: 8px; box-shadow: 0 2px 8px rgba(0,0,0,0.1); display: flex; align-items: center; justify-content: center; width: 90px; height: 90px; flex-shrink: 0;">
              <img :src="'data:image/png;base64,' + pendingBoardingPass.raw_base64" style="width: 100%; height: 100%; object-fit: contain; filter: grayscale(1) contrast(100);" />
            </div>
          </div>
          <div class="bp-actions">
            <button class="bp-btn bp-btn-confirm" @click="confirmBoardingPass">存入票夹</button>
            <button class="bp-btn bp-btn-dismiss" @click="dismissBoardingPass">忽略</button>
          </div>
        </div>
        <!-- 图片预览 -->
        <div v-if="pendingImages.length > 0" class="inline-images-preview" style="display: flex; gap: 8px; flex-wrap: wrap; margin-bottom: 8px;">
          <div v-for="(img, idx) in pendingImages" :key="idx" class="inline-image-preview" style="position: relative;">
            <img :src="'data:image/png;base64,' + img" alt="Pending Image" style="max-height: 100px; border-radius: 4px;" />
            <button class="image-remove-inline btn-icon" @click="pendingImages.splice(idx, 1)" style="position: absolute; top: -6px; right: -6px; background: var(--text-primary); color: var(--bg-primary); padding: 2px; border-radius: 50%; box-shadow: 0 2px 4px rgba(0,0,0,0.2);"><X :size="12" /></button>
          </div>
        </div>
        <!-- 待发送文件预览 -->
        <div v-if="pendingFiles.length > 0" class="inline-files-preview">
          <div v-for="(f, index) in pendingFiles" :key="index" class="pending-file-chip">
            <FileText :size="12" />
            <span class="pending-file-name" :title="f.name">{{ f.name }}</span>
            <button class="file-remove-btn" @click="pendingFiles.splice(index, 1)"><X :size="10" /></button>
          </div>
        </div>
        <!-- 文本输入 -->
        <div class="input-wrapper" style="position: relative;">
          <!-- 移动端 + 按钮 -->
          <button v-if="isMobile" class="mobile-plus-btn" @click="showMobileTools = true; mobileSheetState = 'main'">
            <Plus :size="20" />
          </button>
          <!-- 悬浮指令菜单 (Slash/Mention) -->
          <div v-if="showCommandMenu" class="mention-menu">
            <div 
              v-for="(cmd, index) in commandList" 
              :key="cmd.id"
              class="mention-menu-item"
              :class="{ active: index === activeCommandIndex }"
              @click="executeCommand(index)"
              @mouseenter="activeCommandIndex = index"
            >
              <span style="display: flex; align-items: center; gap: 8px;">
                <component :is="cmd.icon" :size="14" style="opacity: 0.7; flex-shrink: 0;" />
                <span>{{ cmd.label }}</span>
              </span>
              <span v-if="cmd.description" style="color: var(--text-tertiary); font-size: 0.85em; margin-left: 8px;">{{ cmd.description }}</span>
              <span class="mention-shortcut" v-if="index === activeCommandIndex">Enter</span>
            </div>
          </div>
          <textarea
            ref="inputRef"
            v-model="inputText"
            class="chat-input"
            :placeholder="$t('chat.input_placeholder')"
            :rows="isMobile ? 1 : 3"
            enterkeyhint="send"
            @keydown="handleKeydown"
            @input="handleInput"
            @paste="handlePaste"
          ></textarea>
          <!-- Mobile 发送按钮 -->
          <div v-if="isMobile" class="mobile-send-btn-wrap">
            <button v-if="isStreaming" class="action-btn stop-btn" @click="stopGeneration" :title="$t('chat.stop')"><span class="icon-stop"></span></button>
            <button v-else class="action-btn send-btn" :disabled="!canSend || !chatReady" @click="sendMessage" :title="chatReadyMsg || $t('chat.send')">
              <span class="icon-send"></span>
            </button>
          </div>
        </div>
        <!-- 底部工具栏 -->
        <div v-if="!isMobile" class="input-toolbar">
          <button class="toolbar-item attach-btn" :title="$t('chat.attach_tooltip')" @click="handleAttach">
            <Paperclip :size="14" />
          </button>
          <button class="toolbar-item attach-btn" :title="$t('chat.cmd_save_note_tooltip') || '作为笔记速记'" @click="handleSaveToNote">
            <Bookmark :size="14" />
          </button>
          <button class="toolbar-item attach-btn" title="截取屏幕" @click="handleScreenshot" :disabled="isScreenshotting">
            <Camera :size="14" />
          </button>
          <!-- 模型切换器 -->
          <div class="model-switcher-wrap" v-if="currentModelName">
            <button class="toolbar-item model-indicator" @click="toggleModelSwitcher">
              <img v-if="currentModelLogo" :src="currentModelLogo" class="model-logo-sm" @error="(e) => e.target.style.display = 'none'" />
              <span>{{ currentModelName }}</span>
              <ChevronUp :size="10" class="chevron-icon" />
            </button>
            <!-- 弹出选择面板 -->
            <div v-if="showModelSwitcher" class="model-popup">
              <div class="model-popup-cols">
                <!-- 左：供应商列表 -->
                <div class="model-popup-providers">
                  <button
                    v-for="p in modelProviderList"
                    :key="p.id"
                    class="model-provider-btn"
                    :class="{ active: switcherProvider === p.id }"
                    @click="switcherProvider = p.id"
                  >
                    <img v-if="getModelLogo(p.id)" :src="getModelLogo(p.id)" class="model-logo-sm" />
                    <span class="model-provider-name" :title="p.name">{{ p.name }}</span>
                    <span class="provider-count">{{ p.count }}</span>
                  </button>
                </div>
                <!-- 右：当前供应商的模型 -->
                <div class="model-popup-models">
                  <button
                    v-for="m in switcherModels"
                    :key="m.id"
                    class="model-option"
                    :class="{ active: currentModelRaw === m.id }"
                    @click="switchModel(m.id)"
                  >
                    <span class="model-option-label">{{ m.displayName }}</span>
                  </button>
                  <div v-if="switcherModels.length === 0" class="model-option-empty">
                    {{ $t('chat.no_models') }}
                  </div>
                </div>
              </div>
            </div>
          </div>
          
          <!-- 代理模式切换器 -->
          <div class="model-switcher-wrap">
            <button class="toolbar-item model-indicator" @click="showAgentModeSwitcher = !showAgentModeSwitcher">
              <Shield v-if="agentMode === 'insight'" :size="12" style="color: var(--text-tertiary);" />
              <Zap v-else-if="agentMode === 'yolo'" :size="12" style="color: var(--accent-primary);" />
              <Target v-else :size="12" style="color: #ff9800;" />
              <span>{{ agentMode === 'insight' ? $t('chat.mode_qa') : (agentMode === 'yolo' ? $t('chat.mode_act') : $t('chat.mode_goal')) }}</span>
              <ChevronUp :size="10" class="chevron-icon" />
            </button>
            <div v-if="showAgentModeSwitcher" class="model-popup">
              <button class="model-option" :class="{ active: agentMode === 'insight' }" @click="agentMode = 'insight'; showAgentModeSwitcher = false">
                <Shield :size="14" style="margin-right: 8px;" />
                <span class="model-option-label">{{ $t('chat.mode_qa_desc') }}</span>
              </button>
              <button class="model-option" :class="{ active: agentMode === 'yolo' }" @click="agentMode = 'yolo'; showAgentModeSwitcher = false">
                <Zap :size="14" style="margin-right: 8px;" />
                <span class="model-option-label">{{ $t('chat.mode_act_desc') }}</span>
              </button>
              <button class="model-option" :class="{ active: agentMode === 'goal' }" @click="agentMode = 'goal'; showAgentModeSwitcher = false">
                <Target :size="14" style="margin-right: 8px;" />
                <span class="model-option-label">{{ $t('chat.mode_goal_desc') }}</span>
              </button>
            </div>
          </div>

          <!-- 全局权限开关 -->
          <label class="toolbar-item global-access-toggle" :class="{ active: globalFileAccess }" :title="$t('chat.global_access_tooltip')">
            <input type="checkbox" v-model="globalFileAccess" style="display: none;" />
            <Unlock v-if="globalFileAccess" :size="12" style="color: var(--accent-primary);" />
            <Lock v-else :size="12" style="opacity: 0.5;" />
            <span class="global-access-text">{{ $t('chat.global_access') }}</span>
          </label>

          <div class="toolbar-spacer"></div>
          <!-- 导出按钮 -->
          <button class="toolbar-item" @click="exportConversation" :title="$t('chat.export_tooltip')">
            <Download :size="13" />
          </button>
          <!-- 计费指示器 -->
          <span class="toolbar-item cost-indicator" :title="$t('chat.cost_tooltip')">
            ¥{{ sessionCost.toFixed(4) }}
          </span>
          <button
            v-if="isStreaming"
            class="action-btn stop-btn"
            @click="stopGeneration"
            :title="$t('chat.stop')"
          >
            <span class="icon-stop"></span>
          </button>
          <button
            v-else
            class="action-btn send-btn"
            :disabled="!canSend || !chatReady"
            @click="sendMessage"
            :title="chatReadyMsg || $t('chat.send')"
          >
            <span class="icon-send"></span>
          </button>
        </div>
      </div>
    </div>
  </div>

    


    <!-- 移动端底部抽屉 (Bottom Sheet) -->
    <Teleport to="body">
      <div v-if="isMobile && showMobileTools" class="mobile-sheet-overlay" @click="showMobileTools = false"></div>
      <transition name="slide-up">
        <div v-if="isMobile && showMobileTools" class="mobile-bottom-sheet">
          <div v-show="mobileSheetState !== 'main'" class="sheet-header">
            <button v-if="mobileSheetState === 'models'" class="sheet-back-btn" @click="mobileSheetState = 'providers'">
              <ChevronLeft :size="20" /> <span style="margin-left:4px">{{ $t('chat.back') || '返回' }}</span>
            </button>
            <button v-else-if="mobileSheetState === 'providers' || mobileSheetState === 'agentMode'" class="sheet-back-btn" @click="mobileSheetState = 'main'">
              <ChevronLeft :size="20" /> <span style="margin-left:4px">{{ $t('chat.back') || '返回' }}</span>
            </button>
            
            <div class="sheet-title" style="font-size: 14px; font-weight: 600; color: var(--text-primary);">
              {{
                mobileSheetState === 'main' ? '快捷功能' :
                mobileSheetState === 'providers' ? '选择供应商' :
                mobileSheetState === 'models' ? '选择具体模型' : '选择执行形式'
              }}
            </div>
          </div>
          <div v-if="mobileSheetState === 'main'" class="sheet-content main-grid" style="grid-template-columns: repeat(3, 1fr); padding-bottom: 24px;">
            <!-- 1. 添加附件 -->
            <button class="sheet-grid-item" @click="handleAttach(); showMobileTools = false;">
              <div class="grid-icon-wrap" style="background: rgba(var(--user-accent-rgb, 39, 118, 187), 0.1); color: var(--accent-primary);">
                <Paperclip :size="24" />
              </div>
              <span class="grid-item-label">{{ $t('chat.mobile_attach') }}</span>
            </button>
            
            <!-- 2. 选择模型 -->
            <button class="sheet-grid-item" @click="mobileSheetState = 'providers'">
              <div class="grid-icon-wrap" style="background: rgba(var(--user-accent-rgb, 39, 118, 187), 0.1); color: var(--accent-primary);">
                <Cpu :size="24" />
              </div>
              <span class="grid-item-label">{{ $t('chat.mobile_select_model') }}</span>
            </button>
            
            <!-- 3. 执行形式 -->
            <button class="sheet-grid-item" @click="mobileSheetState = 'agentMode'">
              <div class="grid-icon-wrap" style="background: rgba(var(--user-accent-rgb, 39, 118, 187), 0.1); color: var(--accent-primary);">
                <Zap :size="24" />
              </div>
              <span class="grid-item-label">{{ $t('chat.mobile_agent_mode') }}</span>
            </button>
          </div>
          <div v-else-if="mobileSheetState === 'providers'" class="sheet-content list-view">
            <button v-for="p in modelProviderList" :key="p.id" class="sheet-list-item" :class="{ active: switcherProvider === p.id }" @click="switcherProvider = p.id; mobileSheetState = 'models'">
              <img v-if="getModelLogo(p.id)" :src="getModelLogo(p.id)" class="model-logo-sm" />
              <div class="item-info">
                <span class="item-name">{{ p.name }}</span>
                <span class="item-count">{{ p.count }} Models</span>
              </div>
              <ChevronRight :size="16" class="text-tertiary" />
            </button>
            <div v-if="modelProviderList.length === 0" class="sheet-empty">{{ $t('chat.no_models') || '暂无可用模型，请先配置 API Key' }}</div>
          </div>
          <div v-else-if="mobileSheetState === 'models'" class="sheet-content list-view">
            <button v-for="m in switcherModels" :key="m.id" class="sheet-list-item" :class="{ active: currentModelRaw === m.id }" @click="switchModel(m.id); showMobileTools = false; mobileSheetState = 'main'">
              <span class="item-name">{{ m.displayName }}</span>
              <Check v-if="currentModelRaw === m.id" :size="16" class="text-accent" />
            </button>
            <div v-if="switcherModels.length === 0" class="sheet-empty">{{ $t('chat.no_models') || '无可用模型' }}</div>
          </div>
          <div v-else-if="mobileSheetState === 'agentMode'" class="sheet-content list-view">
            <button class="sheet-list-item" :class="{ active: agentMode === 'insight' }" @click="agentMode = 'insight'; showMobileTools = false; mobileSheetState = 'main'">
              <Shield :size="20" style="margin-right: 12px;" />
              <span class="item-name">{{ $t('chat.mode_qa_desc') }}</span>
              <Check v-if="agentMode === 'insight'" :size="16" class="text-accent" />
            </button>
            <button class="sheet-list-item" :class="{ active: agentMode === 'yolo' }" @click="agentMode = 'yolo'; showMobileTools = false; mobileSheetState = 'main'">
              <Zap :size="20" style="margin-right: 12px;" />
              <span class="item-name">{{ $t('chat.mode_act_desc') }}</span>
              <Check v-if="agentMode === 'yolo'" :size="16" class="text-accent" />
            </button>
            <button class="sheet-list-item" :class="{ active: agentMode === 'goal' }" @click="agentMode = 'goal'; showMobileTools = false; mobileSheetState = 'main'">
              <Target :size="20" style="margin-right: 12px;" />
              <span class="item-name">{{ $t('chat.mode_goal_desc') }}</span>
              <Check v-if="agentMode === 'goal'" :size="16" class="text-accent" />
            </button>
          </div>
        </div>
      </transition>
    </Teleport>

    <!-- Custom Offline Engine Modal -->
    <Teleport to="body">
      <div v-if="showOfflineEngineModal" class="modal-overlay">
        <div class="modal-content custom-confirm-modal">
          <div class="modal-header">
            <h3>启动本地引擎</h3>
          </div>
          <div class="modal-body">
            <p>本地模型 <strong>{{ pendingOfflineEngineModel?.displayName }}</strong> 尚未启动，或正在运行其他模型。</p>
            <p>是否立即启动？（需要约 5-10 秒加载至内存）</p>
          </div>
          <div class="modal-footer">
            <button class="btn btn-secondary" @click="showOfflineEngineModal = false">取消</button>
            <button class="btn btn-primary" @click="startOfflineEngineFromModal">立即启动</button>
          </div>
        </div>
      </div>
    </Teleport>

</template>

<script>
import '@/utils/markdown';
</script>

<script setup>
import { ref, watch, onMounted, onUnmounted, nextTick, inject, computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { Sparkles, FileText, Camera, Calendar, User, ChevronRight, ChevronDown, ChevronUp, ChevronLeft, X, FileUp, Paperclip, Bookmark, Loader2, Shield, Zap, Target, Lock, Unlock, Download, Smartphone, Monitor, ClipboardCopy, Check, BookmarkPlus, Plus, Menu, Cpu, Play, PenLine, BookOpen, Pin } from 'lucide-vue-next';
import ConfirmCard from '../components/ConfirmCard.vue';
import FileCard from '../components/FileCard.vue';
import CodeBlock from '../components/CodeBlock.vue';
import SearchCard from '../components/SearchCard.vue';
import BrowserEnableCard from '../components/BrowserEnableCard.vue';
import FolderDropCard from '../components/FolderDropCard.vue';
import KBEstimateCard from '../components/KBEstimateCard.vue';
import MorningBriefing from '../components/MorningBriefing.vue';


import { useChat } from '../composables/useChat.js';
import { useModelSwitcher } from '../composables/useModelSwitcher.js';
import { useDragDrop } from '../composables/useDragDrop.js';

const isEditingTitle = ref(false);
const titleInputRef = ref(null);
const editTitleText = ref('');

function startTitleEdit() {
  editTitleText.value = conversationTitle.value;
  isEditingTitle.value = true;
  nextTick(() => {
    if (titleInputRef.value) titleInputRef.value.focus();
  });
}

function saveTitle() {
  if (!isEditingTitle.value) return;
  isEditingTitle.value = false;
  const newTitle = editTitleText.value.trim();
  if (newTitle && newTitle !== conversationTitle.value) {
    emit('update-title', { id: props.conversationId, title: newTitle });
  }
}

function cancelTitleEdit() {
  isEditingTitle.value = false;
}

const showOfflineEngineModal = ref(false);
const pendingOfflineEngineModel = ref(null);
let engineJustStarted = false;

async function startOfflineEngineFromModal() {
  if (!pendingOfflineEngineModel.value) return;
  const currentModelObj = pendingOfflineEngineModel.value;
  showOfflineEngineModal.value = false;
  
  messages.value.push({ role: 'system', content: `⚙️ 正在启动本地模型 ${currentModelObj.displayName}...` });
  scrollToBottom();
  
  try {
    const res = await window.appAPI.startOfflineEngine(currentModelObj.id);
    if (res && res.error) {
       messages.value.push({ role: 'system', content: `❌ 本地模型启动失败: ${res.error}` });
    } else {
       messages.value.push({ role: 'system', content: `✅ 本地模型已就绪。` });
       // Set bypass flag so sendMessage() skips the engine re-check
       engineJustStarted = true;
       sendMessage();
    }
  } catch (e) {
    console.error('Failed to start offline engine', e);
    messages.value.push({ role: 'system', content: `❌ 本地模型启动失败: ${e}` });
  }
  scrollToBottom();
}



const { t } = useI18n();

const props = defineProps({
  conversationId: String,
});
const emit = defineEmits(['update-title', 'open-drawer']);

// ── DOM refs (留在组件层) ─────────────────────────────
const messagesArea = ref(null);
const inputRef = ref(null);
const logoOpacity = ref(1);

const zoomedImage = ref(null);
const imageScale = ref(1);
const imageTranslateX = ref(0);
const imageTranslateY = ref(0);
let isDraggingImage = false;
let startDragX = 0;
let startDragY = 0;

function handleImageWheel(e) {
  if (!zoomedImage.value) return;
  // zoom speed
  const zoomFactor = 0.1;
  if (e.deltaY < 0) {
    // scroll up -> zoom in
    imageScale.value = Math.min(imageScale.value + zoomFactor, 5); // max 5x
  } else {
    // scroll down -> zoom out
    imageScale.value = Math.max(imageScale.value - zoomFactor, 0.2); // min 0.2x
  }
}

function handleImageMouseDown(e) {
  e.preventDefault();
  isDraggingImage = true;
  startDragX = e.clientX - imageTranslateX.value;
  startDragY = e.clientY - imageTranslateY.value;
}

function handleImageMouseMove(e) {
  if (!isDraggingImage) return;
  imageTranslateX.value = e.clientX - startDragX;
  imageTranslateY.value = e.clientY - startDragY;
}

function handleImageMouseUp(e) {
  isDraggingImage = false;
}

const showCommandMenu = ref(false);
const showMobileTools = ref(false);
const mobileSheetState = ref('main');
const commandTriggerIndex = ref(-1);
const commandType = ref(''); // 'slash' or 'mention'
const activeCommandIndex = ref(0);

const commandList = computed(() => {
  if (commandType.value === 'slash') {
    return [
      { id: 'memo', icon: PenLine, label: t('chat.cmd_memo') || '/memo', description: t('chat.cmd_memo_desc') || '作为闪念笔记保存，不发给AI', action: () => insertSlashCommand('/memo ') },
      { id: 'note', icon: BookOpen, label: '/note', description: t('chat.cmd_note_desc') || '新建笔记并打开编辑器', action: () => insertSlashCommand('/note ') },
      { id: 'clip', icon: Pin, label: '/clip', description: t('chat.cmd_clip_desc') || '将AI最近回复保存为笔记', action: () => handleClipCommand() },
    ];
  } else {
    return [
      { id: 'file', icon: Paperclip, label: t('chat.cmd_file') || '引用本地文件/图片', description: '', action: async () => { 
          await handleAttach(); 
          if (commandTriggerIndex.value >= 0) {
            inputText.value = inputText.value.substring(0, commandTriggerIndex.value) + inputText.value.substring(commandTriggerIndex.value + 1);
          }
      } }
    ];
  }
});

function executeCommand(index) {
  if (index >= 0 && index < commandList.value.length) {
    commandList.value[index].action();
    showCommandMenu.value = false;
    commandTriggerIndex.value = -1;
  }
}

function insertSlashCommand(cmdStr) {
  const text = inputText.value;
  const triggerIdx = commandTriggerIndex.value;
  inputText.value = text.substring(0, triggerIdx) + cmdStr + text.substring(triggerIdx + 1);
  nextTick(() => {
    if (inputRef.value) {
      inputRef.value.focus();
      const newPos = triggerIdx + cmdStr.length;
      inputRef.value.setSelectionRange(newPos, newPos);
    }
  });
}

async function onOpenMobileModelSwitcher() {
  await toggleModelSwitcher();
  showModelSwitcher.value = false;
  mobileSheetState.value = 'providers';
  showMobileTools.value = true;
}

function handleSaveToNote() {
  const text = inputText.value.trim();
  if (!text) {
    window.appAPI?.showNotification(t('app.notification_title') || '提示', t('chat.cmd_note_empty') || '请先输入内容再保存为笔记');
    return;
  }
  // 在原本内容前强制加上 /memo 并触发发送
  inputText.value = '/memo ' + text;
  sendMessage();
}

// P3-3: /clip 命令按钮处理
function handleClipCommand() {
  showCommandMenu.value = false;
  inputText.value = '/clip';
  sendMessage();
}

// ── 闪念速记入口 (从 App.vue provide) ─────────────────
const openQuickNote = inject('openQuickNote', () => {});
const isMobile = inject('isMobile', ref(false));
const conversationTitle = ref('');

// ── 手势返回 (T-2225) ──────────────────────────────
let touchStartX = 0;
let touchStartY = 0;
function handleTouchStart(e) {
  if (!isMobile.value) return;
  touchStartX = e.changedTouches[0].screenX;
  touchStartY = e.changedTouches[0].screenY;
}
function handleTouchEnd(e) {
  if (!isMobile.value) return;
  const touchEndX = e.changedTouches[0].screenX;
  const touchEndY = e.changedTouches[0].screenY;
  if (touchStartX < 40 && touchEndX - touchStartX > 50 && Math.abs(touchEndY - touchStartY) < 50) {
    emit('open-drawer');
  }
}

// ── 本地 UI 状态 ─────────────────────────────────────
const globalFileAccess = ref(false);
const agentMode = ref('insight');
const showAgentModeSwitcher = ref(false);

// ── T-1304: Doctor 健康横幅 ──────────────────────────
const healthBanner = ref(null);

function dismissHealthBanner(code) {
  healthBanner.value = null;
  if (code) {
    localStorage.setItem('dismissed_banner_' + code, Date.now().toString());
  }
}

// ── T-1305: 聊天就绪守卫 ─────────────────────────────
const chatReady = ref(true);  // fail-open: 默认可发送
const chatReadyMsg = ref('');

// ── streamThinking 流式思考 ──────────────────────────
const streamThinkingExpanded = ref(true);  // 默认展开，用户可折叠
const streamThinkingRef = ref(null);

// ── 组合 Composables ─────────────────────────────────

// 滚动辅助 (注入给 composables 使用)
function scrollToBottom() {
  nextTick(() => {
    if (messagesArea.value) {
      messagesArea.value.scrollTop = messagesArea.value.scrollHeight;
    }
  });
}

// 模型切换
const {
  currentModelRaw, showModelSwitcher, availableModels, switcherProvider,
  modelProviderList, switcherModels, currentModelName, currentModelLogo,
  getModelLogo, toggleModelSwitcher, switchModel, initModels, refreshModel,
} = useModelSwitcher();

// 核心聊天
const {
  messages, displayMessages, inputText, isStreaming, streamContent, streamThinking,
  activeTools, isParsing, sessionCost, canSend,
  loadMessages, onBriefingChat, sendMessage: _sendMessage,
  handleStreamChunk, stopGeneration, exportConversation,
  renderMarkdown, renderMessageBlocks,
  parseTextAsEvent: _parseTextAsEvent,
  handleConfirmEvent, handleCancelEvent,
  clipMessageToNote,
} = useChat(props, emit, { scrollToBottom, currentModelName, globalFileAccess, agentMode });

// 拖拽/附件
const {
  isDragging, pendingImages, pendingFiles, pendingFolderInfo, pendingKBEstimate,
  pendingBoardingPass,
  handleAction, handleAttach, handlePaste, onDragEnter, handleDrop, handleTauriDrop,
  cancelFolderTrack, confirmFolderTrack, cancelKBEstimate, startKBBuild,
  confirmBoardingPass, dismissBoardingPass,
  setupTauriDragListeners,
} = useDragDrop({
  messages, inputText, scrollToBottom, globalFileAccess, agentMode,
  conversationId: () => props.conversationId,
});

// ── 模板绑定的包装函数 ──────────────────────────────
  // sendMessage 需要传入 pendingImage/pendingFiles/resetTextareaHeight
  async function sendMessage() {
    // 自动探测文本中的绝对路径，转为附件
    const txt = inputText.value || '';
    const pathRegex = /([a-zA-Z]:\\[^"'<>|*?]+?\.(?:pdf|txt|md|csv|json|yaml|yml|log|py|js|rs|ts|vue|html|css|docx|xlsx|png|jpg|jpeg|gif|webp))/gi;
    let match;
    while ((match = pathRegex.exec(txt)) !== null) {
      const pathStr = match[1];
      if (!pendingFiles.value.find(f => f.path === pathStr)) {
        pendingFiles.value.push({
          name: pathStr.substring(pathStr.lastIndexOf('\\') + 1),
          path: pathStr
        });
      }
    }

    const currentModelObj = availableModels.value.find(m => m.id === currentModelRaw.value);
    const hasVision = currentModelObj && currentModelObj.vision;

    if (pendingImages.value.length > 0) {
      if (!hasVision) {
        messages.value.push({
          role: 'assistant',
          content: '当前选定的模型不支持视觉（图像识别）能力，无法处理截图/图像。请切换至支持 vision 的模型（如 GPT-4o, Gemini 等）后再试。',
          _isError: true,
          _thinkingExpanded: false,
        });
        scrollToBottom();
        return;
      }
    }

    if (currentModelObj && currentModelObj.provider === 'offline' && !engineJustStarted) {
      try {
        const engineStatus = await window.appAPI.invoke('get_offline_engine_status');
        if (engineStatus.status !== 'running' || engineStatus.model !== currentModelObj.id) {
          pendingOfflineEngineModel.value = currentModelObj;
          showOfflineEngineModal.value = true;
          return;
        }
      } catch (e) {
        console.error(e);
        return;
      }
    }
    engineJustStarted = false;

    _sendMessage(pendingImages, pendingFiles, resetTextareaHeight, async ({ userMessage, filesToRead, streamThinking }) => {
      // 检查是否有 pdf 需要走 vision 通道渲染为图片
      if (hasVision && filesToRead.length > 0) {
        const pdfFiles = filesToRead.filter(f => f.path && f.path.toLowerCase().endsWith('.pdf'));
        if (pdfFiles.length > 0) {
          streamThinking.value = `正在使用 PDFium 引擎将 ${pdfFiles.length} 个 PDF 渲染为高清图片流，请稍候...`;

          for (const pdfFile of pdfFiles) {
            const pdfPath = pdfFile.path;
            try {
              const b64Array = await window.appAPI.invoke('system_render_pdf_to_images', { path: pdfPath });
              if (b64Array && b64Array.length > 0) {
                if (!userMessage.image_base64s) userMessage.image_base64s = [];
                userMessage.image_base64s.push(...b64Array);
              }
            } catch (e) {
              console.error('PDF 渲染失败', e);
              throw new Error(`PDF ${pdfPath} 渲染图片失败: ${e}`);
            }

            // 从 filesToRead 中剔除，因为已经转成了图片，不走纯文本读取通道
            const idx = filesToRead.findIndex(f => f.path === pdfPath);
            if (idx !== -1) filesToRead.splice(idx, 1);
          }
          
          streamThinking.value = ''; // 渲染完成，清空思考状态让位于大模型
        }
      }
    });
  }

function parseTextAsEvent() {
  _parseTextAsEvent(resetTextareaHeight);
}

// ── T-1201: 富文本复制 ──────────────────────────────
async function saveToNote(msg) {
  if (!msg.content) return;
  try {
    const noteText = typeof msg.content === 'string' ? msg.content : JSON.stringify(msg.content);
    // 过滤掉内部标签，如 <|mem|>
    const cleanText = noteText.replace(/<\|mem\|>/g, '').trim();
    if (!cleanText) return;
    
    const res = await window.appAPI.notebookAppendDaily(cleanText);
    if (res && res.ok) {
      msg._savedToNote = true;
      // 可以在此处弹出全局 notification，但最简单的是 UI 响应
    }
  } catch (e) {
    console.error('Save to note failed:', e);
  }
}

async function copyRichText(msg) {
  try {
    const html = renderMarkdown(msg.content);
    const plainText = msg.content;
    // 优先使用 Tauri 剪贴板插件（writeHtml），fallback 到浏览器 Clipboard API
    if (window.__TAURI_INTERNALS__) {
      const { writeHtml } = await import('@tauri-apps/plugin-clipboard-manager');
      await writeHtml(html, plainText);
    } else {
      // 浏览器环境 fallback（dev server 预览时）
      const blob = new Blob([html], { type: 'text/html' });
      const textBlob = new Blob([plainText], { type: 'text/plain' });
      await navigator.clipboard.write([
        new ClipboardItem({ 'text/html': blob, 'text/plain': textBlob })
      ]);
    }
    // 短暂 ✓ 反馈
    msg._copyDone = true;
    setTimeout(() => { msg._copyDone = false; }, 1500);
  } catch (err) {
    console.error('[clipboard] copy rich text failed:', err);
  }
}

// ── 浏览器增强确认 ──────────────────────────────────
async function handleBrowserEnable(data, tool) {
  try {
    await window.appAPI.browserEnable();
    tool._browserEnable = null;
    tool.result = '✅ 浏览器已启用，正在重新加载页面...';
  } catch (err) {
    console.error('[browser] enable failed:', err);
    tool.result = `浏览器启用失败: ${err.message}`;
  }
}

// ── T-1304: 自动修复处理 ─────────────────────────────
async function handleAutoFix(code) {
  try {
    const result = await window.appAPI.autoFix(code);
    if (result?.ok) {
      healthBanner.value = null;
    } else {
      healthBanner.value.message = result?.message || '修复失败';
      healthBanner.value.fixable = false;
    }
  } catch (e) {
    console.error('[doctor] autoFix failed:', e);
  }
}

// ── 输入辅助 (依赖 DOM ref, 留在组件层) ──────────────
function handleInput(event) {
  autoResize();
  const text = inputText.value;
  const cursorIndex = inputRef.value?.selectionStart || 0;
  const textBeforeCursor = text.substring(0, cursorIndex);
  
  const slashMatch = /(?:^|\n|\s)\/$/.test(textBeforeCursor);
  const mentionMatch = /(?:^|\n|\s)@$/.test(textBeforeCursor);

  if (slashMatch) {
    showCommandMenu.value = true;
    commandType.value = 'slash';
    commandTriggerIndex.value = cursorIndex - 1;
    activeCommandIndex.value = 0;
  } else if (mentionMatch) {
    showCommandMenu.value = true;
    commandType.value = 'mention';
    commandTriggerIndex.value = cursorIndex - 1;
    activeCommandIndex.value = 0;
  } else {
    showCommandMenu.value = false;
  }
}

const isScreenshotting = ref(false);

async function handleScreenshot() {
  if (isScreenshotting.value) return; // 防止重复点击
  isScreenshotting.value = true;
  try {
    
    // 记录截图前的剪贴板状态，用于检测用户是否取消了截图
    let prevClipHash = '';
    try {
      const { readImage } = await import('@tauri-apps/plugin-clipboard-manager');
      const prevImg = await readImage();
      const prevBytes = await prevImg.rgba();
      prevClipHash = prevBytes.length.toString();
    } catch (_) { /* 剪贴板可能为空 */ }

    // 截图期间的窗口隐藏、等待和恢复逻辑已经全部移到了 Rust 后端
    await window.appAPI.takeScreenshot();

    // 彻底抛弃 Tauri 官方的 readImage 插件！
    // 它在处理部分 Windows Snipping Tool 的 DIB 图像时，会导致 Rust 线程死锁或 IPC 序列化永久挂起。
    // 我们强制使用原生 HTML5 navigator.clipboard.read()，并且增加一个 Race 超时机制，防止权限弹窗无人点击导致假死。
    try {
      const clipboardItems = await Promise.race([
        navigator.clipboard.read(),
        new Promise((_, reject) => setTimeout(() => reject(new Error('Permission prompt timeout or clipboard locked')), 15000))
      ]);

      for (const item of clipboardItems) {
        const imageType = item.types.find(t => t.startsWith('image/'));
        if (imageType) {
          const blob = await item.getType(imageType);
          const base64Result = await new Promise((resolve, reject) => {
            const reader = new FileReader();
            reader.onload = (e) => resolve(e.target.result.replace(/^data:image\/\w+;base64,/, ''));
            reader.onerror = reject;
            reader.readAsDataURL(blob);
          });
          if (base64Result) {
            pendingImages.value.push(base64Result);
            return; // 成功粘贴图
          }
        }
      }
      console.log('No image found in native clipboard');
    } catch (err) {
      console.warn("Native clipboard read failed (permission denied, timed out, or unreadable):", err);
    }
  } catch (e) {
    console.error('Screenshot failed:', e);
  } finally {
    isScreenshotting.value = false;
  }
}

function handleKeydown(event) {
  if (showCommandMenu.value) {
    if (event.key === 'Enter') {
      event.preventDefault();
      executeCommand(activeCommandIndex.value);
      return;
    }
    if (event.key === 'Escape') {
      event.preventDefault();
      showCommandMenu.value = false;
      return;
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault();
      activeCommandIndex.value = (activeCommandIndex.value > 0) ? activeCommandIndex.value - 1 : commandList.value.length - 1;
      return;
    }
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      activeCommandIndex.value = (activeCommandIndex.value < commandList.value.length - 1) ? activeCommandIndex.value + 1 : 0;
      return;
    }
  }

  if (event.key === 'Enter' && !event.shiftKey) {
    event.preventDefault();
    sendMessage();
  }
}

function autoResize() {
  const textarea = inputRef.value;
  if (!textarea) return;
  textarea.style.height = 'auto';
  textarea.style.height = Math.min(textarea.scrollHeight, 160) + 'px';
}

function resetTextareaHeight() {
  nextTick(() => {
    if (inputRef.value) {
      inputRef.value.style.height = 'auto';
    }
  });
}

// ── 点击外部关闭模型弹窗 ──────────────────────────
function onClickOutside(e) {
  if (!e.target.closest('.model-switcher-wrap')) {
    showModelSwitcher.value = false;
    showAgentModeSwitcher.value = false;
  }
}

// ── Logo 滚动视差 ────────────────────────────────────
function onMessagesScroll() {
  const el = messagesArea.value;
  if (!el) return;
  const scrollY = el.scrollTop;
  const fadeDistance = 100;
  logoOpacity.value = Math.max(0, 1 - scrollY / fadeDistance);
}

// ── 拦截消息链接点击 ─────────────────────────────────
function onMessageLinkClick(e) {
  const a = e.target.closest('a');
  if (!a || !a.href) return;
  e.preventDefault();
  const href = a.getAttribute('href');
  if (href.startsWith('file://') || /^[A-Za-z]:[\\\/]/.test(href)) {
    let filePath = href.replace('file:///', '');
    try { filePath = decodeURIComponent(filePath); } catch(e){}
    window.appAPI.openFile(filePath).catch(err => {
      console.error('打开文件失败:', err);
    });
  } else {
    if (window.appAPI.openExternal) {
      window.appAPI.openExternal(href);
    }
  }
}

// ── 知识库构建 (需要 kbUnlistens 引用) ───────────────
let kbUnlistens = [];

function onStartKBBuild(folderPath, plan) {
  startKBBuild(folderPath, plan, kbUnlistens);
}

// -- CDN 上传进度状态 --
const cdnUpload = ref({
  active: false,
  fileName: '',
  percent: 0,
  bytesSent: 0,
  totalBytes: 0,
});
function formatBytes(bytes) {
  if (bytes < 1024) return bytes + ' B';
  if (bytes < 1048576) return (bytes / 1024).toFixed(1) + ' KB';
  return (bytes / 1048576).toFixed(1) + ' MB';
}

let cdnUnlistens = [];

// ── 生命周期 ─────────────────────────────────────────
let cleanupStreamListener = null;
let tauriDragUnlistens = [];
let remoteMessageUnlisten = null;

function onAndroidBackPressed(e) {
  if (showOfflineEngineModal.value) {
    showOfflineEngineModal.value = false;
    e.preventDefault();
  } else if (showCommandMenu.value) {
    showCommandMenu.value = false;
    e.preventDefault();
  } else if (showMobileTools.value) {
    showMobileTools.value = false;
    e.preventDefault();
  } else if (showAgentModeSwitcher.value) {
    showAgentModeSwitcher.value = false;
    e.preventDefault();
  } else if (showModelSwitcher.value) {
    showModelSwitcher.value = false;
    e.preventDefault();
  }
}

function onSendMessageToBob(e) {
  if (e.detail) {
    inputText.value = e.detail;
    sendMessage();
  }
}

function onDetectBoardingPass(e) {
  if (e.detail) {
    pendingBoardingPass.value = e.detail;
  }
}

onMounted(async () => {
  window.addEventListener('open-mobile-model-switcher', onOpenMobileModelSwitcher);
  window.addEventListener('android-back-pressed', onAndroidBackPressed);
  window.addEventListener('send-message-to-bob', onSendMessageToBob);
  window.addEventListener('detect-boarding-pass', onDetectBoardingPass);
  cleanupStreamListener = window.appAPI.onStreamChunk(handleStreamChunk);

  // 远程消息监听
  if (window.appAPI.onRemoteNewMessage) {
    remoteMessageUnlisten = await window.appAPI.onRemoteNewMessage((event) => {
      const payload = event?.payload || event;
      const convId = payload?.conversation_id || event?.conversation_id;
      if (convId && convId === props.conversationId) {
        if (payload?.status === 'thinking') {
          isStreaming.value = true;
          streamContent.value = '';
          streamThinking.value = '';
        } else {
          isStreaming.value = false;
        }
        loadMessages();
      }
    });
  }

  // CDN 上传进度监听
  if (window.__TAURI_INTERNALS__) {
    cdnUnlistens.push(await window.appAPI.listenEvent('cdn:upload-start', (e) => {
      cdnUpload.value = { active: true, fileName: e.payload.file_name, percent: 0, bytesSent: 0, totalBytes: e.payload.total_bytes, attempt: 1 };
    }));
    cdnUnlistens.push(await window.appAPI.listenEvent('cdn:upload-progress', (e) => {
      cdnUpload.value.percent = e.payload.percent;
      cdnUpload.value.bytesSent = e.payload.bytes_sent;
    }));
    cdnUnlistens.push(await window.appAPI.listenEvent('cdn:upload-done', () => {
      cdnUpload.value.percent = 100;
      setTimeout(() => { cdnUpload.value.active = false; }, 1500);
    }));
    cdnUnlistens.push(await window.appAPI.listenEvent('cdn:upload-error', () => {
      cdnUpload.value.active = false;
    }));
  }

  loadMessages();
  await initModels();

  // T-1304: 启动健康检查
  try {
    const health = await window.appAPI.healthCheck();
    if (health && !health.healthy) {
      const firstIssue = health.issues?.[0];
      if (firstIssue) {
        const lastDismissed = localStorage.getItem('dismissed_banner_' + firstIssue.code);
        if (lastDismissed) {
          const hoursPassed = (Date.now() - parseInt(lastDismissed, 10)) / (1000 * 60 * 60);
          if (hoursPassed < 24) return;
        }

        healthBanner.value = {
          severity: firstIssue.severity,
          message: firstIssue.message,
          code: firstIssue.code,
          fixable: firstIssue.fixable,
        };
      }
    }
  } catch (e) {
    console.warn('[doctor] healthCheck failed:', e);
  }

  // T-1305: 聊天就绪校验
  try {
    const ready = await window.appAPI.validateChatReady();
    chatReady.value = ready?.ready !== false;  // fail-open
    chatReadyMsg.value = ready?.message || '';
  } catch (e) {
    console.warn('[chatReady] validation failed:', e);
    chatReady.value = true;  // fail-open
  }

  // 拖拽监听
  document.addEventListener('dragenter', onDragEnter);
  setupTauriDragListeners(tauriDragUnlistens);

  // 点击外部关闭弹窗
  document.addEventListener('click', onClickOutside);

  // Logo 滚动视差
  if (messagesArea.value) {
    messagesArea.value.addEventListener('scroll', onMessagesScroll);
    messagesArea.value.addEventListener('click', onMessageLinkClick);
  }
});

onUnmounted(() => {
  window.removeEventListener('open-mobile-model-switcher', onOpenMobileModelSwitcher);
  window.removeEventListener('android-back-pressed', onAndroidBackPressed);
  window.removeEventListener('send-message-to-bob', onSendMessageToBob);
  window.removeEventListener('detect-boarding-pass', onDetectBoardingPass);
  if (cleanupStreamListener) cleanupStreamListener();
  if (remoteMessageUnlisten) remoteMessageUnlisten();
  document.removeEventListener('dragenter', onDragEnter);
  document.removeEventListener('click', onClickOutside);
  if (messagesArea.value) {
    messagesArea.value.removeEventListener('scroll', onMessagesScroll);
    messagesArea.value.removeEventListener('click', onMessageLinkClick);
  }
  tauriDragUnlistens.forEach(u => typeof u === 'function' && u());
  tauriDragUnlistens = [];
  kbUnlistens.forEach(u => typeof u === 'function' && u());
  kbUnlistens = [];
  cdnUnlistens.forEach(u => typeof u === 'function' && u());
  cdnUnlistens = [];
});

// 切换对话时重新加载消息
watch(() => props.conversationId, async () => {
  if (props.conversationId) {
    const conv = await window.appAPI.getConversation(props.conversationId);
    sessionCost.value = conv?.cost || conv?.total_cost || 0;
    conversationTitle.value = conv?.title || '新对话';
  } else {
    sessionCost.value = 0;
    conversationTitle.value = '';
  }

  loadMessages();
  globalFileAccess.value = false;
  currentModelRaw.value = (await window.appAPI.getActiveModels())?.main || '';
}, { immediate: true });

// streamThinking 自动滚动到底部
watch(streamThinking, () => {
  nextTick(() => {
    if (streamThinkingRef.value) {
      streamThinkingRef.value.scrollTop = streamThinkingRef.value.scrollHeight;
    }
  });
});

// ── 暴露给父组件 ────────────────────────────────────────
defineExpose({
  refreshModel,
  scrollToBottom,
});

</script>

<style scoped>
.chat-view {
  flex: 1;
  min-width: 0;
  height: 100%;
  display: flex;
  flex-direction: column;
  position: relative;
}

/* ── T-1304: 健康横幅 ─────────────────────────────── */
.health-banner {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  font-size: 12px;
  line-height: 1.4;
  border-bottom: 1px solid var(--border-subtle);
  flex-shrink: 0;
}
.health-banner.error {
  background: rgba(220, 38, 38, 0.08);
  color: var(--color-error);
}
.health-banner.warning {
  background: rgba(217, 119, 6, 0.08);
  color: var(--color-warning);
}
.health-icon {
  width: 16px;
  height: 16px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 10px;
  font-weight: 700;
  flex-shrink: 0;
}
.health-banner.error .health-icon {
  background: var(--color-error);
  color: var(--bg-primary);
}
.health-banner.warning .health-icon {
  background: var(--color-warning);
  color: var(--bg-primary);
}
.health-text {
  flex: 1;
}
.health-fix-btn {
  background: none;
  border: 1px solid currentColor;
  border-radius: 4px;
  padding: 2px 8px;
  font-size: 11px;
  cursor: pointer;
  color: inherit;
  opacity: 0.8;
  transition: opacity 0.15s;
}
.health-fix-btn:hover { opacity: 1; }
.health-dismiss-btn {
  background: none;
  border: none;
  font-size: 16px;
  cursor: pointer;
  color: inherit;
  opacity: 0.5;
  padding: 0 4px;
  line-height: 1;
}
.health-dismiss-btn:hover { opacity: 1; }

/* ── 消息区域 ───────────────────────────────────────── */
.messages-area {
  flex: 1;
  min-height: 0;  /* flex 收缩链：允许消息区域缩到内容以下 */
  overflow-y: auto;
  padding: var(--space-6) var(--space-8);
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
  position: relative;  /* 为绝对定位的 logo 背景层提供锚点 */
}

.view-header {
  position: sticky;
  top: 0;
  z-index: 10;
  text-align: left;
  padding-bottom: var(--space-2);
  pointer-events: none;
}

.view-title {
  display: flex;
  align-items: center;
  height: 36px;
}

.title-bob-logo {
  height: 24px;
  width: 80px;
  background-color: var(--logo-color);
  -webkit-mask-image: url(/bob_logo.svg);
  mask-image: url(/bob_logo.svg);
  -webkit-mask-size: contain;
  mask-size: contain;
  -webkit-mask-repeat: no-repeat;
  mask-repeat: no-repeat;
  -webkit-mask-position: center;
  mask-position: center;
}

/* ── 空状态 ─────────────────────────────────────────── */

/* 背景 logo：脱离文档流，锚定在 messages-area 底部 */
.empty-logo-wrapper {
  position: absolute;
  bottom: 0;
  left: var(--space-8);
  right: var(--space-8);
  display: flex;
  justify-content: center;
  pointer-events: none;
  z-index: 0;
}

.empty-logo-wrapper .empty-bob-logo {
  max-width: 1000px;
  width: 100%;
  pointer-events: auto; /* 复原点击事件，允许点击触发闪念速记 */
}

/* 前景内容层（晨间汇报等） */
.empty-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: flex-end;
  padding-bottom: 8px;
  width: 100%;
  z-index: 1;  /* 在 logo 上方 */
  min-height: 0;  /* 允许 flex 子项收缩到内容以下，防止溢出 */
  overflow: hidden;  /* 裁剪超出容器的内容 */
}

.empty-bob-logo {
  width: 100%;
  /* 保持 bob_bob.svg 的宽高比 152.9:99.9 */
  aspect-ratio: 152.9 / 99.9;
  opacity: 0.05;
  background-color: var(--logo-color);
  -webkit-mask-image: url(/bob_bob.svg);
  mask-image: url(/bob_bob.svg);
  -webkit-mask-size: contain;
  mask-size: contain;
  -webkit-mask-repeat: no-repeat;
  mask-repeat: no-repeat;
  -webkit-mask-position: center;
  mask-position: center;
  display: block;
}

/* ── 消息行（聊天气泡布局）─────────────────────────── */
.message-row {
  display: flex;
  gap: var(--space-2);
  max-width: 1000px;
  width: 100%;
  margin: 0 auto;
  align-items: flex-start;
}

/* 用户消息：头像在左，内容靠左 */
.message-user {
  flex-direction: row;
}

/* Bob 消息：头像在右，内容靠右 */
.message-assistant {
  flex-direction: row-reverse;
}

/* 头像 */
.message-avatar {
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius-sm);
  flex-shrink: 0;
  margin-top: 4px;
  font-size: 12px;
  font-weight: 600;
}

.avatar-user {
  background: var(--surface-input);
  color: var(--text-secondary);
}

.avatar-bob {
  background: var(--surface-glass);
  border: 1px solid var(--border-subtle);
}

.bob-avatar-icon {
  width: 60%;
  height: 60%;
  background-color: var(--logo-color);
  -webkit-mask-image: url(/bob_logo.svg);
  mask-image: url(/bob_logo.svg);
  -webkit-mask-size: contain;
  mask-size: contain;
  -webkit-mask-repeat: no-repeat;
  mask-repeat: no-repeat;
  -webkit-mask-position: center;
  mask-position: center;
}

/* 内容块：最宽占 80%，文字始终左对齐 */
.message-body {
  max-width: 80%;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 0;
  text-align: left;
}

.message-content {
  padding: var(--space-2) 0;
  line-height: 1.6;
  word-break: break-word;
}

/* 用户消息稍暗 */
.message-user .message-content {
  color: var(--text-secondary);
}

/* Bob 回复正常亮度 */
.message-assistant .message-content {
  color: var(--text-primary);
}

.message-content :deep(code) {
  font-family: var(--font-mono);
  font-size: 0.95em;
  padding: 0 2px;
  border-radius: 2px;
  background: transparent;
  color: var(--text-primary);
}

.message-content :deep(strong) {
  font-weight: 600;
  color: var(--text-primary);
}

/* ── 思维链折叠 ─────────────────────────────────────── */
.thinking-card {
  margin-top: var(--space-2);
  border-radius: 0;
  border: none;
  background: transparent;
  border-left: 2px solid var(--border-subtle);
  margin-left: 2px;
}

.thinking-toggle {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  width: auto;
  padding: var(--space-1) var(--space-3);
  border: none;
  background: transparent;
  color: var(--text-muted);
  font-family: var(--font-sans);
  font-size: var(--text-sm);
  cursor: pointer;
  transition: color var(--duration-fast);
}

.thinking-arrow {
  transition: transform var(--duration-fast);
}

.thinking-arrow.expanded {
  transform: rotate(90deg);
}

.thinking-toggle:hover {
  color: var(--text-secondary);
}

.thinking-content {
  padding: var(--space-2) var(--space-4);
  font-size: var(--text-sm);
  color: var(--text-tertiary);
  line-height: 1.6;
  white-space: pre-wrap;
  max-height: 300px;
  overflow-y: auto;
}

/* ── 流式思考动画 ──────────────────────────────────── */
.stream-thinking {
  border-left-color: var(--accent-primary);
}

.thinking-pulse {
  display: inline-block;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--accent-primary);
  animation: pulse-thinking 1.4s ease-in-out infinite;
}

@keyframes pulse-thinking {
  0%, 100% { opacity: 0.3; transform: scale(0.85); }
  50% { opacity: 1; transform: scale(1.15); }
}

.stream-thinking-content {
  max-height: 200px;
  scroll-behavior: smooth;
}

/* 思考状态输入光标动画 */
.typing-cursor {
  display: inline-block;
  width: 6px;
  height: 14px;
  background-color: var(--accent-primary);
  margin-left: 4px;
  vertical-align: middle;
  animation: blink-cursor 1s step-end infinite;
}

@keyframes blink-cursor {
  0%, 100% { opacity: 1; }
  50% { opacity: 0; }
}

/* ── 文件卡片 ───────────────────────────────────────── */
.message-content a[href^="file://"],
.message-content a[href^="C:\\"],
.message-content a[href^="D:\\"],
.message-content a[href^="E:\\"],
.message-content a[href^="F:\\"] {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  background: color-mix(in srgb, var(--accent-primary) 10%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent-primary) 30%, transparent);
  border-radius: 6px;
  text-decoration: none;
  color: var(--accent-primary);
  font-weight: 500;
  font-size: 13px;
  margin: 4px 0;
  transition: all 0.2s;
}
.message-content a[href^="file://"]:hover,
.message-content a[href^="C:\\"]:hover,
.message-content a[href^="D:\\"]:hover,
.message-content a[href^="E:\\"]:hover,
.message-content a[href^="F:\\"]:hover {
  background: color-mix(in srgb, var(--accent-primary) 20%, transparent);
}
.message-content a[href^="file://"]::before,
.message-content a[href^="C:\\"]::before,
.message-content a[href^="D:\\"]::before,
.message-content a[href^="E:\\"]::before,
.message-content a[href^="F:\\"]::before {
  content: "";
  display: inline-block;
  width: 14px;
  height: 14px;
  background-image: url('data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="%236366f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path><polyline points="14 2 14 8 20 8"></polyline></svg>');
  background-size: cover;
}

/* ── 消息图片 ───────────────────────────────────────── */
.message-image {
  border-radius: var(--radius-md);
  overflow: hidden;
}

.message-image img {
  width: auto;
  height: auto;
  max-width: 300px;
  max-height: 240px;
  display: block;
  object-fit: contain;
  border-radius: var(--radius-sm);
}

/* ── 图片预览条 ─────────────────────────────────────── */
.image-preview-bar {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-8);
  background: var(--bg-secondary);
  border-top: 1px solid var(--border-subtle);
}

.image-preview-thumb {
  position: relative;
  width: 48px;
  height: 48px;
  border-radius: var(--radius-sm);
  overflow: hidden;
  border: 1px solid var(--border-default);
}

.image-preview-thumb img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.image-remove {
  position: absolute;
  top: -4px;
  right: -4px;
  width: 20px;
  height: 20px;
  font-size: 10px;
  background: var(--color-error);
  color: var(--text-inverse);
  border-radius: 50%;
}

.image-hint {
  font-size: var(--text-sm);
  color: var(--text-tertiary);
}

/* ── 错误卡片 ─────────────────────────────────────── */
.error-card {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 12px 16px;
  background: color-mix(in srgb, var(--color-error, #ef4444) 8%, var(--bg-secondary));
  border: 1px solid color-mix(in srgb, var(--color-error, #ef4444) 20%, transparent);
  border-radius: 8px;
  margin: 4px 0;
}

.error-icon {
  width: 22px;
  height: 22px;
  border-radius: 50%;
  background: var(--color-error, #ef4444);
  color: var(--text-inverse);
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 800;
  font-size: 13px;
  flex-shrink: 0;
  line-height: 1;
}

.error-title {
  font-weight: 600;
  font-size: 13px;
  color: var(--text-primary);
  margin-bottom: 2px;
}

.error-detail {
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.5;
  word-break: break-word;
}

/* ── 工具调用面板 ─────────────────────────────────────── */
.tool-calls-panel {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin: 8px 0;
}

/* CDN 上传进度条 */
.cdn-upload-progress {
  margin: 8px 0;
  padding: 10px 14px;
  background: var(--bg-secondary);
  border-radius: 8px;
  border-left: 2px solid var(--accent-primary);
  animation: fadeIn 0.2s ease;
}
.cdn-upload-info {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--text-secondary);
  margin-bottom: 6px;
}
.cdn-upload-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-primary);
  font-weight: 500;
}
.cdn-upload-percent {
  font-variant-numeric: tabular-nums;
  color: var(--accent-primary);
  font-weight: 600;
  min-width: 36px;
  text-align: right;
}
.cdn-upload-bar-track {
  width: 100%;
  height: 4px;
  background: var(--bg-tertiary);
  border-radius: 2px;
  overflow: hidden;
}
.cdn-upload-bar-fill {
  height: 100%;
  background: var(--accent-primary);
  border-radius: 2px;
  transition: width 0.15s ease;
}
.cdn-upload-detail {
  font-size: 11px;
  color: var(--text-tertiary);
  margin-top: 4px;
  font-variant-numeric: tabular-nums;
}
@keyframes fadeIn {
  from { opacity: 0; transform: translateY(-4px); }
  to { opacity: 1; transform: translateY(0); }
}

.tool-call-item {
  border-left: 2px solid var(--border-subtle);
  padding: 4px 0 4px 12px;
  font-size: 12px;
  transition: border-color 0.2s ease;
}

.tool-call-item.is-running {
  border-left-color: var(--accent-primary);
}

.tool-call-header {
  display: flex;
  align-items: center;
  gap: 6px;
  cursor: pointer;
  user-select: none;
}

.tool-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
}

.dot-running {
  background: var(--accent-primary);
  animation: dot-blink 1s ease-in-out infinite;
}

.dot-done {
  background: var(--accent-primary);
}

@keyframes dot-blink {
  0%, 100% { opacity: 0.3; }
  50% { opacity: 1; }
}

.tool-name {
  color: var(--text-secondary);
  font-family: var(--font-mono, monospace);
  font-size: 12px;
}

.tool-expand-icon {
  color: var(--text-tertiary);
  transition: transform 0.15s ease;
  flex-shrink: 0;
  margin-left: auto;
}

.tool-expand-icon.rotate-90 {
  transform: rotate(90deg);
}

.tool-result-preview {
  margin-top: 6px;
  padding: 8px 10px;
  background: var(--bg-secondary);
  border-radius: 6px;
  font-size: 11px;
  font-family: var(--font-mono, monospace);
  color: var(--text-tertiary);
  white-space: pre-wrap;
  word-break: break-all;
  line-height: 1.4;
}

.tool-result-text {
  overflow: hidden;
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 4;
}

.tool-result-text.is-expanded {
  display: block;
  -webkit-line-clamp: unset;
  max-height: 400px;
  overflow-y: auto;
}

.btn-expand-result {
  margin-top: 6px;
  font-size: 10px;
  padding: 2px 6px;
  border-radius: 4px;
  background: transparent;
  border: 1px solid var(--border-subtle);
  color: var(--text-tertiary);
  cursor: pointer;
  display: inline-block;
  transition: all var(--duration-fast) var(--ease-out);
}

.btn-expand-result:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
  border-color: var(--border-default);
}

/* ── 拖拽遮罩 ───────────────────────────────────────── */
.drop-overlay {
  position: absolute;
  inset: 0;
  background: color-mix(in srgb, var(--bg-primary) 80%, transparent);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
  backdrop-filter: blur(4px);
  outline: 2px dashed var(--accent-primary);
  outline-offset: -12px;
}

.drop-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-3);
  color: var(--accent-tertiary);
  font-size: var(--text-xl);
}

.drop-icon {
  font-size: 3rem;
}

/* ── 聊天头部 ───────────────────────────────────────── */
.view-header {
  max-width: 1000px;
  width: 100%;
  margin: 0 auto;
  padding: 0 0 var(--space-6) 0;
  text-align: left;
}

.view-title {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-2xl);
  font-weight: 600;
  color: var(--text-primary);
}

/* ── 输入区 ─────────────────────────────────────────── */
.quick-actions-bar {
  position: absolute;
  bottom: 100%;
  right: var(--space-8);
  display: flex;
  justify-content: flex-end;
  align-items: center;
  margin-bottom: var(--space-2);
  z-index: 100;
}

.actions-spacer {
  flex: 1;
}

.model-switcher-wrap {
  position: relative;
}

/* ── 统一工具栏项基线 ────────────────────────────── */
.toolbar-item {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 22px;
  padding: 0 6px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-tertiary);
  font-size: var(--text-xs);
  font-family: var(--font-sans);
  cursor: pointer;
  transition: all var(--duration-fast);
  white-space: nowrap;
  flex-shrink: 0;
}

.toolbar-item:hover {
  color: var(--text-secondary);
  background: var(--surface-glass);
}

.chevron-icon {
  opacity: 0.5;
}

.model-logo-sm {
  width: 12px;
  height: 12px;
  object-fit: contain;
  border-radius: 2px;
}

/* ── 模型切换弹窗 ─────────────────────────────────── */
.model-popup {
  position: absolute;
  bottom: calc(100% + 6px);
  left: 0;
  width: max-content;
  max-width: 450px;
  min-width: 300px;
  background: var(--bg-primary);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-lg);
  z-index: 200;
  overflow: hidden;
}

.model-popup-cols {
  display: flex;
  height: 280px;
}

/* 左侧：供应商列表 */
.model-popup-providers {
  width: 140px;
  flex-shrink: 0;
  border-right: 1px solid var(--border-subtle);
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-gutter: stable;
  padding: var(--space-1);
  background: var(--bg-secondary);
}

.model-provider-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 5px 8px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  text-align: left;
  cursor: pointer;
  transition: all var(--duration-fast);
  overflow: hidden;
}

.model-provider-name {
  flex: 1;
  white-space: nowrap;
  text-overflow: ellipsis;
  overflow: hidden;
  min-width: 0;
}

.model-provider-btn:hover {
  background: var(--surface-glass);
  color: var(--text-primary);
}

.model-provider-btn.active {
  background: var(--bg-tertiary);
  color: var(--text-primary);
  font-weight: 600;
}

.provider-count {
  margin-left: auto;
  font-size: 10px;
  color: var(--text-tertiary);
}

/* 右侧：模型列表 */
.model-popup-models {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-gutter: stable;
  padding: var(--space-1);
  min-width: 0;
}

.model-option {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 10px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  text-align: left;
  cursor: pointer;
  transition: all var(--duration-fast);
}

.model-option:hover {
  background: var(--surface-glass);
  color: var(--text-primary);
}

.model-option.active {
  background: var(--bg-tertiary);
  color: var(--text-primary);
}

.model-option-label {
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.model-option-empty {
  padding: 8px 10px;
  color: var(--text-tertiary);
  font-size: var(--text-xs);
  text-align: center;
}

.btn-parse-event {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  background: transparent;
  color: var(--text-secondary);
  border: none;
  padding: 4px 12px;
  font-size: var(--text-xs);
  cursor: pointer;
  transition: color 0.2s;
  margin-left: auto;
}

.btn-parse-event:hover:not(:disabled) {
  color: var(--text-primary);
}

.btn-parse-event:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.input-area {
  position: relative;
  padding: var(--space-4) var(--space-8) var(--space-6);
}

.input-row {
  display: flex;
  flex-direction: column;
  max-width: 1000px;
  width: 100%;
  margin: 0 auto;
  box-sizing: border-box;
  background: var(--surface-card);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  padding: var(--space-2) var(--space-3);
  transition: border-color var(--duration-fast) var(--ease-out);
}

.input-row:focus-within {
  border-color: var(--border-default);
}

/* ── 内联图片预览 ─────────────────────────────────── */
.inline-image-preview {
  position: relative;
  display: inline-block;
  width: max-content;
  padding: 4px 0 6px 0;
}

.inline-files-preview {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  padding: 4px 0 6px 0;
}

.pending-file-chip {
  display: flex;
  align-items: center;
  gap: 6px;
  background: var(--surface-card);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-sm);
  padding: 4px 8px 4px 6px;
  font-size: 11px;
  color: var(--text-secondary);
}

.pending-file-name {
  max-width: 150px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.file-remove-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  padding: 2px;
  border-radius: var(--radius-xs);
}

.file-remove-btn:hover {
  background: var(--surface-hover);
  color: var(--text-primary);
}

.inline-image-preview img {
  width: auto;
  height: auto;
  max-width: 64px;
  max-height: 48px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-subtle);
  object-fit: contain;
}

.image-remove-inline {
  position: absolute;
  top: 0;
  right: -6px;
  width: 16px;
  height: 16px;
  font-size: 8px;
  background: var(--color-error);
  color: var(--text-inverse);
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  border: none;
  opacity: 0;
  transition: opacity 0.2s ease;
  pointer-events: none;
}

.inline-image-preview:hover .image-remove-inline {
  opacity: 1;
  pointer-events: auto;
}

.chat-input {
  width: 100%;
  border: none;
  background: transparent;
  color: var(--text-primary);
  font-family: var(--font-sans);
  font-size: var(--text-base);
  line-height: var(--leading-normal);
  resize: none;
  outline: none;
  padding: var(--space-1) 0;
  min-height: 56px;
  max-height: 160px;
}

.chat-input::placeholder {
  color: var(--text-tertiary);
}

/* ── 底部工具栏 ───────────────────────────────────── */
.input-toolbar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 4px;
  padding-top: var(--space-2);
}

.toolbar-spacer {
  flex: 1;
}

.attach-btn:hover {
  color: var(--text-primary);
}

/* ── 发送/停止按钮（统一方形） ────────────────────── */
.action-btn {
  position: relative;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 17px;
  height: 17px;
  border-radius: 2px;
  border: 1px solid var(--border-default);
  background: transparent;
  cursor: pointer;
  transition: all var(--duration-fast);
}

/* 扩展点击热区，保持视觉大小不变但更容易被点中 */
.action-btn::after {
  content: "";
  position: absolute;
  top: -6px;
  right: -6px;
  bottom: -6px;
  left: -6px;
}

.action-btn:hover {
  border-color: var(--text-secondary);
}

.send-btn:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}

/* CSS 实心三角形 ▶ */
.icon-send {
  display: block;
  width: 0;
  height: 0;
  border-style: solid;
  border-width: 3.5px 0 3.5px 6px;
  border-color: transparent transparent transparent var(--text-primary);
  margin-left: 1px;
}

.send-btn:disabled .icon-send {
  border-left-color: var(--text-tertiary);
}

/* 红色实心方块 ■ */
.icon-stop {
  display: block;
  width: 5px;
  height: 5px;
  border-radius: 0.5px;
  background: var(--color-error);
}

.stop-btn {
  border-color: var(--border-default);
}

.stop-btn:hover {
  border-color: var(--text-secondary);
  background: var(--color-error-bg);
}

/* ── 计费指示器 ───────────────────────────────────── */
.cost-indicator {
  font-family: var(--font-mono);
  cursor: default;
}

/* ── 全局权限开关 ─────────────────────────────────── */
.global-access-toggle {
  opacity: 0.6;
}

.global-access-toggle:hover {
  opacity: 1;
}

.global-access-toggle.active {
  opacity: 1;
}

.global-access-text {
  font-size: inherit;
  color: inherit;
  font-weight: 500;
}
.global-access-toggle.active .global-access-text {
  color: var(--accent-primary);
}

/* ── 消息元数据标注 ── */
.message-meta-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 2px;
}

.model-label, .source-label {
  font-size: 11px;
  color: var(--text-muted);
  opacity: 0.5;
  font-family: var(--font-mono);
  user-select: none;
  letter-spacing: 0.02em;
}

/* 进化引擎: 记忆提炼彩蛋标志 */
.memory-indicator {
  font-size: 12px;
  opacity: 0.6;
  cursor: default;
  user-select: none;
  transition: opacity 0.3s;
}
.memory-indicator:hover {
  opacity: 1;
}

.source-label {
  display: flex;
  align-items: center;
  gap: 4px;
}

/* T-1201: 富文本复制按钮 */
.copy-rich-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  background: none;
  border: none;
  cursor: pointer;
  color: var(--text-muted);
  opacity: 0.3;
  padding: 2px;
  border-radius: var(--radius-xs, 3px);
  transition: opacity 0.2s, color 0.2s, background 0.2s;
  margin-left: auto;
}

.message-body:hover .copy-rich-btn {
  opacity: 0.6;
}

.copy-rich-btn:hover {
  opacity: 1 !important;
  color: var(--text-primary);
  background: var(--surface-input);
}

.copy-done-icon {
  color: var(--accent-primary);
}

/* ── 思考中动画 ───────────────────────────────────── */
.typing-indicator {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: var(--space-2) 0;
  height: 24px;
}

.typing-indicator .dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-muted);
  opacity: 0.4;
  animation: typing-bounce 1.4s ease-in-out infinite;
}

.typing-indicator .dot:nth-child(1) { animation-delay: 0s; }
.typing-indicator .dot:nth-child(2) { animation-delay: 0.2s; }
.typing-indicator .dot:nth-child(3) { animation-delay: 0.4s; }

@keyframes typing-bounce {
  0%, 60%, 100% {
    transform: translateY(0);
    opacity: 0.4;
  }
  30% {
    transform: translateY(-6px);
    opacity: 1;
  }
}

/* ── Bob 可点击元素：闪念速记入口 ── */
.bob-clickable {
  cursor: pointer;
  transition: opacity 0.15s, transform 0.15s;
}
.bob-clickable:hover {
  opacity: 0.7;
}
.bob-clickable:active {
  transform: scale(0.93);
}

.mention-menu {
  position: absolute;
  bottom: 100%;
  left: 16px;
  margin-bottom: 8px;
  background-color: var(--bg-secondary);
  border: 1px solid var(--border-light);
  border-radius: 8px;
  padding: 4px;
  box-shadow: 0 4px 12px var(--shadow-color, rgba(0,0,0,0.1));
  z-index: 100;
  min-width: 200px;
}
.mention-menu-item {
  padding: 8px 12px;
  border-radius: 6px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 13px;
  color: var(--text-primary);
}
.mention-menu-item:hover, .mention-menu-item.active {
  background-color: var(--bg-tertiary);
}
.mention-shortcut {
  font-size: 11px;
  color: var(--text-tertiary);
  background-color: var(--bg-root);
  padding: 2px 6px;
  border-radius: 4px;
}

/* Image Lightbox */
.image-lightbox {
  position: fixed;
  top: 0;
  left: 0;
  width: 100vw;
  height: 100vh;
  background-color: var(--overlay-bg);
  display: flex;
  justify-content: center;
  align-items: center;
  z-index: 9999;
  cursor: zoom-out;
  backdrop-filter: blur(4px);
}
.image-lightbox img {
  max-width: 90vw;
  max-height: 90vh;
  object-fit: contain;
  border-radius: 8px;
  box-shadow: 0 10px 30px var(--shadow-lg);
  cursor: grab;
  transition: transform 0.1s ease-out;
}
.image-lightbox img:active {
  cursor: grabbing;
  transition: none; /* smooth tracking while dragging */
}
.lightbox-close {
  position: absolute;
  top: 20px;
  right: 20px;
  background: transparent;
  border: none;
  color: var(--text-inverse);
  font-size: 40px;
  line-height: 1;
  cursor: pointer;
  opacity: 0.7;
  transition: opacity 0.2s;
}
.lightbox-close:hover {
  opacity: 1;
}

/* ── Mobile Input Alignment (Absolute Positioning) ── */
.mobile-plus-btn {
  position: absolute;
  left: 0;
  top: 2px;
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  color: var(--text-secondary);
  padding: 0;
  flex-shrink: 0;
  cursor: pointer;
  z-index: 10;
}
.mobile-send-btn-wrap {
  position: absolute;
  right: 0;
  top: 2px;
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  z-index: 10;
}
.mobile-mode-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  width: 28px;
  height: 28px;
  cursor: pointer;
  padding: 0;
}
.chat-view.is-mobile .input-row {
  position: relative;
  align-items: flex-end !important;
  padding: 8px 12px;
}
.chat-view.is-mobile .input-wrapper {
  padding-left: 36px;
  padding-right: 36px;
  width: 100%;
  box-sizing: border-box;
}
.chat-view.is-mobile .chat-input {
  display: block;
  box-sizing: border-box;
  min-height: 36px;
  line-height: 24px;
  padding: 6px 0;
}

/* ── Mobile Bottom Sheet ────────────────────────────────────────── */
.mobile-sheet-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.4);
  z-index: 999;
  backdrop-filter: blur(2px);
}
.mobile-bottom-sheet {
  position: fixed;
  bottom: 0;
  left: 0;
  right: 0;
  background: var(--bg-primary);
  border-top: 1px solid var(--border-subtle);
  z-index: 1000;
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  padding-bottom: env(safe-area-inset-bottom);
  box-shadow: 0 -10px 30px rgba(0, 0, 0, 0.25);
  transition: all 0.3s ease;
}
.sheet-header {
  height: 48px;
  display: flex;
  align-items: center;
  justify-content: center;
  position: relative;
  flex-shrink: 0;
  border-bottom: 1px solid var(--border-subtle);
}
.sheet-drag-handle {
  width: 40px;
  height: 4px;
  background: var(--border-strong);
  border-radius: 2px;
  opacity: 0.6;
}
.sheet-back-btn {
  position: absolute;
  left: 12px;
  display: flex;
  align-items: center;
  background: transparent;
  border: none;
  color: var(--text-primary);
  font-size: 14px;
  padding: 8px;
  cursor: pointer;
}
.sheet-content {
  flex: 1;
  overflow-y: auto;
  padding: 0 16px 24px;
}
.main-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 16px;
  padding-top: 12px;
}
.sheet-grid-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  background: transparent;
  border: none;
  color: var(--text-primary);
  font-size: 11px;
  cursor: pointer;
}
.grid-icon-wrap {
  width: 56px;
  height: 56px;
  border-radius: 16px;
  background: var(--bg-secondary);
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-primary);
  transition: background 0.2s;
}
.sheet-grid-item:active .grid-icon-wrap {
  background: var(--surface-glass);
}
.list-view {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding-top: 8px;
}
.sheet-list-item {
  display: flex;
  align-items: center;
  padding: 12px 16px;
  background: var(--bg-secondary);
  border: none;
  border-radius: var(--radius-md);
  color: var(--text-primary);
  cursor: pointer;
  text-align: left;
}
.sheet-list-item.active {
  border: 1px solid var(--accent-primary);
  background: rgba(var(--accent-primary-rgb), 0.1);
}
.item-info {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  flex: 1;
  margin-left: 12px;
}
.item-name {
  font-size: 15px;
  font-weight: 500;
}
.item-count {
  font-size: 12px;
  color: var(--text-tertiary);
  margin-top: 2px;
}
.text-tertiary {
  color: var(--text-tertiary);
}
.text-accent {
  color: var(--accent-primary);
}
.sheet-empty {
  padding: 24px;
  text-align: center;
  color: var(--text-tertiary);
}
.slide-up-enter-active,
.slide-up-leave-active {
  transition: transform 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}
.slide-up-enter-from,
.slide-up-leave-to {
  transform: translateY(100%);
}



/* ── Mobile Bottom Sheet ────────────────────────────────────────── */

/* ── Boarding Pass Confirmation Card ───────────────────────────── */
.boarding-pass-card {
  background: var(--bg-secondary);
  border: 1px solid var(--user-accent, #4f8cf7);
  border-radius: 12px;
  padding: 16px;
  margin-bottom: 10px;
  animation: slideUp 0.3s ease-out;
}
@keyframes slideUp {
  from { opacity: 0; transform: translateY(12px); }
  to { opacity: 1; transform: translateY(0); }
}
.bp-header {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 12px;
}
.bp-icon { font-size: 1.3em; }
.bp-title {
  font-weight: 600;
  color: var(--text-primary);
  font-size: 0.95em;
}
.bp-route {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
  padding: 10px 0;
}
.bp-airport { text-align: center; flex: 0 0 auto; }
.bp-code {
  font-size: 1.6em;
  font-weight: 700;
  color: var(--text-primary);
  letter-spacing: 2px;
}
.bp-flight-info {
  display: flex;
  flex-direction: column;
  align-items: center;
  flex: 1;
  gap: 2px;
}
.bp-flight-number {
  font-size: 0.85em;
  font-weight: 600;
  color: var(--user-accent, #4f8cf7);
}
.bp-arrow {
  font-size: 1.2em;
  color: var(--text-tertiary);
}
.bp-date {
  font-size: 0.8em;
  color: var(--text-secondary);
}
.bp-details {
  display: flex;
  gap: 16px;
  margin-bottom: 14px;
  padding: 8px 12px;
  background: var(--bg-tertiary);
  border-radius: 8px;
}
.bp-detail-item {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1;
}
.bp-label {
  font-size: 0.7em;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 1px;
}
.bp-value {
  font-size: 0.9em;
  font-weight: 600;
  color: var(--text-primary);
}
.bp-actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}
.bp-btn {
  padding: 6px 16px;
  border-radius: 8px;
  font-size: 0.85em;
  font-weight: 500;
  cursor: pointer;
  border: none;
  transition: all 0.2s ease;
}
.bp-btn-confirm {
  background: var(--user-accent, #4f8cf7);
  color: #fff;
}
.bp-btn-confirm:hover {
  filter: brightness(1.1);
}
.bp-btn-dismiss {
  background: var(--bg-tertiary);
  color: var(--text-secondary);
}
.bp-btn-dismiss:hover {
  background: var(--bg-primary);
}
</style>
