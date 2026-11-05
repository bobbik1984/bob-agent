#!/usr/bin/env python3
"""Official quota adapters. No OAuth/token extraction and no model prompts.

Run `collect --config CONFIG` from a timer. `capture` is an official statusLine
sink. Configuration is trusted operator-owned JSON, never a browser input.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import selectors
import shlex
import signal
import subprocess
import sys
import tempfile
import time
from datetime import datetime

PROVIDERS = ('codex', 'antigravity', 'claude')

def atomic(path, data):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    with tempfile.NamedTemporaryFile('w', encoding='utf-8', dir=path.parent, delete=False) as f:
        json.dump(data, f, allow_nan=False)
        name = f.name
    os.chmod(name, 0o600)
    os.replace(name, path)

def read_json(path):
    path = Path(path)
    if path.stat().st_size > 65536:
        raise ValueError('oversize')
    return json.loads(path.read_text(encoding='utf-8'))

def reset_seconds(v):
    if isinstance(v, str):
        try:return int(datetime.fromisoformat(v.replace('Z', '+00:00')).timestamp())
        except ValueError:return None
    return int(v) if isinstance(v, (float, int)) and math.isfinite(v) and v > 0 else None

def window(key, remaining, reset):
    if isinstance(remaining, bool) or not isinstance(remaining, (float, int)) or not math.isfinite(remaining) or not 0 <= remaining <= 100:
        return None
    return {'id': key, 'remaining_percent': remaining, 'resets_at': reset_seconds(reset)}

def normalize(provider, payload, node, now=None):
    windows = []
    if provider == 'codex':
        q = payload.get('rateLimits') or {}
        for key in ['primary', 'secondary']:
            w = q.get(key) or {}
            if not isinstance(w.get('usedPercent'), (float, int)):continue
            label = {10080:'weekly', 300:'five_hour'}.get(w.get('windowDurationMins'), key)
            windows.append(window(label, 100-w['usedPercent'], w.get('resetsAt')))
    elif provider == 'antigravity':
        for key, q in (payload.get('quota') or {}).items():
            if not isinstance(q, dict) or q.get('disabled'):continue
            fraction=q.get('remaining_fraction')
            if isinstance(fraction,(float,int)):
                windows.append(window(key, fraction*100, q.get('reset_time')))
    elif provider == 'claude':
        for key, label in [('five_hour','five_hour'),('seven_day','weekly')]:
            q=(payload.get('rate_limits') or {}).get(key) or {}
            if isinstance(q.get('used_percentage'),(float,int)):
                windows.append(window(label,100-q['used_percentage'],q.get('resets_at')))
    windows=[w for w in windows if w]
    if not windows:raise ValueError('no_official_quota')
    result={'source_kind':'codex_app_server' if provider=='codex' else 'official_statusline',
            'source_node':node[:64], 'observed_at':int(now if now is not None else time.time()),'windows':windows}
    # Account scope stays opaque. Never export email, transcript, cwd, tokens or session IDs.
    if isinstance(payload.get('email'),str):
        result['account_ref']=hashlib.sha256((provider+':'+payload['email']).encode()).hexdigest()
    return result

def stop(process):
    if process.poll() is not None:return
    # Every collector-owned CLI has its own group; never signal a pre-existing daemon.
    os.killpg(process.pid, signal.SIGTERM)
    try:process.wait(timeout=4)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid,signal.SIGKILL);process.wait()

def codex(source):
    p=subprocess.Popen(source['command']+['app-server','--stdio'],stdin=subprocess.PIPE,
                       stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,bufsize=1,start_new_session=True)
    sel=selectors.DefaultSelector();sel.register(p.stdout,selectors.EVENT_READ)
    def send(data):p.stdin.write(json.dumps(data)+'\n');p.stdin.flush()
    def receive(request_id):
        until=time.monotonic()+25
        while time.monotonic()<until:
            if not sel.select(timeout=1):continue
            line=p.stdout.readline()
            if not line:break
            try:v=json.loads(line)
            except ValueError:continue
            if v.get('id')==request_id:
                if 'result' not in v:raise ValueError('provider_error')
                return v['result']
        raise TimeoutError('provider_timeout')
    try:
        send({'id':1,'method':'initialize','params':{'clientInfo':{'name':'bob_quota','version':'1.0'},'capabilities':{}}})
        receive(1);send({'method':'initialized'})
        send({'id':2,'method':'account/rateLimits/read'})
        return normalize('codex',receive(2),source['source_node'])
    finally:sel.close();stop(p)

def antigravity(source):
    import pty,select,fcntl,termios,struct
    with tempfile.TemporaryDirectory(prefix='bob-quota-') as folder:
        output=Path(folder)/'quota.json'
        command=shlex.join([sys.executable,str(Path(__file__).resolve()),'capture','--provider','antigravity','--node',source['source_node'],'--output',str(output)])
        settings=json.dumps({'statusLine':{'type':'command','command':command}})
        master,slave=pty.openpty()
        fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',30,120,0,0))
        env=os.environ.copy();env['TERM']='xterm-256color'
        p=subprocess.Popen(source['command']+['--settings',settings],stdin=slave,stdout=slave,stderr=slave,
                           cwd=folder,env=env,start_new_session=True)
        os.close(slave)
        try:
            until=time.monotonic()+25
            while time.monotonic()<until:
                if output.exists():return read_json(output)
                if p.poll() is not None:break
                if select.select([master],[],[],.2)[0]:
                    try:os.read(master,65536)  # Drain only. Never parse or persist the terminal screen.
                    except OSError:break
            raise TimeoutError('provider_timeout')
        finally:stop(p);os.close(master)

def snapshot(source):
    p=subprocess.run(source['command'],capture_output=True,timeout=20)
    if p.returncode or len(p.stdout)>65536:raise ValueError('source_unavailable')
    result=json.loads(p.stdout)
    if result.get('source_kind')!='official_statusline' or not result.get('observed_at') or not result.get('windows'):
        raise ValueError('invalid_snapshot')
    # Preserve the callback timestamp; polling a file is not a new provider observation.
    result['source_node']=source['source_node']
    return result

def collect(config):
    import fcntl
    path=Path(config['output'])
    path.parent.mkdir(parents=True,exist_ok=True)
    with path.with_suffix('.lock').open('a',encoding='utf-8') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        try:cache=read_json(path)
        except (OSError,ValueError):cache={'version':1,'providers':{}}
        for source in config['sources']:
            provider=source['provider']
            if provider not in PROVIDERS:raise ValueError('unsupported_provider')
            try:
                adapter={'codex':codex,'antigravity':antigravity,'snapshot':snapshot}[source['adapter']]
                cache['providers'][provider]=adapter(source)
                print(provider+': observed',flush=True)
            except Exception as error:
                previous=cache['providers'].get(provider,{})
                previous['last_error']=type(error).__name__
                previous['last_attempt_at']=int(time.time())
                cache['providers'][provider]=previous
                print(provider+': unavailable; previous observation retained',flush=True)
            atomic(path,cache)

def main():
    parser=argparse.ArgumentParser(description=__doc__);sub=parser.add_subparsers(dest='mode',required=True)
    c=sub.add_parser('collect');c.add_argument('--config',required=True,type=Path)
    c=sub.add_parser('capture');c.add_argument('--provider',choices=PROVIDERS,required=True);c.add_argument('--node',required=True);c.add_argument('--output',required=True,type=Path)
    c=sub.add_parser('export');c.add_argument('--file',required=True,type=Path)
    a=parser.parse_args()
    if a.mode=='collect':collect(read_json(a.config))
    elif a.mode=='export':print(json.dumps(read_json(a.file)))
    else:
        try:
            raw=sys.stdin.read(1048577)
            if len(raw)>1048576:raise ValueError('oversize')
            result=normalize(a.provider,json.loads(raw),a.node)
            atomic(a.output,result)
            print(' · '.join(w['id']+': '+str(round(w['remaining_percent']))+'% left' for w in result['windows']))
        except (ValueError,TypeError,KeyError):pass  # Missing field is not a new observation.

if __name__=='__main__':main()
