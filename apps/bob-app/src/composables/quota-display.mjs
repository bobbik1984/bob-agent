export function compactQuota(data, now, windowId = 'weekly') {
  if (!['available', 'stale'].includes(data?.status)) return null;
  const windows = (data.windows || []).filter(w => Number.isFinite(w.remaining_percent) && w.remaining_percent >= 0 && w.remaining_percent <= 100);
  // A short window must never silently replace the selected weekly allowance.
  const w = windows.find(w => w.id === windowId);
  if (!w) return null;
  const duration = {weekly:604800, five_hour:18000, 'gemini-weekly':604800, 'gemini-5h':18000, '3p-weekly':604800, '3p-5h':18000}[w.id];
  const seconds = w.resets_at ? Math.max(0, w.resets_at-now) : null;
  const mins = seconds === null ? null : Math.ceil(seconds / 60);
  const days = Math.floor(mins / 1440), hours = Math.floor((mins % 1440) / 60);
  return {...w, expired:seconds === 0, timePercent:seconds === null || !duration ? null : Math.min(100, seconds/duration*100),
    timeText:mins === null ? '—' : days ? `${days}d ${hours}h` : hours ? `${hours}h ${mins%60}m` : `${mins}m`};
}
