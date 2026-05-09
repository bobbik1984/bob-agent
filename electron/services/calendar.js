/**
 * bob-agent — Microsoft 365 日历同步
 *
 * 移植自 TodoList calendar_sync.py (122行)
 * 认证方式: OAuth 2.0 Client Credentials flow (MSAL)
 * 凭据来源: api-registry → microsoft_graph_azure → bobs_calendar
 *
 * TODO: Jules 完成 MSAL 集成
 */

// 占位 — Sprint 3 实现
class CalendarClient {
  constructor() {
    this.enabled = false;
  }

  async createEvent(event) {
    console.warn('[Calendar] Not configured — saving locally only');
    return null;
  }

  async listEvents(startDatetime, endDatetime) {
    console.warn('[Calendar] Not configured');
    return [];
  }

  async deleteEvent(eventId) {
    console.warn('[Calendar] Not configured');
  }
}

module.exports = { CalendarClient };
