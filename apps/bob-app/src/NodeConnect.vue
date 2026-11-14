<script setup>
import { onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { kernelBridge } from './native-bridge.mjs';
import PairingQr from './PairingQr.vue';
import QrScanModal from './components/QrScanModal.vue';
import { extractPairingCode } from './qr-image-decoder.mjs';
import { nodeAddress } from './node-connection.mjs';

const { t } = useI18n();
const emit = defineEmits(['connected', 'close']);

const mode = ref('address');
const address = ref('');
const name = ref('');
const password = ref('');
const pairingCode = ref('');
const tokenInput = ref('');
const authChoice = ref('owner_password');
const info = ref(null);
const capabilities = ref(null);
const busy = ref(false);
const error = ref('');
const qr = ref(null);
const connected = ref(false);

onMounted(async () => {
  capabilities.value = await kernelBridge.pairingCapabilities().catch(() => null);
});

function clearSecrets() {
  password.value = '';
  pairingCode.value = '';
  tokenInput.value = '';
}

function mapErrorKey(err, fallback) {
  const code = String(err?.message || err || '');
  if (code === 'NODE_RATE_LIMITED') return 'nodeConnectRate';
  if (
    [
      'NODE_UNREACHABLE',
      'PAIRING_HOME_UNREACHABLE',
      'PAIRING_NETWORK_UNAVAILABLE',
      'NODE_ADDRESS_INVALID',
      'NODE_HTTPS_REQUIRED',
    ].includes(code)
  ) {
    return 'nodeConnectUnavailable';
  }
  if (code === 'PAIRING_INVITATION_EXPIRED') return 'pairExpired';
  if (['PAIRING_CODE_INVALID', 'SCAN_INVALID'].includes(code)) return 'qrInvalid';
  return fallback;
}

async function discover() {
  if (busy.value) return;
  busy.value = true;
  error.value = '';
  try {
    const cleanAddr = nodeAddress(address.value.trim());
    const found = await kernelBridge.nodeDiscover(cleanAddr);
    if (!found || found.protocol !== 'bob-node-v1') throw Error('NODE_UNREACHABLE');
    info.value = found;
    authChoice.value = found.authMode === 'pairing_code' ? 'pairing_code' : 'owner_password';
  } catch (err) {
    error.value = mapErrorKey(err, 'nodeConnectUnavailable');
  } finally {
    busy.value = false;
  }
}

async function connect() {
  if (busy.value || !info.value) return;
  busy.value = true;
  error.value = '';
  try {
    const res = await kernelBridge.nodeConnect({
      address: info.value.baseUrl || nodeAddress(address.value.trim()),
      label: name.value.trim() || undefined,
      password: authChoice.value === 'owner_password' ? password.value : undefined,
      pairingCode: authChoice.value === 'pairing_code' ? pairingCode.value.trim() : undefined,
      token: authChoice.value === 'token' ? tokenInput.value.trim() : undefined,
    });
    if (!res?.connected) throw Error('NODE_CONNECT_FAILED');
    clearSecrets();
    connected.value = true;
    qr.value = res.mobileQr || null;
    emit('connected');
  } catch (err) {
    error.value = mapErrorKey(err, 'nodeConnectFailed');
  } finally {
    clearSecrets();
    busy.value = false;
  }
}

async function connectByQr(rawCode) {
  if (busy.value) return;
  const code = extractPairingCode(rawCode) || String(rawCode || '').trim();
  if (!code) {
    error.value = 'qrInvalid';
    return;
  }
  busy.value = true;
  error.value = '';
  try {
    const res = await kernelBridge.nodeConnect({
      qrCode: code,
      label: name.value.trim() || undefined,
    });
    if (!res?.connected) throw Error('NODE_CONNECT_FAILED');
    connected.value = true;
    qr.value = res.mobileQr || null;
    emit('connected');
  } catch (err) {
    error.value = mapErrorKey(err, 'nodeConnectFailed');
  } finally {
    busy.value = false;
  }
}

async function scanNativeQr() {
  if (busy.value) return;
  error.value = '';
  busy.value = true;
  let scannedCode = '';
  try {
    const res = await kernelBridge.scanPairingCode();
    scannedCode = res?.code || res || '';
  } catch (err) {
    error.value = mapErrorKey(err, 'qrInvalid');
    busy.value = false;
    return;
  }
  busy.value = false;
  if (scannedCode) await connectByQr(scannedCode);
}

function resetDiscovery() {
  info.value = null;
  connected.value = false;
  qr.value = null;
  error.value = '';
  clearSecrets();
}
</script>

<template>
  <section class="node-connect">
    <div v-if="!connected" class="node-connect-tabs" role="tablist">
      <button
        type="button"
        role="tab"
        class="node-connect-tab"
        :class="{ active: mode === 'address' }"
        :aria-selected="mode === 'address'"
        @click="mode = 'address'; error = ''"
      >
        {{ t('nodeConnectTabAddress') }}
      </button>
      <button
        type="button"
        role="tab"
        class="node-connect-tab"
        :class="{ active: mode === 'qr' }"
        :aria-selected="mode === 'qr'"
        @click="mode = 'qr'; error = ''"
      >
        {{ t('nodeConnectTabQr') }}
      </button>
    </div>

    <template v-if="connected">
      <p role="status">{{ t('nodeConnectSaved') }}</p>
      <PairingQr v-if="qr" :code="qr.code" :expires-at="qr.expires_at" @expired="qr = null" />
      <p class="view-card-desc">{{ t(qr ? 'nodeConnectScan' : 'nodeConnectDoneHint') }}</p>
    </template>

    <template v-else-if="mode === 'address'">
      <form v-if="!info" @submit.prevent="discover">
        <label>
          <span>{{ t('connectionAddress') }}</span>
          <input
            v-model="address"
            required
            autocomplete="url"
            :placeholder="t('nodeConnectAddressPlaceholder')"
            :disabled="busy"
          />
        </label>
        <p class="view-card-desc">{{ t('nodeConnectAddressHint') }}</p>
        <button type="submit" :disabled="busy">
          {{ t(busy ? 'nodeConnectChecking' : 'nodeConnectCheck') }}
        </button>
      </form>

      <form v-else @submit.prevent="connect">
        <div class="node-connect-found">
          <span>{{ info.baseUrl }}</span>
          <button type="button" class="node-connect-link" :disabled="busy" @click="resetDiscovery">
            {{ t('nodeConnectChangeAddress') }}
          </button>
        </div>
        <label>
          <span>{{ t('connectionName') }}</span>
          <input v-model="name" :placeholder="info.suggestedLabel || ''" :disabled="busy" />
        </label>

        <div class="node-connect-auth-switch">
          <button
            type="button"
            class="node-connect-pill"
            :class="{ active: authChoice === 'owner_password' }"
            @click="authChoice = 'owner_password'"
          >
            {{ t('nodeConnectAuthPassword') }}
          </button>
          <button
            type="button"
            class="node-connect-pill"
            :class="{ active: authChoice === 'pairing_code' }"
            @click="authChoice = 'pairing_code'"
          >
            {{ t('nodeConnectAuthPairingCode') }}
          </button>
          <button
            type="button"
            class="node-connect-pill"
            :class="{ active: authChoice === 'token' }"
            @click="authChoice = 'token'"
          >
            {{ t('nodeConnectAuthToken') }}
          </button>
        </div>

        <label v-if="authChoice === 'owner_password'">
          <span>{{ t('nodeConnectPassword') }}</span>
          <input v-model="password" type="password" autocomplete="current-password" required :disabled="busy" />
        </label>
        <label v-else-if="authChoice === 'pairing_code'">
          <span>{{ t('nodeConnectPairingCodeLabel') }}</span>
          <input v-model="pairingCode" required autocomplete="off" :disabled="busy" />
        </label>
        <label v-else>
          <span>{{ t('connectionToken') }}</span>
          <input v-model="tokenInput" type="password" required autocomplete="off" :disabled="busy" />
        </label>

        <p class="view-card-desc">{{ t('nodeConnectHint') }}</p>
        <button type="submit" :disabled="busy">
          {{ t(busy ? 'nodeConnectChecking' : 'nodeConnectConfirm') }}
        </button>
      </form>
    </template>

    <template v-else>
      <div class="node-connect-qr-pane">
        <label>
          <span>{{ t('connectionName') }}</span>
          <input v-model="name" :placeholder="t('nodeConnectOptionalNamePlaceholder')" :disabled="busy" />
        </label>
        <button
          v-if="capabilities?.can_scan"
          type="button"
          class="node-connect-native-scan"
          :disabled="busy"
          @click="scanNativeQr"
        >
          {{ t('qrScanNativeBtn') }}
        </button>
        <QrScanModal :inline="true" @detected="connectByQr" />
      </div>
    </template>

    <p v-if="error" role="alert" class="node-connect-error">{{ t(error) }}</p>
    <button type="button" :disabled="busy" @click="resetDiscovery(); emit('close')">
      {{ t('scheduleClose') }}
    </button>
  </section>
</template>

<style scoped>
.node-connect {
  padding: 1rem;
  border: 1px solid var(--border-color);
  border-radius: 0.5rem;
  background: var(--bg-primary);
  display: grid;
  gap: 0.75rem;
}
.node-connect-tabs {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 0.375rem;
}
.node-connect-tab {
  padding: 0.5rem 0.75rem;
  border: 1px solid var(--border-color);
  border-radius: 0.375rem;
  background: var(--bg-secondary);
  color: var(--text-secondary);
  font-size: 0.8125rem;
  font-weight: 600;
  cursor: pointer;
}
.node-connect-tab.active {
  border-color: var(--accent-primary);
  background: var(--accent-primary);
  color: var(--text-on-accent);
}
form,
.node-connect-qr-pane {
  display: grid;
  gap: 0.75rem;
}
label {
  display: grid;
  gap: 0.3rem;
}
input {
  min-width: 0;
  width: 100%;
  box-sizing: border-box;
}
.node-connect-found {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  font-size: 0.8125rem;
  color: var(--text-secondary);
}
.node-connect-link {
  border: 0;
  padding: 0;
  background: none;
  color: var(--accent-primary);
  font-size: 0.8125rem;
  cursor: pointer;
}
.node-connect-auth-switch {
  display: flex;
  flex-wrap: wrap;
  gap: 0.375rem;
}
.node-connect-pill {
  padding: 0.25rem 0.625rem;
  border: 1px solid var(--border-color);
  border-radius: 9999px;
  background: var(--bg-secondary);
  color: var(--text-secondary);
  font-size: 0.75rem;
  cursor: pointer;
}
.node-connect-pill.active {
  border-color: var(--accent-primary);
  color: var(--accent-primary);
}
.node-connect-native-scan {
  padding: 0.55rem 0.85rem;
  border: 1px solid var(--accent-primary);
  border-radius: 0.5rem;
  background: var(--accent-primary);
  color: var(--text-on-accent);
  font-weight: 600;
  cursor: pointer;
}
.node-connect-error {
  margin: 0;
  color: var(--color-error);
  font-size: 0.8125rem;
}
</style>
