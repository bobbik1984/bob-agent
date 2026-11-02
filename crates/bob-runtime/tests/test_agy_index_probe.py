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

if __name__ == '__main__':
    unittest.main()
