<template>
  <section class="remote-settings" :aria-label="t('remote_agent.connections')">
    <header><h3><Plug :size="18" />{{ t('remote_agent.connections') }}</h3><button v-if="state.unlocked" @click="service.lock()"><LockKeyhole :size="16" />{{ t('remote_agent.lock') }}</button></header>
    <p>{{ t('remote_agent.vault_hint') }}</p>
    <form v-if="!state.unlocked" @submit.prevent="unlock">
      <label v-if="state.migrationRequired">{{ t('remote_agent.vault_password') }}<input v-model="password" type="password" minlength="12" maxlength="1024" autocomplete="off" required /></label>
      <button :disabled="state.storageBusy">{{ t('remote_agent.unlock') }}</button>
    </form>
    <template v-else>
      <article v-for="profile in state.profiles" :key="profile.id">
        <strong>{{ profile.name }}</strong><small>{{ profile.endpoint }}</small>
        <div class="actions">
          <button :disabled="state.busy" @click="service.connectProfile(profile.id)">{{ t('remote_agent.connect') }}</button>
          <button v-if="state.connection?.profileId === profile.id" @click="service.disconnect()">{{ t('remote_agent.disconnect') }}</button>
          <button @click="edit(profile)">{{ t('remote_agent.update_auth') }}</button>
          <button @click="forgetId = profile.id">{{ t('remote_agent.forget') }}</button>
        </div>
        <label class="automatic"><input type="checkbox" :checked="profile.autoReconnect" @change="service.automatic(profile.id, $event.target.checked)" />{{ t('remote_agent.auto_reconnect') }}</label>
        <div v-if="forgetId === profile.id" class="actions"><p>{{ t('remote_agent.forget_hint') }}</p><button @click="forget(profile.id)">{{ t('remote_agent.confirm_forget') }}</button><button @click="forgetId = ''">{{ t('common.cancel') }}</button></div>
      </article>
      <form @submit.prevent="save">
        <h4>{{ t('remote_agent.add_connection') }}</h4>
        <label>{{ t('remote_agent.name') }}<input v-model="name" maxlength="120" required /></label>
        <label>{{ t('remote_agent.endpoint') }}<input v-model="endpoint" type="url" autocomplete="off" spellcheck="false" required /></label>
        <label>{{ t('remote_agent.token') }}<input v-model="token" type="password" autocomplete="off" spellcheck="false" /></label>
        <button :disabled="state.busy">{{ t(state.busy ? 'remote_agent.connecting' : 'remote_agent.save_connect') }}</button>
      </form>
    </template>
    <p v-if="state.connection && !state.error && !state.busy" role="status">{{ t('remote_agent.connection_ready') }}</p>
    <p v-if="state.error" class="error" role="alert">{{ t(`remote_agent.errors.${te(`remote_agent.errors.${state.error}`) ? state.error : 'REMOTE_UNKNOWN'}`) }}</p>
  </section>
</template>
<script setup>
import { onMounted, onBeforeUnmount, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { Plug, LockKeyhole } from 'lucide-vue-next';
import { getRemoteAgentService } from '../../work/remote-agent-service.js';
const { t, te } = useI18n();
const service = getRemoteAgentService(window.appAPI), state = service.state;
const password = ref(''), name = ref(''), endpoint = ref(''), token = ref(''), forgetId = ref('');
async function unlock() { const value = state.migrationRequired ? password.value : undefined; password.value = ''; await service.unlock(value); }
function edit(profile) { name.value = profile.name; endpoint.value = profile.endpoint; token.value = ''; }
async function forget(id) { await service.forget(id); forgetId.value = ''; }
async function save() {
  const input = { adapter: 'dsh', endpoint: endpoint.value, token: token.value };
  token.value = '';
  try { const url = new URL(endpoint.value); url.search = ''; url.hash = ''; endpoint.value = url.toString(); } catch { endpoint.value = ''; }
  await service.save(input, name.value);
}
onMounted(() => service.load());
onBeforeUnmount(() => { password.value = ''; token.value = ''; });
</script>
<style scoped>
.remote-settings { background: var(--bg-secondary); color: var(--text-primary); padding: 20px; border: 1px solid var(--border-color); border-radius: 16px; }
header, h3, .actions { display: flex; align-items: center; flex-wrap: wrap; gap: 12px; }
header { justify-content: space-between; } h3 { margin: 0; }
p, small { color: var(--text-secondary); line-height: 1.6; } small { display: block; overflow-wrap: anywhere; }
form, label { display: grid; gap: 8px; } form { gap: 14px; margin-top: 20px; }
article { border-top: 1px solid var(--border-color); padding-block: 18px; }
input { min-width: 0; box-sizing: border-box; width: 100%; min-height: 44px; font-size: 16px; padding: 10px; border: 1px solid var(--border-color); border-radius: 8px; background: var(--bg-primary); color: var(--text-primary); }
button { display: inline-flex; align-items: center; gap: 8px; min-height: 44px; padding: 8px 14px; font: inherit; border: 1px solid var(--border-color); border-radius: 10px; color: var(--text-primary); background: var(--bg-primary); cursor: pointer; }
button:disabled { opacity: .5; } .actions { margin-top: 12px; } .automatic { display: flex; align-items: center; min-height: 44px; } .automatic input { width: 20px; }
.error { color: var(--color-error); }
</style>
