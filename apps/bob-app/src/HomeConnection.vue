<script setup>
import {ref,computed,onMounted,onUnmounted} from 'vue';
import {useI18n} from 'vue-i18n';
import {kernelBridge} from './native-bridge.mjs';
import PairingQr from './PairingQr.vue';
import ManualPairing from './ManualPairing.vue';
import {createPairingScanController} from './pairing-scan-controller.mjs';
const {t}=useI18n();const emit=defineEmits(['changed','connected']);
const props=defineProps({paired:{type:Boolean,default:false}});
const paired=computed(()=>props.paired || ['connected','enabled'].includes(status.value?.state));
const open=ref(false),capabilities=ref(null),status=ref(null),invitation=ref(null),busy=ref(false),error=ref('');
let poll=null,disposed=false,reading=false;
const scanPhase=ref(null);
const scanner=createPairingScanController({bridge:kernelBridge,onPhase:p=>{scanPhase.value=p;},onStatus:s=>{status.value=s;},onConnected:completePairing});
const errorKeys={SCAN_TIMEOUT:'qrScanTimeout',SCAN_CANCELLED:'qrScanCancelled',PAIRING_HOME_UNREACHABLE:'qrHomeUnreachable',PAIRING_NETWORK_UNAVAILABLE:'qrNetworkUnavailable',PAIRING_PROPOSAL_UNCONFIRMED:'qrProposalUnknown',PAIRING_ALREADY_ACTIVE:'qrAlreadyActive',SCAN_PERMISSION_DENIED:'qrCameraDenied',SCAN_UNAVAILABLE:'qrCameraUnavailable',SCAN_UNSUPPORTED:'qrCameraUnsupported',SCAN_INVALID:'qrInvalid',SCAN_BUSY:'qrCameraBusy',PAIRING_QR_RENDER_FAILED:'qrRenderFailed',PAIRING_CODE_INVALID:'qrInvalid',PAIRING_INVITATION_EXPIRED:'pairExpired'};
const keys={unconfigured:'pairUnconfigured',open:'qrWaiting',pending:'qrConfirmPhone',confirmed:'pairConfirmed',consumed:'pairConfirmed',connected:'pairConnected',enabled:'pairEnabled',expired:'pairExpired',unknown:'pairUnknown',proposing:'pairProposing',stopped:'pairStopped',revoked:'qrRevoked'};
function completePairing(){if(!capabilities.value?.can_scan)return;open.value=false;if(poll)clearInterval(poll);poll=null;emit('connected');}
async function refresh(){if(!open.value||busy.value||reading||disposed)return;reading=true;try{status.value=await kernelBridge.pairingStatus();if(capabilities.value?.can_scan && ['pending','unknown'].includes(status.value?.state)){try{status.value=await kernelBridge.pairingFinish();if(status.value?.state==='connected'){scanPhase.value='connected';error.value='';completePairing();}}catch(e){if(String(e?.message||e)!=='PAIRING_CONFIRMATION_UNAVAILABLE')throw e;}}if(['pending','confirmed','connected','enabled','expired'].includes(status.value?.state))invitation.value=null;emit('changed');}catch(e){const code=String(e?.message||e);error.value=/^[A-Z][A-Z0-9_]{2,90}$/.test(code)?code:'PAIRING_STATUS_UNAVAILABLE';}finally{reading=false;}}
async function toggle(){open.value=!open.value;if(poll)clearInterval(poll);poll=null;if(!open.value)return;try{capabilities.value=await kernelBridge.pairingCapabilities();await refresh();if(capabilities.value?.can_host && ['confirmed','consumed','connected'].includes(status.value?.state)){await act(()=>kernelBridge.pairingEnableHome());}poll=setInterval(refresh,2000);}catch{error.value='PAIRING_CAPABILITIES_UNAVAILABLE';}}
async function act(work){if(busy.value)return;busy.value=true;error.value='';try{const result=await work();status.value=result;if(result?.code)invitation.value=result;emit('changed');}catch(e){const code=String(e?.message||e);error.value=/^[A-Z][A-Z0-9_]{2,90}$/.test(code)?code:'PAIRING_FAILED';}finally{busy.value=false;}}
async function createQr(){await act(async()=>{if(status.value?.state==='expired')await kernelBridge.pairingReset();return kernelBridge.pairingQrCreate();});}
async function scan(){await act(()=>scanner.scan());}

async function reset(){await act(()=>kernelBridge.pairingReset());invitation.value=null;}
onMounted(async()=>{try{const caps=await kernelBridge.pairingCapabilities();capabilities.value=caps;if(caps.start_pairing && caps.can_host){await toggle();if(!status.value || ['unconfigured','revoked','stopped'].includes(status.value.state))await act(()=>kernelBridge.pairingQrCreate());}}catch{/* Opening chat remains usable when setup is unavailable. */}});
onUnmounted(()=>{disposed=true;if(poll)clearInterval(poll);invitation.value=null;});
</script>
<template><div class="home-connection">
 <button type="button" class="connection-toggle" :class="{paired}" :aria-expanded="open" @click="toggle">{{t(capabilities?.can_host?(paired?'qrPhonePaired':'qrConnectPhone'):(paired?'qrHomePaired':'homeConnection'))}}</button>
 <section v-if="open" class="qr-panel" :class="{'qr-display':!!invitation}" :aria-label="t('homeConnection')">
  <button v-if="invitation" type="button" class="qr-close" :aria-label="t('qrClose')" @click="toggle">×</button>
  <template v-if="capabilities?.can_host">
   <h2>{{t('qrConnectPhone')}}</h2><p>{{t('qrHomeHint')}}</p>
   <button v-if="!invitation && !['pending','confirmed','connected','enabled'].includes(status?.state)" type="button" :disabled="busy" @click="createQr">{{t('qrShow')}}</button>
   <PairingQr v-if="invitation" :code="invitation.code" :expires-at="invitation.expires_at" @expired="invitation=null" @error="error='PAIRING_QR_RENDER_FAILED'"/>
   <p v-if="['confirmed','consumed','connected','enabled'].includes(status?.state)">{{t('qrPairedPolicy')}}</p>
  </template>
  <template v-if="capabilities?.can_scan"><h2>{{t('qrConnectHome')}}</h2><p>{{t('qrPhoneHint')}}</p><button type="button" :disabled="busy" @click="scan">{{t('qrScan')}}</button></template>
  <p v-if="scanPhase && scanPhase!=='failed'" role="status" aria-live="polite">{{t({scanning:'qrScanning',connecting:'qrDecodedConnecting',verifying:'qrVerifying',connected:'pairConnected'}[scanPhase])}}</p>
  <p v-else-if="status && scanPhase!=='failed'" role="status">{{t(keys[status.state]||'pairUnconfirmed')}}</p>
  <p v-if="error || status?.error" role="alert">{{t(errorKeys[error||status.error]||'pairFailed')}}<code v-if="!errorKeys[error||status.error]"> {{error||status.error}}</code></p>
  <button v-if="status && !['unconfigured','connected','enabled'].includes(status.state)" type="button" :disabled="busy" @click="reset">{{t('pairReset')}}</button>
  <template v-if="capabilities?.can_host && ['confirmed','consumed','connected','enabled'].includes(status?.state)"><p>{{t('qrRevokeHint')}}</p><button type="button" :disabled="busy" @click="act(()=>kernelBridge.pairingRevoke())">{{t('qrRevoke')}}</button></template>
  <details v-if="!capabilities?.can_scan && !invitation"><summary>{{t('qrAdvanced')}}</summary><ManualPairing @changed="emit('changed')"/></details>
 </section></div></template>
<style scoped>
.connection-toggle.paired{border-color:var(--accent-primary);background:var(--bg-tertiary);font-weight:600}.connection-toggle{padding:.3rem .6rem;font-size:.85rem}.qr-panel{margin-top:.6rem;padding:1rem;border:1px solid var(--line);border-radius:.9rem;background:var(--bg-secondary);display:grid;gap:.8rem;max-height:60dvh;overflow:auto;scrollbar-gutter:stable}.qr-panel h2{font-size:1.1rem;margin:0}.qr-panel p{margin:0;line-height:1.5;font-size:.9rem;color:var(--text-secondary)}.qr-panel code{overflow-wrap:anywhere;font-size:.75rem}.pair-confirm{display:grid;gap:.6rem}.qr-panel details[open]>summary{margin-bottom:.8rem}.qr-panel summary{cursor:pointer;font-size:.85rem;color:var(--text-secondary)}

.qr-display{position:fixed;inset:max(.75rem,env(safe-area-inset-top)) max(.75rem,env(safe-area-inset-right)) max(.75rem,env(safe-area-inset-bottom)) max(.75rem,env(safe-area-inset-left));z-index:20;margin:0;max-height:none;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:.6rem;overflow:auto;text-align:center;box-shadow:0 0 0 2rem var(--bg-root)}
.qr-display h2{padding-inline:2rem}.qr-display p{max-width:42rem}.qr-close{position:absolute;right:.75rem;top:.75rem;font-size:1.4rem;padding:.2rem .6rem}
</style>
