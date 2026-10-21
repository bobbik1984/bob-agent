// Adapter-neutral, per-panel lifetime. No credentials, remote records or handles are persisted.
export function createRemoteAgentController(api, publish) {
  let generation = 0;
  let selection = 0;
  let disposed = false;
  let connection = null;
  const update = patch => { if (!disposed) publish(patch); };
  const close = info => info && api.remoteAgentDisconnect(info.connectionId).catch(() => {});
  const safeError = error => {
    const code = String(error);
    return /^REMOTE_[A-Z_]+$/.test(code) ? code : 'REMOTE_UNKNOWN';
  };
  return {
    async connect(input) {
      const current = ++generation;
      ++selection;
      const previous = connection;
      connection = null;
      await close(previous);
      if (disposed || current !== generation) return;
      update({ connection: null, catalog: null, history: null, sessionId: '', busy: true, error: '' });
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
      update({ history: null, sessionId, historyBusy: true, error: '' });
      try {
        const history = await api.remoteAgentHistory(connection.connectionId, sessionId);
        if (current === generation && ticket === selection) update({ history, historyBusy: false });
      } catch (error) {
        if (current === generation && ticket === selection) update({ historyBusy: false, error: safeError(error) });
      }
    },
    async disconnect() {
      ++generation;
      ++selection;
      const previous = connection;
      connection = null;
      update({ connection: null, catalog: null, history: null, sessionId: '', busy: false, historyBusy: false, error: '' });
      await close(previous);
    },
    dispose() {
      disposed = true;
      ++generation;
      ++selection;
      const previous = connection;
      connection = null;
      return close(previous);
    },
  };
}
