import i18n from '@/i18n';

/** 获取当前语言环境的 locale 标识（如 'zh-CN' 或 'en-US'） */
function getLocale() {
  return i18n.global.locale.value || 'zh-CN';
}

/** 相对时间：今天 10:00 / 昨天 14:30 / 6月1日 */
export function formatRelativeTime(ts) {
  if (!ts) return '';
  const t = i18n.global.t;
  const d = new Date(ts > 1e11 ? ts : ts * 1000);
  const now = new Date();
  const isToday = d.toDateString() === now.toDateString();
  const yesterday = new Date(now); yesterday.setDate(now.getDate() - 1);
  const isYesterday = d.toDateString() === yesterday.toDateString();
  const time = d.toLocaleTimeString(getLocale(), { hour: '2-digit', minute: '2-digit' });
  
  if (isToday) return `${t('date.today')} ${time}`;
  if (isYesterday) return `${t('date.yesterday')} ${time}`;
  return d.toLocaleDateString(getLocale(), { month: 'short', day: 'numeric' }) + ' ' + time;
}

/** 模糊相对时间：刚刚 / 3小时前 / 2天前 / 6月1日 */
export function formatFuzzyTime(ts) {
  if (!ts) return '';
  const t = i18n.global.t;
  const d = new Date(ts > 1e11 ? ts : ts * 1000);
  const diffMs = Date.now() - d.getTime();
  const diffH = Math.floor(diffMs / 3_600_000);
  if (diffH < 1) return t('date.just_now');
  if (diffH < 24) return t('date.hours_ago', { n: diffH });
  const diffD = Math.floor(diffH / 24);
  if (diffD < 7) return t('date.days_ago', { n: diffD });
  return d.toLocaleDateString(getLocale());
}

/** 日期 + 时间：2026/6/30 10:00 */
export function formatDateTime(ts) {
  if (!ts) return '';
  const d = new Date(ts > 1e11 ? ts : ts * 1000);
  return d.toLocaleDateString(getLocale()) + ' ' + d.toLocaleTimeString(getLocale(), { hour: '2-digit', minute: '2-digit' });
}

/** 纯日期：2026/6/30 */
export function formatDate(ts) {
  if (!ts) return '';
  const d = new Date(ts > 1e11 ? ts : ts * 1000);
  return d.toLocaleDateString(getLocale());
}

/** 时间范围：6月1日 10:00 - 11:30 */
export function formatTimeRange(startTs, endTs) {
  if (!startTs) return '';
  const locale = getLocale();
  const start = new Date(startTs);
  let str = start.toLocaleString(locale, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
  if (endTs) {
    const end = new Date(endTs);
    if (start.toDateString() === end.toDateString()) {
      str += ` - ${end.toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' })}`;
    } else {
      str += ` — ${end.toLocaleString(locale, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })}`;
    }
  }
  return str;
}
