<template>


  <!-- 📱 通讯渠道 (Communication Channels) -->
  <section class="settings-section card">
    <h3 class="section-title">
      <Smartphone :size="16" class="section-icon" />
      {{ $t('settings.mobile_assistant') }}
    </h3>
    
    <div class="service-cards-grid">
      <!-- 🔄 多端同步 (P2P Sync) -->
      <div class="service-card static-card" >
        <div class="service-card-header">
          <div class="service-icon" :style="{ background: isUnlocked ? 'rgba(var(--user-accent-rgb, 39,118,187), 0.1)' : 'var(--bg-tertiary)', color: isUnlocked ? 'var(--user-accent)' : 'var(--text-muted)' }">
            <Smartphone :size="20" />
          </div>
          <div class="service-info">
            <span class="service-name">{{ $t('settings.p2p_pairing') }}</span>
            <span class="service-sub">{{ !isUnlocked ? $t('settings.p2p_auth_desc_new') : $t('settings.p2p_pairing_desc') }}</span>
          </div>
          <div style="display: flex; align-items: center; gap: 8px;">
            <button 
              v-if="connectedDevices.length > 0" 
              class="device-indicator-btn"
              @click.stop="showDevicesModal = true" 
              :title="connectedDevices.map(d => `${d.platform === 'android' ? 'Android' : d.platform} (${d.device_id.substring(0, 8)})`).join('\n')"
            >
              <Smartphone :size="12" />
            </button>
            <span class="service-status-dot" :class="isUnlocked ? 'dot-connected' : 'dot-disconnected'"></span>
          </div>
        </div>
        
        <div class="service-card-footer">
          <div v-if="!isUnlocked" style="display: flex; gap: 8px; width: 100%;">
            <template v-if="isMobile">
              <button class="btn btn-primary-outline btn-sm" style="flex: 1; justify-content: center;" @click="handleMobileScan" title="扫码配对">
                <Scan :size="13" style="margin-right: 6px;" /> 扫码配对
              </button>
            </template>
            <template v-else>
              <input v-model="pinInput" type="password" class="input" maxlength="6" placeholder="PIN" style="flex: 1; min-width: 0; height: 28px; padding: 4px 8px; font-size: 12px; border-radius: var(--radius-sm);" @keyup.enter="handlePinSubmit" />
              <button class="btn btn-primary-outline btn-sm" style="padding: 0 10px; flex-shrink: 0; height: 28px;" :disabled="pinInput.length < 4" @click="handlePinSubmit" :title="isInitialized ? $t('settings.p2p_btn_unlock') : '设置 PIN 码'">
                <Lock v-if="isInitialized" :size="13" />
                <Check v-else :size="13" />
              </button>
              <button class="btn btn-ghost btn-sm" style="padding: 0 8px; flex-shrink: 0; height: 28px; opacity: 0.5; cursor: not-allowed;" disabled title="解锁后查看二维码">
                <QrCode :size="13" />
              </button>
            </template>
          </div>
          <div v-else style="display: flex; gap: 8px; width: 100%;">
            <template v-if="isMobile">
              <button class="btn btn-primary-outline btn-sm" style="flex: 1; justify-content: center;" @click="handleMobileScan" title="重新扫码配对">
                <Scan :size="13" style="margin-right: 6px;" /> 重新扫码
              </button>
            </template>
            <template v-else>
              <button class="btn btn-primary-outline btn-sm" style="flex: 1; justify-content: center; height: 28px;" @click="showP2pModal = true">
                <QrCode :size="13" style="margin-right: 6px;" /> 配对二维码
              </button>
            </template>
            <button class="btn btn-danger-outline btn-sm" style="padding: 5px 8px; height: 28px; flex-shrink: 0;" @click="handleReset" :title="$t('settings.p2p_btn_destroy')">
              <X :size="13" />
            </button>
          </div>
        </div>
      </div>

      <!-- WeChat -->
      <div class="service-card static-card" >
        <div class="service-card-header">
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
            <span class="service-sub">{{ $t('settings.channel_wechat_desc') }}</span>
          </div>
          <span class="service-status-dot" :class="wechatConnected ? 'dot-connected' : 'dot-disconnected'"></span>
        </div>
        <div class="service-card-footer">
          <button v-if="wechatConnected" class="btn btn-primary-outline btn-sm" @click="openWechatModal()">
            {{ $t('settings.channel_wechat_rebind') }}
          </button>
          <button v-else class="btn btn-primary-outline btn-sm" @click="openWechatModal()">
            <Scan :size="13" /> {{ $t('settings.channel_wechat_scan') }}
          </button>
        </div>
      </div>

      <!-- Telegram -->
      <div class="service-card static-card" >
        <div class="service-card-header">
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
        
        <Transition name="slide-fade">
          <div v-if="mobileChannel === 'telegram'" class="lark-credential-form">
            <div class="form-group" style="margin-bottom: 10px;">
              <label class="form-label">Bot Token</label>
              <input v-model="tgToken" type="password" class="input" placeholder="123456789:ABCdefGHIjklMNO..." />
              <p class="field-hint" style="margin-top: 8px; margin-bottom: 0;">{{ $t('settings.channel_tg_hint') }}</p>
            </div>
            <div style="display: flex; gap: 8px;">
              <button class="btn btn-primary btn-sm" @click="activateMobileChannel('telegram')" :disabled="!tgToken">
                <Check :size="13" /> {{ $t('settings.channel_activate') }}
              </button>
              <button class="btn btn-sm" @click="mobileChannel = ''">
                {{ $t('settings.mcp_cancel') }}
              </button>
            </div>
          </div>
        </Transition>

        <div class="service-card-footer">
          <button v-if="tgToken" class="btn btn-danger-outline btn-sm" @click="mobileChannel = mobileChannel === 'telegram' ? '' : 'telegram'">
            <Unlink :size="13" /> 修改 Token
          </button>
          <button v-else class="btn btn-primary-outline btn-sm" @click="mobileChannel = mobileChannel === 'telegram' ? '' : 'telegram'">
            <KeyRound :size="13" /> {{ $t('settings.conn_connect') }}
          </button>
        </div>
      </div>

      <!-- Discord -->
      <div class="service-card static-card" >
        <div class="service-card-header">
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
        
        <Transition name="slide-fade">
          <div v-if="mobileChannel === 'discord'" class="lark-credential-form">
            <div class="form-group" style="margin-bottom: 10px;">
              <label class="form-label">Bot Token</label>
              <input v-model="discordToken" type="password" class="input" placeholder="OTg3NjU0MzIx.ABC.defGHIjklMNO..." />
              <p class="field-hint" style="margin-top: 8px; margin-bottom: 0;">{{ $t('settings.channel_discord_hint') }}</p>
            </div>
            <div style="display: flex; gap: 8px;">
              <button class="btn btn-primary btn-sm" @click="activateMobileChannel('discord')" :disabled="!discordToken">
                <Check :size="13" /> {{ $t('settings.channel_activate') }}
              </button>
              <button class="btn btn-sm" @click="mobileChannel = ''">
                {{ $t('settings.mcp_cancel') }}
              </button>
            </div>
          </div>
        </Transition>

        <div class="service-card-footer">
          <button v-if="discordToken" class="btn btn-danger-outline btn-sm" @click="mobileChannel = mobileChannel === 'discord' ? '' : 'discord'">
            <Unlink :size="13" /> 修改 Token
          </button>
          <button v-else class="btn btn-primary-outline btn-sm" @click="mobileChannel = mobileChannel === 'discord' ? '' : 'discord'">
            <KeyRound :size="13" /> {{ $t('settings.conn_connect') }}
          </button>
        </div>
      </div>
    </div>

  </section>

  <!-- 🚇 内网穿墙隧道 (Network Proxy) -->
  <section class="settings-section card">
    <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px;">
      <h3 class="section-title" style="margin-bottom: 0;">
        <Network :size="16" class="section-icon" />
        {{ $t('settings.proxy_tunnel_name') }}
      </h3>
      <label class="mcp-switch">
        <input type="checkbox" :checked="proxyTunnelEnabled" @change="toggleProxyTunnel" />
        <span class="mcp-slider"></span>
      </label>
    </div>
    <div class="field-hint" style="margin-top: -8px; margin-bottom: 0; margin-left: 28px; color: var(--text-secondary);">
      {{ $t('settings.proxy_tunnel_desc') }}
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
      <div class="service-card static-card" :class="{ connected: isConnected('lark') }">
        <div class="service-card-header">
          <div class="service-icon lark-icon">
            <img :src="getAssetUrl('feishu.svg')" style="width: 22px; height: 22px; object-fit: contain;" alt="Feishu" />
          </div>
          <div class="service-info">
            <span class="service-name">{{ $t('settings.conn_lark_name') }}</span>
            <span class="service-sub">{{ $t('settings.conn_lark_desc') }}</span>
          </div>
          <span class="service-status-dot" :class="isConnected('lark') ? 'dot-connected' : 'dot-disconnected'"></span>
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
      <div class="service-card static-card" :class="{ connected: isConnected('google') }">
        <div class="service-card-header">
          <div class="service-icon" style="background: transparent;">
            <img :src="getAssetUrl('google.svg')" style="width: 22px; height: 22px; object-fit: contain;" alt="Google" />
          </div>
          <div class="service-info">
            <span class="service-name">Google Calendar</span>
            <span class="service-sub">{{ $t('settings.conn_native_integration') }}</span>
          </div>
          <span class="service-status-dot" :class="isConnected('google') ? 'dot-connected' : 'dot-disconnected'"></span>
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
            <img v-if="name.toLowerCase().includes('google')" :src="getAssetUrl('google.svg')" style="width: 20px; height: 20px; object-fit: contain;" />
            <img v-else-if="name.toLowerCase().includes('outlook')" :src="getAssetUrl('outlook.svg')" style="width: 20px; height: 20px; object-fit: contain;" />
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
            <img :src="getAssetUrl('outlook.svg')" style="width: 20px; height: 20px; filter: grayscale(1); opacity: 0.6;" alt="Outlook" />
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

  <!-- P2P 配对二维码弹窗 -->
  <Transition name="briefing-fade">
    <div v-if="showP2pModal" class="wechat-modal-overlay">
      <div class="morning-briefing wechat-qr-modal" style="width: 460px;">
        <div class="briefing-header">
          <div class="briefing-icon"><Smartphone :size="18" /></div>
          <div class="briefing-title" style="flex: 1; font-size: 14px; font-weight: 600; color: var(--text-primary);">{{ $t('settings.p2p_pairing') }}</div>
          <button class="briefing-close" @click="showP2pModal = false" title="关闭" style="background: none; border: none; color: var(--text-tertiary); cursor: pointer; padding: 4px; border-radius: 4px; display: flex; align-items: center; justify-content: center;">
            <X :size="14" />
          </button>
        </div>
        <div class="briefing-body" style="padding: 24px; display: flex; flex-direction: column;">
          <div style="display: flex; gap: 32px; align-items: stretch;">
            <!-- 左侧设备信息 -->
            <div style="flex: 1; display: flex; flex-direction: column; gap: 12px; justify-content: center;">
              <div class="form-group" style="margin-bottom: 0;">
                <label class="form-label" style="font-size: 0.8em;">{{ $t('settings.p2p_pc_device_id') }}</label>
                <input type="text" readonly class="input" :value="pairingInfo.device_id || $t('settings.loading')" style="width: 100%; font-family: monospace; font-size: 0.8em; color: var(--text-tertiary); padding: 6px 8px;" />
              </div>
              <div class="form-group" style="margin-bottom: 0;">
                <label class="form-label" style="font-size: 0.8em;">{{ $t('settings.p2p_local_ip') }}</label>
                <div style="display: flex; flex-wrap: wrap; gap: 6px;">
                  <span v-for="ip in pairingInfo.local_ips" :key="ip" class="tag" style="background: var(--bg-tertiary); padding: 2px 8px; border-radius: 4px; font-size: 0.8em;">
                    {{ ip }}
                  </span>
                  <span v-if="!pairingInfo.local_ips || pairingInfo.local_ips.length === 0" style="color: var(--text-tertiary); font-size: 0.8em;">{{ $t('settings.p2p_no_network') }}</span>
                </div>
              </div>
              <div class="form-group" style="margin-bottom: 0;">
                <label class="form-label" style="font-size: 0.8em;">{{ $t('settings.p2p_relay_server') }}</label>
                <div style="display: flex; align-items: center; gap: 8px;">
                  <span class="status-dot" style="background: var(--color-success); width: 8px; height: 8px; border-radius: 50%; display: inline-block;"></span>
                  <span style="font-size: 0.8em;">{{ pairingInfo.relay || 'wss://relay.bobbik.org' }}</span>
                </div>
              </div>
            </div>
            
            <!-- 右侧二维码或成功状态 -->
            <div style="width: 160px; display: flex; flex-direction: column; align-items: center; justify-content: center; background: white; padding: 12px; border-radius: 12px; box-shadow: 0 4px 12px rgba(0,0,0,0.1);">
              <div v-if="pairingSuccessInfo" style="display: flex; flex-direction: column; align-items: center; justify-content: center; width: 136px; height: 136px; background: #e8f5e9; border-radius: 8px;">
                <div style="background: var(--color-success); border-radius: 50%; width: 48px; height: 48px; display: flex; align-items: center; justify-content: center; margin-bottom: 12px;">
                  <Check style="color: white;" :size="24" />
                </div>
                <span style="color: var(--text-primary); font-size: 13px; font-weight: 600; text-align: center; line-height: 1.3;">已连接<br/><span style="color: var(--text-secondary); font-size: 11px;">{{ pairingSuccessInfo.device_id.substring(0,8) }}...</span></span>
              </div>
              <qrcode-vue v-else-if="qrPayload" :value="qrPayload" :size="136" level="M" />
              <div v-else style="width: 136px; height: 136px; display: flex; align-items: center; justify-content: center; background: #f0f0f0; border-radius: 8px;">
                <span style="color: #999; font-size: 0.8em;">{{ $t('settings.p2p_generating') }}</span>
              </div>
            </div>
          </div>
          <p style="color: var(--text-secondary); font-size: 0.85em; margin-top: 20px; text-align: center;">
            {{ $t('settings.p2p_scan_hint') }}
          </p>
        </div>
      </div>
    </div>
  </Transition>

  <!-- 📱 已连接设备列表弹窗 -->
  <Transition name="briefing-fade">
    <div v-if="showDevicesModal" class="wechat-modal-overlay" @click.self="showDevicesModal = false">
      <div class="morning-briefing wechat-qr-modal" style="width: 420px; border-radius: var(--radius-lg); background: var(--bg-secondary); border: 1px solid var(--border-subtle); overflow: hidden; box-shadow: var(--shadow-lg);">
        <div class="briefing-header" style="display: flex; align-items: center; justify-content: space-between; padding: 16px 20px; border-bottom: 1px solid var(--border-subtle); background: var(--bg-tertiary);">
          <div style="display: flex; align-items: center; gap: 8px;">
            <div class="briefing-icon" style="color: var(--user-accent, var(--accent-primary)); display: flex; align-items: center;"><Smartphone :size="18" /></div>
            <div class="briefing-title" style="font-size: 14px; font-weight: 600; color: var(--text-primary);">已配对的设备列表</div>
          </div>
          <button class="briefing-close" @click="showDevicesModal = false" title="关闭" style="background: none; border: none; color: var(--text-tertiary); cursor: pointer; padding: 4px; border-radius: 4px; display: flex; align-items: center; justify-content: center;">
            <X :size="14" />
          </button>
        </div>
        
        <div class="briefing-body" style="padding: 20px; display: flex; flex-direction: column; gap: 12px; max-height: 400px; overflow-y: auto;">
          <div v-for="dev in connectedDevices" :key="dev.device_id" style="display: flex; flex-direction: column; gap: 6px; background: var(--bg-tertiary); padding: 12px; border-radius: var(--radius-md); border: 1px solid var(--border-subtle);">
            <div style="display: flex; justify-content: space-between; align-items: center;">
              <div style="display: flex; align-items: center; gap: 6px;">
                <span class="status-dot" :class="isDeviceOnline(dev) ? 'dot-connected' : 'dot-disconnected'" style="width: 8px; height: 8px; border-radius: 50%;"></span>
                <span style="font-size: 13px; font-weight: 600; color: var(--text-primary);">{{ dev.platform === 'android' ? 'Android Device' : dev.platform }}</span>
                <span style="font-size: 11px; color: var(--text-tertiary); font-family: monospace;">({{ dev.device_id.substring(0, 8) }})</span>
              </div>
              <button class="btn btn-danger-outline btn-sm" style="padding: 4px 8px; font-size: 11px;" @click="handleDisconnectDevice(dev)" title="解绑此设备">
                <Unlink :size="11" /> 解绑
              </button>
            </div>
            <div style="font-size: 11px; color: var(--text-secondary); margin-left: 14px; display: flex; flex-direction: column; gap: 2px;">
              <div>IP 地址: {{ dev.ip_address }}</div>
              <div>最后活跃: {{ formatTime(dev.last_seen) }}</div>
            </div>
          </div>
          <div v-if="connectedDevices.length === 0" style="text-align: center; padding: 20px; color: var(--text-tertiary); font-size: 13px;">
            暂无已配对设备
          </div>
        </div>
      </div>
    </div>
  </Transition>

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
            <Loader2 class="animate-spin" :size="32" style="color: var(--text-tertiary)" />
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

  <!-- ── Pairing Progress Overlay ── -->
  <Transition name="modal-fade">
    <div v-if="showPairingProgress" class="pairing-overlay">
      <div class="pairing-progress-card">
        <div class="pairing-progress-header">
          <span class="pairing-progress-icon">🔗</span>
          <span>{{ pairingDone ? (pairingError ? '配对失败' : '配对成功!') : '正在配对...' }}</span>
        </div>
        <div class="pairing-steps">
          <div
            v-for="step in pairingSteps"
            :key="step.id"
            class="pairing-step"
            :class="'step-' + step.status"
          >
            <span class="step-indicator">
              <span v-if="step.status === 'done'" class="step-check">✅</span>
              <span v-else-if="step.status === 'error'" class="step-cross">❌</span>
              <span v-else-if="step.status === 'running'" class="step-spinner"></span>
              <span v-else-if="step.status === 'skipped'" class="step-skip">⏭️</span>
              <span v-else class="step-pending">○</span>
            </span>
            <div class="step-content">
              <span class="step-label">{{ step.label }}</span>
              <span v-if="step.detail" class="step-detail">{{ step.detail }}</span>
            </div>
          </div>
        </div>
        <div class="pairing-progress-footer">
          <button v-if="pairingDone" class="btn btn-primary-outline" @click="closePairingProgress">
            关闭
          </button>
          <button v-else class="btn btn-secondary-outline" @click="closePairingProgress">
            取消
          </button>
        </div>
      </div>
    </div>
  </Transition>
</template>

<script setup>
const getAssetUrl = (name) => `/logos/${name}`;
import { ref, onMounted, onUnmounted, computed, inject } from 'vue';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import QrcodeVue from 'qrcode.vue';
import { useI18n } from 'vue-i18n';
import {
  Smartphone, Unplug, X, Plus, Loader2, MessageSquare, Check,
  Building2, ExternalLink, Unlink, KeyRound, Network, Info, Copy,
  ShieldAlert, TriangleAlert, QrCode, Lock, Unlock, Scan
} from 'lucide-vue-next';

const { t } = useI18n();

const props = defineProps({
  config: { type: Object, required: true },
});
const emit = defineEmits(['config-changed']);

const isMobile = inject('isMobile', ref(false));

import { useDialog } from '@/composables/useDialog.js';

const { showConfirm, showAlert } = useDialog();

// ── Pairing Progress Overlay State ──
const showPairingProgress = ref(false);
const pairingDone = ref(false);
const pairingError = ref(false);

const pairingSteps = ref([]);

function initPairingSteps() {
  pairingSteps.value = [
    { id: 'parse',         label: '二维码解析',         status: 'pending', detail: '' },
    { id: 'save_config',   label: '保存配对配置',       status: 'pending', detail: '' },
    { id: 'relay_connect', label: '连接 Relay 服务器',  status: 'pending', detail: '' },
    { id: 'relay_notify',  label: 'Relay → 通知 PC',   status: 'pending', detail: '' },
    { id: 'relay_ack',     label: '等待 PC 回应',       status: 'pending', detail: '' },
    { id: 'lan_sync',      label: 'LAN 直连同步',       status: 'pending', detail: '' },
    { id: 'relay_sync',    label: 'Relay 隧道同步',     status: 'pending', detail: '' },
  ];
  pairingDone.value = false;
  pairingError.value = false;
}

function updateStep(id, status, detail) {
  const step = pairingSteps.value.find(s => s.id === id);
  if (step) {
    step.status = status;
    if (detail !== undefined) step.detail = detail;
  }
}

function closePairingProgress() {
  showPairingProgress.value = false;
  if (pairingDone.value && !pairingError.value) {
    fetchConnectedDevices().then(() => {
      if (isMobile.value && connectedDevices.value.length > 0) {
        isUnlocked.value = true;
      }
    });
  }
}

const handleMobileScan = async () => {
  if (window.appAPI?.scanQrCode) {
    document.body.classList.add('scanner-active');
    const code = await window.appAPI.scanQrCode();
    document.body.classList.remove('scanner-active');
    
    if (!code) return;

    let payload;
    try {
      payload = JSON.parse(code);
    } catch (e) {
      await showAlert("二维码内容无法解析: " + e);
      return;
    }

    const confirmed = await showConfirm(`发现设备 PC (ID: ${payload.device_id.substring(0, 8)}...)，是否连接并同步？`);
    if (!confirmed) return;

    // Show progress overlay
    initPairingSteps();
    showPairingProgress.value = true;
    updateStep('parse', 'done', '');

    // Listen for Rust-side progress events
    let unlistenProgress = null;
    try {
      unlistenProgress = await listen('sync:progress', (event) => {
        const { stage, status, detail } = event.payload;
        updateStep(stage, status, detail || '');
      });
    } catch (e) {
      console.warn('Could not listen to sync:progress events:', e);
    }

    try {
      // Step 2: Save config
      updateStep('save_config', 'running', '');
      await window.appAPI.setConfig('pairing_payload', payload);
      updateStep('save_config', 'done', '');

      // Step 3: Relay Handshake (3a/3b/3c are emitted from Rust side)
      if (window.appAPI.relayHandshake) {
        updateStep('relay_connect', 'running', '');
        try {
          const timeoutPromise = new Promise((_, reject) => setTimeout(() => reject(new Error('Relay Timeout')), 5000));
          await Promise.race([
            window.appAPI.relayHandshake(payload.device_id),
            timeoutPromise
          ]);
          // Rust events update 3a/3b/3c individually, but ensure all marked done on success
        } catch (e) {
          // Relay handshake failed. We should NOT return here. 
          // We must continue to try LAN sync!
          console.warn('Relay handshake failed, skipping to data sync...', e);
          updateStep('relay_connect', 'error', '连接超时或失败');
        }
      } else {
        updateStep('relay_connect', 'skipped', '无 Relay 握手接口');
        updateStep('relay_notify', 'skipped', '');
        updateStep('relay_ack', 'skipped', '');
      }

      // Step 4 & 5: Data Sync (lan_sync / relay_sync are emitted from Rust side)
      if (window.appAPI.triggerMobileSync) {
        updateStep('lan_sync', 'running', '');
        try {
          const syncTimeout = new Promise((_, reject) => setTimeout(() => reject(new Error('Sync Timeout')), 15000));
          await Promise.race([
            window.appAPI.triggerMobileSync(payload),
            syncTimeout
          ]);
          // If we got here without error, sync succeeded (LAN or Relay)
          // Check which one actually succeeded
          const lanStep = pairingSteps.value.find(s => s.id === 'lan_sync');
          const relayStep = pairingSteps.value.find(s => s.id === 'relay_sync');
          if (lanStep.status !== 'done' && relayStep.status !== 'done') {
            // Fallback: mark lan as done if no events came through (old PC)
            updateStep('lan_sync', 'done', '');
          }
          if (relayStep.status === 'pending') {
            updateStep('relay_sync', 'skipped', 'LAN 已成功，无需使用');
          }

          pairingDone.value = true;
          pairingError.value = false;
        } catch (e) {
          pairingDone.value = true;
          pairingError.value = true;
          // The specific failing step was already marked by Rust events
        }
      } else {
        updateStep('lan_sync', 'error', '无同步接口');
        pairingDone.value = true;
        pairingError.value = true;
      }
    } catch (e) {
      pairingDone.value = true;
      pairingError.value = true;
    } finally {
      if (unlistenProgress) unlistenProgress();
    }
  } else {
    await showAlert(t('setup.scanner_not_supported') || '当前环境不支持扫码');
  }
};

// ── Proxy Tunnel (Goal 20) ──
const proxyTunnelEnabled = ref(props.config.proxyTunnelEnabled || false);

async function toggleProxyTunnel() {
  proxyTunnelEnabled.value = !proxyTunnelEnabled.value;
  emit('config-changed', { proxyTunnelEnabled: proxyTunnelEnabled.value });
  // If we want to dynamically trigger Rust to reconnect, we could call an IPC here,
  // but for now relying on config-changed to save and backend to watch config is enough.
}

// ── P2P Sync (多端同步) ──
const isInitialized = ref(true); // Will fetch from backend
const isUnlocked = ref(false);
const showP2pModal = ref(false);
const showDevicesModal = ref(false);
const pinInput = ref('');
const pairingInfo = ref({
  device_id: '',
  local_ips: [],
  port: 8080,
  relay: ''
});
const pairingSuccessInfo = ref(null);

const qrPayload = computed(() => {
  if (!pairingInfo.value.device_id) return '';
  return JSON.stringify(pairingInfo.value);
});

const handlePinSubmit = async () => {
  if (pinInput.value.length < 4) return;
  try {
    if (isInitialized.value) {
      await invoke('unlock_device_keys', { pin: pinInput.value });
    } else {
      await invoke('init_device_keys', { pin: pinInput.value });
      isInitialized.value = true;
    }
    isUnlocked.value = true;
    pinInput.value = '';
    await fetchPairingInfo();
  } catch (error) {
    await showAlert(t('settings.p2p_alert_pin_err') + error);
  }
};

const handleReset = async () => {
  const confirmed = await showConfirm(t('settings.p2p_alert_reset'));
  if (confirmed) {
    try {
      await invoke('reset_device_keys');
      isInitialized.value = false;
      isUnlocked.value = false;
      pinInput.value = '';
      connectedDevices.value = [];
    } catch (error) {
      await showAlert(t('settings.p2p_alert_reset_err') + error);
    }
  }
};

const handleDisconnectDevice = async (dev) => {
  const confirmed = await showConfirm(`确定要解绑设备 ${dev.platform} (${dev.device_id.substring(0, 8)}) 吗？`);
  if (confirmed) {
    try {
      await invoke('disconnect_device', { deviceId: dev.device_id });
      await fetchConnectedDevices();
      if (connectedDevices.value.length === 0) {
        showDevicesModal.value = false;
        isUnlocked.value = false;
      }
    } catch (e) {
      console.error('Failed to disconnect device', e);
    }
  }
};

const fetchPairingInfo = async () => {
  try {
    pairingInfo.value = await invoke('get_pairing_payload');
  } catch (error) {
    console.error('获取配对信息失败', error);
  }
};

const connectedDevices = ref([]);
let unlistenDeviceConnected = null;

const fetchConnectedDevices = async () => {
  try {
    connectedDevices.value = await invoke('get_connected_devices');
  } catch (e) {
    console.error('Failed to get connected devices', e);
  }
};

const isDeviceOnline = (dev) => {
  // Consider online if seen within last 2 minutes (120000ms)
  return Date.now() - dev.last_seen < 120000;
};

const formatTime = (ts) => {
  const d = new Date(ts);
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
};

onMounted(async () => {
  // Existing init code if any
  await fetchConnectedDevices();
  if (isMobile.value) {
    // On mobile, bypass PIN lock and mark as unlocked if we have paired devices
    if (connectedDevices.value.length > 0) {
      isUnlocked.value = true;
    }
  } else {
    unlistenDeviceConnected = await listen('sync:device_connected', (event) => {
      fetchConnectedDevices();
      const dev = event.payload;
      if (dev && dev.platform) {
        pairingSuccessInfo.value = dev;
        setTimeout(() => {
          showP2pModal.value = false;
          pairingSuccessInfo.value = null; // reset for next time
        }, 3000);
      } else {
        showP2pModal.value = false;
      }
    });
  }
  await fetchPairingInfo();
});

onUnmounted(() => {
  if (unlistenDeviceConnected) {
    unlistenDeviceConnected();
  }
  // 确保在组件卸载时取消原生的二维码扫描（修复左滑返回卡死的 Bug）
  if (document.body.classList.contains('scanner-active')) {
    document.body.classList.remove('scanner-active');
    if (window.appAPI && window.appAPI.cancelQrCode) {
      window.appAPI.cancelQrCode();
    }
  }
});

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
      await window.appAPI.telegramSaveToken(tgToken.value);
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
      await window.appAPI.discordSaveToken(discordToken.value);
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
  if (!window.appAPI) return;
  qrCodeUrl.value = '';
  try {
    const res = await window.appAPI.wechatGetLoginQr();
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
    const res = await window.appAPI.wechatCheckLoginStatus(rawQrCode.value);
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
  if (!window.appAPI.connectorList) return;
  try {
    const list = await window.appAPI.connectorList();
    for (const c of list) {
      connectorStatuses.value[c.name] = c;
    }
  } catch (e) {
    console.warn('Failed to load connector statuses:', e);
  }
}

async function connectOAuth(name) {
  if (!window.appAPI.connectorStartOAuth) return;
  connectingService.value = name;
  try {
    const res = await window.appAPI.connectorStartOAuth(name);
    if (res && res.url) {
      // 使用默认浏览器打开 OAuth 授权页面
      window.appAPI.openExternal(res.url);
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
  if (!window.appAPI.connectorSaveCredentials) return;
  connectingService.value = 'lark';
  try {
    await window.appAPI.connectorSaveCredentials('lark', {
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
  if (!window.appAPI.connectorDisconnect) return;
  try {
    await window.appAPI.connectorDisconnect(name);
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
  if (!window.appAPI.getMcpConfig) return;
  const config = await window.appAPI.getMcpConfig();
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
  await window.appAPI.setMcpConfig({ mcpServers: updated });
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
      const res = await window.appAPI.connectorSaveCredentials('google', {
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
  await window.appAPI.setMcpConfig({ mcpServers: updated });
  mcpServers.value = updated;
}

// ── Init ──
onMounted(async () => {
  await loadMcpConfig();
  await loadConnectorStatuses();
  
  // P2P Key Initialization check
  try {
    isInitialized.value = await invoke('check_device_keys_initialized');
  } catch(e) {
    console.error('Failed to check key initialization', e);
  }
  if (window.appAPI.wechatGetCurrentStatus) {
    try {
      const res = await window.appAPI.wechatGetCurrentStatus();
      if (res && res.connected) {
        wechatConnected.value = true;
      }
    } catch(err) {}
  }
  
  if (window.appAPI.telegramGetToken) {
    try {
      const res = await window.appAPI.telegramGetToken();
      if (res && res.token) {
        tgToken.value = res.token;
      }
    } catch(err) {}
  }

  if (window.appAPI.discordGetToken) {
    try {
      const res = await window.appAPI.discordGetToken();
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

.field-hint {
  font-size: 12px;
  color: var(--text-tertiary);
  line-height: 1.4;
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
  border-color: var(--user-accent, var(--accent-primary));
  box-shadow: 0 0 0 1px var(--user-accent, var(--accent-primary));
}

/* ── Office service cards grid ── */
.service-cards-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
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
  border-color: var(--user-accent, var(--accent-primary));
  background: color-mix(in srgb, var(--user-accent, var(--accent-primary)) 5%, transparent);
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
  background-color: var(--text-inverse);
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
  justify-content: space-between;
  width: 100%;
  margin-bottom: 8px;
}

.device-indicator-btn {
  background: var(--bg-tertiary);
  border: 1px solid var(--border-subtle);
  color: var(--user-accent, var(--accent-primary));
  padding: 4px;
  border-radius: 50%;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.2s ease;
  width: 22px;
  height: 22px;
  box-shadow: var(--shadow-sm);
  flex-shrink: 0;
}

.device-indicator-btn:hover {
  background: var(--user-accent, var(--accent-primary));
  color: var(--bg-primary);
  border-color: var(--user-accent, var(--accent-primary));
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
  border-radius: var(--radius-md);
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

.static-card {
  cursor: default !important;
  pointer-events: auto !important;
}
.static-card:hover {
  transform: none !important;
  border-color: var(--border-subtle) !important;
  box-shadow: none !important;
}
/* ── Pairing Progress Overlay ── */
.pairing-overlay {
  position: fixed;
  top: 0; left: 0; right: 0; bottom: 0;
  background: rgba(0, 0, 0, 0.5);
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  backdrop-filter: blur(4px);
}

.pairing-progress-card {
  background: var(--bg-primary);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-lg);
  padding: 24px;
  width: 90%;
  max-width: 360px;
}

.pairing-progress-header {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 1.1rem;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 20px;
}

.pairing-progress-icon {
  font-size: 1.3rem;
}

.pairing-steps {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.pairing-step {
  display: flex;
  align-items: flex-start;
  gap: 10px;
}

.step-indicator {
  flex-shrink: 0;
  width: 22px;
  height: 22px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 14px;
}

.step-pending {
  color: var(--text-tertiary);
  font-size: 12px;
}

.step-check, .step-cross, .step-skip {
  font-size: 14px;
}

.step-spinner {
  width: 16px;
  height: 16px;
  border: 2px solid var(--border-subtle);
  border-top-color: var(--accent-primary, #3b82f6);
  border-radius: 50%;
  animation: pairing-spin 0.8s linear infinite;
}

@keyframes pairing-spin {
  to { transform: rotate(360deg); }
}

.step-content {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.step-label {
  font-size: 0.9rem;
  color: var(--text-primary);
  line-height: 22px;
}

.step-detail {
  font-size: 0.75rem;
  color: var(--text-tertiary);
  margin-top: 1px;
  word-break: break-all;
}

.step-done .step-label { color: var(--text-secondary); }
.step-error .step-label { color: var(--color-error, #ef4444); }
.step-error .step-detail { color: var(--color-error, #ef4444); opacity: 0.8; }
.step-skipped .step-label { color: var(--text-tertiary); text-decoration: line-through; opacity: 0.6; }

.pairing-progress-footer {
  margin-top: 20px;
  display: flex;
  justify-content: center;
}

.pairing-progress-footer .btn {
  min-width: 120px;
  justify-content: center;
}

</style>
