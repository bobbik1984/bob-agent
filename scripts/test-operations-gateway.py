import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


gateway = module('gateway', 'run-remote-bob-operation.py')
inspector = module('inspector', 'inspect-bob-online.py')
bootstrap = module('bootstrap', 'bootstrap-bob-operations.py')


def config():
    return {'host': 'server.example', 'user': 'ubuntu', 'port': 22,
            'fingerprint': 'SHA256:' + 'A' * 43, 'root': '/opt/test-bob',
            'private_key': '-----BEGIN OPENSSH PRIVATE KEY-----\nDO_NOT_LOG_THIS_FIXTURE\n-----END OPENSSH PRIVATE KEY-----'}


class GatewayTests(unittest.TestCase):
    def test_rejects_shell_injection_before_network_use(self):
        for field, value in [('host', 'server.example; touch /tmp/bad'),
                             ('host', '-oProxyCommand=bad'), ('user', 'ubuntu\nroot'),
                             ('port', 0), ('fingerprint', 'accept-any'), ('root', '/tmp/..'), ('root', '/')]:
            candidate = config()
            candidate[field] = value
            with self.assertRaises(gateway.GatewayError):
                gateway.validate_config(candidate)

    def test_mismatched_host_key_never_authenticates_and_removes_private_file(self):
        runs = []
        keys = []
        def run(args, **kwargs):
            runs.append(list(map(str, args)))
            if args[0] == 'ssh-keygen':
                key = Path(args[-1])
                self.assertEqual(key.stat().st_mode & 0o777, 0o600)
                keys.append(key)
            return subprocess.CompletedProcess(args, 0)
        with tempfile.TemporaryDirectory() as folder, patch.object(gateway.subprocess, 'run', run), patch.object(
                gateway.subprocess, 'check_output', return_value='256 SHA256:' + 'B' * 43 + ' server (ED25519)\n'):
            with self.assertRaisesRegex(gateway.GatewayError, 'does not match'):
                gateway.run_operation(config(), 'inspect', '', 'owner/project', Path(folder), Path(folder) / 'receipt')
        self.assertFalse(any(call[0] == 'ssh' for call in runs))
        self.assertTrue(keys)
        self.assertFalse(keys[0].exists())

    def test_validated_operation_logs_only_redacted_result(self):
        runs = []
        def output(args, **kwargs):
            if args[0] == 'ssh-keygen':
                return '256 SHA256:' + 'A' * 43 + ' server (ED25519)\n'
            return '/tmp/tmp.bobfixture123\n'
        def run(args, **kwargs):
            runs.append(list(map(str, args)))
            if args[0] == 'ssh' and 'python3' in args[-1]:
                kwargs['stdout'].write('{"readOnly": true}\n')
            return subprocess.CompletedProcess(args, 0)
        configuration = config()
        with tempfile.TemporaryDirectory() as folder, patch.object(gateway.subprocess, 'run', run), patch.object(
                gateway.subprocess, 'check_output', output), patch('sys.stdout', new_callable=io.StringIO) as logged:
            gateway.run_operation(configuration, 'inspect', '', 'owner/project', Path(folder), Path(folder) / 'result')
            self.assertIn('readOnly', logged.getvalue())
            self.assertNotIn('DO_NOT_LOG', logged.getvalue())
        self.assertEqual(configuration, {})
        self.assertNotIn('DO_NOT_LOG', str(runs))
        self.assertTrue(any('StrictHostKeyChecking=yes' in call for call in runs))

    def test_source_sha_is_required_before_deployment(self):
        with self.assertRaisesRegex(gateway.GatewayError, 'exact source'):
            gateway.run_operation(config(), 'deploy', 'main; bad', 'owner/project', Path('.'), Path('unused'))

    def test_one_time_sudo_policy_is_limited_to_three_bob_service_actions(self):
        calls = []
        class Identity:
            pw_uid = 1000
            pw_gid = 1000
        with tempfile.TemporaryDirectory() as folder, patch.object(bootstrap, 'run', lambda args, **kw: calls.append(list(map(str, args)))), patch.object(
                bootstrap.shutil, 'which', return_value='/usr/bin/systemctl'), patch.object(
                bootstrap.pwd, 'getpwnam', return_value=Identity()), patch.object(
                bootstrap.os, 'access', return_value=False), patch.object(Path, 'exists', return_value=False):
            bootstrap.configure_service_control(Path(folder) / 'app', 'ubuntu', Path(folder))
            policy = (Path(folder) / 'sudoers-policy').read_text()
        self.assertNotIn('NOPASSWD: ALL', policy)
        self.assertEqual(policy.count('bob-online.service'), 3)
        ownership = [call for call in calls if 'chown' in call]
        self.assertEqual(len(ownership), 2)
        self.assertTrue(all('-R' not in call for call in ownership))
        self.assertTrue(all('/data' not in call[-1] for call in ownership))

    def test_existing_different_sudo_policy_is_never_overwritten(self):
        calls = []
        with tempfile.TemporaryDirectory() as folder, patch.object(bootstrap, 'run', lambda args, **kw: calls.append(list(map(str, args)))), patch.object(
                bootstrap.shutil, 'which', return_value='/usr/bin/systemctl'), patch.object(Path, 'exists', return_value=True), patch.object(
                bootstrap.subprocess, 'run', return_value=subprocess.CompletedProcess([], 1)):
            with self.assertRaisesRegex(bootstrap.SetupError, 'not overwritten'):
                bootstrap.configure_service_control(Path(folder) / 'app', 'ubuntu', Path(folder))
        self.assertFalse(any('install' in call or 'chown' in call for call in calls))

    def test_origin_redacts_all_userinfo_query_fragment_and_path(self):
        self.assertEqual(inspector.endpoint_origin('https://user:password@server.example:443/private-token?q=secret#token'), 'https://server.example:443')


if __name__ == '__main__':
    unittest.main()
