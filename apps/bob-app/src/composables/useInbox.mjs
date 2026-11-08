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

  async function loadInbox() {
    if (!available || !kernelBridge.inboxList) return;
    inboxBusy.value = true;
    try {
      const list = await kernelBridge.inboxList(null);
      inboxItems.value = (Array.isArray(list) ? list : []).map(item => ({
        ...item,
        content: item.content || item.content_ref || item.contentRef || '',
      }));
    } catch (err) {
      console.error('Failed to load inbox:', err);
    } finally {
      inboxBusy.value = false;
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

  return {
    inboxItems,
    inboxBusy,
    inboxFilter,
    unsortedCount,
    sortedCount,
    laterCount,
    filteredItems,
    triageDraft,
    triageDraftBusy,
    triageDraftError,
    triageStatusNotice,
    captureInput,
    captureMediaId,
    captureBusy,
    loadInbox,
    loadTriageDraft,
    dismissTriageDraft,
    applyTriageSuggestion,
    applyAllTriageSuggestions,
    submitZeroDecisionCapture,
    manuallySetItemDestination,
  };
}
