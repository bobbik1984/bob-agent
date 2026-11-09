<script setup>
import { inject } from 'vue';
import ConversationDirectory from './ConversationDirectory.vue';
import SidebarCreateForm from './SidebarCreateForm.vue';

const props = defineProps({
  docked: {
    type: Boolean,
    default: false,
  },
});

const {
  t,
  brandLogo,
  navigationOpen,
  remoteConversation,
  sessionId,
  assistantConversationId,
  openAssistant,
  sidebarGroupMode,
  switchSidebarMode,
  organizeSessionsViaAgent,
  sidebarSearch,
  startCreateConversation,
  startCreateProject,
  directoryCreationRevision,
  modelHub,
  catalog,
  directorySyncRevision,
  selectConversation,
  deleteLocalConversation,
  selectRemoteConversation,
} = inject('bobAppShell');
</script>

<template>
  <nav
    v-if="docked"
    class="navigation-drawer desktop-docked-drawer"
    :aria-label="t('conversations')"
  >
    <div class="settings-heading">
      <h2>{{ t('conversations') }}</h2>
    </div>

    <button
      type="button"
      class="sidebar-primary-agent-row"
      :class="{ active: !remoteConversation && sessionId === assistantConversationId }"
      @click="openAssistant"
    >
      <span class="agent-row-icon" aria-hidden="true">
        <img :src="brandLogo" alt="" class="agent-row-logo" />
      </span>
      <span class="agent-row-text">
        <strong>{{ t('defaultAssistant') }}</strong>
        <small>{{ t('defaultAssistantDesc') }}</small>
      </span>
    </button>

        <div class="sidebar-mode-switch" role="group">
      <button
        type="button"
        class="sidebar-mode-btn"
        :class="{ active: sidebarGroupMode === 'project' }"
        :title="t('groupByProject')"
        :aria-label="t('groupByProject')"
        @click="switchSidebarMode('project')"
      >
        <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
          <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
        </svg>
        <span>{{ t('modeProjectShort') }}</span>
      </button>
      <button
        type="button"
        class="sidebar-mode-btn"
        :class="{ active: sidebarGroupMode === 'smart' }"
        :title="t('groupBySmart')"
        :aria-label="t('groupBySmart')"
        @click="switchSidebarMode('smart')"
      >
        <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
          <path d="m12 3-1.9 5.8a2 2 0 0 1-1.3 1.3L3 12l5.8 1.9a2 2 0 0 1 1.3 1.3L12 21l1.9-5.8a2 2 0 0 1 1.3-1.3L21 12l-5.8-1.9a2 2 0 0 1-1.3-1.3z" />
        </svg>
        <span>{{ t('modeSmartShort') }}</span>
      </button>
      <button
        type="button"
        class="sidebar-mode-btn"
        :class="{ active: sidebarGroupMode === 'device' }"
        :title="t('groupByDevice')"
        :aria-label="t('groupByDevice')"
        @click="switchSidebarMode('device')"
      >
        <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
          <rect x="2" y="3" width="20" height="14" rx="2" ry="2" />
          <line x1="8" y1="21" x2="16" y2="21" />
          <line x1="12" y1="17" x2="12" y2="21" />
        </svg>
        <span>{{ t('modeDeviceShort') }}</span>
      </button>
    </div>

    <div class="sidebar-search-row">
      <input
        v-model="sidebarSearch"
        type="search"
        class="sidebar-search-input"
        :placeholder="t('sidebarSearch')"
        :aria-label="t('sidebarSearch')"
      />
      <button
        type="button"
        class="sidebar-icon-action-btn"
        :title="t('newConversation')"
        :aria-label="t('newConversation')"
        @click="startCreateConversation(null)"
      >
        <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
          <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
          <line x1="12" y1="7" x2="12" y2="13" />
          <line x1="9" y1="10" x2="15" y2="10" />
        </svg>
      </button>
      <button
        type="button"
        class="sidebar-icon-action-btn"
        :title="t('newProject')"
        :aria-label="t('newProject')"
        @click="startCreateProject"
      >
        <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
          <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
          <line x1="12" y1="11" x2="12" y2="17" />
          <line x1="9" y1="14" x2="15" y2="14" />
        </svg>
      </button>
    </div>

    <SidebarCreateForm />

    <ConversationDirectory :nodes="modelHub.state.remote_nodes || []" :catalog="catalog" :sync-revision="directorySyncRevision" :creation-revision="directoryCreationRevision" @create-in-project="startCreateConversation" :search="sidebarSearch" :mode="sidebarGroupMode" @select-local="selectConversation" :delete-local="deleteLocalConversation" @select-remote="selectRemoteConversation" />
  </nav>

  <div v-else v-show="navigationOpen" class="navigation-overlay" @keydown.esc="navigationOpen = false">
    <button
      type="button"
      class="navigation-backdrop"
      :aria-label="t('closeNavigation')"
      @click="navigationOpen = false"
    ></button>
    <nav class="navigation-drawer" :aria-label="t('conversations')">
      <div class="settings-heading">
        <h2>{{ t('conversations') }}</h2>
        <button
          type="button"
          :aria-label="t('closeNavigation')"
          @click="navigationOpen = false"
        >
          ×
        </button>
      </div>

      <button
        type="button"
        class="sidebar-primary-agent-row"
        :class="{ active: !remoteConversation && sessionId === assistantConversationId }"
        @click="openAssistant"
      >
        <span class="agent-row-icon" aria-hidden="true">
          <img :src="brandLogo" alt="" class="agent-row-logo" />
        </span>
        <span class="agent-row-text">
          <strong>{{ t('defaultAssistant') }}</strong>
          <small>{{ t('defaultAssistantDesc') }}</small>
        </span>
      </button>

          <div class="sidebar-mode-switch" role="group">
      <button
        type="button"
        class="sidebar-mode-btn"
        :class="{ active: sidebarGroupMode === 'project' }"
        :title="t('groupByProject')"
        :aria-label="t('groupByProject')"
        @click="switchSidebarMode('project')"
      >
        <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
          <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
        </svg>
        <span>{{ t('modeProjectShort') }}</span>
      </button>
      <button
        type="button"
        class="sidebar-mode-btn"
        :class="{ active: sidebarGroupMode === 'smart' }"
        :title="t('groupBySmart')"
        :aria-label="t('groupBySmart')"
        @click="switchSidebarMode('smart')"
      >
        <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
          <path d="m12 3-1.9 5.8a2 2 0 0 1-1.3 1.3L3 12l5.8 1.9a2 2 0 0 1 1.3 1.3L12 21l1.9-5.8a2 2 0 0 1 1.3-1.3L21 12l-5.8-1.9a2 2 0 0 1-1.3-1.3z" />
        </svg>
        <span>{{ t('modeSmartShort') }}</span>
      </button>
      <button
        type="button"
        class="sidebar-mode-btn"
        :class="{ active: sidebarGroupMode === 'device' }"
        :title="t('groupByDevice')"
        :aria-label="t('groupByDevice')"
        @click="switchSidebarMode('device')"
      >
        <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
          <rect x="2" y="3" width="20" height="14" rx="2" ry="2" />
          <line x1="8" y1="21" x2="16" y2="21" />
          <line x1="12" y1="17" x2="12" y2="21" />
        </svg>
        <span>{{ t('modeDeviceShort') }}</span>
      </button>
    </div>

    <div class="sidebar-search-row">
      <input
        v-model="sidebarSearch"
        type="search"
        class="sidebar-search-input"
        :placeholder="t('sidebarSearch')"
        :aria-label="t('sidebarSearch')"
      />
      <button
        type="button"
        class="sidebar-icon-action-btn"
        :title="t('newConversation')"
        :aria-label="t('newConversation')"
        @click="startCreateConversation(null)"
      >
        <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
          <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
          <line x1="12" y1="7" x2="12" y2="13" />
          <line x1="9" y1="10" x2="15" y2="10" />
        </svg>
      </button>
      <button
        type="button"
        class="sidebar-icon-action-btn"
        :title="t('newProject')"
        :aria-label="t('newProject')"
        @click="startCreateProject"
      >
        <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
          <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
          <line x1="12" y1="11" x2="12" y2="17" />
          <line x1="9" y1="14" x2="15" y2="14" />
        </svg>
      </button>
    </div>

      <SidebarCreateForm />

      <ConversationDirectory :nodes="modelHub.state.remote_nodes || []" :catalog="catalog" :sync-revision="directorySyncRevision" :creation-revision="directoryCreationRevision" @create-in-project="startCreateConversation" :search="sidebarSearch" :mode="sidebarGroupMode" @select-local="selectConversation" :delete-local="deleteLocalConversation" @select-remote="selectRemoteConversation" />
    </nav>
  </div>
</template>
