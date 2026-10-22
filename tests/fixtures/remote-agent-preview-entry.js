import { createApp, ref, h, provide } from 'vue';
import { createI18n } from 'vue-i18n';
import WorkView from '../../src/views/WorkView.vue';
import RemoteAgentConnections from '../../src/components/work/RemoteAgentConnections.vue';
import zh from '../../src/locales/zh-CN.json';
import en from '../../src/locales/en-US.json';
import '../../src/index.css';
// Test-only fixture. Not a production bridge and not a connected remote host.
window.__remoteFixtureCalls = [];
let live = null;
let unlocked = false, profiles = [];
const connection = () => ({ connectionId: 'fixture', profileId: 'p1', adapter: 'dsh', endpoint: 'https://fixture.invalid/', capabilities: { catalog: true, history: true, send: true } });
const offlineDemo = location.protocol === 'file:';
const localProject = { id: 'local1', title: 'Existing local project', status: 'active', updatedAt: 1, mission: 'Preserve original work records' };
window.appAPI = {
  workProjectList: async () => [localProject],
  workProjectGet: async () => ({ project: localProject, goals: [{ id: 'goal1', title: 'Existing local goal', status: 'active' }], tasks: [], commitments: [], decisions: [], artifacts: [], recentEvents: [] }),
  goalRuntimeList: async () => [],
  workChangeReviewList: async () => [],
  workProjectLinkListPending: async () => [],
  listenEvent: async () => () => {},
  remoteAgentProfiles: async () => ({ unlocked, profiles: structuredClone(profiles) }),
  remoteAgentUnlock: async () => { unlocked = true; return { unlocked, profiles: structuredClone(profiles) }; },
  remoteAgentLock: async () => { unlocked = false; },
  remoteAgentSave: async (_, name) => { profiles = [{ id: 'p1', name, endpoint: 'https://fixture.invalid/', sessionId: '', autoReconnect: true, uncertainSessions: [] }]; return connection(); },
  remoteAgentReconnect: async () => connection(),
  remoteAgentProfileUpdate: async (id, patch) => { Object.assign(profiles.find(p => p.id === id), patch); },
  remoteAgentForget: async id => { profiles = profiles.filter(p => p.id !== id); },
  remoteAgentConnect: async () => ({ connectionId: 'fixture', adapter: 'dsh', endpoint: 'https://fixture.invalid/', capabilities: { catalog: true, history: true, send: true } }),
  remoteAgentCatalog: async () => ({ workspaces: [{ workspaceId: 'w1', title: 'Blender · 演示项目', sessionIds: ['s1', 's2'] }], sessions: [{ sessionId: 's1', title: '建筑模型渲染 · 示例', running: false }, { sessionId: 's2', title: '另一条已有会话 · 示例', running: true }] }),
  remoteAgentHistory: async (_, sessionId) => ({ sessionId, hasMore: true, omittedRecords: 2, messages: [{ id: '1', role: 'user', text: '这是一条界面测试消息，不会发送到远端。' }, { id: '2', role: 'assistant', text: '<img src="file:///private" onerror="window.__unsafe=true">\n这段内容必须作为普通文字展示，不得执行或读取本地文件。\n' + '长记录边界测试'.repeat(20) }] }),
  remoteAgentDisconnect: async id => { window.__remoteFixtureCalls.push(['disconnect', id]); },
  remoteAgentWatch: async (_, sessionId) => (live = { watchId: 'watch', sessionId, ready: true, phase: 'idle', messages: [{ id: '1', role: 'assistant', text: offlineDemo ? '示例会话：准备调整模型的灯光与相机。你可以在下方发送一条测试消息，查看实时文字、折叠过程与审批卡片。\nDemo session: send a sample message to preview live text and an approval card.' : '<img src="file:///private" onerror="window.__unsafe=true">\n纯文字，不执行。' }], interactions: [], hasMore: false }),
  remoteAgentLive: async () => ({ ...live }),
  remoteAgentAction: async (_, input) => {
    window.__remoteFixtureCalls.push([input.kind, input.watchId]);
    if (input.kind === 'send') {
      live = { ...live, phase: 'running', draft: '实时文字 · Live text', reasoning: '过程默认折叠 · Reasoning collapsed', interactions: [{ eventId: 'approval1', agentId: live.sessionId, kind: 'approval', request: { toolName: 'render', reason: '请确认本次渲染 · Approve this render' }, status: 'pending' }] };
    } else if (input.kind === 'answer') live = { ...live, interactions: live.interactions.map(i => ({ ...i, status: 'answered' })) };
    else if (input.kind === 'cancel') live = { ...live, phase: 'cancel_requested' };
    return { ...live };
  },
};
const app = createApp({ setup() {
  const active = ref('settings');
  const workPanel = ref('overview');
  provide('activeDrawer', active);
  provide('openRemoteConnections', () => { active.value = 'settings'; });
  return () => h('div', [
    h('nav', { style: 'display:flex;gap:12px;padding:12px' }, ['settings', 'work'].map(id => h('button', { 'data-nav': id, onClick: () => { active.value = id; } }, id))),
    h('div', { style: active.value === 'settings' ? '' : 'display:none' }, [h(RemoteAgentConnections)]),
    h('div', { style: active.value === 'work' ? '' : 'display:none' }, [h(WorkView, { activePanel: workPanel.value, 'onUpdate:activePanel': value => { workPanel.value = value; } })]),
  ]);
} });
app.provide('isMobile', ref(innerWidth <= 720));
app.use(createI18n({ legacy: false, locale: new URLSearchParams(location.search).get('lang') || 'zh-CN', messages: { 'zh-CN': zh, 'en-US': en } }));
app.mount('#app');
