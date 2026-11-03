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

def _mk_row(rid, sid, h, lbl, kind, title, ws, ts, steps=1, st='completed', res=False):
    return {
        'id': rid, 'nativeSessionId': sid, 'harness': h, 'toolLabel': lbl,
        'sourceKind': kind, 'title': title, 'workspaceUris': [ws], 'updatedAt': ts,
        'stepCount': max(steps, 1), 'nativeStatus': st or 'completed',
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
        rows = db.execute(
            'SELECT conversation_id,substr(title,1,512),substr(workspace_uris,1,16385),'
            'last_modified_time,step_count,status FROM conversation_summaries ' + pred +
            ' ORDER BY last_modified_time DESC,conversation_id LIMIT ?', (limit + 1,)
        ).fetchall()
    out = []
    for sid, title, enc, mod, cnt, st in rows[:limit]:
        try:
            if str(uuid.UUID(sid)) != sid:
                raise ValueError()
            uris = json.loads(enc)
            if not isinstance(uris, list) or len(uris) > 32 or any(not isinstance(u, str) for u in uris):
                raise ValueError()
        except (ValueError, TypeError, AttributeError):
            raise ValueError('AGY_INDEX_ROW_INVALID') from None
        h = root / 'conversations' / (sid + '.db')
        loc = h.is_file() and not h.is_symlink() and h.resolve().is_relative_to(root)
        out.append({
            'nativeSessionId': sid, 'harness': 'agy', 'title': title, 'workspaceUris': uris,
            'updatedAt': mod, 'stepCount': cnt, 'nativeStatus': st, 'sourceKind': 'agy_index',
            'localHistoryPresent': loc, 'historyReadable': False,
            'historyReason': 'AGY_HISTORY_CODEC_UNSUPPORTED' if loc else 'AGY_HISTORY_NOT_LOCAL',
            'resumeVerified': False,
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
        out.append(_mk_row(f'claude-jsonl:{sid}', sid, 'claude', 'Claude', 'claude_native',
                           title or f'Claude · {Path(ws).name} ({sid[:8]})', ws, ms, cnt, 'completed', True))
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
        out.append(_mk_row(f'pi:{sid}', sid, 'pi', 'Pi', 'pi_native',
                           title or f'Pi · {wn} ({sid[:8]})', cwd or wn, ms, cnt))
    return out

def discover_codex(d, limit=20):
    sr = (Path(d) / 'sessions') if d else None
    if not sr or not sr.is_dir():
        return []
    out, seen = [], set()
    for f in sorted(sr.glob('**/*.jsonl'), key=lambda p: p.stat().st_mtime, reverse=True):
        if len(out) >= limit:
            break
        ms, sid, cwd, guard, ut, at = int(f.stat().st_mtime * 1000), None, '', False, None, None
        for _, o in _iter_jsonl(f, 120):
            if o.get('type') == 'session_meta':
                p = o.get('payload', {})
                sid, cwd, guard = p.get('id') or p.get('session_id'), p.get('cwd') or '', p.get('thread_source') == 'guardian_review'
            elif o.get('type') == 'response_item':
                p = o.get('payload', {})
                c = p.get('content')
                txt = '\n'.join(x.get('text', '') for x in c if isinstance(x, dict)) if isinstance(c, list) else str(c or '')
                if p.get('role') == 'user' and '<environment_context>' not in txt and not ut:
                    ut = _clean(txt)
                elif p.get('role') == 'assistant' and not at:
                    at = _clean(txt)
        if guard or (cwd and cwd in seen):
            continue
        if not sid:
            m = re.search(r'([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})', f.name)
            sid = m.group(1) if m else f.stem
        if cwd:
            seen.add(cwd)
        wn = Path(cwd).name if cwd else 'codex'
        out.append(_mk_row(f'codex:{sid}', sid, 'codex', 'Codex', 'codex_native',
                           at or ut or f'Codex · {wn}', cwd or str(f.parent), ms))
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
            out.append(_mk_row(f'cp:{at}:{tid}', tid, at, lmap.get(at, at.upper()),
                               f'{at}_control_plane', (title or pn or tid).strip(), wp or pn or 'vps3', uat, 1, st))
            if len(out) >= limit:
                break
    return out

def discover_multi(cfg):
    s = []
    if cfg.get('agyDataDir'):
        try:
            s.extend(discover(cfg['agyDataDir'], int(cfg.get('limit', 60))).get('sessions', []))
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
                r['execution_status'], r['ended_at'], chg = 'completed', now_ms, True
        out.append(r)
    if chg:
        _save_r(d)
    out.sort(key=lambda x: int(x.get('started_at') or 0))
    return out

def read_session_page(cfg, sid):
    ev, ws, h = [], '', 'dsh'
    push = lambda r, t, ts=0: ev.append((r, t.strip(), ts)) if (t or '').strip() else None
    if sid.startswith('claude-jsonl:'):
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
                    if not any(u2 == utxt for r2, u2, _ in ev[-4:] if r2 == 'user'):
                        push('user', utxt, ts)
                elif t == 'last-prompt' and o.get('lastPrompt'):
                    lp = str(o['lastPrompt']).strip()
                    if lp and lp != 'ping' and not lp.startswith('This session is being continued') and not any(
                        u2 == lp for r2, u2, _ in ev[-6:] if r2 == 'user'
                    ):
                        push('user', lp, ts)
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
            for _, o in _iter_jsonl(m[0]):
                ts = _ts(o.get('timestamp'), fb)
                if o.get('type') == 'session_meta':
                    ws = o.get('payload', {}).get('cwd') or ws
                elif o.get('type') == 'response_item':
                    p = o.get('payload', {})
                    if p.get('type') in ('function_call', 'custom_tool_call') and p.get('name'):
                        push('step', _tstep(p['name'], p.get('arguments') or p.get('input') or ''), ts)
                    elif p.get('role') in ('user', 'assistant'):
                        c = p.get('content')
                        txt = '\n'.join(x.get('text', '') for x in c if isinstance(x, dict)) if isinstance(c, list) else str(c or '')
                        if not any(g in txt for g in ('<environment_context>', '<skills_instructions>', '<plugins_instructions>')):
                            push(p['role'], txt, ts)
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
    recs = _reconcile(sid)
    ru = {t for r, t, _ in ev[-20:] if r == 'user'}
    ra = {t for r, t, _ in ev[-20:] if r == 'assistant'}
    for r in recs:
        ut, at, sat = (r.get('text') or '').strip(), (r.get('reply_text') or '').strip(), int(r.get('started_at') or 0)
        if ut and ut not in ru:
            idx = next((i for i, (_, _, ets) in enumerate(ev) if sat and ets and ets >= sat - 1000), len(ev))
            ev.insert(idx, ('user', ut, sat))
            ru.add(ut)
        if at and at not in ra and not (sid.startswith('claude-jsonl:') and ev and at.startswith('Execution error:')):
            push('assistant', at, int(r.get('ended_at') or sat))
            ra.add(at)
    msgs = _consolidate(ev, 60) or [{'sequence': 1, 'role': 'assistant', 'text': f'Session `{sid}` loaded (`{ws or "default"}`).', 'timestamp': _now()}]
    return {'harness': h, 'workspace': ws, 'messages': msgs,
            'request_results': [{'request_id': r['request_id'], 'status': r.get('execution_status', 'completed')} for r in recs if r.get('request_id')]}

def inspect_receipt(cfg):
    sid, rid = cfg.get('sessionId', ''), cfg.get('requestId', '')
    for r in _reconcile(sid):
        if r.get('request_id') == rid:
            return {'status': 'accepted', 'execution_status': r.get('execution_status', 'completed'), 'session_id': sid, 'request_id': rid}
    return {'status': 'unknown', 'session_id': sid, 'request_id': rid}

def _run_worker(cfg):
    sid, rid, text = cfg['session_id'], cfg['request_id'], cfg['text']
    pcwd, wcwd = cfg.get('project_cwd') or str(Path.home()), cfg.get('work_cwd') or str(Path.home())
    env = os.environ.copy()
    env['PATH'] = f"{Path.home() / '.local/bin'}:{Path.home() / '.npm-global/bin'}:/usr/local/bin:" + env.get('PATH', '')
    reply = ''
    try:
        if sid.startswith('claude-jsonl:'):
            raw = sid.split(':', 1)[1]
            cbin = shutil.which('claude', path=env['PATH']) or '/home/bobbik/.local/bin/claude'
            pr = subprocess.run([cbin, '--resume', raw, '-p', text, '--dangerously-skip-permissions'],
                                cwd=pcwd if Path(pcwd).is_dir() else str(Path.home()), env=env, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=3600)
            if (pr.stdout or '').strip() and pr.returncode == 0:
                reply = pr.stdout.strip()
            elif pr.returncode != 0:
                pr2 = subprocess.run([cbin, '-p', text, '--dangerously-skip-permissions'],
                                     cwd=wcwd if Path(wcwd).is_dir() else str(Path.home()), env=env, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=3600)
                reply = (pr2.stdout or pr2.stderr or pr.stderr or 'Completed.').strip()
        elif sid.startswith('codex:'):
            cbin = shutil.which('codex', path=env['PATH']) or 'codex'
            pr = subprocess.run([cbin, 'exec', text], cwd=wcwd if Path(wcwd).is_dir() else str(Path.home()), env=env, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=3600)
            reply = (pr.stdout or pr.stderr or 'Completed.').strip()
        elif sid.startswith('pi:'):
            cbin = shutil.which('pi', path=env['PATH']) or 'pi'
            pr = subprocess.run([cbin, '-p', text], cwd=wcwd if Path(wcwd).is_dir() else str(Path.home()), env=env, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=3600)
            reply = (pr.stdout or pr.stderr or 'Completed.').strip()
    except Exception as exc:
        if not sid.startswith('claude-jsonl:'):
            reply = f'Execution error: {exc}'
    d = _load_r()
    rec = d.get(rid) or {'request_id': rid, 'session_id': sid, 'status': 'accepted', 'text': text}
    rec['execution_status'], rec['ended_at'] = 'completed', _now()
    if reply:
        rec['reply_text'] = reply[:15000]
    d[rid] = rec
    _save_r(d)

def prompt_session(cfg):
    sid, text, rid = (cfg.get('sessionId') or '').strip(), (cfg.get('text') or '').strip(), (cfg.get('requestId') or '').strip()
    if not sid or not text or not rid:
        raise ValueError('DSH_REQUEST_REQUIRED')
    d = _load_r()
    if rid in d and d[rid].get('session_id') == sid and (d[rid].get('execution_status') == 'running' or d[rid].get('reply_text')):
        return {'status': 'accepted', 'session_id': sid, 'request_id': rid}
    pcwd = wcwd = str(Path.home())
    jpath = ''
    if sid.startswith('claude-jsonl:'):
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
    wcfg = json.dumps({'mode': 'worker', 'session_id': sid, 'request_id': rid, 'text': text, 'project_cwd': pcwd, 'work_cwd': wcwd, 'jsonl_path': jpath}, ensure_ascii=False)
    sf = Path.home() / '.bob_remote_probe.py'
    try:
        sf.write_text(globals().get('__BOB_PROBE_SRC__', ''), encoding='utf-8')
    except Exception:
        pass
    proc = subprocess.Popen([sys.executable, str(sf), wcfg], start_new_session=True, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    d[rid] = {'request_id': rid, 'session_id': sid, 'status': 'accepted', 'execution_status': 'running', 'text': text, 'pid': proc.pid, 'started_at': _now()}
    _save_r(d)
    return {'status': 'accepted', 'session_id': sid, 'request_id': rid}

if __name__ == '__main__':
    try:
        a1 = sys.argv[1] if len(sys.argv) > 1 else ''
        if a1.startswith('{'):
            cfg = json.loads(a1)
            m = cfg.get('mode', 'list')
            if m == 'page':
                print(json.dumps(read_session_page(cfg, cfg.get('sessionId', '')), ensure_ascii=False))
            elif m == 'prompt':
                print(json.dumps(prompt_session(cfg), ensure_ascii=False))
            elif m == 'receipt':
                print(json.dumps(inspect_receipt(cfg), ensure_ascii=False))
            elif m == 'worker':
                _run_worker(cfg)
            else:
                print(json.dumps(discover_multi(cfg), ensure_ascii=False))
        else:
            print(json.dumps(discover(a1, int(sys.argv[2]) if len(sys.argv[2:]) else 100), ensure_ascii=False))
    except (ValueError, OSError, sqlite3.Error, IndexError):
        print(json.dumps({'error': 'AGY_INDEX_READ_FAILED'}))
        sys.exit(1)
