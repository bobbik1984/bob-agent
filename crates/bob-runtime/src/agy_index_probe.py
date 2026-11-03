"""Bounded, read-only multi-harness index & session page probe (AGY, Claude, Pi, Codex, DSH/Control-Plane).

The data directories are supplied by the host's authorization configuration.
This module does not authenticate, run models, or create missing databases.
"""
from contextlib import closing
from datetime import datetime
import json
from pathlib import Path
import re
import sqlite3
import sys
import uuid

MAX_ROWS = 200
REQUIRED = {'conversation_id', 'title', 'workspace_uris', 'last_modified_time',
            'step_count', 'status'}


def discover(data_dir, limit=100):
    if isinstance(limit, bool) or not isinstance(limit, int) or not 1 <= limit <= MAX_ROWS:
        raise ValueError('AGY_INDEX_LIMIT_INVALID')
    root = Path(data_dir).resolve(strict=True)
    index = root / 'conversation_summaries.db'
    if index.is_symlink() or not index.is_file():
        raise ValueError('AGY_INDEX_UNAVAILABLE')
    with closing(sqlite3.connect(index.as_uri() + '?mode=ro', uri=True, timeout=2)) as db:
        db.execute('PRAGMA query_only=ON')
        budget = [0]
        def stop():
            budget[0] += 1
            return budget[0] > 2000
        db.set_progress_handler(stop, 1000)
        cols = {r[1] for r in db.execute('PRAGMA table_info(conversation_summaries)')}
        if not REQUIRED <= cols:
            raise ValueError('AGY_INDEX_SCHEMA_UNSUPPORTED')
        predicate = "WHERE COALESCE(parent_conversation_id,'')=''" if 'parent_conversation_id' in cols else ''
        rows = db.execute('SELECT conversation_id,substr(title,1,512),'
                          'substr(workspace_uris,1,16385),last_modified_time,step_count,status '
                          'FROM conversation_summaries ' + predicate +
                          ' ORDER BY last_modified_time DESC,conversation_id LIMIT ?',
                          (limit + 1,)).fetchall()
    sessions = []
    for sid, title, encoded, modified, count, status in rows[:limit]:
        try:
            if str(uuid.UUID(sid)) != sid:
                raise ValueError()
            uris = json.loads(encoded)
            if not isinstance(uris, list) or len(uris) > 32 or any(not isinstance(u, str) for u in uris):
                raise ValueError()
        except (ValueError, TypeError, AttributeError):
            raise ValueError('AGY_INDEX_ROW_INVALID') from None
        history = root / 'conversations' / (sid + '.db')
        local = history.is_file() and not history.is_symlink() and history.resolve().is_relative_to(root)
        sessions.append({
            'nativeSessionId': sid, 'harness': 'agy', 'title': title,
            'workspaceUris': uris, 'updatedAt': modified, 'stepCount': count,
            'nativeStatus': status, 'sourceKind': 'agy_index',
            'localHistoryPresent': local,
            'historyReadable': False,
            'historyReason': 'AGY_HISTORY_CODEC_UNSUPPORTED' if local else 'AGY_HISTORY_NOT_LOCAL',
            'resumeVerified': False,
        })
    return {'schemaVersion': 1, 'sessions': sessions, 'hasMore': len(rows) > limit,
            'authentication': 'unverified', 'source': 'local_readonly_index'}


def _parse_ts(val, fallback_ms=0):
    if isinstance(val, (int, float)):
        return int(val * 1000) if 0 < val < 1e11 else int(val)
    if isinstance(val, str) and val.strip():
        try:
            s = val.strip().replace('Z', '+00:00')
            return int(datetime.fromisoformat(s).timestamp() * 1000)
        except Exception:
            pass
    return fallback_ms


def _clean_title(text, fallback=''):
    if not text:
        return fallback
    t = str(text)
    t = re.sub(r'<environment_context>[\s\S]*?</environment_context>', ' ', t)
    t = re.sub(r'<skills_instructions>[\s\S]*?</skills_instructions>', ' ', t)
    t = re.sub(r'<plugins_instructions>[\s\S]*?</plugins_instructions>', ' ', t)
    t = re.sub(r'<[^>]+>', ' ', t)
    t = re.sub(r'END_TIME=\$\(date[^\n]*\n?', '', t)
    t = re.sub(r'DURATION=\$\(\([^\n]*\n?', '', t)
    t = re.sub(r'echo\s+"=== TURN[^\n]*\n?', '', t)
    t = re.sub(r'git status --short\s*', '', t)
    for line in t.splitlines():
        s = line.strip().lstrip('?#*- ')
        if not s or len(s) < 3:
            continue
        if s.startswith(('Project conversation context', 'Current request:', 'Recent messages:', '/', '---', '```')):
            continue
        if s.count('?') > len(s) * 0.35:
            continue
        return s[:110]
    return fallback


def discover_claude(claude_dir, limit=30):
    if not claude_dir:
        return []
    proj_root = Path(claude_dir) / 'projects'
    if not proj_root.is_dir():
        return []
    files = sorted(proj_root.glob('*/*.jsonl'), key=lambda p: p.stat().st_mtime, reverse=True)
    out = []
    for f in files[:limit]:
        sid = f.stem
        if '-tmp-bob-claude-probe' in str(f):
            continue
        mtime_ms = int(f.stat().st_mtime * 1000)
        title = None
        cwd = None
        msg_count = 0
        try:
            with open(f, 'r', encoding='utf-8', errors='replace') as fp:
                for i, line in enumerate(fp):
                    if i > 400 and title and cwd:
                        break
                    try:
                        obj = json.loads(line)
                    except Exception:
                        continue
                    if obj.get('cwd') and (not cwd or cwd in ('/home/bobbik', '/home/ubuntu')):
                        cwd = obj.get('cwd')
                    t = obj.get('type')
                    if t == 'queue-operation' and obj.get('operation') == 'enqueue' and obj.get('content') and not title:
                        title = _clean_title(obj.get('content'), '')
                    elif t == 'user' and not obj.get('isMeta'):
                        msg = obj.get('message', {})
                        content = msg.get('content') if isinstance(msg, dict) else None
                        if isinstance(content, str) and '<command-name>' not in content and '<local-command' not in content:
                            msg_count += 1
                            if not title:
                                title = _clean_title(content, '')
                    elif t == 'assistant':
                        msg_count += 1
        except OSError:
            continue
        ws = cwd or str(f.parent.name)
        if not title:
            title = f'Claude · {Path(ws).name} ({sid[:8]})'
        out.append({
            'id': f'claude-jsonl:{sid}',
            'nativeSessionId': sid,
            'harness': 'claude',
            'toolLabel': 'Claude',
            'sourceKind': 'claude_native',
            'title': title,
            'workspaceUris': [ws],
            'updatedAt': mtime_ms,
            'stepCount': max(msg_count, 1),
            'nativeStatus': 'completed',
            'localHistoryPresent': True,
            'historyReadable': True,
            'historyReason': None,
            'resumeVerified': True,
        })
    return out


def discover_pi(pi_dir, limit=30):
    if not pi_dir:
        return []
    root = Path(pi_dir)
    if not root.is_dir():
        return []
    files = sorted(root.glob('**/*.jsonl'), key=lambda p: p.stat().st_mtime, reverse=True)
    out = []
    for f in files[:limit]:
        mtime_ms = int(f.stat().st_mtime * 1000)
        sid = f.stem.split('_')[-1]
        cwd = ''
        title = None
        msg_count = 0
        try:
            with open(f, 'r', encoding='utf-8', errors='replace') as fp:
                for i, line in enumerate(fp):
                    if i > 200 and title and cwd:
                        break
                    try:
                        obj = json.loads(line)
                    except Exception:
                        continue
                    if obj.get('type') == 'session':
                        sid = obj.get('id') or sid
                        cwd = obj.get('cwd') or cwd
                    elif obj.get('type') == 'message':
                        msg_count += 1
                        msg = obj.get('message', {})
                        if msg.get('role') in ('user', 'assistant') and not title:
                            parts = msg.get('content', [])
                            txt = '\n'.join(p.get('text', '') for p in parts if isinstance(p, dict) and p.get('type') == 'text')
                            cand = _clean_title(txt, '')
                            if cand:
                                title = cand
        except OSError:
            continue
        ws_name = Path(cwd).name if cwd else f.parent.name.strip('-')
        if not title:
            title = f'Pi · {ws_name} ({sid[:8]})'
        out.append({
            'id': f'pi:{sid}',
            'nativeSessionId': sid,
            'harness': 'pi',
            'toolLabel': 'Pi',
            'sourceKind': 'pi_native',
            'title': title,
            'workspaceUris': [cwd or ws_name],
            'updatedAt': mtime_ms,
            'stepCount': max(msg_count, 1),
            'nativeStatus': 'completed',
            'localHistoryPresent': True,
            'historyReadable': True,
            'historyReason': None,
            'resumeVerified': False,
        })
    return out


def discover_codex(codex_dir, limit=20):
    if not codex_dir:
        return []
    root = Path(codex_dir)
    sess_root = root / 'sessions'
    if not sess_root.is_dir():
        return []
    files = sorted(sess_root.glob('**/*.jsonl'), key=lambda p: p.stat().st_mtime, reverse=True)
    out = []
    seen_cwds = set()
    for f in files:
        if len(out) >= limit:
            break
        mtime_ms = int(f.stat().st_mtime * 1000)
        sid = None
        cwd = ''
        is_guardian = False
        user_title = None
        asst_title = None
        try:
            with open(f, 'r', encoding='utf-8', errors='replace') as fp:
                for i, line in enumerate(fp):
                    if i > 120:
                        break
                    try:
                        obj = json.loads(line)
                    except Exception:
                        continue
                    if obj.get('type') == 'session_meta':
                        p = obj.get('payload', {})
                        sid = p.get('id') or p.get('session_id')
                        cwd = p.get('cwd') or ''
                        if p.get('thread_source') == 'guardian_review':
                            is_guardian = True
                    elif obj.get('type') == 'response_item':
                        p = obj.get('payload', {})
                        role = p.get('role')
                        content = p.get('content')
                        if isinstance(content, list):
                            txt = '\n'.join(c.get('text', '') for c in content if isinstance(c, dict))
                        else:
                            txt = str(content or '')
                        if role == 'user' and '<environment_context>' not in txt and not user_title:
                            user_title = _clean_title(txt, '')
                        elif role == 'assistant' and not asst_title:
                            asst_title = _clean_title(txt, '')
        except OSError:
            continue
        if is_guardian:
            continue
        if not sid:
            m = re.search(r'([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})', f.name)
            sid = m.group(1) if m else f.stem
        if cwd and cwd in seen_cwds:
            continue
        if cwd:
            seen_cwds.add(cwd)
        ws_name = Path(cwd).name if cwd else 'codex'
        title = asst_title or user_title or f'Codex · {ws_name}'
        out.append({
            'id': f'codex:{sid}',
            'nativeSessionId': sid,
            'harness': 'codex',
            'toolLabel': 'Codex',
            'sourceKind': 'codex_native',
            'title': title,
            'workspaceUris': [cwd or str(f.parent)],
            'updatedAt': mtime_ms,
            'stepCount': 1,
            'nativeStatus': 'completed',
            'localHistoryPresent': True,
            'historyReadable': True,
            'historyReason': None,
            'resumeVerified': False,
        })
    return out


def discover_control_plane(db_paths, allowed_harnesses=None, limit=40):
    if not db_paths:
        return []
    allowed = set(allowed_harnesses or ['dsh', 'pi', 'codex', 'agy'])
    label_map = {'dsh': 'DSH', 'pi': 'Pi', 'codex': 'Codex', 'agy': 'AGY', 'claude': 'Claude'}
    seen_tasks = set()
    out = []
    for dbp in db_paths:
        p = Path(dbp)
        if not p.is_file():
            continue
        try:
            with closing(sqlite3.connect(p.as_uri() + '?mode=ro', uri=True, timeout=2)) as conn:
                conn.execute('PRAGMA query_only=ON')
                rows = conn.execute('''
                    SELECT e.agent_type, t.id, t.title, t.status, t.updated_at, COALESCE(w.path, ''), COALESCE(p.name, '')
                    FROM executions e
                    JOIN tasks t ON t.id = e.task_id
                    LEFT JOIN workspaces w ON w.id = e.workspace_id
                    LEFT JOIN projects p ON p.id = t.project_id
                    WHERE e.agent_type IN ('dsh', 'pi', 'codex', 'agy', 'claude')
                      AND COALESCE(w.path, '') NOT LIKE '/home/bobbik/%'
                    ORDER BY t.updated_at DESC
                    LIMIT 120
                ''').fetchall()
        except Exception:
            continue
        for agent_type, tid, title, status, updated_at, wpath, pname in rows:
            if agent_type not in allowed:
                continue
            key = (agent_type, tid)
            if key in seen_tasks:
                continue
            if title and '自动测试，非人工验收' in title:
                continue
            seen_tasks.add(key)
            clean_t = (title or pname or tid).strip()
            out.append({
                'id': f'cp:{agent_type}:{tid}',
                'nativeSessionId': tid,
                'harness': agent_type,
                'toolLabel': label_map.get(agent_type, agent_type.upper()),
                'sourceKind': f'{agent_type}_control_plane',
                'title': clean_t,
                'workspaceUris': [wpath or pname or 'vps3'],
                'updatedAt': updated_at,
                'stepCount': 1,
                'nativeStatus': status or 'completed',
                'localHistoryPresent': True,
                'historyReadable': True,
                'historyReason': None,
                'resumeVerified': False,
            })
            if len(out) >= limit:
                break
    return out


def discover_multi(cfg):
    sessions = []
    agy_dir = cfg.get('agyDataDir')
    if agy_dir:
        try:
            res = discover(agy_dir, int(cfg.get('limit', 60)))
            sessions.extend(res.get('sessions', []))
        except Exception:
            pass
    sessions.extend(discover_claude(cfg.get('claudeDataDir')))
    sessions.extend(discover_pi(cfg.get('piDataDir')))
    sessions.extend(discover_codex(cfg.get('codexDataDir')))
    sessions.extend(discover_control_plane(cfg.get('controlPlaneDbs'), cfg.get('harnesses')))
    return {'schemaVersion': 1, 'sessions': sessions, 'hasMore': False,
            'authentication': 'unverified', 'source': 'local_readonly_index'}


def _fmt_tool_step(name, detail):
    clean = ' '.join(str(detail or '').split())
    if len(clean) > 180:
        clean = clean[:177] + '...'
    return f'`[Tool: {name}]` {clean}' if clean else f'`[Tool: {name}]`'


def _consolidate_raw_events(raw_events, limit=60):
    consolidated = []
    cur_steps = []
    cur_texts = []
    cur_ts = 0

    def flush_assistant():
        nonlocal cur_steps, cur_texts, cur_ts
        if not cur_steps and not cur_texts:
            return
        steps = []
        formal_parts = []
        text_only = [val for kind, val in cur_texts if kind == 'text']
        has_tools = any(kind == 'step' for kind, _ in cur_texts)
        if not text_only:
            steps = [val for _, val in cur_texts]
        elif not has_tools or len(text_only) == 1:
            steps = [val for kind, val in cur_texts if kind == 'step']
            formal_parts = text_only
        else:
            keep_count = 2 if (len(text_only) >= 2 and text_only[-1].rstrip().endswith(':') and len(text_only[-1]) < 240) else 1
            cutoff_idx = len(text_only) - keep_count
            seen_text = 0
            for kind, val in cur_texts:
                if kind == 'step':
                    steps.append(val)
                else:
                    if seen_text < cutoff_idx:
                        first_line = ' '.join(val.split())
                        if len(first_line) > 220:
                            first_line = first_line[:217] + '...'
                        steps.append(f'`[Thinking]` {first_line}')
                    else:
                        formal_parts.append(val)
                    seen_text += 1
        body = '\n\n'.join(formal_parts).strip()
        if steps:
            step_lines = '\n'.join(f'- {s}' for s in steps[:40])
            if len(steps) > 40:
                step_lines += f'\n- ... (+{len(steps) - 40} steps)'
            full_text = f'```thinking\n{step_lines}\n```\n\n{body}'.strip()
        else:
            full_text = body
        if full_text:
            consolidated.append(('assistant', full_text, cur_ts))
        cur_steps = []
        cur_texts = []
        cur_ts = 0

    for role, text, ts in raw_events:
        txt = (text or '').strip()
        if not txt:
            continue
        if role == 'user':
            flush_assistant()
            consolidated.append(('user', txt, ts))
        elif role == 'step':
            cur_steps.append(txt)
            cur_texts.append(('step', txt))
            if ts:
                cur_ts = ts
        else:
            cur_texts.append(('text', txt))
            if ts:
                cur_ts = ts
    flush_assistant()

    tail = consolidated[-limit:] if len(consolidated) > limit else consolidated
    out = []
    now_ms = int(datetime.now().timestamp() * 1000)
    for idx, (role, txt, ts) in enumerate(tail, 1):
        out.append({
            'sequence': idx,
            'role': role,
            'text': txt[:16000],
            'timestamp': ts or now_ms,
        })
    return out


def read_session_page(cfg, session_id):
    raw_events = []
    workspace = ''
    harness = 'dsh'

    def push(role, text, ts=0):
        txt = (text or '').strip()
        if txt:
            raw_events.append((role, txt, ts))

    if session_id.startswith('claude-jsonl:'):
        harness = 'claude'
        sid = session_id.split(':', 1)[1]
        claude_dir = cfg.get('claudeDataDir') or ''
        matches = list((Path(claude_dir) / 'projects').glob(f'*/{sid}.jsonl')) if claude_dir else []
        if matches:
            f = matches[0]
            fallback_ms = int(f.stat().st_mtime * 1000)
            with open(f, 'r', encoding='utf-8', errors='replace') as fp:
                for line in fp:
                    try:
                        obj = json.loads(line)
                    except Exception:
                        continue
                    if obj.get('cwd') and (not workspace or workspace in ('/home/bobbik', '/home/ubuntu')):
                        workspace = obj.get('cwd')
                    ts = _parse_ts(obj.get('timestamp'), fallback_ms)
                    t = obj.get('type')
                    if t == 'user' and not obj.get('isMeta'):
                        msg = obj.get('message', {})
                        c = msg.get('content') if isinstance(msg, dict) else None
                        if isinstance(c, str) and '<command-name>' not in c and '<local-command' not in c:
                            push('user', c, ts)
                    elif t == 'assistant':
                        msg = obj.get('message', {})
                        c = msg.get('content') if isinstance(msg, dict) else None
                        if isinstance(c, list):
                            for blk in c:
                                if not isinstance(blk, dict):
                                    continue
                                btype = blk.get('type')
                                if btype == 'thinking' and blk.get('thinking'):
                                    t_clean = ' '.join(str(blk['thinking']).split())
                                    if len(t_clean) > 200:
                                        t_clean = t_clean[:197] + '...'
                                    push('step', f'`[Thinking]` {t_clean}', ts)
                                elif btype == 'tool_use' and blk.get('name'):
                                    inp = blk.get('input', {}) if isinstance(blk.get('input'), dict) else {}
                                    cmd_str = inp.get('command') or inp.get('file_path') or inp.get('pattern') or inp.get('description') or ''
                                    push('step', _fmt_tool_step(blk['name'], cmd_str), ts)
                                elif btype == 'text' and blk.get('text'):
                                    push('assistant', blk['text'], ts)
                        elif isinstance(c, str):
                            push('assistant', c, ts)

    elif session_id.startswith('pi:'):
        harness = 'pi'
        sid = session_id.split(':', 1)[1]
        pi_dir = cfg.get('piDataDir') or ''
        matches = [p for p in Path(pi_dir).glob('**/*.jsonl') if sid in p.name] if pi_dir else []
        if matches:
            f = matches[0]
            fallback_ms = int(f.stat().st_mtime * 1000)
            with open(f, 'r', encoding='utf-8', errors='replace') as fp:
                for line in fp:
                    try:
                        obj = json.loads(line)
                    except Exception:
                        continue
                    if obj.get('type') == 'session':
                        workspace = obj.get('cwd') or workspace
                    elif obj.get('type') == 'message':
                        msg = obj.get('message', {})
                        role = msg.get('role')
                        ts = _parse_ts(msg.get('timestamp') or obj.get('timestamp'), fallback_ms)
                        if role in ('user', 'assistant'):
                            for blk in msg.get('content', []):
                                if not isinstance(blk, dict):
                                    continue
                                btype = blk.get('type')
                                if btype == 'thinking' and blk.get('thinking'):
                                    t_clean = ' '.join(str(blk['thinking']).split())
                                    if len(t_clean) > 200:
                                        t_clean = t_clean[:197] + '...'
                                    push('step', f'`[Thinking]` {t_clean}', ts)
                                elif btype == 'toolCall' and blk.get('name'):
                                    args = blk.get('arguments', {}) if isinstance(blk.get('arguments'), dict) else {}
                                    push('step', _fmt_tool_step(blk['name'], args.get('command') or args.get('path') or ''), ts)
                                elif btype == 'text' and blk.get('text'):
                                    push(role, blk['text'], ts)

    elif session_id.startswith('codex:'):
        harness = 'codex'
        sid = session_id.split(':', 1)[1]
        codex_dir = cfg.get('codexDataDir') or ''
        matches = [p for p in (Path(codex_dir) / 'sessions').glob('**/*.jsonl') if sid in p.name] if codex_dir else []
        if matches:
            f = matches[0]
            fallback_ms = int(f.stat().st_mtime * 1000)
            with open(f, 'r', encoding='utf-8', errors='replace') as fp:
                for line in fp:
                    try:
                        obj = json.loads(line)
                    except Exception:
                        continue
                    ts = _parse_ts(obj.get('timestamp'), fallback_ms)
                    if obj.get('type') == 'session_meta':
                        workspace = obj.get('payload', {}).get('cwd') or workspace
                    elif obj.get('type') == 'response_item':
                        p = obj.get('payload', {})
                        ptype = p.get('type')
                        role = p.get('role')
                        if ptype in ('function_call', 'custom_tool_call') and p.get('name'):
                            push('step', _fmt_tool_step(p['name'], p.get('arguments') or p.get('input') or ''), ts)
                        elif role in ('user', 'assistant'):
                            content = p.get('content')
                            txt = '\n'.join(c.get('text', '') for c in content if isinstance(c, dict)) if isinstance(content, list) else str(content or '')
                            if '<environment_context>' in txt or '<skills_instructions>' in txt or '<plugins_instructions>' in txt:
                                continue
                            push(role, txt, ts)

    elif session_id.startswith('cp:'):
        parts = session_id.split(':', 2)
        if len(parts) == 3:
            _, harness, tid = parts
            for dbp in cfg.get('controlPlaneDbs') or []:
                p = Path(dbp)
                if not p.is_file():
                    continue
                try:
                    with closing(sqlite3.connect(p.as_uri() + '?mode=ro', uri=True, timeout=2)) as conn:
                        conn.execute('PRAGMA query_only=ON')
                        t_row = conn.execute('SELECT title, instruction, status, created_at, updated_at, project_id FROM tasks WHERE id=?', (tid,)).fetchone()
                        if not t_row:
                            continue
                        title, instruction, status, created_at, updated_at, pid = t_row
                        w_row = conn.execute('SELECT w.path FROM executions e JOIN workspaces w ON w.id=e.workspace_id WHERE e.task_id=? LIMIT 1', (tid,)).fetchone()
                        if w_row and w_row[0]:
                            workspace = w_row[0]
                        push('user', instruction or title or tid, created_at or updated_at)
                        tables = {r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'").fetchall()}
                        if 'project_messages' in tables:
                            p_msgs = conn.execute('SELECT role, content, created_at FROM project_messages WHERE task_id=? ORDER BY sequence ASC LIMIT 30', (tid,)).fetchall()
                            for r_role, r_content, r_ts in p_msgs:
                                if r_content and r_content != instruction:
                                    push('user' if r_role == 'user' else 'assistant', r_content, r_ts)
                        execs = conn.execute('SELECT id, agent_type, status, model_id, runtime_session_ref, started_at, ended_at FROM executions WHERE task_id=? ORDER BY started_at ASC', (tid,)).fetchall()
                        for eid, atype, estatus, mid, sref, sat, eat in execs:
                            push('step', f"Execution `{eid}` · Harness: `{atype.upper()}` · Model: `{mid or 'default'}` · Status: `{estatus}`", sat or updated_at)
                            summary_parts = []
                            revs = conn.execute('SELECT status, note, created_at FROM reviews WHERE execution_id=? ORDER BY created_at ASC', (eid,)).fetchall()
                            for rstat, rnote, rts in revs:
                                if rnote:
                                    summary_parts.append(f"**Review ({rstat})**: {rnote}")
                            arts = conn.execute('SELECT type, name, substr(uri_or_content, 1, 1200) FROM artifacts WHERE execution_id=? LIMIT 5', (eid,)).fetchall()
                            for atype_name, aname, acontent in arts:
                                summary_parts.append(f"**Artifact `{aname}` ({atype_name})**:\n```\n{acontent}\n```")
                            if summary_parts:
                                push('assistant', '\n\n'.join(summary_parts), eat or sat or updated_at)
                        break
                except Exception:
                    continue

    messages = _consolidate_raw_events(raw_events, limit=60)
    if not messages:
        messages = [{
            'sequence': 1,
            'role': 'assistant',
            'text': f'Session `{session_id}` metadata loaded (workspace: `{workspace or "default"}`).',
            'timestamp': int(datetime.now().timestamp() * 1000),
        }]
    return {
        'harness': harness,
        'workspace': workspace,
        'messages': messages,
    }


if __name__ == '__main__':
    try:
        arg1 = sys.argv[1] if len(sys.argv) > 1 else ''
        if arg1.startswith('{'):
            cfg = json.loads(arg1)
            mode = cfg.get('mode', 'list')
            if mode == 'page':
                print(json.dumps(read_session_page(cfg, cfg.get('sessionId', '')), ensure_ascii=False))
            else:
                print(json.dumps(discover_multi(cfg), ensure_ascii=False))
        else:
            result = discover(arg1, int(sys.argv[2]) if len(sys.argv) > 2 else 100)
            print(json.dumps(result, ensure_ascii=False))
    except (ValueError, OSError, sqlite3.Error, IndexError):
        print(json.dumps({'error': 'AGY_INDEX_READ_FAILED'}))
        sys.exit(1)
