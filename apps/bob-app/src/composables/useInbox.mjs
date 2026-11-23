import { ref, computed } from 'vue';
import { kernelBridge as defaultBridge } from '../native-bridge.mjs';

export function useInbox(ctx = {}) {
  const kernelBridge = ctx.kernelBridge || defaultBridge;
  const available = ctx.available !== undefined ? ctx.available : (kernelBridge?.available ? kernelBridge.available() : true);
  const t = ctx.t || (k => k);

  const inboxItems = ref([]);
  const inboxBusy = ref(false);
  const inboxFilter = ref('all'); // 'all' | 'unsorted' | 'later' | 'sorted'

  const triageDraft = ref(null);
  const triageDraftBusy = ref(false);
  const triageDraftError = ref(null);
  const triageStatusNotice = ref('');

  const captureInput = ref('');
  const captureMediaId = ref(null);
  const captureBusy = ref(false);
  const checkingImplemented = ref(false);

  const unsortedCount = computed(() => {
    return inboxItems.value.filter(
      item => item.triage_status === 'unsorted' || !item.triage_status,
    ).length;
  });

  const sortedCount = computed(() => {
    return inboxItems.value.filter(
      item => item.triage_status === 'sorted' && item.destination_kind !== 'keep',
    ).length;
  });

  const laterCount = computed(() => {
    return inboxItems.value.filter(
      item => item.triage_status === 'sorted' && item.destination_kind === 'keep',
    ).length;
  });

  const filteredItems = computed(() => {
    const list = [...inboxItems.value].sort(
      (a, b) => (b.created_at || 0) - (a.created_at || 0),
    );
    if (inboxFilter.value === 'unsorted') {
      return list.filter(
        item => item.triage_status === 'unsorted' || !item.triage_status,
      );
    }
    if (inboxFilter.value === 'later') {
      return list.filter(
        item => item.triage_status === 'sorted' && item.destination_kind === 'keep',
      );
    }
    if (inboxFilter.value === 'sorted') {
      return list.filter(
        item => item.triage_status === 'sorted' && item.destination_kind !== 'keep',
      );
    }
    return list;
  });

  const sortedProjectGroups = computed(() => {
    const sorted = filteredItems.value.filter(
      item => item.triage_status === 'sorted' && item.destination_kind !== 'keep',
    );
    const groups = new Map();
    const otherItems = [];
    for (const item of sorted) {
      if (item.destination_kind === 'project_todo') {
        const key = item.destination_ref_id || item.destination_label || '__project__';
        const title = item.destination_label || item.destination_ref_id || key;
        if (!groups.has(key)) {
          groups.set(key, {
            key,
            projectId: item.destination_ref_id || null,
            title,
            isOther: false,
            items: [],
            implementedCount: 0,
            pendingCount: 0,
          });
        }
        const g = groups.get(key);
        g.items.push(item);
        if (item.resolution_status === 'implemented') g.implementedCount += 1;
        else if (item.resolution_status !== 'wont_do') g.pendingCount += 1;
      } else {
        otherItems.push(item);
      }
    }
    const list = [...groups.values()].sort((a, b) => b.items.length - a.items.length || a.title.localeCompare(b.title));
    if (otherItems.length) {
      list.push({
        key: '__other__',
        projectId: null,
        title: t('inboxOtherDest'),
        isOther: true,
        items: otherItems,
        implementedCount: otherItems.filter(i => i.resolution_status === 'implemented').length,
        pendingCount: otherItems.filter(i => i.resolution_status !== 'implemented' && i.resolution_status !== 'wont_do').length,
      });
    }
    return list;
  });

  let inboxRead = 0;
  async function loadInbox() {
    if (!available || !kernelBridge.inboxList) return;
    const read = ++inboxRead;
    inboxBusy.value = true;
    try {
      const list = await kernelBridge.inboxList(null);
      if (read !== inboxRead) return;
      inboxItems.value = (Array.isArray(list) ? list : []).map(item => ({
        ...item,
        content: item.content || item.content_ref || item.contentRef || '',
      }));
    } catch (err) {
      console.error('Failed to load inbox:', err);
    } finally {
      if (read === inboxRead) inboxBusy.value = false;
    }
  }

  async function loadTriageDraft(regenerate = false) {
    if (!available || !kernelBridge.inboxTriageDraft) return;
    triageDraftBusy.value = true;
    triageDraftError.value = null;
    try {
      const res = await kernelBridge.inboxTriageDraft({ regenerate });
      if (res && res.status !== 'empty' && res.batch?.suggestions?.length) {
        triageDraft.value = res;
      } else {
        triageDraft.value = null;
        if (regenerate) {
          triageStatusNotice.value = t('inboxTriageEmptyNotice');
        }
      }
    } catch (err) {
      triageDraftError.value = err?.message || 'TRIAGE_DRAFT_FAILED';
    } finally {
      triageDraftBusy.value = false;
    }
  }

  async function dismissTriageDraft() {
    if (!triageDraft.value?.id || !kernelBridge.inboxTriageClose) return;
    try {
      await kernelBridge.inboxTriageClose({
        batchId: triageDraft.value.id,
        status: 'dismissed',
      });
      triageDraft.value = null;
    } catch (err) {
      console.error('Failed to dismiss triage draft:', err);
    }
  }

  async function applyTriageSuggestion(suggestionIndex, chosenDestination = null) {
    if (!triageDraft.value?.batch?.suggestions) return;
    const sugg = triageDraft.value.batch.suggestions[suggestionIndex];
    if (!sugg) return;

    const dest = chosenDestination || {
      kind: sugg.destination,
      refId:
        sugg.destination === 'project_todo'
          ? sugg.project_id
          : sugg.destination === 'calendar'
            ? null
            : null,
      label:
        sugg.destination === 'project_todo'
          ? sugg.project_title || sugg.project_id
          : sugg.title || sugg.destination,
    };

    triageDraftBusy.value = true;
    try {
      for (const sparkId of sugg.spark_ids || []) {
        const item = inboxItems.value.find(x => x.id === sparkId);
        const expectedVersion = item ? item.triage_version || 0 : 0;
        await kernelBridge.inboxTriageSet({
          captureId: sparkId,
          destination: dest,
          batchId: triageDraft.value.id,
          expectedVersion,
        });
      }
      sugg._applied = true;
      sugg._appliedLabel = dest.label || dest.kind;
      await loadInbox();

      const allDone = triageDraft.value.batch.suggestions.every(s => s._applied);
      if (allDone) {
        await kernelBridge.inboxTriageClose({
          batchId: triageDraft.value.id,
          status: 'applied',
        });
        triageDraft.value = null;
      }
    } catch (err) {
      console.error('Failed to apply triage suggestion:', err);
    } finally {
      triageDraftBusy.value = false;
    }
  }

  async function applyAllTriageSuggestions() {
    if (!triageDraft.value?.batch?.suggestions) return;
    triageDraftBusy.value = true;
    try {
      const suggestions = triageDraft.value.batch.suggestions;
      for (let i = 0; i < suggestions.length; i++) {
        if (!suggestions[i]._applied) {
          await applyTriageSuggestion(i);
        }
      }
      await kernelBridge.inboxTriageClose({
        batchId: triageDraft.value.id,
        status: 'applied',
      });
      triageDraft.value = null;
    } catch (err) {
      console.error('Failed to apply all triage suggestions:', err);
    } finally {
      triageDraftBusy.value = false;
      await loadInbox();
    }
  }

  async function submitZeroDecisionCapture() {
    const text = captureInput.value.trim();
    if (!text && !captureMediaId.value) return;
    captureBusy.value = true;
    try {
      const id = typeof crypto !== 'undefined' && crypto.randomUUID
        ? `cap-${crypto.randomUUID()}`
        : `cap-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
      await kernelBridge.inboxCapture({
        id,
        content: text,
        mediaId: captureMediaId.value || null,
      });
      captureInput.value = '';
      captureMediaId.value = null;
      await loadInbox();
    } catch (err) {
      console.error('Failed to capture spark:', err);
    } finally {
      captureBusy.value = false;
    }
  }

  async function manuallySetItemDestination(sparkId, destination) {
    if (!available || !kernelBridge.inboxTriageSet) return;
    const item = inboxItems.value.find(x => x.id === sparkId);
    const expectedVersion = item ? (item.triage_version || 0) : 0;
    inboxBusy.value = true;
    try {
      await kernelBridge.inboxTriageSet({
        captureId: sparkId,
        destination: destination || null,
        batchId: triageDraft.value?.id || null,
        expectedVersion,
      });
      if (triageDraft.value?.batch?.suggestions) {
        for (const sugg of triageDraft.value.batch.suggestions) {
          if (Array.isArray(sugg.spark_ids) && sugg.spark_ids.includes(sparkId)) {
            sugg._applied = true;
            sugg._appliedLabel = destination?.label || destination?.kind || '';
          }
        }
      }
      await loadInbox();
    } catch (err) {
      console.error('Failed to manually set destination:', err);
    } finally {
      inboxBusy.value = false;
    }
  }

  async function createAndAssignProject(sparkId, rawTitle, suggestionIndex = -1) {
    const trimmed = String(rawTitle || '').trim();
    if (!trimmed || !available) return null;

    const existingProjects = ctx.catalog?.value?.projects || [];
    const importedIds = new Set(ctx.catalog?.value?.imported_project_ids || []);
    let targetProject = existingProjects.find(
      p =>
        p?.id &&
        p.status !== 'archived' &&
        p.source_kind !== 'legacy_import' &&
        !importedIds.has(p.id) &&
        String(p.title || '').trim().toLowerCase() === trimmed.toLowerCase(),
    );

    if (!targetProject) {
      if (!kernelBridge.createProject) return null;
      const pid =
        typeof crypto !== 'undefined' && crypto.randomUUID
          ? `proj-${crypto.randomUUID().slice(0, 8)}`
          : `proj-${Date.now().toString(36)}`;
      const snap = await kernelBridge.createProject(pid, trimmed);
      if (snap) {
        if (ctx.applyCatalogPreferences) {
          ctx.applyCatalogPreferences(snap);
        } else if (ctx.catalog) {
          ctx.catalog.value = snap;
        }
      }
      targetProject = snap?.projects?.find(p => p.id === pid) || { id: pid, title: trimmed };
    }

    const finalId = targetProject.id;
    const finalTitle = targetProject.title || trimmed;

    if (suggestionIndex >= 0 && triageDraft.value?.batch?.suggestions?.[suggestionIndex]) {
      const sugg = triageDraft.value.batch.suggestions[suggestionIndex];
      sugg.destination = 'project_todo';
      sugg.project_id = finalId;
      sugg.project_title = finalTitle;
      await applyTriageSuggestion(suggestionIndex, {
        kind: 'project_todo',
        refId: finalId,
        label: finalTitle,
      });
    } else if (sparkId) {
      await manuallySetItemDestination(sparkId, {
        kind: 'project_todo',
        refId: finalId,
        label: finalTitle,
      });
    }

    if (kernelBridge.syncHome) {
      void Promise.resolve(kernelBridge.syncHome()).catch(() => {});
    }
    return { id: finalId, title: finalTitle };
  }

  async function setItemResolution(captureId, resolutionStatus, resolutionNote = null) {
    if (!available || !kernelBridge.inboxResolutionSet) return;
    inboxBusy.value = true;
    try {
      await kernelBridge.inboxResolutionSet({ captureId, resolutionStatus, resolutionNote });
      await loadInbox();
    } catch (err) {
      console.error('Failed to update inbox resolution:', err);
    } finally {
      inboxBusy.value = false;
    }
  }

  async function updateItemContent(captureId, content) {
    if (!available || !kernelBridge.inboxUpdateContent) return false;
    const trimmed = String(content || '').trim();
    if (!trimmed) return false;
    inboxBusy.value = true;
    try {
      await kernelBridge.inboxUpdateContent({ captureId, content: trimmed });
      await loadInbox();
      return true;
    } catch (err) {
      console.error('Failed to update inbox content:', err);
      return false;
    } finally {
      inboxBusy.value = false;
    }
  }

  async function checkImplemented({ projectId = null, captureIds = null } = {}) {
    if (!available || !kernelBridge.inboxCheckImplemented) return null;
    checkingImplemented.value = true;
    try {
      const res = await kernelBridge.inboxCheckImplemented({ projectId, captureIds });
      await loadInbox();
      return res;
    } catch (err) {
      console.error('Failed to check implemented status:', err);
      return null;
    } finally {
      checkingImplemented.value = false;
    }
  }

  return {
    inboxItems,
    inboxBusy,
    inboxFilter,
    unsortedCount,
    sortedCount,
    laterCount,
    filteredItems,
    sortedProjectGroups,
    triageDraft,
    triageDraftBusy,
    triageDraftError,
    triageStatusNotice,
    captureInput,
    captureMediaId,
    captureBusy,
    checkingImplemented,
    loadInbox,
    loadTriageDraft,
    dismissTriageDraft,
    applyTriageSuggestion,
    applyAllTriageSuggestions,
    submitZeroDecisionCapture,
    manuallySetItemDestination,
    createAndAssignProject,
    setItemResolution,
    updateItemContent,
    checkImplemented,
  };
}
