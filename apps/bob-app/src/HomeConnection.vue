<script setup>
import {ref,onMounted,onUnmounted} from 'vue';
import {useI18n} from 'vue-i18n';
import {kernelBridge} from './native-bridge.mjs';
import PairingQr from './PairingQr.vue';
import ManualPairing from './ManualPairing.vue';
const {t}=useI18n();const emit=defineEmits(['changed']);
const open=ref(false),capabilities=ref(null),status=ref(null),invitation=ref(null),busy=ref(false),error=ref('');
let poll=null,disposed=false,reading=false;
const errorKeys={PAIRING_HOME_UNREACHABLE:'qrHomeUnreachable',PAIRING_NETWORK_UNAVAILABLE:'qrNetworkUnavailable',PAIRING_PROPOSAL_UNCONFIRMED:'qrProposalUnknown',PAIRING_ALREADY_ACTIVE:'qrAlreadyActive',SCAN_PERMISSION_DENIED:'qrCameraDenied',SCAN_UNAVAILABLE:'qrCameraUnavailable',SCAN_UNSUPPORTED:'qrCameraUnsupported',SCAN_INVALID:'qrInvalid',SCAN_BUSY:'qrCameraBusy',PAIRING_QR_RENDER_FAILED:'qrRenderFailed',PAIRING_CODE_INVALID:'qrInvalid',PAIRING_INVITATION_EXPIRED:'pairExpired'};
const keys={unconfigured:'pairUnconfigured',open:'qrWaiting',pending:'qrConfirmPhone',confirmed:'pairConfirmed',consumed:'pairConfirmed',connected:'pairConnected',enabled:'pairEnabled',expired:'pairExpired',unknown:'pairUnknown',proposing:'pairProposing',stopped:'pairStopped',revoked:'qrRevoked'};
async function refresh(){if(!open.value||busy.value||reading||disposed)return;reading=true;try{status.value=await kernelBridge.pairingStatus();if(capabilities.value?.can_scan && ['pending','unknown'].includes(status.value?.state)){try{status.value=await kernelBridge.pairingFinish();}catch(e){if(String(e?.message||e)!=='PAIRING_CONFIRMATION_UNAVAILABLE')throw e;}}if(['pending','confirmed','connected','enabled','expired'].includes(status.value?.state))invitation.value=null;emit('changed');}catch{error.value='PAIRING_STATUS_UNAVAILABLE';}finally{reading=false;}}
async function toggle(){open.value=!open.value;if(poll)clearInterval(poll);poll=null;if(!open.value)return;try{capabilities.value=await kernelBridge.pairingCapabilities();await refresh();poll=setInterval(refresh,2000);}catch{error.value='PAIRING_CAPABILITIES_UNAVAILABLE';}}
async function act(work){if(busy.value)return;busy.value=true;error.value='';try{const result=await work();status.value=result;if(result?.code)invitation.value=result;emit('changed');}catch(e){const code=String(e?.message||e);if(code!=='SCAN_CANCELLED')error.value=/^[A-Z][A-Z0-9_]{2,90}$/.test(code)?code:'PAIRING_FAILED';}finally{busy.value=false;}}
async function createQr(){await act(async()=>{if(status.value?.state==='expired')await kernelBridge.pairingReset();return kernelBridge.pairingQrCreate();});}
async function scan(){await act(async()=>{const result=await kernelBridge.scanPairingCode();if(!result)return status.value;const code=typeof result==='string'?result:result.code;if(!code)return status.value;return kernelBridge.pairingQrPropose(code);});}
async function confirm(){await act(()=>kernelBridge.pairingHomeConfirm());await refresh();}
async function reset(){await act(()=>kernelBridge.pairingReset());invitation.value=null;}
onMounted(async()=>{try{const caps=await kernelBridge.pairingCapabilities();capabilities.value=caps;if(caps.start_pairing && caps.can_host){await toggle();if(!status.value || status.value.state==='unconfigured')await act(()=>kernelBridge.pairingQrCreate());}}catch{/* Opening chat remains usable when setup is unavailable. */}});
onUnmounted(()=>{disposed=true;if(poll)clearInterval(poll);invitation.value=null;});
</script>
<template><div class="home-connection">
 <button type="button" class="connection-toggle" :aria-expanded="open" @click="toggle">{{t(capabilities?.can_host?'qrConnectPhone':'homeConnection')}}</button>
 <section v-if="open" class="qr-panel" :aria-label="t('homeConnection')">
  <template v-if="capabilities?.can_host">
   <h2>{{t('qrConnectPhone')}}</h2><p>{{t('qrHomeHint')}}</p>
   <button v-if="!invitation && !['pending','confirmed','connected','enabled'].includes(status?.state)" type="button" :disabled="busy" @click="createQr">{{t('qrShow')}}</button>
   <PairingQr v-if="invitation" :code="invitation.code" :expires-at="invitation.expires_at" @expired="invitation=null" @error="error='PAIRING_QR_RENDER_FAILED'"/>
   <div v-if="status?.state==='pending'" class="pair-confirm"><p>{{t('qrConfirmHint')}}</p><code>{{status.candidate_pin}}</code><button type="button" :disabled="busy" @click="confirm">{{t('pairConfirmDevice')}}</button></div>
   <template v-if="['confirmed','consumed','connected'].includes(status?.state)"><p>{{t('pairReadonlyPolicy')}}</p><button type="button" :disabled="busy" @click="act(()=>kernelBridge.pairingEnableHome())">{{t('pairEnableReadonly')}}</button></template>
  </template>
  <template v-if="capabilities?.can_scan"><h2>{{t('qrConnectHome')}}</h2><p>{{t('qrPhoneHint')}}</p><button v-if="status?.state!=='connected'" type="button" :disabled="busy" @click="scan">{{t('qrScan')}}</button></template>
  <p v-if="status" role="status">{{t(keys[status.state]||'pairUnconfirmed')}}</p>
  <p v-if="error || status?.error" role="alert">{{t(errorKeys[error||status.error]||'pairFailed')}}<code v-if="!errorKeys[error||status.error]"> {{error||status.error}}</code></p>
  <button v-if="status && !['unconfigured','connected','enabled'].includes(status.state)" type="button" :disabled="busy" @click="reset">{{t('pairReset')}}</button>
  <template v-if="capabilities?.can_host && ['confirmed','consumed','connected','enabled'].includes(status?.state)"><p>{{t('qrRevokeHint')}}</p><button type="button" :disabled="busy" @click="act(()=>kernelBridge.pairingRevoke())">{{t('qrRevoke')}}</button></template>
  <details><summary>{{t('qrAdvanced')}}</summary><ManualPairing @changed="emit('changed')"/></details>
 </section></div></template>
<style scoped>
.connection-toggle{padding:.3rem .6rem;font-size:.85rem}.qr-panel{margin-top:.6rem;padding:1rem;border:1px solid var(--line);border-radius:.9rem;background:var(--bg-secondary);display:grid;gap:.8rem;max-height:60dvh;overflow:auto;scrollbar-gutter:stable}.qr-panel h2{font-size:1.1rem;margin:0}.qr-panel p{margin:0;line-height:1.5;font-size:.9rem;color:var(--text-secondary)}.qr-panel code{overflow-wrap:anywhere;font-size:.75rem}.pair-confirm{display:grid;gap:.6rem}.qr-panel details[open]>summary{margin-bottom:.8rem}.qr-panel summary{cursor:pointer;font-size:.85rem;color:var(--text-secondary)}
</style>
