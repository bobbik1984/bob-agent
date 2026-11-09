from collections import deque
from contextlib import closing
from datetime import datetime
import json, os, re, shutil, sqlite3, subprocess, sys, time, uuid
from pathlib import Path

MAX_ROWS = 200
REQUIRED = {'conversation_id', 'title', 'workspace_uris', 'last_modified_time', 'step_count', 'status'}
SKIP_TYPES = (
    '"file-history-snapshot"', '"api-request"', '"cost-state"', '"system"',
    '"attachment"', '"bridge-session"', '"permission-mode"', '"atis-latch"',
    '"session-owner"', '"tools-used"', '"mode"'
)
_now = lambda: int(time.time() * 1000)

def _j(s):
    try:
        return json.loads(s)
    except Exception:
        return {}

def _mk_row(rid, sid, h, lbl, kind, title, ws, ts, steps=1, st='completed', res=False, fp='', ls=''):
    return {
        'id': rid, 'nativeSessionId': sid, 'harness': h, 'toolLabel': lbl,
        'sourceKind': kind, 'title': title, 'workspaceUris': [ws], 'updatedAt': ts,
        'stepCount': max(steps, 1), 'nativeStatus': st or 'completed',
        'firstPrompt': (fp or '')[:120], 'lastSnippet': (ls or '')[:120],
        'localHistoryPresent': True, 'historyReadable': True, 'historyReason': None,
        'resumeVerified': res,
    }

def discover(data_dir, limit=100):
    if isinstance(limit, bool) or not isinstance(limit, int) or not 1 <= limit <= MAX_ROWS:
        raise ValueError('AGY_INDEX_LIMIT_INVALID')
    root = Path(data_dir).resolve(strict=True)
    idx = root / 'conversation_summaries.db'
    if idx.is_symlink() or not idx.is_file():
        raise ValueError('AGY_INDEX_UNAVAILABLE')
    with closing(sqlite3.connect(idx.as_uri() + '?mode=ro', uri=True, timeout=2)) as db:
        db.execute('PRAGMA query_only=ON')
        b = [0]
        db.set_progress_handler(lambda: (b.__setitem__(0, b[0] + 1) or b[0] > 2000), 1000)
        cols = {r[1] for r in db.execute('PRAGMA table_info(conversation_summaries)')}
        if not REQUIRED <= cols:
            raise ValueError('AGY_INDEX_SCHEMA_UNSUPPORTED')
        pred = "WHERE COALESCE(parent_conversation_id,'')=''" if 'parent_conversation_id' in cols else ''
        prev_col = "substr(preview,1,240)" if 'preview' in cols else "''"
        rows = db.execute(
            'SELECT conversation_id,substr(title,1,512),substr(workspace_uris,1,16385),'
            'last_modified_time,step_count,status,' + prev_col + ' FROM conversation_summaries ' + pred +
            ' ORDER BY last_modified_time DESC,conversation_id LIMIT ?', (limit + 1,)
        ).fetchall()
    out = []
    for sid, title, enc, mod, cnt, st, prev in rows[:limit]:
        try:
            if str(uuid.UUID(sid)) != sid:
                raise ValueError()
            uris = json.loads(enc) if enc and str(enc).strip() else []
            if not isinstance(uris, list) or len(uris) > 32 or any(not isinstance(u, str) for u in uris):
                raise ValueError()
        except (ValueError, TypeError, AttributeError):
            raise ValueError('AGY_INDEX_ROW_INVALID') from None
        h = root / 'conversations' / (sid + '.db')
        tr_full = root / 'brain' / sid / '.system_generated' / 'logs' / 'transcript_full.jsonl'
        tr_comp = root / 'brain' / sid / '.system_generated' / 'logs' / 'transcript.jsonl'
        has_tr = (tr_full.is_file() and not tr_full.is_symlink()) or (tr_comp.is_file() and not tr_comp.is_symlink())
        loc = has_tr or (h.is_file() and not h.is_symlink() and h.resolve().is_relative_to(root))
        prev_clean = _clean(prev) if prev else ''
        eff_title = title if (title and str(title).strip()) else (prev_clean or title)
        fp, ls = prev_clean, ''
        if has_tr:
            tf = tr_full if (tr_full.is_file() and not tr_full.is_symlink()) else tr_comp
            fp_tr, ls_tr = _seek_digest(tf, 'agy')
            fp = fp_tr or fp
            ls = ls_tr or ls
        out.append({
            'id': f'agy:{sid}', 'nativeSessionId': sid, 'harness': 'agy', 'toolLabel': 'AGY',
            'title': eff_title, 'workspaceUris': uris,
            'updatedAt': mod, 'stepCount': cnt, 'nativeStatus': st, 'sourceKind': 'agy_index',
            'firstPrompt': (fp or '')[:120], 'lastSnippet': (ls or '')[:120],
            'localHistoryPresent': loc, 'historyReadable': has_tr,
            'historyReason': None if has_tr else ('AGY_HISTORY_CODEC_UNSUPPORTED' if loc else 'AGY_HISTORY_NOT_LOCAL'),
            'resumeVerified': has_tr,
        })
    return {'schemaVersion': 1, 'sessions': out, 'hasMore': len(rows) > limit,
            'authentication': 'unverified', 'source': 'local_readonly_index'}

def _ts(v, fb=0):
    if isinstance(v, (int, float)):
        return int(v * 1000) if 0 < v < 1e11 else int(v)
    if isinstance(v, str) and v.strip():
        try:
            return int(datetime.fromisoformat(v.strip().replace('Z', '+00:00')).timestamp() * 1000)
        except Exception:
            pass
    return fb

def _clean(text, fb=''):
    if not text:
        return fb
    t = re.sub(r'<(?:environment_context|skills_instructions|plugins_instructions)>[\s\S]*?</(?:environment_context|skills_instructions|plugins_instructions)>', ' ', str(text))
    t = re.sub(r'<[^>]+>|END_TIME=\$\(date[^\n]*\n?|DURATION=\$\(\([^\n]*\n?|echo\s+"=== TURN[^\n]*\n?|git status --short\s*', ' ', t)
    for ln in t.splitlines():
        s = ln.strip().lstrip('?#*- ')
        if len(s) >= 3 and not s.startswith(('Project conversation context', 'Current request:', 'Recent messages:', '/', '---', '```')) and s.count('?') <= len(s) * 0.35:
            return s[:110]
    return fb

def _iter_jsonl(f, max_lines=0):
    try:
        with open(f, 'r', encoding='utf-8', errors='replace') as fp:
            for i, ln in enumerate(fp):
                if max_lines and i > max_lines:
                    break
                h = ln[:160]
                if any(k in h for k in SKIP_TYPES):
                    continue
                if len(ln) > 65536 and ('"tool_result"' in ln[:600] or '"base64"' in ln[:1200] or '"type":"user"' in h):
                    continue
                o = _j(ln)
                if isinstance(o, dict) and o:
                    yield i, o
    except OSError:
        return

def _seek_digest(f, harness):
    fp, ls = '', ''
    try:
        sz = f.stat().st_size
        with open(f, 'rb') as fh:
            head_b = fh.read(24576)
            tail_b = b''
            if sz > 24576:
                fh.seek(max(24576, sz - 32768))
                tail_b = fh.read(32768)
        lines = head_b.decode('utf-8', errors='replace').splitlines()
        if tail_b:
            t_lines = tail_b.decode('utf-8', errors='replace').splitlines()
            lines.extend(t_lines[1:] if len(t_lines) > 1 else t_lines)
        for ln in lines:
            if not ln or len(ln) > 65536 or any(k in ln[:160] for k in SKIP_TYPES):
                continue
            o = _j(ln)
            if not isinstance(o, dict) or not o:
                continue
            if harness == 'agy':
                tp = o.get('type')
                if tp == 'USER_INPUT':
                    c = str(o.get('content') or '')
                    m = re.search(r'<USER_REQUEST>\s*([\s\S]*?)\s*</USER_REQUEST>', c)
                    u = _clean(m.group(1) if m else c)
                    if u:
                        fp = fp or u
                        ls = u
                elif tp == 'PLANNER_RESPONSE':
                    a = _clean(o.get('content') or o.get('thinking') or '')
                    if a:
                        ls = a
            elif harness == 'claude':
                if o.get('isMeta') or o.get('isSidechain'):
                    continue
                t = o.get('type')
                msg = o.get('message') if isinstance(o.get('message'), dict) else {}
                c = msg.get('content')
                if t == 'user':
                    u = c if isinstance(c, str) else '\n'.join(b.get('text', '') for b in c if isinstance(b, dict) and b.get('type') == 'text') if isinstance(c, list) else ''
                    if '<command-name>' not in u and '<local-command' not in u:
                        cl = _clean(u)
                        if cl and cl != 'ping':
                            fp = fp or cl
                            ls = cl
                elif t == 'assistant' and isinstance(c, list):
                    a = _clean('\n'.join(b.get('text', '') for b in c if isinstance(b, dict) and b.get('type') == 'text'))
                    if a:
                        ls = a
            elif harness == 'pi':
                if o.get('type') == 'message':
                    m = o.get('message') if isinstance(o.get('message'), dict) else {}
                    txt = _clean('\n'.join(p.get('text', '') for p in (m.get('content') or []) if isinstance(p, dict) and p.get('type') == 'text'))
                    if txt:
                        if m.get('role') == 'user':
                            fp = fp or txt
                        ls = txt
            elif harness == 'codex':
                if o.get('type') == 'response_item':
                    p = o.get('payload') if isinstance(o.get('payload'), dict) else {}
                    c = p.get('content')
                    raw_t = '\n'.join(x.get('text', '') for x in c if isinstance(x, dict)) if isinstance(c, list) else str(c or '')
                    if '<environment_context>' not in raw_t and 'The following is the Codex agent history added since your last approval assessment' not in raw_t:
                        txt = _clean(raw_t)
                        if txt:
                            if p.get('role') == 'user':
                                fp = fp or txt
                            ls = txt
    except OSError:
        pass
    return fp[:110], ls[:110]

def discover_claude(d, limit=30):
    pr = Path(d) / 'projects' if d else None
    if not pr or not pr.is_dir():
        return []
    out = []
    for f in sorted(pr.glob('*/*.jsonl'), key=lambda p: p.stat().st_mtime, reverse=True)[:limit]:
        if '-tmp-bob-claude-probe' in str(f):
            continue
        sid, ms, title, cwd, cnt = f.stem, int(f.stat().st_mtime * 1000), None, None, 0
        for i, o in _iter_jsonl(f, 250):
            if title and cwd and i > 60:
                break
            if o.get('cwd') and (not cwd or cwd in ('/home/bobbik', '/home/ubuntu')):
                cwd = o['cwd']
            t = o.get('type')
            if t == 'ai-title' and o.get('aiTitle'):
                title = _clean(o['aiTitle']) or title
            elif t == 'queue-operation' and o.get('operation') == 'enqueue' and o.get('content') and not title:
                title = _clean(o['content'])
            elif t == 'user' and not o.get('isMeta'):
                c = (o.get('message') or {}).get('content') if isinstance(o.get('message'), dict) else None
                if isinstance(c, str) and '<command-name>' not in c and '<local-command' not in c:
                    cnt += 1
                    title = title or _clean(c)
            elif t == 'assistant':
                cnt += 1
        ws = cwd or str(f.parent.name)
        fp, ls = _seek_digest(f, 'claude')
        out.append(_mk_row(f'claude-jsonl:{sid}', sid, 'claude', 'Claude', 'claude_native',
                           title or fp or f'Claude · {Path(ws).name} ({sid[:8]})', ws, ms, cnt, 'completed', True, fp, ls))
    return out

def discover_pi(d, limit=30):
    r = Path(d) if d else None
    if not r or not r.is_dir():
        return []
    out = []
    for f in sorted(r.glob('**/*.jsonl'), key=lambda p: p.stat().st_mtime, reverse=True)[:limit]:
        ms, sid, cwd, title, cnt = int(f.stat().st_mtime * 1000), f.stem.split('_')[-1], '', None, 0
        for i, o in _iter_jsonl(f, 250):
            if i > 200 and title and cwd:
                break
            if o.get('type') == 'session':
                sid, cwd = o.get('id') or sid, o.get('cwd') or cwd
            elif o.get('type') == 'message':
                cnt += 1
                m = o.get('message', {})
                if m.get('role') in ('user', 'assistant') and not title:
                    txt = '\n'.join(p.get('text', '') for p in m.get('content', []) if isinstance(p, dict) and p.get('type') == 'text')
                    title = _clean(txt) or title
        wn = Path(cwd).name if cwd else f.parent.name.strip('-')
        fp, ls = _seek_digest(f, 'pi')
        out.append(_mk_row(f'pi:{sid}', sid, 'pi', 'Pi', 'pi_native',
                           title or fp or f'Pi · {wn} ({sid[:8]})', cwd or wn, ms, cnt, 'completed', False, fp, ls))
    return out

def discover_codex(d, limit=20):
    sr = (Path(d) / 'sessions') if d else None
    if not sr or not sr.is_dir():
        return []
    out, seen = [], set()
    for f in sorted(sr.glob('**/*.jsonl'), key=lambda p: p.stat().st_mtime, reverse=True):
        if len(out) >= limit:
            break
        ms, sid, cwd, guard, ut, at, cnt = int(f.stat().st_mtime * 1000), None, '', False, None, None, 0
        for _, o in _iter_jsonl(f, 120):
            if o.get('type') == 'session_meta':
                p = o.get('payload', {})
                sid, cwd, guard = p.get('id') or p.get('session_id'), p.get('cwd') or '', p.get('thread_source') == 'guardian_review'
            elif o.get('type') == 'response_item':
                cnt += 1
                p = o.get('payload', {})
                c = p.get('content')
                txt = '\n'.join(x.get('text', '') for x in c if isinstance(x, dict)) if isinstance(c, list) else str(c or '')
                if p.get('role') == 'user' and '<environment_context>' not in txt and 'The following is the Codex agent history added since your last approval assessment' not in txt and not ut:
                    ut = _clean(txt)
                elif p.get('role') == 'assistant' and not at:
                    at = _clean(txt)
        if guard:
            continue
        fp, ls = _seek_digest(f, 'codex')
        u_prompt = ut or fp or ''
        if cwd in ('/home/bobbik', '/home/ubuntu', '') and u_prompt:
            pm = re.search(r'(/home/(?:bobbik|ubuntu)/[A-Za-z0-9_./-]+)', u_prompt)
            if pm:
                cwd = pm.group(1).rstrip('.,;:)')
        if cwd and cwd not in ('/home/bobbik', '/home/ubuntu') and cwd in seen:
            continue
        if not sid:
            m = re.search(r'([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})', f.name)
            sid = m.group(1) if m else f.stem
        if cwd and cwd not in ('/home/bobbik', '/home/ubuntu'):
            seen.add(cwd)
        wn = Path(cwd).name if cwd else 'codex'
        out.append(_mk_row(f'codex:{sid}', sid, 'codex', 'Codex', 'codex_native',
                           ut or fp or at or f'Codex · {wn}', cwd or str(f.parent), ms, max(cnt, 1), 'completed', False, fp or ut, ls or at))
    return out

def discover_control_plane(dbs, harnesses=None, limit=40):
    if not dbs:
        return []
    allow = set(harnesses or ['dsh', 'pi', 'codex', 'agy'])
    lmap = {'dsh': 'DSH', 'pi': 'Pi', 'codex': 'Codex', 'agy': 'AGY', 'claude': 'Claude'}
    seen, out = set(), []
    for dbp in dbs:
        p = Path(dbp)
        if not p.is_file():
            continue
        try:
            with closing(sqlite3.connect(p.as_uri() + '?mode=ro', uri=True, timeout=2)) as conn:
                conn.execute('PRAGMA query_only=ON')
                rows = conn.execute(
                    "SELECT e.agent_type, t.id, t.title, t.status, t.updated_at, COALESCE(w.path, ''), COALESCE(p.name, '') "
                    "FROM executions e JOIN tasks t ON t.id = e.task_id LEFT JOIN workspaces w ON w.id = e.workspace_id "
                    "LEFT JOIN projects p ON p.id = t.project_id WHERE e.agent_type IN ('dsh','pi','codex','agy','claude') "
                    "AND COALESCE(w.path, '') NOT LIKE '/home/bobbik/%' ORDER BY t.updated_at DESC LIMIT 120"
                ).fetchall()
        except Exception:
            continue
        for at, tid, title, st, uat, wp, pn in rows:
            if at not in allow or (at, tid) in seen or (title and '自动测试，非人工验收' in title):
                continue
            seen.add((at, tid))
            t_clean = (title or pn or tid).strip()
            out.append(_mk_row(f'cp:{at}:{tid}', tid, at, lmap.get(at, at.upper()),
                               f'{at}_control_plane', t_clean, wp or pn or 'vps3', uat, 1, st, False, t_clean, ''))
            if len(out) >= limit:
                break
    return out

def discover_multi(cfg):
    s = []
    if cfg.get('agyDataDir'):
        try:
            raw_agy = discover(cfg['agyDataDir'], min(MAX_ROWS, int(cfg.get('limit', 120)))).get('sessions', [])
            ql_cron = 0
            for item in raw_agy:
                if item.get('stepCount', 0) == 0 and not (item.get('title') or '').strip():
                    continue
                uris = item.get('workspaceUris') or []
                if any('quant_lab' in u for u in uris) and item.get('stepCount', 0) <= 3:
                    ql_cron += 1
                    if ql_cron > 5:
                        continue
                s.append(item)
                if len(s) >= 45:
                    break
        except Exception:
            pass
    s.extend(discover_claude(cfg.get('claudeDataDir')))
    s.extend(discover_pi(cfg.get('piDataDir')))
    s.extend(discover_codex(cfg.get('codexDataDir')))
    s.extend(discover_control_plane(cfg.get('controlPlaneDbs'), cfg.get('harnesses')))
    return {'schemaVersion': 1, 'sessions': s, 'hasMore': False, 'authentication': 'unverified', 'source': 'local_readonly_index'}

def _tstep(name, d):
    c = ' '.join(str(d or '').split())
    return f'`[Tool: {name}]` {c[:177] + "..." if len(c) > 180 else c}' if c else f'`[Tool: {name}]`'

def _tool_result_step(name, output):
    # Preserve returned evidence, without inferring success from arbitrary text.
    if isinstance(output,list):
        text=' '.join(str(x.get('text') or '') for x in output if isinstance(x,dict))
    elif isinstance(output,dict):
        text=json.dumps(output,ensure_ascii=False)
    else:text=str(output or '')
    text=' '.join(text.split()).replace('`',"'")
    name=' '.join(str(name).split()).replace('`',"'")[:80]
    return f'`[ToolResult: {name}]` '+(text[:597]+'...' if len(text)>600 else text)


def _thk(t, n=200):
    c = ' '.join(str(t or '').split())
    return f"`[Thinking]` {c[:n-3] + '...' if len(c) > n else c}"

def _consolidate(events, limit=60):
    out, cur, cts = [], [], 0
    def flush():
        nonlocal cur, cts
        if not cur:
            return
        chunks, sbuf = [], []
        def flush_steps():
            nonlocal sbuf
            if not sbuf:
                return
            sl = '\n'.join(f'- {s}' for s in sbuf[:40]) + (f'\n- ... (+{len(sbuf) - 40} steps)' if len(sbuf) > 40 else '')
            chunks.append(f'```thinking\n{sl}\n```')
            sbuf = []
        for k, v in cur:
            if k == 'step':
                sbuf.append(v)
            else:
                flush_steps()
                chunks.append(v)
        flush_steps()
        full = '\n\n'.join(chunks).strip()
        if full:
            out.append(('assistant', full, cts))
        cur, cts = [], 0
    for role, text, ts in events:
        txt = (text or '').strip()
        if not txt:
            continue
        if role == 'user':
            flush()
            out.append(('user', txt, ts))
        else:
            cur.append(('step' if role == 'step' else 'text', txt))
            cts = ts or cts
    flush()
    now_ms = _now()
    return [{'sequence': i, 'role': r, 'text': t[:32000], 'timestamp': ts or now_ms} for i, (r, t, ts) in enumerate(out[-limit:], 1)]

def _rpath():
    return Path.home() / '.bob_remote_receipts.json'

def _load_r():
    p = _rpath()
    d = _j(p.read_text(encoding='utf-8', errors='replace')) if p.is_file() else {}
    return d if isinstance(d, dict) else {}

def _save_r(d):
    p = _rpath()
    t = p.with_suffix('.tmp')
    try:
        t.write_text(json.dumps(d, ensure_ascii=False), encoding='utf-8')
        t.replace(p)
    except Exception:
        pass

def _reconcile(sid):
    d, chg, out, now_ms = _load_r(), False, [], _now()
    for r in d.values():
        if not isinstance(r, dict) or r.get('session_id') != sid:
            continue
        if r.get('execution_status') == 'running':
            pid, alive = r.get('pid'), False
            if isinstance(pid, int) and pid > 0:
                try:
                    os.kill(pid, 0)
                    alive = True
                except OSError:
                    pass
            if not alive and now_ms - int(r.get('started_at') or 0) > 2500:
                r['execution_status'], r['ended_at'], chg = 'unknown', now_ms, True
        out.append(r)
    if chg:
        _save_r(d)
    out.sort(key=lambda x: int(x.get('started_at') or 0))
    out.extend(_stream_records(sid))
    return out

def read_session_page(cfg, sid):
    ev, ws, h = [], '', 'dsh'
    codex_turn_texts = {}
    native_user_ids = set()
    push = lambda r, t, ts=0: ev.append((r, t.strip(), ts)) if (t or '').strip() else None
    if sid.startswith('agy:'):
        h, raw, adir = 'agy', sid.split(':', 1)[1], cfg.get('agyDataDir') or ''
        root = Path(adir) if adir else None
        if root and root.is_dir():
            idx = root / 'conversation_summaries.db'
            if idx.is_file():
                try:
                    with closing(sqlite3.connect(idx.as_uri() + '?mode=ro', uri=True, timeout=2)) as db:
                        row = db.execute('SELECT workspace_uris FROM conversation_summaries WHERE conversation_id=?', (raw,)).fetchone()
                        if row and row[0]:
                            uris = _j(row[0])
                            if isinstance(uris, list) and uris and isinstance(uris[0], str):
                                ws = uris[0].removeprefix('file://')
                except Exception:
                    pass
            tf = root / 'brain' / raw / '.system_generated' / 'logs' / 'transcript_full.jsonl'
            if not tf.is_file():
                tf = root / 'brain' / raw / '.system_generated' / 'logs' / 'transcript.jsonl'
            if tf.is_file():
                fb = int(tf.stat().st_mtime * 1000)
                for _, o in _iter_jsonl(tf):
                    ts, tp = _ts(o.get('created_at'), fb), o.get('type')
                    if tp == 'USER_INPUT':
                        c = str(o.get('content') or '')
                        m = re.search(r'<USER_REQUEST>\s*([\s\S]*?)\s*</USER_REQUEST>', c)
                        utxt = (m.group(1) if m else re.sub(r'<ADDITIONAL_METADATA>[\s\S]*?</ADDITIONAL_METADATA>', '', c)).strip()
                        if utxt:
                            push('user', utxt, ts)
                    elif tp == 'PLANNER_RESPONSE':
                        th = str(o.get('thinking') or '').strip()
                        if th:
                            push('step', _thk(th), ts)
                        for tc in o.get('tool_calls') or []:
                            if isinstance(tc, dict) and tc.get('name'):
                                a = tc.get('args') if isinstance(tc.get('args'), dict) else tc.get('arguments') if isinstance(tc.get('arguments'), dict) else {}
                                d = a.get('CommandLine') or a.get('Path') or a.get('query') or a.get('Prompt') or tc.get('args') or ''
                                push('step', _tstep(tc['name'], d), ts)
                        ct = str(o.get('content') or '').strip()
                        if ct:
                            push('assistant', ct, ts)
    elif sid.startswith('claude-jsonl:'):
        h, raw, cdir = 'claude', sid.split(':', 1)[1], cfg.get('claudeDataDir') or ''
        m = list((Path(cdir) / 'projects').glob(f'*/{raw}.jsonl')) if cdir else []
        if m:
            fb = int(m[0].stat().st_mtime * 1000)
            seen_uuids = set()
            skip_ping = False
            for _, o in _iter_jsonl(m[0]):
                if o.get('cwd') and (not ws or ws in ('/home/bobbik', '/home/ubuntu')):
                    ws = o['cwd']
                u = o.get('uuid')
                if u:
                    if u in seen_uuids:
                        continue
                    seen_uuids.add(u)
                if o.get('isMeta') or o.get('isSidechain'):
                    continue
                ts, t = _ts(o.get('timestamp'), fb), o.get('type')
                msg = o.get('message') if isinstance(o.get('message'), dict) else {}
                c = msg.get('content')
                if t == 'user':
                    utxt = c if isinstance(c, str) else '\n'.join(
                        b.get('text', '') for b in c
                        if isinstance(b, dict) and b.get('type') == 'text' and not str(b.get('text', '')).lstrip().startswith('<system-reminder>')
                    ) if isinstance(c, list) else ''
                    utxt = re.sub(r'<system-reminder>[\s\S]*?</system-reminder>', '', utxt).strip()
                    if not utxt:
                        continue
                    if utxt == 'ping':
                        skip_ping = True
                        continue
                    if utxt.startswith('This session is being continued from a previous conversation') or any(
                        x in utxt for x in ('<command-name>', '<local-command', '<task-notification>', '<bash-input>', '<bash-stdout>')
                    ):
                        continue
                    # Equal text is a valid new turn. Duplicate native records
                    # were already removed by UUID above; never merge by content.
                    push('user', utxt, ts)
                    if u:native_user_ids.add(u)
                elif t == 'assistant':
                    if isinstance(c, list):
                        has_text = any(isinstance(b, dict) and b.get('type') == 'text' and (b.get('text') or '').strip() for b in c)
                        for b in c:
                            if not isinstance(b, dict):
                                continue
                            bt = b.get('type')
                            if bt == 'thinking':
                                th = (b.get('thinking') or '').strip()
                                if not th:
                                    continue
                                if not has_text and len(th) <= 600 and '\n\n' not in th:
                                    push('assistant', th, ts)
                                else:
                                    push('step', _thk(th), ts)
                            elif bt == 'tool_use' and b.get('name'):
                                inp = b.get('input') if isinstance(b.get('input'), dict) else {}
                                push('step', _tstep(b['name'], inp.get('command') or inp.get('file_path') or inp.get('pattern') or inp.get('description') or ''), ts)
                            elif bt == 'text' and b.get('text'):
                                tx = b['text'].strip()
                                if not tx or tx == 'No response requested.':
                                    continue
                                if skip_ping:
                                    skip_ping = False
                                    continue
                                push('assistant', tx, ts)
                    elif isinstance(c, str):
                        tx = c.strip()
                        if tx and tx != 'No response requested.':
                            if skip_ping:
                                skip_ping = False
                            else:
                                push('assistant', tx, ts)
    elif sid.startswith('pi:'):
        h, raw, pdir = 'pi', sid.split(':', 1)[1], cfg.get('piDataDir') or ''
        m = [p for p in Path(pdir).glob('**/*.jsonl') if raw in p.name] if pdir else []
        if m:
            fb = int(m[0].stat().st_mtime * 1000)
            for _, o in _iter_jsonl(m[0]):
                if o.get('type') == 'session':
                    ws = o.get('cwd') or ws
                elif o.get('type') == 'message':
                    msg = o.get('message', {})
                    role, ts = msg.get('role'), _ts(msg.get('timestamp') or o.get('timestamp'), fb)
                    if role in ('user', 'assistant'):
                        for b in msg.get('content', []):
                            if not isinstance(b, dict):
                                continue
                            bt = b.get('type')
                            if bt == 'thinking' and b.get('thinking'):
                                push('step', _thk(b['thinking']), ts)
                            elif bt == 'toolCall' and b.get('name'):
                                a = b.get('arguments') if isinstance(b.get('arguments'), dict) else {}
                                push('step', _tstep(b['name'], a.get('command') or a.get('path') or ''), ts)
                            elif bt == 'text' and b.get('text'):
                                push(role, b['text'], ts)
    elif sid.startswith('codex:'):
        h, raw, xdir = 'codex', sid.split(':', 1)[1], cfg.get('codexDataDir') or ''
        m = [p for p in (Path(xdir) / 'sessions').glob('**/*.jsonl') if raw in p.name] if xdir else []
        if m:
            fb = int(m[0].stat().st_mtime * 1000)
            current_turn = None
            tool_calls = {}
            for _, o in _iter_jsonl(m[0]):
                ts = _ts(o.get('timestamp'), fb)
                payload = o.get('payload') or {}
                if o.get('type') == 'turn_context' or (o.get('type') == 'event_msg' and payload.get('type') == 'task_started'):
                    next_turn = payload.get('turn_id')
                    if next_turn != current_turn:tool_calls.clear()
                    current_turn = next_turn
                elif o.get('type') == 'event_msg' and payload.get('type') in ('task_complete','turn_aborted'):
                    current_turn = None
                    tool_calls.clear()
                if o.get('type') == 'session_meta':
                    ws = o.get('payload', {}).get('cwd') or ws
                elif o.get('type') == 'response_item':
                    p = o.get('payload', {})
                    if p.get('type') in ('function_call', 'custom_tool_call') and p.get('name'):
                        push('step', _tstep(p['name'], p.get('arguments') or p.get('input') or ''), ts)
                        call_id=p.get('call_id')
                        if current_turn and isinstance(call_id,str) and len(tool_calls)<256:tool_calls[call_id]=p['name']
                    elif p.get('type') in ('function_call_output','custom_tool_call_output'):
                        name=tool_calls.pop(p.get('call_id'),None)
                        if name is not None:push('step',_tool_result_step(name,p.get('output')),ts)
                    elif p.get('role') in ('user', 'assistant'):
                        c = p.get('content')
                        txt = '\n'.join(x.get('text', '') for x in c if isinstance(x, dict)) if isinstance(c, list) else str(c or '')
                        # Harness-injected interruption/control envelopes are not
                        # user chat. Keep ordinary prose, including quoted tags.
                        if p.get('role')=='user' and re.fullmatch(r'\s*<(turn_aborted|codex_internal_context)(?:\s[^>]*)?>[\s\S]*?</\1>\s*',txt):
                            continue
                        if not any(g in txt for g in ('<environment_context>', '<skills_instructions>', '<plugins_instructions>')):
                            push(p['role'], txt, ts)
                            if p['role']=='assistant' and txt.strip() and isinstance(current_turn,str):
                                codex_turn_texts.setdefault(current_turn,[]).append(txt.strip())
    elif sid.startswith('cp:'):
        pts = sid.split(':', 2)
        if len(pts) == 3:
            _, h, tid = pts
            for dbp in cfg.get('controlPlaneDbs') or []:
                p = Path(dbp)
                if not p.is_file():
                    continue
                try:
                    with closing(sqlite3.connect(p.as_uri() + '?mode=ro', uri=True, timeout=2)) as conn:
                        conn.execute('PRAGMA query_only=ON')
                        tr = conn.execute('SELECT title, instruction, status, created_at, updated_at FROM tasks WHERE id=?', (tid,)).fetchone()
                        if not tr:
                            continue
                        title, instr, _, cat, uat = tr
                        wr = conn.execute('SELECT w.path FROM executions e JOIN workspaces w ON w.id=e.workspace_id WHERE e.task_id=? LIMIT 1', (tid,)).fetchone()
                        ws = (wr and wr[0]) or ws
                        push('user', instr or title or tid, cat or uat)
                        tbls = {r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'").fetchall()}
                        if 'project_messages' in tbls:
                            for rr, rc, rts in conn.execute('SELECT role, content, created_at FROM project_messages WHERE task_id=? ORDER BY sequence ASC LIMIT 30', (tid,)).fetchall():
                                if rc and rc != instr:
                                    push('user' if rr == 'user' else 'assistant', rc, rts)
                        for eid, at, est, mid, sat, eat in conn.execute('SELECT id, agent_type, status, model_id, started_at, ended_at FROM executions WHERE task_id=? ORDER BY started_at ASC', (tid,)).fetchall():
                            push('step', f"Execution `{eid}` · `{at.upper()}` · `{mid or 'default'}` · `{est}`", sat or uat)
                            sp = [f"**Review ({rs})**: {rn}" for rs, rn in conn.execute('SELECT status, note FROM reviews WHERE execution_id=? ORDER BY created_at ASC', (eid,)).fetchall() if rn]
                            sp.extend(f"**Artifact `{an}` ({atp})**:\n```\n{ac}\n```" for atp, an, ac in conn.execute('SELECT type, name, substr(uri_or_content, 1, 1200) FROM artifacts WHERE execution_id=? LIMIT 5', (eid,)).fetchall())
                            if sp:
                                push('assistant', '\n\n'.join(sp), eat or sat or uat)
                        break
                except Exception:
                    continue
    recs = [] if cfg.get("mode") == "setup" else _reconcile(sid)
    ru = {t for r, t, _ in ev if r == 'user'}
    ra = {t for r, t, _ in ev if r == 'assistant'}
    for r in recs:
        ut, at, sat = (r.get('text') or '').strip(), (r.get('reply_text') or '').strip(), int(r.get('started_at') or 0)
        if r.get('execution_status') in ('error', 'not_accepted') and not r.get('native_user_message_id') and not at:
            continue
        anchor=r.get('native_user_message_id') if sid.startswith('claude-jsonl:') else None
        input_present=anchor in native_user_ids if anchor else ut in ru
        if ut and not input_present:
            idx = next((i for i, (_, _, ets) in enumerate(ev) if sat and ets and ets >= sat - 1000), len(ev))
            ev.insert(idx, ('user', ut, sat))
            ru.add(ut)
            if anchor:native_user_ids.add(anchor)
        native_parts=codex_turn_texts.get(r.get('native_turn_id')) or []
        native_reply_present=bool(native_parts) and '\n\n'.join(native_parts)==at
        if at and r.get('execution_status') in ('completed','cancelled') and at not in ra and not native_reply_present and not (sid.startswith('claude-jsonl:') and ev and at.startswith('Execution error:')):
            ended=int(r.get('ended_at') or sat)
            idx=next((i for i,(_,_,ets) in enumerate(ev) if ended and ets and ets>ended),len(ev))
            ev.insert(idx,('assistant',at,ended))
            ra.add(at)
    msgs = _consolidate(ev, 60)
    if not msgs and cfg.get('mode') != 'setup':
        msgs = [{'sequence': 1, 'role': 'assistant', 'text': f'Session `{sid}` loaded (`{ws or "default"}`).', 'timestamp': _now()}]
    out = {'harness': h, 'workspace': ws, 'messages': msgs,
           'request_results': [{'request_id': r['request_id'], 'status': r.get('execution_status', 'completed')} for r in recs if r.get('request_id')]}
    if cfg.get('mode') == 'page' and sid.startswith('claude-jsonl:'):
        holder = _safe_claude_holder(sid)
        if holder is not None:
            out['terminal_holder'] = holder
    return out


def _read_artifact_bytes(root,relative,limit=2*1024*1024):
    # Open each component without following symlinks. The held directory FDs
    # prevent path swaps between authorization and reading on node hosts.
    if not hasattr(os,'O_NOFOLLOW') or os.open not in os.supports_dir_fd:
        raise ValueError('DSH_PREVIEW_UNAVAILABLE')
    fd=os.open(root,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
    try:
        for part in relative.parts[:-1]:
            next_fd=os.open(part,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW,dir_fd=fd)
            os.close(fd);fd=next_fd
        target=os.open(relative.parts[-1],os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK,dir_fd=fd)
        with os.fdopen(target,'rb') as stream:
            before=os.fstat(stream.fileno())
            if not stat.S_ISREG(before.st_mode):raise ValueError('DSH_FILE_PATH_REJECTED')
            if before.st_size>limit:raise ValueError('DSH_FILE_TOO_LARGE')
            data=stream.read(limit+1);after=os.fstat(stream.fileno())
            if len(data)!=before.st_size or (before.st_mtime_ns,before.st_ctime_ns,before.st_size)!=(after.st_mtime_ns,after.st_ctime_ns,after.st_size):
                raise ValueError('DSH_FILE_CHANGED')
    except OSError:raise ValueError('DSH_FILE_UNAVAILABLE')
    finally:os.close(fd)
    return data


def _html_resource_bundle(root,relative,source,budget):
    from html.parser import HTMLParser
    from urllib.parse import urlsplit
    import posixpath
    from io import BytesIO
    class Resources(HTMLParser):
        def __init__(self):super().__init__();self.urls=[]
        def handle_starttag(self,tag,attrs):
            attrs=dict(attrs)
            key='src' if tag in ('img','script') else 'href' if tag=='link' and attrs.get('rel')=='stylesheet' else None
            if key and attrs.get(key) and attrs[key] not in self.urls:self.urls.append(attrs[key])
    parser=Resources();parser.feed(source)
    resources=[];omitted=[]
    for url in parser.urls[:24]:
        try:
            parts=urlsplit(url)
            if parts.scheme or parts.netloc or parts.query or parts.fragment or '%' in url or '\\' in url:
                raise ValueError('DSH_FILE_PATH_REJECTED')
            path=Path(parts.path)
            if path.is_absolute() or not path.parts:raise ValueError('DSH_FILE_PATH_REJECTED')
            child=Path(posixpath.normpath((relative.parent/path).as_posix()))
            if child.is_absolute() or any(p.startswith('.') for p in child.parts):raise ValueError('DSH_FILE_PATH_REJECTED')
            ext=child.suffix.lower();mime={'.png':'image/png','.jpg':'image/jpeg','.jpeg':'image/jpeg','.webp':'image/webp','.css':'text/css','.js':'text/javascript'}.get(ext)
            if not mime:raise ValueError('DSH_FILE_TYPE_UNSUPPORTED')
            raw=_read_artifact_bytes(root,child,16*1024*1024 if mime.startswith('image/') else 256*1024)
            original_hash=hashlib.sha256(raw).hexdigest();thumbnail=False
            if mime.startswith('image/'):
                try:
                    from PIL import Image
                    with Image.open(BytesIO(raw)) as im:
                        if im.width*im.height>24000000:raise ValueError('DSH_FILE_TOO_LARGE')
                        im.thumbnail((1280,1280));out=BytesIO();im.convert('RGB').save(out,format='JPEG',quality=82)
                        raw=out.getvalue();mime='image/jpeg';thumbnail=True
                except ImportError:
                    if len(raw)>256*1024:raise ValueError('DSH_IMAGE_RENDERER_UNAVAILABLE')
            if len(raw)>budget:raise ValueError('DSH_FILE_TOO_LARGE')
            budget-=len(raw)
            resources.append({'url':url,'mime':mime,'data':base64.b64encode(raw).decode('ascii'),'bytes':len(raw),'source_sha256':original_hash,'thumbnail':thumbnail})
        except (ValueError,OSError) as exc:
            omitted.append({'url':url,'code':str(exc) if isinstance(exc,ValueError) and str(exc).startswith('DSH_') else 'DSH_FILE_UNAVAILABLE'})
    omitted.extend({'url':u,'code':'DSH_RESOURCE_LIMIT'} for u in parser.urls[24:])
    return resources,omitted

def _document_preview(data, extension, budget=2*1024*1024):
    """Render a bounded document using local tools; never treat extraction as rendering."""
    import shutil, tempfile
    if not shutil.which('pdfinfo') or not shutil.which('pdftoppm'):
        raise ValueError('DSH_DOCUMENT_RENDERER_UNAVAILABLE')
    with tempfile.TemporaryDirectory(prefix='bob-document-') as tmp:
        work=Path(tmp); source=work/('source.'+extension);source.write_bytes(data)
        if extension!='pdf':
            # Office conversion is opt-in through an installed, operator-managed tool.
            office=shutil.which('libreoffice') or shutil.which('soffice')
            if not office:raise ValueError('DSH_DOCUMENT_RENDERER_UNAVAILABLE')
            sandbox=shutil.which('bwrap')
            if not sandbox:raise ValueError('DSH_DOCUMENT_SANDBOX_UNAVAILABLE')
            # A private profile prevents reuse of another user's running Office process.
            profile=work/'profile'; profile.mkdir()
            (profile/'user').mkdir()
            (profile/'user/registrymodifications.xcu').write_text(
                '<?xml version="1.0" encoding="UTF-8"?><oor:items xmlns:oor="http://openoffice.org/2001/registry">'
                '<item oor:path="/org.openoffice.Office.Common/Security/Scripting"><prop oor:name="MacroSecurityLevel" oor:op="fuse"><value>3</value></prop></item>'
                '<item oor:path="/org.openoffice.Office.Common/Load"><prop oor:name="UpdateLinkMode" oor:op="fuse"><value>0</value></prop></item></oor:items>',encoding='utf-8')
            try:
                isolated=[sandbox,'--unshare-all','--die-with-parent','--new-session','--clearenv','--setenv','PATH','/usr/bin:/bin','--setenv','HOME','/work','--setenv','LANG','C.UTF-8']
                for runtime in ['/usr','/bin','/lib','/lib64','/etc/fonts','/etc/ld.so.cache']:
                    if Path(runtime).exists():isolated+=['--ro-bind',runtime,runtime]
                isolated+=['--proc','/proc','--dev','/dev','--tmpfs','/tmp','--bind',str(work),'/work','--chdir','/work']
                completed=subprocess.run(isolated+[office,'-env:UserInstallation=file:///work/profile','--headless','--nologo','--nodefault','--norestore','--convert-to','pdf','--outdir','/work','/work/'+source.name],cwd=work,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=45)
            except subprocess.TimeoutExpired:raise ValueError('DSH_DOCUMENT_RENDER_TIMEOUT')
            source=work/'source.pdf'
            if completed.returncode or not source.is_file():raise ValueError('DSH_DOCUMENT_RENDER_FAILED')
            if source.stat().st_size>16*1024*1024:raise ValueError('DSH_FILE_TOO_LARGE')
        try:
            info=subprocess.run(['pdfinfo',str(source)],stdin=subprocess.DEVNULL,capture_output=True,timeout=10)
            match=re.search(rb'^Pages:\s+(\d+)\s*$',info.stdout,re.M)
            if info.returncode or not match:raise ValueError('DSH_DOCUMENT_RENDER_FAILED')
            count=int(match.group(1))
            if count<1 or count>30:raise ValueError('DSH_DOCUMENT_PAGE_LIMIT')
            rendered=subprocess.run(['pdftoppm','-f','1','-l',str(count),'-scale-to','1280','-jpeg','-jpegopt','quality=80',str(source),str(work/'page')],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=40)
            if rendered.returncode:raise ValueError('DSH_DOCUMENT_RENDER_FAILED')
        except subprocess.TimeoutExpired:raise ValueError('DSH_DOCUMENT_RENDER_TIMEOUT')
        pages=[];total=0
        for image in sorted(work.glob('page-*.jpg'),key=lambda path:int(path.stem.split('-')[-1])):
            total+=image.stat().st_size
            if total>budget:raise ValueError('DSH_FILE_TOO_LARGE')
            raw=image.read_bytes()
            pages.append({'number':int(image.stem.split('-')[-1]),'mime':'image/jpeg','data':base64.b64encode(raw).decode('ascii'),'bytes':len(raw)})
        if len(pages)!=count:raise ValueError('DSH_DOCUMENT_RENDER_FAILED')
        return {'pages':pages,'page_count':count,'renderer':'local-pdf-pages-v1'}


def preview_native_file(cfg):
    sid, requested = cfg.get('sessionId', ''), cfg.get('path', '')
    if not re.fullmatch(r'(codex|claude-jsonl|agy|pi):[A-Za-z0-9_-]{8,128}', sid):
        raise ValueError('DSH_SESSION_NOT_FOUND')
    if not isinstance(requested, str) or not requested or len(requested)>1024 or any(c in requested for c in '\\:%?#\x00'):
        raise ValueError('DSH_FILE_PATH_REJECTED')
    page=read_session_page(cfg,sid)
    workspace=page.get('workspace')
    if not workspace or not Path(workspace).is_absolute():raise ValueError('DSH_FILE_OUTSIDE_WORKSPACE')
    root=Path(workspace).resolve(strict=True)
    candidate=Path(requested)
    try:relative=candidate.relative_to(root) if candidate.is_absolute() else candidate
    except ValueError:raise ValueError('DSH_FILE_OUTSIDE_WORKSPACE')
    if not relative.parts or any(p in ('.','..') or (p.startswith('.') and p != '.bob_attachments') for p in relative.parts):
        raise ValueError('DSH_FILE_PATH_REJECTED')
    ext=relative.suffix.lower().lstrip('.')
    if ext not in ('png','jpg','jpeg','webp','md','txt','json','yaml','yml','gd','tscn','js','css','html','pdf','docx','xlsx','pptx','doc','xls','ppt','odt','ods','odp'):
        raise ValueError('DSH_FILE_TYPE_UNSUPPORTED')
    refs=set()
    for message in page.get('messages',[]):
        body=re.sub(r'`[\s\S]*?`','',message.get('text',''))
        if message.get('role')=='assistant':
            refs.update(re.findall(r'!?\[[^\]]*\]\(<?([^\s)>]+)>?(?:\s+[^)]*)?\)',body))
            refs.update(re.findall(r'([^\n]+)',body))
        refs.update(re.findall(r'\[Attachment:\s*[^->\]]+->\s*([^\]]+)\]',body))
        refs.update(re.findall(r'`([^`\n]+)`',body))
    # Native assistants may emit local file URIs; normalize only their
    # explicit references, never the requested path or workspace boundary.
    from urllib.parse import urlsplit, unquote
    for reference in tuple(refs):
        if not reference.startswith('file:'): continue
        try:
            uri = urlsplit(reference)
            if uri.scheme == 'file' and not uri.netloc and not uri.query and not uri.fragment:
                decoded = unquote(uri.path, errors='strict')
                if decoded.startswith('/') and '\x00' not in decoded:
                    refs.add(decoded)
        except (ValueError, UnicodeError): pass
    if requested not in refs and relative.as_posix() not in refs and str(root/relative) not in refs:
        raise ValueError('DSH_FILE_NOT_REFERENCED')
    data=_read_artifact_bytes(root,relative)
    digest=hashlib.sha256(data).hexdigest()
    result={'path':requested,'session_id':sid,'extension':ext,'bytes':len(data),'version':digest,'sha256':digest,'source':'current_workspace'}
    if ext=='html':
        resources,omitted=_html_resource_bundle(root,relative,data.decode('utf-8'),2*1024*1024-len(data))
        result['resources']=resources;result['resources_unavailable']=omitted
        result['version']=hashlib.sha256((digest+json.dumps([(x['url'],x['source_sha256']) for x in resources])+json.dumps(omitted)).encode()).hexdigest()
    known=cfg.get('known') or {}
    if ext in ('pdf','docx','xlsx','pptx','doc','xls','ppt','odt','ods','odp'):
        result['version']=hashlib.sha256((digest+':local-pdf-pages-v1').encode()).hexdigest()
        if known.get('version')==result['version'] and known.get('bytes')==len(data):
            result['not_modified']=True
            return result
        result['document']=_document_preview(data,ext,2*1024*1024-len(data))
    if known.get('version')==result['version'] and known.get('bytes')==len(data):result['not_modified']=True
    else:result['data']=base64.b64encode(data).decode('ascii')
    return result


def inspect_receipt(cfg):
    sid, rid = cfg.get('sessionId', ''), cfg.get('requestId', '')
    for r in _reconcile(sid):
        if r.get('request_id') != rid: continue
        # A local intent record alone is not proof that the remote tool accepted it.
        if 'text' not in r:
            return {'status':'unknown','session_id':sid,'request_id':rid}
        if r['text'] != cfg.get('text'): raise ValueError('HARNESS_REQUEST_CONFLICT')
        if r.get('execution_status') == 'unknown' and sid.startswith('codex:'):
            r = reconcile_codex_turn(r)
        state=r.get('execution_status','unknown')
        return {'status':'not_accepted' if state=='not_accepted' else 'unknown' if state=='unknown' else 'accepted',
                'execution_status':state,'session_id':sid,'request_id':rid}
    return {'status': 'unknown', 'session_id': sid, 'request_id': rid}

"""Native session driver, durable incremental receipts and conservative permissions."""
import hashlib
import uuid
import queue
import signal
import threading


def _stream_db():
    root = Path.home() / '.local/share/bob-native'
    root.mkdir(parents=True, exist_ok=True, mode=0o700)
    db = sqlite3.connect(root / 'receipts.sqlite', timeout=10)
    db.execute('PRAGMA journal_mode=WAL')
    db.execute('CREATE TABLE IF NOT EXISTS requests (id TEXT PRIMARY KEY, session TEXT NOT NULL, input_hash TEXT NOT NULL, body TEXT NOT NULL)')
    db.execute('CREATE TABLE IF NOT EXISTS events (sequence INTEGER PRIMARY KEY AUTOINCREMENT, request TEXT NOT NULL, session TEXT NOT NULL, kind TEXT NOT NULL, payload TEXT NOT NULL, at INTEGER NOT NULL)')
    db.execute('CREATE INDEX IF NOT EXISTS events_session ON events(session,sequence)')
    _journal_schema(db)
    db.commit()
    return db


def _stream_records(sid):
    with closing(_stream_db()) as db, db:
        db.execute('BEGIN IMMEDIATE')
        _journal_sync(db)
        records=[json.loads(r[0]) for r in db.execute('SELECT body FROM requests WHERE session=?', (sid,))]
        for r in records:
            saved=db.execute('SELECT body FROM event_snapshots WHERE request=? AND session=?',(r['request_id'],sid)).fetchone()
            checkpoint=json.loads(saved[0]) if saved else {}
            if r.get('execution_status')=='unknown' and r.get('claude_wire'):
                recovered_activity={};progress={}
                recovered=_claude_wire_result(r,recovered_activity,progress)
                if not recovered and progress:
                    observed=progress.get('text','');previous=r.get('reply_text','')
                    address={'session_id':sid,'request_id':r['request_id']}
                    if observed!=previous and observed.startswith(previous):
                        r['reply_text']=observed
                        db.execute('UPDATE requests SET body=? WHERE id=?',(json.dumps(r,ensure_ascii=False),r['request_id']))
                        _append_event(db,address,'message_snapshot',{'text':observed})
                    activity=progress.get('activity')
                    if activity:
                        activity['approvals']=checkpoint.get('activity',{}).get('approvals',[])
                        # The lost controller cannot vouch that a pending tool is still running.
                        for item in activity.get('items',[]):
                            if item.get('status')=='running':item['status']='unconfirmed'
                        if activity!=checkpoint.get('activity'):
                            _append_event(db,address,'activity_snapshot',{'activity':activity})
                if recovered:
                    checkpoint.update(native_terminal=recovered[0],native_terminal_text=recovered[1])
                    if recovered_activity:
                        recovered_activity['approvals']=checkpoint.get('activity',{}).get('approvals',[])
                        _append_event(db,{'session_id':sid,'request_id':r['request_id']},'activity_snapshot',{'activity':recovered_activity})
            proven=checkpoint.get('native_terminal')
            if sid.startswith('codex:') and r.get('execution_status')=='unknown' and r.get('error_code')=='HARNESS_TIMEOUT' and not r.get('turn_start_requested') and not r.get('native_turn_id'):
                r.update(execution_status='not_accepted',reconciliation='timeout_before_turn_start')
                db.execute('UPDATE requests SET body=? WHERE id=?',(json.dumps(r,ensure_ascii=False),r['request_id']))
                _append_event(db,{'session_id':sid,'request_id':r['request_id']},'error',{'code':'HARNESS_NOT_ACCEPTED'})
                continue
            if r.get('execution_status') == 'unknown' and proven in ('completed','cancelled'):
                text=checkpoint.get('native_terminal_text',checkpoint.get('text',r.get('reply_text','')))
                r.update(execution_status=proven,reply_text=text,ended_at=checkpoint.get('observed_at',_now()),error_code=None)
                db.execute('UPDATE requests SET body=? WHERE id=?',(json.dumps(r,ensure_ascii=False),r['request_id']))
                address={'session_id':sid,'request_id':r['request_id']}
                _append_event(db,address,'message_snapshot',{'text':text})
                _append_event(db,address,proven)
                continue
            if r.get('execution_status') in ('not_accepted','unknown') and checkpoint.get('status') in ('accepted','running'):
                _append_event(db,{'session_id':sid,'request_id':r['request_id']},'error' if r['execution_status']=='not_accepted' else 'unknown',{'code':r.get('error_code') or 'HARNESS_NOT_ACCEPTED'})
                continue
            if r.get('execution_status') in ('completed','cancelled','error') and checkpoint.get('status') in ('accepted','running','unknown'):
                address={'session_id':sid,'request_id':r['request_id']}
                _append_event(db,address,'message_snapshot',{'text':r.get('reply_text','')})
                _append_event(db,address,r['execution_status'])
                continue
            if r.get('execution_status') in ('accepted','running','unknown') and checkpoint.get('status') in ('completed','cancelled','error'):
                r.update(execution_status=checkpoint['status'],reply_text=checkpoint.get('text',''),ended_at=checkpoint.get('observed_at',_now()))
                db.execute('UPDATE requests SET body=? WHERE id=?',(json.dumps(r,ensure_ascii=False),r['request_id']))
                continue
            if r.get('execution_status') not in ('accepted','running') or _now()-r.get('started_at',0)<30000:continue
            alive=False
            try:
                pid=int(r.get('pid') or 0)
                if pid>0:
                    os.kill(pid,0)
                    # Existence alone is not ownership (or proof of a live
                    # worker on hosts without a readable process command line).
                    proc=Path('/proc')/str(pid)/'cmdline'
                    if r.get('supervisor_token'):
                        alive=_agy_process_owned(pid,r['supervisor_token'],sid)
                    elif proc.exists():alive=r['request_id'].encode() in proc.read_bytes()
            except (OSError,ValueError):pass
            if not alive:
                r.update(execution_status='unknown',error_code='HARNESS_PROCESS_LOST')
                db.execute('UPDATE requests SET body=? WHERE id=?',(json.dumps(r,ensure_ascii=False),r['request_id']))
                db.execute('INSERT INTO events(request,session,kind,payload,at) VALUES(?,?,?,?,?)',(r['request_id'],sid,'unknown',json.dumps({'code':'HARNESS_PROCESS_LOST'}),_now()))
        return records


def _stream_update(rid, **fields):
    with closing(_stream_db()) as db, db:
        db.execute('BEGIN IMMEDIATE')
        row = db.execute('SELECT body FROM requests WHERE id=?', (rid,)).fetchone()
        if not row:
            raise ValueError('HARNESS_RECEIPT_MISSING')
        body = json.loads(row[0]); body.update(fields)
        db.execute('UPDATE requests SET body=? WHERE id=?', (json.dumps(body, ensure_ascii=False), rid))
        return body


def _stream_event(cfg, kind, payload=None):
    with closing(_stream_db()) as db,db:
        db.execute('BEGIN IMMEDIATE')
        claude_activity=(kind=='source_event' and (payload or {}).get('source')=='claude-stream-json')
        def current_activity():
            row=db.execute('SELECT body FROM event_snapshots WHERE request=? AND session=?',(cfg['request_id'],cfg['session_id'])).fetchone()
            return json.loads(row[0]).get('activity') if row else None
        previous=current_activity() if claude_activity else None
        _append_event(db,cfg,kind,payload)
        if claude_activity:
            activity=current_activity()
            if activity is not None and activity!=previous:
                _append_event(db,cfg,'activity_snapshot',{'activity':activity})


# Event replay is a bounded transport cache. Requests remain the durable ledger.
EVENT_MAX_ROWS = 20000
EVENT_MAX_BYTES = 16 * 1024 * 1024
EVENT_KEEP_MS = 7 * 24 * 3600 * 1000
SNAPSHOT_MAX_ROWS = 256
SNAPSHOT_MAX_BYTES = 64 * 1024 * 1024
EVENT_PAGE_BYTES = 1536 * 1024

def _journal_schema(db):
    db.execute('CREATE TABLE IF NOT EXISTS event_meta(id INTEGER PRIMARY KEY CHECK(id=1),epoch TEXT NOT NULL,projected INTEGER NOT NULL DEFAULT 0,floor INTEGER NOT NULL DEFAULT 0)')
    db.execute('INSERT OR IGNORE INTO event_meta(id,epoch) VALUES(1,?)',(os.urandom(16).hex(),))
    db.execute('CREATE TABLE IF NOT EXISTS event_snapshots(request TEXT PRIMARY KEY,session TEXT NOT NULL,sequence INTEGER NOT NULL,body TEXT NOT NULL,at INTEGER NOT NULL)')
    db.execute('CREATE INDEX IF NOT EXISTS snapshots_session ON event_snapshots(session,sequence)')
    db.execute('CREATE TABLE IF NOT EXISTS event_size(id INTEGER PRIMARY KEY CHECK(id=1),rows INTEGER NOT NULL,bytes INTEGER NOT NULL)')
    db.execute('INSERT OR IGNORE INTO event_size SELECT 1,COUNT(*),COALESCE(SUM(length(CAST(payload AS BLOB))),0) FROM events')
    db.execute('CREATE TRIGGER IF NOT EXISTS event_size_add AFTER INSERT ON events BEGIN UPDATE event_size SET rows=rows+1,bytes=bytes+length(CAST(NEW.payload AS BLOB)) WHERE id=1; END')
    db.execute('CREATE TRIGGER IF NOT EXISTS event_size_del AFTER DELETE ON events BEGIN UPDATE event_size SET rows=rows-1,bytes=bytes-length(CAST(OLD.payload AS BLOB)) WHERE id=1; END')

def _journal_fold(db, row):
    seq,rid,sid,kind,raw,at=row
    payload=json.loads(raw)
    old=db.execute('SELECT body FROM event_snapshots WHERE request=? AND session=?',(rid,sid)).fetchone()
    body=json.loads(old[0]) if old else {'request_id':rid,'session_id':sid,'text':'','status':'unknown'}
    # A delayed transport frame cannot reopen a confirmed terminal execution.
    if body.get('status') in ('completed','cancelled','error') and kind not in ('message_snapshot','activity_snapshot','recovery_state'):return
    # Preserve official terminal evidence in the bounded snapshot so a worker
    # dying between receipt and projection can be reconciled without replay.
    if kind=='source_event' and payload.get('source')=='claude-stream-json' and sid.startswith('claude-jsonl:'):
        frame=payload.get('frame') or {}
        if frame.get('session_id')==sid.split(':',1)[1]:
            activity=_project_claude_activity(body.get('activity'),frame)
            if activity is not None:body['activity']=activity
        response=frame.get('response') or {}
        if frame.get('type')=='control_response' and response.get('request_id')=='bob-cancel-'+rid and response.get('subtype')=='success':
            body['claude_interrupt_ack']=True
        if frame.get('type')=='result' and frame.get('session_id')==sid.split(':',1)[1]:
            if frame.get('terminal_reason')=='aborted_streaming':
                body['claude_aborted_result']=True
            elif frame.get('subtype')=='success' and not frame.get('is_error'):
                body['native_terminal']='completed'
                if isinstance(frame.get('result'),str):body['native_terminal_text']=frame['result'][:262144]
        if body.get('claude_interrupt_ack') and body.get('claude_aborted_result'):
            body['native_terminal']='cancelled'
    if kind in ('source_event','permission_decision') and payload.get('source')=='agy-acp' and sid.startswith('agy:'):
        params=payload.get('params') or {}
        record=db.execute('SELECT body FROM requests WHERE id=? AND session=?',(rid,sid)).fetchone()
        execution=json.loads(record[0]) if record else {}
        serial=payload.get('acp_request_id')
        bound=type(serial) is int and type(execution.get('acp_request_id')) is int and serial==execution['acp_request_id']
        if bound and params.get('sessionId')==sid.split(':',1)[1]:
            frame={'method':payload.get('method'),'params':params}
            if 'source_request_id' in payload:frame['id']=payload['source_request_id']
            activity=_project_agy_activity(body.get('activity'),frame,payload.get('local_decision'))
            if activity is not None:body['activity']=activity
            update=params.get('update') or {};content=update.get('content') or {}
            if payload.get('method')=='session/update' and update.get('sessionUpdate')=='agent_message_chunk' and content.get('type')=='text' and isinstance(content.get('text'),str):
                body['agy_source_text']=body.get('agy_source_text','')+content['text']
                if len(body['agy_source_text'])>262144:raise ValueError('HARNESS_OUTPUT_LIMIT')
        if bound and payload.get('method')=='session/prompt_result' and payload.get('session_id')==sid:
            terminal={'end_turn':'completed','cancelled':'cancelled'}.get(payload.get('stop_reason'))
            if terminal:
                body['native_terminal']=terminal
                body['native_terminal_text']=body.get('agy_source_text',body.get('text',''))
    if kind=='recovery_state' and type(payload.get('pending')) is bool:body['recovery_pending']=payload['pending']
    if kind=='activity_snapshot':body['activity']=payload.get('activity') or {}
    if kind=='message_snapshot':body['text']=str(payload.get('text') or '')
    elif kind=='text_delta':body['text']+=str(payload.get('text') or '')
    if len(body['text'])>262144:raise ValueError('HARNESS_OUTPUT_LIMIT')
    if kind=='accepted':body['cancel_supported']=payload.get('cancel_supported') is True
    if kind in ('accepted','running','completed','cancelled','error','unknown'):body['status']=kind
    elif kind=='text_delta':body['status']='running'
    if kind=='cancel_requested':body['cancel_pending']=True
    if kind in ('completed','cancelled','error','unknown'):body['cancel_pending']=False
    if (kind in ('completed','cancelled','error','unknown') or body.get('status') in ('completed','cancelled','error')) and body.get('activity'):
        for item in body['activity'].get('items',[]):
            if item.get('status')=='running':item['status']='unconfirmed'
        for approval in body['activity'].get('approvals',[]):
            if approval.get('status') in ('pending','queued'):approval['status']='unconfirmed'
    if payload.get('code'):body['code']=payload['code']
    body.update(sequence=seq,observed_at=at)
    db.execute('INSERT OR REPLACE INTO event_snapshots VALUES(?,?,?,?,?)',(rid,sid,seq,json.dumps(body,ensure_ascii=False),at))

def _journal_sync(db):
    projected=db.execute('SELECT projected FROM event_meta WHERE id=1').fetchone()[0]
    # Also projects events written by an older worker during a rolling upgrade.
    while True:
        rows=db.execute('SELECT sequence,request,session,kind,payload,at FROM events WHERE sequence>? ORDER BY sequence LIMIT 256',(projected,)).fetchall()
        if not rows:break
        for row in rows:_journal_fold(db,row)
        projected=rows[-1][0]
        db.execute('UPDATE event_meta SET projected=? WHERE id=1',(projected,))
    return projected

def _journal_prune(db):
    # Never prune before projecting; a reset must have a replacement baseline.
    cutoff=_now()-EVENT_KEEP_MS
    old=db.execute('SELECT MAX(sequence) FROM events WHERE at<?',(cutoff,)).fetchone()[0] or 0
    count,size=db.execute('SELECT rows,bytes FROM event_size WHERE id=1').fetchone()
    end=old
    if count>EVENT_MAX_ROWS or size>EVENT_MAX_BYTES:
        for seq,n in db.execute('SELECT sequence,length(CAST(payload AS BLOB)) FROM events ORDER BY sequence'):
            if count<=EVENT_MAX_ROWS and size<=EVENT_MAX_BYTES:break
            count-=1;size-=n;end=max(end,seq)
    if end:
        db.execute('DELETE FROM events WHERE sequence<=?',(end,))
        db.execute('UPDATE event_meta SET floor=MAX(floor,?) WHERE id=1',(end,))
    # These snapshots are recoverable projections, never the send/unknown ledger.
    rows=db.execute('SELECT request,at,length(CAST(body AS BLOB)) FROM event_snapshots ORDER BY at DESC,sequence DESC').fetchall()
    count=size=0
    for rid,at,n in rows:
        count+=1;size+=n
        if count>SNAPSHOT_MAX_ROWS or size>SNAPSHOT_MAX_BYTES or at<cutoff:
            db.execute('DELETE FROM event_snapshots WHERE request=?',(rid,))

def _append_event(db,cfg,kind,payload=None):
    raw=json.dumps(payload or {},ensure_ascii=False)
    if len(raw.encode('utf-8'))>1100*1024:raise ValueError('HARNESS_OUTPUT_LIMIT')
    db.execute('INSERT INTO events(request,session,kind,payload,at) VALUES(?,?,?,?,?)',(cfg['request_id'],cfg['session_id'],kind,raw,_now()))
    _journal_sync(db)
    _journal_prune(db)

def _recovery_snapshots(db,sid):
    records={r['request_id']:r for (raw,) in db.execute('SELECT body FROM requests WHERE session=?',(sid,)) for r in [json.loads(raw)]}
    snapshots={r['request_id']:r for (raw,) in db.execute('SELECT body FROM event_snapshots WHERE session=? ORDER BY sequence DESC',(sid,)) for r in [json.loads(raw)]}
    # An evicted transport snapshot cannot erase a pending/unknown receipt.
    for rid,r in records.items():
        if r.get('execution_status') not in ('accepted','running','unknown'):continue
        if rid not in snapshots:
            snapshots[rid]={'request_id':rid,'session_id':sid,'text':r.get('reply_text',''),'status':r['execution_status'],'observed_at':r.get('ended_at',r.get('started_at',0)),'cancel_supported':r.get('cancel_supported',False),'cancel_pending':bool(r.get('cancel_requested_at'))}
    ordered=sorted(snapshots.values(),key=lambda r:(r.get('status') in ('accepted','running','unknown'),r.get('observed_at',0)),reverse=True)
    result=[];size=0;omitted=False
    for r in ordered:
        n=len(json.dumps(r,ensure_ascii=False).encode('utf-8'))
        if len(result)>=32 or size+n>EVENT_PAGE_BYTES:omitted=True;continue
        result.append(r);size+=n
    return result,omitted

def read_native_events(cfg):
    sid=cfg['sessionId'];after=max(0,int(cfg.get('after') or 0))
    external=touch_external_observer(sid) if cfg.get('observeExternal') else 'history_only'
    _stream_records(sid)
    with closing(_stream_db()) as db,db:
        db.execute('BEGIN IMMEDIATE');head=_journal_sync(db);_journal_prune(db)
        epoch,_,floor=db.execute('SELECT epoch,projected,floor FROM event_meta WHERE id=1').fetchone()
        incoming=cfg.get('epoch')
        reset=(incoming is not None and incoming!=epoch) or after<floor or after>head
        if reset:
            snapshots,omitted=_recovery_snapshots(db,sid)
            return {'session_id':sid,'external_live':external,'epoch':epoch,'cursor':head,'reset':True,'reason':'resync_required','history_required':True,'snapshot_incomplete':omitted,'snapshots':snapshots,'events':[],'has_more':False}
        rows=db.execute('SELECT sequence,request,kind,payload,at FROM events WHERE session=? AND sequence>? ORDER BY sequence LIMIT 101',(sid,after)).fetchall()
        events=[];size=0
        for n,r,k,p,at in rows[:100]:
            event={'sequence':n,'request_id':r,'session_id':sid,'kind':k,'payload':json.loads(p),'observed_at':at}
            added=len(json.dumps(event,ensure_ascii=False).encode('utf-8'))
            if size+added>EVENT_PAGE_BYTES:break
            events.append(event);size+=added
        more=len(events)<len(rows)
        return {'session_id':sid,'external_live':external,'epoch':epoch,'cursor':events[-1]['sequence'] if more and events else head,'events':events,'has_more':more,'reset':False}



def request_native_cancel(cfg):
    """Only cancel a Bob-owned request, never a process discovered by name."""
    sid, rid = cfg.get('sessionId'), cfg.get('requestId')
    if not sid or not rid: raise ValueError('HARNESS_REQUEST_REQUIRED')
    with closing(_stream_db()) as db, db:
        db.execute('BEGIN IMMEDIATE')
        row=db.execute('SELECT body FROM requests WHERE id=? AND session=?',(rid,sid)).fetchone()
        if not row: raise ValueError('HARNESS_RECEIPT_MISSING')
        record=json.loads(row[0]);state=record.get('execution_status')
        if state in ('completed','cancelled','error'):
            return {'session_id':sid,'request_id':rid,'status':state}
        if state not in ('accepted','running'): raise ValueError('HARNESS_CANCEL_UNCONFIRMED')
        if not sid.startswith(('codex:','pi:','agy:','claude-jsonl:')) or not record.get('cancel_supported'): raise ValueError('HARNESS_CANCEL_UNSUPPORTED')
        if not record.get('cancel_requested_at'):
            record['cancel_requested_at']=_now()
            db.execute('UPDATE requests SET body=? WHERE id=?',(json.dumps(record,ensure_ascii=False),rid))
            db.execute('INSERT INTO events(request,session,kind,payload,at) VALUES(?,?,?,?,?)',(rid,sid,'cancel_requested','{}',_now()))
    return {'session_id':sid,'request_id':rid,'status':'cancel_requested'}


def _cancel_requested(rid):
    with closing(_stream_db()) as db:
        row=db.execute('SELECT body FROM requests WHERE id=?',(rid,)).fetchone()
    return bool(row and json.loads(row[0]).get('cancel_requested_at'))


class NativePipe:
    def __init__(self, command, cwd, env):
        self.p = subprocess.Popen(command, cwd=cwd, env=env, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, start_new_session=True)
        self.q = queue.Queue(maxsize=256)
        self.reader_done = threading.Event()
        def reader():
            try:
                while True:
                    line = self.p.stdout.readline(8*1024*1024+1)
                    if not line: break
                    if len(line)>8*1024*1024: raise ValueError()
                    self.q.put(json.loads(line), timeout=10)
            except Exception:
                pass
            finally:
                try: self.q.put(None, timeout=1)
                except queue.Full: pass
                self.reader_done.set()
        threading.Thread(target=reader, daemon=True).start()

    def send(self, frame):
        self.p.stdin.write((json.dumps(frame, ensure_ascii=False)+'\n').encode('utf-8'))
        self.p.stdin.flush()

    def receive(self, timeout=120):
        try: frame = self.q.get(timeout=timeout)
        except queue.Empty:
            # EOF sentinel may not fit in a full queue; report it after draining.
            if self.reader_done.is_set(): raise ValueError('HARNESS_PROCESS_EXITED') from None
            if timeout <= 0.5: return None
            raise ValueError('HARNESS_TIMEOUT') from None
        if frame is None: raise ValueError('HARNESS_PROCESS_EXITED')
        return frame

    def close(self):
        try: self.p.stdin.close()
        except (OSError, ValueError): pass
        try: self.p.wait(timeout=3)
        except subprocess.TimeoutExpired:
            os.killpg(self.p.pid, signal.SIGTERM)
            try: self.p.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(self.p.pid, signal.SIGKILL); self.p.wait(timeout=3)



CLAUDE_WIRE_RELAY = r"""
import sys,json,subprocess,threading,os,sqlite3
from pathlib import Path
cfg=json.loads(sys.argv[1]);limit=8*1024*1024
root=Path(cfg['path']).parent
quota=sqlite3.connect(root/'quota.sqlite',timeout=10)
header=(json.dumps(cfg['identity'])+'\n').encode()
with quota:
    quota.execute('BEGIN IMMEDIATE')
    files=list(root.glob('*.jsonl'))
    if len(files)>=128 or sum(f.stat().st_size for f in files)+len(header)>64*1024*1024:raise ValueError('HARNESS_RECEIPT_CAPACITY')
    fd=os.open(cfg['path'],os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
    log=os.fdopen(fd,'wb',buffering=0);log.write(header)
size=len(header)
def persist(line):
    global size
    with quota:
        quota.execute('BEGIN IMMEDIATE')
        if size+len(line)>limit or sum(f.stat().st_size for f in root.glob('*.jsonl'))+len(line)>64*1024*1024:return False
        log.write(line);size+=len(line)
    return True
child=subprocess.Popen(cfg['command'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL)
def input_loop():
    try:
        while True:
            line=sys.stdin.buffer.readline(limit+1)
            if not line or len(line)>limit:break
            child.stdin.write(line);child.stdin.flush()
    except (OSError,ValueError):pass
    finally:
        try:child.stdin.close()
        except (OSError,ValueError):pass
threading.Thread(target=input_loop,daemon=True).start()
forward=True
try:
    while True:
        line=child.stdout.readline(limit+1)
        if not line:break
        if len(line)>limit:break
        if not persist(line):break
        if forward:
            try:sys.stdout.buffer.write(line);sys.stdout.buffer.flush()
            except (BrokenPipeError,OSError):forward=False
finally:
    log.close();quota.close()
    if child.poll() is None:
        child.terminate()
        try:child.wait(timeout=3)
        except subprocess.TimeoutExpired:child.kill();child.wait(timeout=3)
    done=Path(cfg['path']).with_suffix('.done')
    fd=os.open(str(done)+'.next',os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
    with os.fdopen(fd,'w',encoding='utf-8') as f:json.dump(cfg['identity'],f)
    os.replace(str(done)+'.next',done)
"""

def _claude_wire_path(rid):
    return Path.home()/'.local/share/bob-native/claude-wire'/(hashlib.sha256(rid.encode()).hexdigest()+'.jsonl')

def _claude_prune_wire():
    root=_claude_wire_path('').parent
    if not root.is_dir():return 0
    removed=0
    with closing(sqlite3.connect(root/'quota.sqlite',timeout=10)) as quota,quota:
        quota.execute('BEGIN IMMEDIATE')
        records=[]
        with closing(_stream_db()) as db:
            for candidate in list(root.glob('*.jsonl'))[:128]:
                try:
                    if candidate.is_symlink():continue
                    with candidate.open('rb') as f:header=json.loads(f.readline(2049))
                    rid=header.get('request_id')
                    if not isinstance(rid,str) or len(rid)>200 or _claude_wire_path(rid)!=candidate:continue
                    row=db.execute('SELECT body FROM requests WHERE id=?',(rid,)).fetchone()
                    if row:records.append(json.loads(row[0]))
                except (OSError,ValueError,TypeError):continue
        for record in records:
            if not record.get('claude_wire') or record.get('execution_status') not in ('completed','cancelled'):continue
            ended=record.get('ended_at')
            if not isinstance(ended,(int,float)) or _now()-ended<7*24*3600*1000:continue
            path=_claude_wire_path(record['request_id']);done=path.with_suffix('.done')
            try:
                if path.is_symlink() or done.is_symlink() or not path.is_file() or done.stat().st_size>2048:continue
                expected={'session_id':record['session_id'],'request_id':record['request_id'],'user_uuid':record.get('native_user_message_id')}
                if json.loads(done.read_text(encoding='utf-8'))!=expected:continue
                result=_claude_wire_result(record)
                if result!=(record['execution_status'],record.get('reply_text','')):continue
                path.unlink();done.unlink();removed+=1
            except (OSError,ValueError,TypeError):continue
    return removed


def _claude_durable_pipe(command,cwd,env,sid,rid):
    path=_claude_wire_path(rid);path.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
    _claude_prune_wire()
    # Refuse new capture when the bounded spool is full; never delete live work.
    if len(list(path.parent.glob('*.jsonl')))>=128 or sum(f.stat().st_size for f in path.parent.glob('*.jsonl'))>=64*1024*1024:
        raise ValueError('HARNESS_RECEIPT_CAPACITY')
    user_uuid=str(uuid.uuid5(uuid.NAMESPACE_URL,'bob-native:'+sid+':'+rid))
    identity={'session_id':sid,'request_id':rid,'user_uuid':user_uuid}
    _stream_update(rid,native_user_message_id=user_uuid,claude_wire=True)
    return NativePipe([sys.executable,'-u','-c',CLAUDE_WIRE_RELAY,json.dumps({'path':str(path),'identity':identity,'command':command})],cwd,env)

def _project_claude_activity(previous, frame):
    """Project structured tool blocks only; never infer success from prose."""
    message=frame.get('message') or {}
    blocks=message.get('content')
    if frame.get('type') not in ('assistant','user') or not isinstance(blocks,list):return previous
    activity=json.loads(json.dumps(previous or {'items':[],'approvals':[],'partial':False}))
    entries=activity['items']
    for block in blocks:
        if not isinstance(block,dict):continue
        kind=block.get('type')
        identity=block.get('id') if kind=='tool_use' else block.get('tool_use_id')
        if kind not in ('tool_use','tool_result') or not isinstance(identity,str) or not identity or len(identity)>128:continue
        old=next((x for x in entries if x['id']==identity),None)
        if kind=='tool_use':
            if old:continue
            entries.append({'id':identity,'type':'tool','label':str(block.get('name') or 'tool')[:512],'status':'running'})
        else:
            state='error' if block.get('is_error') is True else 'completed'
            if old:old['status']=state
            else:entries.append({'id':identity,'type':'tool','label':'tool','status':state})
        if len(entries)>64:del entries[:-64];activity['partial']=True
    return activity

def _claude_wire_result(record, activity_out=None, progress_out=None):
    if not record.get('claude_wire') or not record.get('native_user_message_id'):return None
    try:
        path=_claude_wire_path(record['request_id'])
        if path.is_symlink() or path.stat().st_size>8*1024*1024:return None
        with path.open('rb') as f:data=f.read(8*1024*1024+1)
        lines=data.splitlines(keepends=True)
        header=json.loads(lines[0]);sid=record['session_id'];rid=record['request_id']
        if header!={'session_id':sid,'request_id':rid,'user_uuid':record['native_user_message_id']}:return None
        text='';ack=False;aborted=False;identity=False;activity=None
        for line in lines[1:]:
            if not line.endswith(b'\n'):break
            frame=json.loads(line);raw=frame.get('session_id')
            if raw is not None:
                if raw!=sid.split(':',1)[1]:return None
                identity=True
            response=frame.get('response') or {}
            if frame.get('type')=='control_response' and response.get('request_id')=='bob-cancel-'+rid:
                ack=response.get('subtype')=='success'
            if identity:
                activity=_project_claude_activity(activity,frame)
                event=frame.get('event') or {};delta=event.get('delta') or {}
                if event.get('type')=='content_block_delta' and delta.get('type')=='text_delta':text+=delta.get('text','')
                if len(text)>262144:return None
                if frame.get('type')=='result':
                    if frame.get('subtype')=='success' and not frame.get('is_error') and isinstance(frame.get('result'),str):
                        if len(frame['result'])>262144:return None
                        if activity_out is not None and activity:activity_out.update(activity)
                        return 'completed',frame['result']
                    aborted=frame.get('terminal_reason')=='aborted_streaming'
            if identity and ack and aborted:
                if activity_out is not None and activity:activity_out.update(activity)
                return 'cancelled',text or record.get('reply_text','')
        # This is observed progress, not a confirmed execution outcome.
        # Publish only after the entire complete-frame prefix passed identity checks.
        if identity and progress_out is not None:
            progress_out.update(text=text,activity=activity)
    except (OSError,ValueError,KeyError,IndexError,TypeError):pass
    return None


def _read_codex_receipt_turn(pipe, raw, turn_id, on_event):
    """Bounded official summaries, then items for only the acknowledged turn.

    Exhaustion or unsupported pagination leaves the receipt unknown. Never fall
    back to a full-history read or infer a result from a neighbouring turn.
    """
    deadline=time.monotonic()+20;request_id=10;cursor=None;seen=set();found=None
    for _ in range(3):
        if time.monotonic()>=deadline:return None
        params={'threadId':raw,'limit':20,'sortDirection':'desc','itemsView':'summary'}
        if cursor:params['cursor']=cursor
        pipe.send({'id':request_id,'method':'thread/turns/list','params':params})
        page=_control_result(pipe,request_id,on_event);request_id+=1
        turns=page.get('data') or []
        if not isinstance(turns,list) or len(turns)>20:return None
        found=next((t for t in turns if t.get('id')==turn_id),None)
        if found:break
        cursor=page.get('nextCursor')
        if not isinstance(cursor,str) or not cursor or cursor in seen:return None
        seen.add(cursor)
    if not found or found.get('status') not in ('completed','failed','interrupted'):return None
    items=[];cursor=None;seen=set();size=0
    for _ in range(16):
        if time.monotonic()>=deadline:return None
        params={'threadId':raw,'turnId':turn_id,'limit':20,'sortDirection':'asc'}
        if cursor:params['cursor']=cursor
        pipe.send({'id':request_id,'method':'thread/items/list','params':params})
        page=_control_result(pipe,request_id,on_event);request_id+=1
        batch=page.get('data') or []
        if not isinstance(batch,list) or len(batch)>20:return None
        size+=len(json.dumps(batch,ensure_ascii=False).encode('utf-8'))
        if size>1048576:return None
        if any(not isinstance(e,dict) or e.get('turnId')!=turn_id or not isinstance(e.get('item'),dict) for e in batch):return None
        items.extend(e['item'] for e in batch);cursor=page.get('nextCursor')
        if not cursor:return dict(found,items=items)
        if not isinstance(cursor,str) or cursor in seen:return None
        seen.add(cursor)
    return None


def reconcile_codex_turn(record):
    """Read an exact acknowledged turn; never resume, prompt, or match by text."""
    raw=record['session_id'].split(':',1)[1]
    turn_id=record.get('native_turn_id')
    if not turn_id or record.get('native_session_id')!=raw: return record
    env=os.environ.copy()
    env['PATH']=os.pathsep.join(str(Path.home()/p) for p in ['.local/bin','.npm-global/bin','.cargo/bin'])+os.pathsep+env.get('PATH','')
    exe=shutil.which('codex',path=env['PATH'])
    if not exe: return record
    pipe=None
    try:
        pipe=NativePipe([exe,'app-server'],str(Path.home()),env)
        pipe.send({'id':1,'method':'initialize','params':{'clientInfo':{'name':'bob-receipt-reader','version':'1.0.1'},'capabilities':{'experimentalApi':True}}})
        def deny_request(frame):
            if 'id' in frame:
                pipe.send({'id':frame['id'],'error':{'code':-32000,'message':'Receipt inspection is read-only'}})
        _control_result(pipe,1,deny_request)
        pipe.send({'method':'initialized','params':{}})
        pipe.send({'id':2,'method':'thread/read','params':{'threadId':raw,'includeTurns':False}})
        thread=(_control_result(pipe,2,deny_request).get('thread') or {})
        if thread.get('id')!=raw:return record
        turn=_read_codex_receipt_turn(pipe,raw,turn_id,deny_request)
        if turn:
            status=turn.get('status')
            if status not in ('completed','failed','interrupted'):return record
            fold=CodexTurnStream(raw);fold.bind({'turn':{'id':turn_id}})
            for item in turn.get('items') or []:fold.completed_item(item)
            reply='\n\n'.join(t for t in fold.items.values() if t)
            if len(reply)>262144 or (status=='completed' and not reply.strip()):return record
            state={'completed':'completed','failed':'error','interrupted':'cancelled'}[status]
            with closing(_stream_db()) as db, db:
                db.execute('BEGIN IMMEDIATE')
                row=db.execute('SELECT body FROM requests WHERE id=? AND session=?',(record['request_id'],record['session_id'])).fetchone()
                if not row:return record
                current=json.loads(row[0])
                if current.get('execution_status')!='unknown' or current.get('native_turn_id')!=turn_id:return current
                current.update(execution_status=state,reply_text=reply,ended_at=_now(),reconciled_by='codex_bounded_turn_read')
                current.pop('error_code',None)
                if state!='completed':current['error_code']='HARNESS_TURN_'+status.upper()
                db.execute('UPDATE requests SET body=? WHERE id=?',(json.dumps(current,ensure_ascii=False),record['request_id']))
                snapshot=db.execute('SELECT body FROM event_snapshots WHERE request=? AND session=?',(record['request_id'],record['session_id'])).fetchone()
                previous=json.loads(snapshot[0]).get('activity') if snapshot else None
                activity=_activity_from_codex_turn(previous,turn)
                if activity is not None:_append_event(db,record,'activity_snapshot',{'activity':activity})
                for kind,payload in [('message_snapshot',{'text':reply}),(state,{'code':current.get('error_code')})]:
                    _append_event(db,record,kind,payload)
                return current
    except Exception:
        return record
    finally:
        if pipe:pipe.close()
    return record


class CodexTurnStream:
    """Fold only one acknowledged turn; final item text repairs missing deltas."""
    def __init__(self, thread_id):
        self.thread_id = thread_id
        self.turn_id = None
        self.items = {}
        self.completed_items = set()

    def bind(self, result):
        turn_id = (result.get('turn') or {}).get('id')
        if not isinstance(turn_id, str) or not turn_id:
            raise ValueError('HARNESS_IDENTITY_UNCONFIRMED')
        if self.turn_id is not None and self.turn_id != turn_id:
            raise ValueError('HARNESS_IDENTITY_UNCONFIRMED')
        self.turn_id = turn_id

    def completed_item(self, item):
        if item.get('type') != 'agentMessage': return
        item_id, text = item.get('id'), item.get('text')
        if not isinstance(item_id, str) or not item_id or not isinstance(text, str):
            raise ValueError('HARNESS_IDENTITY_UNCONFIRMED')
        self.items[item_id] = text
        self.completed_items.add(item_id)

    def fold(self, method, params):
        if params.get('threadId') != self.thread_id or not self.turn_id:
            return None, False
        turn = params.get('turn') or {}
        turn_id = turn.get('id') if method == 'turn/completed' else params.get('turnId')
        if turn_id != self.turn_id: return None, False
        terminal = False
        if method == 'item/agentMessage/delta':
            item_id, delta = params.get('itemId'), params.get('delta')
            if not isinstance(item_id, str) or not item_id or not isinstance(delta, str):
                raise ValueError('HARNESS_IDENTITY_UNCONFIRMED')
            if item_id not in self.completed_items:
                self.items[item_id] = self.items.get(item_id, '') + delta
        elif method == 'item/completed':
            self.completed_item(params.get('item') or {})
        elif method == 'turn/completed':
            if turn.get('status') != 'completed': raise ValueError('HARNESS_TURN_FAILED')
            # Some versions also return final items here; an empty items array
            # does not invalidate item/completed notifications already received.
            for item in turn.get('items') or []: self.completed_item(item)
            terminal = True
        else:
            return None, False
        text = '\n\n'.join(value for value in self.items.values() if value)
        if len(text) > 262144: raise ValueError('HARNESS_OUTPUT_LIMIT')
        return text, terminal


def _claude_initialize(pipe, raw, text):
    identity='bob-initialize-'+str(uuid.uuid4())
    pipe.send({'type':'control_request','request_id':identity,'request':{'subtype':'initialize','hooks':{}}})
    deadline=time.monotonic()+30
    while time.monotonic()<deadline:
        frame=pipe.receive(.5)
        if frame is None:continue
        response=frame.get('response') or {}
        if frame.get('type')=='control_response' and response.get('request_id')==identity:
            if response.get('subtype')!='success':raise ValueError('HARNESS_INITIALIZE_REJECTED')
            return
    raise ValueError('HARNESS_TIMEOUT')


def _claude_pid_has_remote_control(pid):
    try:
        p=int(pid or 0)
        if p<=0:return False
        raw_cmd=(Path('/proc')/str(p)/'cmdline').read_bytes()
        parts=[seg.decode('utf-8','ignore') for seg in raw_cmd.split(b'\x00') if seg]
        return any(arg in ('--remote-control','--rc','remote-control') or arg.startswith('--remote-control=') for arg in parts)
    except Exception:
        return False


def _inspect_claude_session_holder(exe, raw, env):
    result=subprocess.run([exe,'agents','--json'],cwd=str(Path.home()),env=env,
        stdin=subprocess.DEVNULL,capture_output=True,timeout=8)
    if result.returncode or len(result.stdout)>1048576:raise ValueError()
    rows=json.loads(result.stdout)
    if not isinstance(rows,list) or len(rows)>1024:raise ValueError()
    for row in rows:
        if not isinstance(row,dict) or not isinstance(row.get('sessionId'),str) or not row['sessionId']:
            raise ValueError()
        if row['sessionId']==raw:
            pid=int(row.get('pid') or 0)
            return {
                'busy':True,
                'pid':pid,
                'name':str(row.get('name') or '')[:200],
                'kind':str(row.get('kind') or 'interactive')[:64],
                'status':str(row.get('status') or 'idle')[:64],
                'remote_control':_claude_pid_has_remote_control(pid),
            }
    return {'busy':False,'remote_control':False}


def _release_claude_terminal_holder(exe, raw, env):
    holder=_inspect_claude_session_holder(exe,raw,env)
    if not holder.get('busy'):return holder
    if holder.get('remote_control'):raise ValueError('HARNESS_CLAUDE_REMOTE_CONTROL')
    pid=int(holder.get('pid') or 0)
    if pid<=0 or pid==os.getpid():raise ValueError('HARNESS_SESSION_BUSY')
    try:
        proc_dir=Path('/proc')/str(pid)
        if hasattr(os,'getuid') and proc_dir.exists() and proc_dir.stat().st_uid!=os.getuid():
            raise ValueError('HARNESS_SESSION_BUSY')
        cmdline=(proc_dir/'cmdline').read_bytes().decode('utf-8','ignore').lower() if (proc_dir/'cmdline').exists() else 'claude'
        if 'claude' not in cmdline or 'native_worker' in cmdline:
            raise ValueError('HARNESS_SESSION_BUSY')
        os.kill(pid,signal.SIGTERM)
    except ValueError:
        raise
    except OSError:
        pass
    deadline=time.monotonic()+3.5
    while time.monotonic()<deadline:
        time.sleep(0.25)
        holder=_inspect_claude_session_holder(exe,raw,env)
        if not holder.get('busy'):return holder
    return holder


def _safe_claude_holder(sid):
    if not isinstance(sid,str) or not sid.startswith('claude-jsonl:'):return None
    raw=sid.split(':',1)[1]
    env=os.environ.copy()
    env['PATH']=os.pathsep.join(str(Path.home()/p) for p in ['.local/bin','.npm-global/bin','.cargo/bin'])+os.pathsep+env.get('PATH','')
    exe=shutil.which('claude',path=env['PATH'])
    if not exe:return None
    try:
        return _inspect_claude_session_holder(exe,raw,env)
    except Exception:
        return None


def _ensure_claude_session_available(exe, raw, env):
    """Official active-session inventory, never terminal-log scraping.

    An idle interactive process still owns its session. Its presence forbids
    starting another resume process; missing/invalid inventory is not absence.
    """
    try:
        holder=_inspect_claude_session_holder(exe,raw,env)
        if holder.get('busy'):raise ValueError('HARNESS_SESSION_BUSY')
    except Exception as exc:
        if isinstance(exc,ValueError) and str(exc)=='HARNESS_SESSION_BUSY':raise
        raise ValueError('HARNESS_ACTIVITY_UNCONFIRMED') from None


def _permission_schema(db):
    db.execute('CREATE TABLE IF NOT EXISTS native_permissions (id TEXT PRIMARY KEY, request TEXT NOT NULL, session TEXT NOT NULL, body TEXT NOT NULL)')


def _permission_capacity(db):
    # Bounded metadata; expired records never become reusable authorizations.
    if db.execute('SELECT COUNT(*) FROM native_permissions').fetchone()[0]<1024:return True
    now=_now()
    for pid,body,request_body in db.execute('SELECT p.id,p.body,r.body FROM native_permissions p LEFT JOIN requests r ON r.id=p.request AND r.session=p.session ORDER BY p.rowid LIMIT 1024').fetchall():
        entry=json.loads(body);request=json.loads(request_body) if request_body else {}
        if entry['expires_at']<now and request.get('execution_status') not in ('accepted','running'):
            db.execute('DELETE FROM native_permissions WHERE id=?',(pid,))
    return db.execute('SELECT COUNT(*) FROM native_permissions').fetchone()[0]<1024


def _permission_activity(db, cfg, entry, status):
    row = db.execute('SELECT body FROM event_snapshots WHERE request=? AND session=?', (cfg['request_id'], cfg['session_id'])).fetchone()
    state = json.loads(row[0]) if row else {}
    activity = state.get('activity') or {'items': [], 'approvals': [], 'partial': False}
    key = json.dumps(entry['frame']['id'])
    items = activity.setdefault('approvals', [])
    old = next((x for x in items if x['id'] == key), None)
    if old is None:
        old = {'id': key, 'method': 'can_use_tool' if entry.get('protocol')=='claude' else 'session/request_permission'}
        items.append(old)
    old.update(status=status, permission_id=entry['id'], options=entry['options'],
        title=entry['title'], detail=entry.get('detail',''), actionable=status == 'pending', expires_at=entry['expires_at'])
    if len(items)>32:
        del items[:-32]
        activity['partial']=True
    _append_event(db, cfg, 'activity_snapshot', {'activity': activity})


def _permission_owner(rec, sid):
    if sid.startswith('agy:'):return rec.get('supervisor_token')
    if sid.startswith('claude-jsonl:'):return 'worker:'+str(rec.get('request_id') or '')
    return None

def _permission_owner_alive(rec, sid):
    if sid.startswith('agy:'):return _agy_process_owned(rec.get('pid'),rec.get('supervisor_token'),sid)
    if not sid.startswith('claude-jsonl:'):return False
    try:
        pid=int(rec.get('pid') or 0)
        if pid<=0:return False
        argv=(Path('/proc')/str(pid)/'cmdline').read_bytes()
        return all(v.encode() in argv for v in ('native_worker',sid,rec['request_id']))
    except (OSError,ValueError,KeyError):return False

def _handle_claude_permission(cfg, pipe, frame):
    request=frame.get('request') or {};identity=frame.get('request_id')
    if not isinstance(identity,str) or not identity or len(identity)>128:raise ValueError('HARNESS_PROTOCOL_INVALID')
    if request.get('subtype')!='can_use_tool':
        pipe.send({'type':'control_response','response':{'subtype':'error','request_id':identity,'error':'Unsupported control request'}})
        return
    tool_input=request.get('input')
    if not isinstance(tool_input,dict):raise ValueError('HARNESS_PROTOCOL_INVALID')
    tool_name=request.get('tool_name') or 'tool'
    options=[{'optionId':'allow_once','kind':'allow_once'},{'optionId':'reject_once','kind':'reject_once'}]
    if tool_name=='AskUserQuestion' and isinstance(tool_input.get('questions'),list) and tool_input['questions']:
        q0=tool_input['questions'][0] if isinstance(tool_input['questions'][0],dict) else {}
        q_opts=[]
        for idx,opt in enumerate((q0.get('options') or [])[:8]):
            if not isinstance(opt,dict):continue
            lbl=str(opt.get('label') or opt.get('value') or f'Option {idx+1}').strip()[:120]
            desc=str(opt.get('description') or '').strip()[:120]
            if lbl:
                q_opts.append({'optionId':f'answer_{idx}','kind':'allow_once','name':f'{lbl} — {desc}' if desc else lbl,'answerValue':lbl})
        if q_opts:
            q_opts.append({'optionId':'reject_once','kind':'reject_once','name':'跳过 / 暂不回答'})
            options=q_opts
    normalized={'id':identity,'protocol':'claude','original_input':tool_input,'params':{
        'sessionId':cfg['session_id'].split(':',1)[1],
        'toolCall':{'title':tool_name,'input':tool_input,'tool_use_id':request.get('tool_use_id')},
        'options':options}}
    if tool_name in cfg.get('claude_tools',[]) and len(json.dumps(tool_input,ensure_ascii=False))<=8192 and cfg.get('approval_mode')=='interactive' and _open_native_permission(cfg,normalized,1):return
    pipe.send({'type':'control_response','response':{'subtype':'success','request_id':identity,'response':{'behavior':'deny','message':'Tool execution has not been approved'}}})

def _open_native_permission(cfg, frame, serial):
    identity=frame.get('id');params=frame.get('params') or {}
    if isinstance(identity,bool) or not isinstance(identity,(str,int)) or len(str(identity))>128:return False
    if params.get('sessionId') != cfg['session_id'].split(':',1)[1]:return False
    options=[]
    for option in params.get('options') or []:
        if not isinstance(option,dict):continue
        if option.get('kind') not in ('allow_once','reject_once'):continue
        oid=option.get('optionId')
        if isinstance(oid,str) and oid and len(oid)<=128 and not any(x['id']==oid for x in options):
            item={'id':oid,'kind':option['kind'],'label':str(option.get('name') or option['kind'])[:256]}
            if isinstance(option.get('answerValue'),str) and option['answerValue']:
                item['answer_value']=option['answerValue'][:128]
            options.append(item)
    with closing(_stream_db()) as db,db:
        db.execute('BEGIN IMMEDIATE');_permission_schema(db)
        row=db.execute('SELECT body FROM requests WHERE id=? AND session=?',(cfg['request_id'],cfg['session_id'])).fetchone()
        rec=json.loads(row[0]) if row else {}
        if rec.get('execution_status')!='running' or not _permission_owner(rec,cfg['session_id']):return False
        for (body,) in db.execute('SELECT body FROM native_permissions WHERE request=?',(cfg['request_id'],)):
            prior=json.loads(body)
            if prior['serial']==serial and json.dumps(prior['frame']['id'])==json.dumps(identity):return True
        if db.execute('SELECT COUNT(*) FROM native_permissions WHERE request=?',(cfg['request_id'],)).fetchone()[0]>=32:return False
        if not _permission_capacity(db):return False
        tool=params.get('toolCall') if isinstance(params.get('toolCall'),dict) else {}
        detail=json.dumps(tool,ensure_ascii=False)
        if len(detail)>8192:
            options=[x for x in options if x['kind']!='allow_once']
            detail=detail[:8192]+'…'
        entry={'id':str(uuid.uuid4()),'frame':{'id':identity},'serial':serial,'options':options,'title':str(tool.get('title') or '')[:512],'detail':detail,
            'owner':_permission_owner(rec,cfg['session_id']),'pid':rec['pid'],'expires_at':_now()+300000,'status':'pending'}
        if frame.get('protocol')=='claude':
            entry.update(protocol='claude',original_input=frame['original_input'])
        db.execute('INSERT INTO native_permissions VALUES(?,?,?,?)',(entry['id'],cfg['request_id'],cfg['session_id'],json.dumps(entry)))
        _permission_activity(db,cfg,entry,'pending')
    return True


def submit_native_permission(cfg):
    sid,rid=cfg.get('sessionId'),cfg.get('requestId');pid=cfg.get('permissionId');choice=cfg.get('optionId')
    if not isinstance(sid,str) or not sid.startswith(('agy:','claude-jsonl:')):raise ValueError('HARNESS_APPROVAL_UNSUPPORTED')
    _stream_records(sid)
    with closing(_stream_db()) as db,db:
        db.execute('BEGIN IMMEDIATE');_permission_schema(db)
        row=db.execute('SELECT body FROM native_permissions WHERE id=? AND request=? AND session=?',(pid,rid,sid)).fetchone()
        if not row:raise ValueError('HARNESS_APPROVAL_NOT_FOUND')
        entry=json.loads(row[0])
        recrow=db.execute('SELECT body FROM requests WHERE id=? AND session=?',(rid,sid)).fetchone()
        rec=json.loads(recrow[0]) if recrow else {}
        if rec.get('execution_status')!='running' or _permission_owner(rec,sid)!=entry['owner'] or rec.get('pid')!=entry['pid'] or not _permission_owner_alive(rec,sid):
            raise ValueError('HARNESS_APPROVAL_EXPIRED')
        if entry['status']!='pending':
            if entry.get('choice')!=choice:raise ValueError('HARNESS_APPROVAL_CONFLICT')
            return {'status':entry['status'],'permission_id':pid}
        if _now()>=entry['expires_at']:raise ValueError('HARNESS_APPROVAL_EXPIRED')
        if choice is not None and not any(x['id']==choice for x in entry['options']):raise ValueError('HARNESS_APPROVAL_OPTION_INVALID')
        if sid.startswith('claude-jsonl:') and any(x['id']==choice and x['kind']=='allow_once' for x in entry['options']):
            policy=rec.get('tool_authorization') or {}
            current=cfg.get('claudeTools',[])
            if policy.get('version')!=1 or policy.get('approval_mode')!='interactive' or entry.get('title') not in policy.get('tools',[]) or not isinstance(current,list) or entry.get('title') not in current:
                raise ValueError('HARNESS_TOOL_NOT_GRANTED')
        entry.update(status='queued',choice=choice)
        db.execute('UPDATE native_permissions SET body=? WHERE id=?',(json.dumps(entry),pid))
        _permission_activity(db,{'session_id':sid,'request_id':rid},entry,'queued')
        return {'status':'queued','permission_id':pid}


def _flush_native_permissions(cfg, pipe):
    with closing(_stream_db()) as db,db:
        db.execute('BEGIN IMMEDIATE');_permission_schema(db)
        request=db.execute('SELECT body FROM requests WHERE id=? AND session=?',(cfg['request_id'],cfg['session_id'])).fetchone()
        rec=json.loads(request[0]) if request else {}
        if rec.get('execution_status')!='running':return
        cancelled=bool(rec.get('cancel_requested_at'))
        ready=[]
        for (body,) in db.execute('SELECT body FROM native_permissions WHERE request=? AND session=?',(cfg['request_id'],cfg['session_id'])):
            entry=json.loads(body)
            if entry['status'] in ('pending','queued') and (_now()>=entry['expires_at'] or cancelled):
                entry.update(status='queued',choice=None)
            if entry['status']=='queued':
                entry['status']='dispatching'
                db.execute('UPDATE native_permissions SET body=? WHERE id=?',(json.dumps(entry),entry['id']))
                ready.append(entry)
    # Claim once before writing. A failed or interrupted write is never retried.
    for entry in ready:
        choice=entry.get('choice')
        outcome={'outcome':'cancelled'} if choice is None else {'outcome':'selected','optionId':choice}
        selected_opt=next((x for x in entry['options'] if x['id']==choice),None)
        allowed=bool(selected_opt and selected_opt.get('kind')=='allow_once')
        if entry.get('protocol')=='claude':
            if allowed:
                updated_input=entry['original_input']
                if entry.get('title')=='AskUserQuestion' and selected_opt and selected_opt.get('answer_value') and isinstance(updated_input,dict):
                    updated_input=dict(updated_input)
                    qs=updated_input.get('questions')
                    q_key=(qs[0].get('question') if isinstance(qs,list) and qs and isinstance(qs[0],dict) else None) or 'question'
                    updated_input['answers']={str(q_key):selected_opt['answer_value']}
                answer={'behavior':'allow','updatedInput':updated_input}
            else:
                answer={'behavior':'deny','message':'Tool execution was not approved'}
            pipe.send({'type':'control_response','response':{'subtype':'success','request_id':entry['frame']['id'],'response':answer}})
        else:pipe.send({'jsonrpc':'2.0','id':entry['frame']['id'],'result':{'outcome':outcome}})
        with closing(_stream_db()) as db,db:
            entry['status']='sent'
            db.execute('UPDATE native_permissions SET body=? WHERE id=?',(json.dumps(entry),entry['id']))
            _permission_activity(db,cfg,entry,'approved' if allowed else 'denied')
            _append_event(db,cfg,'permission_decision',{'permission_id':entry['id'],'local_decision':'approved' if allowed else 'denied'})


class AgyAcpSession:
    """One official connection; load replay is never a new execution event."""
    def __init__(self, exe, raw, cwd, env):
        self.raw, self.serial = raw, 0
        self.pipe = NativePipe([exe, 'acp', '--stdio'], cwd, env)
        try:
            result = self.control('initialize', {'protocolVersion': 1,
                'clientCapabilities': {}, 'clientInfo': {'name': 'bob-native', 'version': '1.0.1'}})
            if raw is None:
                result = self.control('session/new', {'cwd': cwd, 'mcpServers': []})
                self.raw = validate_new_native_id(result.get('sessionId'))
            else:
                if not result.get('agentCapabilities', {}).get('loadSession'):
                    raise ValueError('HARNESS_NATIVE_RESUME_UNSUPPORTED')
                result = self.control('session/load', {'sessionId': raw, 'cwd': cwd, 'mcpServers': []})
                if result.get('sessionId', raw) != raw:
                    raise ValueError('HARNESS_SESSION_MISMATCH')
        except Exception:
            self.close()
            raise

    def close(self):
        self.pipe.close()

    def request(self, method, params):
        self.serial += 1
        self.pipe.send({'jsonrpc': '2.0', 'id': self.serial, 'method': method, 'params': params})
        return self.serial

    def deny_request(self, frame):
        if 'id' not in frame or not frame.get('method'): return False
        if frame['method'] == 'session/request_permission':
            response = {'result': {'outcome': {'outcome': 'cancelled'}}}
        else:
            response = {'error': {'code': -32601, 'message': 'Client operation not enabled'}}
        self.pipe.send({'jsonrpc': '2.0', 'id': frame['id'], **response})
        return True

    def control(self, method, params):
        wanted = self.request(method, params)
        deadline = time.monotonic() + 180
        while time.monotonic() < deadline:
            frame = self.pipe.receive(.5)
            if frame is None: continue
            if self.deny_request(frame): continue
            # session/load may replay hundreds of updates, including tool calls.
            # Consume these without storing them under the next request ID.
            if frame.get('method'): continue
            if frame.get('id') != wanted: continue
            if 'error' in frame: raise ValueError('HARNESS_RESUME_REJECTED')
            return frame.get('result') or {}
        raise ValueError('HARNESS_TIMEOUT')

    def prompt(self, cfg):
        rid = cfg['request_id']
        if _cancel_requested(rid):
            _stream_update(rid, execution_status='cancelled', ended_at=_now(), cancel_confirmed_by='not_dispatched')
            _stream_event(cfg, 'cancelled')
            return
        # Persist dispatch intent before writing. A lost response stays unknown;
        # ACP has no proved exact-turn lookup, so never retry the prompt here.
        _stream_update(rid, execution_status='running', pid=os.getpid(),
            native_session_id=self.raw, turn_start_requested=True, transport='agy-acp')
        _stream_event(cfg, 'running')
        wanted = self.request('session/prompt', {'sessionId': self.raw,
            'prompt': [{'type': 'text', 'text': cfg['text']}]})
        _stream_update(rid, acp_request_id=wanted)
        reply, cancel_sent = '', False
        while True:
            if not cancel_sent and _cancel_requested(rid):
                self.pipe.send({'jsonrpc': '2.0', 'method': 'session/cancel', 'params': {'sessionId': self.raw}})
                cancel_sent = True
                _stream_update(rid, cancel_dispatched_at=_now())
            if cfg.get('approval_mode') == 'interactive':
                _flush_native_permissions(cfg, self.pipe)
            frame = self.pipe.receive(.5)
            if frame is None: continue
            params = frame.get('params') or {}
            if frame.get('method'):
                if params.get('sessionId') != self.raw:
                    self.deny_request(frame)
                    continue
                if len(json.dumps(frame, ensure_ascii=False).encode('utf-8')) > 262144:
                    raise ValueError('HARNESS_OUTPUT_LIMIT')
                _capture_agy_event(cfg, frame, wanted)
                if frame['method'] == 'session/request_permission' and cfg.get('approval_mode') == 'interactive':
                    if _open_native_permission(cfg, frame, wanted): continue
                if self.deny_request(frame):
                    if frame['method'] == 'session/request_permission':
                        _capture_agy_event(cfg, frame, wanted, decision='denied')
                    continue
                update = params.get('update') or {}
                if frame['method'] == 'session/update' and update.get('sessionUpdate') == 'agent_message_chunk':
                    content = update.get('content') or {}
                    delta = content.get('text', '') if content.get('type') == 'text' else ''
                    if not isinstance(delta, str): raise ValueError('HARNESS_TRANSPORT_FAILED')
                    if delta:
                        reply += delta
                        if len(reply) > 262144: raise ValueError('HARNESS_OUTPUT_LIMIT')
                        _stream_event(cfg, 'text_delta', {'text': delta})
                        _stream_update(rid, reply_text=reply)
                continue
            if frame.get('id') != wanted: continue
            if 'error' in frame: raise ValueError('HARNESS_TURN_FAILED')
            reason = (frame.get('result') or {}).get('stopReason')
            if reason not in ('end_turn', 'cancelled'):
                raise ValueError('HARNESS_OUTPUT_TRUNCATED' if reason in ('max_tokens', 'max_turn_requests') else 'HARNESS_TURN_FAILED')
            state = 'cancelled' if reason == 'cancelled' else 'completed'
            # An authoritative end_turn may contain only tool/permission events.
            # Empty visible text is not an unknown execution or reason to replay.
            _stream_event(cfg, 'source_event', {'source': 'agy-acp', 'method': 'session/prompt_result',
                'acp_request_id': wanted, 'session_id': cfg['session_id'], 'stop_reason': reason})
            _stream_update(rid, execution_status=state, ended_at=_now(),
                cancel_confirmed_by='acp_stop_reason' if state == 'cancelled' else None)
            _stream_event(cfg, state)
            return



def _claude_tool_arguments(cfg):
    # Only the host grant supplies this list. Never accept wildcard/default
    # bundles: each enabled tool must pass the existing one-shot approval path.
    tools=cfg.get('claude_tools',[])
    if not isinstance(tools,list) or len(tools)>32 or any(not isinstance(t,str) or not re.fullmatch(r'[A-Z][A-Za-z0-9_]{0,63}',t) or t=='Default' for t in tools):
        raise ValueError('HARNESS_TOOL_POLICY_INVALID')
    tools=list(dict.fromkeys(tools))
    if cfg.get('approval_mode')!='interactive' or not tools:
        return ['--tools','','--permission-mode','plan']
    return ['--tools',','.join(tools),'--permission-mode','default',
        '--permission-prompt-tool','stdio','--settings',
        json.dumps({'permissions':{'ask':tools}},ensure_ascii=False)]

def validate_new_native_id(raw):
    if not isinstance(raw, str) or not re.fullmatch(r'[a-zA-Z0-9-]{8,80}', raw):
        raise ValueError('HARNESS_IDENTITY_UNCONFIRMED')
    return raw


def _native_worker(cfg):
    sid, rid, text = cfg['session_id'], cfg['request_id'], cfg['text']
    tool, raw = sid.split(':', 1)
    creating = cfg.get('new_harness')
    tool = creating or ('claude' if tool == 'claude-jsonl' else tool)
    env = os.environ.copy()
    env['PATH'] = os.pathsep.join(str(Path.home()/p) for p in ['.local/bin','.npm-global/bin','.cargo/bin'])+os.pathsep+env.get('PATH','')
    exe = shutil.which(tool, path=env['PATH']); pipe = None
    reply = ''; terminal = False; identity = False; cancel_sent = False; cancelled = False
    claude_cancel_ack=False;claude_cancel_terminal=False;claude_cancel_deadline=None
    codex_stream = CodexTurnStream(raw)
    pending_frames = deque(); ready_frames = deque(); pending_bytes = 0
    try:
        if _cancel_requested(rid):
            _stream_update(rid,execution_status='cancelled',ended_at=_now(),cancel_confirmed_by='not_dispatched')
            _stream_event(cfg,'cancelled');return
        if not exe: raise ValueError('HARNESS_EXECUTABLE_MISSING')
        cwd = cfg['work_cwd']
        if not Path(cwd).is_dir(): raise ValueError('HARNESS_WORKSPACE_MISSING')
        if tool == 'codex':
            pipe = NativePipe([exe,'app-server'],cwd,env)
            pipe.send({'id':1,'method':'initialize','params':{'clientInfo':{'name':'bob-native','version':'1.0.1'}}})
            init_deadline=time.monotonic()+30
            while True:
                frame=pipe.receive(timeout=max(1,init_deadline-time.monotonic()))
                if frame.get('method'):
                    if 'id' in frame:pipe.send({'id':frame['id'],'error':{'code':-32000,'message':'Bob read-only verification denies tool approval'}})
                    if time.monotonic()>=init_deadline:raise ValueError('HARNESS_TIMEOUT')
                    continue
                if frame.get('id')==1:
                    if 'error' in frame:raise ValueError('HARNESS_INITIALIZE_REJECTED')
                    break
                if time.monotonic()>=init_deadline:raise ValueError('HARNESS_TIMEOUT')
            pipe.send({'method':'initialized','params':{}})
            params = {'cwd':cwd, 'approvalPolicy':'never', 'sandbox':'read-only'}
            if not creating:
                params['threadId'] = raw
            pipe.send({'id':2, 'method':'thread/start' if creating else 'thread/resume', 'params':params})
        elif tool == 'claude':
            if not creating:
                if cfg.get('takeover') is True:
                    _release_claude_terminal_holder(exe,raw,env)
                _ensure_claude_session_available(exe,raw,env)
            session_flag = '--session-id' if creating else '--resume'
            pipe = _claude_durable_pipe([exe,session_flag,raw,'-p','--input-format','stream-json','--output-format','stream-json','--verbose',
                '--include-partial-messages']+_claude_tool_arguments(cfg),cwd,env,sid,rid)
            _claude_initialize(pipe,raw,text)
            if _cancel_requested(rid):
                _stream_update(rid,execution_status='cancelled',ended_at=_now(),cancel_confirmed_by='not_dispatched')
                _stream_event(cfg,'cancelled');return
            user_uuid=str(uuid.uuid5(uuid.NAMESPACE_URL,'bob-native:'+sid+':'+rid))
            # Persist the exact input anchor before dispatch; text matching is
            # insufficient when identical prompts occur in the same session.
            _stream_update(rid,native_user_message_id=user_uuid,native_session_id=raw)
            pipe.send({'type':'user','uuid':user_uuid,'session_id':raw,'parent_tool_use_id':None,'message':{'role':'user','content':text}})
        elif tool == 'pi':
            model_args=[]
            profile=Path.home()/'.local/share/bob-native/pi-profile.json'
            if profile.is_file():
                settings=json.loads(profile.read_text(encoding='utf-8'))
                env['PI_CODING_AGENT_DIR']=str(profile.parent/'pi')
                env['BOB_PI_API_KEY']=settings['api_key']
                model_args=['--provider',settings['provider'],'--model',settings['model'],'--thinking','off']
            session_args = ['--session-dir',cfg['pi_session_dir']] if creating else ['--session',cfg['jsonl_path']]
            pipe = NativePipe([exe,*model_args,'--mode','rpc',*session_args,'--no-tools',
                '--no-extensions','--no-skills','--no-prompt-templates','--no-themes'],cwd,env)
            pipe.send({'id':'identity','type':'get_state'})
        elif tool == 'agy':
            pipe = AgyAcpSession(exe, None if creating else raw, cwd, env)
            if creating:
                _stream_update(rid,native_session_id=pipe.raw)
            pipe.prompt(cfg)
            return
        else: raise ValueError('HARNESS_NATIVE_RESUME_UNSUPPORTED')
        _stream_update(rid, execution_status='running', pid=os.getpid())
        _stream_event(cfg,'running')
        startup_deadline = time.monotonic()+180
        while True:
            # A confirmed native turn is supervised until its terminal event.
            # An HTTP/startup timeout must not kill a healthy long-running turn.
            confirmed = identity and (tool != 'codex' or codex_stream.turn_id is not None)
            if not confirmed and time.monotonic() >= startup_deadline:
                raise ValueError('HARNESS_TIMEOUT')
            if claude_cancel_deadline and time.monotonic()>claude_cancel_deadline:raise ValueError('HARNESS_CANCEL_UNCONFIRMED')
            if not cancel_sent and identity and tool in ('codex','pi','claude') and _cancel_requested(rid):
                if tool=='pi':
                    pipe.send({'id':'bob-cancel','type':'abort'});cancel_sent=True
                elif tool=='claude':
                    pipe.send({'type':'control_request','request_id':'bob-cancel-'+rid,'request':{'subtype':'interrupt'}});cancel_sent=True
                elif codex_stream.turn_id:
                    pipe.send({'id':4,'method':'turn/interrupt','params':{'threadId':raw,'turnId':codex_stream.turn_id}});cancel_sent=True
                if cancel_sent: _stream_update(rid,cancel_dispatched_at=_now())
            if tool=='claude' and cfg.get('approval_mode')=='interactive':_flush_native_permissions(cfg,pipe)
            frame, already_answered = ready_frames.popleft() if ready_frames else (pipe.receive(timeout=0.5), False)
            if frame is None: continue
            _stream_update(rid,last_protocol_frame=str(frame.get('method') or frame.get('event') or frame.get('type') or frame.get('id'))[:100])
            delta = ''; snapshot = None
            if tool == 'codex':
                # Server requests have their own ID namespace. Never treat an
                # approval request numbered 2/3 as our resume/start response.
                if frame.get('method') and 'id' in frame:
                    if codex_stream.turn_id:
                        _capture_codex_event(sid,raw,frame,'worker:'+rid,rid,codex_stream.turn_id)
                    if not already_answered:
                        pipe.send({'id':frame['id'],'error':{'code':-32000,'message':'Bob read-only verification denies tool approval'}})
                    params=frame.get('params') or {}
                    if identity and codex_stream.turn_id is None and isinstance(params,dict) and params.get('threadId')==raw:
                        pending_bytes += len(json.dumps(frame).encode('utf-8'))
                        if len(pending_frames)>=1024 or pending_bytes>1048576:
                            raise ValueError('HARNESS_TRANSPORT_FAILED')
                        # Replay only the observation after exact turn binding,
                        # never the response to the server-initiated request.
                        pending_frames.append((frame,True))
                    continue
                if frame.get('id')==2:
                    _stream_update(rid,resume_response_received=True)
                    if 'error' in frame: raise ValueError('HARNESS_RESUME_REJECTED')
                    thread = frame.get('result',{}).get('thread',{})
                    if creating:
                        raw = validate_new_native_id(thread.get('id'))
                        codex_stream = CodexTurnStream(raw)
                        _stream_update(rid,native_session_id=raw)
                    elif thread.get('id') != raw:
                        raise ValueError('HARNESS_SESSION_MISMATCH')
                    identity=True
                    if _cancel_requested(rid):
                        _stream_update(rid,execution_status='cancelled',ended_at=_now(),cancel_confirmed_by='not_dispatched')
                        _stream_event(cfg,'cancelled');return
                    _stream_update(rid,native_session_id=raw,turn_start_requested=True)
                    pipe.send({'id':3,'method':'turn/start','params':{'threadId':raw,'input':[{'type':'text','text':text}],
                        'approvalPolicy':'never','sandboxPolicy':{'type':'readOnly'}}})
                if frame.get('id')==3:
                    _stream_update(rid,turn_start_response_received=True)
                    if 'error' in frame: raise ValueError('HARNESS_SEND_REJECTED')
                    if not identity: raise ValueError('HARNESS_IDENTITY_UNCONFIRMED')
                    codex_stream.bind(frame.get('result') or {})
                    _stream_update(rid,native_turn_id=codex_stream.turn_id)
                    ready_frames.extend(pending_frames); pending_frames.clear(); pending_bytes=0
                method=frame.get('method',''); p=frame.get('params') or {}
                if method and isinstance(p,dict) and p.get('threadId')==raw:
                    if codex_stream.turn_id is None:
                        if not identity:continue
                        pending_bytes += len(json.dumps(frame).encode('utf-8'))
                        if len(pending_frames)>=1024 or pending_bytes>1048576:
                            raise ValueError('HARNESS_TRANSPORT_FAILED')
                        pending_frames.append((frame,False))
                        continue
                    _capture_codex_event(sid,raw,frame,'worker:'+rid,rid,codex_stream.turn_id)
                if method in ('item/agentMessage/delta','item/completed','turn/completed'):
                    if p.get('threadId') != raw: continue
                    if method=='turn/completed' and p.get('threadId')==raw and (p.get('turn') or {}).get('id')==codex_stream.turn_id and (p.get('turn') or {}).get('status')=='interrupted':
                        cancelled=True;terminal=True
                    else: snapshot,terminal=codex_stream.fold(method,p)
                    if snapshot is not None and snapshot.startswith(reply):
                        delta=snapshot[len(reply):]
            elif tool == 'claude':
                if frame.get('type')=='control_request':
                    _handle_claude_permission(cfg if identity else dict(cfg,approval_mode='deny'),pipe,frame)
                    continue
                if frame.get('type')=='control_response':
                    response=frame.get('response') or {}
                    if cancel_sent and response.get('request_id')=='bob-cancel-'+rid:
                        _stream_event(cfg,'source_event',{'source':'claude-stream-json','frame':frame})
                        if response.get('subtype')!='success':raise ValueError('HARNESS_CANCEL_UNCONFIRMED')
                        claude_cancel_ack=True
                        if claude_cancel_terminal:cancelled=True;terminal=True;break
                    continue
                returned=frame.get('session_id') or frame.get('conversation_id')
                if returned:
                    if returned!=raw: raise ValueError('HARNESS_SESSION_MISMATCH')
                    identity=True
                # A dedicated pipe is not proof of the resumed conversation.
                # Do not publish content or terminal evidence before identity.
                if not identity: raise ValueError('HARNESS_IDENTITY_UNCONFIRMED')
                if frame.get('type') in ('stream_event','assistant','user','result') or isinstance(frame.get('event'),dict):
                    _stream_event(cfg,'source_event',{'source':'claude-stream-json','frame':frame})
                event=frame.get('event') or {}
                if event.get('type')=='content_block_delta' and event.get('delta',{}).get('type')=='text_delta': delta=event['delta'].get('text','')
                if frame.get('type')=='assistant':
                    content=frame.get('message',{}).get('content',[])
                    snapshot=''.join(b.get('text','') for b in content if b.get('type')=='text')
                if frame.get('type')=='result':
                    if cancel_sent and frame.get('terminal_reason')=='aborted_streaming':
                        claude_cancel_terminal=True
                        if claude_cancel_ack:cancelled=True;terminal=True;break
                        claude_cancel_deadline=time.monotonic()+10
                        continue
                    if frame.get('is_error') or frame.get('subtype') not in (None,'success'): raise ValueError('HARNESS_TURN_FAILED')
                    snapshot=frame.get('result') or snapshot;terminal=True
            else:
                if frame.get('id')=='identity':
                    state=frame.get('data') or {}
                    if frame.get('success') is False: raise ValueError('HARNESS_SESSION_MISMATCH')
                    if creating:
                        raw = validate_new_native_id(state.get('sessionId'))
                        session_file = state.get('sessionFile')
                        if not isinstance(session_file,str) or not Path(session_file).resolve().is_relative_to(Path(cfg['pi_session_dir']).resolve()):
                            raise ValueError('HARNESS_WORKSPACE_MISMATCH')
                        _stream_update(rid,native_session_id=raw)
                    elif state.get('sessionId') != raw:
                        raise ValueError('HARNESS_SESSION_MISMATCH')
                    identity=True
                    if _cancel_requested(rid):
                        _stream_update(rid,execution_status='cancelled',ended_at=_now(),cancel_confirmed_by='not_dispatched')
                        _stream_event(cfg,'cancelled');return
                    pipe.send({'id':'prompt','type':'prompt','message':text})
                if frame.get('id')=='prompt' and frame.get('success') is False: raise ValueError('HARNESS_SEND_REJECTED')
                event=frame.get('assistantMessageEvent') or {}
                if frame.get('type')=='message_update' and event.get('type')=='text_delta': delta=event.get('delta','')
                if frame.get('type')=='agent_settled': terminal=True
                if frame.get('type')=='message_end' and (frame.get('message') or {}).get('role')=='assistant':
                    msg=frame.get('message') or {}
                    if msg.get('stopReason')=='aborted': cancelled=True
                    if msg.get('stopReason')=='error': raise ValueError('HARNESS_TURN_FAILED')
                    if msg.get('stopReason')=='length': raise ValueError('HARNESS_OUTPUT_TRUNCATED')
                    snapshot=''.join(part.get('text','') for part in msg.get('content',[]) if part.get('type')=='text') or None
            if delta:
                reply+=delta
                if len(reply)>262144: raise ValueError('HARNESS_OUTPUT_LIMIT')
                _stream_event(cfg,'text_delta',{'text':delta})
                _stream_update(rid,reply_text=reply)
            elif snapshot!=reply and (snapshot or (tool=='codex' and snapshot is not None)):
                reply=snapshot[:262144]
                _stream_event(cfg,'message_snapshot',{'text':reply})
                _stream_update(rid,reply_text=reply)
            if terminal: break
        if not terminal: raise ValueError('HARNESS_TIMEOUT')
        if not identity: raise ValueError('HARNESS_IDENTITY_UNCONFIRMED')
        if not reply.strip() and not cancelled: raise ValueError('HARNESS_EMPTY_REPLY')
        state='cancelled' if cancelled else 'completed'
        _stream_event(cfg,state)
        _stream_update(rid,execution_status=state,ended_at=_now(),native_session_id=raw)
    except Exception as exc:
        code=str(exc) if isinstance(exc,ValueError) and str(exc).startswith('HARNESS_') else 'HARNESS_TRANSPORT_FAILED'
        if tool=='codex' and not identity and code=='HARNESS_TIMEOUT':
            _stream_event(cfg,'error',{'code':'HARNESS_NOT_ACCEPTED'})
            _stream_update(rid,execution_status='not_accepted',error_code=code,ended_at=_now())
        else:
            state='unknown' if code in ('HARNESS_TIMEOUT','HARNESS_PROCESS_EXITED','HARNESS_TRANSPORT_FAILED','HARNESS_IDENTITY_UNCONFIRMED','HARNESS_CANCEL_UNCONFIRMED') else 'error'
            _stream_event(cfg,state,{'code':code})
            _stream_update(rid,execution_status=state,error_code=code,ended_at=_now())
    finally:
        if pipe: pipe.close()


def _agy_tables(db):
    db.execute('CREATE TABLE IF NOT EXISTS agy_supervisors(session TEXT PRIMARY KEY,token TEXT NOT NULL,pid INTEGER NOT NULL,source TEXT NOT NULL)')
    db.execute('CREATE TABLE IF NOT EXISTS agy_queue(request TEXT PRIMARY KEY,cfg TEXT NOT NULL)')


def _agy_process_owned(pid, token, sid):
    try:
        if int(pid) <= 0 or not token: return False
        os.kill(int(pid), 0)
        argv = (Path('/proc')/str(pid)/'cmdline').read_bytes()
        return all(x.encode() in argv for x in ('agy_supervisor', token, sid))
    except (OSError, ValueError):
        return False


def _agy_start(cfg, source):
    sid, rid, text = cfg['session_id'], cfg['request_id'], cfg['text']
    digest = hashlib.sha256(text.encode('utf-8')).hexdigest()
    version = hashlib.sha256(source.encode('utf-8')).hexdigest()
    # Reconcile a lost supervisor before allowing a different request. Unknown
    # dispatches remain a session barrier; starting another worker is not recovery.
    _stream_records(sid)
    with closing(_stream_db()) as db, db:
        db.execute('BEGIN IMMEDIATE'); _agy_tables(db)
        old = db.execute('SELECT session,input_hash,body FROM requests WHERE id=?', (rid,)).fetchone()
        if old:
            if old[:2] != (sid, digest): raise ValueError('HARNESS_REQUEST_CONFLICT')
            state = json.loads(old[2]).get('execution_status', 'unknown')
            return {'status': state if state in ('unknown', 'not_accepted') else 'accepted',
                'execution_status': state, 'session_id': sid, 'request_id': rid}
        for row in db.execute('SELECT body FROM requests WHERE session=?', (sid,)):
            if json.loads(row[0]).get('execution_status') in ('accepted', 'running', 'unknown'):
                raise ValueError('HARNESS_SESSION_BUSY')
        supervisor = db.execute('SELECT token,pid,source FROM agy_supervisors WHERE session=?', (sid,)).fetchone()
        alive = supervisor and _agy_process_owned(supervisor[1], supervisor[0], sid)
        if alive and supervisor[2] != version:
            # Durable negative receipt: this request was never queued or sent.
            record = {'request_id': rid, 'session_id': sid, 'text': text,
                'execution_status': 'not_accepted', 'started_at': _now(), 'ended_at': _now(),
                'error_code': 'HARNESS_SUPERVISOR_VERSION_BUSY'}
            db.execute('INSERT INTO requests VALUES(?,?,?,?)', (rid,sid,digest,json.dumps(record,ensure_ascii=False)))
            return {'status': 'not_accepted', 'session_id': sid, 'request_id': rid,
                'reason': 'HARNESS_SUPERVISOR_VERSION_BUSY'}
        if alive:
            token, pid = supervisor[:2]
        else:
            token = uuid.uuid4().hex
            root = Path.home()/'.local/share/bob-native'
            sf = root/(version+'.py')
            if not sf.exists():
                sf.write_text(source, encoding='utf-8'); sf.chmod(0o600)
            command = {'mode': 'agy_supervisor', 'session_id': sid, 'supervisor_token': token}
            # Launch while holding the transaction. The worker waits for commit;
            # a failed launch rolls back both registry and queued request.
            proc = subprocess.Popen([sys.executable, str(sf), json.dumps(command)],
                start_new_session=True, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            pid = proc.pid
            db.execute('INSERT OR REPLACE INTO agy_supervisors VALUES(?,?,?,?)', (sid, token, pid, version))
        record = {'request_id': rid, 'session_id': sid, 'text': text, 'execution_status': 'accepted',
            'started_at': _now(), 'cancel_supported': True, 'pid': pid, 'supervisor_token': token}
        db.execute('INSERT INTO requests VALUES(?,?,?,?)', (rid, sid, digest, json.dumps(record, ensure_ascii=False)))
        db.execute('INSERT INTO agy_queue VALUES(?,?)', (rid, json.dumps(cfg, ensure_ascii=False)))
        _append_event(db, cfg, 'accepted', {'cancel_supported': True})
    return {'status': 'accepted', 'session_id': sid, 'request_id': rid}


def _agy_supervise(command):
    sid, token = command['session_id'], command['supervisor_token']
    session = None; active = None; last_work = time.monotonic(); cwd = None
    try:
        while True:
            with closing(_stream_db()) as db, db:
                db.execute('BEGIN IMMEDIATE'); _agy_tables(db)
                owner = db.execute('SELECT token FROM agy_supervisors WHERE session=?', (sid,)).fetchone()
                if not owner or owner[0] != token: return
                queued = db.execute('SELECT q.request,q.cfg,r.body FROM agy_queue q JOIN requests r ON r.id=q.request WHERE r.session=? ORDER BY q.rowid', (sid,)).fetchall()
                active = None
                for rid, raw_cfg, raw_body in queued:
                    body = json.loads(raw_body)
                    if body.get('execution_status') == 'accepted' and body.get('supervisor_token') == token:
                        active = json.loads(raw_cfg)
                        body.update(execution_status='running', pid=os.getpid())
                        db.execute('UPDATE requests SET body=? WHERE id=?', (json.dumps(body), rid))
                        break
                if active is None and time.monotonic()-last_work > 300:
                    # Close before relinquishing the registry, under the same
                    # transaction, so a new caller cannot lose its queued work.
                    if session: session.close(); session = None
                    db.execute('DELETE FROM agy_supervisors WHERE session=? AND token=?', (sid, token))
                    return
            if active is None:
                time.sleep(.25)
                continue
            if session and cwd != active['work_cwd']:
                raise ValueError('HARNESS_WORKSPACE_MISMATCH')
            if session is None:
                env = os.environ.copy()
                env['PATH'] = os.pathsep.join(str(Path.home()/p) for p in ['.local/bin','.npm-global/bin','.cargo/bin'])+os.pathsep+env.get('PATH','')
                exe = shutil.which('agy', path=env['PATH'])
                if not exe: raise ValueError('HARNESS_EXECUTABLE_MISSING')
                cwd = active['work_cwd']
                session = AgyAcpSession(exe, sid.split(':',1)[1], cwd, env)
            session.prompt(active)
            with closing(_stream_db()) as db, db:
                db.execute('DELETE FROM agy_queue WHERE request=?', (active['request_id'],))
            active = None; last_work = time.monotonic()
    except Exception as exc:
        if active:
            code = str(exc) if isinstance(exc, ValueError) and str(exc).startswith('HARNESS_') else 'HARNESS_TRANSPORT_FAILED'
            # Even protocol errors may follow side effects. Keep the barrier
            # unless the original prompt provided a terminal result.
            with closing(_stream_db()) as db, db:
                db.execute('BEGIN IMMEDIATE')
                row = db.execute('SELECT body FROM requests WHERE id=?', (active['request_id'],)).fetchone()
                body = json.loads(row[0]) if row else {}
                if body.get('execution_status') not in ('completed', 'cancelled', 'error'):
                    body.update(execution_status='unknown', error_code=code)
                    db.execute('UPDATE requests SET body=? WHERE id=?', (json.dumps(body), active['request_id']))
                    _append_event(db, active, 'unknown', {'code': code})
    finally:
        if session: session.close()
        with closing(_stream_db()) as db, db:
            _agy_tables(db)
            db.execute('DELETE FROM agy_supervisors WHERE session=? AND token=?', (sid, token))


def _native_start(cfg, source):
    if cfg['session_id'].startswith('agy:'): return _agy_start(cfg, source)
    sid,rid,text=cfg['session_id'],cfg['request_id'],cfg['text']
    digest=hashlib.sha256(text.encode('utf-8')).hexdigest()
    # Reconcile the original worker before answering a repeated delivery.
    _stream_records(sid)
    with closing(_stream_db()) as db, db:
        db.execute('BEGIN IMMEDIATE')
        old=db.execute('SELECT session,input_hash,body FROM requests WHERE id=?',(rid,)).fetchone()
        if old:
            if old[:2]!=(sid,digest): raise ValueError('HARNESS_REQUEST_CONFLICT')
            existing=json.loads(old[2])
            if cfg.get('new_harness') and (existing.get('setup_harness') != cfg['new_harness']
                    or existing.get('setup_workspace') != cfg['work_cwd']):
                raise ValueError('HARNESS_REQUEST_CONFLICT')
            state=existing.get('execution_status','unknown')
            return {'status':state if state in ('unknown','not_accepted') else 'accepted',
                    'execution_status':state,'session_id':sid,'request_id':rid}
        for row in db.execute('SELECT body FROM requests WHERE session=?',(sid,)):
            if json.loads(row[0]).get('execution_status') in ('accepted','running','unknown'):
                raise ValueError('HARNESS_SESSION_BUSY')
        record={'request_id':rid,'session_id':sid,'text':text,'execution_status':'accepted','started_at':_now(),'cancel_supported':sid.startswith(('codex:','pi:','agy:','claude-jsonl:'))}
        if cfg.get('new_harness'):
            record.update(setup_harness=cfg['new_harness'],setup_workspace=cfg['work_cwd'])
        if sid.startswith('claude-jsonl:'):
            args=_claude_tool_arguments(cfg)
            record['tool_authorization']={'version':1,'approval_mode':cfg.get('approval_mode','deny'),
                'tools':args[1].split(',') if args[1] else []}
        db.execute('INSERT INTO requests VALUES(?,?,?,?)',(rid,sid,digest,json.dumps(record,ensure_ascii=False)))
    root=Path.home()/'.local/share/bob-native'
    sf=root/(hashlib.sha256(source.encode()).hexdigest()+'.py')
    if not sf.exists(): sf.write_text(source,encoding='utf-8');sf.chmod(0o600)
    _stream_event(cfg,'accepted',{'cancel_supported':record['cancel_supported']})
    proc=subprocess.Popen([sys.executable,str(sf),json.dumps(dict(cfg,mode='native_worker'),ensure_ascii=False)],
        start_new_session=True,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    _stream_update(rid,pid=proc.pid)
    return {'status':'accepted','session_id':sid,'request_id':rid}



def setup_native_session(cfg):
    rid, tool, cwd = (cfg.get(key) for key in ('requestId','harness','workspace'))
    if not isinstance(rid, str) or not re.fullmatch(r'setup-[0-9a-f-]{36}', rid):
        raise ValueError('HARNESS_REQUEST_CONFLICT')
    prefixes = {'claude':'claude-jsonl', 'codex':'codex', 'agy':'agy', 'pi':'pi'}
    if tool not in prefixes or not isinstance(cwd,str) or not Path(cwd).is_absolute():
        raise ValueError('HARNESS_WORKSPACE_MISSING')
    sid = 'setup:' + rid.removeprefix('setup-')
    text = cfg.get('text')
    start = cfg.get('action') == 'start'
    with closing(_stream_db()) as db:
        row = db.execute('SELECT session,input_hash,body FROM requests WHERE id=?',(rid,)).fetchone()
    if row is None:
        if not start:
            return {'request_id':rid,'status':'missing'}
        if not isinstance(text,str) or not text.strip() or len(text.encode('utf-8')) > 32000 or not Path(cwd).is_dir():
            raise ValueError('HARNESS_SETUP_INVALID')
        source = globals().get('__BOB_PROBE_SRC__') or Path(__file__).read_text(encoding='utf-8')
        worker = {'session_id':sid,'request_id':rid,'text':text,'work_cwd':cwd,'project_cwd':cwd,
                  'new_harness':tool,'approval_mode':'deny','claude_tools':[],
                  'pi_session_dir':cfg.get('piDataDir') or ''}
        if tool == 'pi' and not worker['pi_session_dir']:
            raise ValueError('HARNESS_WORKSPACE_MISSING')
        # Scope is included in the persisted fingerprint before spawning.
        return _native_start(worker, source)
    if row[0] != sid:
        raise ValueError('HARNESS_REQUEST_CONFLICT')
    record = json.loads(row[2])
    if record.get('setup_harness') != tool or record.get('setup_workspace') != cwd:
        raise ValueError('HARNESS_REQUEST_CONFLICT')
    if start and (not isinstance(text,str) or row[1] != hashlib.sha256(text.encode('utf-8')).hexdigest()):
        raise ValueError('HARNESS_REQUEST_CONFLICT')
    records = _stream_records(sid)
    record = next((r for r in records if r.get('request_id') == rid), record)
    status = record.get('execution_status','unknown')
    response = {'request_id':rid,'status':status,'error':record.get('error_code'),
                'reply':str(record.get('reply_text') or '')[:16000]}
    if status != 'completed':
        return response
    raw = validate_new_native_id(record.get('native_session_id'))
    native = prefixes[tool] + ':' + raw
    page = read_session_page(cfg,native)
    messages = page.get('messages') or []
    if not any(m.get('role') == 'assistant' and m.get('text') for m in messages):
        return dict(response,status='persisting')
    if not page.get('workspace') or Path(page['workspace']).resolve() != Path(cwd).resolve():
        return dict(response,status='unknown',error='HARNESS_WORKSPACE_MISMATCH')
    return dict(response,status='ready',session_id=native)


def prompt_session(cfg):
    sid, text, rid = (cfg.get('sessionId') or '').strip(), (cfg.get('text') or '').strip(), (cfg.get('requestId') or '').strip()
    if not sid or not text or not rid:
        raise ValueError('DSH_REQUEST_REQUIRED')
    if not re.fullmatch(r'[a-z-]+:[a-zA-Z0-9-]{8,80}', sid):
        raise ValueError('HARNESS_SESSION_INVALID')
    d = _load_r()
    if rid in d and d[rid].get('session_id') == sid and (d[rid].get('execution_status') == 'running' or d[rid].get('reply_text')):
        return {'status': 'accepted', 'session_id': sid, 'request_id': rid}
    pcwd = wcwd = str(Path.home())
    jpath = ''; row = None; idx = None
    if sid.startswith('agy:'):
        raw, adir = sid.split(':', 1)[1], cfg.get('agyDataDir') or ''
        idx = (Path(adir) / 'conversation_summaries.db') if adir else None
        if idx and idx.is_file():
            try:
                with closing(sqlite3.connect(idx.as_uri() + '?mode=ro', uri=True, timeout=2)) as db:
                    row = db.execute('SELECT workspace_uris FROM conversation_summaries WHERE conversation_id=?', (raw,)).fetchone()
                    if row and row[0]:
                        uris = _j(row[0])
                        if isinstance(uris, list) and uris and isinstance(uris[0], str):
                            wcwd = pcwd = uris[0].removeprefix('file://')
            except Exception:
                pass
    elif sid.startswith('claude-jsonl:'):
        raw, cdir = sid.split(':', 1)[1], cfg.get('claudeDataDir') or ''
        m = list((Path(cdir) / 'projects').glob(f'*/{raw}.jsonl')) if cdir else []
        if m:
            jpath, slug = str(m[0]), m[0].parent.name
            for _, o in _iter_jsonl(m[0], 60):
                cw = o.get('cwd')
                if cw:
                    wcwd = cw
                    if cw.replace('/', '-') == slug:
                        pcwd = cw
    elif sid.startswith('pi:'):
        raw, pdir = sid.split(':', 1)[1], cfg.get('piDataDir') or ''
        m = [p for p in Path(pdir).glob('**/*.jsonl') if raw in p.name] if pdir else []
        if m:
            jpath = str(m[0])
            for _, o in _iter_jsonl(m[0], 30):
                if o.get('type') == 'session' and o.get('cwd'):
                    wcwd = pcwd = o['cwd']
                    break
    elif sid.startswith('codex:'):
        raw, xdir = sid.split(':', 1)[1], cfg.get('codexDataDir') or ''
        m = [p for p in (Path(xdir) / 'sessions').glob('**/*.jsonl') if raw in p.name] if xdir else []
        if m:
            jpath = str(m[0])
            for _, o in _iter_jsonl(m[0], 30):
                if o.get('type') == 'session_meta' and (o.get('payload') or {}).get('cwd'):
                    wcwd = pcwd = o['payload']['cwd']
                    break
    elif sid.startswith('cp:'):
        pts = sid.split(':', 2)
        if len(pts) == 3:
            _, _, tid = pts
            for dbp in cfg.get('controlPlaneDbs') or []:
                p = Path(dbp)
                if p.is_file():
                    try:
                        with closing(sqlite3.connect(p.as_uri() + '?mode=ro', uri=True, timeout=2)) as conn:
                            wr = conn.execute('SELECT w.path FROM executions e JOIN workspaces w ON w.id=e.workspace_id WHERE e.task_id=? LIMIT 1', (tid,)).fetchone()
                            if wr and wr[0]:
                                wcwd = pcwd = wr[0]
                                break
                    except Exception:
                        pass
    if sid.startswith('cp:'):
        raise ValueError('HARNESS_NATIVE_BINDING_REQUIRED')
    if not re.fullmatch(r'[a-z-]+:[a-zA-Z0-9-]{8,80}', sid):
        raise ValueError('HARNESS_SESSION_INVALID')
    if not jpath and not sid.startswith('agy:'):
        raise ValueError('HARNESS_SESSION_NOT_FOUND')
    if sid.startswith('agy:') and not (idx and idx.is_file() and row):
        raise ValueError('HARNESS_SESSION_NOT_FOUND')
    if not Path(wcwd).is_dir():raise ValueError('HARNESS_WORKSPACE_MISSING')
    approval_mode = 'interactive' if cfg.get('approvalMode') == 'interactive' else 'deny'
    claude_tools=cfg.get('claudeTools',[]) if sid.startswith('claude-jsonl:') else []
    takeover=bool(cfg.get('takeover'))
    _claude_tool_arguments({'approval_mode':approval_mode,'claude_tools':claude_tools})
    if sid.startswith('claude-jsonl:'):
        raw=sid.split(':',1)[1]
        env=os.environ.copy()
        env['PATH']=os.pathsep.join(str(Path.home()/p) for p in ['.local/bin','.npm-global/bin','.cargo/bin'])+os.pathsep+env.get('PATH','')
        exe=shutil.which('claude',path=env['PATH'])
        if exe:
            try:
                holder=_release_claude_terminal_holder(exe,raw,env) if takeover else _inspect_claude_session_holder(exe,raw,env)
                if holder.get('busy'):
                    reason='HARNESS_CLAUDE_REMOTE_CONTROL' if holder.get('remote_control') else 'HARNESS_SESSION_BUSY'
                    return {'status':'not_accepted','execution_status':'not_accepted','session_id':sid,'request_id':rid,'reason':reason,'terminal_holder':holder}
            except ValueError as exc:
                if str(exc)=='HARNESS_CLAUDE_REMOTE_CONTROL':
                    holder=_safe_claude_holder(sid) or {'busy':True,'remote_control':True}
                    return {'status':'not_accepted','execution_status':'not_accepted','session_id':sid,'request_id':rid,'reason':'HARNESS_CLAUDE_REMOTE_CONTROL','terminal_holder':holder}
    attachments = cfg.get('attachments') or []
    if isinstance(attachments, list) and attachments:
        att_dir = Path(wcwd) / '.bob_attachments'
        try:
            att_dir.mkdir(parents=True, exist_ok=True)
            for item in attachments[:8]:
                raw_name = (item.get('name') or '').strip()
                b64 = (item.get('base64') or '').strip()
                if not raw_name or not b64:
                    continue
                try:
                    raw_bytes = base64.b64decode(b64)
                except Exception:
                    continue
                if not raw_bytes or len(raw_bytes) > 10 * 1024 * 1024:
                    continue
                base_name = os.path.basename(raw_name.replace('\\', '/'))
                safe_name = ''.join(c if (c.isalnum() or c in '.-_') else '_' for c in base_name).strip('.') or 'attachment.bin'
                target_path = att_dir / safe_name
                try:
                    target_path.write_bytes(raw_bytes)
                    target_path.chmod(0o644)
                except Exception:
                    pass
                target_str = str(target_path).replace('\\', '/')
                prefix = f"[Attachment: {raw_name}"
                new_marker = f"[Attachment: {raw_name} -> {target_str}]"
                if prefix in text:
                    s_idx = text.find(prefix)
                    e_idx = text.find(']', s_idx)
                    if e_idx != -1:
                        text = text[:s_idx] + new_marker + text[e_idx + 1:]
                    else:
                        text = f"{text}\n\n{new_marker}"
                else:
                    text = f"{text}\n\n{new_marker}"
        except Exception:
            pass
    cfg = {'session_id':sid,'request_id':rid,'text':text,'project_cwd':pcwd,'work_cwd':wcwd,'jsonl_path':jpath}
    cfg['approval_mode'] = approval_mode
    cfg['claude_tools'] = claude_tools
    if takeover:
        cfg['takeover'] = True
    source = globals().get('__BOB_PROBE_SRC__') or Path(__file__).read_text(encoding='utf-8')
    return _native_start(cfg, source)

# Official external Codex observation. No terminal scraping and no new turn.
import socket
import struct
import base64
import stat


class CodexControlSocket:
    def __init__(self, path):
        st=os.stat(path)
        if st.st_uid!=os.getuid() or not stat.S_ISSOCK(st.st_mode): raise ValueError('HARNESS_CONTROL_NOT_OWNED')
        self.s=socket.socket(socket.AF_UNIX);self.s.settimeout(5);self.s.connect(path)
        self.buf=b'';self.fragment=b''
        key=base64.b64encode(os.urandom(16)).decode()
        self.s.sendall(('GET / HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: '+key+'\r\nSec-WebSocket-Version: 13\r\n\r\n').encode())
        try:
            while b'\r\n\r\n' not in self.buf:
                part=self.s.recv(4096)
                if not part or len(self.buf)>16384: raise ValueError('HARNESS_CONTROL_HANDSHAKE')
                self.buf+=part
            header,self.buf=self.buf.split(b'\r\n\r\n',1)
            expected=base64.b64encode(hashlib.sha1((key+'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').encode()).digest())
            if not header.startswith(b'HTTP/1.1 101 ') or expected not in header: raise ValueError('HARNESS_CONTROL_HANDSHAKE')
        except Exception:
            self.s.close();raise

    def _send(self, payload, opcode=1):
        mask=os.urandom(4);n=len(payload)
        prefix=bytes([0x80|opcode,0x80|n]) if n<126 else bytes([0x80|opcode,0xfe])+struct.pack('!H',n) if n<65536 else bytes([0x80|opcode,0xff])+struct.pack('!Q',n)
        self.s.sendall(prefix+mask+bytes(x^mask[i%4] for i,x in enumerate(payload)))

    def send(self, frame): self._send(json.dumps(frame).encode('utf-8'))

    def receive(self, timeout=.5):
        deadline=time.monotonic()+timeout
        while time.monotonic()<deadline:
            if len(self.buf)>=2:
                a,b=self.buf[:2];n=b&127;offset=2
                if b&128 or a&0x70: raise ValueError('HARNESS_CONTROL_FRAME')
                if n==126 and len(self.buf)>=4:n=struct.unpack('!H',self.buf[2:4])[0];offset=4
                elif n==127 and len(self.buf)>=10:n=struct.unpack('!Q',self.buf[2:10])[0];offset=10
                elif n>=126:n=None
                if n is not None:
                    if n>8*1024*1024: raise ValueError('HARNESS_OUTPUT_LIMIT')
                    if len(self.buf)>=offset+n:
                        payload,self.buf=self.buf[offset:offset+n],self.buf[offset+n:];op=a&15
                        if op==8: raise ValueError('HARNESS_PROCESS_EXITED')
                        if op==9:self._send(payload,10);continue
                        if op==10:continue
                        if op not in (0,1):raise ValueError('HARNESS_CONTROL_FRAME')
                        self.fragment+=payload
                        if len(self.fragment)>8*1024*1024:raise ValueError('HARNESS_OUTPUT_LIMIT')
                        if not a&128:continue
                        data,self.fragment=self.fragment,b''
                        return json.loads(data)
            self.s.settimeout(max(.01,deadline-time.monotonic()))
            try:part=self.s.recv(65536)
            except socket.timeout:return None
            if not part:raise ValueError('HARNESS_PROCESS_EXITED')
            self.buf+=part
        return None

    def close(self):self.s.close()


def _codex_socket_paths():
    if not hasattr(socket,'AF_UNIX') or not Path('/proc/net/unix').is_file():return []
    inodes=set()
    for proc in Path('/proc').iterdir():
        if not proc.name.isdigit():continue
        try:
            if proc.stat().st_uid!=os.getuid():continue
            argv=(proc/'cmdline').read_bytes().decode('utf-8').split('\0')
            if not argv or Path(argv[0]).name!='codex' or 'app-server' not in argv or '--listen' not in argv:continue
            if not argv[argv.index('--listen')+1].startswith('unix://'):continue
            for fd in (proc/'fd').iterdir():
                try:
                    target=os.readlink(fd)
                    if target.startswith('socket:['):inodes.add(target[8:-1])
                except OSError:pass
        except (OSError,ValueError,IndexError,UnicodeError):continue
    paths=[]
    for line in Path('/proc/net/unix').read_text(encoding='utf-8').splitlines()[1:]:
        parts=line.split()
        if len(parts)>7 and parts[6] in inodes and parts[7].startswith('/') and parts[7] not in paths:paths.append(parts[7])
    return paths[:8]


def _control_result(pipe, request_id, on_event=None):
    deadline=time.monotonic()+8
    while time.monotonic()<deadline:
        frame=pipe.receive(.5)
        # Server-initiated approvals use their own request IDs; an ID match
        # alone is not a response to our read request. Never approve implicitly.
        if frame and 'method' not in frame and frame.get('id')==request_id and ('result' in frame or 'error' in frame):
            if 'error' in frame:raise ValueError('HARNESS_CONTROL_REJECTED')
            return frame.get('result') or {}
        if frame and frame.get('method') and on_event is not None:
            on_event(frame)
    raise ValueError('HARNESS_CONTROL_TIMEOUT')


def _open_loaded_codex(raw, on_event=None):
    for path in _codex_socket_paths():
        pipe=None
        try:
            pipe=CodexControlSocket(path)
            response=lambda request_id: _control_result(pipe,request_id,on_event) if on_event is not None else _control_result(pipe,request_id)
            pipe.send({'id':1,'method':'initialize','params':{'clientInfo':{'name':'bob-external-observer','version':'1.0.1'},'capabilities':{'experimentalApi':True}}})
            response(1);pipe.send({'method':'initialized','params':{}})
            pipe.send({'id':2,'method':'thread/loaded/list','params':{'limit':100}})
            loaded=response(2)
            # Never resume a disk-only thread into a second runtime.
            if raw not in loaded.get('data',[]):pipe.close();continue
            # Attach without downloading the entire long-running thread again.
            # Older servers that cannot provide bounded reads fail honestly.
            pipe.send({'id':3,'method':'thread/resume','params':{'threadId':raw,'excludeTurns':True}})
            result=response(3);thread=result.get('thread') or {}
            if thread.get('id')!=raw:raise ValueError('HARNESS_IDENTITY_UNCONFIRMED')
            pipe.send({'id':4,'method':'thread/turns/list','params':{'threadId':raw,'limit':1,'sortDirection':'desc','itemsView':'full'}})
            latest=response(4)
            thread['turns']=list(reversed(latest.get('data') or []))
            return pipe,thread
        except Exception:
            if pipe:pipe.close()
    return None,None


def _observer_table(db):
    db.execute('CREATE TABLE IF NOT EXISTS observers(session TEXT PRIMARY KEY,pid INTEGER,touched INTEGER,status TEXT)')


def touch_external_observer(sid):
    if not sid.startswith('codex:'):return 'history_only'
    source=globals().get('__BOB_PROBE_SRC__') or Path(__file__).read_text(encoding='utf-8')
    with closing(_stream_db()) as db,db:
        _observer_table(db);db.execute('BEGIN IMMEDIATE')
        row=db.execute('SELECT pid,touched,status FROM observers WHERE session=?',(sid,)).fetchone()
        alive=False
        if row and row[0]:
            try:
                cmd=(Path('/proc')/str(row[0])/'cmdline').read_bytes()
                alive=sid.encode() in cmd and (hashlib.sha256(source.encode()).hexdigest()+'.py').encode() in cmd
            except OSError:pass
        if alive:
            db.execute('UPDATE observers SET touched=? WHERE session=?',(_now(),sid));return row[2]
        if row and row[2]=='unavailable' and _now()-row[1]<30000:return 'unavailable'
        if db.execute("SELECT COUNT(*) FROM observers WHERE touched>? AND status IN ('live','connecting')",(_now()-60000,)).fetchone()[0]>=8:return 'capacity'
        root=Path.home()/'.local/share/bob-native';sf=root/(hashlib.sha256(source.encode()).hexdigest()+'.py')
        if not sf.exists():sf.write_text(source,encoding='utf-8');sf.chmod(0o600)
        p=subprocess.Popen([sys.executable,str(sf),json.dumps({'mode':'external_observer','sessionId':sid})],start_new_session=True,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        db.execute('INSERT OR REPLACE INTO observers VALUES(?,?,?,?)',(sid,p.pid,_now(),'connecting'))
    return 'connecting'


def _older_external_turn_statuses(pipe,sid,raw,current,on_event=None):
    prefix='external-codex-'+raw+'-';current_ids={t.get('id') for t in current}
    with closing(_stream_db()) as db:
        saved=[json.loads(r[0]) for r in db.execute('SELECT body FROM event_snapshots WHERE session=?',(sid,))]
    wanted={r['request_id'][len(prefix):] for r in saved if r['request_id'].startswith(prefix) and (r.get('status') in ('accepted','running','unknown') or r.get('recovery_pending') is True)} - current_ids
    if not wanted:return current
    cursor=None;seen=set();found=[]
    for index in range(3):
        params={'threadId':raw,'limit':20,'sortDirection':'desc','itemsView':'summary'}
        if cursor:params['cursor']=cursor
        request_id=100+index
        try:
            pipe.send({'id':request_id,'method':'thread/turns/list','params':params})
            result=_control_result(pipe,request_id,on_event)
        except (ValueError,OSError):break
        data=result.get('data')
        if not isinstance(data,list) or len(data)>20:break
        matches={turn['id']:turn for turn in data if isinstance(turn,dict) and turn.get('id') in wanted}
        full={}
        if matches:
            # Fetch only pages containing an unresolved execution, never replay a
            # prompt. Full items restore missed tool/artifact observations.
            try:
                full_params=dict(params,itemsView='full')
                pipe.send({'id':200+index,'method':'thread/turns/list','params':full_params})
                detail=_control_result(pipe,200+index,on_event)
                detail_rows=detail.get('data')
                if isinstance(detail_rows,list) and len(detail_rows)<=20:
                    for turn in detail_rows:
                        if isinstance(turn,dict) and turn.get('id') in matches and isinstance(turn.get('items'),list):
                            full[turn['id']]=turn
            except (ValueError,OSError):pass
        for tid,turn in matches.items():
            found.append(full.get(tid) or {'id':tid,'status':turn.get('status'),'_status_only':True})
            wanted.remove(tid)
        if not wanted:break
        cursor=result.get('nextCursor')
        if not isinstance(cursor,str) or not cursor or len(cursor)>4096 or cursor in seen:break
        seen.add(cursor)
    # Latest full turn stays last. Summary-only recovery must not erase saved text.
    return list(reversed(found))+current


def _restore_external_checkpoints(sid, raw, turns):
    prefix='external-codex-'+raw+'-'
    known={t.get('id'):t for t in turns}
    latest=(turns[-1].get('id') if turns else None)
    with closing(_stream_db()) as db,db:
        db.execute('BEGIN IMMEDIATE');_journal_sync(db)
        saved=[json.loads(row[0]) for row in db.execute('SELECT body FROM event_snapshots WHERE session=?',(sid,))]
        for checkpoint in saved:
            rid=checkpoint['request_id']
            if not rid.startswith(prefix) or (checkpoint.get('status') not in ('accepted','running','unknown') and checkpoint.get('recovery_pending') is not True):continue
            tid=rid[len(prefix):];turn=known.get(tid);cfg={'session_id':sid,'request_id':rid}
            if tid==latest and turn and turn.get('status')=='inProgress':continue
            terminal={'completed':'completed','interrupted':'cancelled','failed':'error'}.get((turn or {}).get('status'))
            if terminal:
                if not turn.get('_status_only'):
                    text='\n\n'.join(i.get('text','') for i in turn.get('items') or [] if i.get('type')=='agentMessage')
                    _append_event(db,cfg,'message_snapshot',{'text':text})
                    activity=_activity_from_codex_turn(checkpoint.get('activity'),turn)
                    if activity is not None:_append_event(db,cfg,'activity_snapshot',{'activity':activity})
                pending=turn.get('_status_only') is True
                if checkpoint.get('recovery_pending') is not pending:_append_event(db,cfg,'recovery_state',{'pending':pending})
                if checkpoint.get('status')!=terminal:_append_event(db,cfg,terminal)
            elif checkpoint.get('status') not in ('unknown','completed','cancelled','error'):
                _append_event(db,cfg,'unknown',{'code':'HARNESS_EXTERNAL_TURN_UNCONFIRMED'})


def _project_agy_activity(previous, frame, decision=None):
    params = frame.get('params') or {}
    if frame.get('method') == 'session/request_permission':
        identity = frame.get('id')
        if isinstance(identity, bool) or not isinstance(identity, (str, int)) or len(str(identity)) > 128: return None
        activity = json.loads(json.dumps(previous or {'items': [], 'approvals': [], 'partial': False}))
        entries = activity.setdefault('approvals', [])
        key = json.dumps(identity)
        old = next((x for x in entries if x['id'] == key), None)
        if old and old['status'] == 'denied' and decision is None: return None
        tool_call = params.get('toolCall') if isinstance(params.get('toolCall'), dict) else {}
        item = {'id': key, 'item_id': str(tool_call.get('toolCallId') or '')[:128],
            'status': 'denied' if decision == 'denied' else 'pending', 'method': 'session/request_permission'}
        entries[:] = [x for x in entries if x['id'] != key]
        entries.append(item)
        if len(entries) > 32:
            del entries[:-32]
            activity['partial'] = True
        return activity
    update = params.get('update') or {}
    if frame.get('method') != 'session/update' or not isinstance(update, dict): return None
    if update.get('sessionUpdate') not in ('tool_call', 'tool_call_update'): return None
    identity = update.get('toolCallId')
    if not isinstance(identity, str) or not identity or len(identity) > 128: return None
    activity = json.loads(json.dumps(previous or {'items': [], 'approvals': [], 'partial': False}))
    entries = activity.setdefault('items', [])
    old = next((x for x in entries if x['id'] == identity), None)
    item = dict(old or {'id': identity, 'type': 'tool', 'label': identity, 'status': 'unconfirmed'})
    # ACP updates are patches: omitted title/status must retain prior evidence.
    if isinstance(update.get('title'), str): item['label'] = update['title'][:512]
    if isinstance(update.get('kind'), str): item['type'] = update['kind'][:64]
    state = {'pending': 'running', 'in_progress': 'running', 'completed': 'completed', 'failed': 'error'}.get(update.get('status'))
    if state and not (old and old['status'] in ('completed', 'error') and state == 'running'):
        item['status'] = state
    entries[:] = [x for x in entries if x['id'] != identity]
    entries.append(item)
    if len(entries) > 64:
        del entries[:-64]
        activity['partial'] = True
    return activity


def _capture_agy_event(cfg, frame, request_serial, decision=None):
    if (frame.get('params') or {}).get('sessionId') != cfg['session_id'].split(':', 1)[1]: return False
    payload = {'source': 'agy-acp', 'method': frame['method'], 'params': frame['params'], 'acp_request_id': request_serial}
    if 'id' in frame: payload['source_request_id'] = frame['id']
    with closing(_stream_db()) as db, db:
        db.execute('BEGIN IMMEDIATE')
        old = db.execute('SELECT body FROM event_snapshots WHERE request=? AND session=?', (cfg['request_id'], cfg['session_id'])).fetchone()
        state = json.loads(old[0]) if old else {}
        activity = _project_agy_activity(state.get('activity'), frame, decision)
        if decision is not None: payload['local_decision'] = decision
        _append_event(db, cfg, 'permission_decision' if decision is not None else 'source_event', payload)
        if activity is not None and state.get('status') not in ('completed', 'cancelled', 'error'):
            _append_event(db, cfg, 'activity_snapshot', {'activity': activity})
    return True


def _project_source_activity(previous, payload):
    activity=json.loads(json.dumps(previous or {'items':[],'approvals':[],'partial':False}))
    method=payload.get('method','');params=payload.get('params') or {}
    item=params.get('item') or {}
    if method in ('item/started','item/completed') and isinstance(item,dict):
        identity=item.get('id');kind=item.get('type')
        if not isinstance(identity,str) or not identity or kind in ('agentMessage','reasoning','userMessage'):return None
        if not isinstance(kind,str):return None
        state='running' if method=='item/started' else ('error' if item.get('status') in ('failed','declined') or item.get('exitCode') not in (None,0) else 'completed')
        label=str(item.get('command') or item.get('name') or kind)[:512]
        entries=activity.setdefault('items',[]);old=next((x for x in entries if x['id']==identity),None)
        if old and old['status'] in ('completed','error') and state=='running':return None
        entries[:]=[x for x in entries if x['id']!=identity]
        entries.append({'id':identity[:128],'type':kind[:64],'label':label,'status':state})
        if len(entries)>64:del entries[:-64];activity['partial']=True
    elif method.endswith('/requestApproval') and 'source_request_id' in payload:
        identity=payload['source_request_id']
        if not isinstance(identity,(str,int)) or isinstance(identity,bool):return None
        entries=activity.setdefault('approvals',[])
        entries[:]=[x for x in entries if x['id']!=str(identity)]
        entries.append({'id':str(identity)[:128],'item_id':str(params.get('itemId') or '')[:128],'status':'pending','method':method[:256],'request_key':json.dumps(identity),'observer_id':payload.get('observer_id')})
        if len(entries)>32:del entries[:-32];activity['partial']=True
    else:return None
    return activity


def _resolve_external_approval(sid, raw, frame, observer_id):
    params=frame.get('params') or {};identity=params.get('requestId')
    if not isinstance(identity,(str,int)) or isinstance(identity,bool):return False
    request_key=json.dumps(identity)
    with closing(_stream_db()) as db,db:
        db.execute('BEGIN IMMEDIATE')
        matches=[]
        for rid,body in db.execute('SELECT request,body FROM event_snapshots WHERE session=?',(sid,)):
            state=json.loads(body)
            for approval in state.get('activity',{}).get('approvals',[]):
                if approval.get('request_key')==request_key and approval.get('observer_id')==observer_id and approval.get('status') in ('pending','unconfirmed'):
                    matches.append((rid,state,approval))
        # Never attribute a reply to the newest turn by guesswork or across sockets.
        if len(matches)!=1:return False
        rid,state,approval=matches[0];approval['status']='cleared'
        request={'session_id':sid,'request_id':rid}
        _append_event(db,request,'source_event',{'method':'serverRequest/resolved','params':params,'source':'codex-app-server','observer_id':observer_id})
        _append_event(db,request,'activity_snapshot',{'activity':state['activity']})
    return True


def _capture_external_codex_event(sid, raw, frame, observer_id=None):
    return _capture_codex_event(sid,raw,frame,observer_id)


def _capture_codex_event(sid, raw, frame, observer_id=None, request_id=None, expected_turn=None):
    """Persist source events without replaying snapshot-overlapping text deltas."""
    params=frame.get('params') or {}
    if not isinstance(params,dict) or params.get('threadId')!=raw:return False
    if frame.get('method')=='serverRequest/resolved':return _resolve_external_approval(sid,raw,frame,observer_id)
    turn_id=params.get('turnId') or (params.get('turn') or {}).get('id')
    if not isinstance(turn_id,str) or not turn_id or len(turn_id)>128:return False
    if request_id is not None and (not expected_turn or turn_id!=expected_turn):return False
    method=frame.get('method')
    if not isinstance(method,str) or len(method)>256:return False
    # Source payloads are untrusted and bounded like the event journal itself.
    if len(json.dumps(frame,ensure_ascii=False).encode('utf-8'))>262144:
        raise ValueError('HARNESS_OUTPUT_LIMIT')
    request={'session_id':sid,'request_id':request_id or 'external-codex-'+raw+'-'+turn_id}
    payload={'method':method,'params':params,'source':'codex-app-server','observer_id':observer_id}
    if 'id' in frame:payload['source_request_id']=frame['id']
    with closing(_stream_db()) as db,db:
        db.execute('BEGIN IMMEDIATE')
        old=db.execute('SELECT body FROM event_snapshots WHERE request=? AND session=?',(request['request_id'],sid)).fetchone()
        state=json.loads(old[0]) if old else {}
        activity=_project_source_activity(state.get('activity'),payload)
        _append_event(db,request,'source_event',payload)
        if activity is not None and state.get('status') not in ('completed','cancelled','error'):
            _append_event(db,request,'activity_snapshot',{'activity':activity})
    return True


def _activity_from_codex_turn(previous, turn):
    """Recover source-confirmed activity while retaining unresolved approvals."""
    activity=previous;changed=False
    for item in turn.get('items') or []:
        if not isinstance(item,dict):continue
        status=item.get('status')
        if status in ('completed','failed','declined'):method='item/completed'
        elif status=='inProgress':method='item/started'
        elif status is None and turn.get('status')=='completed':method='item/completed'
        else:continue
        projected=_project_source_activity(activity,{'method':method,'params':{'item':item}})
        if projected is not None:activity=projected;changed=True
    return activity if changed else None


def _seed_codex_activity(sid, raw, turn):
    turn_id=turn.get('id')
    if not isinstance(turn_id,str) or not turn_id:return
    request={'session_id':sid,'request_id':'external-codex-'+raw+'-'+turn_id}
    with closing(_stream_db()) as db,db:
        db.execute('BEGIN IMMEDIATE')
        old=db.execute('SELECT body FROM event_snapshots WHERE request=? AND session=?',(request['request_id'],sid)).fetchone()
        state=json.loads(old[0]) if old else {}
        activity=_activity_from_codex_turn(state.get('activity'),turn)
        if activity is not None:_append_event(db,request,'activity_snapshot',{'activity':activity})


def observe_external_codex(cfg):
    sid=cfg['sessionId'];raw=sid.split(':',1)[1];pipe=None;fold=None;turn_id=None;last_text=None
    observer_id=os.urandom(16).hex()
    def status(value):
        with closing(_stream_db()) as db,db:
            _observer_table(db);db.execute('UPDATE observers SET status=? WHERE session=? AND pid=?',(value,sid,os.getpid()))
    def emit(kind,payload=None):
        if turn_id:_stream_event({'session_id':sid,'request_id':'external-codex-'+raw+'-'+turn_id},kind,payload)
    def safe_capture(frame):
        try:_capture_external_codex_event(sid,raw,frame,observer_id)
        except ValueError as exc:
            if str(exc)!='HARNESS_OUTPUT_LIMIT':raise
            fp=frame.get('params') if isinstance(frame.get('params'),dict) else {}
            it=fp.get('item') if isinstance(fp.get('item'),dict) else None
            if it is not None:
                slim={'method':frame.get('method'),'params':{'threadId':fp.get('threadId'),'turnId':fp.get('turnId') or (fp.get('turn') or {}).get('id'),'item':{'id':it.get('id'),'type':it.get('type'),'status':it.get('status'),'exitCode':it.get('exitCode'),'command':str(it.get('command') or '')[:512],'name':str(it.get('name') or '')[:512]}}}
                if 'id' in frame:slim['id']=frame['id']
                try:_capture_external_codex_event(sid,raw,slim,observer_id)
                except ValueError:pass
    try:
        pipe,thread=_open_loaded_codex(raw,safe_capture)
        if not pipe:
            _restore_external_checkpoints(sid,raw,[])
            status('unavailable');return
        recovery_turns=_older_external_turn_statuses(pipe,sid,raw,thread.get('turns') or [],safe_capture)
        _restore_external_checkpoints(sid,raw,recovery_turns)
        status('live')
        for turn in (thread.get('turns') or [])[-1:]:
            turn_id=turn.get('id');fold=CodexTurnStream(raw);fold.bind({'turn':turn})
            for item in turn.get('items') or []:
                if item.get('type')=='agentMessage':
                    fold.items[item['id']]=item.get('text','')
                    if turn.get('status')!='inProgress':fold.completed_items.add(item['id'])
            last_text='\n\n'.join(fold.items.values())
            if len(last_text)>262144:raise ValueError('HARNESS_OUTPUT_LIMIT')
            emit('message_snapshot',{'text':last_text})
            _seed_codex_activity(sid,raw,turn)
            emit('running' if turn.get('status')=='inProgress' else {'completed':'completed','interrupted':'cancelled','failed':'error'}.get(turn.get('status'),'unknown'))
        while True:
            with closing(_stream_db()) as db:
                row=db.execute('SELECT touched FROM observers WHERE session=? AND pid=?',(sid,os.getpid())).fetchone()
            if not row:break
            if _now()-row[0]>60000:
                emit('unknown',{'code':'HARNESS_EXTERNAL_OBSERVER_IDLE'});break
            f=pipe.receive(.5)
            if not f:continue
            method=f.get('method');p=f.get('params') or {}
            safe_capture(f)
            if 'id' in f and method:
                # An observer has no authority to decide approvals or tool calls.
                continue
            if p.get('threadId')!=raw:continue
            if method=='turn/started':
                turn=p.get('turn') or {};turn_id=turn.get('id');fold=CodexTurnStream(raw);fold.bind({'turn':turn});last_text='';emit('running')
            elif fold and method=='turn/completed' and (p.get('turn') or {}).get('id')==turn_id and (p.get('turn') or {}).get('status')!='completed':
                emit('cancelled' if p['turn'].get('status')=='interrupted' else 'error')
            elif fold:
                text,terminal=fold.fold(method,p)
                if text is not None and text!=last_text:
                    if last_text is not None and text.startswith(last_text):emit('text_delta',{'text':text[len(last_text):]})
                    else:emit('message_snapshot',{'text':text})
                    last_text=text
                if terminal:emit('completed')
    except Exception:
        emit('unknown',{'code':'HARNESS_EXTERNAL_STREAM_LOST'});status('reconnecting')
    finally:
        if pipe:pipe.close()


if __name__ == '__main__':
    try:
        a1 = sys.argv[1] if len(sys.argv) > 1 else ''
        if a1.startswith('{'):
            cfg = json.loads(a1)
            m = cfg.get('mode', 'list')
            if m == 'page':
                print(json.dumps(read_session_page(cfg, cfg.get('sessionId', '')), ensure_ascii=False))
            elif m == 'setup':
                try: print(json.dumps(setup_native_session(cfg),ensure_ascii=False))
                except ValueError as exc:
                    code=str(exc)
                    print(json.dumps({'error':code if re.fullmatch(r'HARNESS_[A-Z_]+',code) else 'HARNESS_SETUP_UNCONFIRMED'}))
            elif m == 'prompt':
                print(json.dumps(prompt_session(cfg), ensure_ascii=False))
            elif m == 'preview':
                try:print(json.dumps(preview_native_file(cfg), ensure_ascii=False))
                except (ValueError,OSError) as exc:
                    code=str(exc) if isinstance(exc,ValueError) and re.fullmatch(r'DSH_[A-Z_]+',str(exc)) else 'DSH_PREVIEW_UNAVAILABLE'
                    print(json.dumps({'error':code}))
            elif m == 'receipt':
                print(json.dumps(inspect_receipt(cfg), ensure_ascii=False))
            elif m == 'agy_supervisor':
                _agy_supervise(cfg)
            elif m == 'native_worker':
                _native_worker(cfg)
            elif m == 'external_observer':
                observe_external_codex(cfg)
            elif m == 'cancel':
                print(json.dumps(request_native_cancel(cfg), ensure_ascii=False))
            elif m == 'permission':
                try: print(json.dumps(submit_native_permission(cfg), ensure_ascii=False))
                except ValueError as exc: print(json.dumps({'error': str(exc)}))
            elif m == 'events':
                print(json.dumps(read_native_events(cfg), ensure_ascii=False))
            else:
                print(json.dumps(discover_multi(cfg), ensure_ascii=False))
        else:
            print(json.dumps(discover(a1, int(sys.argv[2]) if len(sys.argv[2:]) else 100), ensure_ascii=False))
    except (ValueError, OSError, sqlite3.Error, IndexError):
        print(json.dumps({'error': 'AGY_INDEX_READ_FAILED'}))
        sys.exit(1)
