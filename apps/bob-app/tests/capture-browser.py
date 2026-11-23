import argparse,json,uuid
from pathlib import Path
from playwright.sync_api import sync_playwright,expect
parser=argparse.ArgumentParser(description="Use an isolated bob-node test database, never production.")
parser.add_argument('--url',required=True)
parser.add_argument('--output',required=True,type=Path)
args=parser.parse_args()
base=args.url.rstrip('/')
out=args.output;out.mkdir(parents=True,exist_ok=True)
results=[]
with sync_playwright() as p:
 browser=p.chromium.launch()
 ctx=browser.new_context(viewport={'width':1180,'height':800})
 def rpc(name,args={}):
  res=ctx.request.post(base+'/api/rpc/'+name,data=args).json()
  assert res.get('ok'),res
  return res['result']
 suffix=uuid.uuid4().hex[:8];old='old-'+suffix
 rpc('app_conversation_create',{'conversationId':old,'title':'Old snake fixture','projectId':None})
 rpc('app_accept',{'requestId':'r-'+suffix,'messageId':'m-'+suffix,'sessionId':old,'content':'OLD_HISTORY_MUST_NOT_OPEN'})
 before=rpc('app_catalog')
 page=ctx.new_page();errors=[];page.on('pageerror',lambda e:errors.append(str(e)))
 page.goto(base+'/');page.wait_for_timeout(500)
 assert 'OLD_HISTORY_MUST_NOT_OPEN' not in page.locator('body').inner_text()
 assert len(rpc('app_catalog')['conversations'])==len(before['conversations'])
 results.append('First visit blank; no empty persistent conversation')
 main=page.locator('textarea:visible').first;main.fill('UI local save '+suffix)
 main.dispatch_event('input')
 page.get_by_role('button',name='保存消息',exact=True).click()
 page.wait_for_timeout(600)
 assert 'UI local save '+suffix in page.locator('body').inner_text()
 page.reload();page.wait_for_timeout(600)
 assert 'UI local save '+suffix in page.locator('body').inner_text()
 results.append('First send creates own conversation; refresh restores it')
 # Another browser client must not inherit this selection.
 other=browser.new_context();q=other.new_page();q.goto(base+'/');q.wait_for_timeout(500)
 assert 'UI local save '+suffix not in q.locator('body').inner_text();other.close()
 results.append('Independent client remains blank')
 page.get_by_role('button',name='灵光一现 · 快捷速记').click()
 note=page.get_by_placeholder('灵光一现，回车即记…');note.fill('Capture '+suffix)
 page.locator('#quicknote-project').select_option('__new')
 page.get_by_placeholder('项目名称',exact=True).fill('Idea project '+suffix)
 assert page.get_by_text('切换模型',exact=True).count()==0
 page.reload();page.wait_for_timeout(500)
 page.get_by_role('button',name='灵光一现 · 快捷速记').click()
 expect(page.get_by_placeholder('灵光一现，回车即记…')).to_have_value('Capture '+suffix)
 expect(page.get_by_placeholder('项目名称',exact=True)).to_have_value('Idea project '+suffix)
 results.append('Capture text and project selection survive reload')
 # Fail the actual request once: no success, draft remains, then retry.
 page.route('**/api/rpc/app_capture_create',lambda route:route.fulfill(status=503,json={'ok':False,'error':'TEST_UNAVAILABLE'}),times=1)
 page.get_by_role('button',name='保存灵感',exact=True).click()
 expect(page.get_by_role('alert')).to_contain_text('保存尚未完成')
 expect(page.get_by_placeholder('灵光一现，回车即记…')).to_have_value('Capture '+suffix)
 page.get_by_role('button',name='保存灵感',exact=True).click()
 expect(page.locator('.quicknote-saved-badge')).to_be_visible()
 snap=rpc('app_personal_snapshot');cat=rpc('app_catalog')
 assert len([c for c in snap['captures'] if c['content_ref']=='Capture '+suffix])==1
 project=next(x for x in cat['projects'] if x['title']=='Idea project '+suffix)
 assert any(n['project_id']==project['id'] and n['body_markdown']=='Capture '+suffix for n in snap['notes'])
 results.append('Failed save retains draft; retry commits one capture and associated project note')
 assert page.locator('.device-indicator svg').count()==0
 assert page.locator('.device-dot').evaluate('(e)=>getComputedStyle(e).boxShadow')=='none'
 results.append('Connection indicator is one dot without halo')
 page.get_by_placeholder('灵光一现，回车即记…').fill('Next idea')
 page.locator('#quicknote-project').select_option(project['id'])
 page.screenshot(path=str(out/'capture-desktop.png'))
 page.set_viewport_size({'width':390,'height':844})
 page.screenshot(path=str(out/'capture-390.png'))
 assert page.evaluate('document.documentElement.scrollWidth <= window.innerWidth')
 results.append('390px capture input/project selector without horizontal overflow')
 rpc('app_preferences_save',{'locale':'en-US','theme':'dark'})
 page.reload();page.wait_for_timeout(500)
 page.locator('.mobile-fab').click()
 expect(page.locator('.quicknote-input')).to_have_value('Next idea')
 assert page.evaluate('document.documentElement.scrollWidth <= window.innerWidth')
 assert page.locator('html').get_attribute('data-theme')=='dark'
 page.screenshot(path=str(out/'capture-390-dark-en.png'))
 results.append('English/dark capture draft and narrow layout retained')
 assert not errors,errors
 (out/'results.json').write_text(json.dumps({'passed':results,'pageErrors':errors},ensure_ascii=False,indent=2),encoding='utf-8')
 print(json.dumps(results,ensure_ascii=False))
 browser.close()
