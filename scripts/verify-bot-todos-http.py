import os,subprocess,tempfile,socket,time,json,secrets,hashlib,sqlite3,urllib.request,urllib.error
from pathlib import Path
import argparse
parser=argparse.ArgumentParser(description='Isolated Bot todo HTTP verification; never uses production data')
parser.add_argument('--binary',required=True)
binary=Path(parser.parse_args().binary).resolve()
assert binary.is_file(), 'Build the candidate first'
folder=Path(tempfile.mkdtemp(prefix='bob-todo-http-'));data=folder/'data';data.mkdir()
with socket.socket() as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
base=f'http://127.0.0.1:{port}';token='bob_bot_'+secrets.token_urlsafe(32)
checks=[];proc=None
def http(path,body,auth=None):
 h={'Content-Type':'application/json','X-Bob-Rpc':'1'}
 if auth:h['Authorization']='Bearer '+auth
 req=urllib.request.Request(base+path,data=json.dumps(body).encode(),headers=h)
 try:
  with urllib.request.urlopen(req,timeout=15) as r:return r.status,json.load(r)
 except urllib.error.HTTPError as e:return e.code,json.load(e)
def owner(name,args={}):
 status,out=http('/api/rpc/'+name,args);assert status==200 and out.get('ok'),(status,out);return out['result']
def call(name,args={},auth=token):
 return http('/api/bot/v1/mcp',{'jsonrpc':'2.0','id':1,'method':'tools/call','params':{'name':name,'arguments':args}},auth)
def result(out):
 status,v=out;assert status==200 and not v['result']['isError'],v;return json.loads(v['result']['content'][0]['text'])
def denied(out):
 status,v=out;assert status in (401,403) or v.get('result',{}).get('isError') is True,v
def start():
 global proc
 env=os.environ.copy()
 for k in list(env):
  if k.startswith('BOB_'):env.pop(k)
 proc=subprocess.Popen([str(binary),'--bind',f'127.0.0.1:{port}','--data-dir',str(data),'--no-qr'],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
 for _ in range(120):
  assert proc.poll() is None,'candidate exited'
  try:owner('app_catalog');return
  except (OSError,AssertionError):time.sleep(.25)
 raise TimeoutError('candidate readiness')
def stop():
 global proc
 if proc:proc.terminate();proc.wait(timeout=15);proc=None
grant={'username':'grok_bot_test','token_hash':hashlib.sha256(token.encode()).hexdigest(),'expires_at':int(time.time()*1000)+3600000,'revoked':False,'projects':['allowed'],'sessions':[],'organize':False,'todos_read':True,'todos_create':True}
def save():
 p=data/'bot-grants.next';p.write_text(json.dumps([grant]),encoding='utf-8');p.replace(data/'bot-grants.json')
try:
 start()
 for pid in ['allowed','private']:owner('app_project_create',{'projectId':pid,'title':pid})
 save()
 args={'project_id':'allowed','request_id':'http-1','title':'HTTP todo check','start_at':int(time.time()*1000),'notes':'isolated validation only'}
 denied(call('bob_todos',{'project_id':'allowed'},auth=None));checks.append('missing_auth_denied')
 first=result(call('bob_todo_create',args));tid=first['todo']['id'];assert first['replayed'] is False
 again=result(call('bob_todo_create',args));assert again['todo']['id']==tid and again['replayed'] is True
 denied(call('bob_todo_create',dict(args,title='Changed')));checks.append('retry_and_conflicting_payload')
 assert len(result(call('bob_todos',{'project_id':'allowed'}))['todos'])==1
 snapshot=owner('app_personal_snapshot');assert len([s for s in snapshot['schedules'] if s['id']==tid])==1;checks.append('same_ledger_as_web_personal_snapshot')
 denied(call('bob_todos',{'project_id':'private'}));denied(call('bob_todo_create',dict(args,request_id='forbidden',project_id='private')))
 denied(call('shell',{'command':'not executed'}));assert http('/api/rpc/app_catalog',{},token)[0]==403;checks.append('project_execution_owner_rpc_denied')
 grant['todos_create']=False;save();denied(call('bob_todo_create',dict(args,request_id='readonly')));result(call('bob_todos',{'project_id':'allowed'}));checks.append('read_only_grant')
 grant['todos_create']=True;save();stop();start()
 replay=result(call('bob_todo_create',args));assert replay['replayed'] and replay['todo']['id']==tid
 assert len(result(call('bob_todos',{'project_id':'allowed'}))['todos'])==1;checks.append('process_restart_persistence')
 # Even deletion by the owner cannot turn a retried creation into a second write.
 owner('app_schedule_delete',{'id':tid});replay=result(call('bob_todo_create',args));assert replay['replayed']
 assert not result(call('bob_todos',{'project_id':'allowed'}))['todos'];checks.append('deleted_todo_retry_does_not_recreate')
 # Independent todos use the same owner ledger, with optional independent dates.
 grant['todos_unassigned']=True;save()
 independent={'request_id':'independent','title':'No dates or project'}
 item=result(call('bob_todo_create',independent))['todo'];assert item['project_id'] is None and item['start_at'] is None and item['due_at'] is None
 assert result(call('bob_todo_create',independent))['replayed']
 deadline=result(call('bob_todo_create',{'request_id':'deadline','title':'Only a deadline','due_at':2000}))['todo']
 assert deadline['start_at'] is None and deadline['due_at']==2000
 assert len(result(call('bob_todos',{'project_id':None}))['todos'])==2
 assert len(result(call('bob_todos'))['todos'])==2
 assert not result(call('bob_todos',{'project_id':'allowed'}))['todos']
 edited=owner('app_schedule_edit',{'id':deadline['id'],'expectedVersion':1,'title':'Deadline unchanged','projectId':None,'startAt':1000,'endAt':2000,'notes':''})
 assert edited['end_at']==2000
 cleared=owner('app_schedule_edit',{'id':deadline['id'],'expectedVersion':2,'title':'Cleared','projectId':None,'startAt':None,'endAt':None,'notes':''})
 assert cleared['start_at']==0 and cleared['end_at'] is None
 owner_item=owner('app_schedule_create',{'id':'owner-independent','title':'Owner no dates','kind':'todo','projectId':None,'startAt':None,'endAt':None})
 assert owner_item['start_at']==0 and owner_item['project_id'] is None
 checks.append('independent_optional_dates_owner_edit_and_filters')
 stop();start()
 assert len(result(call('bob_todos',{'project_id':None}))['todos'])==3
 checks.append('independent_restart_persistence')
 # Date-only patch, permission isolation, CAS and original receipt after restart.
 patch={'id':item['id'],'request_id':'semantic-update','expected_version':1,'timing':{'due_date':'2026-11-15','time_zone':'Asia/Shanghai','source_text':'11/15前'}}
 denied(call('bob_todo_update',patch));grant['todos_update']=True;save()
 updated=result(call('bob_todo_update',patch));assert updated['todo']['id']==item['id'] and updated['todo']['version']==2
 assert updated['todo']['timing']['due_date']=='2026-11-15' and updated['todo']['due_at'] is None
 assert result(call('bob_todo_update',patch))['replayed']
 denied(call('bob_todo_update',dict(patch,request_id='stale')))
 denied(call('bob_todo_update',dict(patch,request_id='scope',expected_version=2,project_id='private')))
 snap=owner('app_personal_snapshot');assert next(v for v in snap['schedules'] if v['id']==item['id'])['timing']['due_date']=='2026-11-15'
 stop();start();assert result(call('bob_todo_update',patch))['replayed']
 assert len(result(call('bob_todos',{'project_id':None}))['todos'])==3
 checks.append('semantic_update_scope_cas_same_ledger_retry_restart')
 grant['revoked']=True;save();assert call('bob_todos',{'project_id':'allowed'})[0]==401;checks.append('revoked_token_denied')
 grant['revoked']=False;grant['expires_at']=1;save();assert call('bob_todos',{'project_id':'allowed'})[0]==401;checks.append('expired_token_denied')
 audit=(data/'bot-audit.jsonl').read_text(encoding='utf-8');assert token not in audit and args['notes'] not in audit;checks.append('audit_no_secret_or_body')
 with sqlite3.connect(data/'kernel.sqlite') as db:
  assert db.execute('pragma quick_check').fetchone()[0]=='ok'
  assert db.execute('select count(*) from bot_todo_receipts').fetchone()[0]==4
 checks.append('sqlite_receipt_integrity')
 report={'checks':checks,'passed':len(checks),'production_changed':False,'candidate_stopped':True,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}
 (folder/'result.json').write_text(json.dumps(report,indent=2),encoding='utf-8');print(json.dumps(dict(report,evidence=str(folder/'result.json'))))
finally:
 stop()
 # Revoke the local test credential even after failed checks.
 grant['revoked']=True;grant['expires_at']=1;save()
