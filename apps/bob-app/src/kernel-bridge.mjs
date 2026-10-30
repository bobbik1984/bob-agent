// Pure IPC mapping; production injects Tauri invoke, tests may inject a recorder.
// A browser/mock store must never impersonate the native SQLite kernel.
export function createKernelBridge(invoke, isAvailable) {
  function call(command, args) {
    if (!isAvailable()) return Promise.reject(new Error('NATIVE_HOST_REQUIRED'));
    return invoke(command, args);
  }
  return {
    available: isAvailable,
    bootstrap: () => call('app_bootstrap'),
    syncHome: () => call('app_home_sync'),
    pairingCapabilities: () => call('app_pairing_capabilities'),
    pairingQrCreate: () => call('app_pairing_qr_create'),
    pairingQrPropose: invitationCode => call('app_pairing_qr_propose', { invitationCode }),
    scanPairingCode: () => call('app_scan_pairing_code'),
    pairingDeviceCode: () => call('app_pairing_device_code'),
    pairingReset: () => call('app_pairing_reset'),
    pairingRecover: () => call('app_pairing_recover'),
    pairingStatus: () => call('app_pairing_status'),
    pairingHomeInvite: (deviceCode, bindAddress, advertisedHost) =>
      call('app_pairing_home_invite', { deviceCode, bindAddress, advertisedHost }),
    pairingPropose: invitationCode => call('app_pairing_propose', { invitationCode }),
    pairingRevoke: () => call('app_pairing_revoke'),
    pairingHomeConfirm: () => call('app_pairing_home_confirm'),
    pairingFinish: () => call('app_pairing_finish'),
    pairingEnableHome: () => call('app_pairing_enable_home'),
    readiness: () => call('app_readiness'),
    history: sessionId => call('app_history', { sessionId }),
    acceptMessage: request => {
      const payload = {
        requestId: request.request_id,
        messageId: request.message_id,
        sessionId: request.session_id,
        content: request.content,
      };
      if (request.expected_draft_revision !== undefined) {
        payload.expectedDraftRevision = request.expected_draft_revision;
      }
      return call('app_accept', payload);
    },
    prewarm: () => call('app_prewarm'),
    catalog: () => call('app_catalog'),
    createProject: (projectId, title) => call('app_project_create', { projectId, title }),
    createConversation: (conversationId, projectId, title) =>
      call('app_conversation_create', {
        conversationId,
        projectId: projectId ?? null,
        title: title ?? '',
      }),
    selectConversation: conversationId =>
      call('app_conversation_select', { conversationId }),
    saveViewState: ({
      conversationId,
      draft,
      scrollAnchorMessageId,
      scrollOffset,
      markRead,
    }) =>
      call('app_view_state_save', {
        conversationId,
        draft: draft ?? null,
        scrollAnchorMessageId:
          scrollAnchorMessageId === undefined ? null : scrollAnchorMessageId,
        scrollOffset: scrollOffset ?? null,
        markRead: markRead ?? false,
      }),
    savePreferences: ({ sidebarGroupMode, locale, theme }) =>
      call('app_preferences_save', {
        sidebarGroupMode: sidebarGroupMode ?? null,
        locale: locale ?? null,
        theme: theme ?? null,
      }),
    providerGet: () => call('app_provider_get'),
    providerSave: ({
      providerKind,
      label,
      baseUrl,
      modelId,
      apiKey,
      clearKey,
      enabled,
    }) =>
      call('app_provider_save', {
        providerKind,
        label,
        baseUrl,
        modelId,
        apiKey: apiKey ?? null,
        clearKey: clearKey ?? false,
        enabled: enabled ?? true,
      }),
    providerTest: () => call('app_provider_test'),
    routeGet: sessionId => call('app_route_get', { sessionId }),
    routeSet: (sessionId, route) => call('app_route_set', { sessionId, route }),
    modelHubGet: () => call('app_model_hub_get'),
    providerKeySet: (providerIdOrObj, apiKey, clearKey) => {
      if (typeof providerIdOrObj === 'object' && providerIdOrObj !== null) {
        return call('app_provider_key_set', {
          providerId: providerIdOrObj.providerId,
          apiKey: providerIdOrObj.apiKey ?? null,
          clearKey: providerIdOrObj.clearKey ?? false,
        });
      }
      return call('app_provider_key_set', {
        providerId: providerIdOrObj,
        apiKey: apiKey ?? null,
        clearKey: clearKey ?? false,
      });
    },
    modelRoleAssign: (roleOrObj, modelEntryId) => {
      if (typeof roleOrObj === 'object' && roleOrObj !== null) {
        const entryId =
          roleOrObj.modelEntryId ||
          `${roleOrObj.providerId}::${roleOrObj.modelId}`;
        return call('app_model_role_assign', {
          role: roleOrObj.role,
          modelEntryId: entryId,
        });
      }
      return call('app_model_role_assign', { role: roleOrObj, modelEntryId });
    },
    providerVariantSet: (providerId, variant) =>
      call('app_provider_variant_set', { providerId, variant }),
    customModelSave: ({ name, modelId, baseUrl, apiKey }) =>
      call('app_custom_model_save', {
        name,
        modelId: modelId ?? '',
        baseUrl,
        apiKey: apiKey ?? '',
      }),
    customModelDelete: id => call('app_custom_model_delete', { id }),
    registrySave: registry => call('app_registry_save', { registry: registry ?? null }),
    personalSnapshot: () => call('app_personal_snapshot'),
    scheduleCreate: ({
      id,
      projectId,
      taskId,
      title,
      startAt,
      dueAtMs,
      endAt,
      reminderAt,
      notes,
    }) =>
      call('app_schedule_create', {
        id,
        projectId: projectId ?? null,
        taskId: taskId ?? null,
        title,
        startAt: startAt ?? dueAtMs ?? Math.floor(Date.now() / 1000),
        endAt: endAt ?? null,
        reminderAt: reminderAt ?? null,
        notes: notes ?? '',
      }),
    scheduleStatus: (id, status) => call('app_schedule_status', { id, status }),
    noteUpsert: ({ id, projectId, conversationId, title, bodyMarkdown, body, tags }) =>
      call('app_note_upsert', {
        id,
        projectId: projectId ?? null,
        conversationId: conversationId ?? null,
        title,
        bodyMarkdown: bodyMarkdown ?? body ?? '',
        tags: tags ?? [],
      }),
    captureCreate: ({ id, sourceMessageId, messageId, contentRef, content }) =>
      call('app_capture_create', {
        id,
        sourceMessageId: sourceMessageId ?? messageId ?? null,
        contentRef: contentRef ?? content ?? '',
      }),
    memoryState: (id, newState) => call('app_memory_state', { id, newState }),
    execute: requestId => call('app_execute', { requestId }),
    executions: sessionId => call('app_executions', { sessionId }),
    cancel: executionId => call('app_cancel', { executionId }),
  };
}
