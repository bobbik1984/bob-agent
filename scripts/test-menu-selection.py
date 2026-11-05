import threading,http.server,functools,json,argparse
from pathlib import Path
from playwright.sync_api import sync_playwright
parser=argparse.ArgumentParser(description='Browser regression for menu text selection; requires installed Playwright and Edge.')
parser.add_argument('--dist',required=True,type=Path)
options=parser.parse_args()
class Quiet(http.server.SimpleHTTPRequestHandler):
 def log_message(self,*args):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(Quiet,directory=str(options.dist.resolve())))
threading.Thread(target=server.serve_forever,daemon=True).start()
out=[]
try:
 with sync_playwright() as pw:
  for channel in [None,'msedge']:
   b=pw.chromium.launch(headless=True,**({'channel':channel} if channel else {}))
   for width in [390,1180]:
    page=b.new_page(viewport={'width':width,'height':900});errors=[];crashes=[]
    page.on('pageerror',lambda e:errors.append(str(e)));page.on('crash',lambda _:crashes.append(True))
    page.goto('http://127.0.0.1:'+str(server.server_port),wait_until='domcontentloaded')
    menu=page.locator('.conversation-menu');summary=page.locator('.conversation-menu > summary');summary.click()
    for row in page.locator('.quota-sub-row p').all():
     row.dblclick(timeout=5000);assert page.evaluate('window.getSelection().toString().length')>0
     rect=row.bounding_box();page.mouse.move(rect['x']+1,rect['y']+rect['height']/2);page.mouse.down();page.mouse.move(rect['x']+rect['width']-1,rect['y']+rect['height']/2,steps=12);page.mouse.up();page.keyboard.press('Control+c')
     assert menu.get_attribute('open') is not None
    page.locator('.quota-other-summary').click();assert page.locator('.quota-other-body').is_visible()
    page.keyboard.press('Escape');assert menu.get_attribute('open') is None
    summary.click();page.locator('.page-name').click();assert menu.get_attribute('open') is None
    summary.click();page.locator('.device-indicator').focus();assert menu.get_attribute('open') is None
    out.append({'browser':channel or 'chromium','width':width,'selectCopy':True,'otherUsage':True,'escapeOutsideFocusClose':True,'errors':errors,'crashes':crashes});page.close()
   b.close()
 print(json.dumps(out))
finally:server.shutdown()
