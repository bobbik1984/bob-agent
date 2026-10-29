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
fixture = '''export const kernelBridge = {
 available:()=>true, bootstrap:async()=>({session_id:'ui-fixture'}),
 syncHome:async()=> 'ok', readiness:async()=>({mode:'home_configured',saved_messages:2,pending_messages:0}),
 history:async()=>[{id:'user',role:'user',content:'UI-only fixture request',send_state:'peer_accepted'},
 {id:'reply',role:'assistant',source:'home_codex_execution',content:'UI fixture reply — copy this exact text.'}],
 executions:async()=>[], prewarm:async()=>null,
 pairingCapabilities:async()=>({can_scan:true,can_host:false}),
 pairingStatus:async()=>({state:'connected'})
};'''
results = []
with sync_playwright() as p:
 browser = p.chromium.launch()
 try:
  for width,height,locale,scheme in [(390,844,'en-US','light'),(1180,757,'en-US','light'),(390,420,'en-US','light'),(390,844,'zh-CN','dark'),(1180,757,'zh-CN','dark')]:
   labels={'copy':'复制','settings':'设置','close':'关闭设置'} if locale=='zh-CN' else {'copy':'Copy','settings':'Settings','close':'Close settings'}
   context=browser.new_context(viewport={'width':width,'height':height},permissions=['clipboard-read','clipboard-write'],locale=locale,color_scheme=scheme)
   page=context.new_page()
   page.route('**/src/native-bridge.mjs',lambda route:route.fulfill(content_type='text/javascript',body=fixture))
   page.goto(args.url)
   page.get_by_text('UI fixture reply — copy this exact text.',exact=True).wait_for()
   page.locator('textarea').fill('Retain this draft')
   history=page.locator('.history').bounding_box()
   assert history['height'] > height*.40, history
   assert not page.locator('.topbar img').count()
   page.get_by_role('button',name='打开对话与项目' if locale=='zh-CN' else 'Open conversations and projects',exact=True).click()
   page.locator('.conversation-link').click()
   assert page.locator('textarea').input_value()=='Retain this draft'
   assert not page.locator('.storage-summary').count()
   page.get_by_role('button',name=labels['copy'],exact=True).click()
   assert page.evaluate('navigator.clipboard.readText()')=='UI fixture reply — copy this exact text.'
   page.get_by_role('button',name=labels['settings'],exact=True).click()
   page.get_by_role('dialog').wait_for(state='visible')
   assert page.get_by_role('button',name=labels['close']).evaluate('(e)=>e===document.activeElement')
   page.keyboard.press('Escape')
   assert page.get_by_role('button',name=labels['settings'],exact=True).evaluate('(e)=>e===document.activeElement')
   page.get_by_role('button',name=labels['settings'],exact=True).click()
   page.get_by_role('button',name=labels['close']).click()
   page.locator('.device-indicator').click()
   page.locator('.qr-panel').wait_for(state='visible')
   page.get_by_role('button',name=labels['close']).click()
   assert page.locator('textarea').input_value()=='Retain this draft'
   assert page.locator('.history').bounding_box()==history
   page.screenshot(path=str(args.output/f'ui-{width}-{height}-{locale}-{scheme}.png'))
   results.append({'viewport':[width,height],'locale':locale,'scheme':scheme,'history_height':history['height'],'copy':True,'settings_return_draft':True,'logo':True,'scope':'UI fixture, not Android keyboard or real Home'})
   context.close()
 finally:
  browser.close()
(args.output/'results.json').write_text(json.dumps(results,indent=2),encoding='utf-8')
print(json.dumps(results))
