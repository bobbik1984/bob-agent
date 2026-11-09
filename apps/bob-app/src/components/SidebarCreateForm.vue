<script setup>
import { inject } from 'vue';
import { nodeLabel } from '../conversation-directory.mjs';

const {
  t, remoteCreationEnabled, catalog, creatingKind, creatingForProjectId, newItemTitle,
  creationMode, creationConnectionId, creationWorkspace, creationNodes, creationWorkspaces,
  creationLoading, creationBusy, creationError, remoteCreation, loadCreationWorkspaces,
  cancelCreateItem, submitCreateItem,
} = inject('bobAppShell');
</script>

<template>
  <form v-if="creatingKind" class="sidebar-create-form" @submit.prevent="submitCreateItem">
    <fieldset :disabled="creationBusy || !!remoteCreation">
      <label>
        <span>{{ t(creatingKind === 'project' ? 'projectTitlePlaceholder' : 'conversationTitlePlaceholder') }}</span>
        <input v-model="newItemTitle" type="text" maxlength="256" :required="creatingKind === 'project'" />
      </label>
      <label v-if="creatingKind === 'conversation'">
        <span>{{ t('projectLinkLabel') }}</span>
        <select v-model="creatingForProjectId">
          <option :value="null">{{ t('projectLinkNone') }}</option>
          <option v-for="project in catalog?.projects || []" :key="project.id" :value="project.id">
            {{ project.title }}
          </option>
        </select>
      </label>
      <label>
        <span>{{ t(creatingKind === 'project' ? 'createFirstSession' : 'createSessionTool') }}</span>
        <select v-model="creationMode" @change="loadCreationWorkspaces">
          <option v-if="creatingKind === 'project'" value="none">{{ t('createProjectOnly') }}</option>
          <option value="local">{{ t('createLocalAssistant') }}</option>
          <option v-if="remoteCreationEnabled" value="dsh">DSH</option>
        </select>
      </label>
      <template v-if="creationMode === 'dsh'">
        <label>
          <span>{{ t('createSessionNode') }}</span>
          <select v-model="creationConnectionId" required @change="loadCreationWorkspaces">
            <option disabled value="">{{ t('createSelectNode') }}</option>
            <option v-for="node in creationNodes" :key="node.id" :value="node.id">{{ nodeLabel(node) }}</option>
          </select>
        </label>
        <label>
          <span>{{ t('createSessionWorkspace') }}</span>
          <select v-model="creationWorkspace" required :disabled="creationLoading || !creationWorkspaces.length">
            <option disabled value="">{{ t(creationLoading ? 'dshLoading' : 'createSelectWorkspace') }}</option>
            <option v-for="workspace in creationWorkspaces" :key="workspace" :value="workspace">{{ workspace }}</option>
          </select>
        </label>
      </template>
    </fieldset>
    <p v-if="creationError" role="status">{{ t(creationError) }}</p>
    <div class="sidebar-create-buttons">
      <button type="button" :disabled="creationBusy" @click="cancelCreateItem">{{ t('cancelAction') }}</button>
      <button type="submit" :disabled="creationBusy || creationLoading || (creationMode === 'dsh' && !creationWorkspace)">
        {{ t(creationBusy ? 'createItemPending' : remoteCreation ? 'createRetry' : 'create') }}
      </button>
    </div>
  </form>
</template>

<style scoped>
fieldset { display: grid; gap: .45rem; min-width: 0; padding: 0; margin: 0; border: 0; }
label { display: grid; gap: .2rem; min-width: 0; font-size: var(--text-xs); color: var(--text-secondary); }
select { width: 100%; min-width: 0; padding: .35rem .5rem; }
p { margin: 0; color: var(--text-secondary); font-size: var(--text-xs); overflow-wrap: anywhere; }
</style>
