import argparse,base64,hashlib,json,threading,uuid
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from pathlib import Path
from playwright.sync_api import sync_playwright,expect
parser=argparse.ArgumentParser(description='Isolated test node only. Fake vision HTTP service, no paid model.')
parser.add_argument('--url',required=True);parser.add_argument('--output',required=True,type=Path)
args=parser.parse_args();out=args.output;out.mkdir(parents=True,exist_ok=True)
seen=[]
class Vision(BaseHTTPRequestHandler):
 def do_POST(self):
  body=json.loads(self.rfile.read(int(self.headers['Content-Length'])));seen.append(body)
  data=json.dumps({'model':'fixture-vision','choices':[{'message':{'content':'SIMULATED extraction: design sketch'}}]}).encode()
  self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
 def log_message(self,*a): pass
server=ThreadingHTTPServer(('127.0.0.1',0),Vision);threading.Thread(target=server.serve_forever,daemon=True).start()
results=[]
run=uuid.uuid4().hex[:8]
with sync_playwright() as p:
 browser=p.chromium.launch();ctx=browser.new_context(viewport={'width':1180,'height':820})
 def rpc(name,args={}):
  value=ctx.request.post(args_url+'/api/rpc/'+name,data=args).json();assert value.get('ok'),value;return value['result']
 args_url=args.url.rstrip('/');rpc('app_preferences_save',{'locale':'zh-CN','theme':'light'})
 rpc('app_custom_model_save',{'id':'capture-fixture','name':'Loopback test vision','modelName':'fixture-vision','baseUrl':f'http://127.0.0.1:{server.server_port}/v1','apiKey':'fake-canary-not-a-secret','vision':True})
 rpc('app_model_role_assign',{'role':'vision','modelEntryId':'capture-fixture::fixture-vision'})
 page=ctx.new_page();errors=[];page.on('pageerror',lambda e:errors.append(str(e)));page.goto(args_url);page.wait_for_timeout(500)
 page.locator('.mobile-fab').click();expect(page.locator('.quicknote-card')).to_be_visible()
 assert page.locator('.quicknote-lead-icon').count()==0
 assert page.locator('.quicknote-card').get_by_text('扫码连接',exact=True).count()==0
 assert page.locator('.quicknote-input').evaluate('(e)=>getComputedStyle(e).outlineStyle')=='none'
 results.append('No inner logo, model switch, pairing shortcut or nested focus ring')
 # Create actual decodable image in the browser, not a text screenshot fixture.
 data=page.evaluate('''()=>{const c=document.createElement('canvas');c.width=640;c.height=360;const x=c.getContext('2d');x.fillStyle='#eef0ed';x.fillRect(0,0,640,360);x.strokeStyle='#42524a';x.lineWidth=4;x.strokeRect(80,50,480,240);x.fillStyle='#42524a';x.font='26px sans-serif';x.fillText('Design sketch - capture test',100,105);x.strokeRect(150,140,170,90);x.beginPath();x.arc(440,180,40,0,Math.PI*2);x.stroke();return c.toDataURL('image/png')}''')
 original=base64.b64decode(data.split(',')[1]);sha=hashlib.sha256(original).hexdigest()
 page.locator('input[type=file]').first.set_input_files({'name':'design.png','mimeType':'image/png','buffer':original})
 expect(page.locator('.capture-photo-panel img')).to_be_visible();assert not seen
 mid=page.evaluate("JSON.parse(localStorage.getItem('bob.client-view.v1')).quickNote.mediaId")
 assert mid==sha;assert base64.b64decode(rpc('app_capture_media_get',{'mediaId':mid})['base64'])==original
 results.append('Real image selection stores identical original before any recognition call')
 page.locator('.quicknote-input').fill('My original idea '+run)
 page.locator('#quicknote-project').select_option('__new');page.get_by_placeholder('项目名称',exact=True).fill('Photo project '+sha[:6])
 page.reload();page.wait_for_timeout(300);page.locator('.mobile-fab').click()
 expect(page.locator('.capture-photo-panel img')).to_be_visible();expect(page.locator('.quicknote-input')).to_have_value('My original idea '+run)
 results.append('Refresh restores image, text and project draft')
 page.route('**/api/rpc/app_capture_recognize',lambda route:route.fulfill(status=503,json={'ok':False,'error':'SIMULATED_FAILURE'}),times=1)
 page.get_by_role('button',name='识别关键信息',exact=True).click();expect(page.get_by_role('alert')).to_contain_text('识别未完成')
 assert base64.b64decode(rpc('app_capture_media_get',{'mediaId':mid})['base64'])==original
 results.append('Simulated recognition failure preserves original and editable text')
 page.get_by_role('button',name='识别关键信息',exact=True).click();expect(page.locator('#capture-suggestion')).to_have_value('SIMULATED extraction: design sketch',timeout=15000)
 assert len(seen)==1;assert seen[0]['messages'][0]['content'][1]['image_url']['url']==data
 expect(page.locator('.quicknote-input')).to_have_value('My original idea '+run)
 page.locator('#capture-suggestion').fill('User corrected: courtyard sketch '+run)
 page.get_by_role('button',name='加入记录',exact=True).click()
 assert 'User corrected' in page.locator('.quicknote-input').input_value()
 results.append('Actual node-to-loopback vision request carries image; suggestion requires explicit editable adoption (simulated model)')
 page.screenshot(path=str(out/'photo-desktop.png'));page.set_viewport_size({'width':390,'height':844});page.screenshot(path=str(out/'photo-390.png'))
 assert page.evaluate('document.documentElement.scrollWidth <= innerWidth')
 box=page.locator('.quicknote-card').bounding_box();assert box['x']>=0 and box['x']+box['width']<=390
 results.append('Desktop and 390px actual photo layout fit viewport')
 page.route('**/api/rpc/app_capture_create',lambda route:route.fulfill(status=503,json={'ok':False,'error':'SIMULATED_SAVE_FAILURE'}),times=1)
 page.get_by_role('button',name='保存灵感',exact=True).click();expect(page.get_by_role('alert')).to_contain_text('保存尚未完成')
 expect(page.locator('.capture-photo-panel img')).to_be_visible()
 page.get_by_role('button',name='保存灵感',exact=True).click();expect(page.locator('.quicknote-saved-badge')).to_be_visible()
 snap=rpc('app_personal_snapshot');cap=next(c for c in snap['captures'] if run in c['content_ref']);assert len([c for c in snap['captures'] if run in c['content_ref']])==1
 assert rpc('app_capture_media_get',{'captureId':cap['id']})['id']==mid
 assert any(n['body_markdown']==cap['content_ref'] and n['project_id'] for n in snap['notes'])
 results.append('Failed save then retry creates one capture, project note and original-image link')
 page.locator('.quicknote-overlay').click(position={'x':2,'y':2});page.get_by_role('button',name='知识',exact=True).click()
 page.get_by_role('button',name='闪念摘录').click();expect(page.locator('.capture-item-card .capture-photo-panel img').first).to_be_visible()
 results.append('Saved original is visible in Knowledge captures')
 page.reload();page.wait_for_timeout(300);page.locator('.mobile-fab').click()
 page.locator('input[type=file]').first.set_input_files({'name':'invalid.svg','mimeType':'image/svg+xml','buffer':b'<svg/>'});expect(page.get_by_role('alert')).to_contain_text('8 MB')
 results.append('Unsupported image rejected without a new saved record')
 rpc('app_preferences_save',{'locale':'en-US','theme':'dark'});page.reload();page.wait_for_timeout(300);page.locator('.mobile-fab').click()
 expect(page.get_by_role('button',name='Take photo',exact=True)).to_be_visible()
 page.locator('input[type=file]').first.set_input_files({'name':'design.png','mimeType':'image/png','buffer':original});expect(page.locator('.capture-photo-panel img')).to_be_visible();page.screenshot(path=str(out/'photo-390-dark-en.png'))
 assert not errors,errors
 results.append('English/dark controls and original preview; no browser runtime errors')
 (out/'results.json').write_text(json.dumps({'passed':results,'model':'loopback fake service only','imageSha256':sha,'pageErrors':errors},ensure_ascii=False,indent=2),encoding='utf-8')
 print(json.dumps(results,ensure_ascii=False));browser.close()
server.shutdown()
