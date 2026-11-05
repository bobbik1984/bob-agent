<script setup>
import {ref} from 'vue';
import {useI18n} from 'vue-i18n';
import {kernelBridge} from './native-bridge.mjs';
import PairingQr from './PairingQr.vue';
import {nodeAddress,nodeClient,invitationUri} from './node-connection.mjs';
const {t}=useI18n();
const emit=defineEmits(['connected','close']);
const address=ref(''),password=ref(''),name=ref(''),info=ref(null),busy=ref(false),error=ref(''),qr=ref(null),connected=ref(false);
let base='', owner='';
async function discover(){
 busy.value=true;error.value='';
 try {base=nodeAddress(address.value.trim());const found=await nodeClient(base)('info');if(found.protocol!=='bob-node-v1'||found.authentication!=='owner_password')throw Error();info.value=found;}
 catch{error.value='nodeConnectUnavailable';}finally{busy.value=false;}
}
async function connect(){
 busy.value=true;error.value='';
 try {
  const api=nodeClient(base);
  const auth=await api('login',{password:password.value});password.value='';owner=auth.access_token;
  const invite=await api('invitations',{},owner);
  const credential=await api('exchange',{pairing_id:invite.pairing_id,code:invite.user_code,device_name:'Bob operator'});
  try {await kernelBridge.remoteNodeSave({deviceId:info.value.node_id,label:name.value.trim()||new URL(base).hostname,baseUrl:base,token:credential.access_token,enabled:true});}
  catch(e){await api('revoke',{},credential.access_token).catch(()=>{});throw e;}
  connected.value=true;emit('connected');
  const mobile=await api('invitations',{},owner);qr.value={code:invitationUri(base,mobile),expires_at:mobile.expires_at};
 }catch(e){error.value=e?.message==='NODE_RATE_LIMITED'?'nodeConnectRate':'nodeConnectFailed';}
 finally{password.value='';owner='';busy.value=false;}
}
</script>
<template>
 <section class="node-connect">
  <form v-if="!info" @submit.prevent="discover">
   <label>{{t('connectionAddress')}}<input v-model="address" required autocomplete="url" :disabled="busy" /></label>
   <button :disabled="busy">{{t(busy?'nodeConnectChecking':'nodeConnectCheck')}}</button>
  </form>
  <form v-else-if="!connected" @submit.prevent="connect">
   <p>{{base}}</p>
   <label>{{t('connectionName')}}<input v-model="name" :disabled="busy" /></label>
   <label>{{t('nodeConnectPassword')}}<input v-model="password" type="password" autocomplete="current-password" required :disabled="busy" /></label>
   <p class="view-card-desc">{{t('nodeConnectHint')}}</p>
   <button :disabled="busy">{{t(busy?'nodeConnectChecking':'nodeConnectConfirm')}}</button>
  </form>
  <template v-else><p role="status">{{t('nodeConnectSaved')}}</p><PairingQr v-if="qr" :code="qr.code" :expires-at="qr.expires_at" @expired="qr=null" /><p>{{t(qr?'nodeConnectScan':'nodeConnectExpired')}}</p></template>
  <p v-if="error" role="alert">{{t(error)}}</p>
  <button type="button" :disabled="busy" @click="password='';owner='';qr=null;emit('close')">{{t('scheduleClose')}}</button>
 </section>
</template>
<style scoped>
.node-connect{padding:1rem;border:1px solid var(--border-color);border-radius:0.5rem;background:var(--bg-primary)}
form{display:grid;gap:0.75rem}label{display:grid;gap:0.3rem}input{min-width:0;width:100%;box-sizing:border-box}button{margin-top:0.5rem}
</style>
