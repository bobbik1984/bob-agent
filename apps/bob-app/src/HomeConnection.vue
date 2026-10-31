<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { kernelBridge } from './native-bridge.mjs';
import PairingQr from './PairingQr.vue';
import ManualPairing from './ManualPairing.vue';
import { createPairingScanController } from './pairing-scan-controller.mjs';

const { t } = useI18n();
const emit = defineEmits(['changed', 'connected']);
const props = defineProps({ paired: { type: Boolean, default: false } });
const paired = computed(
  () => props.paired || ['connected', 'enabled'].includes(status.value?.state),
);
const open = ref(true);
const capabilities = ref(null);
const status = ref(null);
const invitation = ref(null);
const busy = ref(false);
const error = ref('');
const syncSuccess = ref(false);
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

function completePairing() {
  if (!capabilities.value?.can_scan) return;
  if (poll) clearInterval(poll);
  poll = null;
  emit('connected');
}

async function refresh() {
  if (!open.value || busy.value || reading || disposed) return;
  reading = true;
  try {
    status.value = await kernelBridge.pairingStatus();
    if (error.value === lastStatusError) error.value = '';
    lastStatusError = '';
    if (
      capabilities.value?.can_scan &&
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
    return kernelBridge.pairingQrCreate();
  });
}

async function scan() {
  await act(() => scanner.scan());
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
  try {
    const caps = await kernelBridge.pairingCapabilities();
    capabilities.value = caps;
    if (caps.start_pairing && caps.can_host) {
      await ensureOpen();
      if (
        !status.value ||
        ['unconfigured', 'revoked', 'stopped'].includes(status.value.state)
      ) {
        await act(() => kernelBridge.pairingQrCreate());
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
    <section
      v-if="open"
      class="qr-panel hub-card"
      :class="{ 'qr-display': !!invitation }"
      :aria-label="t('homeConnection')"
    >
      <div class="hub-card-header">
        <div class="hub-card-title-group">
          <svg class="hub-icon" viewBox="0 0 24 24" aria-hidden="true">
            <path d="M12 22v-5M9 8V2M15 8V2M18 8v5a6 6 0 0 1-12 0V8Z" />
          </svg>
          <h3>
            {{
              t(
                capabilities?.can_host
                  ? 'qrConnectPhone'
                  : capabilities?.can_scan
                    ? 'qrConnectHome'
                    : 'deviceAndConnection',
              )
            }}
          </h3>
        </div>
        <span class="device-status-pill" :class="{ online: paired }">
          {{
            t(
              capabilities?.can_host
                ? paired
                  ? 'qrPhonePaired'
                  : 'pairUnconfigured'
                : paired
                  ? 'qrHomePaired'
                  : 'pairUnconfigured',
            )
          }}
        </span>
      </div>

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
              !['pending', 'confirmed', 'connected', 'enabled'].includes(status?.state)
            "
            type="button"
            class="primary-action-btn"
            :disabled="busy"
            @click="createQr"
          >
            {{ t('qrShow') }}
          </button>
        </div>
        <PairingQr
          v-if="invitation"
          :code="invitation.code"
          :expires-at="invitation.expires_at"
          @expired="invitation = null"
          @error="error = 'PAIRING_QR_RENDER_FAILED'"
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

      <details v-if="!capabilities?.can_scan && !invitation" class="about-footer-details">
        <summary class="about-footer-summary">{{ t('qrAdvanced') }}</summary>
        <ManualPairing @changed="emit('changed')" />
      </details>
    </section>
  </div>
</template>

<style scoped>
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
