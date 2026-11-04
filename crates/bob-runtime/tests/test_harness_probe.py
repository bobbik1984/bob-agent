import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE=Path(__file__).resolve().parent
SOURCE=HERE/'harness_probe.py'
if not SOURCE.exists():
    SOURCE=HERE.parent/'src'/'harness_probe.py'
spec=importlib.util.spec_from_file_location('harness_probe',SOURCE)
probe=importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class NativeProtocolTests(unittest.TestCase):
    def child(self, code):
        work=tempfile.TemporaryDirectory()
        self.addCleanup(work.cleanup)
        script=Path(work.name)/'fake.py'
        script.write_text(code,encoding='utf-8')
        child=probe.JsonLinesProcess([sys.executable,'-u',str(script)],work.name,timeout=1)
        self.addCleanup(child.close)
        return child

    def test_persistent_process_correlates_and_ignores_notifications(self):
        child=self.child('''import json,sys,os
for line in sys.stdin:
    m=json.loads(line)
    print(json.dumps({'method':'turn/completed'}),flush=True)
    print(json.dumps({'id':'unrelated','result':False}),flush=True)
    print(json.dumps({'id':m['id'],'result':{'pid':os.getpid(),'text':'雪\\u2028山'}}),flush=True)
''')
        a=child.request({'id':1,'method':'one'})
        b=child.request({'id':2,'method':'two'})
        self.assertEqual(a['pid'],b['pid'])
        self.assertEqual(a['text'],'雪\u2028山')
        self.assertEqual(child.next_event()['method'],'turn/completed')

    def test_utf8_split_writes_and_crlf(self):
        child=self.child('''import sys,json,time
json.loads(sys.stdin.readline())
wire=json.dumps({'id':1,'result':'雪山'},ensure_ascii=False).encode('utf-8')+b'\\r\\n'
for b in wire:
    sys.stdout.buffer.write(bytes([b]));sys.stdout.buffer.flush()
''')
        self.assertEqual(child.request({'id':1}),'雪山')

    def test_error_does_not_leak_payload(self):
        child=self.child('''import sys,json
for line in sys.stdin:
    m=json.loads(line);print(json.dumps({'id':m['id'],'error':{'code':-32601,'message':'SECRET_DO_NOT_RETURN'}}),flush=True)
''')
        with self.assertRaisesRegex(probe.ProbeError,'^METHOD_UNSUPPORTED$'):
            child.request({'id':1})

    def test_accepted_rpc_result_is_not_a_completed_turn(self):
        child=self.child('''import sys,json
for line in sys.stdin:
    m=json.loads(line);print(json.dumps({'id':m['id'],'type':'response','success':True,'data':{'disposition':'started'}}),flush=True)
''')
        self.assertEqual(child.request({'id':'r'}),{'disposition':'started'})

    def test_output_limits(self):
        child=self.child("import sys;sys.stdin.readline();print('x'*1100000,flush=True)")
        with self.assertRaisesRegex(probe.ProbeError,'OUTPUT_LIMIT'):
            child.request({'id':1})

    def test_invalid_json_does_not_become_history(self):
        child=self.child("import sys;sys.stdin.readline();print('not-json',flush=True)")
        with self.assertRaisesRegex(probe.ProbeError,'PROTOCOL_INVALID'):
            child.request({'id':1})

    def test_timeout_closes_owned_process(self):
        child=self.child("import time;time.sleep(30)")
        child.timeout=0.2
        with self.assertRaisesRegex(probe.ProbeError,'PROBE_TIMEOUT'):
            child.request({'id':1})
        child.close()
        self.assertIsNotNone(child.proc.poll())

    def test_no_executable_is_not_service_absence_or_login_failure(self):
        with patch.object(probe.shutil,'which',return_value=None):
            report=probe.probe({'tool':'dsh'})
        self.assertEqual(report['auth_state'],'unknown')
        self.assertEqual(report['reason'],'EXECUTABLE_NOT_FOUND_SERVICE_NOT_PROBED')
        self.assertTrue(all(v=='unknown' for v in report['capabilities'].values()))

    def test_pi_available_models_do_not_imply_authenticated_or_streaming(self):
        with patch.object(probe.shutil, 'which', return_value='pi'), \
             patch.object(probe, 'bounded_command', return_value=(0, '0.85.1')), \
             patch.object(probe, 'JsonLinesProcess') as process:
            process.return_value.request.side_effect = [
                {'model': {'id': 'test', 'provider': 'test'}}, {'messages': []}, {'models': []}]
            report = probe.probe({'tool': 'pi'})
        self.assertEqual(report['reason'], 'NO_AVAILABLE_MODEL')
        self.assertEqual(report['auth_state'], 'unknown')
        self.assertEqual(report['capabilities']['stream_events'], 'unknown')
        self.assertEqual(report['available_model_count'], 0)
        process.return_value.close.assert_called_once()

    def test_notification_backpressure_is_bounded(self):
        child = self.child("import sys,json;sys.stdin.readline();" +
            "[print(json.dumps({'method':'delta'}),flush=True) for _ in range(65)];" +
            "print(json.dumps({'id':1,'result':True}),flush=True)")
        with self.assertRaisesRegex(probe.ProbeError, 'BACKPRESSURE_LIMIT'):
            child.request({'id': 1})

    def test_unrecognized_tool_rejected_before_launch(self):
        with self.assertRaisesRegex(probe.ProbeError,'HARNESS_TOOL_UNSUPPORTED'):
            probe.probe({'tool':'arbitrary-command'})


if __name__=='__main__':
    unittest.main()
