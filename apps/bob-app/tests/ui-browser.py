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
  for width,height in [(390,844),(1180,757),(390,420)]:
   context=browser.new_context(viewport={'width':width,'height':height},permissions=['clipboard-read','clipboard-write'],locale='en-US')
   page=context.new_page()
   page.route('**/src/native-bridge.mjs',lambda route:route.fulfill(content_type='text/javascript',body=fixture))
   page.goto(args.url)
   page.get_by_text('UI fixture reply — copy this exact text.',exact=True).wait_for()
   page.locator('textarea').fill('Retain this draft')
   history=page.locator('.history').bounding_box()
   assert history['height'] > height*.40, history
   assert page.locator('.brand img').evaluate('(e)=>e.complete && e.naturalWidth>0')
   assert not page.locator('.storage-summary').count()
   page.get_by_role('button',name='Copy',exact=True).click()
   assert page.evaluate('navigator.clipboard.readText()')=='UI fixture reply — copy this exact text.'
   page.get_by_role('button',name='Settings',exact=True).click()
   page.get_by_role('dialog').wait_for(state='visible')
   assert page.get_by_role('button',name='Close settings').evaluate('(e)=>e===document.activeElement')
   page.keyboard.press('Escape')
   assert page.get_by_role('button',name='Settings',exact=True).evaluate('(e)=>e===document.activeElement')
   page.get_by_role('button',name='Settings',exact=True).click()
   page.get_by_role('button',name='Close settings').click()
   assert page.locator('textarea').input_value()=='Retain this draft'
   assert page.locator('.history').bounding_box()==history
   page.screenshot(path=str(args.output/f'ui-{width}-{height}.png'))
   results.append({'viewport':[width,height],'history_height':history['height'],'copy':True,'settings_return_draft':True,'logo':True,'scope':'UI fixture, not Android keyboard or real Home'})
   context.close()
 finally:
  browser.close()
(args.output/'results.json').write_text(json.dumps(results,indent=2),encoding='utf-8')
print(json.dumps(results))
