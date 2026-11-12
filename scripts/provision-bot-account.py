"""Owner-side local provisioning; never sends credentials or contacts a node.

scope.json: {"projects":["approved-id"],"sessions":[{"connection_id":"...",
"harness":"...","session_id":"..."}],"organize":false}
The secret file is for a private connector setup, never for Git/chat/log output.
"""
import argparse, hashlib, json, os, re, secrets, subprocess, time
from pathlib import Path

def private_file(path, text):
    path=Path(path)
    fd=os.open(path,os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o600)
    try:
        if os.name=='nt':
            who=subprocess.check_output(['whoami'],text=True,encoding='utf-8').strip()
            subprocess.run(['icacls',str(path),'/inheritance:r','/grant:r',who+':F'],check=True,capture_output=True)
        with os.fdopen(fd,'w',encoding='utf-8') as f:
            fd=None;f.write(text);f.flush();os.fsync(f.fileno())
    finally:
        if fd is not None: os.close(fd)

def provision(data_dir,scope,secret_file,username='grok_bot',hours=24):
    if not re.fullmatch(r'[a-z][a-z0-9_]{2,63}',username) or (hours is not None and not 1<=hours<=168):
        raise ValueError('Invalid username or lifetime')
    if not {'projects','sessions','organize'} <= set(scope) or set(scope)-{'projects','sessions','organize','todos_read','todos_create','all_projects','todos_unassigned','todos_update'} or type(scope['organize']) is not bool:
        raise ValueError('Explicit project/session scope and organize flag required')
    if not isinstance(scope['projects'],list) or (not scope['projects'] and scope.get('all_projects') is not True) or any(not isinstance(x,str) or not x or x=='*' for x in scope['projects']):
        raise ValueError('Explicit project IDs required; no wildcard')
    if not isinstance(scope['sessions'],list): raise ValueError('Session list required')
    for s in scope['sessions']:
        if not isinstance(s,dict) or set(s)!={'connection_id','harness','session_id'} or any(not isinstance(x,str) or not x or x=='*' for x in s.values()):
            raise ValueError('Explicit session identity required')
    scope=dict(scope)
    for permission in ('todos_read','todos_create','all_projects','todos_unassigned','todos_update'):
        scope.setdefault(permission,False)
        if type(scope[permission]) is not bool: raise ValueError('Todo permissions must be explicit booleans')
    folder=Path(data_dir);folder.mkdir(parents=True,exist_ok=True)
    dest=folder/'bot-grants.json'
    # Initial provisioning only. Updating/revoking is an owner action; no public admin endpoint.
    if dest.exists(): raise FileExistsError('Grant file exists; refusing to replace accounts')
    token='bob_bot_'+secrets.token_urlsafe(32)
    grant=dict(username=username,token_hash=hashlib.sha256(token.encode()).hexdigest(),expires_at=int(time.time()*1000)+hours*3600000 if hours is not None else 0,no_expiry=hours is None,revoked=False,**scope)
    private_file(secret_file,token+'\n')
    private_file(dest,json.dumps([grant],ensure_ascii=False,indent=2)+'\n')
    return {'username':username,'expires_at':grant['expires_at'],'organize':scope['organize'],'execution':False}

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--data-dir',required=True);p.add_argument('--scope',required=True);p.add_argument('--secret-file',required=True);p.add_argument('--username',default='grok_bot');p.add_argument('--hours',type=int,default=24)
    p.add_argument('--no-expiry',action='store_true',help='Explicit owner-approved lifetime until revocation')
    a=p.parse_args()
    print(json.dumps(provision(a.data_dir,json.loads(Path(a.scope).read_text(encoding='utf-8')),a.secret_file,a.username,None if a.no_expiry else a.hours)))
