// Pure IPC mapping; production injects Tauri invoke, tests may inject a recorder.
// A browser/mock store must never impersonate the native SQLite kernel.
export function createKernelBridge(invoke, isAvailable) {
  function call(command, args) {
    if (!isAvailable()) return Promise.reject(new Error('NATIVE_HOST_REQUIRED'));
    return invoke(command, args);
  }
  return {
    available: isAvailable,
    projectLinks: () => call('app_project_links',{}),
    projectLinkSet: args => call('app_project_link_set',args),
    contextTasks: grantId => call('app_context_tasks',{grantId}),
    contextList: () => call('app_context_list',{}),
    contextRead: (grantId,refresh=false) => call(refresh?'app_context_refresh':'app_context_read',{grantId}),
    nativePermission: (connectionId,sessionId,requestId,permissionId,optionId) => call('app_native_permission',{connectionId,sessionId,requestId,permissionId,optionId}),
    nativeCancel: (connectionId,sessionId,requestId) => call('app_native_cancel',{connectionId,sessionId,requestId}),
    nativeRelease: (connectionId,sessionId) => call('app_native_cancel',{connectionId,sessionId,requestId:'__bob_release_holder__'}),
    nativeEvents: (connectionId,sessionId,after=0,epoch='') => call('app_native_events',{connectionId,sessionId,after,epoch}),
    dshReceipt: (connectionId,sessionId,text,requestId) => call('app_dsh_receipt',{connectionId,sessionId,text,requestId}),
    dshPreview: (connectionId,sessionId,path,known=null,messageSequence=null) => call('app_dsh_preview',{connectionId,sessionId,path,known,messageSequence}),
    dshList: connectionId => call('app_dsh_list', {connectionId}),
    dshLive: (connectionId,sessionId) => call('app_dsh_live',{connectionId,sessionId}),
    dshCancel: (connectionId,sessionId) => call('app_dsh_cancel',{connectionId,sessionId}),
    dshCreate: (connectionId,sessionId,workspace,title) => call('app_dsh_create',{connectionId,sessionId,workspace,title}),
    nativeSetup: args => call('app_native_setup',args),
    pairedSessionSetup: (ownerId,command,args={}) => call('app_paired_session_setup',{ownerId,command,args}),
    dshPage: (connectionId,sessionId,before=null,through=null,after=null) => call('app_dsh_page',{connectionId,sessionId,before,through,after}),
    dshHistory: (connectionId,sessionId) => call('app_dsh_history',{connectionId,sessionId}),
    dshPrompt: (connectionId,sessionId,text,requestId,attachments=null,options=null) => call('app_dsh_prompt',{connectionId,sessionId,text,requestId,...(attachments?.length?{attachments}:{}),...(options?.takeover?{takeover:true}:{}),...(options?.approvalMode?{approvalMode:options.approvalMode}:{}),...(options?.workspace?{workspace:options.workspace}:{})}),
    claudeCapabilities: (connectionId=null) => call('app_claude_capabilities',{connectionId}),
    claudeSessions: (connectionId=null) => call('app_claude_sessions',{connectionId}),
    claudeHistory: (connectionId,sessionId) => call('app_claude_history',{connectionId,sessionId}),
    claudePrompt: ({connectionId,sessionId,text,requestId,parentVersionId=null,detachedHoldMs=null}) => call('app_claude_prompt',{connectionId,sessionId,text,requestId,parentVersionId,detachedHoldMs}),
    claudeCancel: (connectionId,sessionId,requestId) => call('app_claude_cancel',{connectionId,sessionId,requestId}),
    claudeReconcile: (connectionId,sessionId) => call('app_claude_reconcile',{connectionId,sessionId}),
    claudeReview: ({connectionId,sessionId,versionId,verdict,note=null}) => call('app_claude_review',{connectionId,sessionId,versionId,verdict,note}),
    claudeArtifacts: (connectionId,sessionId) => call('app_claude_artifacts',{connectionId,sessionId}),
    claudePreview: (connectionId,sessionId,path,versionId=null) => call('app_claude_preview',{connectionId,sessionId,path,versionId}),
    subagentDispatch: (grant, request) => call('app_subagent_dispatch', { grant, request }),
    subagentExecute: (grant, runId, actions) => call('app_subagent_execute', { grant, runId, actions }),
    subagentCancel: (parentTaskId, runId = null, reason = 'EXPLICIT_CANCEL') =>
      call('app_subagent_cancel', { parentTaskId, runId, reason }),
    subagentReconcile: () => call('app_subagent_reconcile', {}),
    subagentStatus: runId => call('app_subagent_status', { runId }),
    subagentList: parentTaskId => call('app_subagent_list', { parentTaskId }),
    scheduleEdit: payload => call('app_schedule_edit', payload),
    bootstrap: () => call('app_bootstrap'),
    syncHome: () => call('app_home_sync'),
    topologySync: payload => call('app_topology_sync', payload),
    pairingCapabilities: () => call('app_pairing_capabilities'),
    pairingQrCreate: args =>
      args !== undefined ? call('app_pairing_qr_create', args) : call('app_pairing_qr_create'),
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
    assistantConversation: () => call('app_assistant_conversation'),
    assistantExecute: requestId => call('app_assistant_execute', {requestId}),
    assistantPoll: sessionId => call('app_assistant_poll', {sessionId}),
    catalog: () => call('app_catalog'),
    createProject: (projectId, title) => call('app_project_create', { projectId, title }),
    createConversation: (conversationId, projectId, title) =>
      call('app_conversation_create', {
        conversationId,
        projectId: projectId ?? null,
        title: title ?? '',
      }),
    promoteConversation: (conversationIdOrObj, projectId, title) => {
      if (conversationIdOrObj && typeof conversationIdOrObj === 'object') {
        return call('app_conversation_promote', {
          conversationId: conversationIdOrObj.conversationId,
          projectId: conversationIdOrObj.projectId,
          title: conversationIdOrObj.title ?? '',
        });
      }
      return call('app_conversation_promote', {
        conversationId: conversationIdOrObj,
        projectId,
        title: title ?? '',
      });
    },
    deleteConversation: conversationId =>
      call('app_conversation_delete', { conversationId }),
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
    quotaCheckpointSync: () => call('app_quota_checkpoint_sync'),
    modelPresetsList: () => call('app_model_presets_list', {}),
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
    remoteNodeSave: ({ id, label, baseUrl, token, clearToken, selectedProjectId, enabled, deviceId }) =>
      call('app_remote_node_save', {
        id: id ?? null,
        label: label ?? null,
        baseUrl,
        token: token ?? null,
        clearToken: clearToken ?? false,
        selectedProjectId: selectedProjectId ?? null,
        ...(deviceId ? { deviceId } : {}),
        enabled: enabled ?? true,
      }),
    remoteNodeDelete: id => call('app_remote_node_delete', { id }),
    onlineBeginPairing: ({ origin, clientInstanceId, clientName }) =>
      call('app_online_begin_pairing', {
        origin,
        clientInstanceId,
        clientName: clientName ?? null,
      }),
    onlineExchangePairing: ({ origin, pairingId, clientInstanceId, verifier }) =>
      call('app_online_exchange_pairing', {
        origin,
        pairingId,
        clientInstanceId,
        verifier: verifier ?? null,
      }),
    onlineSync: ({ origin, projectId, after }) =>
      call('app_online_sync', {
        origin,
        projectId: projectId ?? null,
        after: after ?? null,
      }),
    onlineSubmitOperation: ({
      origin,
      operationKind,
      projectId,
      taskId,
      prompt,
      parentVersionId,
      requestId,
    }) =>
      call('app_online_submit_operation', {
        origin,
        operationKind,
        projectId,
        taskId: taskId ?? null,
        prompt: prompt ?? null,
        parentVersionId: parentVersionId ?? null,
        requestId: requestId ?? null,
      }),
    onlineRevoke: ({ origin, localOnly }) =>
      call('app_online_revoke', {
        origin,
        localOnly: localOnly ?? false,
      }),
    registrySave: registry => call('app_registry_save', { registry: registry ?? null }),
    modelVisibilitySet: (providerId, modelId, visible) =>
      call('app_model_visibility_set', { providerId, modelId, visible: Boolean(visible) }),
    modelsDiscover: providerId =>
      call('app_models_discover', { providerId: providerId ?? null }),
    personalSnapshot: () => call('app_personal_snapshot'),
    scheduleCreate: ({
      id,
      projectId,
      taskId,
      title,
      kind,
      startAt,
      dueAtMs,
      timing,
      endAt,
      reminderAt,
      notes,
    }) =>
      call('app_schedule_create', {
        id,
        projectId: projectId ?? null,
        taskId: taskId ?? null,
        title,
        kind: kind ?? 'event',
        startAt: startAt ?? null,
        endAt: endAt ?? dueAtMs ?? null,
        reminderAt: reminderAt ?? null,
        notes: notes ?? '',
        timing: timing ?? {},
      }),
    scheduleStatus: (id, status) => call('app_schedule_status', { id, status }),
    scheduleDelete: id => call('app_schedule_delete', { id }),
    noteUpsert: ({ id, projectId, conversationId, title, bodyMarkdown, body, tags }) =>
      call('app_note_upsert', {
        id,
        projectId: projectId ?? null,
        conversationId: conversationId ?? null,
        title,
        bodyMarkdown: bodyMarkdown ?? body ?? '',
        tags: tags ?? [],
      }),
    noteDelete: id => call('app_note_delete', { id }),
    ticketUpsert: ({ id, title, category, ticketMeta, startAt, endAt, notesSummary }) =>
      call('app_ticket_upsert', {
        id,
        title,
        category: category ?? 'general',
        ticketMeta: ticketMeta ?? {},
        startAt: startAt ?? null,
        endAt: endAt ?? null,
        notesSummary: notesSummary ?? '',
      }),
    captureMediaPut: (mime, base64) => call('app_capture_media_put', { mime, base64 }),
    captureMediaGet: (mediaId, captureId) => call('app_capture_media_get', { mediaId: mediaId || null, captureId: captureId || null }),
    captureVisionTarget: () => call('app_capture_vision_target'),
    captureRecognize: (mediaId, modelRef) => call('app_capture_recognize', { mediaId, modelRef }),
    ticketRecognize: (mediaId, modelRef = null, id = null) =>
      call('app_ticket_recognize', { mediaId, modelRef: modelRef ?? null, id: id ?? null }),
    captureCreate: ({ id, sourceMessageId, messageId, contentRef, content, projectId, newProjectTitle, mediaId }) =>
      call('app_capture_create', {
        id,
        sourceMessageId: sourceMessageId ?? messageId ?? null,
        contentRef: contentRef ?? content ?? '',
        projectId: projectId ?? null,
        newProjectTitle: newProjectTitle ?? null,
        mediaId: mediaId ?? null,
      }),
    inboxList: (status = null) => call('app_inbox_list', { status: status || null }),
    inboxCapture: ({ id, content, mediaId = null }) =>
      call('app_inbox_capture', { id, content: content ?? '', mediaId: mediaId || null }),
    inboxTriageSet: ({ captureId, destination = null, batchId = null, expectedVersion }) =>
      call('app_inbox_triage_set', {
        captureId,
        destination: destination || null,
        batchId: batchId || null,
        expectedVersion,
      }),
    inboxTriageDraft: ({ regenerate = false } = {}) =>
      call('app_inbox_triage_draft', { regenerate: Boolean(regenerate) }),
    inboxTriageClose: ({ batchId, status }) =>
      call('app_inbox_triage_close', { batchId, status }),
    memoryState: (id, newState) => call('app_memory_state', { id, newState }),
    decisionRecord: ({
      id,
      projectId,
      taskId,
      sourceMessageId,
      body,
      rationaleJson,
      status,
      supersedesId,
    }) =>
      call('app_decision_record', {
        id,
        projectId: projectId ?? null,
        taskId: taskId ?? null,
        sourceMessageId: sourceMessageId ?? null,
        body,
        rationaleJson: rationaleJson ?? '{}',
        status: status ?? 'proposed',
        supersedesId: supersedesId ?? null,
      }),
    decisionStatus: (id, status) => call('app_decision_status', { id, status }),
    execute: requestId => call('app_execute', { requestId }),
    executions: sessionId => call('app_executions', { sessionId }),
    cancel: executionId => call('app_cancel', { executionId }),
    jarvisChat: ({ sessionId, requestId, messageId = null, content = null }) =>
      call('app_jarvis_chat', { sessionId, requestId, messageId, content }),
    jarvisWorkerTasks: (sessionId = null, afterSeq = 0) =>
      call('app_jarvis_worker_tasks', { sessionId, afterSeq }),
    jarvisWorkerCancel: taskId => call('app_jarvis_worker_cancel', { taskId }),
    jarvisApprovalsList: (sessionId = null) => call('app_jarvis_approvals_list', { sessionId }),
    jarvisApprove: approvalId => call('app_jarvis_approve', { approvalId }),
    jarvisReject: approvalId => call('app_jarvis_reject', { approvalId }),
    jarvisBudgetGet: () => call('app_jarvis_budget_get', {}),
    jarvisBudgetSet: monthlyCapUsd => call('app_jarvis_budget_set', { monthlyCapUsd }),
    featuresList: () => call('app_features_list', {}),
    featureKeep: name => call('app_feature_keep', { name }),
    featureRollback: (name, targetVersion = null) =>
      call('app_feature_rollback', { name, targetVersion }),
    featureToggle: (name, enabled) => call('app_feature_toggle', { name, enabled }),
    featureCall: (name, route, payload = {}) =>
      call('app_feature_call', { name, route, payload }),
    featureSaveStorage: (name, storage) =>
      call('app_feature_storage_save', { name, storage }),
    featureReportError: (name, error) => call('app_feature_report_error', { name, error }),
    browserSessionsList: () => call('app_browser_sessions_list', {}),
    browserSessionOpen: target => call('app_browser_session_open', { target }),
    browserSessionClear: domain => call('app_browser_session_clear', { domain }),
    browserTakeoverComplete: takeoverId => call('app_browser_takeover_complete', { takeoverId }),
    proactivityGet: () => call('app_proactivity_get', {}),
    proactivitySet: (args = {}) => {
      const { level, quietHoursStart = '22:00', quietHoursEnd = '08:00' } =
        typeof args === 'number' ? { level: args } : args;
      return call('app_proactivity_set', { level, quietHoursStart, quietHoursEnd });
    },
    pushVapidPublicKey: () => call('app_push_vapid_public_key', {}),
    pushSubscribe: subscription => call('app_push_subscribe', { subscription }),
  };
}
