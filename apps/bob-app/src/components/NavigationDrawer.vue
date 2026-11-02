<script setup>
import { inject } from 'vue';
import ConversationDirectory from './ConversationDirectory.vue';

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
  creatingKind,
  newItemTitle,
  submitCreateItem,
  modelHub,
  catalog,
  selectConversation,
  selectRemoteConversation,
} = inject('bobAppShell');
</script>

<template>
    <div v-if="navigationOpen" class="navigation-overlay" @keydown.esc="navigationOpen = false">
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
            @click="switchSidebarMode('project')"
          >
            {{ t('groupByProject') }}
          </button>
          <button
            type="button"
            class="sidebar-mode-btn"
            :class="{ active: sidebarGroupMode === 'smart' }"
            @click="switchSidebarMode('smart')"
          >
            {{ t('groupBySmart') }}
          </button>
          <button
            type="button"
            class="sidebar-mode-btn"
            :class="{ active: sidebarGroupMode === 'device' }"
            @click="switchSidebarMode('device')"
          >
            {{ t('groupByDevice') }}
          </button>
        </div>

        <input v-model="sidebarSearch" type="search" :placeholder="t('sidebarSearch')" :aria-label="t('sidebarSearch')" />

        <div class="sidebar-actions">
          <button type="button" @click="startCreateConversation(null)">
            + {{ t('newConversation') }}
          </button>
          <button type="button" @click="startCreateProject">
            + {{ t('newProject') }}
          </button>
          <button type="button" class="sidebar-smart-btn" @click="organizeSessionsViaAgent">
            {{ t('smartGroupActionBtn') }}
          </button>
        </div>

        <form v-if="creatingKind" class="sidebar-create-form" @submit.prevent="submitCreateItem">
          <input
            v-model="newItemTitle"
            type="text"
            :placeholder="
              t(
                creatingKind === 'project'
                  ? 'projectTitlePlaceholder'
                  : 'conversationTitlePlaceholder',
              )
            "
          />
          <div class="sidebar-create-buttons">
            <button type="button" @click="creatingKind = null">{{ t('cancelAction') }}</button>
            <button type="submit">{{ t('create') }}</button>
          </div>
        </form>

        <ConversationDirectory :nodes="modelHub.state.remote_nodes || []" :catalog="catalog" :search="sidebarSearch" :mode="sidebarGroupMode" @select-local="selectConversation" @select-remote="selectRemoteConversation" />
      </nav>
    </div>
</template>
