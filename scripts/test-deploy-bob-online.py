import hashlib
import importlib.util
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('deploy_bob', Path(__file__).with_name('deploy-bob-online.py'))
deploy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(deploy)


class DeploymentTests(unittest.TestCase):
    def layout(self, base):
        root = base / 'live'
        binary = root / 'target/release/bob-node'
        binary.parent.mkdir(parents=True)
        binary.write_text('old program')
        data = root / 'data'
        data.mkdir()
        (data / 'private-state').write_text('keep private data')
        web = root / 'web_dist'
        web.mkdir()
        (web / 'index.html').write_text('<html><head></head><body>old</body></html>')
        candidate = base / 'new-program'
        candidate.write_text('new program')
        candidate_web = base / 'new-web'
        candidate_web.mkdir()
        (candidate_web / 'index.html').write_text('<html><head></head><body>new</body></html>')
        return root, binary, data, candidate, candidate_web

    def local_privileged(self, *args):
        # Exercise the same cp/install/mv arguments, restricted to temporary fixtures.
        return subprocess.run([str(arg) for arg in args], check=True, stdout=subprocess.DEVNULL)

    def test_success_and_rollback_preserve_new_live_data(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(deploy, 'privileged', self.local_privileged):
            root, binary, data, candidate, web = self.layout(Path(folder))
            backup = root / 'backups/test'
            deploy.backup_files(root, binary, data, backup)
            deploy.replace_files(root, binary, candidate, web, 'fixture')
            self.assertEqual(binary.read_text(), 'new program')
            self.assertIn('new', (root / 'web_dist/index.html').read_text())
            (data / 'private-state').write_text('new data after restart')
            deploy.restore_files(root, binary, backup, 'fixture')
            self.assertEqual(binary.read_text(), 'old program')
            self.assertIn('old', (root / 'web_dist/index.html').read_text())
            self.assertEqual((data / 'private-state').read_text(), 'new data after restart')
            self.assertEqual((backup / 'data/private-state').read_text(), 'keep private data')

    def test_rollback_after_partial_program_replacement(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(deploy, 'privileged', self.local_privileged):
            root, binary, data, candidate, web = self.layout(Path(folder))
            backup = root / 'backups/test'
            deploy.backup_files(root, binary, data, backup)
            def fail_once(*args):
                if args[0] == 'mv' and str(args[-1]) == str(root / 'web_dist'):
                    raise OSError('simulated web rename failure')
                return self.local_privileged(*args)
            with patch.object(deploy, 'privileged', fail_once):
                with self.assertRaises(OSError):
                    deploy.replace_files(root, binary, candidate, web, 'fixture')
            deploy.restore_files(root, binary, backup, 'fixture')
            self.assertEqual(binary.read_text(), 'old program')
            self.assertIn('old', (root / 'web_dist/index.html').read_text())
            self.assertEqual((data / 'private-state').read_text(), 'keep private data')

    def test_checks_block_live_execution_and_symlinked_data(self):
        with tempfile.TemporaryDirectory() as folder:
            data = Path(folder)
            with sqlite3.connect(data / 'kernel.sqlite') as db:
                db.execute('CREATE TABLE executions(status TEXT)')
                db.execute("INSERT INTO executions VALUES('running')")
            with self.assertRaisesRegex(RuntimeError, 'active executions'):
                deploy.check_data(data)
            with sqlite3.connect(data / 'kernel.sqlite') as db:
                db.execute("UPDATE executions SET status='completed'")
            deploy.check_data(data)
            (data / 'link').symlink_to(data / 'kernel.sqlite')
            with self.assertRaisesRegex(RuntimeError, 'links or special'):
                deploy.check_data(data)

    def test_noninteractive_file_operations_need_no_root_and_preserve_data(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(deploy, 'NON_INTERACTIVE', True):
            root, binary, data, candidate, web = self.layout(Path(folder))
            backup = root / 'operator-backups/test'
            deploy.backup_files(root, binary, data, backup)
            deploy.replace_files(root, binary, candidate, web, 'fixture')
            self.assertEqual(binary.read_text(), 'new program')
            deploy.restore_files(root, binary, backup, 'fixture')
            self.assertEqual(binary.read_text(), 'old program')
            self.assertEqual((data / 'private-state').read_text(), 'keep private data')

    def test_http_hash_preserves_crlf_and_utf8(self):
        with tempfile.TemporaryDirectory() as folder:
            web = Path(folder)
            html = '<html>\r\n<head></head><body>测试</body>\r\n</html>'
            (web / 'index.html').write_bytes(html.encode('utf-8'))
            expected = html.replace('<head>', '<head><script>window.__BOB_NODE_RUNTIME__=true;</script>', 1)
            self.assertEqual(deploy.http_index_sha(web), hashlib.sha256(expected.encode('utf-8')).hexdigest())

    def test_http_hash_includes_bob_runtime_marker(self):
        with tempfile.TemporaryDirectory() as folder:
            web = Path(folder)
            (web / 'index.html').write_text('<html><head></head></html>')
            self.assertNotEqual(deploy.sha(web / 'index.html'), deploy.http_index_sha(web))


if __name__ == '__main__':
    unittest.main()
