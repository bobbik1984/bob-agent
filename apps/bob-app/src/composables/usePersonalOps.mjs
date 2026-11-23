import { ref } from 'vue';

export function usePersonalOps(ctx) {
  const {
    t,
    available,
    kernelBridge,
    clientState,
    saveClientState,
    sessionId,
    catalog,
    messages,
    locallyAccepted,
    draft,
    controller,
    activeDrawer,
    settingsOpen,
    navigationOpen,
    newConversationPending,
    currentConversation,
    activeConversationItem,
    handleWorkOpenConversation,
    flushCurrentViewState,
    applyCatalogPreferences,
    reload,
  } = ctx;

const personalOps = ref({
  schedules: [],
  notes: [],
  captures: [],
  memories: [],
  decisions: [],
  pending_sync_count: 0,
});
const personalBusy = ref(false);
const personalFault = ref(null);

async function loadPersonalOps() {
  if (!available || !kernelBridge.personalSnapshot) return;
  const snap = await kernelBridge.personalSnapshot(null).catch(() => null);
  if (snap) {
    personalOps.value = {
      schedules: Array.isArray(snap.schedules) ? snap.schedules : [],
      notes: Array.isArray(snap.notes) ? snap.notes : [],
      captures: Array.isArray(snap.captures) ? snap.captures : [],
      memories: Array.isArray(snap.memories) ? snap.memories : [],
      decisions: Array.isArray(snap.decisions) ? snap.decisions : [],
      pending_sync_count: Number(snap.pending_sync_count) || 0,
    };
  }
}

async function saveCalendarItem(payload) {
  if (!available) throw new Error('NATIVE_HOST_REQUIRED');
  if (payload.expectedVersion != null) await kernelBridge.scheduleEdit(payload);
  else await kernelBridge.scheduleCreate(payload);
  await loadPersonalOps();
}

async function handleCreateSchedule(payload) {
  if (personalBusy.value) return;
  personalBusy.value = true;
  personalFault.value = null;
  try {
    if (available && kernelBridge.scheduleCreate) {
      await kernelBridge.scheduleCreate({
        id: payload.id,
        projectId: payload.projectId ?? null,
        taskId: payload.taskId ?? null,
        title: payload.title,
        kind: payload.kind ?? 'event',
        startAt: payload.startAt ?? null,
        endAt: payload.endAt ?? payload.dueAtMs ?? null,
        reminderAt: payload.reminderAt ?? null,
        notes: payload.notes,
      });
      await loadPersonalOps();
    } else {
      personalOps.value.schedules.unshift({
        id: payload.id,
        project_id: payload.projectId ?? null,
        task_id: payload.taskId ?? null,
        title: payload.title,
        kind: payload.kind ?? 'event',
        start_at: payload.startAt ?? null,
        end_at: payload.endAt ?? payload.dueAtMs ?? null,
        status: 'scheduled',
        notes: payload.notes,
      });
    }
  } catch (err) {
    personalFault.value = String(err?.message || err || 'SCHEDULE_CREATE_FAILED');
  } finally {
    personalBusy.value = false;
  }
}

async function handleCreateProjectTodo({ projectId, title }) {
  if (!title) return;
  await handleCreateSchedule({
    id: `sched-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
    projectId: projectId || null,
    title,
    kind: 'todo',
    startAt: null,
    notes: '',
  });
}

async function handleStartTodoSession({ projectId, todo }) {
  if (!todo) return;
  const targetProjectId = projectId || todo.project_id;
  if (!targetProjectId) {
    activeDrawer.value = 'work';
    return;
  }
  const existingConv = (catalog.value?.conversations || []).find(
    c => c.project_id === targetProjectId,
  );
  const promptText = todo.notes ? `${todo.title}\n${todo.notes}` : todo.title;
  if (existingConv) {
    await handleWorkOpenConversation(existingConv.id);
    draft.value = promptText;
    controller.loadDraft(promptText, 0);
  } else if (available && kernelBridge.createConversation) {
    await flushCurrentViewState();
    const cid = `conv-${crypto.randomUUID().slice(0, 8)}`;
    const snap = await kernelBridge
      .createConversation(cid, targetProjectId, todo.title.slice(0, 48))
      .catch(() => null);
    if (snap) {
      applyCatalogPreferences(snap);
      locallyAccepted.clear();
      sessionId.value = cid;
      newConversationPending.value = false;
      clientState.selectedConversation = cid;
      saveClientState(clientState);
      draft.value = promptText;
      controller.loadDraft(promptText, 0);
      activeDrawer.value = 'chat';
      settingsOpen.value = false;
      navigationOpen.value = false;
      await reload();
    }
  } else {
    activeDrawer.value = 'chat';
    draft.value = promptText;
    controller.loadDraft(promptText, 0);
  }
}

async function handleOpenProjectTodo(todo) {
  if (!todo) return;
  await handleStartTodoSession({ projectId: todo.project_id, todo });
}

async function handlePromoteTodoToProject(todo) {
  if (!todo || !todo.title) return;
  const pid = `proj-${crypto.randomUUID().slice(0, 8)}`;
  if (available && kernelBridge.createProject) {
    const snap = await kernelBridge.createProject(pid, todo.title).catch(() => null);
    if (snap) applyCatalogPreferences(snap);
  } else if (catalog.value?.projects) {
    catalog.value.projects.push({ id: pid, title: todo.title });
  }
  todo.project_id = pid;
  await handleStartTodoSession({ projectId: pid, todo });
}

async function handleUpdateScheduleStatus(payload) {
  if (personalBusy.value) return;
  personalBusy.value = true;
  personalFault.value = null;
  try {
    if (available && kernelBridge.scheduleStatus) {
      await kernelBridge.scheduleStatus(payload.id, payload.status);
      await loadPersonalOps();
    } else {
      const target = personalOps.value.schedules.find(s => s.id === payload.id);
      if (target) target.status = payload.status;
    }
  } catch (err) {
    personalFault.value = String(err?.message || err || 'SCHEDULE_UPDATE_FAILED');
  } finally {
    personalBusy.value = false;
  }
}

async function handleSaveNote(payload) {
  if (personalBusy.value) return;
  personalBusy.value = true;
  personalFault.value = null;
  try {
    if (available && kernelBridge.noteUpsert) {
      await kernelBridge.noteUpsert({
        id: payload.id,
        title: payload.title,
        body: payload.body,
        tags: payload.tags,
      });
      await loadPersonalOps();
    } else {
      const idx = personalOps.value.notes.findIndex(n => n.id === payload.id);
      if (idx >= 0) personalOps.value.notes[idx] = { ...payload };
      else personalOps.value.notes.unshift({ ...payload });
    }
  } catch (err) {
    personalFault.value = String(err?.message || err || 'NOTE_SAVE_FAILED');
  } finally {
    personalBusy.value = false;
  }
}

const recognizingTicket = ref(false);
const composerTicketFileRef = ref(null);

async function handleDeleteSchedule(id) {
  if (!id || personalBusy.value) return;
  personalBusy.value = true;
  personalFault.value = null;
  try {
    if (available && kernelBridge.scheduleDelete) {
      await kernelBridge.scheduleDelete(id);
      await loadPersonalOps();
    } else {
      personalOps.value.schedules = personalOps.value.schedules.filter(s => s.id !== id);
    }
  } catch (err) {
    personalFault.value = String(err?.message || err || 'SCHEDULE_DELETE_FAILED');
  } finally {
    personalBusy.value = false;
  }
}

async function handleDeleteNote(id) {
  if (!id || personalBusy.value) return;
  personalBusy.value = true;
  personalFault.value = null;
  try {
    if (available && kernelBridge.noteDelete) {
      await kernelBridge.noteDelete(id);
      await loadPersonalOps();
    } else {
      personalOps.value.notes = personalOps.value.notes.filter(n => n.id !== id);
    }
  } catch (err) {
    personalFault.value = String(err?.message || err || 'NOTE_DELETE_FAILED');
  } finally {
    personalBusy.value = false;
  }
}

async function handleSaveTicket(payload) {
  if (!payload?.title || personalBusy.value) return;
  personalBusy.value = true;
  personalFault.value = null;
  try {
    if (available && kernelBridge.ticketUpsert) {
      await kernelBridge.ticketUpsert(payload);
      await loadPersonalOps();
    } else {
      const cat = payload.category || 'general';
      const note = {
        id: payload.id || `ticket-${Date.now()}`,
        title: payload.title,
        tags: ['ticket', `ticket:${cat}`],
        body_markdown: JSON.stringify({
          kind: 'ticket',
          category: cat,
          title: payload.title,
          start_at_ms: payload.startAt || null,
          end_at_ms: payload.endAt || null,
          ticket: payload.ticketMeta || {},
        }),
        updated_at: Date.now(),
      };
      const idx = personalOps.value.notes.findIndex(n => n.id === note.id);
      if (idx >= 0) personalOps.value.notes[idx] = note;
      else personalOps.value.notes.unshift(note);
    }
  } catch (err) {
    personalFault.value = String(err?.message || err || 'TICKET_SAVE_FAILED');
  } finally {
    personalBusy.value = false;
  }
}

async function handleDeleteTicket(ticketItem) {
  if (!ticketItem?.id || personalBusy.value) return;
  personalBusy.value = true;
  personalFault.value = null;
  try {
    if (available && kernelBridge.noteDelete) {
      await kernelBridge.noteDelete(ticketItem.id);
      if (kernelBridge.scheduleDelete) {
        await kernelBridge.scheduleDelete(`sched-${ticketItem.id}`).catch(() => {});
      }
      await loadPersonalOps();
    } else {
      personalOps.value.notes = personalOps.value.notes.filter(n => n.id !== ticketItem.id);
      personalOps.value.schedules = personalOps.value.schedules.filter(
        s => s.id !== `sched-${ticketItem.id}`,
      );
    }
  } catch (err) {
    personalFault.value = String(err?.message || err || 'TICKET_DELETE_FAILED');
  } finally {
    personalBusy.value = false;
  }
}

async function handleRecognizeTicketFile(file) {
  if (!file || recognizingTicket.value) return;
  if (!['image/png', 'image/jpeg', 'image/webp'].includes(file.type) || file.size > 8 * 1024 * 1024) {
    personalFault.value = t('captureImageInvalid');
    return;
  }
  recognizingTicket.value = true;
  personalFault.value = null;
  try {
    const data = await new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result);
      reader.onerror = reject;
      reader.readAsDataURL(file);
    });
    const stored = await kernelBridge.captureMediaPut(file.type, data.split(',')[1]);
    let modelRef = null;
    try {
      const target = await kernelBridge.captureVisionTarget();
      modelRef = target?.model_ref || null;
    } catch {
      modelRef = null;
    }
    const result = await kernelBridge.ticketRecognize(stored.id, modelRef);
    await loadPersonalOps();
    if (activeDrawer.value === 'chat' && result?.text) {
      messages.value.push({
        id: `msg-ticket-${Date.now()}`,
        role: 'assistant',
        source: 'direct_provider',
        content: result.text,
        send_state: 'reply_saved',
      });
    }
  } catch {
    personalFault.value = t('captureRecognitionFailed');
  } finally {
    recognizingTicket.value = false;
  }
}

function handleComposerTicketFileChange(event) {
  const file = event.target?.files?.[0];
  if (event.target) event.target.value = '';
  if (!file) return;
  void handleRecognizeTicketFile(file);
}

async function handleCreateCapture(payload) {
  if (personalBusy.value) return;
  personalBusy.value = true;
  personalFault.value = null;
  try {
    if (available && kernelBridge.captureCreate) {
      await kernelBridge.captureCreate({
        id: payload.id,
        conversationId: sessionId.value,
        messageId: null,
        content: payload.content,
      });
      await loadPersonalOps();
    } else {
      personalOps.value.captures.unshift({
        id: payload.id,
        content_ref: payload.content,
      });
    }
  } catch (err) {
    personalFault.value = String(err?.message || err || 'CAPTURE_SAVE_FAILED');
  } finally {
    personalBusy.value = false;
  }
}

async function handleUpdateMemory(payload) {
  if (personalBusy.value) return;
  personalBusy.value = true;
  personalFault.value = null;
  try {
    if (available && kernelBridge.memoryState) {
      await kernelBridge.memoryState(payload.id, payload.state);
      await loadPersonalOps();
    } else {
      const target = personalOps.value.memories.find(m => m.id === payload.id);
      if (target) target.state = payload.state;
    }
  } catch (err) {
    personalFault.value = String(err?.message || err || 'MEMORY_UPDATE_FAILED');
  } finally {
    personalBusy.value = false;
  }
}

async function handleCreateDecision(payload) {
  if (personalBusy.value) return;
  personalBusy.value = true;
  personalFault.value = null;
  try {
    if (available && kernelBridge.decisionRecord) {
      await kernelBridge.decisionRecord({
        id: payload.id,
        body: payload.body,
        status: payload.status ?? 'accepted',
      });
      await loadPersonalOps();
    } else {
      personalOps.value.decisions.unshift({
        id: payload.id,
        project_id: 'proj-local',
        body: payload.body,
        status: payload.status ?? 'accepted',
      });
    }
  } catch (err) {
    personalFault.value = String(err?.message || err || 'DECISION_SAVE_FAILED');
  } finally {
    personalBusy.value = false;
  }
}

async function handleUpdateDecision(payload) {
  if (personalBusy.value) return;
  personalBusy.value = true;
  personalFault.value = null;
  try {
    if (available && kernelBridge.decisionStatus) {
      await kernelBridge.decisionStatus(payload.id, payload.status);
      await loadPersonalOps();
    } else {
      const target = personalOps.value.decisions.find(d => d.id === payload.id);
      if (target) target.status = payload.status;
    }
  } catch (err) {
    personalFault.value = String(err?.message || err || 'DECISION_UPDATE_FAILED');
  } finally {
    personalBusy.value = false;
  }
}

async function promoteConversationToWork() {
  if (!available) {
    activeDrawer.value = 'work';
    return;
  }
  personalFault.value = null;
  try {
    await flushCurrentViewState();
    const title = currentConversation.value || t('newProject');
    const existingPid = activeConversationItem.value?.project_id;
    const pid = existingPid || `proj-${crypto.randomUUID().slice(0, 8)}`;
    if (kernelBridge.promoteConversation && sessionId.value) {
      const snap = await kernelBridge.promoteConversation(sessionId.value, pid, title);
      if (snap) applyCatalogPreferences(snap);
    } else if (kernelBridge.createProject) {
      const snap = await kernelBridge.createProject(pid, title);
      if (snap) applyCatalogPreferences(snap);
    }
    await reload();
    activeDrawer.value = 'work';
  } catch (err) {
    personalFault.value = String(err?.message || err || 'PROJECT_CREATE_FAILED');
  }
}

  return Object.assign(ctx, {
    personalOps,
    personalBusy,
    personalFault,
    recognizingTicket,
    composerTicketFileRef,
    loadPersonalOps,
    saveCalendarItem,
    handleCreateSchedule,
    handleCreateProjectTodo,
    handleStartTodoSession,
    handleOpenProjectTodo,
    handlePromoteTodoToProject,
    handleUpdateScheduleStatus,
    handleSaveNote,
    handleDeleteSchedule,
    handleDeleteNote,
    handleSaveTicket,
    handleDeleteTicket,
    handleRecognizeTicketFile,
    handleComposerTicketFileChange,
    handleCreateCapture,
    handleUpdateMemory,
    handleCreateDecision,
    handleUpdateDecision,
    promoteConversationToWork,
  });
}
