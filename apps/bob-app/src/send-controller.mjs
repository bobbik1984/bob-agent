/** UI boundary only. The injected bridge must resolve acceptMessage only after
 * the kernel transaction commits. No network retry or transport state here. */
export function createSendController({ bridge, createIntent, onAccepted }) {
  let draft = '';
  let revision = 0;
  let pending = null;
  let retry = null;
  let lastError = null;

  function prewarm() {
    // Failure is silent and cannot affect local acceptance. Kernel owns cooldown.
    return Promise.resolve().then(() => bridge.prewarm()).catch(() => undefined);
  }
  function setDraft(value) { draft = value; revision += 1; }
  function send() {
    if (pending) return pending;
    if (!draft.trim()) return Promise.resolve(null);
    const text = draft;
    const sentRevision = revision;
    const intent = retry?.revision === revision ? retry.intent : createIntent(text);
    retry = { revision, intent };
    lastError = null;
    pending = Promise.resolve().then(() => bridge.acceptMessage(intent)).then(receipt => {
      if (receipt?.request_id !== intent.request_id || receipt?.message_id !== intent.message_id) {
        throw new Error('LOCAL_RECEIPT_MISMATCH');
      }
      // Do not erase text typed while the durable transaction was pending.
      if (revision === sentRevision) { draft = ''; revision += 1; }
      retry = null;
      // Delivery to the view is separate from durable acceptance. A rendering
      // error must not cause a second business send; history can be reloaded.
      try { onAccepted(receipt); } catch { lastError = 'VIEW_REFRESH_REQUIRED'; }
      return receipt;
    }).catch(error => {
      lastError = 'LOCAL_SAVE_UNCONFIRMED';
      // Retry uses the same identity if draft was not edited; this also covers
      // a lost IPC response after a successful durable commit.
      throw error;
    }).finally(() => { pending = null; });
    return pending;
  }
  return { setDraft, prewarm, send,
    snapshot: () => ({ draft, saving: pending !== null, errorKey: lastError }) };
}
