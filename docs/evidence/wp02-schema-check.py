import argparse
import hashlib
import json
import re
import sqlite3
from pathlib import Path

p = argparse.ArgumentParser()
p.add_argument('document', type=Path)
p.add_argument('receipt', type=Path)
a = p.parse_args()
text = a.document.read_text(encoding='utf-8')
sql = re.findall(r'```sql\n(.*?)```', text, re.S)
assert len(sql) == 1
c = sqlite3.connect(':memory:')
c.executescript(sql[0])
results = []

def ok(name, condition):
    assert condition, name
    results.append({'check': name, 'result': 'passed'})

def put(table, **values):
    c.execute('INSERT INTO ' + table + ' (' + ','.join(values) + ') VALUES (' + ','.join('?' for _ in values) + ')', tuple(values.values()))

def rejects(name, operation):
    c.execute('SAVEPOINT negative_case')
    try:
        operation()
    except sqlite3.IntegrityError:
        results.append({'check': name, 'result': 'passed'})
    else:
        raise AssertionError(name + ': accepted invalid data')
    finally:
        c.execute('ROLLBACK TO negative_case')
        c.execute('RELEASE negative_case')

ok('DDL creates with foreign keys enabled', c.execute('PRAGMA foreign_keys').fetchone()[0] == 1)
for device in ['home', 'replica', 'worker']:
    put('devices', id=device, label=device, trust_state='paired', created_at=1)
put('home_authority', singleton=1, device_id='home', epoch=1)
for project in ['p1', 'p2']:
    put('projects', id=project, title=project, status='active', version=1, created_at=1, updated_at=1)
put('workspaces', id='w1', device_id='worker', directory_ref='fixture-workspace', policy_ref='fake-policy', version=1)
put('project_workspaces', project_id='p1', workspace_id='w1')
put('conversations', id='c1', project_id='p1', title='fixture', version=1, created_at=1)
put('goals', id='g1', project_id='p1', title='goal', status='active', version=1)
put('milestones', id='m1', project_id='p1', goal_id='g1', title='milestone', status='planned', version=1)

def task(tid, **kw):
    args = dict(id=tid, project_id='p1', goal_id='g1', milestone_id='m1', title='task', status='draft', constraints_json='{}', version=1, created_at=1, updated_at=1)
    args.update(kw)
    put('tasks', **args)

task('t1')
task('t2')
rejects('Cross-project goal rejected', lambda: task('bad', project_id='p2'))
rejects('Milestone requires goal', lambda: task('bad', goal_id=None))

def execution(eid, **kw):
    args = dict(id=eid, project_id='p1', task_id='t1', workspace_id='w1', request_id='req-' + eid,
                status='completed', requested_json='{}', contract_json='{}', contract_sha256='fake-hash', version=1)
    args.update(kw)
    put('executions', **args)

execution('e1')
rejects('Execution cannot use unrelated project workspace', lambda: execution('bad', project_id='p2'))
rejects('Observed identity requires evidence reference', lambda: execution('bad', observed_json='{}'))
rejects('Stop confirmation requires stop request', lambda: execution('bad', stop_confirmed_at=2))
rejects('Frozen requested config immutable', lambda: c.execute("UPDATE executions SET requested_json='[1]' WHERE id='e1'"))
execution('e-live', task_id='t2', status='unknown')
rejects('Unknown execution retains task exclusion', lambda: execution('bad', task_id='t2', status='pending'))
put('workspace_leases', workspace_id='w1', execution_id='e-live', home_epoch=1, fencing_token=1, state='unknown', updated_at=1)
rejects('Only one lease row per workspace', lambda: put('workspace_leases', workspace_id='w1', execution_id='e1', home_epoch=1, fencing_token=2, state='held', updated_at=1))
put('artifacts', id='a1', task_id='t1', title='fixture', kind='document')
put('artifact_versions', id='v1', artifact_id='a1', task_id='t1', execution_id='e1', ordinal=1,
    manifest_json='{}', manifest_sha256='fake-manifest', snapshot_ref='fixture-content', snapshot_state='available', created_at=1)
execution('e2', parent_execution_id='e1', parent_artifact_version_id='v1')
ok('Same-task explicit parent version accepted', c.execute("SELECT parent_artifact_version_id FROM executions WHERE id='e2'").fetchone()[0] == 'v1')
rejects('Cross-task parent version rejected', lambda: execution('bad', task_id='t2', parent_execution_id='e1', parent_artifact_version_id='v1'))
rejects('Artifact version update rejected', lambda: c.execute("UPDATE artifact_versions SET snapshot_ref='other' WHERE id='v1'"))
rejects('Artifact version delete rejected', lambda: c.execute("DELETE FROM artifact_versions WHERE id='v1'"))
put('verification_runs', id='vr1', task_id='t1', execution_id='e1', artifact_version_id='v1', status='pending', scope_json='{}', verifier='fixture')
rejects('Successful check requires evidence reference', lambda: put('verification_checks', id='bad', run_id='vr1', check_key='test', result='passed', explanation='fixture'))
rejects('Review cannot reference other execution version', lambda: put('reviews', id='bad', task_id='t1', execution_id='e2', artifact_version_id='v1', verification_run_id='vr1', actor_device_id='home', verdict='accepted', request_id='review-1', created_at=1))

def message(mid):
    put('messages', id=mid, conversation_id='c1', origin_device_id='replica', request_id='r-' + mid, role='user', content_json='{}', acceptance='local', created_at=1)

def outbox(rid, **kw):
    args = dict(request_id=rid, session_id='c1', origin_device_id='replica', target_device_id='home', home_epoch=1,
                kind='append_message', payload_json='{}', payload_sha256='fake-payload', send_state='local_saved', created_at=1, transport_evidence='none')
    args.update(kw)
    put('intent_outbox', **args)

c.commit()
with c:
    message('msg1')
    outbox('r-msg1', message_id='msg1')
ok('Message and outbox atomic positive insert', c.execute("SELECT count(*) FROM intent_outbox WHERE message_id='msg1'").fetchone()[0] == 1)
try:
    with c:
        message('rolled-back')
        outbox('r-rolled-back', message_id='rolled-back', send_state='invalid-state')
except sqlite3.IntegrityError:
    pass
ok('Failed transaction retains neither partial message nor outbox', c.execute("SELECT count(*) FROM messages WHERE id='rolled-back'").fetchone()[0] == 0)
rejects('Duplicate request ID constrained', lambda: outbox('r-msg1'))
rejects('Malformed payload JSON rejected', lambda: outbox('bad', payload_json='{'))
rejects('Mutation requires expected version', lambda: outbox('bad', kind='task_update'))
rejects('Unknown send requires transport evidence', lambda: outbox('bad', send_state='unknown_send'))
rejects('Peer accepted requires ACK metadata', lambda: outbox('bad', send_state='peer_accepted'))
put('inbox_receipts', request_id='r-msg1', origin_device_id='replica', home_epoch=1, payload_sha256='fake-payload', outcome='accepted', result_json='{}', effect_ref='fixture-effect', decided_at=2)
rejects('Receiver receipt ID unique across reconnect sessions', lambda: put('inbox_receipts', request_id='r-msg1', origin_device_id='replica', home_epoch=1, payload_sha256='different', outcome='accepted', result_json='{}', decided_at=3))
ok('Foreign key check clean after fixtures', not c.execute('PRAGMA foreign_key_check').fetchall())
ok('SQLite integrity check clean', c.execute('PRAGMA integrity_check').fetchone()[0] == 'ok')
receipt = {'scope':'WP-02 in-memory DDL and relational constraints only', 'document_sha256': hashlib.sha256(text.encode('utf-8')).hexdigest(), 'document_hash_basis': 'UTF-8 normalized LF',
           'sql_sha256': hashlib.sha256(sql[0].encode('utf-8')).hexdigest(), 'sqlite_version': sqlite3.sqlite_version,
           'table_count': c.execute("SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'").fetchone()[0],
           'passed': len(results), 'failed': 0, 'checks': results,
           'not_run':['Rust implementation','disk durability/crash recovery','network/DSH/CLI calls','Android/Windows app interaction','old app/online test suites','production migration'],
           'note':'Synthetic fixtures; these checks do not validate runtime state transitions or exactly-once external effects.'}
a.receipt.write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
print(json.dumps({k:v for k,v in receipt.items() if k not in ('checks',)}, ensure_ascii=False))
