<template>
  <!-- 📱 通讯渠道 (Communication Channels) -->
  <section class="settings-section card">
    <h3 class="section-title">
      <Smartphone :size="16" class="section-icon" />
      {{ $t('settings.mobile_assistant') }}
    </h3>
    
    <div class="service-cards-grid">
      <!-- WeChat -->
      <div class="service-card" :class="{ active: mobileChannel === 'wechat' }">
        <div class="service-card-header" @click="mobileChannel === 'wechat' ? openWechatModal() : mobileChannel = 'wechat'" style="cursor: pointer; margin-bottom: 0;">
          <div class="service-icon" style="background: rgba(7,193,96,0.1); color: #07c160; display: flex; align-items: center; justify-content: center;">
            <svg viewBox="-51.45 -69.25 445.9 415.5" xmlns="http://www.w3.org/2000/svg" style="width: 20px; height: 20px; fill: currentColor;">
              <g fill="currentColor" fill-rule="evenodd">
                <path d="M274 167c-7.778 0-14-6.222-14-14s6.222-14 14-14 14 6.222 14 14c0 7.389-6.222 14-14 14m-69 0c-7.778 0-14-6.222-14-14s6.222-14 14-14 14 6.222 14 14c0 7.389-6.222 14-14 14m102.39 78.581C329.216 229.871 343 206.5 343 180.827 343 133.316 297.052 95 240 95s-103 38.316-103 85.827c0 47.512 45.948 85.828 103 85.828 11.87 0 22.974-1.533 33.695-4.598.766-.383 1.915-.383 3.063-.383 1.915 0 3.83.766 5.361 1.532l22.591 13.028c.766.383 1.149.766 1.915.766a3.433 3.433 0 003.446-3.448c0-.767-.383-1.533-.383-2.683 0-.383-3.063-10.728-4.595-17.242-.383-.766-.383-1.532-.383-2.299-.383-2.682.766-4.597 2.68-5.747"/>
                <path d="M164 86c-8.93 0-16-7.07-16-16s7.07-16 16-16 16 7.07 16 16c0 8.558-7.07 16-16 16m-82 0c-8.93 0-16-7.07-16-16s7.07-16 16-16 16 7.07 16 16c0 8.558-7.07 16-16 16m41.96-86C55.646 0 0 45.895 0 102.88c0 30.98 16.502 58.899 42.983 77.64 1.919 1.53 3.454 3.824 3.454 6.884 0 .764-.384 1.912-.384 2.677-1.919 7.649-5.373 20.27-5.757 20.652-.383 1.148-.767 1.913-.767 3.06 0 2.295 1.919 4.207 4.221 4.207.768 0 1.535-.382 2.303-.765l27.248-15.68c1.919-1.148 4.222-1.913 6.524-1.913 1.152 0 2.303 0 3.454.383 12.665 3.442 26.48 5.736 40.297 5.736h6.908c-2.687-8.031-4.222-16.445-4.222-25.242 0-51.631 50.658-93.701 112.83-93.701H246C237.173 37.48 185.747 0 123.96 0"/>
              </g>
            </svg>
          </div>
          <div class="service-info">
            <span class="service-name">{{ $t('settings.channel_wechat') }}</span>
            <span class="service-sub" v-if="mobileChannel === 'wechat'" style="color: var(--user-accent);">{{ wechatConnected ? $t('settings.channel_wechat_rebind') : $t('settings.channel_wechat_scan') }}</span>
            <span class="service-sub" v-else>{{ $t('settings.channel_wechat_desc') }}</span>
          </div>
          <span class="service-status-dot" :class="mobileChannel === 'wechat' ? 'dot-connected' : 'dot-disconnected'"></span>
        </div>
      </div>

      <!-- Telegram -->
      <div class="service-card" :class="{ active: mobileChannel === 'telegram' }">
        <div class="service-card-header" @click="mobileChannel = 'telegram'" style="cursor: pointer; margin-bottom: 0;">
          <div class="service-icon" style="background: rgba(42,171,238,0.1); color: #2aabee; display: flex; align-items: center; justify-content: center;">
            <svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg" style="width: 18px; height: 18px; fill: currentColor;">
              <path d="M29.919 6.163l-4.225 19.925c-0.319 1.406-1.15 1.756-2.331 1.094l-6.438-4.744-3.106 2.988c-0.344 0.344-0.631 0.631-1.294 0.631l0.463-6.556 11.931-10.781c0.519-0.462-0.113-0.719-0.806-0.256l-14.75 9.288-6.35-1.988c-1.381-0.431-1.406-1.381 0.288-2.044l24.837-9.569c1.15-0.431 2.156 0.256 1.781 2.013z"/>
            </svg>
          </div>
          <div class="service-info">
            <span class="service-name">{{ $t('settings.channel_telegram') }}</span>
            <span class="service-sub">Telegram Bot</span>
          </div>
          <span class="service-status-dot" :class="tgToken ? 'dot-connected' : 'dot-disconnected'"></span>
        </div>
        <div class="service-card-body" v-if="mobileChannel === 'telegram'" style="border-top: 1px solid var(--border-default); padding-top: 12px; margin-top: 14px;">
          <div class="input-group" style="margin-bottom: 0;">
            <label style="font-size: 0.85em; color: var(--text-secondary); margin-bottom: 6px; display: block;">Bot Token</label>
            <div style="display: flex; flex-direction: column; gap: 8px;">
              <input type="password" v-model="tgToken" placeholder="123456789:ABCdefGHIjklMNO..." class="input" />
              <button class="btn btn-primary" @click="activateMobileChannel('telegram')" style="width: 100%;">
                <svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg" style="width: 14px; height: 14px; fill: currentColor; flex-shrink: 0;"><path d="M29.919 6.163l-4.225 19.925c-0.319 1.406-1.15 1.756-2.331 1.094l-6.438-4.744-3.106 2.988c-0.344 0.344-0.631 0.631-1.294 0.631l0.463-6.556 11.931-10.781c0.519-0.462-0.113-0.719-0.806-0.256l-14.75 9.288-6.35-1.988c-1.381-0.431-1.406-1.381 0.288-2.044l24.837-9.569c1.15-0.431 2.156 0.256 1.781 2.013z"/></svg> {{ $t('settings.channel_activate') }}
              </button>
            </div>
            <p class="field-hint" style="margin-top: 8px; margin-bottom: 0;">{{ $t('settings.channel_tg_hint') }}</p>
          </div>
        </div>
      </div>

      <!-- Discord -->
      <div class="service-card" :class="{ active: mobileChannel === 'discord' }">
        <div class="service-card-header" @click="mobileChannel = 'discord'" style="cursor: pointer; margin-bottom: 0;">
          <div class="service-icon" style="background: rgba(88,101,242,0.1); color: #5865F2; display: flex; align-items: center; justify-content: center;">
            <svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" style="width: 20px; height: 20px; fill: currentColor;">
              <path d="M18.59 5.88997C17.36 5.31997 16.05 4.89997 14.67 4.65997C14.5 4.95997 14.3 5.36997 14.17 5.69997C12.71 5.47997 11.26 5.47997 9.83001 5.69997C9.69001 5.36997 9.49001 4.95997 9.32001 4.65997C7.94001 4.89997 6.63001 5.31997 5.40001 5.88997C2.92001 9.62997 2.25001 13.28 2.58001 16.87C4.23001 18.1 5.82001 18.84 7.39001 19.33C7.78001 18.8 8.12001 18.23 8.42001 17.64C7.85001 17.43 7.31001 17.16 6.80001 16.85C6.94001 16.75 7.07001 16.64 7.20001 16.54C10.33 18 13.72 18 16.81 16.54C16.94 16.65 17.07 16.75 17.21 16.85C16.7 17.16 16.15 17.42 15.59 17.64C15.89 18.23 16.23 18.8 16.62 19.33C18.19 18.84 19.79 18.1 21.43 16.87C21.82 12.7 20.76 9.08997 18.61 5.88997H18.59ZM8.84001 14.67C7.90001 14.67 7.13001 13.8 7.13001 12.73C7.13001 11.66 7.88001 10.79 8.84001 10.79C9.80001 10.79 10.56 11.66 10.55 12.73C10.55 13.79 9.80001 14.67 8.84001 14.67ZM15.15 14.67C14.21 14.67 13.44 13.8 13.44 12.73C13.44 11.66 14.19 10.79 15.15 10.79C16.11 10.79 16.87 11.66 16.86 12.73C16.86 13.79 16.11 14.67 15.15 14.67Z"/>
            </svg>
          </div>
          <div class="service-info">
            <span class="service-name">{{ $t('settings.channel_discord') }}</span>
            <span class="service-sub">Discord Bot</span>
          </div>
          <span class="service-status-dot" :class="discordToken ? 'dot-connected' : 'dot-disconnected'"></span>
        </div>
        <div class="service-card-body" v-if="mobileChannel === 'discord'" style="border-top: 1px solid var(--border-default); padding-top: 12px; margin-top: 14px;">
          <div class="input-group" style="margin-bottom: 0;">
            <label style="font-size: 0.85em; color: var(--text-secondary); margin-bottom: 6px; display: block;">Bot Token</label>
            <div style="display: flex; flex-direction: column; gap: 8px;">
              <input type="password" v-model="discordToken" placeholder="OTg3NjU0MzIx.ABC.defGHIjklMNO..." class="input" />
              <button class="btn btn-primary" @click="activateMobileChannel('discord')" style="width: 100%;">
                <svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" style="width: 14px; height: 14px; fill: currentColor; flex-shrink: 0;"><path d="M18.59 5.88997C17.36 5.31997 16.05 4.89997 14.67 4.65997C14.5 4.95997 14.3 5.36997 14.17 5.69997C12.71 5.47997 11.26 5.47997 9.83001 5.69997C9.69001 5.36997 9.49001 4.95997 9.32001 4.65997C7.94001 4.89997 6.63001 5.31997 5.40001 5.88997C2.92001 9.62997 2.25001 13.28 2.58001 16.87C4.23001 18.1 5.82001 18.84 7.39001 19.33C7.78001 18.8 8.12001 18.23 8.42001 17.64C7.85001 17.43 7.31001 17.16 6.80001 16.85C6.94001 16.75 7.07001 16.64 7.20001 16.54C10.33 18 13.72 18 16.81 16.54C16.94 16.65 17.07 16.75 17.21 16.85C16.7 17.16 16.15 17.42 15.59 17.64C15.89 18.23 16.23 18.8 16.62 19.33C18.19 18.84 19.79 18.1 21.43 16.87C21.82 12.7 20.76 9.08997 18.61 5.88997H18.59ZM8.84001 14.67C7.90001 14.67 7.13001 13.8 7.13001 12.73C7.13001 11.66 7.88001 10.79 8.84001 10.79C9.80001 10.79 10.56 11.66 10.55 12.73C10.55 13.79 9.80001 14.67 8.84001 14.67ZM15.15 14.67C14.21 14.67 13.44 13.8 13.44 12.73C13.44 11.66 14.19 10.79 15.15 10.79C16.11 10.79 16.87 11.66 16.86 12.73C16.86 13.79 16.11 14.67 15.15 14.67Z"/></svg> {{ $t('settings.channel_activate') }}
              </button>
            </div>
            <p class="field-hint" style="margin-top: 8px; margin-bottom: 0;">{{ $t('settings.channel_discord_hint') }}</p>
          </div>
        </div>
      </div>
    </div>
  </section>

  <!-- 🏢 办公服务 (Office Services) -->
  <section class="settings-section card">
    <h3 class="section-title">
      <Building2 :size="16" class="section-icon" />
      {{ $t('settings.conn_office_services') }}
    </h3>

    <div class="service-cards-grid">
      <!-- 飞书 (Feishu / Lark) -->
      <div class="service-card" :class="{ connected: isConnected('lark') }">
        <div class="service-card-header">
          <div class="service-icon lark-icon">
            <img src="/logos/feishu.svg" style="width: 22px; height: 22px; object-fit: contain;" alt="Feishu" />
          </div>
          <div class="service-info">
            <span class="service-name">{{ $t('settings.conn_lark_name') }}</span>
            <span class="service-sub">{{ $t('settings.conn_lark_desc') }}</span>
          </div>
          <span class="service-status-dot" :class="isConnected('lark') ? 'dot-connected' : 'dot-disconnected'"></span>
        </div>

        <div class="service-card-body">
          <span class="service-status-text">
            <template v-if="isConnected('lark')">
              {{ $t('settings.conn_connected') }}
              <span v-if="connectorStatuses['lark']?.connected_at" class="connected-time">
                · {{ connectorStatuses['lark'].connected_at }}
              </span>
            </template>
            <template v-else>{{ $t('settings.conn_not_configured') }}</template>
          </span>
        </div>

        <!-- 飞书凭证表单 (内联展开) -->
        <Transition name="slide-fade">
          <div v-if="showLarkForm" class="lark-credential-form">
            <div class="form-group" style="margin-bottom: 10px;">
              <label class="form-label">{{ $t('settings.conn_lark_app_id') }}</label>
              <input v-model="larkCreds.app_id" class="input" placeholder="cli_xxxxxxxxxxxxxx" />
            </div>
            <div class="form-group" style="margin-bottom: 10px;">
              <label class="form-label">{{ $t('settings.conn_lark_app_secret') }}</label>
              <input v-model="larkCreds.app_secret" type="password" class="input" placeholder="••••••••••••••••" />
            </div>
            <div style="display: flex; gap: 8px;">
              <button
                class="btn btn-primary btn-sm"
                @click="saveLarkCredentials"
                :disabled="!larkCreds.app_id || !larkCreds.app_secret || connectingService === 'lark'"
              >
                <Check :size="13" />
                {{ $t('settings.conn_save_connect') }}
              </button>
              <button class="btn btn-sm" @click="showLarkForm = false">
                {{ $t('settings.mcp_cancel') }}
              </button>
            </div>
          </div>
        </Transition>

        <div class="service-card-footer">
          <button
            v-if="isConnected('lark')"
            class="btn btn-danger-outline btn-sm"
            @click="disconnectService('lark')"
          >
            <Unlink :size="13" />
            {{ $t('settings.conn_disconnect') }}
          </button>
          <button
            v-else
            class="btn btn-primary btn-sm"
            @click="showLarkForm = !showLarkForm"
          >
            <KeyRound :size="13" />
            {{ $t('settings.conn_connect') }}
          </button>
        </div>
      </div>

      <!-- Google Calendar (Native) -->
      <div class="service-card" :class="{ connected: isConnected('google') }">
        <div class="service-card-header">
          <div class="service-icon" style="background: transparent;">
            <img src="/logos/google.svg" style="width: 22px; height: 22px; object-fit: contain;" alt="Google" />
          </div>
          <div class="service-info">
            <span class="service-name">Google Calendar</span>
            <span class="service-sub">{{ $t('settings.conn_native_integration') }}</span>
          </div>
          <span class="service-status-dot" :class="isConnected('google') ? 'dot-connected' : 'dot-disconnected'"></span>
        </div>

        <div class="service-card-body">
          <span class="service-status-text">
            <template v-if="isConnected('google')">
              {{ $t('settings.conn_connected') }}
              <span v-if="connectorStatuses['google']?.connected_at" class="connected-time">
                · {{ connectorStatuses['google'].connected_at }}
              </span>
            </template>
            <template v-else>{{ $t('settings.conn_not_configured') }}</template>
          </span>
        </div>

        <div class="service-card-footer">
          <button
            v-if="isConnected('google')"
            class="btn btn-danger-outline btn-sm"
            @click="disconnectService('google')"
          >
            <Unlink :size="13" />
            {{ $t('settings.conn_disconnect') }}
          </button>
          <button
            v-else
            class="btn btn-primary btn-sm"
            @click="connectGoogleNative"
            :disabled="connectingService === 'google'"
          >
            <KeyRound :size="13" />
            {{ $t('settings.conn_select_credential') }}
          </button>
        </div>
      </div>

    </div>
  </section>

  <!-- 🔧 自定义 MCP 扩展 (Custom MCP Extensions) -->
  <section class="settings-section card">
    <h3 class="section-title">
      <Unplug :size="16" class="section-icon" />
      {{ $t('settings.mcp_servers') }}
    </h3>

    <div class="service-cards-grid">
      <!-- 已配置的 MCP Servers -->
      <div
        v-for="(cfg, name) in mcpServers"
        :key="name"
        class="service-card active"
      >
        <div class="service-card-header" style="margin-bottom: 0;">
          <div class="service-icon" style="background: rgba(142, 142, 147, 0.1); color: var(--text-secondary); display: flex; align-items: center; justify-content: center;">
            <img v-if="name.toLowerCase().includes('google')" src="/logos/google.svg" style="width: 20px; height: 20px; object-fit: contain;" />
            <img v-else-if="name.toLowerCase().includes('outlook')" src="/logos/outlook.svg" style="width: 20px; height: 20px; object-fit: contain;" />
            <Terminal v-else :size="18" />
          </div>
          <div class="service-info" style="min-width: 0; padding-right: 8px;">
            <span class="service-name" style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap; display: block;" :title="name">{{ name === 'GoogleCalendar' ? 'Google Calendar' : (name === 'Outlook365' ? 'Outlook 365' : name) }}</span>
            <span class="service-sub" style="font-size: 0.75em; color: var(--text-tertiary); display: flex; align-items: center; gap: 4px;">
              {{ name.toLowerCase().includes('google') || name.toLowerCase().includes('outlook') ? $t('settings.mcp_preset') : $t('settings.mcp_custom') }}
            </span>
          </div>
          <label class="mcp-switch" title="断开连接">
            <input type="checkbox" checked @change="removeMcpServer(name)" />
            <span class="mcp-slider"></span>
          </label>
        </div>
        <div v-if="!name.toLowerCase().includes('google') && !name.toLowerCase().includes('outlook')" class="service-card-body" style="border-top: 1px solid var(--border-subtle); padding-top: 8px; margin-top: 8px; font-family: monospace; font-size: 0.7em; word-break: break-all; color: var(--text-secondary);">
          {{ cfg.command }} {{ (cfg.args || []).join(' ') }}
        </div>
      </div>

      <!-- Quick Add Outlook MCP -->
      <div v-if="!showAddMcp && !mcpServers['Outlook365']" class="service-card preset-card" @click="addOutlookMcpPreset">
        <div class="service-card-header" style="margin-bottom: 0;">
          <div class="service-icon" style="background: transparent;">
            <img src="/logos/outlook.svg" style="width: 20px; height: 20px; filter: grayscale(1); opacity: 0.6;" alt="Outlook" />
          </div>
          <div class="service-info">
            <span class="service-name" style="color: var(--text-secondary);">Outlook 365</span>
            <span class="service-sub">{{ $t('settings.conn_quick_connect') }}</span>
          </div>
          <label class="mcp-switch" title="接入服务" @click.prevent>
            <input type="checkbox" :checked="false" />
            <span class="mcp-slider"></span>
          </label>
        </div>
      </div>

      <!-- 添加自定义 MCP Server 卡片 -->
      <div v-if="!showAddMcp" class="service-card preset-card" @click="showAddMcp = true">
        <div class="service-card-header" style="margin-bottom: 0;">
          <div class="service-icon" style="background: transparent; color: var(--text-tertiary);">
            <Plus :size="20" />
          </div>
          <div class="service-info">
            <span class="service-name" style="color: var(--text-secondary);">{{ $t('settings.mcp_add') }}</span>
            <span class="service-sub">{{ $t('settings.mcp_custom') }}</span>
          </div>
        </div>
      </div>
      
      <!-- 添加表单卡片 -->
      <div v-else class="service-card active" style="grid-column: 1 / -1;">
        <div class="service-card-body" style="padding: 12px; margin-top: 0; display: flex; flex-direction: column; gap: 8px;">
          <div class="form-group" style="margin: 0;">
            <label class="form-label" style="font-size: 0.8em; margin-bottom: 4px;">{{ $t('settings.mcp_name') }}</label>
            <input v-model="newMcp.name" class="input" placeholder="例如 filesystem" style="padding: 4px 8px; font-size: 0.85em;" />
          </div>
          <div class="form-group" style="margin: 0;">
            <label class="form-label" style="font-size: 0.8em; margin-bottom: 4px;">{{ $t('settings.mcp_command') }}</label>
            <input v-model="newMcp.command" class="input" placeholder="npx" style="padding: 4px 8px; font-size: 0.85em;" />
          </div>
          <div class="form-group" style="margin: 0;">
            <label class="form-label" style="font-size: 0.8em; margin-bottom: 4px;">{{ $t('settings.mcp_args') }}</label>
            <input v-model="newMcp.args" class="input" placeholder="-y @modelcontextprotocol/server-filesystem /path" style="padding: 4px 8px; font-size: 0.85em;" />
            <div v-if="newMcp.name === 'Outlook365'" style="margin-top: 6px; font-size: 0.75em; color: var(--text-tertiary); line-height: 1.5; background: var(--bg-root); padding: 8px; border-radius: 6px;">
              <div style="display: flex; align-items: center; margin-bottom: 4px; color: var(--text-secondary);">
                <Info :size="14" style="margin-right: 4px;" /> <b>{{ $t('settings.mcp_outlook_guide') }}</b>
              </div>
              <div style="display: flex; align-items: center; gap: 6px; margin-top: 4px;">
                <span>{{ $t('settings.mcp_outlook_step1') }}</span>
                <span style="user-select: text; color: var(--color-primary);">https://portal.azure.com/</span>
                <button class="btn btn-sm" style="padding: 4px; border: none; background: transparent; cursor: pointer; color: var(--text-tertiary);" @click.stop="copyUrl('https://portal.azure.com/')">
                  <Check v-if="copiedUrl === 'https://portal.azure.com/'" :size="14" style="color: var(--color-success);" />
                  <Copy v-else :size="14" />
                </button>
              </div>
              <div style="display: flex; align-items: center; gap: 6px; margin-top: 4px;">
                <span>{{ $t('settings.mcp_outlook_step2') }}</span>
                <span style="user-select: text; color: var(--color-primary);">https://github.com/smithery-ai/mcp-server-outlook</span>
                <button class="btn btn-sm" style="padding: 4px; border: none; background: transparent; cursor: pointer; color: var(--text-tertiary);" @click.stop="copyUrl('https://github.com/smithery-ai/mcp-server-outlook')">
                  <Check v-if="copiedUrl === 'https://github.com/smithery-ai/mcp-server-outlook'" :size="14" style="color: var(--color-success);" />
                  <Copy v-else :size="14" />
                </button>
              </div>
            </div>
          </div>
        </div>
        <div class="service-card-footer" style="padding: 0 12px 12px; gap: 8px; margin-top: auto;">
          <button class="btn btn-primary btn-sm" @click="addMcpServer" :disabled="!newMcp.name || !newMcp.command" style="flex: 1; justify-content: center;">
            <Check :size="13" /> {{ $t('settings.mcp_save') }}
          </button>
          <button class="btn btn-sm" @click="showAddMcp = false" style="flex: 1; justify-content: center;">
            {{ $t('settings.mcp_cancel') }}
          </button>
        </div>
      </div>
    </div>
  </section>

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
</template>

<script setup>
import { ref, onMounted, onUnmounted } from 'vue';
import { open } from '@tauri-apps/plugin-dialog';
import {
  Smartphone, Unplug, X, Plus, Loader2, MessageSquare, Check,
  Building2, ExternalLink, Unlink, KeyRound, Terminal, Info, Copy
} from 'lucide-vue-next';

const props = defineProps({
  config: { type: Object, required: true },
});
const emit = defineEmits(['config-changed']);

// ── Mobile channels ──
const mobileChannel = ref('wechat');
const tgToken = ref('');
const discordToken = ref('');

const showWechatModal = ref(false);
const qrCodeUrl = ref('');
const wechatConnected = ref(false);
const rawQrCode = ref('');
let wechatPollTimer = null;

const copiedUrl = ref('');
async function copyUrl(url) {
  try {
    await navigator.clipboard.writeText(url);
    copiedUrl.value = url;
    setTimeout(() => { copiedUrl.value = ''; }, 2000);
  } catch(e) {
    console.error('Failed to copy', e);
  }
}

async function activateMobileChannel(channel) {
  if (channel === 'telegram') {
    if (!tgToken.value) {
      alert('请填写 Telegram Bot Token');
      return;
    }
    try {
      await window.electronAPI.telegramSaveToken(tgToken.value);
      alert('Telegram 绑定成功！机器人已在后台启动。');
    } catch(e) {
      alert('绑定失败: ' + e);
    }
  } else if (channel === 'discord') {
    if (!discordToken.value) {
      alert('请填写 Discord Bot Token');
      return;
    }
    try {
      await window.electronAPI.discordSaveToken(discordToken.value);
      alert('Discord 绑定成功！机器人已在后台启动。');
    } catch(e) {
      alert('绑定失败: ' + e);
    }
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

// ── 办公服务连接器 (Office Connectors) ──
const connectorStatuses = ref({});
const connectingService = ref('');
const showLarkForm = ref(false);
const larkCreds = ref({ app_id: '', app_secret: '' });

function isConnected(name) {
  return connectorStatuses.value[name]?.status === 'connected';
}

async function loadConnectorStatuses() {
  if (!window.electronAPI.connectorList) return;
  try {
    const list = await window.electronAPI.connectorList();
    for (const c of list) {
      connectorStatuses.value[c.name] = c;
    }
  } catch (e) {
    console.warn('Failed to load connector statuses:', e);
  }
}

async function connectOAuth(name) {
  if (!window.electronAPI.connectorStartOAuth) return;
  connectingService.value = name;
  try {
    const res = await window.electronAPI.connectorStartOAuth(name);
    if (res && res.url) {
      // 使用默认浏览器打开 OAuth 授权页面
      window.electronAPI.openExternal(res.url);
    } else if (res && res.error) {
      alert('OAuth Error: ' + res.error);
    }
  } catch (e) {
    console.error('OAuth start failed:', e);
    alert('连接失败: ' + e);
  } finally {
    connectingService.value = '';
    // 延迟刷新状态，等用户完成 OAuth 回调
    setTimeout(loadConnectorStatuses, 3000);
  }
}

async function saveLarkCredentials() {
  if (!window.electronAPI.connectorSaveCredentials) return;
  connectingService.value = 'lark';
  try {
    await window.electronAPI.connectorSaveCredentials('lark', {
      app_id: larkCreds.value.app_id,
      app_secret: larkCreds.value.app_secret,
    });
    showLarkForm.value = false;
    larkCreds.value = { app_id: '', app_secret: '' };
    await loadConnectorStatuses();
  } catch (e) {
    console.error('Failed to save Lark credentials:', e);
    alert('保存失败: ' + e);
  } finally {
    connectingService.value = '';
  }
}

async function disconnectService(name) {
  if (!window.electronAPI.connectorDisconnect) return;
  try {
    await window.electronAPI.connectorDisconnect(name);
    delete connectorStatuses.value[name];
  } catch (e) {
    console.error('Disconnect failed:', e);
  }
}

// ── MCP 配置管理 ──
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

async function connectGoogleNative() {
  try {
    const selectedPath = await open({
      multiple: false,
      title: '选择 Google OAuth credentials.json',
      filters: [{ name: 'JSON Credentials', extensions: ['json'] }]
    });
    
    if (selectedPath) {
      connectingService.value = 'google';
      const res = await window.electronAPI.connectorSaveCredentials('google', {
        file_path: selectedPath
      });
      if (res && res.error) {
        alert('配置失败: ' + res.error);
        connectingService.value = '';
        return;
      }
      await loadConnectorStatuses();
      await connectOAuth('google');
    }
  } catch (err) {
    console.error('Failed to configure Google Calendar natively', err);
    alert('配置失败: ' + err);
    connectingService.value = '';
  }
}

function addOutlookMcpPreset() {
  newMcp.value = {
    name: 'Outlook365',
    command: 'npx',
    args: '-y @smithery/mcp-server-outlook --client-id <YOUR_CLIENT_ID> --tenant-id <YOUR_TENANT_ID>'
  };
  showAddMcp.value = true;
}

async function removeMcpServer(name) {
  const updated = { ...mcpServers.value };
  delete updated[name];
  await window.electronAPI.setMcpConfig({ mcpServers: updated });
  mcpServers.value = updated;
}

// ── Init ──
onMounted(async () => {
  await loadMcpConfig();
  await loadConnectorStatuses();
  if (window.electronAPI.wechatGetCurrentStatus) {
    try {
      const res = await window.electronAPI.wechatGetCurrentStatus();
      if (res && res.connected) {
        wechatConnected.value = true;
      }
    } catch(err) {}
  }
  
  if (window.electronAPI.telegramGetToken) {
    try {
      const res = await window.electronAPI.telegramGetToken();
      if (res && res.token) {
        tgToken.value = res.token;
      }
    } catch(err) {}
  }

  if (window.electronAPI.discordGetToken) {
    try {
      const res = await window.electronAPI.discordGetToken();
      if (res && res.token) {
        discordToken.value = res.token;
      }
    } catch(err) {}
  }
});

onUnmounted(() => {
  if (wechatPollTimer) {
    clearTimeout(wechatPollTimer);
    wechatPollTimer = null;
  }
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

/* ── Service Card Active State (For Mutually Exclusive Cards) ── */
.service-card.active {
  border-color: var(--user-accent, var(--accent-primary, #4facfe));
  box-shadow: 0 0 0 1px var(--user-accent, var(--accent-primary, #4facfe));
}

/* ── Office service cards grid ── */
.service-cards-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px;
  align-items: start;
}
@media (max-width: 900px) {
  .service-cards-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
@media (max-width: 600px) {
  .service-cards-grid {
    grid-template-columns: minmax(0, 1fr);
  }
}

.preset-card {
  cursor: pointer;
  border-style: dashed;
}
.preset-card:hover {
  border-color: var(--user-accent, var(--accent-primary, #4facfe));
  background: color-mix(in srgb, var(--user-accent, var(--accent-primary, #4facfe)) 5%, transparent);
}

.mcp-switch {
  position: relative;
  display: inline-block;
  width: 32px;
  height: 18px;
  margin-left: auto;
  flex-shrink: 0;
}
.mcp-switch input { 
  opacity: 0;
  width: 0;
  height: 0;
}
.mcp-slider {
  position: absolute;
  cursor: pointer;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background-color: color-mix(in srgb, var(--color-success) 80%, transparent);
  transition: .3s;
  border-radius: 34px;
}
.mcp-slider:before {
  position: absolute;
  content: "";
  height: 14px;
  width: 14px;
  left: 2px;
  bottom: 2px;
  background-color: white;
  transition: .3s;
  border-radius: 50%;
  box-shadow: 0 1px 2px rgba(0,0,0,0.2);
}
.mcp-switch input:not(:checked) + .mcp-slider {
  background-color: var(--border-strong);
}
.mcp-switch input:not(:checked) + .mcp-slider:before {
  transform: translateX(0);
}
.mcp-switch input:checked + .mcp-slider:before {
  transform: translateX(14px);
}

.service-card {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  background: var(--bg-secondary);
  padding: 12px 16px;
  transition: border-color var(--duration-fast) var(--ease-out),
              box-shadow var(--duration-fast) var(--ease-out);
}
.service-card:hover {
  border-color: var(--text-muted);
  box-shadow: var(--shadow-sm);
}
.service-card.connected {
  border-color: color-mix(in srgb, var(--color-success) 40%, var(--border-subtle));
}

.service-card-header {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 8px;
}

.service-icon {
  width: 36px;
  height: 36px;
  border-radius: var(--radius-sm);
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  background: var(--bg-tertiary);
  color: var(--text-secondary);
}

.service-info {
  display: flex;
  flex-direction: column;
  min-width: 0;
  flex: 1;
}

.service-name {
  font-weight: 600;
  font-size: 14px;
  color: var(--text-primary);
  line-height: 1.2;
}

.service-sub {
  font-size: 11px;
  color: var(--text-tertiary);
  margin-top: 2px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.service-status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}
.dot-connected {
  background: var(--color-success);
}
.dot-disconnected {
  background: var(--text-muted);
}

.service-card-body {
  margin-bottom: 8px;
}

.service-status-text {
  font-size: 12px;
  color: var(--text-tertiary);
}

.connected-time {
  font-size: 11px;
  color: var(--text-muted);
}

.service-card-footer {
  margin-top: auto;
}

.btn-sm {
  padding: 5px 10px;
  font-size: 12px;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.btn-danger-outline {
  background: transparent;
  color: var(--color-error);
  border: 1px solid color-mix(in srgb, var(--color-error) 35%, transparent);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}
.btn-danger-outline:hover {
  background: color-mix(in srgb, var(--color-error) 10%, transparent);
  border-color: var(--color-error);
}

/* ── 飞书凭证表单展开 ── */
.lark-credential-form {
  padding: 12px;
  margin-top: 8px;
  margin-bottom: 8px;
  background: var(--surface-glass);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-sm);
}

.slide-fade-enter-active {
  transition: all 0.25s var(--ease-out);
}
.slide-fade-leave-active {
  transition: all 0.15s var(--ease-out);
}
.slide-fade-enter-from {
  opacity: 0;
  max-height: 0;
  transform: translateY(-8px);
}
.slide-fade-leave-to {
  opacity: 0;
  max-height: 0;
  transform: translateY(-8px);
}

/* ── WeChat QR modal ── */
.wechat-modal-overlay {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: color-mix(in srgb, var(--bg-root) 60%, transparent);
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
  border-radius: var(--radius-lg, 12px);
  box-shadow: var(--shadow-lg, 0 8px 32px rgba(0,0,0,0.2));
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
</style>
