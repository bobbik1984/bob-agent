import importlib.util
import sqlite3
from contextlib import closing
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location('agy_index_probe', Path(__file__).parents[1] / 'src' / 'agy_index_probe.py')
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)

class IndexTests(unittest.TestCase):
    def test_real_sqlite_readonly_order_limit_and_missing_history(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            p = root / 'conversation_summaries.db'
            with closing(sqlite3.connect(p)) as db:
                db.execute('CREATE TABLE conversation_summaries(conversation_id,title,workspace_uris,last_modified_time,step_count,status,parent_conversation_id)')
                for n in range(3):
                    db.execute('INSERT INTO conversation_summaries VALUES(?,?,?,?,?,?,?)',
                               (str(__import__('uuid').UUID(int=n+1)),str(n),'[]',str(n),1,'IDLE',''))
                db.commit()
            before = p.read_bytes()
            out = probe.discover(root, 2)
            self.assertEqual([s['title'] for s in out['sessions']], ['2', '1'])
            self.assertTrue(out['hasMore'])
            self.assertEqual(out['sessions'][0]['historyReason'], 'AGY_HISTORY_NOT_LOCAL')
            self.assertEqual(before, p.read_bytes())
            self.assertEqual(out['authentication'], 'unverified')

    def test_schema_and_limit_fail_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            with closing(sqlite3.connect(Path(tmp)/'conversation_summaries.db')) as db:
                db.execute('CREATE TABLE unexpected(x)')
                db.commit()
            with self.assertRaisesRegex(ValueError, 'SCHEMA_UNSUPPORTED'):
                probe.discover(tmp)
            with self.assertRaisesRegex(ValueError, 'LIMIT_INVALID'):
                probe.discover(tmp, 201)

    def test_missing_does_not_create_database(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(ValueError):
                probe.discover(tmp)
            self.assertEqual(list(Path(tmp).iterdir()), [])

    def test_agy_transcript_jsonl_makes_session_readable_and_parses_page(self):
        import json
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            sid = str(__import__('uuid').UUID(int=7))
            with closing(sqlite3.connect(root / 'conversation_summaries.db')) as db:
                db.execute('CREATE TABLE conversation_summaries(conversation_id,title,workspace_uris,last_modified_time,step_count,status,parent_conversation_id)')
                db.execute('INSERT INTO conversation_summaries VALUES(?,?,?,?,?,?,?)',
                           (sid, 'Camera Tilt Lens Shift Analysis', '["file:///home/bobbik/dsh_projects/winter-shelter"]', '2026-11-03T13:17:47Z', 4, 'IDLE', ''))
                db.commit()
            log_dir = root / 'brain' / sid / '.system_generated' / 'logs'
            log_dir.mkdir(parents=True)
            (log_dir / 'transcript_full.jsonl').write_text(
                '\n'.join([
                    json.dumps({'step_index': 0, 'type': 'USER_INPUT', 'created_at': '2026-11-03T13:14:17Z', 'content': '<USER_REQUEST>\n/plan Read-only research\n</USER_REQUEST>'}),
                    json.dumps({'step_index': 1, 'type': 'PLANNER_RESPONSE', 'created_at': '2026-11-03T13:14:22Z', 'thinking': 'Checking tilt_lens_shift', 'tool_calls': [{'name': 'run_command', 'args': {'CommandLine': 'grep -rn tilt_lens_shift scripts/'}}]}),
                    json.dumps({'step_index': 2, 'type': 'PLANNER_RESPONSE', 'created_at': '2026-11-03T13:17:34Z', 'content': 'tests/test_build_session.gd:49 [BREAKS] Hardcodes base := deg_to_rad(13.5).'}),
                ]),
                encoding='utf-8',
            )
            out = probe.discover(root, 10)
            self.assertTrue(out['sessions'][0]['historyReadable'])
            self.assertIsNone(out['sessions'][0]['historyReason'])
            page = probe.read_session_page({'agyDataDir': str(root)}, f'agy:{sid}')
            self.assertEqual(page['harness'], 'agy')
            self.assertEqual(page['workspace'], '/home/bobbik/dsh_projects/winter-shelter')
            self.assertEqual(len(page['messages']), 2)
            self.assertEqual(page['messages'][0]['role'], 'user')
            self.assertEqual(page['messages'][0]['text'], '/plan Read-only research')
            self.assertIn('```thinking', page['messages'][1]['text'])
            self.assertIn('tests/test_build_session.gd:49', page['messages'][1]['text'])

if __name__ == '__main__':
    unittest.main()
