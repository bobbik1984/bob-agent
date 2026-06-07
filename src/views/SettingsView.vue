<template>
  <div class="settings-view">
    <div class="settings-scroll">
      <div class="settings-content">
      <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: var(--space-4);">
        <h2 class="settings-title" style="margin-bottom: 0;">
          <SettingsIcon :size="22" class="title-icon" />
          {{ $t('settings.title') }}
        </h2>
        <button class="btn btn-ghost" style="display: flex; align-items: center; gap: 6px; font-size: 0.85em; color: var(--text-tertiary);" @click="toggleAdvancedMode">
          <Eye v-if="isAdvancedMode" :size="14" />
          <EyeOff v-else :size="14" />
          <span>{{ isAdvancedMode ? $t('settings.simple_mode') : $t('settings.advanced_mode') }}</span>
        </button>
      </div>

      <!-- 模型中心 (ModelHub) — 自动发现，替代旧的手填配置 -->
      <ModelHub ref="modelHubRef" @model-changed="emit('config-changed')" />

      <!-- 离线推理引擎 (Offline Engine) -->
      <section v-show="isAdvancedMode" class="settings-section card">
        <h3 class="section-title">
          <Server :size="16" class="section-icon" />
          {{ $t('settings.offline_engine') }}
        </h3>
        <p class="section-desc" style="margin-bottom: 12px;">{{ $t('settings.offline_engine_desc') }}</p>
        
        <div class="form-group workspace-group">
          <input
            v-model="config.offlineModelPath"
            class="input"
            :placeholder="$t('settings.offline_model_placeholder')"
            readonly
          />
          <button class="btn btn-primary browse-btn" @click="selectOfflineModel">
            <FolderOpen :size="14" />
            <span>{{ $t('settings.browse') }}</span>
          </button>
        </div>

        <div style="margin-top: 8px; margin-bottom: 12px;">
          <button class="btn btn-ghost" style="font-size: 0.85em; padding: 4px 0; color: var(--accent-primary); display: flex; align-items: center; gap: 4px;" @click="showLlamaGuide = !showLlamaGuide">
            <Info :size="12" />
            <span>{{ showLlamaGuide ? $t('settings.hide_llama_guide') : $t('settings.show_llama_guide') }}</span>
          </button>
        </div>

        <Transition name="briefing-fade">
          <div v-if="showLlamaGuide" class="card" style="background: var(--bg-secondary); border: 1px dashed var(--border-subtle); padding: 16px; border-radius: var(--radius-md); margin-bottom: 16px; font-size: 0.9em; box-shadow: none;">
            <h4 style="font-size: 1.05em; font-weight: 600; color: var(--text-primary); margin-bottom: 8px;">{{ $t('settings.llama_guide_title') }}</h4>
            <p style="color: var(--text-secondary); margin-bottom: 8px; line-height: 1.5;" v-html="$t('settings.llama_guide_desc')"></p>

            <button class="btn btn-primary" style="display: flex; align-items: center; gap: 6px; font-size: 0.9em; padding: 6px 12px;" @click="openLlamaEngineDir">
              <FolderOpen :size="14" />
              <span>{{ $t('settings.open_llama_dir') }}</span>
            </button>
          </div>
        </Transition>
        
        <div style="display: flex; gap: 8px; align-items: center; margin-top: 12px;">
          <button 
            class="btn" 
            :class="offlineEngineStatus === 'running' ? 'btn-danger' : 'btn-primary'" 
            @click="toggleOfflineEngine"
            :disabled="!config.offlineModelPath"
          >
            <Server :size="14" />
            <span>{{ offlineEngineStatus === 'running' ? $t('settings.offline_engine_stop') : $t('settings.offline_engine_start') }}</span>
          </button>
          
          <span style="font-size: 0.85em; display: flex; align-items: center; gap: 6px;" :style="{ color: offlineEngineStatus === 'running' ? 'var(--accent-primary)' : 'var(--text-tertiary)' }">
            <span class="status-dot" :style="{ background: offlineEngineStatus === 'running' ? 'var(--accent-primary)' : 'var(--text-tertiary)' }" style="width: 8px; height: 8px; border-radius: 50%; display: inline-block;"></span>
            {{ offlineEngineStatus === 'running' ? $t('settings.offline_engine_running') : $t('settings.offline_engine_stopped') }}
          </span>
        </div>
      </section>

      <!-- 移动助手 (Mobile Assistant) -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Smartphone :size="16" class="section-icon" />
          {{ $t('settings.mobile_assistant') }}
        </h3>
        <p class="section-desc" style="margin-bottom: 16px;">{{ $t('settings.mobile_assistant_desc') }}</p>
        
        <div class="channel-selector" style="display: flex; gap: 8px; margin-bottom: 16px; background: var(--bg-secondary); padding: 4px; border-radius: 8px;">
          <button class="channel-btn" :class="{ active: mobileChannel === 'wechat' }" @click="mobileChannel = 'wechat'">
            <svg viewBox="-51.45 -69.25 445.9 415.5" xmlns="http://www.w3.org/2000/svg" style="width: 14px; height: 14px; fill: currentColor; flex-shrink: 0; display: inline-block; vertical-align: middle; margin-right: 4px;">
              <g fill="currentColor" fill-rule="evenodd">
                <path d="M274 167c-7.778 0-14-6.222-14-14s6.222-14 14-14 14 6.222 14 14c0 7.389-6.222 14-14 14m-69 0c-7.778 0-14-6.222-14-14s6.222-14 14-14 14 6.222 14 14c0 7.389-6.222 14-14 14m102.39 78.581C329.216 229.871 343 206.5 343 180.827 343 133.316 297.052 95 240 95s-103 38.316-103 85.827c0 47.512 45.948 85.828 103 85.828 11.87 0 22.974-1.533 33.695-4.598.766-.383 1.915-.383 3.063-.383 1.915 0 3.83.766 5.361 1.532l22.591 13.028c.766.383 1.149.766 1.915.766a3.433 3.433 0 003.446-3.448c0-.767-.383-1.533-.383-2.683 0-.383-3.063-10.728-4.595-17.242-.383-.766-.383-1.532-.383-2.299-.383-2.682.766-4.597 2.68-5.747"/>
                <path d="M164 86c-8.93 0-16-7.07-16-16s7.07-16 16-16 16 7.07 16 16c0 8.558-7.07 16-16 16m-82 0c-8.93 0-16-7.07-16-16s7.07-16 16-16 16 7.07 16 16c0 8.558-7.07 16-16 16m41.96-86C55.646 0 0 45.895 0 102.88c0 30.98 16.502 58.899 42.983 77.64 1.919 1.53 3.454 3.824 3.454 6.884 0 .764-.384 1.912-.384 2.677-1.919 7.649-5.373 20.27-5.757 20.652-.383 1.148-.767 1.913-.767 3.06 0 2.295 1.919 4.207 4.221 4.207.768 0 1.535-.382 2.303-.765l27.248-15.68c1.919-1.148 4.222-1.913 6.524-1.913 1.152 0 2.303 0 3.454.383 12.665 3.442 26.48 5.736 40.297 5.736h6.908c-2.687-8.031-4.222-16.445-4.222-25.242 0-51.631 50.658-93.701 112.83-93.701H246C237.173 37.48 185.747 0 123.96 0"/>
              </g>
            </svg>
            {{ $t('settings.channel_wechat') }}
          </button>
          <button class="channel-btn" :class="{ active: mobileChannel === 'telegram' }" @click="mobileChannel = 'telegram'">
            <svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg" style="width: 14px; height: 14px; fill: currentColor; flex-shrink: 0; display: inline-block; vertical-align: middle; margin-right: 4px;">
              <path d="M29.919 6.163l-4.225 19.925c-0.319 1.406-1.15 1.756-2.331 1.094l-6.438-4.744-3.106 2.988c-0.344 0.344-0.631 0.631-1.294 0.631l0.463-6.556 11.931-10.781c0.519-0.462-0.113-0.719-0.806-0.256l-14.75 9.288-6.35-1.988c-1.381-0.431-1.406-1.381 0.288-2.044l24.837-9.569c1.15-0.431 2.156 0.256 1.781 2.013z"/>
            </svg>
            {{ $t('settings.channel_telegram') }}
          </button>
          <button class="channel-btn" :class="{ active: mobileChannel === 'discord' }" @click="mobileChannel = 'discord'">
            <svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" style="width: 14px; height: 14px; fill: currentColor; flex-shrink: 0; display: inline-block; vertical-align: middle; margin-right: 4px;">
              <path d="M18.59 5.88997C17.36 5.31997 16.05 4.89997 14.67 4.65997C14.5 4.95997 14.3 5.36997 14.17 5.69997C12.71 5.47997 11.26 5.47997 9.83001 5.69997C9.69001 5.36997 9.49001 4.95997 9.32001 4.65997C7.94001 4.89997 6.63001 5.31997 5.40001 5.88997C2.92001 9.62997 2.25001 13.28 2.58001 16.87C4.23001 18.1 5.82001 18.84 7.39001 19.33C7.78001 18.8 8.12001 18.23 8.42001 17.64C7.85001 17.43 7.31001 17.16 6.80001 16.85C6.94001 16.75 7.07001 16.64 7.20001 16.54C10.33 18 13.72 18 16.81 16.54C16.94 16.65 17.07 16.75 17.21 16.85C16.7 17.16 16.15 17.42 15.59 17.64C15.89 18.23 16.23 18.8 16.62 19.33C18.19 18.84 19.79 18.1 21.43 16.87C21.82 12.7 20.76 9.08997 18.61 5.88997H18.59ZM8.84001 14.67C7.90001 14.67 7.13001 13.8 7.13001 12.73C7.13001 11.66 7.88001 10.79 8.84001 10.79C9.80001 10.79 10.56 11.66 10.55 12.73C10.55 13.79 9.80001 14.67 8.84001 14.67ZM15.15 14.67C14.21 14.67 13.44 13.8 13.44 12.73C13.44 11.66 14.19 10.79 15.15 10.79C16.11 10.79 16.87 11.66 16.86 12.73C16.86 13.79 16.11 14.67 15.15 14.67Z"/>
            </svg>
            {{ $t('settings.channel_discord') }}
          </button>
        </div>

        <div v-if="mobileChannel === 'wechat'" class="channel-content">
          <div style="display: flex; gap: 8px; align-items: center;">
            <button 
              class="btn" 
              :class="wechatConnected ? 'btn-danger' : 'btn-primary'" 
              @click="openWechatModal"
              style="display: flex; align-items: center; gap: 6px;"
            >
              <svg viewBox="-51.45 -69.25 445.9 415.5" xmlns="http://www.w3.org/2000/svg" style="width: 14px; height: 14px; fill: currentColor; flex-shrink: 0;">
                <g fill="currentColor" fill-rule="evenodd">
                  <path d="M274 167c-7.778 0-14-6.222-14-14s6.222-14 14-14 14 6.222 14 14c0 7.389-6.222 14-14 14m-69 0c-7.778 0-14-6.222-14-14s6.222-14 14-14 14 6.222 14 14c0 7.389-6.222 14-14 14m102.39 78.581C329.216 229.871 343 206.5 343 180.827 343 133.316 297.052 95 240 95s-103 38.316-103 85.827c0 47.512 45.948 85.828 103 85.828 11.87 0 22.974-1.533 33.695-4.598.766-.383 1.915-.383 3.063-.383 1.915 0 3.83.766 5.361 1.532l22.591 13.028c.766.383 1.149.766 1.915.766a3.433 3.433 0 003.446-3.448c0-.767-.383-1.533-.383-2.683 0-.383-3.063-10.728-4.595-17.242-.383-.766-.383-1.532-.383-2.299-.383-2.682.766-4.597 2.68-5.747"/>
                  <path d="M164 86c-8.93 0-16-7.07-16-16s7.07-16 16-16 16 7.07 16 16c0 8.558-7.07 16-16 16m-82 0c-8.93 0-16-7.07-16-16s7.07-16 16-16 16 7.07 16 16c0 8.558-7.07 16-16 16m41.96-86C55.646 0 0 45.895 0 102.88c0 30.98 16.502 58.899 42.983 77.64 1.919 1.53 3.454 3.824 3.454 6.884 0 .764-.384 1.912-.384 2.677-1.919 7.649-5.373 20.27-5.757 20.652-.383 1.148-.767 1.913-.767 3.06 0 2.295 1.919 4.207 4.221 4.207.768 0 1.535-.382 2.303-.765l27.248-15.68c1.919-1.148 4.222-1.913 6.524-1.913 1.152 0 2.303 0 3.454.383 12.665 3.442 26.48 5.736 40.297 5.736h6.908c-2.687-8.031-4.222-16.445-4.222-25.242 0-51.631 50.658-93.701 112.83-93.701H246C237.173 37.48 185.747 0 123.96 0"/>
                </g>
              </svg>
              <span>{{ wechatConnected ? $t('settings.wechat_rebind') : $t('settings.wechat_scan') }}</span>
            </button>
            
            <span style="font-size: 0.85em; display: flex; align-items: center; gap: 6px;" :style="{ color: wechatConnected ? 'var(--accent-primary)' : 'var(--text-tertiary)' }">
              <span class="status-dot" :style="{ background: wechatConnected ? 'var(--accent-primary)' : 'var(--text-tertiary)' }" style="width: 8px; height: 8px; border-radius: 50%; display: inline-block;"></span>
              {{ wechatConnected ? $t('settings.wechat_connected') : $t('settings.wechat_disconnected') }}
            </span>
          </div>
        </div>
        
        <div v-else-if="mobileChannel === 'telegram'" class="channel-content">
          <div class="input-group">
            <label>Bot Token</label>
            <div style="display: flex; gap: 8px;">
              <input type="password" v-model="tgToken" placeholder="123456789:ABCdefGHIjklMNO..." class="settings-input" style="flex: 1;" />
              <button class="btn btn-primary" @click="activateMobileChannel('telegram')">
                <svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg" style="width: 14px; height: 14px; fill: currentColor; flex-shrink: 0;"><path d="M29.919 6.163l-4.225 19.925c-0.319 1.406-1.15 1.756-2.331 1.094l-6.438-4.744-3.106 2.988c-0.344 0.344-0.631 0.631-1.294 0.631l0.463-6.556 11.931-10.781c0.519-0.462-0.113-0.719-0.806-0.256l-14.75 9.288-6.35-1.988c-1.381-0.431-1.406-1.381 0.288-2.044l24.837-9.569c1.15-0.431 2.156 0.256 1.781 2.013z"/></svg> {{ $t('settings.channel_activate') }}
              </button>
            </div>
            <p class="field-hint" style="margin-top: 8px;">{{ $t('settings.channel_tg_hint') }}</p>
          </div>
        </div>
        
        <div v-else-if="mobileChannel === 'discord'" class="channel-content">
          <div class="input-group">
            <label>Bot Token</label>
            <div style="display: flex; gap: 8px;">
              <input type="password" v-model="discordToken" placeholder="OTg3NjU0MzIx.ABC.defGHIjklMNO..." class="settings-input" style="flex: 1;" />
              <button class="btn btn-primary" @click="activateMobileChannel('discord')">
                <svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" style="width: 14px; height: 14px; fill: currentColor; flex-shrink: 0;"><path d="M18.59 5.88997C17.36 5.31997 16.05 4.89997 14.67 4.65997C14.5 4.95997 14.3 5.36997 14.17 5.69997C12.71 5.47997 11.26 5.47997 9.83001 5.69997C9.69001 5.36997 9.49001 4.95997 9.32001 4.65997C7.94001 4.89997 6.63001 5.31997 5.40001 5.88997C2.92001 9.62997 2.25001 13.28 2.58001 16.87C4.23001 18.1 5.82001 18.84 7.39001 19.33C7.78001 18.8 8.12001 18.23 8.42001 17.64C7.85001 17.43 7.31001 17.16 6.80001 16.85C6.94001 16.75 7.07001 16.64 7.20001 16.54C10.33 18 13.72 18 16.81 16.54C16.94 16.65 17.07 16.75 17.21 16.85C16.7 17.16 16.15 17.42 15.59 17.64C15.89 18.23 16.23 18.8 16.62 19.33C18.19 18.84 19.79 18.1 21.43 16.87C21.82 12.7 20.76 9.08997 18.61 5.88997H18.59ZM8.84001 14.67C7.90001 14.67 7.13001 13.8 7.13001 12.73C7.13001 11.66 7.88001 10.79 8.84001 10.79C9.80001 10.79 10.56 11.66 10.55 12.73C10.55 13.79 9.80001 14.67 8.84001 14.67ZM15.15 14.67C14.21 14.67 13.44 13.8 13.44 12.73C13.44 11.66 14.19 10.79 15.15 10.79C16.11 10.79 16.87 11.66 16.86 12.73C16.86 13.79 16.11 14.67 15.15 14.67Z"/></svg> {{ $t('settings.channel_activate') }}
              </button>
            </div>
            <p class="field-hint" style="margin-top: 8px;">{{ $t('settings.channel_discord_hint') }}</p>
          </div>
        </div>
      </section>

      <!-- API 密钥管理 (Credential Store) -->
      <details v-show="isAdvancedMode" class="settings-section card custom-model-override">
        <summary class="section-title" style="cursor: pointer; display: flex; align-items: center; justify-content: space-between; margin-bottom: 0;">
          <div style="display: flex; align-items: center; gap: 8px;">
            <Key :size="16" class="section-icon" style="opacity: 0.6;" />
            {{ $t('settings.api_keys_title') }}
          </div>
          <ChevronDown :size="16" class="details-chevron" />
        </summary>
        <p class="section-desc" style="margin-top: 16px; margin-bottom: 16px;">{{ $t('settings.api_keys_desc') }}</p>
        
        <!-- T-821: Outbox 引导提示 -->
        <div style="margin-bottom: 16px; padding: 10px 14px; border-radius: 8px; background: rgba(var(--user-accent-rgb, 99,102,241), 0.08); border: 1px solid rgba(var(--user-accent-rgb, 99,102,241), 0.2); font-size: 0.82em; color: var(--text-secondary); line-height: 1.5;" v-html="$t('settings.outbox_hint')">
        </div>

        <!-- 模型供应商密钥 -->
        <h4 style="margin-bottom: 8px; font-size: 0.85em; color: var(--text-secondary);">{{ $t('settings.provider_keys_title') }}</h4>
        <div style="display: flex; flex-direction: column; gap: 6px; margin-bottom: 20px;">
          <div class="form-group" v-for="provider in modelProviders" :key="provider.id" style="display: flex; align-items: center; gap: 12px; border-bottom: 1px solid var(--border-subtle); padding-bottom: 6px;">
            <label class="form-label" style="width: 160px; margin-bottom: 0; display: flex; align-items: center; gap: 8px;">
              <img v-if="getProviderLogo(provider.id)" :src="getProviderLogo(provider.id)" style="width: 16px; height: 16px; object-fit: contain; border-radius: 2px;" />
              <span style="flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;" :title="$te('providers.' + provider.id) ? $t('providers.' + provider.id) : provider.name">{{ $te('providers.' + provider.id) ? $t('providers.' + provider.id) : provider.name }}</span>
            </label>

            <!-- Vertex AI: 凭证文件上传模式 -->
            <template v-if="provider.id === 'vertex_ai'">
              <span class="status-dot" :style="{ background: gcpCredStatus.configured ? 'var(--accent-primary)' : 'transparent', border: gcpCredStatus.configured ? '2px solid var(--accent-primary)' : '2px solid var(--text-tertiary)' }" style="width: 10px; height: 10px; border-radius: 50%; display: inline-block; flex-shrink: 0;"></span>
              <span v-if="gcpCredStatus.configured" style="flex: 1; font-size: 0.82em; color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;" :title="gcpCredStatus.client_email">
                ✅ {{ gcpCredStatus.project_id }} ({{ gcpCredStatus.client_email }})
              </span>
              <span v-else style="flex: 1; font-size: 0.82em; color: var(--text-tertiary);">{{ $t('settings.not_configured') }}</span>
              <button class="btn btn-primary" @click="uploadGcpCredential" style="padding: 4px 10px; font-size: 0.9em; white-space: nowrap;">{{ gcpCredStatus.configured ? '更换凭证' : '上传凭证' }}</button>
              <button v-if="gcpCredStatus.configured" class="btn" @click="testGcpCredential" style="padding: 4px 10px; font-size: 0.9em; white-space: nowrap;">测试</button>
              <button class="btn-icon btn-remove-key" :style="{ visibility: gcpCredStatus.configured ? 'visible' : 'hidden' }" @click="removeGcpCredential" :title="$t('settings.delete_key')">
                <X :size="14" />
              </button>
            </template>

            <!-- 常规 API Key 输入模式 -->
            <template v-else>
              <span class="status-dot" :style="{ background: provider.hasKey ? 'var(--accent-primary)' : 'transparent', border: provider.hasKey ? '2px solid var(--accent-primary)' : '2px solid var(--text-tertiary)' }" style="width: 10px; height: 10px; border-radius: 50%; display: inline-block; flex-shrink: 0;"></span>
              <input 
                v-model="apiKeys[provider.id]" 
                type="password" 
                class="input" 
                :placeholder="provider.hasKey ? $t('settings.configured') : $t('settings.not_configured')" 
                style="flex: 1;" 
              />
              <button class="btn btn-primary" @click="saveApiKey(provider.id)" style="padding: 4px 10px; font-size: 0.9em;">{{ $t('settings.save') }}</button>
              <button class="btn-icon btn-remove-key" :style="{ visibility: provider.hasKey ? 'visible' : 'hidden' }" @click="deleteApiKey(provider.id)" :title="$t('settings.delete_key')">
                <X :size="14" />
              </button>
            </template>
          </div>
        </div>

        <!-- 插件/外部服务密钥 -->
        <h4 style="margin-bottom: 8px; font-size: 0.85em; color: var(--text-secondary);">{{ $t('settings.plugin_keys_title') }}</h4>
        <div style="display: flex; flex-direction: column; gap: 12px;">
          <div class="form-group" v-for="provider in toolProviders" :key="provider.id" style="display: flex; align-items: center; gap: 12px; border-bottom: 1px solid var(--border-subtle); padding-bottom: 8px;">
            <label class="form-label" style="width: 140px; margin-bottom: 0;">{{ provider.name }}</label>
            <span class="status-dot" :style="{ background: provider.hasKey ? 'var(--accent-primary)' : 'transparent', border: provider.hasKey ? '2px solid var(--accent-primary)' : '2px solid var(--text-tertiary)' }" style="width: 10px; height: 10px; border-radius: 50%; display: inline-block;"></span>
            <input 
              v-model="apiKeys[provider.id]" 
              type="password" 
              class="input" 
              :placeholder="provider.hasKey ? $t('settings.configured') : $t('settings.not_configured')" 
              style="flex: 1;" 
            />
            <button class="btn btn-primary" @click="saveApiKey(provider.id)" style="padding: 6px 12px;">{{ $t('settings.save') }}</button>
            <button class="btn-icon btn-remove-key" :style="{ visibility: provider.hasKey ? 'visible' : 'hidden' }" @click="deleteApiKey(provider.id)" :title="$t('settings.delete_key')">
              <X :size="14" />
            </button>
          </div>
        </div>

        <!-- 自定义模型配置 -->
        <div style="margin-top: 16px; padding-top: 12px; border-top: 1px solid var(--border-subtle);">
          <h4 style="margin-bottom: 8px; font-size: 0.85em; color: var(--text-secondary);">{{ $t('settings.custom_model_title') }}</h4>
          
          <div style="display: flex; flex-direction: column; gap: 8px; margin-bottom: 12px;">
            <div v-for="cm in customModels" :key="cm.id" style="display: flex; align-items: center; gap: 8px; padding: 6px 8px; background: var(--bg-tertiary); border-radius: 4px; border: 1px solid var(--border-subtle);">
              <span style="font-weight: bold; width: 120px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{{ cm.displayName }}</span>
              <span style="flex: 1; font-size: 0.85em; color: var(--text-tertiary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{{ cm.baseUrl }}</span>
              <button class="btn-icon" style="color: var(--color-error); width: 24px; height: 24px;" @click="removeCustomModel(cm.id)" :title="$t('settings.delete')">
                <Trash2 :size="14" />
              </button>
            </div>
          </div>
          
          <div style="display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 8px;">
            <input v-model="newCustomModel.name" class="input" :placeholder="$t('settings.custom_model_name')" style="font-size: 0.85em; padding: 4px 8px;" />
            <input v-model="newCustomModel.id" class="input" :placeholder="$t('settings.custom_model_id')" style="font-size: 0.85em; padding: 4px 8px;" />
            <input v-model="newCustomModel.url" class="input" :placeholder="$t('settings.custom_model_url')" style="font-size: 0.85em; padding: 4px 8px;" />
            <input v-model="newCustomModel.key" class="input" type="password" :placeholder="$t('settings.custom_model_key')" style="grid-column: span 2; font-size: 0.85em; padding: 4px 8px;" />
            <button class="btn btn-primary" @click="addCustomModel" :disabled="!newCustomModel.name || !newCustomModel.url || !newCustomModel.key" style="padding: 4px; font-size: 0.85em;">{{ $t('settings.custom_model_add') }}</button>
          </div>
        </div>

        <!-- 工具凭证状态 -->
        <div v-if="toolStatuses.length > 0" style="margin-top: 16px; padding-top: 12px; border-top: 1px solid var(--border-subtle);">
          <h4 style="margin-bottom: 8px; font-size: 0.85em; color: var(--text-secondary);">{{ $t('settings.tool_status_title') }}</h4>
          <div style="display: flex; flex-wrap: wrap; gap: 8px;">
            <span v-for="tool in toolStatuses" :key="tool.name"
              :title="tool.isActive ? tool.description : ($t('settings.tool_missing') + tool.missingCredentials.join(', '))"
              style="padding: 3px 10px; border-radius: 12px; font-size: 0.8em; display: inline-flex; align-items: center; gap: 6px;"
              :style="{ 
                background: tool.isActive ? 'color-mix(in srgb, var(--accent-primary) 10%, transparent)' : 'color-mix(in srgb, var(--text-tertiary) 10%, transparent)',
                color: tool.isActive ? 'var(--text-primary)' : 'var(--text-tertiary)'
              }"
            >
              <span class="status-dot" :style="{ background: tool.isActive ? 'var(--accent-primary)' : 'var(--text-tertiary)' }" style="width: 6px; height: 6px; border-radius: 50%; display: inline-block;"></span>
              {{ tool.name }}
            </span>
          </div>
        </div>
      </details>

      <!-- 模型供应商注册表编辑器 (Registry Editor) -->
      <details v-show="isAdvancedMode" class="settings-section card custom-model-override">
        <summary class="section-title" style="cursor: pointer; display: flex; align-items: center; justify-content: space-between; margin-bottom: 0;">
          <div style="display: flex; align-items: center; gap: 8px;">
            <Database :size="16" class="section-icon" style="opacity: 0.6;" />
            {{ $t('settings.registry_title') }}
          </div>
          <ChevronDown :size="16" class="details-chevron" />
        </summary>
        <p class="section-desc" style="margin-top: 16px; margin-bottom: 16px;">{{ $t('settings.registry_desc') }}</p>

        <div v-if="registryData && registryData.providers">
          <div class="registry-providers-grid">
            <!-- 每个供应商 -->
            <div v-for="(provider, pIdx) in registryData.providers" :key="provider.id"
              class="registry-provider-card">
              <!-- 供应商标题栏 -->
              <div class="registry-provider-header" @click="toggleProviderExpand(provider.id)">
                <img v-if="getProviderLogo(provider.id)" :src="getProviderLogo(provider.id)" class="registry-provider-logo" />
                <span class="registry-provider-title">{{ $te('providers.' + provider.id) ? $t('providers.' + provider.id) : provider.name }}</span>
                <span class="registry-provider-meta-id">{{ provider.id }}</span>
                <span class="registry-provider-meta-count">{{ (provider.models || []).length }} models</span>
                <ChevronDown :size="14" class="registry-provider-chevron" :class="{ expanded: expandedProviders[provider.id] }" />
              </div>

              <!-- 供应商详情（展开时显示） -->
              <div v-if="expandedProviders[provider.id]" class="registry-provider-details">
                <!-- 供应商名称 + Base URL -->
                <div class="registry-form-row">
                  <label class="registry-form-label">{{ $t('settings.registry_provider_name') }}</label>
                  <input v-model="provider.name" class="input" @input="markRegistryDirty" style="font-size: 0.85em; padding: 4px 8px;" />
                </div>
                <div class="registry-form-row">
                  <label class="registry-form-label">{{ $t('settings.registry_base_url') }}</label>
                  <input v-model="provider.base_url" class="input" @input="markRegistryDirty" style="font-size: 0.85em; padding: 4px 8px;" />
                </div>
                <div class="registry-form-row">
                  <label class="registry-form-label">{{ $t('settings.registry_auto_discover') }}</label>
                  <label style="display: flex; align-items: center; gap: 6px; cursor: pointer;">
                    <input type="checkbox" v-model="provider.supports_model_list" @change="markRegistryDirty" />
                    <span style="font-size: 0.82em; color: var(--text-tertiary);">/v1/models</span>
                  </label>
                </div>

                <!-- 模型列表 -->
                <div class="registry-models-section">
                  <div class="registry-models-header">
                    <span class="registry-models-title">{{ $t('settings.registry_models') }}</span>
                    <button class="btn btn-primary" @click="addModelToProvider(provider)" style="padding: 2px 8px; font-size: 0.78em;">
                      <Plus :size="12" /> {{ $t('settings.registry_add_model') }}
                    </button>
                  </div>
                  <div v-for="(model, mIdx) in (provider.models || [])" :key="mIdx" class="registry-model-row">
                    <input v-model="model.id" class="input" :placeholder="$t('settings.registry_model_id')" @input="markRegistryDirty" style="flex: 2; font-size: 0.82em; padding: 3px 6px;" />
                    <input v-model="model.name" class="input" :placeholder="$t('settings.registry_model_name')" @input="markRegistryDirty" style="flex: 2; font-size: 0.82em; padding: 3px 6px;" />
                    <label style="display: flex; align-items: center; gap: 3px; font-size: 0.78em; color: var(--text-tertiary); white-space: nowrap; cursor: pointer;">
                      <input type="checkbox" v-model="model.vision" @change="markRegistryDirty" /> 👁
                    </label>
                    <button class="btn-icon" style="color: var(--color-error); width: 22px; height: 22px; flex-shrink: 0;" @click="removeModelFromProvider(provider, mIdx)" :title="$t('settings.registry_remove_model')">
                      <X :size="12" />
                    </button>
                  </div>
                  <div v-if="!provider.models || provider.models.length === 0" style="font-size: 0.8em; color: var(--text-tertiary); padding: 4px 0;">
                    —
                  </div>
                </div>

                <!-- 删除供应商 -->
                <div style="display: flex; justify-content: flex-end; padding-top: 6px; border-top: 1px solid var(--border-subtle);">
                  <button class="btn btn-danger-ghost" style="padding: 3px 10px; font-size: 0.78em;" @click="removeProvider(pIdx)">
                    <Trash2 :size="12" /> {{ $t('settings.registry_remove_provider') }}
                  </button>
                </div>
              </div>
            </div>

            <!-- 添加新供应商 -->
            <div class="registry-add-provider-card">
              <span class="registry-add-provider-title">{{ $t('settings.registry_add_provider') }}</span>
              <div class="registry-add-provider-form">
                <input v-model="newProviderForm.id" class="input" :placeholder="$t('settings.registry_new_provider_id')" style="font-size: 0.82em; padding: 4px 8px;" />
                <input v-model="newProviderForm.name" class="input" :placeholder="$t('settings.registry_new_provider_name')" style="font-size: 0.82em; padding: 4px 8px;" />
                <input v-model="newProviderForm.base_url" class="input" :placeholder="$t('settings.registry_new_provider_url')" style="font-size: 0.82em; padding: 4px 8px;" />
                <button class="btn btn-primary" @click="addNewProvider" :disabled="!newProviderForm.id || !newProviderForm.name || !newProviderForm.base_url" style="padding: 6px; font-size: 0.82em; width: 100%;">
                  <Plus :size="12" /> {{ $t('settings.registry_add_provider') }}
                </button>
              </div>
            </div>
          </div>

          <!-- 保存 / 重置 -->
          <div style="display: flex; gap: 8px; align-items: center; justify-content: flex-end; padding-top: 12px; border-top: 1px solid var(--border-subtle);">
            <span v-if="registrySaveMsg" style="font-size: 0.82em; color: var(--accent-primary); display: flex; align-items: center; gap: 4px;">
              <Check :size="14" /> {{ registrySaveMsg }}
            </span>
            <button class="btn btn-ghost" @click="resetRegistry" style="padding: 4px 12px; font-size: 0.85em;">{{ $t('settings.registry_reset') }}</button>
            <button class="btn btn-primary" @click="saveRegistry" :disabled="!registryDirty" style="padding: 4px 12px; font-size: 0.85em;">{{ $t('settings.registry_save') }}</button>
          </div>
        </div>
      </details>

      <!-- 外观 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Palette :size="16" class="section-icon" />
          {{ $t('settings.appearance') }}
        </h3>
        <div class="form-group">
          <label class="form-label">{{ $t('settings.theme') }}</label>
          <CustomSelect
            v-model="config.theme"
            :options="themeOptions"
            @change="applyTheme(config.theme)"
          />
        </div>
        <div class="form-group">
          <label class="form-label">{{ $t('settings.ui_scale') }}</label>
          <CustomSelect
            v-model="config.uiScale"
            :options="uiScaleOptions"
            @change="applyUiScale(config.uiScale)"
          />
        </div>
        <div class="form-group" style="margin-top: 12px;">
          <label class="form-label">{{ $t('settings.accent_color') }}</label>
          <div style="display: flex; gap: 12px; flex-wrap: wrap; margin-top: 8px;">
            <button 
              v-for="color in accentColors" 
              :key="color.value"
              class="accent-circle"
              :class="{ active: config.accentColor === color.value }"
              :style="{ backgroundColor: color.value }"
              @click="applyAccentColor(color.value)"
              :title="color.name"
            ></button>
          </div>
        </div>
      </section>

      <!-- 语言 (直接嵌入外观区后面) -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Globe :size="16" class="section-icon" />
          {{ $t('settings.language') }}
        </h3>
        <div class="form-group">
          <CustomSelect
            v-model="currentLocale"
            :options="languageOptions"
            @change="switchLanguage"
          />
        </div>
      </section>

      <!-- Bob 的工作间（目录管理） -->
      <details class="settings-section card custom-model-override">
        <summary class="section-title" style="cursor: pointer; display: flex; align-items: center; justify-content: space-between; margin-bottom: 0;">
          <div style="display: flex; align-items: center; gap: 8px;">
            <HardDrive :size="16" class="section-icon" style="opacity: 0.6;" />
            {{ $t('settings.bob_workspace') }}
          </div>
          <ChevronDown :size="16" class="details-chevron" />
        </summary>
        <p class="section-desc" style="margin-top: 16px; margin-bottom: 16px;">{{ $t('settings.bob_workspace_desc') }}</p>

        <!-- 工作目录 (workspaceDir) -->
        <div class="details-section">
          <label class="form-label" style="font-size: 0.85em; margin-bottom: 6px; display: flex; align-items: center; gap: 6px;">
            <FolderOpen :size="14" style="opacity: 0.6;" />
            {{ $t('settings.workspace') }}
          </label>
          <p class="section-desc" style="margin-bottom: 8px; font-size: 0.8em;">{{ $t('settings.workspace_desc') }}</p>
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

        <div class="details-section">
          <!-- 关注的文件夹 -->
          <label class="form-label" style="font-size: 0.85em; margin-bottom: 6px; display: flex; align-items: center; gap: 6px;">
            <FolderHeart :size="14" style="opacity: 0.6;" />
            {{ $t('settings.tracked_folders') }}
          </label>
          <p class="section-desc" style="margin-bottom: 8px; font-size: 0.8em;">{{ $t('settings.tracked_folders_desc') }}</p>

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
          <!-- 知识库目录 (wikiDir) -->
          <label class="form-label" style="font-size: 0.85em; margin-bottom: 6px; display: flex; align-items: center; gap: 6px;">
            <FileText :size="14" style="opacity: 0.6;" />
            {{ $t('settings.wiki_dir') }}
          </label>
          <p class="section-desc" style="margin-bottom: 8px; font-size: 0.8em;">{{ $t('settings.wiki_dir_desc') }}</p>
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
        </div>

        <div class="details-section">
          <!-- MCP Servers -->
          <label class="form-label" style="font-size: 0.85em; margin-bottom: 6px; display: flex; align-items: center; gap: 6px;">
            <Unplug :size="14" style="opacity: 0.6;" />
            {{ $t('settings.mcp_servers') }}
          </label>
          <p class="section-desc" style="margin-bottom: 8px; font-size: 0.8em;">{{ $t('settings.mcp_desc') }}</p>

          <div v-if="Object.keys(mcpServers).length > 0" class="tracked-folders-list">
            <div
              v-for="(cfg, name) in mcpServers"
              :key="name"
              class="tracked-folder-item"
            >
              <div class="folder-info">
                <span class="folder-name">{{ name }}</span>
                <span class="folder-path">{{ cfg.command }} {{ (cfg.args || []).join(' ') }}</span>
              </div>
              <button class="btn-icon btn-remove-folder" @click="removeMcpServer(name)" title="删除">
                <X :size="14" />
              </button>
            </div>
          </div>
          <div v-else class="empty-folders">
            <span>{{ $t('settings.mcp_empty') }}</span>
          </div>

          <!-- 添加 MCP Server -->
          <div v-if="showAddMcp" class="mcp-add-form">
            <div class="form-group">
              <label class="form-label">{{ $t('settings.mcp_name') }}</label>
              <input v-model="newMcp.name" class="input" placeholder="例如 filesystem" />
            </div>
            <div class="form-group">
              <label class="form-label">{{ $t('settings.mcp_command') }}</label>
              <input v-model="newMcp.command" class="input" placeholder="npx" />
            </div>
            <div class="form-group">
              <label class="form-label">{{ $t('settings.mcp_args') }}</label>
              <input v-model="newMcp.args" class="input" placeholder="-y @modelcontextprotocol/server-filesystem /path" />
            </div>
            <div style="display: flex; gap: 8px; margin-top: 8px;">
              <button class="btn btn-primary" @click="addMcpServer" :disabled="!newMcp.name || !newMcp.command">{{ $t('settings.mcp_save') }}</button>
              <button class="btn btn-primary" @click="showAddMcp = false">{{ $t('settings.mcp_cancel') }}</button>
            </div>
          </div>
          <button v-else class="btn btn-primary" @click="showAddMcp = true" style="margin-top: 12px;">
            <Plus :size="14" />
            <span>{{ $t('settings.mcp_add') }}</span>
          </button>
        </div>

        <div class="details-section">
          <!-- 技能目录 (externalSkillsDir) -->
          <label class="form-label" style="font-size: 0.85em; margin-bottom: 6px; display: flex; align-items: center; gap: 6px;">
            <Puzzle :size="14" style="opacity: 0.6;" />
            {{ $t('settings.skills') }}
          </label>
          <p class="section-desc" style="margin-bottom: 8px; font-size: 0.8em;">{{ $t('settings.skills_desc') }}</p>
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

          <div class="plugin-manager-entry details-section">
            <p class="section-desc" style="margin-bottom: 12px;">{{ $t('settings.plugin_center_desc') }}</p>
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
        <summary class="section-title" style="cursor: pointer; display: flex; align-items: center; justify-content: space-between; margin-bottom: 0;">
          <div style="display: flex; align-items: center; gap: 8px;">
            <Brain :size="16" class="section-icon" />
            {{ $t('settings.memory_title') }}
          </div>
          <ChevronDown :size="16" class="details-chevron" />
        </summary>
        <p class="section-desc" style="margin-top: 16px; margin-bottom: 12px;">{{ $t('settings.memory_desc') }}</p>

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
        <summary class="section-title" style="cursor: pointer; display: flex; align-items: center; justify-content: space-between; margin-bottom: 0;">
          <div style="display: flex; align-items: center; gap: 8px;">
            <Dna :size="16" class="section-icon" />
            {{ $t('settings.evolution_title') || '进化引擎' }}
          </div>
          <ChevronDown :size="16" class="details-chevron" />
        </summary>
        <p class="section-desc" style="margin-top: 16px; margin-bottom: 12px;">{{ $t('settings.evolution_desc') || 'Bob 的自我进化系统：自动从对话中提取知识、定期整理记忆、精炼人格。' }}</p>

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

      <!-- 关于 & 数据 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Info :size="16" class="section-icon" />
          {{ $t('settings.about') }}
        </h3>
        <div class="about-info">
          <p>bob-agent v{{ appVersion }}</p>
        </div>
        
        <div v-show="isAdvancedMode" style="margin-top: 16px; padding-top: 16px; border-top: 1px solid var(--border-subtle); display: grid; grid-template-columns: repeat(auto-fit, minmax(130px, 1fr)); gap: 12px;">
          <button class="btn btn-primary" style="display: flex; align-items: center; justify-content: center; gap: 6px;" @click="openDocs">
            <BookOpen :size="14" />
            <span>{{ $t('settings.open_docs') }}</span>
          </button>
          
          <button class="btn btn-primary" style="display: flex; align-items: center; justify-content: center; gap: 6px;" @click="openDataDir">
            <FolderOpen :size="14" />
            <span>{{ $t('settings.open_data_dir') }}</span>
          </button>
          
          <button class="btn btn-primary" style="display: flex; align-items: center; justify-content: center; gap: 6px;" @click="openLogDir">
            <FileText :size="14" />
            <span>{{ $t('settings.open_log_dir') }}</span>
          </button>
          
          <button class="btn btn-danger" style="display: flex; align-items: center; justify-content: center; gap: 6px;" @click="factoryReset">
            <Trash2 :size="14" />
            <span>{{ $t('settings.clear_all_data') }}</span>
          </button>
        </div>
      </section>
      </div>
    </div>

    <!-- 微信扫码弹窗 -->
    <Transition name="briefing-fade">
      <div v-if="showWechatModal" class="wechat-modal-overlay">
        <div class="morning-briefing wechat-qr-modal">
          <div class="briefing-header">
            <div class="briefing-icon"><MessageSquare :size="18" /></div>
            <div class="briefing-title" style="flex: 1; font-size: 14px; font-weight: 600; color: var(--text-primary);">{{ $t('settings.wechat_bind_title') }}</div>
            <button class="briefing-close" @click="closeWechatModal" title="关闭" style="background: none; border: none; color: var(--text-tertiary); cursor: pointer; padding: 4px; border-radius: 4px; display: flex; align-items: center; justify-content: center;">
              <X :size="14" />
            </button>
          </div>
          <div class="briefing-body" style="display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 32px;">
            <div v-if="!qrCodeUrl && !wechatConnected" class="qr-placeholder" style="display: flex; flex-direction: column; align-items: center;">
              <Loader2 class="spin" :size="32" style="color: var(--text-tertiary)" />
              <p style="margin-top: 12px; font-size: 13px; color: var(--text-secondary);">{{ $t('settings.wechat_loading_qr') }}</p>
            </div>
            <div v-else-if="wechatConnected" class="qr-success" style="text-align: center; display: flex; flex-direction: column; align-items: center;">
              <div style="width: 64px; height: 64px; border-radius: 32px; background-color: rgba(var(--user-accent-rgb, 39, 118, 187), 0.1); color: var(--user-accent); display: flex; align-items: center; justify-content: center;"><Check :size="32" /></div>
              <h3 style="margin-top: 16px; color: var(--user-accent);">{{ $t('settings.wechat_bind_success') }}</h3>
              <p style="color: var(--text-secondary); font-size: 13px; margin-top: 4px;">{{ $t('settings.wechat_bind_success_desc') }}</p>
            </div>
            <div v-else class="qr-box" style="text-align: center;">
              <img :src="qrCodeUrl" style="width: 200px; height: 200px; border-radius: 12px; box-shadow: 0 4px 12px rgba(0,0,0,0.15);" alt="Wechat Login QR" />
              <p style="margin-top: 16px; font-size: 14px; color: var(--text-secondary); font-weight: 500;">{{ $t('settings.wechat_scan_hint') }}</p>
            </div>
          </div>
        </div>
      </div>
    </Transition>

    <!-- 使用文档弹窗 -->
    <Transition name="briefing-fade">
      <div v-if="showHelpModal" class="wechat-modal-overlay" @click.self="showHelpModal = false">
        <div class="help-modal">
          <div class="briefing-header">
            <div class="briefing-icon"><BookOpen :size="18" /></div>
            <div class="briefing-title" style="flex: 1; font-size: 14px; font-weight: 600; color: var(--text-primary);">{{ $t('settings.open_docs') }}</div>
            <button class="briefing-close" @click="showHelpModal = false" style="background: none; border: none; color: var(--text-tertiary); cursor: pointer; padding: 4px; border-radius: 4px; display: flex; align-items: center; justify-content: center;">
              <X :size="14" />
            </button>
          </div>
          <div class="help-body" v-html="renderedGuide"></div>
        </div>
      </div>
    </Transition>

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
  </div>
</template>

<script setup>
import { marked } from 'marked';
import DOMPurify from 'dompurify';

import { ref, computed, onMounted, inject, watch, onUnmounted } from 'vue';
import { Settings as SettingsIcon, Monitor, Tractor, Eye, EyeOff, Plug, Loader2, Palette, Info, FolderOpen, FolderHeart, Puzzle, Layers, X, Plus, Unplug, Globe, HardDrive, Trash2, Key, FileText, Server, ChevronDown, BookOpen, MessageSquare, Check, Database, Brain, Smartphone, Send, MessageCircle, Dna, Moon } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import CustomSelect from '../components/CustomSelect.vue';
import PluginManager from '../components/PluginManager.vue';
import ModelHub from '../components/ModelHub.vue';
import { ACCENT_COLORS, PREMIUM_THEMES } from '@/constants/theme.js';

const emit = defineEmits(['config-changed']);
const { locale, t } = useI18n();
const currentLocale = ref('zh-CN');
const injectedTheme = inject('currentTheme', null);

const isAdvancedMode = ref(localStorage.getItem('bob_advanced_mode') === 'true');
const toggleAdvancedMode = () => {
  isAdvancedMode.value = !isAdvancedMode.value;
  localStorage.setItem('bob_advanced_mode', isAdvancedMode.value);
};

const languageOptions = [
  { label: '简体中文', value: 'zh-CN' },
  { label: 'English', value: 'en-US' },
];

const appVersion = ref('0.32.0');

function switchLanguage(val) {
  locale.value = val || currentLocale.value;
  saveConfig('language', locale.value);
}

const providerOptions = [
  { label: 'DeepSeek', value: 'deepseek' },
  { label: 'OpenAI', value: 'openai' },
  { label: '通义千问 (Qwen)', value: 'qwen' },
  { label: '豆包 (Doubao)', value: 'doubao' },
  { label: '智谱 AI (GLM)', value: 'zhipu' },
  { label: 'Kimi (Moonshot)', value: 'kimi' },
  { label: 'MiniMax', value: 'minimax' },
  { label: t('settings.custom_provider'), value: 'custom' },
];

const accentColors = computed(() => ACCENT_COLORS.map(c => ({
  value: c.value,
  name: c.nameKey ? (t(c.nameKey) || c.name) : c.name,
})));

const themeOptions = computed(() => {
  const baseOptions = [
    { label: t('settings.theme_dark'), value: 'dark' },
    { label: t('settings.theme_light'), value: 'light' },
  ];
  const premiumOptions = PREMIUM_THEMES.map(theme => ({
    label: theme.nameKey ? (t(theme.nameKey) || theme.name) : theme.name,
    value: theme.id
  }));
  return [...baseOptions, ...premiumOptions];
});

const uiScaleOptions = computed(() => [
  { label: t('settings.scale_compact'), value: 'compact' },
  { label: t('settings.scale_comfortable'), value: 'comfortable' },
]);

function applyUiScale(scale, persist = true) {
  document.documentElement.setAttribute('data-ui-scale', scale);
  if (persist) {
    saveConfig('uiScale', scale);
  }
}

function applyTheme(theme, persist = true) {
  document.documentElement.classList.add('theme-transitioning');
  
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      document.documentElement.setAttribute('data-theme', theme);
      setTimeout(() => {
        document.documentElement.classList.remove('theme-transitioning');
      }, 850);
    });
  });

  if (persist) {
    saveConfig('theme', theme);
  }
  if (window.electronAPI.updateTheme) {
    window.electronAPI.updateTheme(theme);
  }
}

function applyAccentColor(color) {
  config.value.accentColor = color;
  localStorage.setItem('bob-accent', color);
  document.documentElement.style.setProperty('--user-accent', color);
  const hex = color.replace('#', '');
  const r = parseInt(hex.substring(0, 2), 16);
  const g = parseInt(hex.substring(2, 4), 16);
  const b = parseInt(hex.substring(4, 6), 16);
  document.documentElement.style.setProperty('--user-accent-rgb', `${r}, ${g}, ${b}`);
  saveConfig('accentColor', color);
  emit('config-changed');
}

const config = ref({
  provider: 'deepseek',
  apiKey: '',
  model: '',
  baseURL: '',
  _defaultBaseURL: '',
  clerkProvider: 'deepseek',
  clerkApiKey: '',
  clerkModel: '',
  clerkBaseURL: '',
  _defaultClerkBaseURL: '',
  theme: 'dark',
  uiScale: 'compact',
  wikiDir: '',
  workspaceDir: '',
  externalSkillsDir: '',
  accentColor: '',
  offlineModelPath: '',
});

const offlineEngineStatus = ref('stopped');
const showLlamaGuide = ref(false);

async function openLlamaEngineDir() {
  await window.electronAPI.openLlamaEngineDir();
}

async function selectOfflineModel() {
  if (window.electronAPI.selectFile) {
    const path = await window.electronAPI.selectFile();
    if (path) {
      config.value.offlineModelPath = path;
      saveConfig('offlineModelPath', path);
    }
  }
}

async function toggleOfflineEngine() {
  if (offlineEngineStatus.value === 'running') {
    const res = await window.electronAPI.stopOfflineEngine();
    if (res && res.status === 'stopped') {
      offlineEngineStatus.value = 'stopped';
    }
  } else {
    if (!config.value.offlineModelPath) return;
    offlineEngineStatus.value = 'starting';
    try {
      const res = await window.electronAPI.startOfflineEngine(config.value.offlineModelPath);
      if (res && res.status === 'running') {
        offlineEngineStatus.value = 'running';
      } else {
        offlineEngineStatus.value = 'stopped';
      }
    } catch(err) {
      offlineEngineStatus.value = 'stopped';
      alert('启动离线引擎失败: ' + err);
    }
  }
}

  const showWechatModal = ref(false);
  const qrCodeUrl = ref('');
  const wechatConnected = ref(false);
  const rawQrCode = ref('');
  let wechatPollTimer = null;
  
  const mobileChannel = ref('wechat');
  const tgToken = ref('');
  const discordToken = ref('');
  
  function activateMobileChannel(channel) {
    if (channel === 'telegram') {
      alert('已模拟绑定 Telegram。互斥逻辑生效，后台其他渠道的长连接将被自动踢出 (需后端实现)。');
      wechatConnected.value = false;
    } else if (channel === 'discord') {
      alert('已模拟绑定 Discord。互斥逻辑生效，后台其他渠道的长连接将被自动踢出 (需后端实现)。');
      wechatConnected.value = false;
    }
  }

  async function openWechatModal() {
  showWechatModal.value = true;
  wechatConnected.value = false;
  await loadWechatQrCode();
}

function closeWechatModal() {
  showWechatModal.value = false;
  if (wechatPollTimer) {
    clearTimeout(wechatPollTimer);
    wechatPollTimer = null;
  }
  qrCodeUrl.value = '';
}

async function loadWechatQrCode() {
  if (!window.electronAPI) return;
  qrCodeUrl.value = '';
  try {
    const res = await window.electronAPI.wechatGetLoginQr();
    if (res && res.qrcode_img_content) {
      const content = res.qrcode_img_content;
      if (content.startsWith('data:')) {
        qrCodeUrl.value = content;
      } else if (content.startsWith('http')) {
        try {
          const resp = await fetch(content);
          const blob = await resp.blob();
          const reader = new FileReader();
          qrCodeUrl.value = await new Promise((resolve, reject) => {
            reader.onloadend = () => resolve(reader.result);
            reader.onerror = reject;
            reader.readAsDataURL(blob);
          });
        } catch { qrCodeUrl.value = content; }
      } else {
        qrCodeUrl.value = 'data:image/png;base64,' + content;
      }
      rawQrCode.value = res.qrcode;
      pollWechatQrStatus();
    }
  } catch (e) {
    console.error('Failed to get QR code', e);
  }
}

async function pollWechatQrStatus() {
  if (!showWechatModal.value || wechatConnected.value) return;
  try {
    const res = await window.electronAPI.wechatCheckLoginStatus(rawQrCode.value);
    if (res && (res.status === 'confirmed' || res.status === 'binded_redirect')) {
      wechatConnected.value = true;
      if (wechatPollTimer) clearTimeout(wechatPollTimer);
      return;
    }
    if (res && res.status === 'expired') {
      await loadWechatQrCode();
      return;
    }
  } catch (e) {}
  wechatPollTimer = setTimeout(pollWechatQrStatus, 2000);
}

const availableModels = ref([]);
const computedModelOptions = computed(() => {
  return availableModels.value.map(m => ({
    label: `${m.label} (${m.id})`,
    value: m.id
  }));
});

const availableClerkModels = ref([]);
const computedClerkModelOptions = computed(() => {
  return availableClerkModels.value.map(m => ({
    label: `${m.label} (${m.id})`,
    value: m.id
  }));
});

const showApiKey = ref(false);
const showClerkApiKey = ref(false);
const isTesting = ref(false);
const testResult = ref(null);
const isMainConnected = computed(() => {
  if (testResult.value) return testResult.value.ok;
  return config.value._hasApiKey || false;
});

const isClerkTesting = ref(false);
const clerkTestResult = ref(null);
const isClerkConnected = computed(() => {
  if (clerkTestResult.value) return clerkTestResult.value.ok;
  return config.value._hasClerkApiKey || false;
});

const showPluginManager = ref(false);
const trackedFolders = ref([]);

// ── T-1302: 记忆管理 ──
const memoryLoading = ref(false);
const memoryEntries = ref([]);

async function loadMemoryEntries() {
  if (!window.electronAPI.getMemoryEntries) return;
  memoryLoading.value = true;
  try {
    const entries = await window.electronAPI.getMemoryEntries();
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
    await window.electronAPI.deleteMemoryEntry(entry.type, entry.id);
    memoryEntries.value = memoryEntries.value.filter(
      e => !(e.type === entry.type && e.id === entry.id)
    );
  } catch (e) {
    console.error('Failed to delete memory entry', e);
  }
}

function formatMemorySize(bytes) {
  if (!bytes || bytes < 1024) return `${bytes || 0} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatMemoryTitle(title) {
  if (!title) return '';
  let cleaned = title.replace(/^(对话摘要|对话记忆|知识条目|知识库)[:：]\s*/g, '');
  if (cleaned.startsWith('conv-')) {
    return t('chat.conversation') + ' ' + cleaned.replace('conv-', '');
  }
  return cleaned;
}

function formatMemoryTime(ts) {
  if (!ts) return '';
  const d = new Date(ts > 1e11 ? ts : ts * 1000);
  return d.toLocaleDateString() + ' ' + d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

// ── 进化引擎看板 ──
const evoLoading = ref(false);
const evoStats = ref({});

async function loadEvolutionStats() {
  if (!window.electronAPI.getEvolutionStats) return;
  evoLoading.value = true;
  try {
    const data = await window.electronAPI.getEvolutionStats();
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

function formatEvoTime(ts) {
  if (!ts) return '';
  const d = new Date(ts > 1e11 ? ts : ts * 1000);
  const now = new Date();
  const diffMs = now - d;
  const diffH = Math.floor(diffMs / 3_600_000);
  if (diffH < 1) return '刚刚';
  if (diffH < 24) return `${diffH}小时前`;
  const diffD = Math.floor(diffH / 24);
  if (diffD < 7) return `${diffD}天前`;
  return d.toLocaleDateString();
}

// ── 凭证管理 (Credential Store) ──
const modelProviders = ref([]); // 从 registry 动态加载

// ── 注册表编辑器 ──
const registryData = ref(null);
const registryDirty = ref(false);
const registrySaveMsg = ref('');
const newProviderForm = ref({ id: '', name: '', base_url: '' });
const expandedProviders = ref({});

async function loadRegistryProviders() {
  try {
    const reg = await window.electronAPI.getRegistry();
    registryData.value = reg;
    if (reg && reg.providers) {
      modelProviders.value = reg.providers
        .filter(p => p.id !== 'offline')
        .map(p => ({ id: p.id, name: p.name, hasKey: false }));
    }
  } catch (e) {
    console.warn('Failed to load registry:', e);
    modelProviders.value = [
      { id: 'deepseek', name: 'DeepSeek', hasKey: false },
      { id: 'openai', name: 'OpenAI', hasKey: false },
      { id: 'qwen', name: '通义千问 (Qwen)', hasKey: false },
      { id: 'doubao', name: '豆包 (Doubao)', hasKey: false },
      { id: 'zhipu', name: '智谱 AI (GLM)', hasKey: false },
      { id: 'kimi', name: 'Kimi (Moonshot)', hasKey: false },
      { id: 'minimax', name: 'MiniMax', hasKey: false },
    ];
  }
}

function toggleProviderExpand(providerId) {
  expandedProviders.value[providerId] = !expandedProviders.value[providerId];
}

function markRegistryDirty() {
  registryDirty.value = true;
  registrySaveMsg.value = '';
}

function addModelToProvider(provider) {
  if (!provider.models) provider.models = [];
  provider.models.push({ id: '', name: '', vision: false, pricing: { input: 0, output: 0 } });
  markRegistryDirty();
}

function removeModelFromProvider(provider, index) {
  provider.models.splice(index, 1);
  markRegistryDirty();
}

function addNewProvider() {
  const f = newProviderForm.value;
  if (!f.id || !f.name || !f.base_url) return;
  if (!registryData.value) return;
  if (registryData.value.providers.some(p => p.id === f.id)) return;
  registryData.value.providers.push({
    id: f.id,
    name: f.name,
    base_url: f.base_url,
    supports_model_list: false,
    models: [],
  });
  newProviderForm.value = { id: '', name: '', base_url: '' };
  markRegistryDirty();
}

function removeProvider(index) {
  if (!registryData.value) return;
  if (!confirm(t('settings.registry_confirm_remove'))) return;
  registryData.value.providers.splice(index, 1);
  markRegistryDirty();
  modelProviders.value = registryData.value.providers
    .filter(p => p.id !== 'offline')
    .map(p => ({ id: p.id, name: p.name, hasKey: false }));
  fetchApiKeys();
}

async function saveRegistry() {
  if (!registryData.value) return;
  try {
    registryData.value.last_updated = new Date().toISOString().slice(0, 10);
    await window.electronAPI.saveRegistry(registryData.value);
    registryDirty.value = false;
    registrySaveMsg.value = t('settings.registry_saved');
    await loadRegistryProviders();
    await fetchApiKeys();
    if (modelHubRef.value) modelHubRef.value.rescan();
    setTimeout(() => { registrySaveMsg.value = ''; }, 3000);
  } catch (e) {
    console.error('Failed to save registry:', e);
  }
}

async function resetRegistry() {
  await loadRegistryProviders();
  registryDirty.value = false;
  registrySaveMsg.value = '';
}

const toolProviders = ref([
  { id: 'TAVILY_API_KEY', name: 'Tavily (Web Search)', hasKey: false },
  { id: 'TINYFISH_API_KEY', name: 'TinyFish (Fetch)', hasKey: false },
]);
const apiKeys = ref({});
const toolStatuses = ref([]);

const customModels = ref([]);
const newCustomModel = ref({ name: '', url: '', key: '', id: '' });

async function loadCustomModels() {
  const allConfig = await window.electronAPI.getAllConfig();
  customModels.value = allConfig.customModels || [];
}

async function addCustomModel() {
  if (!newCustomModel.value.name || !newCustomModel.value.url || !newCustomModel.value.key) return;
  const id = newCustomModel.value.id || ('custom-' + Date.now());
  const provider = 'custom_' + id;
  if (window.electronAPI.addCustomModel) {
    await window.electronAPI.addCustomModel(id, newCustomModel.value.name, provider, newCustomModel.value.url, newCustomModel.value.key);
    newCustomModel.value = { name: '', url: '', key: '', id: '' };
    await loadCustomModels();
    if (modelHubRef.value) modelHubRef.value.rescan();
  }
}

async function removeCustomModel(id) {
  if (window.electronAPI.removeCustomModel) {
    await window.electronAPI.removeCustomModel(id);
    await loadCustomModels();
    if (modelHubRef.value) modelHubRef.value.rescan();
  }
}

function getProviderLogo(providerId) {
  const name = (providerId || '').toLowerCase();
  if (name.includes('deepseek')) return '/logos/deepseek.png';
  if (name.includes('openai')) return '/logos/openai.png';
  if (name.includes('qwen') || name.includes('dashscope')) return '/logos/qwen.png';
  if (name.includes('doubao')) return '/logos/doubao.png';
  if (name.includes('zhipu')) return '/logos/glm.svg';
  if (name.includes('kimi')) return '/logos/kimi.png';
  if (name.includes('minimax')) return '/logos/minimax.png';
  if (name.includes('gemini') || name.includes('google')) return '/logos/google.png';
  if (name.includes('claude') || name.includes('anthropic')) return '/logos/claude.png';
  return null;
}

async function fetchApiKeys() {
  if (window.electronAPI.getApiKeys) {
    const status = await window.electronAPI.getApiKeys();
    [...modelProviders.value, ...toolProviders.value].forEach(p => {
      p.hasKey = !!status[p.id];
    });
  }
}

async function fetchToolStatuses() {
  if (window.electronAPI.getToolStatuses) {
    const statuses = await window.electronAPI.getToolStatuses();
    // 后端返回的 statuses 已经只包含了需要凭证/配置的外部工具（Tavily, TinyFish, Skills 等），直接全量展示即可
    toolStatuses.value = statuses;
  }
}

const modelHubRef = ref(null);

async function saveApiKey(providerId) {
  if (window.electronAPI.setApiKey) {
    const key = apiKeys.value[providerId];
    if (key === undefined || key === null) return;
    
    // 空字符串代表删除该 key
    await window.electronAPI.setApiKey(providerId, key);
    
    await fetchApiKeys(); // refresh key status first
    apiKeys.value[providerId] = ''; // clear input after status is refreshed
    
    await fetchToolStatuses(); // refresh tool activation states
    if (modelHubRef.value) {
      modelHubRef.value.refreshKeyStatus();
    }
    emit('config-changed'); // notify App to refresh
  }
}

async function deleteApiKey(providerId) {
  if (window.electronAPI.setApiKey) {
    await window.electronAPI.setApiKey(providerId, '');
    apiKeys.value[providerId] = '';
    await fetchApiKeys();
    await fetchToolStatuses();
    if (modelHubRef.value) {
      modelHubRef.value.refreshKeyStatus();
    }
    emit('config-changed');
  }
}

// ── GCP Vertex AI 凭证管理 ──────────────────────────────
const gcpCredStatus = ref({ configured: false });

async function loadGcpCredentialStatus() {
  if (window.electronAPI.getGcpCredentialStatus) {
    gcpCredStatus.value = await window.electronAPI.getGcpCredentialStatus();
  }
}

async function uploadGcpCredential() {
  const { open } = await import('@tauri-apps/plugin-dialog');
  const selected = await open({
    multiple: false,
    title: '选择 GCP Service Account JSON 凭证文件',
    filters: [{ name: 'JSON', extensions: ['json'] }],
  });
  if (!selected) return;
  const result = await window.electronAPI.uploadGcpCredential(selected);
  if (result.error) {
    alert('凭证上传失败: ' + result.error);
  } else {
    await loadGcpCredentialStatus();
  }
}

async function testGcpCredential() {
  const result = await window.electronAPI.testGcpCredential();
  if (result.error) {
    alert('❌ 连通性测试失败: ' + result.error);
  } else {
    alert('✅ 连通性测试成功！\nProject: ' + result.project_id + '\nToken: ' + result.token_preview);
  }
}

async function removeGcpCredential() {
  const result = await window.electronAPI.removeGcpCredential();
  if (result.ok) {
    await loadGcpCredentialStatus();
  }
}

onMounted(async () => {
  const allConfig = await window.electronAPI.getAllConfig();
  config.value = {
    provider: allConfig.provider || 'deepseek',
    apiKey: allConfig.apiKey || '',
    model: allConfig.model || '',
    baseURL: allConfig.baseURL || '',
    _defaultBaseURL: allConfig._defaultBaseURL || '',
    _hasApiKey: allConfig._hasApiKey || false,
    clerkProvider: allConfig.clerkProvider || 'deepseek',
    clerkApiKey: allConfig.clerkApiKey || '',
    clerkModel: allConfig.clerkModel || '',
    clerkBaseURL: allConfig.clerkBaseURL || '',
    _defaultClerkBaseURL: allConfig._defaultClerkBaseURL || '',
    _hasClerkApiKey: allConfig._hasClerkApiKey || false,
    theme: injectedTheme ? injectedTheme.value : (allConfig.theme || 'dark'),
    uiScale: allConfig.uiScale || 'compact',
    wikiDir: allConfig.wikiDir || '',
    workspaceDir: allConfig.workspaceDir || '',
    externalSkillsDir: allConfig.externalSkillsDir || '',
    language: allConfig.language || 'zh-CN',
    accentColor: allConfig.accentColor || '',
  };
  // 恢复用户选择的语言
  currentLocale.value = config.value.language;
  locale.value = config.value.language;
  applyUiScale(config.value.uiScale, false);
  await loadModels();
  await loadTrackedFolders();
  await loadMcpConfig();
  await loadRegistryProviders();
  await fetchApiKeys();
  await loadCustomModels();
  await fetchToolStatuses();
  await loadGcpCredentialStatus();
  if (window.electronAPI.getVersion) {
    appVersion.value = await window.electronAPI.getVersion();
  }
  if (window.electronAPI.getOfflineEngineStatus) {
    try {
      const res = await window.electronAPI.getOfflineEngineStatus();
      if (res && res.status) {
        offlineEngineStatus.value = res.status;
      }
    } catch(err) {}
  }
  if (window.electronAPI.wechatGetCurrentStatus) {
    try {
      const res = await window.electronAPI.wechatGetCurrentStatus();
      if (res && res.connected) {
        wechatConnected.value = true;
      }
    } catch(err) {}
  }
  await loadMemoryEntries();
  loadEvolutionStats();  // 不 await，后台加载不阻塞
});

async function loadModels() {
  availableModels.value = await window.electronAPI.getModels(config.value.provider);
  availableClerkModels.value = await window.electronAPI.getModels(config.value.clerkProvider);
}

async function saveConfig(key, value) {
  await window.electronAPI.setConfig(key, value);
  emit('config-changed');
}

// Track the masked key so we don't save it back by accident
const apiKeyOriginalMask = ref('');
const clerkApiKeyOriginalMask = ref('');

function onApiKeyFocus(field) {
  const mask = field === 'apiKey' ? config.value.apiKey : config.value.clerkApiKey;
  if (field === 'apiKey') {
    apiKeyOriginalMask.value = mask;
    // If the current value is a masked key, clear it for fresh input
    if (mask && mask.includes('...')) config.value.apiKey = '';
  } else {
    clerkApiKeyOriginalMask.value = mask;
    if (mask && mask.includes('...')) config.value.clerkApiKey = '';
  }
}

async function onApiKeyBlur(field) {
  const val = field === 'apiKey' ? config.value.apiKey : config.value.clerkApiKey;
  const original = field === 'apiKey' ? apiKeyOriginalMask.value : clerkApiKeyOriginalMask.value;

  if (!val) {
    // User cleared the field without entering new key — restore mask
    if (field === 'apiKey') config.value.apiKey = original;
    else config.value.clerkApiKey = original;
    return;
  }

  // Only save if it's a new, non-masked value
  if (val !== original && !val.includes('...')) {
    await saveConfig(field, val);
    // Update the _has flag and re-mask
    if (field === 'apiKey') {
      config.value._hasApiKey = true;
      const masked = val.length > 8 ? val.substring(0, 5) + '...' + val.substring(val.length - 4) : '\u2022\u2022\u2022\u2022\u2022\u2022';
      config.value.apiKey = masked;
      apiKeyOriginalMask.value = masked;
    } else {
      config.value._hasClerkApiKey = true;
      const masked = val.length > 8 ? val.substring(0, 5) + '...' + val.substring(val.length - 4) : '\u2022\u2022\u2022\u2022\u2022\u2022';
      config.value.clerkApiKey = masked;
      clerkApiKeyOriginalMask.value = masked;
    }
  }
}

async function onProviderChange() {
  await saveConfig('provider', config.value.provider);
  availableModels.value = await window.electronAPI.getModels(config.value.provider);
  // 自动选择默认模型
  const defaultModel = availableModels.value.find(m => m.default);
  if (defaultModel) {
    config.value.model = defaultModel.id;
    await saveConfig('model', defaultModel.id);
  }
}

async function onClerkProviderChange() {
  await saveConfig('clerkProvider', config.value.clerkProvider);
  availableClerkModels.value = await window.electronAPI.getModels(config.value.clerkProvider);
  // 自动选择默认模型
  const defaultModel = availableClerkModels.value.find(m => m.default);
  if (defaultModel) {
    config.value.clerkModel = defaultModel.id;
    await saveConfig('clerkModel', defaultModel.id);
  }
}

async function testConnection(target = 'main') {
  const isMain = target === 'main';
  if (isMain) {
    isTesting.value = true;
    testResult.value = null;
  } else {
    isClerkTesting.value = true;
    clerkTestResult.value = null;
  }

  try {
    const result = await window.electronAPI.sendChat([
      { role: 'user', content: '你好，请回复"连接成功"' }
    ], { useClerk: !isMain }); // 传一个标识给后端

    if (result.error) {
      if (isMain) testResult.value = { ok: false, message: result.error };
      else clerkTestResult.value = { ok: false, message: result.error };
    } else {
      if (isMain) testResult.value = { ok: true, message: t('settings.connection_ok') };
      else clerkTestResult.value = { ok: true, message: t('settings.connection_ok') };
    }
  } catch (err) {
    if (isMain) testResult.value = { ok: false, message: err.message };
    else clerkTestResult.value = { ok: false, message: err.message };
  } finally {
    if (isMain) isTesting.value = false;
    else isClerkTesting.value = false;
  }
}

const showHelpModal = ref(false);
const renderedGuide = ref('');

async function openDocs() {
  showHelpModal.value = true;
  if (!renderedGuide.value) {
    try {
      const resp = await fetch('/guide.md');
      const md = await resp.text();
      const raw = marked.parse(md, { breaks: true });
      renderedGuide.value = DOMPurify.sanitize(raw);
    } catch (e) {
      renderedGuide.value = '<p style="color: var(--text-secondary)">Failed to load guide.</p>';
    }
  }
}

function openDataDir() {
  if (window.electronAPI.openDataDir) {
    window.electronAPI.openDataDir();
  }
}

function openLogDir() {
  if (window.electronAPI.openLogDir) {
    window.electronAPI.openLogDir();
  }
}

async function factoryReset() {
  if (confirm(t('modal.factory_reset_warning'))) {
    if (window.electronAPI.factoryReset) {
      await window.electronAPI.factoryReset();
    }
  }
}

async function selectWorkspaceDir() {
  const dirPath = await window.electronAPI.selectWorkspaceDir();
  if (dirPath) {
    config.value.workspaceDir = dirPath;
    await saveConfig('workspaceDir', dirPath);
  }
}

async function clearWorkspaceDir() {
  config.value.workspaceDir = '';
  await saveConfig('workspaceDir', '');
}

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
    const res = await window.electronAPI.migrateWikiDir(
      config.value.wikiDir || '',
      pendingWikiDir.value,
      migrationMode.value
    );
    if (res && res.ok) {
      config.value.wikiDir = pendingWikiDir.value;
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
  const dirPath = await window.electronAPI.selectDir();
  if (dirPath) {
    if (dirPath === config.value.wikiDir) return;
    pendingWikiDir.value = dirPath;
    showWikiMigrationModal.value = true;
  }
}

async function clearWikiDir() {
  config.value.wikiDir = '';
  await saveConfig('wikiDir', '');
}

async function selectExternalSkillsDir() {
  const dirPath = await window.electronAPI.selectDir();
  if (dirPath) {
    config.value.externalSkillsDir = dirPath;
    await saveConfig('externalSkillsDir', dirPath);
  }
}

async function clearExternalSkillsDir() {
  config.value.externalSkillsDir = '';
  await saveConfig('externalSkillsDir', '');
}

// ── 文件夹跟踪 ────────────────────────────────────────
async function loadTrackedFolders() {
  trackedFolders.value = await window.electronAPI.getTrackedFolders();
}

async function addFolder() {
  const dirPath = await window.electronAPI.selectFolderToTrack();
  if (dirPath) {
    await window.electronAPI.addTrackedFolder(dirPath);
    await loadTrackedFolders();
  }
}

async function removeFolder(folderPath) {
  await window.electronAPI.removeTrackedFolder(folderPath);
  await loadTrackedFolders();
}

// ── MCP 配置管理 ─────────────────────────────────────
const mcpServers = ref({});
const showAddMcp = ref(false);
const newMcp = ref({ name: '', command: '', args: '' });

async function loadMcpConfig() {
  if (!window.electronAPI.getMcpConfig) return;
  const config = await window.electronAPI.getMcpConfig();
  mcpServers.value = config.mcpServers || {};
}

async function addMcpServer() {
  const name = newMcp.value.name.trim();
  if (!name) return;
  const updated = { ...mcpServers.value };
  updated[name] = {
    command: newMcp.value.command.trim(),
    args: newMcp.value.args.trim().split(/\s+/).filter(Boolean),
  };
  await window.electronAPI.setMcpConfig({ mcpServers: updated });
  mcpServers.value = updated;
  newMcp.value = { name: '', command: '', args: '' };
  showAddMcp.value = false;
}

async function removeMcpServer(name) {
  const updated = { ...mcpServers.value };
  delete updated[name];
  await window.electronAPI.setMcpConfig({ mcpServers: updated });
  mcpServers.value = updated;
}

const handleThemeChange = (e) => {
  if (config.value.theme !== e.detail) {
    config.value.theme = e.detail;
  }
};
window.addEventListener('bob-theme-changed', handleThemeChange);

onUnmounted(() => {
  window.removeEventListener('bob-theme-changed', handleThemeChange);
});
</script>

<style scoped>
.settings-view {
  flex: 1;
  min-width: 0;
  height: 100%;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.settings-scroll {
  height: 100%;
  overflow-y: auto;
  padding: var(--space-6) var(--space-8);
}

.settings-content {
  padding: 0;
  max-width: 1000px;
  width: 100%;
  margin: 0 auto;
  box-sizing: border-box;
}

.settings-title {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-2xl);
  font-weight: 600;
  margin-bottom: var(--space-6);
}

.title-icon {
  color: var(--text-secondary);
}



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

.toggle-key {
  position: absolute;
  right: var(--space-2);
  top: 50%;
  transform: translateY(-50%);
}

select.input {
  cursor: pointer;
  appearance: none;
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%23a0a0b8' d='M6 8L1 3h10z'/%3E%3C/svg%3E");
  background-repeat: no-repeat;
  background-position: right 12px center;
  padding-right: 32px;
}

.test-section {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  margin-top: var(--space-3);
}

.test-result {
  font-size: var(--text-sm);
}

.test-result.success {
  color: var(--user-accent);
}

.test-result.error {
  color: var(--color-error);
}

.about-info {
  color: var(--text-secondary);
  font-size: var(--text-sm);
}

.about-desc {
  color: var(--text-tertiary);
  margin-top: var(--space-1);
}

.section-desc {
  font-size: var(--text-sm);
  color: var(--text-tertiary);
  margin-bottom: var(--space-4);
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

.mcp-add-form {
  margin-top: 16px;
  padding: 12px;
  background: var(--surface-glass);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
}

.icon-success {
  color: var(--user-accent);
  transition: color var(--duration-fast);
}

.icon-disabled {
  color: var(--text-tertiary);
  opacity: 0.4;
  transition: color var(--duration-fast), opacity var(--duration-fast);
}

.accent-circle {
  width: 24px;
  height: 24px;
  border-radius: 12px;
  border: 2px solid transparent;
  cursor: pointer;
  transition: transform 0.2s, box-shadow 0.2s;
}

.accent-circle:hover {
  transform: scale(1.1);
}

.accent-circle.active {
  border-color: var(--text-primary);
  transform: scale(1.1);
  box-shadow: 0 0 0 2px var(--bg-primary), 0 0 0 4px var(--text-primary);
}

.btn-remove-key {
  flex-shrink: 0;
  color: var(--text-tertiary);
  opacity: 0.6;
  transition: all 0.15s ease;
}
.btn-remove-key:hover {
  color: var(--color-error);
  opacity: 1;
  background: var(--color-error-bg);
}

.btn-danger-ghost {
  background: transparent;
  color: var(--color-error);
  border: 1px solid color-mix(in srgb, var(--color-error) 30%, transparent);
}
.btn-danger-ghost:hover {
  background: var(--color-error-bg);
  border-color: var(--color-error);
}

  /* 微信二维码弹窗样式 */
  .channel-btn {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 8px 12px;
    background: transparent;
    border: none;
    border-radius: 6px;
    color: var(--text-secondary);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s;
  }
  .channel-btn:hover {
    background: rgba(0,0,0,0.05);
    color: var(--text-primary);
  }
  .channel-btn.active {
    background: var(--bg-primary);
    color: var(--text-primary);
    box-shadow: 0 1px 3px rgba(0,0,0,0.1);
  }

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

.wechat-qr-modal {
  width: 400px;
  background: var(--bg-primary);
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

.spin {
  animation: spin 1s linear infinite;
}
@keyframes spin { 100% { transform: rotate(360deg); } }

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

/* ── 使用文档弹窗 ── */
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

.help-body {
  flex: 1;
  overflow-y: auto;
  padding: 24px 28px;
  font-size: var(--text-sm);
  color: var(--text-secondary);
  line-height: 1.8;
}

.help-body :deep(h1) {
  font-size: 20px;
  font-weight: 700;
  color: var(--text-primary);
  margin: 0 0 8px;
}

.help-body :deep(h2) {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
  margin: 20px 0 8px;
  padding-bottom: 4px;
  border-bottom: 1px solid var(--border-subtle);
}

.help-body :deep(h3) {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  margin: 14px 0 4px;
}

.help-body :deep(hr) {
  border: none;
  border-top: 1px solid var(--border-subtle);
  margin: 12px 0;
}

.help-body :deep(ul),
.help-body :deep(ol) {
  padding-left: 20px;
  margin: 4px 0;
}

.help-body :deep(li) {
  margin: 2px 0;
}

.help-body :deep(strong) {
  font-weight: 600;
  color: var(--text-primary);
}

.help-body :deep(code) {
  font-family: var(--font-mono, 'JetBrains Mono', monospace);
  font-size: 12px;
  background: var(--bg-hover);
  padding: 2px 6px;
  border-radius: 4px;
}

.help-body :deep(table) {
  width: 100%;
  border-collapse: collapse;
  margin: 8px 0;
  font-size: 13px;
}

.help-body :deep(th),
.help-body :deep(td) {
  padding: 6px 12px;
  border: 1px solid var(--border-subtle);
  text-align: left;
}

.help-body :deep(th) {
  background: var(--bg-hover);
  font-weight: 600;
  color: var(--text-primary);
}

.help-body :deep(p) {
  margin: 6px 0;
}

.help-body :deep(a) {
  color: var(--user-accent);
  text-decoration: none;
}

.help-body :deep(a:hover) {
  text-decoration: underline;
}

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

/* ── Model Provider Registry ── */
.registry-providers-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 16px;
  margin-bottom: 16px;
  align-items: start;
}
@media (max-width: 850px) {
  .registry-providers-grid {
    grid-template-columns: 1fr;
  }
}

.registry-provider-card {
  border: 1px solid var(--border-subtle);
  border-radius: 8px;
  overflow: hidden;
  background: var(--bg-secondary);
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
}
.registry-provider-card:hover {
  border-color: var(--border-default);
}

.registry-provider-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  background: var(--bg-tertiary);
  cursor: pointer;
  user-select: none;
}

.registry-provider-logo {
  width: 16px;
  height: 16px;
  object-fit: contain;
  border-radius: 2px;
  flex-shrink: 0;
}

.registry-provider-title {
  font-weight: 600;
  flex: 1;
  font-size: 0.9em;
  color: var(--text-primary);
}

.registry-provider-meta-id {
  font-size: 0.78em;
  color: var(--text-tertiary);
  margin-right: 4px;
}

.registry-provider-meta-count {
  font-size: 0.78em;
  color: var(--text-tertiary);
}

.registry-provider-chevron {
  transition: transform 0.2s ease;
  color: var(--text-secondary);
  flex-shrink: 0;
}
.registry-provider-chevron.expanded {
  transform: rotate(180deg);
}

.registry-provider-details {
  padding: 12px 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  border-top: 1px solid var(--border-subtle);
  background: var(--bg-primary);
}

.registry-form-row {
  display: grid;
  grid-template-columns: 1fr 2.2fr;
  gap: 8px;
  align-items: center;
}

.registry-form-label {
  font-size: 0.82em;
  color: var(--text-secondary);
}

.registry-models-section {
  margin-top: 4px;
}

.registry-models-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 6px;
}

.registry-models-title {
  font-size: 0.82em;
  font-weight: 600;
  color: var(--text-secondary);
}

.registry-model-row {
  display: flex;
  gap: 6px;
  align-items: center;
  margin-bottom: 4px;
}

.registry-add-provider-card {
  border: 1px dashed var(--border-default);
  border-radius: 8px;
  padding: 12px 14px;
  background: var(--bg-secondary);
  display: flex;
  flex-direction: column;
}

.registry-add-provider-title {
  font-size: 0.82em;
  font-weight: 600;
  color: var(--text-secondary);
  margin-bottom: 8px;
  display: block;
}

.registry-add-provider-form {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
</style>
