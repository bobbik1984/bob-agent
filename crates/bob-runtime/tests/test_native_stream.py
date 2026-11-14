import importlib.util,json,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('probe',Path(__file__).parents[1]/'src/agy_index_probe.py')
p=importlib.util.module_from_spec(spec);spec.loader.exec_module(p)

class NativeTests(unittest.TestCase):
 def test_setup_only_reports_ready_when_exact_native_history_is_readable(self):
  rid='setup-00000000-0000-4000-8000-000000000001'
  native='22222222-2222-4222-8222-222222222222'
  cfg={'mode':'setup','action':'check','requestId':rid,'harness':'codex','workspace':self.tmp.name}
  sid=rid.replace('setup-','setup:',1)
  record={'request_id':rid,'session_id':sid,'execution_status':'completed','native_session_id':native,
   'setup_harness':'codex','setup_workspace':self.tmp.name,'reply_text':'saved'}
  with p.closing(p._stream_db()) as db,db:
   db.execute('INSERT INTO requests VALUES(?,?,?,?)',(rid,sid,'hash',json.dumps(record)))
  with patch.object(p.subprocess,'Popen',side_effect=AssertionError('must never replay')):
   self.assertEqual(p.setup_native_session(cfg)['status'],'persisting')
   with patch.object(p,'read_session_page',return_value={'workspace':self.tmp.name,'messages':[{'role':'assistant','text':'saved'}]}):
    self.assertEqual(p.setup_native_session(cfg)['session_id'],'codex:'+native)
   with self.assertRaisesRegex(ValueError,'HARNESS_REQUEST_CONFLICT'):
    p.setup_native_session(dict(cfg,harness='pi'))

 def test_claude_tool_projection_preserves_outcome_and_bounds(self):
  use={'type':'assistant','message':{'content':[{'type':'tool_use','id':'u','name':'Read'}]}}
  result={'type':'user','message':{'content':[{'type':'tool_result','tool_use_id':'u','is_error':True}]}}
  a=p._project_claude_activity(None,use)
  self.assertEqual(a['items'][0]['status'],'running')
  a=p._project_claude_activity(a,result)
  a=p._project_claude_activity(a,use)
  self.assertEqual(a['items'],[{'id':'u','type':'tool','label':'Read','status':'error'}])
  for i in range(70):
   a=p._project_claude_activity(a,{'type':'assistant','message':{'content':[{'type':'tool_use','id':str(i),'name':'Read'}]}})
  self.assertEqual(len(a['items']),64);self.assertTrue(a['partial'])
 def test_claude_live_tool_snapshot_is_session_scoped(self):
  cfg=dict(self.cfg(),session_id='claude-jsonl:original')
  p._stream_event(cfg,'running')
  for sid,identity in [('original','good'),('other','wrong')]:
   p._stream_event(cfg,'source_event',{'source':'claude-stream-json','frame':{'type':'assistant','session_id':sid,'message':{'content':[{'type':'tool_use','id':identity,'name':'Read'}]}}})
  p._stream_event(cfg,'completed')
  page=p.read_native_events({'sessionId':cfg['session_id'],'epoch':''})
  items=page['snapshots'][0]['activity']['items']
  self.assertEqual([x['id'] for x in items],['good'])
  self.assertEqual(items[0]['status'],'unconfirmed')
 def test_claude_wire_recovers_tool_snapshot_without_reexecution(self):
  sid='claude-jsonl:original';rid='recover-tools';uid='input-id'
  record={'session_id':sid,'request_id':rid,'native_user_message_id':uid,'claude_wire':True,'execution_status':'unknown','reply_text':'partial'}
  frames=[{'session_id':sid,'request_id':rid,'user_uuid':uid},
   {'type':'assistant','session_id':'original','message':{'content':[{'type':'tool_use','id':'read-1','name':'Read'}]}},
   {'type':'user','session_id':'original','message':{'content':[{'type':'tool_result','tool_use_id':'read-1','content':'private file text'}]}},
   {'type':'result','session_id':'original','subtype':'success','result':'finished'}]
  path=p._claude_wire_path(rid);path.parent.mkdir(parents=True)
  path.write_text(''.join(json.dumps(x)+'\n' for x in frames),encoding='utf-8')
  with p.closing(p._stream_db()) as db,db:db.execute('INSERT INTO requests VALUES(?,?,?,?)',(rid,sid,'hash',json.dumps(record)))
  p._stream_event(record,'activity_snapshot',{'activity':{'items':[],'approvals':[{'id':'permission','status':'approved'}],'partial':False}})
  with patch.object(p.subprocess,'Popen',side_effect=AssertionError('must not execute')):
   self.assertEqual(p._stream_records(sid)[0]['execution_status'],'completed')
   p._stream_records(sid)
  page=p.read_native_events({'sessionId':sid,'epoch':''})
  activity=page['snapshots'][0]['activity']
  self.assertEqual(activity['items'],[{'id':'read-1','type':'tool','label':'Read','status':'completed'}])
  self.assertNotIn('private file text',json.dumps(activity))
  self.assertEqual(activity['approvals'],[{'id':'permission','status':'approved'}])
  with p.closing(p._stream_db()) as db:
   self.assertEqual(db.execute("SELECT COUNT(*) FROM events WHERE kind='activity_snapshot'").fetchone()[0],2)
 def test_claude_actual_receive_loop_retains_tool_results(self):
  cfg=dict(self.cfg(),session_id='claude-jsonl:12345678-1234-1234-1234-123456789abc',work_cwd=self.tmp.name)
  raw=cfg['session_id'].split(':')[1]
  with p.closing(p._stream_db()) as db,db:db.execute('INSERT INTO requests VALUES(?,?,?,?)',(cfg['request_id'],cfg['session_id'],'hash',json.dumps(dict(cfg,execution_status='accepted'))))
  frames=[{'type':'assistant','session_id':raw,'message':{'content':[{'type':'tool_use','id':'r','name':'Read'}]}},
   {'type':'user','session_id':raw,'message':{'content':[{'type':'tool_result','tool_use_id':'r','content':'fixture'}]}},
   {'type':'result','session_id':raw,'subtype':'success','result':'done'}]
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='claude'),patch.object(p,'_ensure_claude_session_available'),patch.object(p,'_claude_initialize'):
   pipe.return_value.receive.side_effect=frames;p._native_worker(cfg)
  page=p.read_native_events({'sessionId':cfg['session_id'],'epoch':''})
  self.assertEqual(page['snapshots'][0]['activity']['items'][0]['status'],'completed')
 def test_claude_tool_updates_reach_incremental_cursor_without_reload(self):
  cfg=dict(self.cfg(),session_id='claude-jsonl:original')
  p._stream_event(cfg,'running')
  first=p.read_native_events({'sessionId':cfg['session_id'],'epoch':''})
  use={'source':'claude-stream-json','frame':{'type':'assistant','session_id':'original','message':{'content':[{'type':'tool_use','id':'r','name':'Read'}]}}}
  p._stream_event(cfg,'source_event',use);p._stream_event(cfg,'source_event',use)
  p._stream_event(cfg,'source_event',{'source':'claude-stream-json','frame':{'type':'user','session_id':'original','message':{'content':[{'type':'tool_result','tool_use_id':'r'}]}}})
  page=p.read_native_events({'sessionId':cfg['session_id'],'epoch':first['epoch'],'after':first['cursor']})
  self.assertFalse(page['reset'])
  updates=[e['payload']['activity'] for e in page['events'] if e['kind']=='activity_snapshot']
  self.assertEqual([a['items'][0]['status'] for a in updates],['running','completed'])
 def test_claude_lost_worker_keeps_receiving_wire_progress_without_claiming_completion(self):
  sid='claude-jsonl:original';rid='ongoing-wire';uid='input'
  record={'session_id':sid,'request_id':rid,'native_user_message_id':uid,'claude_wire':True,'execution_status':'unknown','reply_text':'first','error_code':'HARNESS_PROCESS_LOST'}
  path=p._claude_wire_path(rid);path.parent.mkdir(parents=True)
  frames=[{'session_id':sid,'request_id':rid,'user_uuid':uid},
   {'type':'system','session_id':'original'},
   {'type':'stream_event','event':{'type':'content_block_delta','delta':{'type':'text_delta','text':'first continued'}}},
   {'type':'assistant','session_id':'original','message':{'content':[{'type':'tool_use','id':'read','name':'Read'}]}}]
  path.write_text(''.join(json.dumps(f)+'\n' for f in frames)+'{"partial":',encoding='utf-8')
  with p.closing(p._stream_db()) as db,db:db.execute('INSERT INTO requests VALUES(?,?,?,?)',(rid,sid,'hash',json.dumps(record)))
  with patch.object(p.subprocess,'Popen',side_effect=AssertionError('no replay')):
   recovered=p._stream_records(sid)[0];p._stream_records(sid)
  self.assertEqual(recovered['reply_text'],'first continued');self.assertEqual(recovered['execution_status'],'unknown')
  self.assertEqual(recovered['error_code'],'HARNESS_PROCESS_LOST')
  with p.closing(p._stream_db()) as db:
   counts=dict(db.execute('SELECT kind,COUNT(*) FROM events GROUP BY kind'))
  self.assertEqual(counts,{'activity_snapshot':1,'message_snapshot':1})
  progress={}
  path.write_text(''.join(json.dumps(f)+'\n' for f in frames)+json.dumps({'type':'system','session_id':'wrong'})+'\n',encoding='utf-8')
  self.assertIsNone(p._claude_wire_result(record,progress_out=progress));self.assertEqual(progress,{})
 def test_claude_permission_queue_exact_single_decision_and_no_replay(self):
  from unittest.mock import Mock
  for decision,cancelled in [('allow_once',False),('reject_once',False),('allow_once',True),(None,False)]:
   rid=str(decision)+str(cancelled);sid='claude-jsonl:original';cfg={'session_id':sid,'request_id':rid,'approval_mode':'interactive','claude_tools':['Read']}
   rec=dict(cfg,execution_status='running',pid=123,started_at=p._now(),tool_authorization={'version':1,'approval_mode':'interactive','tools':['Read']})
   with p.closing(p._stream_db()) as db,db:db.execute('INSERT INTO requests VALUES(?,?,?,?)',(rid,sid,'hash',json.dumps(rec)))
   pipe=Mock();frame={'type':'control_request','request_id':'permission-'+rid,'request':{'subtype':'can_use_tool','tool_name':'Read','input':{'file_path':'fixture.txt'}}}
   p._handle_claude_permission(cfg,pipe,frame);pipe.send.assert_not_called()
   with p.closing(p._stream_db()) as db:entry=json.loads(db.execute('SELECT body FROM native_permissions WHERE request=?',(rid,)).fetchone()[0])
   with patch.object(p,'_permission_owner_alive',return_value=True):
    p.submit_native_permission({'sessionId':sid,'requestId':rid,'permissionId':entry['id'],'optionId':decision,'claudeTools':['Read']})
   if cancelled:p._stream_update(rid,cancel_requested_at=p._now())
   p._flush_native_permissions(cfg,pipe);p._flush_native_permissions(cfg,pipe)
   pipe.send.assert_called_once()
   response=pipe.send.call_args.args[0]['response']
   self.assertEqual(response['request_id'],frame['request_id'])
   allowed=decision=='allow_once' and not cancelled
   self.assertEqual(response['response']['behavior'],'allow' if allowed else 'deny')
   if allowed:self.assertEqual(response['response']['updatedInput'],{'file_path':'fixture.txt'})
 def test_claude_permission_oversize_input_denied_without_queue(self):
  from unittest.mock import Mock
  pipe=Mock();cfg={'session_id':'claude-jsonl:original','request_id':'large','approval_mode':'interactive','claude_tools':['Read']}
  p._handle_claude_permission(cfg,pipe,{'request_id':'p','request':{'subtype':'can_use_tool','tool_name':'Read','input':{'text':'x'*9000}}})
  self.assertEqual(pipe.send.call_args.args[0]['response']['response']['behavior'],'deny')
 def test_claude_equal_prompts_keep_distinct_native_turns(self):
  root=Path(self.tmp.name)/'claude';folder=root/'projects'/'workspace';folder.mkdir(parents=True)
  raw='12345678-1234-1234-1234-123456789abc';rows=[]
  for i in range(2):
   rows.append({'type':'user','uuid':'user-'+str(i),'timestamp':1000+i*2000,'message':{'content':'Same request'}})
   rows.append({'type':'assistant','uuid':'reply-'+str(i),'timestamp':2000+i*2000,'message':{'content':[{'type':'text','text':'Reply '+str(i)}]}})
  rows.append(rows[0])
  (folder/(raw+'.jsonl')).write_text(''.join(json.dumps(x)+'\n' for x in rows),encoding='utf-8')
  receipt={'request_id':'next','native_user_message_id':'user-2','text':'Same request','reply_text':'','started_at':6000000,'execution_status':'running'}
  with patch.object(p,'_reconcile',return_value=[receipt]):
   page=p.read_session_page({'claudeDataDir':str(root)},'claude-jsonl:'+raw)
  messages=page['messages']
  self.assertEqual([m['role'] for m in messages],['user','assistant','user','assistant','user'])
  self.assertEqual([m['text'] for m in messages if m['role']=='user'],['Same request']*3)
  self.assertEqual([m['text'] for m in messages if m['role']=='assistant'],['Reply 0','Reply 1'])
  receipt['native_user_message_id']='user-1'
  with patch.object(p,'_reconcile',return_value=[receipt]):
   again=p.read_session_page({'claudeDataDir':str(root)},'claude-jsonl:'+raw)
  self.assertEqual(len(again['messages']),4)
 def test_claude_host_tool_policy_is_explicit_and_always_asks(self):
  self.assertEqual(p._claude_tool_arguments({}),['--tools','','--permission-mode','plan'])
  self.assertEqual(p._claude_tool_arguments({'claude_tools':['Read'],'approval_mode':'deny'})[:2],['--tools',''])
  args=p._claude_tool_arguments({'claude_tools':['Read','Read','Grep'],'approval_mode':'interactive'})
  self.assertEqual(args[:4],['--tools','Read,Grep','--permission-mode','default'])
  self.assertEqual(json.loads(args[-1]),{'permissions':{'ask':['Read','Grep']}})
  self.assertIn('stdio',args)
  for value in ['Read',['*'],['default'],['Default'],['Read(*)'],['--dangerously-skip-permissions'],[None],['Read']*33]:
   with self.assertRaisesRegex(ValueError,'TOOL_POLICY_INVALID'):
    p._claude_tool_arguments({'approval_mode':'interactive','claude_tools':value})
 def test_claude_ungranted_tool_cannot_open_approval(self):
  from unittest.mock import Mock
  cfg={'session_id':'claude-jsonl:original','request_id':'guard','approval_mode':'interactive','claude_tools':['Read']}
  pipe=Mock()
  with patch.object(p,'_open_native_permission') as queue:
   p._handle_claude_permission(cfg,pipe,{'request_id':'p','request':{'subtype':'can_use_tool','tool_name':'Bash','input':{'command':'echo test'}}})
  queue.assert_not_called()
  self.assertEqual(pipe.send.call_args.args[0]['response']['response']['behavior'],'deny')
 def test_claude_authorization_snapshot_precedes_spawn_and_survives_reentry(self):
  from unittest.mock import Mock
  cfg=dict(self.cfg(),session_id='claude-jsonl:12345678',approval_mode='interactive',claude_tools=['Read','Read'])
  def spawned(*args,**kwargs):
   with p.closing(p._stream_db()) as db:rec=json.loads(db.execute('SELECT body FROM requests WHERE id=?',(cfg['request_id'],)).fetchone()[0])
   self.assertEqual(rec['tool_authorization'],{'version':1,'approval_mode':'interactive','tools':['Read']})
   return Mock(pid=123)
  with patch.object(p.subprocess,'Popen',side_effect=spawned),patch.object(p,'_stream_records',return_value=[]):
   p._native_start(cfg,'# worker')
  with patch.object(p.subprocess,'Popen') as spawn,patch.object(p,'_stream_records',return_value=[]):
   p._native_start(dict(cfg,claude_tools=['Bash']),'# worker')
  spawn.assert_not_called()
  with p.closing(p._stream_db()) as db:rec=json.loads(db.execute('SELECT body FROM requests WHERE id=?',(cfg['request_id'],)).fetchone()[0])
  self.assertEqual(rec['tool_authorization']['tools'],['Read'])
 def test_claude_approval_intersects_original_and_current_grant(self):
  from unittest.mock import Mock
  for original,current in [([],['Read']),(['Read'],[]),(None,['Read'])]:
   rid=str(original)+str(current);sid='claude-jsonl:original';cfg={'session_id':sid,'request_id':rid,'approval_mode':'interactive','claude_tools':['Read']}
   rec=dict(cfg,execution_status='running',pid=123,started_at=p._now())
   if original is not None:rec['tool_authorization']={'version':1,'approval_mode':'interactive','tools':original}
   with p.closing(p._stream_db()) as db,db:db.execute('INSERT INTO requests VALUES(?,?,?,?)',(rid,sid,'hash',json.dumps(rec)))
   p._handle_claude_permission(cfg,Mock(),{'request_id':'p'+rid,'request':{'subtype':'can_use_tool','tool_name':'Read','input':{'file_path':'fixture.txt'}}})
   with p.closing(p._stream_db()) as db:entry=json.loads(db.execute('SELECT body FROM native_permissions WHERE request=?',(rid,)).fetchone()[0])
   decision={'sessionId':sid,'requestId':rid,'permissionId':entry['id'],'optionId':'allow_once','claudeTools':current}
   with patch.object(p,'_permission_owner_alive',return_value=True):
    with self.assertRaisesRegex(ValueError,'TOOL_NOT_GRANTED'):p.submit_native_permission(decision)
    p.submit_native_permission(dict(decision,optionId=None))
   pipe=Mock();p._flush_native_permissions(cfg,pipe)
   self.assertEqual(pipe.send.call_args.args[0]['response']['response']['behavior'],'deny')
 def test_external_recovery_keeps_older_turn_tool_results(self):
  sid=self.cfg()['session_id'];raw=sid.split(':',1)[1];cfg={'session_id':sid,'request_id':'external-codex-'+raw+'-older'}
  p._stream_event(cfg,'running')
  turn={'id':'older','status':'completed','items':[{'id':'command-1','type':'commandExecution','command':'read-only verification','status':'completed','exitCode':0},{'id':'reply','type':'agentMessage','text':'Finished'}]}
  p._restore_external_checkpoints(sid,raw,[turn,{'id':'latest','status':'inProgress'}])
  page=p.read_native_events({'sessionId':sid,'epoch':''});snap=page['snapshots'][0]
  self.assertEqual(snap['status'],'completed');self.assertEqual(snap['text'],'Finished')
  self.assertEqual(snap['activity']['items'][0]['status'],'completed')
  cursor=page['cursor'];p._restore_external_checkpoints(sid,raw,[turn])
  self.assertEqual(p.read_native_events({'sessionId':sid,'epoch':page['epoch'],'after':cursor})['events'],[])
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.home=patch.object(p.Path,'home',return_value=Path(self.tmp.name));self.home.start()
 def tearDown(self):self.home.stop();self.tmp.cleanup()
 def cfg(self):return {'request_id':'request-a','session_id':'codex:12345678-1234-1234-1234-123456789abc','text':'read only'}
 def test_expired_cursor_restores_folded_text_and_terminal_without_replay(self):
  cfg=self.cfg()
  with patch.object(p,'EVENT_MAX_ROWS',2):
   p._stream_event(cfg,'accepted',{'cancel_supported':True})
   first=p.read_native_events({'sessionId':cfg['session_id']})
   for text in ['one ','two ','three']:p._stream_event(cfg,'text_delta',{'text':text})
   p._stream_event(cfg,'completed')
   page=p.read_native_events({'sessionId':cfg['session_id'],'after':first['cursor'],'epoch':first['epoch']})
  self.assertTrue(page['reset']);self.assertEqual(page['events'],[])
  self.assertEqual(page['snapshots'][0]['text'],'one two three');self.assertEqual(page['snapshots'][0]['status'],'completed')
  again=p.read_native_events({'sessionId':cfg['session_id'],'after':page['cursor'],'epoch':page['epoch']})
  self.assertFalse(again['reset']);self.assertEqual(again['events'],[])
 def test_late_frames_cannot_reopen_terminal_checkpoint(self):
  for terminal in ('completed','cancelled','error'):
   cfg=dict(self.cfg(),request_id=terminal)
   p._stream_event(cfg,'text_delta',{'text':'kept'})
   p._stream_event(cfg,terminal)
   for kind in ('running','text_delta','cancel_requested'):
    p._stream_event(cfg,kind,{'text':'late'})
  page=p.read_native_events({'sessionId':cfg['session_id'],'epoch':''})
  self.assertEqual(len(page['snapshots']),3)
  for row in page['snapshots']:
   self.assertEqual(row['status'],row['request_id']);self.assertEqual(row['text'],'kept');self.assertFalse(row.get('cancel_pending'))
 def test_external_reconnect_reconciles_old_running_turns_without_claiming_unknown_complete(self):
  sid=self.cfg()['session_id'];raw=sid.split(':')[1]
  for tid in ('done','missing','live'):
   p._stream_event({'session_id':sid,'request_id':'external-codex-'+raw+'-'+tid},'running')
  p._restore_external_checkpoints(sid,raw,[{'id':'done','status':'completed','items':[{'type':'agentMessage','text':'final'}]},{'id':'live','status':'inProgress'}])
  page=p.read_native_events({'sessionId':sid,'epoch':''});rows={r['request_id'].rsplit('-',1)[1]:r for r in page['snapshots']}
  self.assertEqual(rows['done']['status'],'completed');self.assertEqual(rows['done']['text'],'final')
  self.assertEqual(rows['missing']['status'],'unknown');self.assertEqual(rows['live']['status'],'running')
 def test_terminal_receipt_recovers_event_missing_after_process_exit(self):
  cfg=self.cfg()
  with patch.object(p.subprocess,'Popen') as spawn:
   spawn.return_value.pid=123;p._native_start(cfg,'# worker')
  p._stream_update(cfg['request_id'],execution_status='completed',reply_text='final reply',ended_at=p._now())
  page=p.read_native_events({'sessionId':cfg['session_id'],'epoch':''})
  self.assertEqual(page['snapshots'][0]['status'],'completed');self.assertEqual(page['snapshots'][0]['text'],'final reply')
  again=p.read_native_events({'sessionId':cfg['session_id'],'epoch':page['epoch'],'after':page['cursor']})
  self.assertEqual(again['events'],[])
 def test_rejected_or_unknown_receipt_cannot_restore_a_running_indicator(self):
  for state in ('not_accepted','unknown'):
   cfg=dict(self.cfg(),request_id=state)
   with patch.object(p.subprocess,'Popen') as spawn:
    spawn.return_value.pid=123;p._native_start(cfg,'# worker')
   p._stream_update(cfg['request_id'],execution_status=state)
  page=p.read_native_events({'sessionId':cfg['session_id'],'epoch':''})
  self.assertEqual({r['status'] for r in page['snapshots']},{'error','unknown'})
 def test_old_receipt_reply_is_inserted_at_its_time_not_after_latest_turn(self):
  cfg=self.cfg();raw=cfg['session_id'].split(':')[1]
  root=Path(self.tmp.name)/'codex';(root/'sessions').mkdir(parents=True)
  rows=[{'type':'response_item','timestamp':'2026-11-07T01:00:00Z','payload':{'type':'message','role':'user','content':[{'text':'new question'}]}},{'type':'response_item','timestamp':'2026-11-07T01:01:00Z','payload':{'type':'message','role':'assistant','content':[{'text':'latest answer'}]}}]
  (root/'sessions'/('rollout-'+raw+'.jsonl')).write_text('\n'.join(json.dumps(r) for r in rows),encoding='utf-8')
  old={'request_id':'old','text':'old question','reply_text':'old answer','execution_status':'completed','started_at':1793806373343,'ended_at':1793806403155}
  with patch.object(p,'_reconcile',return_value=[old]):page=p.read_session_page({'codexDataDir':str(root)},cfg['session_id'])
  self.assertEqual([m['text'] for m in page['messages']],['old question','old answer','new question','latest answer'])
 def test_cancelled_partial_history_survives_later_turn_and_internal_markers_are_hidden(self):
  cfg=self.cfg();raw=cfg['session_id'].split(':')[1]
  root=Path(self.tmp.name)/'codex';(root/'sessions').mkdir(parents=True)
  texts=['<turn_aborted>internal interruption</turn_aborted>','<codex_internal_context source="goal">internal continuation</codex_internal_context>','new question mentions <turn_aborted> as literal text']
  rows=[{'type':'response_item','timestamp':'2026-11-07T01:00:00Z','payload':{'type':'message','role':'user','content':[{'text':t}]}} for t in texts]
  (root/'sessions'/('rollout-'+raw+'.jsonl')).write_text('\n'.join(json.dumps(r) for r in rows),encoding='utf-8')
  old={'request_id':'old','text':'old question','reply_text':'partial cancelled answer','execution_status':'cancelled','started_at':1793806373343,'ended_at':1793806403155}
  with patch.object(p,'_reconcile',return_value=[old]):
   page=p.read_session_page({'codexDataDir':str(root)},cfg['session_id'])
   again=p.read_session_page({'codexDataDir':str(root)},cfg['session_id'])
  self.assertEqual([m['text'] for m in page['messages']],['old question','partial cancelled answer',texts[-1]])
  self.assertEqual(page,again);self.assertEqual(page['request_results'][0]['status'],'cancelled')
  rows.append({'type':'response_item','timestamp':'2026-11-07T00:00:00Z','payload':{'type':'message','role':'assistant','content':[{'text':'partial cancelled answer'}]}})
  (root/'sessions'/('rollout-'+raw+'.jsonl')).write_text('\n'.join(json.dumps(r) for r in rows),encoding='utf-8')
  with patch.object(p,'_reconcile',return_value=[old]):page=p.read_session_page({'codexDataDir':str(root)},cfg['session_id'])
  self.assertEqual(sum(m['text'].count('partial cancelled answer') for m in page['messages']),1)
 def test_receipt_combined_reply_deduplicates_only_matching_native_turn(self):
  cfg=self.cfg();raw=cfg['session_id'].split(':')[1]
  root=Path(self.tmp.name)/'codex';(root/'sessions').mkdir(parents=True)
  rows=[{'type':'turn_context','payload':{'turn_id':'exact'}}]
  rows.extend({'type':'response_item','payload':{'type':'message','role':'assistant','content':[{'text':t}]}} for t in ['first','last'])
  (root/'sessions'/('rollout-'+raw+'.jsonl')).write_text('\n'.join(json.dumps(r) for r in rows),encoding='utf-8')
  receipt={'request_id':'original','native_turn_id':'exact','reply_text':'first\n\nlast','execution_status':'completed'}
  with patch.object(p,'_reconcile',return_value=[receipt]):page=p.read_session_page({'codexDataDir':str(root)},cfg['session_id'])
  self.assertEqual(sum(m['text'].count('first') for m in page['messages']),1)
  receipt['native_turn_id']='different'
  with patch.object(p,'_reconcile',return_value=[receipt]):page=p.read_session_page({'codexDataDir':str(root)},cfg['session_id'])
  self.assertEqual(sum(m['text'].count('first') for m in page['messages']),2)
 def test_codex_tool_returns_are_bound_by_call_and_turn_and_bounded(self):
  cfg=self.cfg();raw=cfg['session_id'].split(':')[1]
  root=Path(self.tmp.name)/'codex';(root/'sessions').mkdir(parents=True)
  rows=[{'type':'turn_context','payload':{'turn_id':'first'}},
   {'type':'response_item','payload':{'type':'custom_tool_call','name':'exec','call_id':'a','input':'readonly'}},
   {'type':'response_item','payload':{'type':'custom_tool_call_output','call_id':'a','output':[{'type':'input_text','text':'permission failed '+('x'*700)}]}},
   {'type':'response_item','payload':{'type':'custom_tool_call_output','call_id':'a','output':'duplicate result'}},
   {'type':'response_item','payload':{'type':'custom_tool_call','name':'exec','call_id':'b'}},
   {'type':'turn_context','payload':{'turn_id':'second'}},
   {'type':'response_item','payload':{'type':'custom_tool_call_output','call_id':'b','output':'wrong turn result'}}]
  (root/'sessions'/('rollout-'+raw+'.jsonl')).write_text('\n'.join(json.dumps(r) for r in rows),encoding='utf-8')
  with patch.object(p,'_reconcile',return_value=[]):page=p.read_session_page({'codexDataDir':str(root)},cfg['session_id'])
  text=' '.join(m['text'] for m in page['messages'])
  self.assertIn('[ToolResult: exec]',text);self.assertIn('permission failed',text)
  self.assertNotIn('duplicate result',text);self.assertNotIn('wrong turn result',text);self.assertLess(len(text),850)
  self.assertNotIn('```',p._tool_result_step('exec',[{'text':'```unsafe```'}]))
 def test_epoch_change_and_ahead_cursor_force_baseline(self):
  cfg=self.cfg();p._stream_event(cfg,'message_snapshot',{'text':'latest'})
  for extra in [{'epoch':'old'},{'after':999999}]:
   page=p.read_native_events(dict(sessionId=cfg['session_id'],**extra));self.assertTrue(page['reset']);self.assertEqual(page['snapshots'][0]['text'],'latest')
 def test_initial_new_client_baseline_is_session_scoped(self):
  cfg=self.cfg();p._stream_event(cfg,'text_delta',{'text':'mine'});p._stream_event(dict(cfg,request_id='other',session_id='pi:other'),'text_delta',{'text':'private'})
  page=p.read_native_events({'sessionId':cfg['session_id'],'epoch':''})
  self.assertTrue(page['reset']);self.assertEqual([r['text'] for r in page['snapshots']],['mine'])
 def test_byte_budget_and_unicode_recovery(self):
  cfg=self.cfg()
  with patch.object(p,'EVENT_MAX_BYTES',90):
   for _ in range(6):p._stream_event(cfg,'text_delta',{'text':'你好世界'})
   page=p.read_native_events({'sessionId':cfg['session_id'],'epoch':''})
   with p.closing(p._stream_db()) as db,db:self.assertLessEqual(db.execute('SELECT bytes FROM event_size').fetchone()[0],90)
  self.assertEqual(page['snapshots'][0]['text'],'你好世界'*6)
 def test_old_worker_direct_events_are_projected_before_retention(self):
  cfg=self.cfg()
  with p.closing(p._stream_db()) as db,db:
   for text in ['old ','worker']:db.execute('INSERT INTO events(request,session,kind,payload,at) VALUES(?,?,?,?,?)',(cfg['request_id'],cfg['session_id'],'text_delta',json.dumps({'text':text}),p._now()))
  with patch.object(p,'EVENT_MAX_ROWS',0):page=p.read_native_events({'sessionId':cfg['session_id'],'epoch':''})
  self.assertEqual(page['snapshots'][0]['text'],'old worker')
 def test_transport_cleanup_does_not_delete_unknown_receipt(self):
  cfg,args=self.seed_cancel();p._stream_update(cfg['request_id'],execution_status='unknown',reply_text='retained')
  with patch.object(p,'SNAPSHOT_MAX_ROWS',0),patch.object(p,'EVENT_MAX_ROWS',0):page=p.read_native_events({'sessionId':cfg['session_id'],'epoch':''})
  self.assertEqual(page['snapshots'][0]['text'],'retained');self.assertEqual(page['snapshots'][0]['status'],'unknown')
  self.assertEqual(p._stream_records(cfg['session_id'])[0]['execution_status'],'unknown')
 def test_terminal_event_survives_worker_crash_before_receipt_update(self):
  cfg,args=self.seed_cancel();p._stream_event(cfg,'message_snapshot',{'text':'done'});p._stream_event(cfg,'completed')
  r=p._stream_records(cfg['session_id'])[0];self.assertEqual(r['execution_status'],'completed');self.assertEqual(r['reply_text'],'done')
 def test_page_limit_exposes_more_without_skipping_tail(self):
  cfg=self.cfg()
  for i in range(105):p._stream_event(cfg,'text_delta',{'text':str(i)+','})
  one=p.read_native_events({'sessionId':cfg['session_id']});self.assertEqual(len(one['events']),100);self.assertTrue(one['has_more'])
  two=p.read_native_events({'sessionId':cfg['session_id'],'after':one['cursor'],'epoch':one['epoch']});self.assertEqual(len(two['events']),5);self.assertFalse(two['has_more'])
 def test_age_cleanup_recovers_pending_cancel_from_receipt(self):
  cfg,args=self.seed_cancel();p.request_native_cancel(args)
  with patch.object(p,'EVENT_KEEP_MS',-1):page=p.read_native_events({'sessionId':cfg['session_id'],'epoch':''})
  self.assertTrue(page['snapshots'][0]['cancel_pending']);self.assertEqual(page['snapshots'][0]['status'],'accepted')
 def test_external_socket_partial_frames_survive_timeout(self):
  import socket
  a,b=socket.socketpair();pipe=p.CodexControlSocket.__new__(p.CodexControlSocket);pipe.s=a;pipe.buf=b'';pipe.fragment=b''
  try:
   body=json.dumps({'method':'text','params':{'text':'你好'}}).encode('utf-8');packet=bytes([0x81,len(body)])+body
   b.sendall(packet[:5]);self.assertIsNone(pipe.receive(.01));b.sendall(packet[5:]);self.assertEqual(pipe.receive(.1)['params']['text'],'你好')
  finally:a.close();b.close()
 def test_external_socket_ping_and_fragmentation(self):
  import socket
  a,b=socket.socketpair();pipe=p.CodexControlSocket.__new__(p.CodexControlSocket);pipe.s=a;pipe.buf=b'';pipe.fragment=b''
  try:
   body=b'{"id":1}';b.sendall(bytes([1,3])+body[:3]+bytes([0x89,1])+b'x'+bytes([0x80,len(body)-3])+body[3:])
   self.assertEqual(pipe.receive(.1),{'id':1});self.assertEqual(b.recv(100)[0],0x8a)
  finally:a.close();b.close()
 def test_external_socket_rejects_oversized_frame(self):
  import socket,struct
  a,b=socket.socketpair();pipe=p.CodexControlSocket.__new__(p.CodexControlSocket);pipe.s=a;pipe.buf=b'';pipe.fragment=b''
  try:
   b.sendall(bytes([0x81,127])+struct.pack('!Q',9*1024*1024))
   with self.assertRaisesRegex(ValueError,'OUTPUT_LIMIT'):pipe.receive(.1)
  finally:a.close();b.close()
 def test_external_observer_never_resumes_unloaded_disk_thread(self):
  with patch.object(p,'_codex_socket_paths',return_value=['socket']),patch.object(p,'CodexControlSocket') as pipe,patch.object(p,'_control_result',side_effect=[{}, {'data':['other']} ]):
   self.assertEqual(p._open_loaded_codex('wanted'),(None,None))
   sent=[c.args[0] for c in pipe.return_value.send.call_args_list]
   self.assertFalse(any(f.get('method') in ('thread/resume','turn/start') for f in sent))
 def test_external_observer_attaches_loaded_thread_without_new_turn_or_permission_changes(self):
  with patch.object(p,'_codex_socket_paths',return_value=['socket']),patch.object(p,'CodexControlSocket') as pipe,patch.object(p,'_control_result',side_effect=[{}, {'data':['wanted']},{'thread':{'id':'wanted'}},{'data':[{'id':'latest','status':'inProgress','items':[]}]}]):
   self.assertIsNotNone(p._open_loaded_codex('wanted')[0]);sent=[c.args[0] for c in pipe.return_value.send.call_args_list]
   self.assertEqual(sent[-2],{'id':3,'method':'thread/resume','params':{'threadId':'wanted','excludeTurns':True}})
   self.assertEqual(sent[-1],{'id':4,'method':'thread/turns/list','params':{'threadId':'wanted','limit':1,'sortDirection':'desc','itemsView':'full'}})
   self.assertFalse(any(f.get('method')=='turn/start' for f in sent))
 def test_external_bounded_attach_replaces_old_turns_with_latest(self):
  latest={'id':'now','status':'inProgress','items':[{'id':'answer','type':'agentMessage','text':'live'}]}
  with patch.object(p,'_codex_socket_paths',return_value=['socket']),patch.object(p,'CodexControlSocket'),patch.object(p,'_control_result',side_effect=[{}, {'data':['wanted']},{'thread':{'id':'wanted','turns':[{'id':'old'}]}},{'data':[latest]}]):
   _,thread=p._open_loaded_codex('wanted')
   self.assertEqual(thread['turns'],[latest])
 def test_external_bounded_attach_unsupported_never_falls_back_to_full_history(self):
  with patch.object(p,'_codex_socket_paths',return_value=['socket']),patch.object(p,'CodexControlSocket') as pipe,patch.object(p,'_control_result',side_effect=[{}, {'data':['wanted']},ValueError('HARNESS_CONTROL_REJECTED')]):
   self.assertEqual(p._open_loaded_codex('wanted'),(None,None))
   calls=[c.args[0] for c in pipe.return_value.send.call_args_list]
   self.assertEqual(len([c for c in calls if c.get('method')=='thread/resume']),1)
   self.assertFalse(any(c.get('method')=='turn/start' for c in calls))
   pipe.return_value.close.assert_called_once()
 def test_control_response_does_not_confuse_inbound_approval_with_reply(self):
  from unittest.mock import Mock
  pipe=Mock()
  pipe.receive.side_effect=[{'id':4,'method':'item/commandExecution/requestApproval','params':{'threadId':'wanted'}},{'id':4,'result':{'data':[{'id':'latest'}]}}]
  self.assertEqual(p._control_result(pipe,4),{'data':[{'id':'latest'}]})
  pipe.send.assert_not_called()
 def test_control_error_remains_an_error_after_unrelated_event(self):
  from unittest.mock import Mock
  pipe=Mock()
  pipe.receive.side_effect=[{'method':'turn/started','params':{}},{'id':4,'error':{'code':-32601}}]
  with self.assertRaisesRegex(ValueError,'HARNESS_CONTROL_REJECTED'):p._control_result(pipe,4)
 def test_control_wait_persists_exact_turn_events_without_double_appending_text(self):
  from unittest.mock import Mock
  sid=self.cfg()['session_id'];raw=sid.split(':',1)[1];rid='external-codex-'+raw+'-turn-live'
  cfg={'session_id':sid,'request_id':rid}
  p._stream_event(cfg,'message_snapshot',{'text':'already included'})
  frames=[{'method':'item/agentMessage/delta','params':{'threadId':raw,'turnId':'turn-live','itemId':'item','delta':'already included'}},
   {'id':4,'method':'item/commandExecution/requestApproval','params':{'threadId':raw,'turnId':'turn-live','itemId':'tool'}},
   {'id':4,'result':{'data':[]}}]
  pipe=Mock();pipe.receive.side_effect=frames
  self.assertEqual(p._control_result(pipe,4,lambda f:p._capture_external_codex_event(sid,raw,f)),{'data':[]})
  from contextlib import closing
  with closing(p._stream_db()) as db:
   rows=db.execute("SELECT request,payload FROM events WHERE kind='source_event' ORDER BY sequence").fetchall()
   self.assertEqual(len(rows),2);self.assertTrue(all(row[0]==rid for row in rows))
   self.assertEqual(json.loads(rows[1][1])['source_request_id'],4)
   body=json.loads(db.execute('SELECT body FROM event_snapshots WHERE request=?',(rid,)).fetchone()[0]);self.assertEqual(body['text'],'already included')
  pipe.send.assert_not_called()
 def test_source_event_requires_exact_thread_and_turn_and_bounded_payload(self):
  sid=self.cfg()['session_id'];raw=sid.split(':',1)[1]
  for params in [{'threadId':'other','turnId':'t'},{'threadId':raw},{'threadId':raw,'turnId':123}]:
   self.assertFalse(p._capture_external_codex_event(sid,raw,{'method':'item/started','params':params}))
  with self.assertRaisesRegex(ValueError,'OUTPUT_LIMIT'):
   p._capture_external_codex_event(sid,raw,{'method':'item/started','params':{'threadId':raw,'turnId':'t','text':'x'*262145}})
 def test_control_wait_capture_failure_is_not_silently_discarded(self):
  from unittest.mock import Mock
  pipe=Mock();pipe.receive.return_value={'method':'item/started','params':{}}
  with self.assertRaisesRegex(ValueError,'OUTPUT_LIMIT'):
   p._control_result(pipe,1,Mock(side_effect=ValueError('HARNESS_OUTPUT_LIMIT')))
 def test_activity_survives_event_pruning_and_does_not_invent_approval_result(self):
  sid=self.cfg()['session_id'];raw=sid.split(':',1)[1];rid='external-codex-'+raw+'-activity'
  cfg={'session_id':sid,'request_id':rid}
  with patch.object(p,'EVENT_MAX_ROWS',2):
   p._stream_event(cfg,'running');before=p.read_native_events({'sessionId':sid})
   def event(method,item=None,**extra):
    frame={'method':method,'params':{'threadId':raw,'turnId':'activity',**({'item':item} if item else {}),**extra}}
    if method.endswith('/requestApproval'):frame['id']='approval-1'
    p._capture_external_codex_event(sid,raw,frame)
   event('item/started',{'id':'tool-1','type':'commandExecution','command':'pytest'})
   event('item/completed',{'id':'tool-1','type':'commandExecution','command':'pytest','exitCode':0})
   event('item/commandExecution/requestApproval',itemId='tool-2')
   restored=p.read_native_events({'sessionId':sid,'after':before['cursor'],'epoch':before['epoch']})
   self.assertTrue(restored['reset']);activity=restored['snapshots'][0]['activity']
   self.assertEqual(activity['items'][0]['status'],'completed');self.assertEqual(activity['approvals'][0]['status'],'pending')
   p._stream_event(cfg,'unknown')
   reset=p.read_native_events({'sessionId':sid,'epoch':''})
   self.assertEqual(reset['snapshots'][0]['activity']['approvals'][0]['status'],'unconfirmed')
 def test_activity_is_bounded_idempotent_and_late_start_cannot_reopen_completed_item(self):
  a=None
  for i in range(70):a=p._project_source_activity(a,{'method':'item/started','params':{'item':{'id':str(i),'type':'commandExecution','command':'x'*1000}}})
  self.assertEqual(len(a['items']),64);self.assertTrue(a['partial']);self.assertEqual(len(a['items'][0]['label']),512)
  payload={'method':'item/completed','params':{'item':{'id':'69','type':'commandExecution','exitCode':1}}}
  a=p._project_source_activity(a,payload);again=p._project_source_activity(a,payload)
  self.assertEqual(a,again);self.assertEqual(a['items'][-1]['status'],'error')
  self.assertIsNone(p._project_source_activity(a,{'method':'item/started','params':{'item':{'id':'69','type':'commandExecution'}}}))
  self.assertIsNone(p._project_source_activity(a,{'method':'item/agentMessage/delta','params':{'delta':'not tool state'}}))
 def test_activity_bootstrap_preserves_approval_and_only_uses_confirmed_tool_status(self):
  sid=self.cfg()['session_id'];raw=sid.split(':',1)[1]
  p._capture_external_codex_event(sid,raw,{'id':7,'method':'item/commandExecution/requestApproval','params':{'threadId':raw,'turnId':'boot','itemId':'pending'}})
  p._seed_codex_activity(sid,raw,{'id':'boot','status':'inProgress','items':[{'id':'done','type':'commandExecution','status':'completed','command':'test'},{'id':'unclear','type':'commandExecution'}]})
  page=p.read_native_events({'sessionId':sid,'epoch':''});activity=page['snapshots'][0]['activity']
  self.assertEqual(activity['approvals'][0]['id'],'7');self.assertEqual([x['id'] for x in activity['items']],['done']);self.assertEqual(activity['items'][0]['status'],'completed')
 def test_terminal_checkpoint_can_recover_activity_without_reopening_execution(self):
  sid=self.cfg()['session_id'];raw=sid.split(':',1)[1];cfg={'session_id':sid,'request_id':'external-codex-'+raw+'-finished'}
  p._stream_event(cfg,'completed')
  p._seed_codex_activity(sid,raw,{'id':'finished','status':'completed','items':[{'id':'done','type':'commandExecution','status':'completed','exitCode':0}]})
  snap=p.read_native_events({'sessionId':sid,'epoch':''})['snapshots'][0]
  self.assertEqual(snap['status'],'completed');self.assertEqual(snap['activity']['items'][0]['status'],'completed')
  p._stream_event(cfg,'activity_snapshot',{'activity':{'items':[{'id':'late','status':'running'}],'approvals':[{'id':'stale','status':'pending'}]}})
  snap=p.read_native_events({'sessionId':sid,'epoch':''})['snapshots'][0]
  self.assertEqual(snap['status'],'completed');self.assertEqual(snap['activity']['items'][0]['status'],'unconfirmed');self.assertEqual(snap['activity']['approvals'][0]['status'],'unconfirmed')
 def test_official_request_resolved_clears_only_exact_socket_request_without_claiming_approval(self):
  sid=self.cfg()['session_id'];raw=sid.split(':',1)[1]
  prompt={'id':4,'method':'item/commandExecution/requestApproval','params':{'threadId':raw,'turnId':'approval-turn','itemId':'tool'}}
  p._capture_external_codex_event(sid,raw,prompt,'socket-a')
  done={'method':'serverRequest/resolved','params':{'threadId':raw,'requestId':4}}
  self.assertFalse(p._capture_external_codex_event(sid,raw,done,'socket-b'))
  self.assertFalse(p._capture_external_codex_event(sid,raw,dict(done,params={'threadId':raw,'requestId':'4'}),'socket-a'))
  self.assertTrue(p._capture_external_codex_event(sid,raw,done,'socket-a'))
  self.assertFalse(p._capture_external_codex_event(sid,raw,done,'socket-a'))
  snap=p.read_native_events({'sessionId':sid,'epoch':''})['snapshots'][0]
  self.assertEqual(snap['activity']['approvals'][0]['status'],'cleared');self.assertEqual(snap['status'],'unknown')
 def test_ambiguous_approval_resolution_never_guesses_latest_turn(self):
  sid=self.cfg()['session_id'];raw=sid.split(':',1)[1]
  for turn in ['one','two']:
   p._capture_external_codex_event(sid,raw,{'id':9,'method':'item/commandExecution/requestApproval','params':{'threadId':raw,'turnId':turn}},'same-socket')
  self.assertFalse(p._capture_external_codex_event(sid,raw,{'method':'serverRequest/resolved','params':{'threadId':raw,'requestId':9}},'same-socket'))
 def test_old_turn_reconciliation_pages_summaries_and_preserves_saved_text(self):
  from unittest.mock import Mock
  sid=self.cfg()['session_id'];raw=sid.split(':',1)[1];cfg={'session_id':sid,'request_id':'external-codex-'+raw+'-old'}
  p._stream_event(cfg,'message_snapshot',{'text':'keep previous body'});p._stream_event(cfg,'unknown')
  current=[{'id':'latest','status':'inProgress','items':[]}];pipe=Mock()
  pages=[{'data':[{'id':'latest','status':'inProgress'}],'nextCursor':'next'},{'data':[{'id':'old','status':'completed'}],'nextCursor':None}]
  with patch.object(p,'_control_result',side_effect=pages+[OSError('offline')]):turns=p._older_external_turn_statuses(pipe,sid,raw,current)
  self.assertEqual(turns[-1]['id'],'latest');self.assertEqual(pipe.send.call_count,3)
  self.assertEqual(pipe.send.call_args_list[1].args[0]['params']['cursor'],'next')
  self.assertEqual([c.args[0]['params']['itemsView'] for c in pipe.send.call_args_list],['summary','summary','full'])
  p._restore_external_checkpoints(sid,raw,turns)
  snap=p.read_native_events({'sessionId':sid,'epoch':''})['snapshots'][0]
  self.assertEqual(snap['status'],'completed');self.assertEqual(snap['text'],'keep previous body')
 def test_old_turn_reconciliation_reads_full_matching_page_and_recovers_tools(self):
  from unittest.mock import Mock
  sid=self.cfg()['session_id'];raw=sid.split(':',1)[1];cfg={'session_id':sid,'request_id':'external-codex-'+raw+'-old'}
  p._stream_event(cfg,'unknown');pipe=Mock();current=[{'id':'latest','status':'inProgress','items':[]}]
  turn={'id':'old','status':'completed','items':[{'id':'reply','type':'agentMessage','text':'restored body'},{'id':'tool','type':'commandExecution','status':'completed','exitCode':0}]}
  pages=[{'data':[{'id':'old','status':'completed'}]}, {'data':[turn,{'id':'unrelated','status':'completed','items':[]}]}]
  with patch.object(p,'_control_result',side_effect=pages):turns=p._older_external_turn_statuses(pipe,sid,raw,current)
  self.assertEqual([t['id'] for t in turns],['old','latest']);self.assertEqual(pipe.send.call_count,2)
  p._restore_external_checkpoints(sid,raw,turns)
  snap=p.read_native_events({'sessionId':sid,'epoch':''})['snapshots'][0]
  self.assertEqual(snap['text'],'restored body');self.assertEqual(snap['status'],'completed')
  self.assertEqual(snap['activity']['items'][0]['status'],'completed')
 def test_agy_source_terminal_recovers_exact_request_without_resending(self):
  cfg=dict(self.cfg(),session_id='agy:original',acp_request_id=3,execution_status='unknown',reply_text='')
  with p.closing(p._stream_db()) as db,db:
   db.execute('INSERT INTO requests VALUES(?,?,?,?)',(cfg['request_id'],cfg['session_id'],'hash',json.dumps(cfg)))
  payload={'source':'agy-acp','acp_request_id':3,'method':'session/update','params':{'sessionId':'original','update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'real source text'}}}}
  p._stream_event(cfg,'source_event',payload)
  result={'source':'agy-acp','acp_request_id':3,'method':'session/prompt_result','session_id':cfg['session_id'],'stop_reason':'end_turn'}
  for invalid in [dict(result,acp_request_id=4),dict(result,session_id='agy:other'),dict(result,acp_request_id=True)]:
   p._stream_event(cfg,'source_event',invalid)
   self.assertEqual(p._stream_records(cfg['session_id'])[0]['execution_status'],'unknown')
  p._stream_event(cfg,'source_event',result)
  rec=p._stream_records(cfg['session_id'])[0]
  self.assertEqual(rec['execution_status'],'completed');self.assertEqual(rec['reply_text'],'real source text')
  self.assertEqual(p._stream_records(cfg['session_id'])[0]['execution_status'],'completed')
 def test_summary_terminal_can_later_recover_full_items_without_second_terminal(self):
  from unittest.mock import Mock
  sid=self.cfg()['session_id'];raw=sid.split(':',1)[1];cfg={'session_id':sid,'request_id':'external-codex-'+raw+'-old'}
  p._stream_event(cfg,'unknown')
  p._restore_external_checkpoints(sid,raw,[{'id':'old','status':'completed','_status_only':True}])
  snap=p.read_native_events({'sessionId':sid,'epoch':''})['snapshots'][0]
  self.assertTrue(snap['recovery_pending']);self.assertEqual(snap['status'],'completed')
  full={'id':'old','status':'completed','items':[{'id':'reply','type':'agentMessage','text':'late body'}]}
  with patch.object(p,'_control_result',side_effect=[{'data':[{'id':'old','status':'completed'}]},{'data':[full]}]):turns=p._older_external_turn_statuses(Mock(),sid,raw,[])
  p._restore_external_checkpoints(sid,raw,turns)
  snap=p.read_native_events({'sessionId':sid,'epoch':''})['snapshots'][0]
  self.assertFalse(snap['recovery_pending']);self.assertEqual(snap['text'],'late body')
  with p.closing(p._stream_db()) as db:self.assertEqual(db.execute("SELECT count(*) FROM events WHERE request=? AND kind='completed'",(cfg['request_id'],)).fetchone()[0],1)
 def test_old_turn_reconciliation_bounds_requests_and_keeps_unconfirmed_unknown(self):
  from unittest.mock import Mock
  sid=self.cfg()['session_id'];raw=sid.split(':',1)[1];cfg={'session_id':sid,'request_id':'external-codex-'+raw+'-missing'}
  p._stream_event(cfg,'running');pipe=Mock();current=[{'id':'latest','status':'inProgress'}]
  with patch.object(p,'_control_result',side_effect=[{'data':[],'nextCursor':str(i)} for i in range(3)]):turns=p._older_external_turn_statuses(pipe,sid,raw,current)
  self.assertEqual(pipe.send.call_count,3);self.assertEqual(turns,current)
  p._restore_external_checkpoints(sid,raw,turns)
  self.assertEqual(p.read_native_events({'sessionId':sid,'epoch':''})['snapshots'][0]['status'],'unknown')
 def seed_cancel(self):
  cfg=dict(self.cfg(),work_cwd=self.tmp.name)
  with patch.object(p.subprocess,'Popen') as spawn:
   spawn.return_value.pid=123;p._native_start(cfg,'# worker')
  return cfg,{'sessionId':cfg['session_id'],'requestId':cfg['request_id']}
 def test_cancel_is_pending_idempotent_and_session_scoped(self):
  cfg,args=self.seed_cancel()
  for _ in range(2):self.assertEqual(p.request_native_cancel(args)['status'],'cancel_requested')
  self.assertEqual(p._stream_records(cfg['session_id'])[0]['execution_status'],'accepted')
  self.assertEqual([e['kind'] for e in p.read_native_events({'sessionId':cfg['session_id']})['events']].count('cancel_requested'),1)
  with self.assertRaisesRegex(ValueError,'MISSING'):p.request_native_cancel(dict(args,sessionId='codex:other'))
 def test_cancel_before_dispatch_never_calls_model(self):
  cfg,args=self.seed_cancel();p.request_native_cancel(args)
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='codex'):
   p._native_worker(cfg);pipe.assert_not_called()
  self.assertEqual(p._stream_records(cfg['session_id'])[0]['execution_status'],'cancelled')
 def test_cancel_completed_is_noop_and_unknown_is_not_replayed(self):
  cfg,args=self.seed_cancel();p._stream_update(cfg['request_id'],execution_status='completed')
  self.assertEqual(p.request_native_cancel(args)['status'],'completed')
  p._stream_update(cfg['request_id'],execution_status='unknown')
  with self.assertRaisesRegex(ValueError,'UNCONFIRMED'):p.request_native_cancel(args)
 def test_codex_server_request_ids_do_not_impersonate_control_replies(self):
  cfg,args=self.seed_cancel();raw=cfg['session_id'].split(':')[1]
  request=lambda n:{'id':n,'method':'item/commandExecution/requestApproval','params':{'threadId':raw,'turnId':'t'}}
  frames=[request(1),{'id':1,'result':{}},request(2),{'id':2,'result':{'thread':{'id':raw}}},request(3),{'id':3,'result':{'turn':{'id':'t'}}},{'method':'turn/completed','params':{'threadId':raw,'turn':{'id':'t','status':'completed','items':[{'type':'agentMessage','id':'m','text':'done'}]}}}]
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='codex'):
   pipe.return_value.receive.side_effect=frames;p._native_worker(cfg)
   sent=[c.args[0] for c in pipe.return_value.send.call_args_list]
  row=p._stream_records(cfg['session_id'])[0]
  self.assertEqual(row['execution_status'],'completed');self.assertEqual(row['reply_text'],'done')
  self.assertEqual(len([f for f in sent if f.get('method')=='turn/start']),1)
  self.assertEqual([f['id'] for f in sent if 'error' in f],[1,2,3])
 def test_pi_cancel_during_identity_does_not_dispatch_prompt(self):
  cfg=dict(self.cfg(),session_id='pi:12345678-1234-1234-1234-123456789abc',work_cwd=self.tmp.name,jsonl_path='session.jsonl')
  with patch.object(p.subprocess,'Popen') as spawn:
   spawn.return_value.pid=123;p._native_start(cfg,'# worker')
  def receive(**kw):
   p.request_native_cancel({'sessionId':cfg['session_id'],'requestId':cfg['request_id']})
   return {'id':'identity','success':True,'data':{'sessionId':cfg['session_id'].split(':')[1]}}
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='pi'):
   pipe.return_value.receive.side_effect=receive;p._native_worker(cfg)
   sent=[c.args[0] for c in pipe.return_value.send.call_args_list]
  self.assertFalse(any(f.get('type')=='prompt' for f in sent))
  row=p._stream_records(cfg['session_id'])[0]
  self.assertEqual(row['execution_status'],'cancelled');self.assertEqual(row['cancel_confirmed_by'],'not_dispatched')
 def test_codex_interrupt_ack_alone_does_not_confirm_stop(self):
  cfg,args=self.seed_cancel();raw=cfg['session_id'].split(':')[1]
  frames=[{'id':1,'result':{}},{'id':2,'result':{'thread':{'id':raw}}},{'id':3,'result':{'turn':{'id':'t'}}},{'id':4,'result':{}}]
  def receive(**kw):
   if frames:
    f=frames.pop(0)
    if f.get('id')==3:p.request_native_cancel(args)
    return f
   raise ValueError('HARNESS_PROCESS_EXITED')
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='codex'):
   pipe.return_value.receive.side_effect=receive;p._native_worker(cfg)
   sent=[c.args[0] for c in pipe.return_value.send.call_args_list]
  self.assertEqual(len([f for f in sent if f.get('method')=='turn/interrupt']),1)
  self.assertEqual(p._stream_records(cfg['session_id'])[0]['execution_status'],'unknown')
 def test_codex_matching_interrupted_turn_confirms_cancel(self):
  cfg,args=self.seed_cancel();raw=cfg['session_id'].split(':')[1]
  frames=[{'id':1,'result':{}},{'id':2,'result':{'thread':{'id':raw}}},{'id':3,'result':{'turn':{'id':'t'}}},{'method':'turn/completed','params':{'threadId':raw,'turn':{'id':'t','status':'interrupted'}}}]
  def receive(**kw):
   f=frames.pop(0)
   if f.get('id')==3:p.request_native_cancel(args)
   return f
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='codex'):
   pipe.return_value.receive.side_effect=receive;p._native_worker(cfg)
  self.assertEqual(p._stream_records(cfg['session_id'])[0]['execution_status'],'cancelled')
 def test_confirmed_long_turn_outlives_startup_deadline(self):
  cfg=dict(self.cfg(),work_cwd=self.tmp.name);raw=cfg['session_id'].split(':')[1];clock=[0]
  with patch.object(p.subprocess,'Popen') as spawn:
   spawn.return_value.pid=123;p._native_start(cfg,'# worker')
  frames=iter([{'id':1,'result':{}},{'id':2,'result':{'thread':{'id':raw}}},{'id':3,'result':{'turn':{'id':'long-turn'}}},None,{'method':'turn/completed','params':{'threadId':raw,'turn':{'id':'long-turn','status':'completed','items':[{'type':'agentMessage','id':'reply','text':'late result'}]}}}])
  def receive(**kwargs):
   frame=next(frames)
   if frame is None:clock[0]=3600
   return frame
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='codex'),patch.object(p.time,'monotonic',side_effect=lambda:clock[0]):
   pipe.return_value.receive.side_effect=receive;p._native_worker(cfg)
   self.assertEqual(sum(c.args[0].get('method')=='turn/start' for c in pipe.return_value.send.call_args_list),1)
  rec=p._stream_records(cfg['session_id'])[0]
  self.assertEqual(rec['execution_status'],'completed');self.assertEqual(rec['reply_text'],'late result')
 def test_startup_without_turn_ack_still_times_out_unknown(self):
  cfg=dict(self.cfg(),work_cwd=self.tmp.name);raw=cfg['session_id'].split(':')[1];clock=[0]
  with patch.object(p.subprocess,'Popen') as spawn:
   spawn.return_value.pid=123;p._native_start(cfg,'# worker')
  frames=iter([{'id':1,'result':{}},{'id':2,'result':{'thread':{'id':raw}}},None])
  def receive(**kwargs):
   frame=next(frames)
   if frame is None:clock[0]=181
   return frame
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='codex'),patch.object(p.time,'monotonic',side_effect=lambda:clock[0]):
   pipe.return_value.receive.side_effect=receive;p._native_worker(cfg)
  rec=p._stream_records(cfg['session_id'])[0]
  self.assertEqual(rec['execution_status'],'unknown');self.assertEqual(rec['error_code'],'HARNESS_TIMEOUT')
 def test_drained_queue_detects_reader_exit_without_sentinel(self):
  pipe=p.NativePipe.__new__(p.NativePipe);pipe.q=p.queue.Queue();pipe.reader_done=p.threading.Event()
  self.assertIsNone(pipe.receive(timeout=.001))
  pipe.q.put({'type':'last'});pipe.reader_done.set()
  self.assertEqual(pipe.receive(timeout=.001),{'type':'last'})
  with self.assertRaisesRegex(ValueError,'HARNESS_PROCESS_EXITED'):pipe.receive(timeout=.001)
 def test_receipt_reads_only_exact_turn_items_after_summary_pages(self):
  with patch.object(p,'NativePipe') as factory:
   pipe=factory.return_value
   pages=[{'data':[{'id':'other','status':'completed'}],'nextCursor':'older'}, {'data':[{'id':'wanted','status':'completed'}]}, {'data':[{'turnId':'wanted','item':{'type':'agentMessage','id':'a','text':'one'}}],'nextCursor':'item-next'}, {'data':[{'turnId':'wanted','item':{'type':'agentMessage','id':'b','text':'two'}}]}]
   with patch.object(p,'_control_result',side_effect=pages):turn=p._read_codex_receipt_turn(pipe,'thread','wanted',None)
   self.assertEqual(turn['id'],'wanted');self.assertEqual(len(turn['items']),2)
   sent=[c.args[0] for c in pipe.send.call_args_list]
   self.assertEqual([f['method'] for f in sent],['thread/turns/list']*2+['thread/items/list']*2)
   self.assertTrue(all(f['params'].get('itemsView')=='summary' for f in sent[:2]))
   self.assertTrue(all(f['params'].get('turnId')=='wanted' for f in sent[2:]))
 def test_receipt_item_from_wrong_turn_cannot_settle_request(self):
  pages=[{'data':[{'id':'wanted','status':'completed'}]},{'data':[{'turnId':'neighbour','item':{'type':'agentMessage','id':'x','text':'wrong answer'}}]}]
  with patch.object(p,'NativePipe') as factory,patch.object(p,'_control_result',side_effect=pages):
   self.assertIsNone(p._read_codex_receipt_turn(factory.return_value,'thread','wanted',None))
 def test_receipt_exhaustion_and_repeated_cursor_do_not_guess(self):
  for pages in ([{'data':[{'id':'other'}],'nextCursor':str(i)} for i in range(3)], [{'data':[],'nextCursor':'loop'}]*2):
   with patch.object(p,'NativePipe') as factory,patch.object(p,'_control_result',side_effect=pages):
    self.assertIsNone(p._read_codex_receipt_turn(factory.return_value,'thread','missing',None))
    self.assertLessEqual(factory.return_value.send.call_count,3)
 def test_receipt_incomplete_or_oversized_items_remain_unconfirmed(self):
  for batches in ([{'data':[],'nextCursor':str(i)} for i in range(16)], [{'data':[{'text':'x'*1048577}]}]):
   pages=[{'data':[{'id':'wanted','status':'completed'}]}]+batches
   with patch.object(p,'NativePipe') as factory,patch.object(p,'_control_result',side_effect=pages):
    self.assertIsNone(p._read_codex_receipt_turn(factory.return_value,'thread','wanted',None))
 def test_duplicate_never_launches_again_and_conflict_rejected(self):
  cfg=self.cfg()
  with patch.object(p.subprocess,'Popen') as spawn:
   spawn.return_value.pid=123
   p._native_start(cfg,'# worker')
   p._native_start(cfg,'# worker')
   self.assertEqual(spawn.call_count,1)
   with self.assertRaisesRegex(ValueError,'CONFLICT'):p._native_start(dict(cfg,text='changed'),'# worker')
 def test_busy_session_and_ordered_cursor(self):
  cfg=self.cfg()
  with patch.object(p.subprocess,'Popen') as spawn:
   spawn.return_value.pid=123;p._native_start(cfg,'# worker')
   with self.assertRaisesRegex(ValueError,'BUSY'):p._native_start(dict(cfg,request_id='other'),'# worker')
  p._stream_event(cfg,'text_delta',{'text':'hello'})
  a=p.read_native_events({'sessionId':cfg['session_id'],'after':0})
  self.assertEqual([e['kind'] for e in a['events']],['accepted','text_delta'])
  self.assertEqual(p.read_native_events({'sessionId':cfg['session_id'],'after':a['cursor']})['events'],[])
  self.assertEqual(p.read_native_events({'sessionId':'codex:other','after':0})['events'],[])
 def test_missing_session_does_not_start_fresh(self):
  with patch.object(p.subprocess,'Popen') as spawn:
   with self.assertRaisesRegex(ValueError,'NOT_FOUND'):p.prompt_session({'sessionId':self.cfg()['session_id'],'requestId':'x','text':'test'})
   spawn.assert_not_called()
 def test_resume_rejection_has_no_second_process(self):
  cfg=dict(self.cfg(),work_cwd=self.tmp.name)
  with patch.object(p.subprocess,'Popen') as spawn:
   spawn.return_value.pid=123;p._native_start(cfg,'# worker')
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='codex'):
   pipe.return_value.receive.side_effect=[{'id':1,'result':{}},{'id':2,'error':{'code':-1}}]
   p._native_worker(cfg)
   self.assertEqual(pipe.call_count,1)
  rec=p._stream_records(cfg['session_id'])[0]
  self.assertEqual(rec['execution_status'],'error');self.assertEqual(rec['error_code'],'HARNESS_RESUME_REJECTED')


 def test_lost_worker_is_unknown_not_completed(self):
  cfg=self.cfg()
  with patch.object(p.subprocess,'Popen') as spawn:
   spawn.return_value.pid=123;p._native_start(cfg,'# worker')
  p._stream_update(cfg['request_id'],started_at=1)
  with patch.object(p.os,'kill',side_effect=ProcessLookupError()):
   rec=p._stream_records(cfg['session_id'])[0]
  self.assertEqual(rec['execution_status'],'unknown')
  with patch.object(p.subprocess,'Popen') as spawn:
   with self.assertRaisesRegex(ValueError,'BUSY'):p._native_start(dict(cfg,request_id='second'),'# worker')
   spawn.assert_not_called()
 def test_repeated_delivery_after_real_worker_exit_is_unknown_without_respawn(self):
  cfg=self.cfg();children=[];real_spawn=p.subprocess.Popen
  def spawn(*args,**kwargs):
   child=real_spawn(*args,**kwargs);children.append(child);return child
  with patch.object(p.subprocess,'Popen',side_effect=spawn) as launcher:
   p._native_start(cfg,'# Dedicated no-op worker: exit without terminal receipt.\n')
   self.assertEqual(children[0].wait(timeout=10),0)
   p._stream_update(cfg['request_id'],started_at=1,reply_text='preserved')
   repeated=p._native_start(cfg,'# Dedicated no-op worker: exit without terminal receipt.\n')
   self.assertEqual(repeated['status'],'unknown')
   self.assertEqual(repeated['execution_status'],'unknown')
   self.assertEqual(launcher.call_count,1)
   with self.assertRaisesRegex(ValueError,'BUSY'):p._native_start(dict(cfg,request_id='second'),'# worker')
   self.assertEqual(launcher.call_count,1)
  record=p._stream_records(cfg['session_id'])[0]
  self.assertEqual(record['reply_text'],'preserved')
  self.assertEqual(record['error_code'],'HARNESS_PROCESS_LOST')

 def test_repeated_delivery_returns_durable_terminal_state(self):
  cfg=self.cfg()
  with patch.object(p.subprocess,'Popen') as spawn:
   spawn.return_value.pid=123;p._native_start(cfg,'# worker')
   p._stream_update(cfg['request_id'],execution_status='cancelled',reply_text='partial')
   repeated=p._native_start(cfg,'# worker')
   self.assertEqual(repeated['execution_status'],'cancelled')
   self.assertEqual(spawn.call_count,1)

 def agy_cfg(self):
  cfg=dict(self.cfg(),session_id='agy:12345678-1234-1234-1234-123456789abc',work_cwd=self.tmp.name)
  with patch.object(p.subprocess,'Popen') as spawn:
   spawn.return_value.pid=123;p._native_start(cfg,'# worker')
  return cfg
 def acp_update(self,raw,text):
  return {'method':'session/update','params':{'sessionId':raw,'update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':text}}}}
 def acp_init(self,raw):
  return [{'id':1,'result':{'agentCapabilities':{'loadSession':True}}},self.acp_update(raw,'OLD HISTORY'),{'id':2,'result':{'sessionId':raw}}]
 def test_agy_post_terminal_transport_error_does_not_erase_completion(self):
  cfg=self.agy_cfg()
  with p.closing(p._stream_db()) as db:token=db.execute('SELECT token FROM agy_supervisors').fetchone()[0]
  def prompt(c):
   p._stream_update(c['request_id'],execution_status='completed',reply_text='confirmed')
   raise OSError('event write failed after terminal receipt')
  with patch.object(p,'AgyAcpSession') as session,patch.object(p.shutil,'which',return_value='agy'):
   session.return_value.prompt.side_effect=prompt
   p._agy_supervise({'session_id':cfg['session_id'],'supervisor_token':token})
  self.assertEqual(p._stream_records(cfg['session_id'])[0]['execution_status'],'completed')

 def test_agy_launcher_reuses_live_owner_across_requests(self):
  cfg=self.agy_cfg()
  p._stream_update(cfg['request_id'],execution_status='completed')
  with patch.object(p,'_agy_process_owned',return_value=True),patch.object(p.subprocess,'Popen') as spawn:
   second=dict(cfg,request_id='second')
   p._native_start(second,'# worker');spawn.assert_not_called()
  records=p._stream_records(cfg['session_id'])
  self.assertEqual(records[0]['pid'],records[1]['pid'])
  self.assertEqual(records[0]['supervisor_token'],records[1]['supervisor_token'])
 def test_agy_new_source_cannot_compete_with_live_supervisor(self):
  cfg=self.agy_cfg();p._stream_update(cfg['request_id'],execution_status='completed')
  with patch.object(p,'_agy_process_owned',return_value=True),patch.object(p.subprocess,'Popen') as spawn:
   self.assertEqual(p._native_start(dict(cfg,request_id='new'),'# changed')['status'],'not_accepted')
   self.assertEqual(p._native_start(dict(cfg,request_id='new'),'# changed')['status'],'not_accepted')
   spawn.assert_not_called()
  receipt=p.inspect_receipt({'sessionId':cfg['session_id'],'requestId':'new','text':cfg['text']})
  self.assertEqual(receipt['status'],'not_accepted')
  self.assertEqual(len(p._stream_records(cfg['session_id'])),2)
 def test_agy_spawn_failure_rolls_back_request(self):
  cfg=dict(self.cfg(),session_id='agy:12345678-1234-1234-1234-123456789abc',work_cwd=self.tmp.name)
  with patch.object(p.subprocess,'Popen',side_effect=OSError('failed')):
   with self.assertRaises(OSError):p._native_start(cfg,'# worker')
  self.assertEqual(p._stream_records(cfg['session_id']),[])
 def test_agy_dead_owner_keeps_unknown_barrier(self):
  cfg=self.agy_cfg();p._stream_update(cfg['request_id'],started_at=1)
  with patch.object(p.os,'kill'),patch.object(p,'_agy_process_owned',return_value=False),patch.object(p.subprocess,'Popen') as spawn:
   with self.assertRaisesRegex(ValueError,'BUSY'):p._native_start(dict(cfg,request_id='new'),'# worker')
   spawn.assert_not_called()
  self.assertEqual(p._stream_records(cfg['session_id'])[0]['execution_status'],'unknown')
 def test_agy_supervisor_consumes_queue_and_closes_only_after_idle(self):
  cfg=self.agy_cfg()
  with p.closing(p._stream_db()) as db:
   token=db.execute('SELECT token FROM agy_supervisors').fetchone()[0]
  seen=[]
  def prompt(c):
   seen.append(c['request_id']);p._stream_update(c['request_id'],execution_status='completed')
  with patch.object(p,'AgyAcpSession') as session,patch.object(p.shutil,'which',return_value='agy'),patch.object(p.time,'monotonic',side_effect=[0,0,301]):
   session.return_value.prompt.side_effect=prompt
   p._agy_supervise({'session_id':cfg['session_id'],'supervisor_token':token})
   session.return_value.close.assert_called_once()
  self.assertEqual(seen,[cfg['request_id']])
  with p.closing(p._stream_db()) as db:
   self.assertEqual(db.execute('SELECT COUNT(*) FROM agy_queue').fetchone()[0],0)
   self.assertEqual(db.execute('SELECT COUNT(*) FROM agy_supervisors').fetchone()[0],0)

 def test_agy_native_stream_excludes_load_replay_and_wrong_session(self):
  cfg=self.agy_cfg();raw=cfg['session_id'].split(':')[1]
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='agy'):
   pipe.return_value.receive.side_effect=self.acp_init(raw)+[self.acp_update('other','WRONG'),self.acp_update(raw,'hello'),{'id':3,'result':{'stopReason':'end_turn'}}]
   p._native_worker(cfg)
   self.assertEqual(pipe.call_args.args[0],['agy','acp','--stdio'])
  rec=p._stream_records(cfg['session_id'])[0]
  self.assertEqual(rec['execution_status'],'completed');self.assertEqual(rec['reply_text'],'hello')
  events=p.read_native_events({'sessionId':cfg['session_id']})['events']
  self.assertEqual([e['kind'] for e in events],['accepted','running','source_event','text_delta','source_event','completed'])
 def test_agy_tool_updates_reach_durable_snapshot_without_replay(self):
  cfg=self.agy_cfg();raw=cfg['session_id'].split(':')[1]
  def tool(sid,event_kind,**kw):return {'method':'session/update','params':{'sessionId':sid,'update':{'sessionUpdate':event_kind,'toolCallId':'call1',**kw}}}
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='agy'):
   pipe.return_value.receive.side_effect=self.acp_init(raw)+[tool('other','tool_call',title='WRONG'),tool(raw,'tool_call',title='Read source',kind='read',status='in_progress'),tool(raw,'tool_call_update',status='completed'),self.acp_update(raw,'done'),{'id':3,'result':{'stopReason':'end_turn'}}]
   p._native_worker(cfg)
  with p.closing(p._stream_db()) as db:
   snapshot=json.loads(db.execute('SELECT body FROM event_snapshots WHERE request=?',(cfg['request_id'],)).fetchone()[0])
  self.assertEqual(snapshot['activity']['items'],[{'id':'call1','label':'Read source','type':'read','status':'completed'}])
  self.assertEqual(snapshot['status'],'completed')
 def test_agy_tool_patch_is_bounded_and_does_not_reopen_terminal(self):
  def frame(i,**kw):return {'method':'session/update','params':{'update':{'sessionUpdate':'tool_call_update','toolCallId':str(i),**kw}}}
  activity=None
  for i in range(70):activity=p._project_agy_activity(activity,frame(i,title='x'*1000,status='completed'))
  self.assertEqual(len(activity['items']),64);self.assertTrue(activity['partial'])
  activity=p._project_agy_activity(activity,frame(69,status='in_progress'))
  self.assertEqual(activity['items'][-1]['status'],'completed');self.assertEqual(len(activity['items'][-1]['label']),512)
  self.assertIsNone(p._project_agy_activity(activity,frame('x'*129)))
 def test_agy_lost_prompt_is_unknown_and_never_replayed(self):
  cfg=self.agy_cfg();raw=cfg['session_id'].split(':')[1]
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='agy'):
   pipe.return_value.receive.side_effect=self.acp_init(raw)+[self.acp_update(raw,'partial'),ValueError('HARNESS_PROCESS_EXITED')]
   p._native_worker(cfg)
   sent=[c.args[0] for c in pipe.return_value.send.call_args_list]
   self.assertEqual(sum(f.get('method')=='session/prompt' for f in sent),1)
  rec=p._stream_records(cfg['session_id'])[0]
  self.assertEqual(rec['execution_status'],'unknown');self.assertEqual(rec['reply_text'],'partial')
  with patch.object(p.subprocess,'Popen') as spawn:
   self.assertEqual(p._native_start(cfg,'# worker')['status'],'unknown');spawn.assert_not_called()
   with self.assertRaisesRegex(ValueError,'BUSY'):p._native_start(dict(cfg,request_id='next'),'# worker')
 def test_agy_cancel_requires_prompt_terminal_and_denies_permission(self):
  cfg=self.agy_cfg();raw=cfg['session_id'].split(':')[1]
  permission={'id':3,'method':'session/request_permission','params':{'sessionId':raw,'options':[{'optionId':'yes','kind':'allow_always'}]}}
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='agy'):
   frames=iter(self.acp_init(raw)+[permission,self.acp_update(raw,'partial'),{'id':3,'result':{'stopReason':'cancelled'}}])
   def receive(timeout):
    frame=next(frames)
    if frame is permission:p.request_native_cancel({'sessionId':cfg['session_id'],'requestId':cfg['request_id']})
    return frame
   pipe.return_value.receive.side_effect=receive
   p._native_worker(cfg)
   sent=[c.args[0] for c in pipe.return_value.send.call_args_list]
   self.assertEqual(sum(f.get('method')=='session/cancel' for f in sent),1)
   self.assertIn({'jsonrpc':'2.0','id':3,'result':{'outcome':{'outcome':'cancelled'}}},sent)
  rec=p._stream_records(cfg['session_id'])[0]
  self.assertEqual(rec['execution_status'],'cancelled');self.assertEqual(rec['cancel_confirmed_by'],'acp_stop_reason')
  with p.closing(p._stream_db()) as db:state=json.loads(db.execute('SELECT body FROM event_snapshots WHERE request=?',(cfg['request_id'],)).fetchone()[0])
  self.assertEqual(state['activity']['approvals'][0]['status'],'denied')
 def test_agy_permission_decision_is_durable_and_send_failure_unconfirmed(self):
  cfg=self.agy_cfg();raw=cfg['session_id'].split(':')[1]
  frame={'method':'session/request_permission','id':3,'params':{'sessionId':raw,'toolCall':{'toolCallId':'permission-tool'}}}
  p._capture_agy_event(cfg,frame,3)
  p._capture_agy_event(cfg,frame,3,decision='denied')
  p._stream_event(cfg,'unknown')
  with p.closing(p._stream_db()) as db:state=json.loads(db.execute('SELECT body FROM event_snapshots WHERE request=?',(cfg['request_id'],)).fetchone()[0])
  self.assertEqual(state['activity']['approvals'][0]['status'],'denied')
  self.assertIsNone(p._project_agy_activity(state['activity'],frame))
  cfg2=dict(cfg,request_id='permission-unconfirmed')
  p._capture_agy_event(cfg2,frame,4);p._stream_event(cfg2,'unknown')
  with p.closing(p._stream_db()) as db:state=json.loads(db.execute('SELECT body FROM event_snapshots WHERE request=?',(cfg2['request_id'],)).fetchone()[0])
  self.assertEqual(state['activity']['approvals'][0]['status'],'unconfirmed')
  typed=p._project_agy_activity(state['activity'],dict(frame,id='3'))
  self.assertEqual(len(typed['approvals']),2)
 def test_agy_permission_transport_failure_never_claims_local_reply_sent(self):
  cfg=self.agy_cfg();raw=cfg['session_id'].split(':')[1]
  permission={'id':3,'method':'session/request_permission','params':{'sessionId':raw,'toolCall':{'toolCallId':'tool1'}}}
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='agy'):
   pipe.return_value.receive.side_effect=self.acp_init(raw)+[permission]
   def send(frame):
    if frame.get('result',{}).get('outcome'):raise ValueError('HARNESS_TRANSPORT_FAILED')
   pipe.return_value.send.side_effect=send
   p._native_worker(cfg)
  self.assertEqual(p._stream_records(cfg['session_id'])[0]['execution_status'],'unknown')
  with p.closing(p._stream_db()) as db:
   state=json.loads(db.execute('SELECT body FROM event_snapshots WHERE request=?',(cfg['request_id'],)).fetchone()[0])
   decisions=db.execute("SELECT COUNT(*) FROM events WHERE request=? AND kind='permission_decision'",(cfg['request_id'],)).fetchone()[0]
  self.assertEqual(decisions,0);self.assertEqual(state['activity']['approvals'][0]['status'],'unconfirmed')
 def permission_fixture(self):
  cfg=self.agy_cfg();p._stream_update(cfg['request_id'],execution_status='running')
  frame={'id':7,'method':'session/request_permission','params':{'sessionId':cfg['session_id'].split(':')[1],'toolCall':{'title':'Read fixture'},'options':[{'optionId':'once','kind':'allow_once'},{'optionId':'always','kind':'allow_always'}]}}
  self.assertTrue(p._open_native_permission(cfg,frame,3))
  with p.closing(p._stream_db()) as db:entry=json.loads(db.execute('SELECT body FROM native_permissions').fetchone()[0])
  return cfg,frame,entry,{'sessionId':cfg['session_id'],'requestId':cfg['request_id'],'permissionId':entry['id'],'optionId':'once'}
 def test_permission_queue_exact_scope_and_single_write(self):
  cfg,frame,entry,action=self.permission_fixture()
  self.assertEqual([x['id'] for x in entry['options']],['once'])
  with patch.object(p,'_agy_process_owned',return_value=True):
   with self.assertRaisesRegex(ValueError,'NOT_FOUND'):p.submit_native_permission(dict(action,requestId='another'))
   with self.assertRaisesRegex(ValueError,'OPTION_INVALID'):p.submit_native_permission(dict(action,optionId='always'))
   self.assertEqual(p.submit_native_permission(action)['status'],'queued')
   self.assertEqual(p.submit_native_permission(action)['status'],'queued')
   with self.assertRaisesRegex(ValueError,'CONFLICT'):p.submit_native_permission(dict(action,optionId=None))
  with patch.object(p,'NativePipe') as pipe:
   p._flush_native_permissions(cfg,pipe);p._flush_native_permissions(cfg,pipe)
   pipe.send.assert_called_once_with({'jsonrpc':'2.0','id':7,'result':{'outcome':{'outcome':'selected','optionId':'once'}}})
 def test_permission_lost_owner_and_terminal_request_reject_choice(self):
  cfg,frame,entry,action=self.permission_fixture()
  with patch.object(p,'_agy_process_owned',return_value=False):
   with self.assertRaisesRegex(ValueError,'EXPIRED'):p.submit_native_permission(action)
  p._stream_update(cfg['request_id'],execution_status='completed')
  with patch.object(p,'_agy_process_owned',return_value=True):
   with self.assertRaisesRegex(ValueError,'EXPIRED'):p.submit_native_permission(action)
 def test_permission_write_failure_is_not_replayed(self):
  cfg,frame,entry,action=self.permission_fixture()
  with patch.object(p,'_agy_process_owned',return_value=True):p.submit_native_permission(action)
  with patch.object(p,'NativePipe') as pipe:
   pipe.send.side_effect=ValueError('HARNESS_TRANSPORT_FAILED')
   with self.assertRaisesRegex(ValueError,'TRANSPORT_FAILED'):p._flush_native_permissions(cfg,pipe)
   p._flush_native_permissions(cfg,pipe);self.assertEqual(pipe.send.call_count,1)
  with p.closing(p._stream_db()) as db:entry=json.loads(db.execute('SELECT body FROM native_permissions').fetchone()[0])
  self.assertEqual(entry['status'],'dispatching')
 def test_permission_cancel_or_expiry_wins_before_dispatch_claim(self):
  for cause in ('cancel','expired'):
   with self.subTest(cause=cause):
    cfg,frame,entry,action=self.permission_fixture()
    with patch.object(p,'_agy_process_owned',return_value=True):p.submit_native_permission(action)
    if cause=='cancel':p._stream_update(cfg['request_id'],cancel_requested_at=p._now())
    else:
     with p.closing(p._stream_db()) as db,db:
      body=json.loads(db.execute('SELECT body FROM native_permissions WHERE id=?',(entry['id'],)).fetchone()[0]);body['expires_at']=0
      db.execute('UPDATE native_permissions SET body=? WHERE id=?',(json.dumps(body),entry['id']))
    with patch.object(p,'NativePipe') as pipe:
     p._flush_native_permissions(cfg,pipe)
     self.assertEqual(pipe.send.call_args.args[0]['result'],{'outcome':{'outcome':'cancelled'}})
    with p.closing(p._stream_db()) as db,db:
     db.execute('DELETE FROM native_permissions');db.execute('DELETE FROM requests');db.execute('DELETE FROM agy_supervisors');db.execute('DELETE FROM agy_queue')
 def test_permission_capacity_prunes_only_expired_inactive_records(self):
  cfg,frame,entry,action=self.permission_fixture()
  with p.closing(p._stream_db()) as db,db:
   for i in range(1023):db.execute('INSERT INTO native_permissions VALUES(?,?,?,?)',('old'+str(i),'gone',cfg['session_id'],json.dumps({'expires_at':0})))
   self.assertTrue(p._permission_capacity(db))
   self.assertEqual(db.execute('SELECT COUNT(*) FROM native_permissions').fetchone()[0],1)
 def test_agy_authoritative_end_turn_without_text_is_terminal(self):
  cfg=self.agy_cfg();raw=cfg['session_id'].split(':')[1]
  permission={'id':'source','method':'session/request_permission','params':{'sessionId':raw,'options':[]}}
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='agy'):
   pipe.return_value.receive.side_effect=self.acp_init(raw)+[permission,{'id':3,'result':{'stopReason':'end_turn'}}]
   p._native_worker(cfg)
  self.assertEqual(p._stream_records(cfg['session_id'])[0]['execution_status'],'completed')
  events=p.read_native_events({'sessionId':cfg['session_id']})['events']
  self.assertTrue(any(e['payload'].get('stop_reason')=='end_turn' for e in events))
 def test_agy_connection_reuses_load_for_two_serial_prompts(self):
  cfg=self.agy_cfg();raw=cfg['session_id'].split(':')[1]
  with patch.object(p,'NativePipe') as pipe:
   pipe.return_value.receive.side_effect=self.acp_init(raw)+[self.acp_update(raw,'one'),{'id':3,'result':{'stopReason':'end_turn'}},self.acp_update(raw,'two'),{'id':4,'result':{'stopReason':'end_turn'}}]
   session=p.AgyAcpSession('agy',raw,self.tmp.name,{})
   session.prompt(cfg)
   second=dict(cfg,request_id='second')
   with patch.object(p.subprocess,'Popen') as spawn:
    spawn.return_value.pid=123;p._native_start(second,'# worker')
   session.prompt(second);session.close()
   sent=[c.args[0] for c in pipe.return_value.send.call_args_list]
   self.assertEqual(sum(f.get('method')=='session/load' for f in sent),1)
   self.assertEqual(sum(f.get('method')=='session/prompt' for f in sent),2)
  self.assertEqual([r['reply_text'] for r in p._stream_records(cfg['session_id'])],['one','two'])

 def test_pi_length_limit_is_not_success(self):
  cfg=dict(self.cfg(),session_id='pi:12345678-1234-1234-1234-123456789abc',work_cwd=self.tmp.name,jsonl_path='existing.jsonl')
  with patch.object(p.subprocess,'Popen') as spawn:
   spawn.return_value.pid=123;p._native_start(cfg,'# worker')
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='pi'):
   pipe.return_value.receive.side_effect=[{'id':'identity','success':True,'data':{'sessionId':cfg['session_id'].split(':')[1]}},{'type':'message_end','message':{'role':'assistant','stopReason':'length','content':[]}}]
   p._native_worker(cfg)
  rec=p._stream_records(cfg['session_id'])[0]
  self.assertEqual(rec['execution_status'],'error');self.assertEqual(rec['error_code'],'HARNESS_OUTPUT_TRUNCATED')


 def test_pi_user_message_never_becomes_assistant_snapshot(self):
  cfg=dict(self.cfg(),session_id='pi:12345678-1234-1234-1234-123456789abc',work_cwd=self.tmp.name,jsonl_path='existing.jsonl')
  with patch.object(p.subprocess,'Popen') as spawn:
   spawn.return_value.pid=123;p._native_start(cfg,'# worker')
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='pi'):
   pipe.return_value.receive.side_effect=[{'id':'identity','success':True,'data':{'sessionId':cfg['session_id'].split(':')[1]}},{'type':'message_end','message':{'role':'user','content':[{'type':'text','text':'request'}]}},{'type':'message_update','assistantMessageEvent':{'type':'text_delta','delta':'reply'}},{'type':'message_end','message':{'role':'assistant','stopReason':'stop','content':[{'type':'text','text':'reply'}]}},{'type':'agent_settled'}]
   p._native_worker(cfg)
  rec=p._stream_records(cfg['session_id'])[0]
  self.assertEqual(rec['reply_text'],'reply');self.assertEqual(rec['execution_status'],'completed')
  self.assertNotIn('message_snapshot',[e['kind'] for e in p.read_native_events({'sessionId':cfg['session_id']})['events']])


 def run_codex(self, frames, start_result=None):
  cfg=dict(self.cfg(),work_cwd=self.tmp.name)
  raw=cfg['session_id'].split(':')[1]
  with patch.object(p.subprocess,'Popen') as spawn:
   spawn.return_value.pid=123;p._native_start(cfg,'# worker')
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='codex'):
   initial=[{'id':1,'result':{}},{'id':2,'result':{'thread':{'id':raw}}}]
   if start_result is not None:initial.append({'id':3,'result':start_result})
   pipe.return_value.receive.side_effect=initial+frames
   p._native_worker(cfg)
   sent=[call.args[0] for call in pipe.return_value.send.call_args_list]
   self.codex_sent=sent
   self.assertEqual(sum(f.get('method')=='turn/start' for f in sent),1)
   self.assertEqual(pipe.call_count,1)
  return p._stream_records(cfg['session_id'])[0],p.read_native_events({'sessionId':cfg['session_id']})['events']

 def codex_frame(self,method,**params):
  return {'method':method,'params':{'threadId':self.cfg()['session_id'].split(':')[1],'turnId':'turn-new',**params}}

 def codex_end(self,**turn):
  return self.codex_frame('turn/completed',turn={'id':'turn-new','status':'completed',**turn})

 def codex_item(self,text,item_id='answer',**params):
  return self.codex_frame('item/completed',item={'id':item_id,'type':'agentMessage','text':text},**params)

 def test_codex_final_item_recovers_missing_stream_without_duplicate(self):
  frames=[self.codex_frame('item/agentMessage/delta',itemId='answer',delta='hel'),
   self.codex_item('hello'),self.codex_item('hello'),
   self.codex_frame('item/agentMessage/delta',itemId='answer',delta='late'),self.codex_end()]
  rec,events=self.run_codex(frames,{'turn':{'id':'turn-new'}})
  self.assertEqual(rec['execution_status'],'completed');self.assertEqual(rec['reply_text'],'hello')
  self.assertEqual(rec['native_turn_id'],'turn-new')
  self.assertEqual([e['payload']['text'] for e in events if e['kind']=='text_delta'],['hel','lo'])

 def test_codex_final_only_and_multiple_items_keep_all_text(self):
  rec,events=self.run_codex([self.codex_item('Checking','commentary'),self.codex_item('Finished'),self.codex_end()],{'turn':{'id':'turn-new'}})
  self.assertEqual(rec['execution_status'],'completed');self.assertEqual(rec['reply_text'],'Checking\n\nFinished')

 def test_codex_rejects_foreign_turn_thread_and_missing_identity_frames(self):
  frames=[self.codex_item('wrong-thread',threadId='foreign'),self.codex_item('old',turnId='turn-old'),
   self.codex_item('anonymous',threadId=None),self.codex_item('no-turn',turnId=None),
   self.codex_end(id='turn-old'),self.codex_item('correct'),self.codex_end()]
  rec,events=self.run_codex(frames,{'turn':{'id':'turn-new'}})
  self.assertEqual(rec['reply_text'],'correct');self.assertEqual(rec['execution_status'],'completed')

 def test_codex_notifications_before_start_response_are_replayed_once(self):
  frames=[self.codex_item('old',turnId='turn-old'),self.codex_item('early'),self.codex_end(),
   {'id':3,'result':{'turn':{'id':'turn-new'}}}]
  rec,events=self.run_codex(frames)
  self.assertEqual(rec['reply_text'],'early');self.assertEqual(rec['execution_status'],'completed')

 def test_codex_final_snapshot_repairs_repeated_delta(self):
  delta=self.codex_frame('item/agentMessage/delta',itemId='answer',delta='same')
  rec,events=self.run_codex([delta,delta,self.codex_item('same'),self.codex_end()],{'turn':{'id':'turn-new'}})
  self.assertEqual(rec['reply_text'],'same')
  self.assertEqual([e['payload']['text'] for e in events if e['kind']=='message_snapshot'],['same'])

 def test_codex_missing_start_turn_stays_unknown(self):
  rec,events=self.run_codex([],{'turn':{}})
  self.assertEqual(rec['execution_status'],'unknown');self.assertEqual(rec['error_code'],'HARNESS_IDENTITY_UNCONFIRMED')

 def test_codex_final_turn_items_recover_without_deltas(self):
  rec,events=self.run_codex([self.codex_end(items=[{'id':'answer','type':'agentMessage','text':'final'}])],{'turn':{'id':'turn-new'}})
  self.assertEqual(rec['reply_text'],'final');self.assertEqual(rec['execution_status'],'completed')

 def test_codex_failed_turn_does_not_claim_completed(self):
  rec,events=self.run_codex([self.codex_item('partial'),self.codex_end(status='failed')],{'turn':{'id':'turn-new'}})
  self.assertEqual(rec['execution_status'],'error');self.assertEqual(rec['error_code'],'HARNESS_TURN_FAILED')


 def test_codex_early_notification_buffer_is_bounded_and_unknown(self):
  rec,events=self.run_codex([self.codex_item('x'*1048577)])
  self.assertEqual(rec['execution_status'],'unknown')
  self.assertEqual(rec['error_code'],'HARNESS_TRANSPORT_FAILED')

 def test_codex_large_final_item_is_rejected_not_silently_truncated(self):
  rec,events=self.run_codex([self.codex_item('x'*262145)],{'turn':{'id':'turn-new'}})
  self.assertEqual(rec['execution_status'],'error')
  self.assertEqual(rec['error_code'],'HARNESS_OUTPUT_LIMIT')

 def test_codex_empty_final_item_clears_speculative_partial(self):
  rec,events=self.run_codex([self.codex_frame('item/agentMessage/delta',itemId='answer',delta='partial'),self.codex_item(''),self.codex_end()],{'turn':{'id':'turn-new'}})
  self.assertEqual(rec['reply_text'],'');self.assertEqual(rec['error_code'],'HARNESS_EMPTY_REPLY')


 def test_owned_codex_tools_buffer_before_ack_and_restore_under_original_request(self):
  frames=[self.codex_frame('item/started',item={'id':'tool','type':'commandExecution','command':'readonly check'}),
   {'id':3,'result':{'turn':{'id':'turn-new'}}},
   self.codex_frame('item/completed',turnId='unrelated',item={'id':'wrong','type':'commandExecution','status':'completed'}),
   self.codex_frame('item/completed',item={'id':'tool','type':'commandExecution','status':'completed','exitCode':0}),
   self.codex_item('final'),self.codex_end()]
  record,events=self.run_codex(frames)
  self.assertEqual(record['execution_status'],'completed')
  snapshots=p.read_native_events({'sessionId':self.cfg()['session_id'],'epoch':''})['snapshots']
  self.assertEqual(len(snapshots),1);self.assertEqual(snapshots[0]['request_id'],self.cfg()['request_id'])
  self.assertEqual([(x['id'],x['status']) for x in snapshots[0]['activity']['items']],[('tool','completed')])
  self.assertEqual(snapshots[0]['text'],'final')
  self.assertTrue(any(e['kind']=='source_event' and e['payload']['method']=='item/started' for e in events))
 def test_owned_codex_approval_is_observed_but_never_implicitly_granted(self):
  request=self.codex_frame('item/commandExecution/requestApproval',itemId='tool');request['id']=3
  record,events=self.run_codex([request,self.codex_item('final'),self.codex_end()],{'turn':{'id':'turn-new'}})
  self.assertEqual(record['execution_status'],'completed')
  snapshot=p.read_native_events({'sessionId':self.cfg()['session_id'],'epoch':''})['snapshots'][0]
  self.assertEqual(snapshot['activity']['approvals'][0]['status'],'unconfirmed')
  self.assertEqual(snapshot['activity']['approvals'][0]['observer_id'],'worker:'+self.cfg()['request_id'])
 def test_owned_approval_before_ack_is_persisted_without_second_reply(self):
  request=self.codex_frame('item/commandExecution/requestApproval',itemId='tool');request['id']=3
  record,events=self.run_codex([request,{'id':3,'result':{'turn':{'id':'turn-new'}}},self.codex_item('final'),self.codex_end()])
  self.assertEqual(record['execution_status'],'completed')
  approvals=p.read_native_events({'sessionId':self.cfg()['session_id'],'epoch':''})['snapshots'][0]['activity']['approvals']
  self.assertEqual(len(approvals),1);self.assertEqual(approvals[0]['status'],'unconfirmed')
  self.assertEqual(sum(f.get('id')==3 and 'error' in f for f in self.codex_sent),1)
  self.assertEqual(sum(e['kind']=='source_event' and e['payload']['method'].endswith('/requestApproval') for e in events),1)
 def test_owned_preack_approval_for_other_turn_is_not_attributed(self):
  request=self.codex_frame('item/commandExecution/requestApproval',turnId='other',itemId='tool');request['id']=3
  record,events=self.run_codex([request,{'id':3,'result':{'turn':{'id':'turn-new'}}},self.codex_item('final'),self.codex_end()])
  self.assertEqual(record['execution_status'],'completed')
  self.assertFalse(any(e['kind']=='source_event' and e['payload']['method'].endswith('/requestApproval') for e in events))
  self.assertEqual(sum(f.get('id')==3 and 'error' in f for f in self.codex_sent),1)
 def test_claude_inventory_busy_idle_and_unknown_are_not_absence(self):
  from types import SimpleNamespace
  for rows,expected in [([{'sessionId':'same','status':'idle'}],'HARNESS_SESSION_BUSY'),([{'sessionId':'same','status':'running'}],'HARNESS_SESSION_BUSY'),([{'id':'ambiguous'}],'HARNESS_ACTIVITY_UNCONFIRMED'),({},'HARNESS_ACTIVITY_UNCONFIRMED')]:
   with patch.object(p.subprocess,'run',return_value=SimpleNamespace(returncode=0,stdout=json.dumps(rows).encode())) as run:
    with self.assertRaisesRegex(ValueError,expected):p._ensure_claude_session_available('claude','same',{})
    self.assertEqual(run.call_args.args[0],['claude','agents','--json'])
  with patch.object(p.subprocess,'run',return_value=SimpleNamespace(returncode=0,stdout=b'[{"sessionId":"different"}]')):
   p._ensure_claude_session_available('claude','same',{})
  with patch.object(p.subprocess,'run',side_effect=p.subprocess.TimeoutExpired('claude',8)):
   with self.assertRaisesRegex(ValueError,'HARNESS_ACTIVITY_UNCONFIRMED'):p._ensure_claude_session_available('claude','same',{})
 def test_claude_owned_external_session_never_launches_resume(self):
  cfg=dict(self.cfg(),session_id='claude-jsonl:12345678-1234-1234-1234-123456789abc',work_cwd=self.tmp.name)
  with patch.object(p.subprocess,'Popen') as spawn:
   spawn.return_value.pid=123;p._native_start(cfg,'# worker')
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='claude'),patch.object(p,'_ensure_claude_session_available',side_effect=ValueError('HARNESS_SESSION_BUSY')):
   p._native_worker(cfg);pipe.assert_not_called()
  record=p._stream_records(cfg['session_id'])[0]
  self.assertEqual(record['execution_status'],'error');self.assertEqual(record['error_code'],'HARNESS_SESSION_BUSY')
 def test_claude_empty_tool_message_does_not_clear_streamed_text(self):
  cfg=dict(self.cfg(),session_id='claude-jsonl:12345678-1234-1234-1234-123456789abc',work_cwd=self.tmp.name)
  raw=cfg['session_id'].split(':')[1]
  with patch.object(p.subprocess,'Popen') as spawn:
   spawn.return_value.pid=123;p._native_start(cfg,'# worker')
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='claude'),patch.object(p,'_ensure_claude_session_available'),patch.object(p,'_claude_initialize'):
   pipe.return_value.receive.side_effect=[{'session_id':raw,'event':{'type':'content_block_delta','delta':{'type':'text_delta','text':'reply'}}},
    {'session_id':raw,'type':'assistant','message':{'content':[{'type':'tool_use'}]}},
    {'session_id':raw,'type':'result','subtype':'success','result':'reply'}]
   p._native_worker(cfg)
  rec=p._stream_records(cfg['session_id'])[0]
  self.assertEqual(rec['reply_text'],'reply');self.assertEqual(rec['execution_status'],'completed')
  self.assertNotIn('message_snapshot',[e['kind'] for e in p.read_native_events({'sessionId':cfg['session_id']})['events']])


 def test_claude_stream_identity_precedes_content_and_raw_events_persist(self):
  for name,frames,expected in [
   ('unbound',[{'type':'stream_event','event':{'type':'content_block_delta','delta':{'type':'text_delta','text':'untrusted'}}}],False),
   ('wrong',[{'session_id':'another-session','type':'result','subtype':'success','result':'wrong'}],False),
   ('bound',[{'session_id':'12345678-1234-1234-1234-123456789abc','type':'system','subtype':'init'},
    {'type':'stream_event','event':{'type':'content_block_delta','delta':{'type':'text_delta','text':'confirmed'}}},
    {'type':'result','subtype':'success','result':'confirmed'}],True)]:
   cfg=dict(self.cfg(),request_id=name,session_id='claude-jsonl:12345678-1234-1234-1234-123456789abc',work_cwd=self.tmp.name)
   # Seed the actual worker receipt without spawning a CLI.
   with p.closing(p._stream_db()) as db,db:
    db.execute('INSERT INTO requests VALUES(?,?,?,?)',(name,cfg['session_id'],'test-input-hash',json.dumps(dict(cfg,execution_status='accepted'))))
   with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='claude'),patch.object(p,'_ensure_claude_session_available'),patch.object(p,'_claude_initialize'):
    pipe.return_value.receive.side_effect=frames;p._native_worker(cfg)
   with p.closing(p._stream_db()) as db:
    rec=json.loads(db.execute('SELECT body FROM requests WHERE id=?',(name,)).fetchone()[0])
    events=[(row[0],json.loads(row[1])) for row in db.execute('SELECT kind,payload FROM events WHERE request=? ORDER BY sequence',(name,))]
   if expected:
    self.assertEqual(rec['execution_status'],'completed');self.assertEqual(rec['reply_text'],'confirmed')
    raw=[payload for kind,payload in events if kind=='source_event']
    self.assertEqual(len(raw),2);self.assertTrue(all(v['source']=='claude-stream-json' for v in raw))
    self.assertEqual(raw[-1]['frame']['type'],'result')
   else:
    self.assertFalse(rec.get('reply_text'));self.assertNotEqual(rec['execution_status'],'completed')
    self.assertFalse(any(kind in ('source_event','text_delta','message_snapshot','completed') for kind,_ in events))

 def test_claude_cancel_needs_exact_ack_and_aborted_terminal_in_either_order(self):
  for order in ('ack-first','result-first','completed-race'):
   cfg=dict(self.cfg(),request_id=order,session_id='claude-jsonl:12345678-1234-1234-1234-123456789abc',work_cwd=self.tmp.name)
   raw=cfg['session_id'].split(':')[1]
   with p.closing(p._stream_db()) as db,db:db.execute('INSERT INTO requests VALUES(?,?,?,?)',(order,cfg['session_id'],'hash',json.dumps(dict(cfg,execution_status='accepted',cancel_supported=True))))
   ack={'type':'control_response','response':{'subtype':'success','request_id':'bob-cancel-'+order}}
   end={'type':'result','session_id':raw,'is_error':True,'terminal_reason':'aborted_streaming'}
   frames=[ack,end] if order=='ack-first' else [end,ack]
   if order=='completed-race':frames=[ack,{'type':'result','session_id':raw,'subtype':'success','result':'partial finished'}]
   frames.insert(0,{'session_id':raw,'type':'stream_event','event':{'type':'content_block_delta','delta':{'type':'text_delta','text':'partial'}}})
   calls=0
   def receive(**kwargs):
    nonlocal calls
    calls+=1
    return frames.pop(0)
   def requested(rid):return calls>=1
   with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='claude'),patch.object(p,'_ensure_claude_session_available'),patch.object(p,'_claude_initialize'),patch.object(p,'_cancel_requested',side_effect=requested):
    pipe.return_value.receive.side_effect=receive;p._native_worker(cfg)
    commands=[c.args[0] for c in pipe.return_value.send.call_args_list]
   self.assertEqual(sum(c.get('request',{}).get('subtype')=='interrupt' for c in commands),1)
   with p.closing(p._stream_db()) as db:r=json.loads(db.execute('SELECT body FROM requests WHERE id=?',(order,)).fetchone()[0])
   self.assertEqual(r['execution_status'],'completed' if order=='completed-race' else 'cancelled')
   self.assertEqual(r['reply_text'],'partial finished' if order=='completed-race' else 'partial')
   sent=next(c for c in commands if c.get('type')=='user')
   self.assertEqual(sent['uuid'],r['native_user_message_id'])
   self.assertEqual(sent['uuid'],str(p.uuid.uuid5(p.uuid.NAMESPACE_URL,'bob-native:'+cfg['session_id']+':'+order)))

 def test_claude_unconfirmed_cancel_preserves_partial_and_unknown(self):
  for mode in ('missing-ack','wrong-ack','rejected-ack','transport-loss'):
   with self.subTest(mode=mode):
    cfg=dict(self.cfg(),request_id=mode,session_id='claude-jsonl:12345678-1234-1234-1234-123456789abc',work_cwd=self.tmp.name)
    raw=cfg['session_id'].split(':')[1]
    with p.closing(p._stream_db()) as db,db:db.execute('INSERT INTO requests VALUES(?,?,?,?)',(mode,cfg['session_id'],'hash',json.dumps(dict(cfg,execution_status='accepted',cancel_supported=True))))
    frames=[{'session_id':raw,'type':'stream_event','event':{'type':'content_block_delta','delta':{'type':'text_delta','text':'retained partial'}}}]
    if mode in ('wrong-ack','rejected-ack'):
     frames.append({'type':'control_response','response':{'subtype':'error' if mode=='rejected-ack' else 'success','request_id':'unrelated' if mode=='wrong-ack' else 'bob-cancel-'+mode}})
    if mode!='transport-loss':frames.append({'type':'result','session_id':raw,'is_error':True,'terminal_reason':'aborted_streaming'})
    calls=0;clock=[0.0]
    def receive(**kwargs):
     nonlocal calls
     calls+=1
     if frames:return frames.pop(0)
     if mode=='transport-loss':raise ValueError('HARNESS_PROCESS_EXITED')
     clock[0]+=11
     return None
    with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='claude'),patch.object(p,'_ensure_claude_session_available'),patch.object(p,'_claude_initialize'),patch.object(p,'_cancel_requested',side_effect=lambda rid:calls>=1),patch.object(p.time,'monotonic',side_effect=lambda:clock[0]):
     pipe.return_value.receive.side_effect=receive;p._native_worker(cfg)
     commands=[c.args[0] for c in pipe.return_value.send.call_args_list]
    self.assertEqual(sum(c.get('request',{}).get('subtype')=='interrupt' for c in commands),1)
    with p.closing(p._stream_db()) as db:receipt=json.loads(db.execute('SELECT body FROM requests WHERE id=?',(mode,)).fetchone()[0])
    self.assertEqual(receipt['execution_status'],'unknown')
    self.assertEqual(receipt['reply_text'],'retained partial')
    self.assertEqual(receipt['error_code'],'HARNESS_PROCESS_EXITED' if mode=='transport-loss' else 'HARNESS_CANCEL_UNCONFIRMED')
    events=p.read_native_events({'sessionId':cfg['session_id']})['events']
    self.assertFalse(any(e['kind'] in ('cancelled','completed') for e in events))

 def test_claude_source_terminal_recovers_after_projection_crash(self):
  for mode in ('success','cancel','cancel-reversed','missing-ack','wrong-session','wrong-ack'):
   with self.subTest(mode=mode):
    sid='claude-jsonl:12345678-1234-1234-1234-123456789abc';cfg=dict(self.cfg(),session_id=sid,request_id=mode)
    with p.closing(p._stream_db()) as db,db:db.execute('INSERT INTO requests VALUES(?,?,?,?)',(mode,sid,'hash',json.dumps(dict(cfg,execution_status='unknown',reply_text='partial'))))
    p._stream_event(cfg,'text_delta',{'text':'partial'})
    end={'type':'result','session_id':sid.split(':')[1],'subtype':'success','result':'final'}
    ack={'type':'control_response','response':{'subtype':'success','request_id':'bob-cancel-'+mode}}
    if mode!='success':end.update(subtype='error_during_execution',is_error=True,terminal_reason='aborted_streaming')
    if mode=='wrong-session':end['session_id']='other'
    if mode=='wrong-ack':ack['response']['request_id']='other'
    frames=[end] if mode in ('success','missing-ack') else [end,ack] if mode=='cancel-reversed' else [ack,end]
    for frame in frames:p._stream_event(cfg,'source_event',{'source':'claude-stream-json','frame':frame})
    with patch.object(p.subprocess,'Popen') as spawn:
     rows=p._stream_records(sid);p._stream_records(sid);spawn.assert_not_called()
    receipt=next(row for row in rows if row['request_id']==mode)
    expected='completed' if mode=='success' else 'cancelled' if mode in ('cancel','cancel-reversed') else 'unknown'
    self.assertEqual(receipt['execution_status'],expected)
    self.assertEqual(receipt['reply_text'],'final' if mode=='success' else 'partial')
    with p.closing(p._stream_db()) as db:count=db.execute("SELECT COUNT(*) FROM events WHERE request=? AND kind IN ('completed','cancelled')",(mode,)).fetchone()[0]
    self.assertEqual(count,0 if expected=='unknown' else 1)

 def test_claude_relay_keeps_terminal_after_output_reader_closes(self):
  import subprocess,sys,os
  sid='claude-jsonl:12345678-1234-1234-1234-123456789abc';rid='relay-check';uid='input-id'
  path=p._claude_wire_path(rid);path.parent.mkdir(parents=True)
  identity={'session_id':sid,'request_id':rid,'user_uuid':uid}
  init={'type':'system','session_id':sid.split(':')[1]}
  result={'type':'result','session_id':sid.split(':')[1],'subtype':'success','result':'confirmed result'}
  child="import time,json;print("+repr(json.dumps(init))+",flush=True);time.sleep(.2);print("+repr(json.dumps(result))+",flush=True)"
  cfg={'path':str(path),'identity':identity,'command':[sys.executable,'-u','-c',child]}
  proc=subprocess.Popen([sys.executable,'-u','-c',p.CLAUDE_WIRE_RELAY,json.dumps(cfg)],stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL)
  try:
   self.assertEqual(json.loads(proc.stdout.readline()),init)
   proc.stdout.close();proc.wait(timeout=10)
  finally:
   if proc.poll() is None:proc.kill();proc.wait(timeout=3)
  record={'session_id':sid,'request_id':rid,'native_user_message_id':uid,'claude_wire':True}
  self.assertEqual(p._claude_wire_result(record),('completed','confirmed result'))
  self.assertIsNone(p._claude_wire_result(dict(record,native_user_message_id='wrong')))
  data=path.read_bytes();path.write_bytes(data[:-1])
  self.assertIsNone(p._claude_wire_result(record))

 def test_claude_relay_serializes_capacity_admission(self):
  import subprocess,sys
  root=p._claude_wire_path('capacity').parent;root.mkdir(parents=True)
  for i in range(127):(root/(str(i)+'.jsonl')).write_text('',encoding='utf-8')
  children=[]
  try:
   for i in range(2):
    cfg={'path':str(root/('new'+str(i)+'.jsonl')),'identity':{'request_id':str(i)},'command':[sys.executable,'-c','print(123)']}
    children.append(subprocess.Popen([sys.executable,'-u','-c',p.CLAUDE_WIRE_RELAY,json.dumps(cfg)],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL))
   results=[child.wait(timeout=10) for child in children]
   self.assertEqual(sum(code==0 for code in results),1)
   self.assertEqual(len(list(root.glob('*.jsonl'))),128)
  finally:
   for child in children:
    if child.poll() is None:child.kill();child.wait(timeout=3)

 def test_claude_retention_preserves_unconfirmed_live_and_mismatched_records(self):
  sid='claude-jsonl:12345678-1234-1234-1234-123456789abc'
  modes=('expired','recent','unknown','no-closed-marker','different-text','different-marker')
  for mode in modes:
   identity={'session_id':sid,'request_id':mode,'user_uuid':'id-'+mode}
   path=p._claude_wire_path(mode);path.parent.mkdir(parents=True,exist_ok=True)
   end={'type':'result','session_id':sid.split(':')[1],'subtype':'success','result':'retained result'}
   path.write_text(json.dumps(identity)+'\n'+json.dumps(end)+'\n',encoding='utf-8')
   if mode!='no-closed-marker':path.with_suffix('.done').write_text(json.dumps(dict(identity,user_uuid='wrong') if mode=='different-marker' else identity),encoding='utf-8')
   record=dict(identity,native_user_message_id=identity['user_uuid'],claude_wire=True,execution_status='unknown' if mode=='unknown' else 'completed',ended_at=p._now()-(0 if mode=='recent' else 8*24*3600*1000),reply_text='different' if mode=='different-text' else 'retained result')
   with p.closing(p._stream_db()) as db,db:db.execute('INSERT INTO requests VALUES(?,?,?,?)',(mode,sid,'hash',json.dumps(record)))
  self.assertEqual(p._claude_prune_wire(),1)
  self.assertFalse(p._claude_wire_path('expired').exists())
  for mode in modes[1:]:self.assertTrue(p._claude_wire_path(mode).exists())
  with p.closing(p._stream_db()) as db:self.assertEqual(db.execute('SELECT COUNT(*) FROM requests').fetchone()[0],len(modes))
  self.assertEqual(p._claude_prune_wire(),0)

 def unknown_codex(self, turn_id='turn-known'):
  cfg=self.cfg()
  with patch.object(p.subprocess,'Popen') as spawn:
   spawn.return_value.pid=123;p._native_start(cfg,'# worker')
  p._stream_update(cfg['request_id'],execution_status='unknown',native_session_id=cfg['session_id'].split(':')[1],native_turn_id=turn_id)
  return cfg

 def test_unknown_without_turn_identity_never_starts_another_process(self):
  cfg=self.unknown_codex(None)
  with patch.object(p,'NativePipe') as pipe:
   result=p.inspect_receipt({'sessionId':cfg['session_id'],'requestId':cfg['request_id'],'text':cfg['text']})
   self.assertEqual(result['status'],'unknown');pipe.assert_not_called()

 def test_unknown_exact_turn_recovers_once_via_read_only_api(self):
  cfg=self.unknown_codex();raw=cfg['session_id'].split(':')[1]
  frames=[{'id':1,'result':{}},{'id':2,'result':{'thread':{'id':raw}}},{'id':10,'result':{'data':[{'id':'turn-known','status':'completed'}]}},{'id':11,'result':{'data':[{'turnId':'turn-known','item':{'id':'answer','type':'agentMessage','text':'recovered'}}]}}]
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='codex'):
   pipe.return_value.receive.side_effect=frames
   args={'sessionId':cfg['session_id'],'requestId':cfg['request_id'],'text':cfg['text']}
   result=p.inspect_receipt(args);again=p.inspect_receipt(args)
   self.assertEqual(result['execution_status'],'completed');self.assertEqual(again,result);self.assertEqual(pipe.call_count,1)
   self.assertEqual([c.args[0].get('method') for c in pipe.return_value.send.call_args_list],['initialize','initialized','thread/read','thread/turns/list','thread/items/list'])
  events=p.read_native_events({'sessionId':cfg['session_id']})['events']
  self.assertEqual(sum(e['kind']=='completed' for e in events),1)
  self.assertEqual(p._stream_records(cfg['session_id'])[0]['reply_text'],'recovered')

 def test_unknown_recovery_restores_tools_and_keeps_approval_unconfirmed(self):
  cfg=self.unknown_codex();raw=cfg['session_id'].split(':')[1]
  p._stream_event(cfg,'activity_snapshot',{'activity':{'items':[],'approvals':[{'id':'pending','status':'pending'}],'partial':False}})
  turn={'id':'turn-known','status':'completed','items':[
   {'id':'answer','type':'agentMessage','text':'recovered'},
   {'id':'tool-ok','type':'commandExecution','command':'check','status':'completed','exitCode':0},
   {'id':'tool-fail','type':'commandExecution','status':'failed','exitCode':1},
   {'id':'tool-stale','type':'commandExecution','status':'inProgress'}]}
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='codex'),patch.object(p,'_read_codex_receipt_turn',return_value=turn):
   pipe.return_value.receive.side_effect=[{'id':1,'result':{}},{'id':2,'result':{'thread':{'id':raw}}}]
   result=p.inspect_receipt({'sessionId':cfg['session_id'],'requestId':cfg['request_id'],'text':cfg['text']})
  self.assertEqual(result['execution_status'],'completed')
  restored=p.read_native_events({'sessionId':cfg['session_id'],'epoch':''})['snapshots'][0]
  self.assertEqual(restored['status'],'completed');self.assertEqual(restored['text'],'recovered')
  self.assertEqual({x['id']:x['status'] for x in restored['activity']['items']},{'tool-ok':'completed','tool-fail':'error','tool-stale':'unconfirmed'})
  self.assertEqual(restored['activity']['approvals'][0]['status'],'unconfirmed')
 def test_unknown_wrong_thread_or_missing_turn_is_not_recovered(self):
  cfg=self.unknown_codex();raw=cfg['session_id'].split(':')[1]
  for thread in [{'id':'other','turns':[{'id':'turn-known','status':'completed'}]},{'id':raw,'turns':[{'id':'other','status':'completed'}]}]:
   with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='codex'):
    pipe.return_value.receive.side_effect=[{'id':1,'result':{}},{'id':2,'result':{'thread':thread}}]
    result=p.inspect_receipt({'sessionId':cfg['session_id'],'requestId':cfg['request_id'],'text':cfg['text']})
    self.assertEqual(result['status'],'unknown')

 def test_receipt_content_conflict_never_probes_or_clears_unknown(self):
  cfg=self.unknown_codex()
  with patch.object(p,'NativePipe') as pipe:
   with self.assertRaisesRegex(ValueError,'CONFLICT'):p.inspect_receipt({'sessionId':cfg['session_id'],'requestId':cfg['request_id'],'text':'different'})
   pipe.assert_not_called()

 def test_claude_remote_control_vs_terminal_takeover_and_no_fake_bubble(self):
  root=Path(self.tmp.name)/'claude';folder=root/'projects'/'workspace';folder.mkdir(parents=True)
  raw='12345678-1234-1234-1234-123456789abc';sid='claude-jsonl:'+raw
  (folder/(raw+'.jsonl')).write_text(json.dumps({'type':'user','uuid':'u1','timestamp':1000,'cwd':str(folder),'message':{'content':'First'}})+'\n',encoding='utf-8')
  with patch.object(p.shutil,'which',return_value='claude'),patch.object(p,'_inspect_claude_session_holder',return_value={'busy':True,'pid':4242,'name':'Winter shelter','kind':'interactive','status':'idle','remote_control':True}):
   res_rc=p.prompt_session({'sessionId':sid,'requestId':'req-rc','text':'Follow up','claudeDataDir':str(root)})
  self.assertEqual(res_rc['status'],'not_accepted')
  self.assertEqual(res_rc['reason'],'HARNESS_CLAUDE_REMOTE_CONTROL')
  self.assertTrue(res_rc['terminal_holder']['remote_control'])
  with patch.object(p.shutil,'which',return_value='claude'),patch.object(p,'_inspect_claude_session_holder',return_value={'busy':True,'pid':4242,'name':'Winter shelter','kind':'interactive','status':'idle','remote_control':False}):
   res_busy=p.prompt_session({'sessionId':sid,'requestId':'req-busy','text':'Follow up','claudeDataDir':str(root)})
  self.assertEqual(res_busy['status'],'not_accepted')
  self.assertEqual(res_busy['reason'],'HARNESS_SESSION_BUSY')
  self.assertFalse(res_busy['terminal_holder']['remote_control'])
  failed_rec={'request_id':'req-busy','text':'Fake bubble text','reply_text':'','started_at':5000,'execution_status':'error','error_code':'HARNESS_SESSION_BUSY'}
  with patch.object(p,'_reconcile',return_value=[failed_rec]):
   page=p.read_session_page({'claudeDataDir':str(root)},sid)
  self.assertEqual([m['text'] for m in page['messages']],['First'])

 def test_claude_ask_user_question_in_bubble_options(self):
  from unittest.mock import Mock
  rid='ask-1';sid='claude-jsonl:original';cfg={'session_id':sid,'request_id':rid,'approval_mode':'interactive','claude_tools':['AskUserQuestion']}
  rec=dict(cfg,execution_status='running',pid=123,started_at=p._now(),tool_authorization={'version':1,'approval_mode':'interactive','tools':['AskUserQuestion']})
  with p.closing(p._stream_db()) as db,db:db.execute('INSERT INTO requests VALUES(?,?,?,?)',(rid,sid,'hash',json.dumps(rec)))
  pipe=Mock()
  q_input={'questions':[{'question':'Which mode?','options':[{'label':'Fast','description':'Quick'},{'label':'Safe','description':'Careful'}]}]}
  frame={'type':'control_request','request_id':'perm-ask','request':{'subtype':'can_use_tool','tool_name':'AskUserQuestion','input':q_input}}
  p._handle_claude_permission(cfg,pipe,frame);pipe.send.assert_not_called()
  with p.closing(p._stream_db()) as db:entry=json.loads(db.execute('SELECT body FROM native_permissions WHERE request=?',(rid,)).fetchone()[0])
  self.assertEqual([o['id'] for o in entry['options']],['answer_0','answer_1','reject_once'])
  with patch.object(p,'_permission_owner_alive',return_value=True):
   p.submit_native_permission({'sessionId':sid,'requestId':rid,'permissionId':entry['id'],'optionId':'answer_0','claudeTools':['AskUserQuestion']})
   p._flush_native_permissions(cfg,pipe)
  resp=pipe.send.call_args.args[0]['response']['response']
  self.assertEqual(resp['behavior'],'allow')
  self.assertEqual(resp['updatedInput']['answers'],{'Which mode?':'Fast'})

 def test_codex_app_server_holder_and_hot_interrupt_takeover(self):
  from unittest.mock import Mock
  raw='12345678-1234-4234-8234-123456789abc'
  pipe=Mock();pipe.path='/tmp/codex-app.sock'
  thread={'id':raw,'turns':[{'id':'turn-live','status':'inProgress'}]}
  with patch.object(p,'_open_loaded_codex',return_value=(pipe,thread)),patch.object(p,'_codex_socket_pid_map',return_value={'/tmp/codex-app.sock':4321}),patch.object(p,'_observer_process_owned',return_value=False):
   holder=p._inspect_codex_session_holder(raw)
  self.assertTrue(holder['busy']);self.assertEqual(holder['pid'],4321)
  self.assertEqual(holder['harness'],'codex');self.assertEqual(holder['mode'],'codex_app_server')
  self.assertTrue(holder['can_hot_interrupt']);self.assertFalse(holder['remote_control'])
  with patch.object(p,'_open_loaded_codex',return_value=(pipe,thread)),patch.object(p,'_codex_socket_pid_map',return_value={'/tmp/codex-app.sock':4321}),patch.object(p,'_observer_process_owned',return_value=False),patch.object(p,'_control_result',return_value={}),patch.object(p.os,'kill',side_effect=AssertionError('hot interrupt must not SIGTERM')):
   p._release_codex_terminal_holder(raw)
  sent=[c.args[0] for c in pipe.send.call_args_list]
  self.assertTrue(any(f.get('method')=='turn/interrupt' for f in sent))

 def test_codex_interactive_permission_accept_and_decline_flush(self):
  from unittest.mock import Mock
  sid='codex:12345678-1234-4234-8234-123456789abc';raw=sid.split(':',1)[1];rid='req-codex-perm'
  cfg={'session_id':sid,'request_id':rid}
  with p.closing(p._stream_db()) as db,db:
   db.execute('INSERT INTO requests VALUES(?,?,?,?)',(rid,sid,'h',json.dumps({'request_id':rid,'session_id':sid,'execution_status':'running','pid':4444,'started_at':p._now()})))
  normalized={'id':77,'protocol':'codex','codex_method':'item/commandExecution/requestApproval',
   'params':{'sessionId':raw,'toolCall':{'title':'echo ok','command':'echo ok'},
    'options':[{'optionId':'allow_once','kind':'allow_once','name':'Allow once'},
               {'optionId':'reject_once','kind':'reject_once','name':'Decline'}]}}
  self.assertTrue(p._open_native_permission(cfg,normalized,1))
  with p.closing(p._stream_db()) as db:
   entry=json.loads(db.execute('SELECT body FROM native_permissions WHERE request=?',(rid,)).fetchone()[0])
  opt_ids=[o['id'] for o in entry['options']]
  self.assertEqual(opt_ids,['allow_once','reject_once'])
  with patch.object(p,'_permission_owner_alive',return_value=True):
   p.submit_native_permission({'sessionId':sid,'requestId':rid,'permissionId':entry['id'],'optionId':'allow_once'})
  pipe=Mock()
  p._flush_native_permissions(cfg,pipe)
  pipe.send.assert_called_once()
  sent=pipe.send.call_args.args[0]
  self.assertEqual(sent['id'],77);self.assertEqual(sent['result']['decision'],'accept')

 def test_agy_holder_distinguishes_ide_protected_and_cli_takeover(self):
  raw='agy-sess-1234'
  ide_procs=[(7001,['/opt/Antigravity/antigravity','--type=renderer',raw])]
  cli_procs=[(7002,['agy','--session',raw])]
  with patch.object(p,'_iter_same_uid_procs',return_value=ide_procs),patch.object(p,'_is_bob_owned_proc',return_value=False),patch.object(p,'_proc_fd_targets',return_value=()):
   ide_h=p._inspect_agy_session_holder(raw)
  self.assertTrue(ide_h['busy']);self.assertTrue(ide_h['remote_control']);self.assertEqual(ide_h['mode'],'agy_ide')
  with patch.object(p,'_iter_same_uid_procs',return_value=ide_procs),patch.object(p,'_is_bob_owned_proc',return_value=False),patch.object(p,'_proc_fd_targets',return_value=()),patch.object(p.os,'kill',side_effect=AssertionError('IDE must never be killed')):
   with self.assertRaisesRegex(ValueError,'HARNESS_REMOTE_CONTROL'):
    p._release_agy_terminal_holder(raw)
  with patch.object(p,'_iter_same_uid_procs',return_value=cli_procs),patch.object(p,'_is_bob_owned_proc',return_value=False),patch.object(p,'_proc_fd_targets',return_value=()):
   cli_h=p._inspect_agy_session_holder(raw)
  self.assertTrue(cli_h['busy']);self.assertFalse(cli_h['remote_control']);self.assertEqual(cli_h['mode'],'agy_terminal')
  with patch.object(p,'_iter_same_uid_procs',side_effect=[cli_procs,[]]),patch.object(p,'_is_bob_owned_proc',return_value=False),patch.object(p,'_proc_fd_targets',return_value=()),patch.object(p.os,'kill') as kill_mock:
   p._release_agy_terminal_holder(raw)
  kill_mock.assert_called_with(7002,p.signal.SIGTERM)

 def test_bob_holder_reverse_release_stops_idle_and_guards_running(self):
  sid='agy:release-test-1';rid='req-release-1'
  with p.closing(p._stream_db()) as db,db:
   p._agy_tables(db)
   db.execute('INSERT INTO requests VALUES(?,?,?,?)',(rid,sid,'h',json.dumps({'request_id':rid,'session_id':sid,'execution_status':'running','started_at':p._now()})))
   db.execute('INSERT INTO agy_supervisors VALUES(?,?,?,?)',(sid,'tok-1',8801,'cli'))
  with patch.object(p,'_agy_process_owned',return_value=True),patch.object(p.os,'kill') as kill_mock:
   busy_res=p.request_native_cancel({'sessionId':sid,'requestId':'__bob_release_holder__'})
  self.assertEqual(busy_res['status'],'busy');self.assertFalse(busy_res['released']);kill_mock.assert_not_called()
  with p.closing(p._stream_db()) as db,db:
   db.execute('UPDATE requests SET body=? WHERE id=?',(json.dumps({'request_id':rid,'session_id':sid,'execution_status':'completed','started_at':p._now()}),rid))
   db.execute('INSERT OR REPLACE INTO agy_supervisors VALUES(?,?,?,?)',(sid,'tok-2',8802,'cli'))
  with patch.object(p,'_agy_process_owned',return_value=True),patch.object(p.os,'kill') as kill_mock:
   rel_res=p.request_native_cancel({'sessionId':sid,'requestId':'__bob_release_holder__'})
  self.assertEqual(rel_res['status'],'released');self.assertTrue(rel_res['released'])
  kill_mock.assert_called_with(8802,p.signal.SIGTERM)

if __name__=='__main__':unittest.main()
