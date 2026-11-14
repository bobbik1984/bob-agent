<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { useI18n } from 'vue-i18n';
import {readingCache,connectionPrefix} from './reading-cache.mjs';
import { kernelBridge } from './native-bridge.mjs';
import { isBobNodeRuntime } from './http-bridge.mjs';
import PairingQr from './PairingQr.vue';
import ManualPairing from './ManualPairing.vue';
import QrScanModal from './components/QrScanModal.vue';
import { extractPairingCode } from './qr-image-decoder.mjs';
import {deviceGroups} from './device-paths.mjs';
import {checkPath,pathState,pathHealth} from './path-health.mjs';
import NodeConnect from './NodeConnect.vue';
const addNode = ref(false);
const webScannerOpen = ref(false);
import { createPairingScanController } from './pairing-scan-controller.mjs';

const { t } = useI18n();
const emit = defineEmits(['changed', 'connected']);
const browserClient = isBobNodeRuntime();
const props = defineProps({ paired: { type: Boolean, default: false } });
const paired = computed(() =>
  ['connected', 'enabled'].includes(status.value?.state),
);
const open = ref(true);
const capabilities = ref(null);
const status = ref(null);
const invitation = ref(null);
const busy = ref(false);
const error = ref('');
const syncSuccess = ref(false);
const connections = ref([]), connectionForm = ref(false), deleting = ref(null);
const connectionError = ref(''), connectionBusy = ref(false);
const devices = computed(()=>deviceGroups(connections.value));
const testing=ref({});
async function testConnection(node){testing.value={...testing.value,[node.id]:true};try{await checkPath(node,kernelBridge);}catch{}finally{testing.value={...testing.value,[node.id]:false};}}
function healthKey(node){return {readable:'connectionReadable',failed:'connectionReadFailed',checking:'connectionChecking',unverified:'connectionSavedUnverified'}[pathState(node)];}
const connectionDraft = ref({id:null,label:'',baseUrl:'',token:''});
async function loadConnections() {
  try { const snap=await kernelBridge.modelHubGet(); connections.value=snap?.remoteNodes || []; }
  catch { connectionError.value='connectionLoadFailed'; }
}
function editConnection(node=null,deviceId=null) {
  connectionError.value='';
  connectionDraft.value={id:node?.id||null,label:node?.name||node?.label||'',baseUrl:node?.url||node?.baseUrl||'',token:'',deviceId:deviceId||node?.deviceId||node?.id||''};
  connectionForm.value=true;
}
async function saveConnection() {
  if(connectionBusy.value)return;
  try { const url=new URL(connectionDraft.value.baseUrl); if(!['https:','http:'].includes(url.protocol)||url.username||url.password||url.search||url.hash)throw Error(); }
  catch {connectionError.value='connectionInvalidAddress';return;}
  connectionBusy.value=true;connectionError.value='';
  try {
    const snap=await kernelBridge.remoteNodeSave({...connectionDraft.value,token:connectionDraft.value.token||null,enabled:true});
    if(!snap)throw Error(); if(connectionDraft.value.id)await readingCache.removePrefix(connectionPrefix(connectionDraft.value.id)); connections.value=snap.remoteNodes||[];if(connectionDraft.value.id)delete pathHealth[connectionDraft.value.id];
    connectionDraft.value.token='';connectionForm.value=false;emit('changed');
  } catch {connectionError.value='connectionSaveFailed';}
  finally {connectionBusy.value=false;}
}
async function deleteConnection() {
  if(!deleting.value||connectionBusy.value)return;
  connectionBusy.value=true;
  try {const snap=await kernelBridge.remoteNodeDelete(deleting.value.id);if(!snap)throw Error();await readingCache.removePrefix(connectionPrefix(deleting.value.id));connections.value=snap.remoteNodes||[];delete pathHealth[deleting.value.id];deleting.value=null;emit('changed');}
  catch {connectionError.value='connectionDeleteFailed';}
  finally {connectionBusy.value=false;}
}
let poll = null;
let disposed = false;
let reading = false;
let lastStatusError = '';
const scanPhase = ref(null);

const scanner = createPairingScanController({
  bridge: kernelBridge,
  onPhase: p => {
    scanPhase.value = p;
  },
  onStatus: s => {
    status.value = s;
  },
  onConnected: completePairing,
});

const errorKeys = {
  SCAN_TIMEOUT: 'qrScanTimeout',
  SCAN_CANCELLED: 'qrScanCancelled',
  PAIRING_HOME_UNREACHABLE: 'qrHomeUnreachable',
  PAIRING_NETWORK_UNAVAILABLE: 'qrNetworkUnavailable',
  PAIRING_PROPOSAL_UNCONFIRMED: 'qrProposalUnknown',
  PAIRING_ALREADY_ACTIVE: 'qrAlreadyActive',
  SCAN_PERMISSION_DENIED: 'qrCameraDenied',
  SCAN_UNAVAILABLE: 'qrCameraUnavailable',
  SCAN_UNSUPPORTED: 'qrCameraUnsupported',
  SCAN_INVALID: 'qrInvalid',
  SCAN_BUSY: 'qrCameraBusy',
  VAULT_READ_FAILED: 'deviceStorageReadFailed',
  PAIRING_QR_RENDER_FAILED: 'qrRenderFailed',
  PAIRING_CODE_INVALID: 'qrInvalid',
  PAIRING_INVITATION_EXPIRED: 'pairExpired',
};

const keys = {
  unconfigured: 'pairUnconfigured',
  open: 'qrWaiting',
  pending: 'qrConfirmPhone',
  confirmed: 'pairConfirmed',
  consumed: 'pairConfirmed',
  connected: 'pairConnected',
  enabled: 'pairEnabled',
  expired: 'pairExpired',
  unknown: 'pairUnknown',
  proposing: 'pairProposing',
  stopped: 'pairStopped',
  revoked: 'qrRevoked',
};

async function completePairing() {
  if (poll) clearInterval(poll);
  poll = null;
  await loadConnections();
  for (const node of connections.value) {
    if (node?.enabled !== false && node?.id) {
      void testConnection(node);
    }
  }
  emit('changed');
  emit('connected');
}

async function refresh() {
  if (!open.value || busy.value || reading || disposed) return;
  reading = true;
  try {
    const hadInvitation = Boolean(invitation.value);
    status.value = await kernelBridge.pairingStatus();
    if (error.value === lastStatusError) error.value = '';
    lastStatusError = '';
    if (
      (capabilities.value?.can_scan || status.value?.role === 'replica') &&
      ['pending', 'unknown'].includes(status.value?.state)
    ) {
      try {
        status.value = await kernelBridge.pairingFinish();
        if (status.value?.state === 'connected') {
          scanPhase.value = 'connected';
          error.value = '';
          completePairing();
        }
      } catch (e) {
        if (String(e?.message || e) !== 'PAIRING_CONFIRMATION_UNAVAILABLE') throw e;
      }
    }
    if (
      ['pending', 'confirmed', 'connected', 'enabled', 'expired'].includes(
        status.value?.state,
      )
    ) {
      invitation.value = null;
      if (hadInvitation && status.value?.state === 'connected') {
        await loadConnections();
      }
    }
    emit('changed');
  } catch (e) {
    const code = String(e?.message || e);
    error.value = /^[A-Z][A-Z0-9_]{2,90}$/.test(code)
      ? code
      : 'PAIRING_STATUS_UNAVAILABLE';
    lastStatusError = error.value;
  } finally {
    reading = false;
  }
}

async function ensureOpen() {
  open.value = true;
  if (poll) clearInterval(poll);
  poll = null;
  try {
    capabilities.value = await kernelBridge.pairingCapabilities();
    await refresh();
    if (
      !browserClient &&
      capabilities.value?.can_host &&
      ['confirmed', 'consumed', 'connected'].includes(status.value?.state)
    ) {
      await act(() => kernelBridge.pairingEnableHome());
    }
    poll = setInterval(refresh, 2000);
  } catch {
    error.value = 'PAIRING_CAPABILITIES_UNAVAILABLE';
  }
}

async function act(work) {
  if (busy.value) return;
  busy.value = true;
  error.value = '';
  try {
    const result = await work();
    status.value = result;
    if (result?.code) invitation.value = result;
    emit('changed');
  } catch (e) {
    const code = String(e?.message || e);
    error.value = /^[A-Z][A-Z0-9_]{2,90}$/.test(code) ? code : 'PAIRING_FAILED';
  } finally {
    busy.value = false;
  }
}

defineExpose({ openDetails: ensureOpen });

async function createQr() {
  await act(async () => {
    if (status.value?.state === 'expired') await kernelBridge.pairingReset();
    if (
      browserClient &&
      typeof window !== 'undefined' &&
      typeof window.location?.origin === 'string' &&
      window.location.origin.startsWith('http')
    ) {
      return kernelBridge.pairingQrCreate({ origin: window.location.origin });
    }
    return kernelBridge.pairingQrCreate();
  });
}

async function scan() {
  await act(() => scanner.scan());
}

async function handleQrDetected(rawCode) {
  const uri = extractPairingCode(rawCode) || String(rawCode || '').trim();
  if (!uri) {
    error.value = 'PAIRING_CODE_INVALID';
    return;
  }
  webScannerOpen.value = false;
  await act(async () => {
    if (uri.startsWith('bob://pair?')) {
      await kernelBridge.nodeConnect({ qrCode: uri });
      await loadConnections();
      emit('connected');
      return { ...(status.value || {}), state: 'connected' };
    }
    const proposed = await kernelBridge.pairingQrPropose(uri);
    status.value = proposed;
    const finished = await kernelBridge.pairingFinish();
    if (finished?.state === 'connected') {
      scanPhase.value = 'connected';
      await completePairing();
    }
    return finished;
  });
}

async function reset() {
  await act(() => kernelBridge.pairingReset());
  invitation.value = null;
}

async function syncNow() {
  if (busy.value) return;
  busy.value = true;
  error.value = '';
  syncSuccess.value = false;
  try {
    await kernelBridge.syncHome();
    syncSuccess.value = true;
    emit('changed');
  } catch (e) {
    const code = String(e?.message || e);
    error.value = /^[A-Z][A-Z0-9_]{2,90}$/.test(code) ? code : 'PAIRING_FAILED';
  } finally {
    busy.value = false;
  }
}

onMounted(async () => {
  await loadConnections();
  try {
    const caps = await kernelBridge.pairingCapabilities();
    capabilities.value = caps;
    await ensureOpen();
    if (caps.start_pairing && caps.can_host) {
      if (
        !status.value ||
        ['unconfigured', 'revoked', 'stopped'].includes(status.value.state)
      ) {
        await createQr();
      }
    }
  } catch {
    /* Opening chat remains usable when setup is unavailable. */
  }
});

onUnmounted(() => {
  disposed = true;
  if (poll) clearInterval(poll);
  invitation.value = null;
});
</script>

<template>
  <div class="home-connection">
    <section class="hub-card" :aria-label="t('connectionListTitle')">
      <div class="hub-card-header"><h3>{{ t('connectionListTitle') }}</h3><button type="button" @click="addNode=true">{{ t('connectionAdd') }}</button></div>
      <NodeConnect v-if="addNode" @close="addNode=false" @connected="loadConnections();emit('changed')" />
      <p v-if="connectionError" role="alert">{{ t(connectionError) }}</p>
      <p v-if="!connections.length && !connectionForm" class="view-card-desc">{{ t('connectionEmpty') }}</p>
      <article v-for="device in devices" :key="device.id" class="connection-device">
        <h4>{{ device.label }}</h4>
        <button type="button" @click="editConnection(null,device.id)">{{ t('connectionAddPath') }}</button>
        <div v-for="node in device.paths" :key="node.id" class="connection-list-row">
          <div><p>{{ node.url || node.baseUrl }}</p><small>{{ t(healthKey(node)) }}</small></div>
          <div><button type="button" :disabled="testing[node.id]" @click="testConnection(node)">{{t('connectionTest')}}</button><button type="button" @click="editConnection(node)">{{ t('connectionEdit') }}</button><button type="button" @click="deleting=node">{{ t('connectionDeletePath') }}</button></div>
        </div>
      </article>
      <form v-if="connectionForm" class="connection-form" @submit.prevent="saveConnection">
        <label>{{ t('connectionName') }}<input v-model="connectionDraft.label" :disabled="connectionBusy" /></label>
        <label>{{ t('connectionDevice') }}<select v-model="connectionDraft.deviceId" :disabled="connectionBusy"><option :value="connectionDraft.id || ''">{{t('connectionSeparateDevice')}}</option><option v-for="device in devices" :key="device.id" :value="device.id">{{device.label}} · {{device.paths[0].baseUrl || device.paths[0].url}}</option></select></label>
        <p>{{t('connectionDeviceHint')}}</p>
        <label>{{ t('connectionAddress') }}<input v-model="connectionDraft.baseUrl" type="url" required :disabled="connectionBusy" /></label>
        <label>{{ t('connectionToken') }}<input v-model="connectionDraft.token" type="password" autocomplete="off" :disabled="connectionBusy" /></label>
        <p class="view-card-desc">{{ t('connectionTokenHint') }}</p>
        <div><button type="submit" :disabled="connectionBusy">{{ t('connectionSave') }}</button><button type="button" :disabled="connectionBusy" @click="connectionForm=false;connectionDraft.token=''">{{ t('scheduleClose') }}</button></div>
      </form>
      <div v-if="deleting" role="alertdialog" :aria-label="t('connectionDelete')" class="connection-delete-confirm">
        <p>{{ t('connectionDeleteConfirm', {name:deleting.name || deleting.label || deleting.url || deleting.baseUrl}) }}</p>
        <button type="button" :disabled="connectionBusy" @click="deleteConnection">{{ t('connectionDelete') }}</button><button type="button" :disabled="connectionBusy" @click="deleting=null">{{ t('scheduleClose') }}</button>
      </div>
      <p class="view-card-desc">{{ t('connectionProtocolBoundary') }}</p>
    </section>
    <details v-if="capabilities?.can_host || capabilities?.can_scan" :open="browserClient" class="connection-pairing-details"><summary>{{ t('connectionPairDevice') }}</summary>
    <section
      v-if="open"
      class="qr-panel hub-card"
      :class="{ 'qr-display': !!invitation }"
      :aria-label="t('homeConnection')"
    >
      <div class="hub-card-header">
        <h3>{{ t('connectComputerTitle') }}</h3>
        <span v-if="paired" class="device-status-pill">{{ t('connectionSaved') }}</span>
      </div>
      <p v-if="browserClient" class="view-card-desc">{{ t('connectComputerWebHint') }}</p>
      <button
        v-if="invitation"
        type="button"
        class="qr-close"
        :aria-label="t('qrClose')"
        @click="invitation = null"
      >
        ×
      </button>

      <template v-if="capabilities?.can_host">
        <p class="view-card-desc">{{ t('qrHomeHint') }}</p>
        <div class="provider-actions">
          <button
            v-if="
              !invitation &&
              (browserClient || !['pending', 'confirmed', 'connected', 'enabled'].includes(status?.state))
            "
            type="button"
            class="primary-action-btn"
            :disabled="busy"
            @click="createQr"
          >
            {{ t('qrShow') }}
          </button>
          <button
            v-if="!invitation"
            type="button"
            class="secondary-action-btn"
            :disabled="busy"
            @click="capabilities?.can_scan ? scan() : (webScannerOpen = !webScannerOpen)"
          >
            {{ t('qrScanOtherDevice') }}
          </button>
        </div>
        <PairingQr
          v-if="invitation"
          :code="invitation.code"
          :expires-at="invitation.expires_at"
          @expired="invitation = null"
          @error="error = 'PAIRING_QR_RENDER_FAILED'"
        />
        <QrScanModal
          v-if="webScannerOpen"
          @close="webScannerOpen = false"
          @detected="handleQrDetected"
        />
        <p
          v-if="['confirmed', 'consumed', 'connected', 'enabled'].includes(status?.state)"
          class="view-card-desc"
        >
          {{ t('qrPairedPolicy') }}
        </p>
      </template>

      <template v-else-if="capabilities?.can_scan">
        <p class="view-card-desc">{{ t('qrPhoneHint') }}</p>
        <div class="provider-actions">
          <button
            type="button"
            class="primary-action-btn"
            :disabled="busy"
            @click="scan"
          >
            {{ t('qrScan') }}
          </button>
        </div>
      </template>

      <p v-if="scanPhase && scanPhase !== 'failed'" role="status" aria-live="polite" class="view-card-desc">
        {{
          t(
            {
              scanning: 'qrScanning',
              connecting: 'qrDecodedConnecting',
              verifying: 'qrVerifying',
              connected: 'pairConnected',
            }[scanPhase],
          )
        }}
      </p>
      <p v-else-if="status && scanPhase !== 'failed'" role="status" class="view-card-desc">
        {{ t(keys[status.state] || 'pairUnconfirmed') }}
      </p>
      <p v-if="error || status?.error" role="alert" class="view-card-desc">
        {{ t(errorKeys[error || status.error] || 'pairFailed') }}
        <code v-if="!errorKeys[error || status.error]"> {{ error || status.error }}</code>
      </p>

      <p v-if="paired" class="view-card-desc">
        {{ t('peerSyncCapabilitiesDesc') }}
      </p>
      <p v-if="syncSuccess" role="status" class="view-card-desc">
        {{ t('peerSyncSuccessHint') }}
      </p>

      <details v-if="!browserClient && paired" class="about-footer-details">
        <summary>{{ t('connectionManage') }}</summary>
      <div class="provider-actions">
        <button
          v-if="paired"
          type="button"
          class="primary-action-btn"
          :disabled="busy"
          @click="syncNow"
        >
          {{ t('peerSyncNowBtn') }}
        </button>
        <button
          v-if="status && !['unconfigured', 'connected', 'enabled'].includes(status.state)"
          type="button"
          class="mini-ghost-btn"
          :disabled="busy"
          @click="reset"
        >
          {{ t('pairReset') }}
        </button>
        <button
          v-if="
            capabilities?.can_host &&
            ['confirmed', 'consumed', 'connected', 'enabled'].includes(status?.state)
          "
          type="button"
          class="mini-ghost-btn"
          :disabled="busy"
          @click="act(() => kernelBridge.pairingRevoke())"
        >
          {{ t('qrRevoke') }}
        </button>
      </div>

      </details>

      <details v-if="!browserClient && !invitation && (capabilities?.can_host || capabilities?.can_scan)" class="about-footer-details">
        <summary class="about-footer-summary">{{ t('qrAdvanced') }}</summary>
        <ManualPairing @changed="emit('changed')" />
      </details>
    </section>
    </details>
  </div>
</template>

<style scoped>
.home-connection {
  display: grid;
  gap: 0.75rem;
  min-width: 0;
}
.connection-pairing-details {
  min-width: 0;
}
.connection-pairing-details > summary {
  cursor: pointer;
  color: var(--text-secondary);
  font-size: var(--text-xs);
}
.connection-pairing-details > .qr-panel:not(.qr-display) {
  margin-top: 0.55rem;
}

.qr-display {
  position: fixed;
  inset: max(0.75rem, env(safe-area-inset-top)) max(0.75rem, env(safe-area-inset-right))
    max(0.75rem, env(safe-area-inset-bottom)) max(0.75rem, env(safe-area-inset-left));
  z-index: 20;
  margin: 0;
  max-height: none;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0.6rem;
  overflow: auto;
  text-align: center;
  box-shadow: 0 0 0 2rem var(--bg-root);
}
.qr-close {
  position: absolute;
  right: 0.75rem;
  top: 0.75rem;
  font-size: 1.2rem;
  padding: 0.2rem 0.5rem;
}
</style>
