"""Bounded, read-only AGY index discovery. Never decodes private history blobs.

The data directory is supplied by the host's authorization configuration.
This module does not authenticate, run the model, or create missing databases.
"""
import json
from contextlib import closing
import sqlite3
import sys
import uuid
from pathlib import Path

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
        # Limit malformed/oversized index queries; no raw_summary or credential reads.
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


if __name__ == '__main__':
    try:
        result = discover(sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 100)
        print(json.dumps(result, ensure_ascii=False))
    except (ValueError, OSError, sqlite3.Error, IndexError):
        print(json.dumps({'error': 'AGY_INDEX_READ_FAILED'}))
        sys.exit(1)
