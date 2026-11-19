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

    def test_disconnected_running_cli_session_flags_interrupted_and_resumable_and_decodes_uri(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            sid = str(__import__('uuid').UUID(int=99))
            convs = root / 'conversations'
            convs.mkdir(parents=True)
            (convs / f'{sid}.db').write_bytes(b'protobuf-placeholder')
            with closing(sqlite3.connect(root / 'conversation_summaries.db')) as db:
                db.execute('CREATE TABLE conversation_summaries(conversation_id,title,workspace_uris,last_modified_time,step_count,status,parent_conversation_id)')
                db.execute(
                    'INSERT INTO conversation_summaries VALUES(?,?,?,?,?,?,?)',
                    (sid, 'PuTTY Flight Dev', '["file:///home/ubuntu/my%20app"]', '2026-11-14T08:00:00Z', 3, 'RUNNING', ''),
                )
                db.commit()
            out = probe.discover(root, 10)
            row = out['sessions'][0]
            self.assertEqual(row['historyReason'], 'AGY_HISTORY_CODEC_UNSUPPORTED')
            self.assertFalse(row['historyReadable'])
            self.assertTrue(row['cliInterrupted'])
            self.assertTrue(row['resumable'])
            page = probe.read_session_page({'agyDataDir': str(root)}, f'agy:{sid}')
            self.assertEqual(page['workspace'], '/home/ubuntu/my app')
            self.assertTrue(page['cli_interrupted'])
            self.assertGreaterEqual(len(page['messages']), 1)

    def test_discover_multi_excludes_unowned_opt_runtime_sessions(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            sid_opt_ql = str(__import__('uuid').UUID(int=201))
            sid_opt_qb = str(__import__('uuid').UUID(int=202))
            sid_oracle_no_uri = str(__import__('uuid').UUID(int=203))
            sid_dev_ql = str(__import__('uuid').UUID(int=204))
            sid_bob_opt = str(__import__('uuid').UUID(int=205))
            with closing(sqlite3.connect(root / 'conversation_summaries.db')) as db:
                db.execute('CREATE TABLE conversation_summaries(conversation_id,title,workspace_uris,last_modified_time,step_count,status,parent_conversation_id)')
                db.executemany(
                    'INSERT INTO conversation_summaries VALUES(?,?,?,?,?,?,?)',
                    [
                        (sid_opt_ql, "你是一名顶尖量化与主观交易员，代号'硅基先知 (Silicon Oracle)'，正在 Quant Lab 赛马竞技场中执行交易。", '["file:///opt/quant_lab"]', '2026-11-17T03:00:00Z', 4, 'IDLE', ''),
                        (sid_opt_qb, 'Arena Run', '["file:///opt/quant_bot"]', '2026-11-17T02:00:00Z', 2, 'IDLE', ''),
                        (sid_oracle_no_uri, "硅基先知 · Quant Lab 赛马竞技场定时执行", '[]', '2026-11-17T01:30:00Z', 2, 'IDLE', ''),
                        (sid_dev_ql, 'Quant Lab 核心回测修复', '["file:///home/ubuntu/work/code/quant_lab"]', '2026-11-17T01:00:00Z', 8, 'IDLE', ''),
                        (sid_bob_opt, 'User-launched Opt Check', '["file:///opt/quant_lab"]', '2026-11-17T00:30:00Z', 2, 'IDLE', ''),
                    ],
                )
                db.commit()

            orig_streamed = probe._sessions_with_bob_streams
            probe._sessions_with_bob_streams = lambda: {f'agy:{sid_bob_opt}'}
            try:
                res = probe.discover_multi({'agyDataDir': str(root)})
            finally:
                probe._sessions_with_bob_streams = orig_streamed

            ids = [item['id'] for item in res['sessions']]
            self.assertNotIn(f'agy:{sid_opt_ql}', ids)
            self.assertNotIn(f'agy:{sid_opt_qb}', ids)
            self.assertNotIn(f'agy:{sid_oracle_no_uri}', ids)
            self.assertIn(f'agy:{sid_dev_ql}', ids)
            self.assertIn(f'agy:{sid_bob_opt}', ids)

    def test_make_image_preview_resizes_and_compresses_and_original_bypasses(self):
        try:
            from PIL import Image
        except ImportError:
            self.skipTest('Pillow not installed')
        import base64, io, random
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            ws = root / 'ws'
            ws.mkdir()
            img_path = ws / 'big.png'
            rng = random.Random(42)
            raw_rgb = bytes(rng.getrandbits(8) for _ in range(1600 * 1200 * 3))
            Image.frombytes('RGB', (1600, 1200), raw_rgb).save(img_path, format='PNG')
            raw_bytes = img_path.read_bytes()
            self.assertGreater(len(raw_bytes), 1024 * 1024)

            preview_bytes, meta = probe._make_image_preview(raw_bytes, 'png')
            self.assertIsNotNone(meta)
            self.assertTrue(meta['preview_compressed'])
            self.assertLessEqual(len(preview_bytes), 1024 * 1024)
            self.assertLessEqual(max(meta['preview_width'], meta['preview_height']), 1000)
            self.assertEqual(meta['original_width'], 1600)
            self.assertEqual(meta['original_height'], 1200)
            self.assertEqual(meta['original_bytes'], len(raw_bytes))

            with Image.open(io.BytesIO(preview_bytes)) as pimg:
                self.assertLessEqual(max(pimg.size), 1000)

            small_buf = io.BytesIO()
            Image.new('RGB', (640, 480), (10, 20, 30)).save(small_buf, format='PNG')
            small_bytes = small_buf.getvalue()
            untouched, untouched_meta = probe._make_image_preview(small_bytes, 'png')
            self.assertIsNone(untouched_meta)
            self.assertEqual(untouched, small_bytes)

    def test_native_file_references_strips_line_number_and_fragment_suffixes(self):
        refs = probe._native_file_references({
            'role': 'assistant',
            'text': 'See [stage](/home/u/ws/CURRENT_STAGE.md:21) and [other](/home/u/ws/README.md#L10-L20) and [col](/home/u/ws/main.py:12:4).',
        })
        self.assertIn('/home/u/ws/CURRENT_STAGE.md', refs)
        self.assertIn('/home/u/ws/README.md', refs)
        self.assertIn('/home/u/ws/main.py', refs)

if __name__ == '__main__':
    unittest.main()
