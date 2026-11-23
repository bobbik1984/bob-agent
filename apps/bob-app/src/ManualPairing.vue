<script setup>
import { computed, ref, onUnmounted } from 'vue';
import { useI18n } from 'vue-i18n';
import CustomSelect from './components/CustomSelect.vue';
import { kernelBridge } from './native-bridge.mjs';
const { t } = useI18n();
const emit = defineEmits(['changed']);
const open = ref(true), role = ref('replica'), busy = ref(false), error = ref('');
const roleOptions = computed(() => [
  { value: 'replica', label: t('pairReplica') },
  { value: 'home', label: t('pairHome') },
]);
const deviceCode = ref(''), invitationCode = ref(''), output = ref('');
const bindAddress = ref(''), advertisedHost = ref(''), status = ref(null);
let poll = null;
let disposed = false;
const stateKeys = {unconfigured:'pairUnconfigured',open:'pairOpen',pending:'pairPending',pending_local_confirmation:'pairPending',proposing:'pairProposing',unknown:'pairUnknown',confirmed:'pairConfirmed',consumed:'pairConfirmed',connected:'pairConnected',enabled:'pairEnabled',expired:'pairExpired',stopped:'pairStopped',error:'pairError'};
function stateKey() { return stateKeys[status.value?.state] || 'pairUnconfirmed'; }
async function reset() { await perform(() => kernelBridge.pairingReset()); output.value = ''; invitationCode.value = ''; }
async function refresh() {
  if (!open.value || busy.value || disposed) return;
  try { status.value = await kernelBridge.pairingStatus(); }
  catch { error.value = 'PAIRING_STATUS_UNAVAILABLE'; }
}
async function toggle() {
  open.value = !open.value;
  if (poll) { clearInterval(poll); poll = null; }
  if (open.value) { await refresh(); poll = setInterval(refresh,3000); }
}
async function perform(action) {
  if (busy.value) return;
  busy.value = true; error.value = '';
  try {
    const result = await action();
    if (result?.code) output.value = result.code;
    if (result?.state === 'connected') { output.value = ''; invitationCode.value = ''; }
    status.value = result;
    emit('changed');
  } catch (e) {
    // Native errors are fixed identifiers; never echo pasted pairing material.
    const code = String(e?.message || e || 'PAIRING_FAILED');
    error.value = /^[A-Z][A-Z0-9_]{2,90}$/.test(code) ? code : 'PAIRING_FAILED';
  } finally { busy.value = false; }
}
onUnmounted(() => { disposed = true; if (poll) clearInterval(poll); });
</script>
<template>
  <div class="home-connection">
    <section v-if="open" class="pair-panel" :aria-label="t('homeConnection')">
      <p class="pair-intro">{{ t('pairIntro') }}</p>
      <label>{{ t('pairRole') }}<CustomSelect v-model="role" :options="roleOptions" :disabled="busy" /></label>
      <template v-if="role === 'replica'">
        <button type="button" :disabled="busy" @click="perform(() => kernelBridge.pairingDeviceCode())">{{ t('pairExportDevice') }}</button>
        <label>{{ t('pairPasteInvitation') }}<textarea v-model="invitationCode" rows="2" spellcheck="false" autocomplete="off" :disabled="busy" /></label>
        <button type="button" :disabled="busy || !invitationCode.trim()" @click="perform(() => kernelBridge.pairingPropose(invitationCode))">{{ t('pairPropose') }}</button>
        <p>{{ t('pairConfirmHint') }}</p>
        <button type="button" :disabled="busy" @click="perform(() => kernelBridge.pairingFinish())">{{ t('pairFinish') }}</button>
      </template>
      <template v-else>
        <label>{{ t('pairPasteDevice') }}<textarea v-model="deviceCode" rows="2" spellcheck="false" autocomplete="off" :disabled="busy" /></label>
        <label>{{ t('pairListenAddress') }}<input v-model="bindAddress" :disabled="busy" autocomplete="off" spellcheck="false" /></label>
        <label>{{ t('pairReachableHost') }}<input v-model="advertisedHost" :disabled="busy" autocomplete="off" spellcheck="false" /></label>
        <p>{{ t('pairAddressHint') }}</p>
        <button type="button" :disabled="busy || !deviceCode.trim() || !bindAddress.trim() || !advertisedHost.trim()" @click="perform(() => kernelBridge.pairingHomeInvite(deviceCode,bindAddress,advertisedHost))">{{ t('pairCreateInvite') }}</button>
        <button type="button" :disabled="busy || !['pending','pending_local_confirmation'].includes(status?.state)" @click="perform(() => kernelBridge.pairingHomeConfirm())">{{ t('pairConfirmDevice') }}</button>
        <p>{{ t('pairReadonlyPolicy') }}</p>
        <button type="button" :disabled="busy || !['confirmed','consumed','connected','enabled'].includes(status?.state)" @click="perform(() => kernelBridge.pairingEnableHome())">{{ t('pairEnableReadonly') }}</button>
      </template>
      <label v-if="output">{{ t('pairOutputCode') }}<textarea :value="output" readonly rows="3" spellcheck="false" @focus="$event.target.select()" /></label>
      <p v-if="output" class="pair-intro">{{ t('pairCodeHint') }}</p>
      <div v-if="status" class="pair-state" role="status">
        <strong>{{ t(stateKey()) }}</strong>
        <span v-if="status.home_pin">{{ t('pairHomeFingerprint') }} <code>{{ status.home_pin }}</code></span>
        <span v-if="status.candidate_pin">{{ t('pairFingerprint') }} <code>{{ status.candidate_pin }}</code></span>
      </div>
      <p v-if="error" role="alert">{{ t('pairFailed') }} <code>{{ error }}</code></p>
      <p v-if="status?.error" role="alert">{{ t('pairFailed') }} <code>{{ status.error }}</code></p>
      <button type="button" :disabled="busy" @click="perform(() => kernelBridge.pairingRecover())">{{ t('pairRecover') }}</button>
      <button type="button" :disabled="busy" @click="reset">{{ t('pairReset') }}</button>
      <button type="button" :disabled="busy" @click="refresh">{{ t('checkStatus') }}</button>
    </section>
  </div>
</template>
<style scoped>
.connection-toggle { padding: .3rem .6rem; font-size: .85rem; }
.pair-panel { margin-top: .6rem; padding: .8rem; border: 1px solid var(--line); border-radius: .8rem; background: var(--bg-secondary); display: grid; gap: .65rem; max-height: min(65dvh,38rem); overflow-y: auto; scrollbar-gutter: stable; }
.pair-panel label { display: grid; gap: .3rem; font-size: .85rem; }
.pair-panel input,.pair-panel select,.pair-panel textarea { min-width: 0; width: 100%; font: inherit; padding: .5rem; border: 1px solid var(--line); border-radius: .5rem; background: var(--bg-primary); color: var(--text-primary); }
.pair-panel p { margin: 0; font-size: .85rem; line-height: 1.5; }
.pair-intro { color: var(--text-secondary); }
.pair-state { display: grid; gap: .4rem; }
.pair-panel code { overflow-wrap: anywhere; font-size: .75rem; }
</style>
