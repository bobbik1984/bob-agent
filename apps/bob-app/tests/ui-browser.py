"""UI-only fixture through the actual Vue app. Not native or model evidence."""
import argparse
import json
from pathlib import Path
from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser()
parser.add_argument('--url', required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
fixture = '''
let selectedId = 'ui-fixture';
const drafts = { 'ui-fixture': { draft: '', rev: 0 }, 'ui-second': { draft: 'Second session saved draft', rev: 1 } };
let groupMode = 'project';
function snapshot() {
  const convs = [
    {
      id: 'ui-fixture',
      project_id: 'proj-bob',
      project_title: 'Bob Next',
      title: 'Conversation',
      target_device_id: 'dev-x1c',
      target_device_label: 'ThinkPad X1C',
      target_device_online: true,
      status_reason: 'Local execution host is ready.',
      view_state: { draft: drafts['ui-fixture'].draft, draft_revision: drafts['ui-fixture'].rev, scroll_offset: 0 },
      unread_count: 0,
      has_running_execution: false,
      version: 1
    },
    {
      id: 'ui-second',
      project_id: null,
      project_title: null,
      title: 'Standalone Check',
      target_device_id: 'dev-x1t',
      target_device_label: 'ThinkPad X1T',
      target_device_online: true,
      status_reason: 'Peer node reachable; clean workspace.',
      view_state: { draft: drafts['ui-second'].draft, draft_revision: drafts['ui-second'].rev, scroll_offset: 0 },
      unread_count: 1,
      has_running_execution: false,
      version: 1
    }
  ];
  return {
    selected_conversation_id: selectedId,
    preferences: { sidebar_group_mode: groupMode, locale: null, theme: null },
    projects: [{ id: 'proj-bob', title: 'Bob Next', workspace_id: 'ws-1', directory_ref: 'D:/Code/bob-agent-next', execution_device_id: 'dev-x1c', version: 1 }],
    conversations: convs,
    standalone_conversations: [convs[1]],
    device_groups: [
      { device_id: 'dev-x1c', label: 'ThinkPad X1C', role: 'home', availability_state: 'online', immediate_execution_available: true, status_reason: 'Local execution host is ready.', conversations: [convs[0]] },
      { device_id: 'dev-x1t', label: 'ThinkPad X1T', role: 'peer', availability_state: 'online', immediate_execution_available: true, status_reason: 'Peer node reachable; clean workspace.', conversations: [convs[1]] }
    ]
  };
}
export const kernelBridge = {
 available:()=>true,
 bootstrap:async()=>({session_id:selectedId}),
 catalog:async()=>snapshot(),
 createProject:async()=>snapshot(),
 createConversation:async()=>snapshot(),
 selectConversation:async(id)=>{ selectedId = id; return snapshot(); },
 saveViewState:async(v)=>{ if (drafts[v.conversationId] && v.draft !== null && v.draft !== undefined) { drafts[v.conversationId].draft = v.draft; drafts[v.conversationId].rev += 1; } return { conversation_id: v.conversationId, draft: v.draft ?? '', draft_revision: 1 }; },
 savePreferences:async(p)=>{ if (p.sidebarGroupMode) groupMode = p.sidebarGroupMode; return p; },
 syncHome:async()=> 'ok',
 readiness:async()=>({mode:'home_configured',saved_messages:2,pending_messages:0}),
 history:async(id)=> id === 'ui-second'
   ? [{id:'m-second',role:'assistant',source:'home_codex_execution',content:'Second session history item.'}]
   : [{id:'user',role:'user',content:'UI-only fixture request',send_state:'peer_accepted'},
      {id:'reply',role:'assistant',source:'home_codex_execution',content:'UI fixture reply — copy this exact text.'}],
 executions:async()=>[], prewarm:async()=>null,
 providerGet:async()=>({id:'default',provider_kind:'openai_compatible',label:'OpenAI',base_url:'https://api.openai.com/v1',model_id:'gpt-4o-mini',api_key_slot:'provider-key-default',api_key_configured:true,enabled:true,last_check_status:'ok',last_check_reason:'gpt-4o-mini',last_checked_at:100,updated_at:100}),
 providerSave:async(p)=>({id:'default',provider_kind:p.providerKind,label:p.label,base_url:p.baseUrl,model_id:p.modelId,api_key_slot:'provider-key-default',api_key_configured:true,enabled:p.enabled,last_check_status:'ok',last_check_reason:p.modelId,last_checked_at:100,updated_at:100}),
 providerTest:async()=>({id:'default',provider_kind:'openai_compatible',label:'OpenAI',base_url:'https://api.openai.com/v1',model_id:'gpt-4o-mini',api_key_slot:'provider-key-default',api_key_configured:true,enabled:true,last_check_status:'ok',last_check_reason:'gpt-4o-mini',last_checked_at:100,updated_at:100}),
 routeGet:async()=>'auto',
 routeSet:async(_,r)=>r,
 pairingCapabilities:async()=>({can_scan:true,can_host:false}),
 pairingStatus:async()=>({state:'connected'})
};'''
results = []
with sync_playwright() as p:
 browser = p.chromium.launch()
 try:
  for width,height,locale,scheme in [(390,844,'en-US','light'),(1180,757,'en-US','light'),(390,420,'en-US','light'),(390,844,'zh-CN','dark'),(1180,757,'zh-CN','dark')]:
   labels={'copy':'复制','settings':'设置','close':'关闭设置','back':'返回设置','by_device':'按执行设备','by_project':'按项目 / 任务','models_badge':'199 个可用模型'} if locale=='zh-CN' else {'copy':'Copy','settings':'Settings','close':'Close settings','back':'Back to Settings','by_device':'By Execution Device','by_project':'By Project / Task','models_badge':'199 available models'}
   context=browser.new_context(viewport={'width':width,'height':height},permissions=['clipboard-read','clipboard-write'],locale=locale,color_scheme=scheme)
   page=context.new_page()
   page.route('**/src/native-bridge.mjs',lambda route:route.fulfill(content_type='text/javascript',body=fixture))
   page.goto(args.url)
   page.get_by_text('UI fixture reply — copy this exact text.',exact=True).wait_for()
   expected_layout = 'layout-desktop-wide' if (width > height and width > 768) else 'layout-mobile-portrait'
   shell_class = page.locator('.app-shell').get_attribute('class')
   assert expected_layout in shell_class, shell_class
   page.locator('textarea').fill('Retain this draft')
   history=page.locator('.history').bounding_box()
   assert history['height'] > height*.40, history
   assert not page.locator('.topbar img').count()
   page.get_by_role('button',name='打开对话与项目' if locale=='zh-CN' else 'Open conversations and projects',exact=True).click()
   page.get_by_role('button',name=labels['by_device'],exact=True).click()
   page.locator('.device-status-reason').first.wait_for(state='visible')
   page.get_by_role('button',name=labels['by_project'],exact=True).click()
   page.locator('.conversation-link').first.click()
   page.get_by_text('Second session history item.',exact=True).wait_for()
   assert page.locator('textarea').input_value()=='Second session saved draft'
   page.get_by_role('button',name='打开对话与项目' if locale=='zh-CN' else 'Open conversations and projects',exact=True).click()
   page.locator('.project-card .conversation-link').first.click()
   page.get_by_text('UI fixture reply — copy this exact text.',exact=True).wait_for()
   assert page.locator('textarea').input_value()=='Retain this draft'
   assert not page.locator('.storage-summary').count()
   page.get_by_role('button',name=labels['copy'],exact=True).click()
   assert page.evaluate('navigator.clipboard.readText()')=='UI fixture reply — copy this exact text.'
   page.get_by_role('button',name=labels['settings'],exact=True).click()
   page.get_by_role('dialog').wait_for(state='visible')
   page.locator('.settings-home-grid').wait_for(state='visible')
   page.get_by_text(labels['models_badge'],exact=True).wait_for(state='visible')
   assert page.locator('.vault-key-row').count() >= 11
   assert page.get_by_role('button',name=labels['close']).evaluate('(e)=>e===document.activeElement')
   page.screenshot(path=str(args.output/f'ui-modelhub-{width}-{height}-{locale}-{scheme}.png'))
   page.locator('.open-provider-subpage').click()
   page.locator('.provider-form').wait_for(state='visible')
   page.get_by_role('button',name=labels['back'],exact=True).click()
   page.locator('.settings-home-grid').wait_for(state='visible')
   page.locator('.open-device-subpage').click()
   page.locator('.qr-panel').wait_for(state='visible')
   page.get_by_role('button',name=labels['back'],exact=True).click()
   page.locator('.settings-home-grid').wait_for(state='visible')
   page.keyboard.press('Escape')
   assert page.get_by_role('button',name=labels['settings'],exact=True).evaluate('(e)=>e===document.activeElement')
   page.locator('.device-indicator').click()
   page.locator('.qr-panel').wait_for(state='visible')
   page.get_by_role('button',name=labels['close']).click()
   assert page.locator('textarea').input_value()=='Retain this draft'
   assert page.locator('.history').bounding_box()==history
   page.screenshot(path=str(args.output/f'ui-{width}-{height}-{locale}-{scheme}.png'))
   results.append({'viewport':[width,height],'layout':expected_layout,'locale':locale,'scheme':scheme,'history_height':history['height'],'model_hub_199':True,'copy':True,'multi_session_switch_draft':True,'settings_subpage_and_return_draft':True,'logo':True,'scope':'UI fixture, not Android keyboard or real Home'})
   context.close()
 finally:
  browser.close()
(args.output/'results.json').write_text(json.dumps(results,indent=2),encoding='utf-8')
print(json.dumps(results))
