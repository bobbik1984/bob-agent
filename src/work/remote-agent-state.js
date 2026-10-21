// Adapter-neutral, per-panel lifetime. No credentials, remote records or handles are persisted.
export function createRemoteAgentController(api, publish) {
  let generation = 0;
  let selection = 0;
  let disposed = false;
  let connection = null;
  let live = null;
  let timer = null;
  let actionBusy = false;
  let pollEpoch = 0;
  const stopPolling = () => { clearTimeout(timer); timer = null; ++pollEpoch; live = null; };
  const update = patch => { if (!disposed) publish(patch); };
  const close = info => info && api.remoteAgentDisconnect(info.connectionId).catch(() => {});
  const safeError = error => {
    const code = String(error);
    return /^REMOTE_[A-Z_]+$/.test(code) ? code : 'REMOTE_UNKNOWN';
  };
  const publishLive = value => {
    live = value;
    update({ live: value, history: { sessionId: value.sessionId, messages: value.messages, hasMore: value.hasMore, omittedRecords: value.omittedRecords }, error: value.error || '' });
  };
  const poll = async (current, ticket) => {
    if (disposed || generation !== current || selection !== ticket || !live) return;
    const epoch = pollEpoch;
    try {
      if (!actionBusy) {
        const value = await api.remoteAgentLive(connection.connectionId, live.watchId);
        if (generation !== current || selection !== ticket || disposed || epoch !== pollEpoch) return;
        publishLive(value);
        if (!value.ready) return;
      }
    } catch (error) {
      if (generation === current && selection === ticket && epoch === pollEpoch) {
        publishLive({ ...live, ready: false, phase: 'unknown', error: safeError(error), interactions: [] });
      }
      return;
    }
    timer = setTimeout(() => void poll(current, ticket), 250);
  };
  const action = async input => {
    if (!connection || !live?.ready || actionBusy || disposed) return false;
    const current = generation, ticket = selection;
    actionBusy = true; ++pollEpoch; clearTimeout(timer);
    update({ actionBusy: true, error: '' });
    try {
      const value = await api.remoteAgentAction(connection.connectionId, { watchId: live.watchId, ...input });
      if (current !== generation || ticket !== selection || disposed) return false;
      publishLive(value);
      return true;
    } catch (error) {
      if (current === generation && ticket === selection && !disposed) {
        const code = safeError(error);
        if (['REMOTE_PROTOCOL', 'REMOTE_BUSY', 'REMOTE_NOT_RUNNING', 'REMOTE_STALE'].includes(code)) update({ error: code });
        else publishLive({ ...live, ready: false, phase: 'unknown', error: code, interactions: [] });
      }
      return false;
    } finally {
      actionBusy = false;
      update({ actionBusy: false });
      if (current === generation && ticket === selection && !disposed) {
        if (live?.ready) timer = setTimeout(() => void poll(current, ticket), 250);
      }
    }
  };
  return {
    async connect(input) {
      const current = ++generation;
      stopPolling();
      ++selection;
      const previous = connection;
      connection = null;
      await close(previous);
      if (disposed || current !== generation) return;
      update({ connection: null, catalog: null, history: null, live: null, sessionId: '', busy: true, error: '' });
      let opened;
      try {
        opened = await api.remoteAgentConnect(input);
        // The caller clears the input immediately; never copy it into controller state.
        if (disposed || current !== generation) { await close(opened); return; }
        if (opened.capabilities?.catalog !== true) throw 'REMOTE_UNSUPPORTED_CAPABILITY';
        const catalog = await api.remoteAgentCatalog(opened.connectionId);
        if (disposed || current !== generation) { await close(opened); return; }
        connection = opened;
        update({ connection, catalog, busy: false });
      } catch (error) {
        await close(opened);
        if (current === generation) update({ busy: false, error: safeError(error) });
      }
    },
    async refresh() {
      if (!connection || disposed) return;
      const current = generation;
      update({ busy: true, error: '' });
      try {
        const catalog = await api.remoteAgentCatalog(connection.connectionId);
        if (current === generation) update({ catalog, busy: false });
      } catch (error) {
        if (current === generation) update({ busy: false, error: safeError(error) });
      }
    },
    async select(sessionId) {
      if (!connection || disposed) return;
      if (connection.capabilities?.history !== true) {
        update({ error: 'REMOTE_UNSUPPORTED_CAPABILITY' });
        return;
      }
      const current = generation;
      const ticket = ++selection;
      stopPolling();
      update({ history: null, live: null, sessionId, historyBusy: true, error: '' });
      try {
        if (connection.capabilities?.send === true) {
          const value = await api.remoteAgentWatch(connection.connectionId, sessionId);
          if (current === generation && ticket === selection && !disposed) {
            publishLive(value); update({ historyBusy: false });
            timer = setTimeout(() => void poll(current, ticket), 250);
          }
          return;
        }
        const history = await api.remoteAgentHistory(connection.connectionId, sessionId);
        if (current === generation && ticket === selection) update({ history, historyBusy: false });
      } catch (error) {
        if (current === generation && ticket === selection) update({ historyBusy: false, error: safeError(error) });
      }
    },
    send(text, requestId) { return action({ kind: 'send', text, requestId }); },
    cancel() { return action({ kind: 'cancel' }); },
    answer(eventId, answer) { return action({ kind: 'answer', eventId, answer }); },
    async disconnect() {
      stopPolling();
      ++generation;
      ++selection;
      const previous = connection;
      connection = null;
      update({ connection: null, catalog: null, history: null, live: null, sessionId: '', busy: false, historyBusy: false, actionBusy: false, error: '' });
      await close(previous);
    },
    dispose() {
      stopPolling();
      disposed = true;
      ++generation;
      ++selection;
      const previous = connection;
      connection = null;
      return close(previous);
    },
  };
}
