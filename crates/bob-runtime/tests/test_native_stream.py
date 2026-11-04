import importlib.util,json,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('probe',Path(__file__).parents[1]/'src/agy_index_probe.py')
p=importlib.util.module_from_spec(spec);spec.loader.exec_module(p)

class NativeTests(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.home=patch.object(p.Path,'home',return_value=Path(self.tmp.name));self.home.start()
 def tearDown(self):self.home.stop();self.tmp.cleanup()
 def cfg(self):return {'request_id':'request-a','session_id':'codex:12345678-1234-1234-1234-123456789abc','text':'read only'}
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
 def test_agy_native_stream_and_identity(self):
  cfg=dict(self.cfg(),session_id='agy:12345678-1234-1234-1234-123456789abc',work_cwd=self.tmp.name)
  raw=cfg['session_id'].split(':')[1]
  with patch.object(p.subprocess,'Popen') as spawn:
   spawn.return_value.pid=123;p._native_start(cfg,'# worker')
  with patch.object(p,'NativePipe') as pipe,patch.object(p.shutil,'which',return_value='agy'):
   pipe.return_value.receive.side_effect=[{'event':'init','conversation_id':raw},{'event':'step_update','step_update':{'conversation_id':raw,'step_type':'agent_response','text_delta':'hello'}},{'event':'result','result':{'conversation_id':raw,'status':'SUCCESS','response':'hello'}}]
   p._native_worker(cfg)
  rec=p._stream_records(cfg['session_id'])[0]
  self.assertEqual(rec['execution_status'],'completed');self.assertEqual(rec['native_session_id'],raw)
  self.assertEqual([e['kind'] for e in p.read_native_events({'sessionId':cfg['session_id']})['events']],['accepted','running','text_delta','completed'])


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

if __name__=='__main__':unittest.main()
