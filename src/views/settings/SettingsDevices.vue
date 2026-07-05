<template>
  <div class="settings-devices-panel">
    <!-- 未初始化 / 锁屏状态 -->
    <section v-if="!isUnlocked" class="settings-section card">
      <div style="display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 40px 20px; text-align: center;">
        <ShieldAlert :size="48" style="color: var(--text-tertiary); margin-bottom: 20px;" />
        <h3 style="margin-bottom: 12px;">安全设备认证</h3>
        <p style="color: var(--text-secondary); max-width: 400px; margin-bottom: 24px; font-size: 0.9em; line-height: 1.5;">
          {{ isInitialized ? '请输入本地 PIN 码解锁设备身份。' : '首次使用多端同步，请设置一个 4~6 位纯数字 PIN 码以保护您的设备私钥。' }}
        </p>
        
        <div style="display: flex; gap: 12px; margin-bottom: 24px;">
          <input 
            v-model="pinInput" 
            type="password" 
            class="input" 
            maxlength="6"
            :placeholder="isInitialized ? '输入 PIN 解锁' : '设置新 PIN 码 (4-6位)'"
            style="width: 200px; text-align: center; font-size: 1.1em; letter-spacing: 4px;"
            @keyup.enter="handlePinSubmit"
          />
        </div>
        
        <button class="btn btn-primary" :disabled="pinInput.length < 4" @click="handlePinSubmit">
          {{ isInitialized ? '解 锁' : '生成设备密钥' }}
        </button>

        <div v-if="isInitialized" style="margin-top: 32px; padding-top: 24px; border-top: 1px solid var(--border-subtle); width: 100%; max-width: 300px;">
          <button class="btn btn-ghost" style="color: var(--color-error); font-size: 0.85em; width: 100%;" @click="handleReset">
            忘记 PIN 码？强制重置设备身份
          </button>
        </div>
      </div>
    </section>

    <!-- 已解锁状态 -->
    <template v-else>
      <section class="settings-section card">
        <h3 class="section-title">
          <Smartphone :size="16" class="section-icon" />
          扫码配对手机
        </h3>
        <p style="color: var(--text-secondary); font-size: 0.9em; margin-bottom: 20px;">
          使用手机端 Bob 扫描下方二维码，建立端到端加密的 P2P 同步连接。
        </p>
        
        <div style="display: flex; gap: 40px; align-items: stretch;">
          <!-- 左侧设备信息 -->
          <div style="flex: 1; display: flex; flex-direction: column; gap: 16px;">
            <div class="form-group">
              <label class="form-label">PC 设备 ID (公钥)</label>
              <div style="display: flex; gap: 8px;">
                <input type="text" readonly class="input" :value="pairingInfo.device_id || '正在加载...'" style="flex: 1; font-family: monospace; font-size: 0.85em; color: var(--text-tertiary);" />
              </div>
            </div>
            
            <div class="form-group">
              <label class="form-label">局域网地址</label>
              <div style="display: flex; flex-wrap: wrap; gap: 8px;">
                <span v-for="ip in pairingInfo.local_ips" :key="ip" class="tag" style="background: var(--bg-tertiary); padding: 4px 10px; border-radius: 4px; font-size: 0.85em;">
                  {{ ip }}
                </span>
                <span v-if="!pairingInfo.local_ips || pairingInfo.local_ips.length === 0" style="color: var(--text-tertiary); font-size: 0.85em;">未检测到网络</span>
              </div>
            </div>
            
            <div class="form-group">
              <label class="form-label">中继服务器 (Relay)</label>
              <div style="display: flex; align-items: center; gap: 8px;">
                <span class="status-dot" style="background: var(--color-success); width: 8px; height: 8px; border-radius: 50%; display: inline-block;"></span>
                <span style="font-size: 0.85em;">{{ pairingInfo.relay || 'wss://relay.bobbik.org' }}</span>
              </div>
            </div>
          </div>
          
          <!-- 右侧二维码 -->
          <div style="width: 200px; display: flex; flex-direction: column; align-items: center; justify-content: center; background: white; padding: 20px; border-radius: var(--radius-md);">
            <qrcode-vue v-if="qrPayload" :value="qrPayload" :size="160" level="M" />
            <div v-else style="width: 160px; height: 160px; display: flex; align-items: center; justify-content: center; background: #f0f0f0; border-radius: 8px;">
              <span style="color: #999; font-size: 0.8em;">正在生成...</span>
            </div>
          </div>
        </div>
      </section>

      <!-- 危险操作区 -->
      <section class="settings-section card" style="margin-top: 24px; border: 1px solid rgba(255, 77, 79, 0.3);">
        <h3 class="section-title" style="color: var(--color-error);">
          <TriangleAlert :size="16" class="section-icon" style="color: inherit;" />
          危险区域
        </h3>
        <div style="display: flex; justify-content: space-between; align-items: center;">
          <div style="color: var(--text-secondary); font-size: 0.9em; max-width: 60%;">
            强制销毁当前密钥并生成新设备身份。旧设备将永远无法再次连接本 PC，必须重新扫码配对。
          </div>
          <button class="btn" style="border-color: var(--color-error); color: var(--color-error);" @click="handleReset">
            销毁并重置
          </button>
        </div>
      </section>
    </template>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { ShieldAlert, Smartphone, TriangleAlert } from 'lucide-vue-next';
import QrcodeVue from 'qrcode.vue';

const isInitialized = ref(true); // Will fetch from backend
const isUnlocked = ref(false);
const pinInput = ref('');
const pairingInfo = ref({
  device_id: '',
  local_ips: [],
  port: 8080,
  relay: ''
});

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
    alert('PIN 错误或操作失败: ' + error);
  }
};

const handleReset = async () => {
  if (confirm('警告：此操作不可逆！旧手机将全部失效，需重新扫码配对。确认重置？')) {
    try {
      await invoke('reset_device_keys');
      isInitialized.value = false;
      isUnlocked.value = false;
      pinInput.value = '';
    } catch (error) {
      alert('重置失败: ' + error);
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

onMounted(async () => {
  try {
    isInitialized.value = await invoke('check_device_keys_initialized');
  } catch(e) {
    console.error('Failed to check key initialization', e);
  }
});
</script>

<style scoped>
/* Base styling for SettingsDevices */
</style>
