"""Bounded native-protocol probes. No prompts, login, or credential extraction.

Executed on the granted host by bob-runtime; the returned object contains only
allowlisted observations. Official documentation is not execution evidence.
"""
import json
import os
from collections import deque
from pathlib import Path
import queue
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time

TOOLS = {
    'codex': 'app_server_stdio', 'agy': 'acp_stdio', 'pi': 'stdio_rpc',
    'claude': 'structured_cli', 'dsh': 'native_service',
    'hermes': 'http_service', 'openclaw': 'native_gateway',
}
CAPABILITIES = ('list_sessions', 'read_history', 'resume_chat', 'stream_events',
                'cancel_execution', 'read_artifacts', 'approvals')
MAX_FRAME = 1024 * 1024
MAX_TOTAL = 4 * MAX_FRAME


class ProbeError(Exception):
    pass


class JsonLinesProcess:
    """Owned persistent subprocess; never attaches to or kills external services."""
    def __init__(self, command, cwd, env=None, timeout=12):
        self.timeout = timeout
        self.records = queue.Queue(maxsize=64)
        self.stop = threading.Event()
        self.error = None
        self.total = 0
        self.events = deque()
        self.proc = subprocess.Popen(command, cwd=cwd, env=env,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            start_new_session=(os.name == 'posix'))
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()

    def _read(self):
        try:
            while not self.stop.is_set():
                line = self.proc.stdout.readline(MAX_FRAME + 1)
                if not line:
                    self.error = 'PROCESS_EXITED'
                    return
                self.total += len(line)
                if len(line) > MAX_FRAME or self.total > MAX_TOTAL:
                    self.error = 'OUTPUT_LIMIT'
                    return
                try:
                    value = json.loads(line.decode('utf-8'))
                    if not isinstance(value, dict):
                        raise ValueError()
                except (UnicodeError, ValueError):
                    self.error = 'PROTOCOL_INVALID'
                    return
                try:
                    self.records.put(value, timeout=1)
                except queue.Full:
                    self.error = 'BACKPRESSURE_LIMIT'
                    return
        except (OSError, ValueError):
            self.error = 'PIPE_CLOSED'

    def send(self, message):
        wire = json.dumps(message, ensure_ascii=False).encode('utf-8') + b'\n'
        if len(wire) > 16384:
            raise ProbeError('INPUT_LIMIT')
        try:
            self.proc.stdin.write(wire)
            self.proc.stdin.flush()
        except (OSError, ValueError):
            raise ProbeError('PIPE_CLOSED') from None

    def request(self, message):
        self.send(message)
        deadline = time.monotonic() + self.timeout
        while time.monotonic() < deadline:
            try:
                value = self.records.get(timeout=0.1)
            except queue.Empty:
                if self.error:
                    raise ProbeError(self.error)
                continue
            if value.get('id') != message['id']:
                # Keep notifications received before the acceptance response.
                if 'id' not in value:
                    if len(self.events) >= 64:
                        raise ProbeError('BACKPRESSURE_LIMIT')
                    self.events.append(value)
                continue
            if 'error' in value or value.get('success') is False:
                error = value.get('error')
                code = error.get('code') if isinstance(error, dict) else None
                if code == -32601:
                    raise ProbeError('METHOD_UNSUPPORTED')
                raise ProbeError('REQUEST_REJECTED')
            return value.get('result', value.get('data'))
        raise ProbeError('PROBE_TIMEOUT')

    def next_event(self, timeout=None):
        if self.events:
            return self.events.popleft()
        deadline = time.monotonic() + (self.timeout if timeout is None else timeout)
        while time.monotonic() < deadline:
            try:
                return self.records.get(timeout=0.1)
            except queue.Empty:
                if self.error:
                    raise ProbeError(self.error)
        raise ProbeError('PROBE_TIMEOUT')

    def close(self):
        self.stop.set()
        try:
            self.proc.stdin.close()
        except (OSError, ValueError):
            pass
        try:
            self.proc.wait(timeout=2)
        except subprocess.TimeoutExpired:
            try:
                if os.name == 'posix':
                    os.killpg(self.proc.pid, signal.SIGTERM)
                else:
                    self.proc.terminate()
                self.proc.wait(timeout=2)
            except (ProcessLookupError, subprocess.TimeoutExpired):
                if self.proc.poll() is None:
                    if os.name == 'posix':
                        os.killpg(self.proc.pid, signal.SIGKILL)
                    else:
                        self.proc.kill()
                    self.proc.wait(timeout=2)
        self.reader.join(timeout=1)
        self.proc.stdout.close()


def environment():
    env = os.environ.copy()
    home = Path.home()
    env['PATH'] = os.pathsep.join(str(home / p) for p in
        ['.local/bin', '.npm-global/bin', '.cargo/bin']) + os.pathsep + env.get('PATH', '')
    return env


def bounded_command(argv, cwd, env):
    # Temporary stdout is capped by a reader and timeout, never emitted verbatim.
    with tempfile.TemporaryFile() as output:
        process = subprocess.Popen(argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
            stdout=output, stderr=subprocess.DEVNULL, start_new_session=(os.name == 'posix'))
        deadline = time.monotonic() + 10
        try:
            while process.poll() is None:
                if time.monotonic() >= deadline or os.fstat(output.fileno()).st_size > 65536:
                    raise ProbeError('PROBE_TIMEOUT_OR_OUTPUT_LIMIT')
                time.sleep(0.05)
            output.seek(0)
            return process.returncode, output.read(65536).decode('utf-8', errors='replace')
        finally:
            if process.poll() is None:
                if os.name == 'posix':
                    os.killpg(process.pid, signal.SIGKILL)
                else:
                    process.kill()
                process.wait(timeout=2)


def probe(config):
    tool = config.get('tool')
    if tool not in TOOLS:
        raise ProbeError('HARNESS_TOOL_UNSUPPORTED')
    env = environment()
    executable = shutil.which(tool, path=env['PATH'])
    report = {
        'schema_version': 1, 'tool': tool, 'installed': executable is not None,
        'auth_state': 'unknown', 'observed_at': int(time.time() * 1000),
        'version': None, 'preferred_transport': TOOLS[tool], 'observed_transport': None,
        'probe_kind': 'read_only_no_model', 'capabilities': {c: 'unknown' for c in CAPABILITIES},
        'protocol_verified': False, 'process_reused': False, 'checks': [],
    }
    if not executable:
        # A service may be present even if its CLI isn't on this user's PATH.
        report['reason'] = 'EXECUTABLE_NOT_FOUND_SERVICE_NOT_PROBED'
        return report
    process = None
    with tempfile.TemporaryDirectory(prefix='bob-harness-probe-') as work:
        try:
            code, version = bounded_command([executable, '--version'], work, env)
            match = re.search(r'\b\d+\.\d+\.\d+(?:[-+][a-zA-Z0-9.-]+)?', version)
            report['version'] = match.group(0) if code == 0 and match else None
            if config.get('discovery_only'):
                return report
            if tool == 'codex':
                process = JsonLinesProcess([executable, 'app-server'], work, env)
                process.request({'id': 1, 'method': 'initialize', 'params': {
                    'clientInfo': {'name': 'bob-capability-probe', 'version': '1.0.1'}}})
                process.send({'method': 'initialized', 'params': {}})
                report['protocol_verified'] = True
                report['observed_transport'] = TOOLS[tool]
                account = process.request({'id': 2, 'method': 'account/read', 'params': {'refreshToken': False}})
                report['auth_state'] = 'authenticated' if isinstance(account, dict) and account.get('account') else 'login_required'
                report['checks'].append('account/read')
                threads = process.request({'id': 3, 'method': 'thread/list', 'params': {'limit': 10}})
                if isinstance(threads, dict) and isinstance(threads.get('data'), list):
                    report['capabilities']['list_sessions'] = 'supported'
                    report['sample_count'] = len(threads['data'])
                    report['checks'].append('thread/list')
                else:
                    raise ProbeError('PROTOCOL_INVALID')
                report['process_reused'] = True
            elif tool == 'pi':
                # Probe creates no lasting conversation and loads no project resources.
                process = JsonLinesProcess([executable, '--mode', 'rpc', '--no-session',
                    '--no-extensions', '--no-skills', '--no-prompt-templates', '--no-themes'], work, env)
                state = process.request({'id': 'state-1', 'type': 'get_state'})
                if not isinstance(state, dict):
                    raise ProbeError('PROTOCOL_INVALID')
                report['protocol_verified'] = True
                report['observed_transport'] = TOOLS[tool]
                report['checks'].append('get_state')
                messages = process.request({'id': 'history-1', 'type': 'get_messages'})
                if isinstance(messages, dict) and isinstance(messages.get('messages'), list):
                    report['capabilities']['read_history'] = 'supported'
                    report['history_scope'] = 'isolated_probe_session'
                    report['sample_count'] = len(messages['messages'])
                    report['checks'].append('get_messages')
                else:
                    raise ProbeError('PROTOCOL_INVALID')
                available = process.request({'id': 'models-1', 'type': 'get_available_models'})
                if not isinstance(available, dict) or not isinstance(available.get('models'), list):
                    raise ProbeError('PROTOCOL_INVALID')
                report['checks'].append('get_available_models')
                models = available['models']
                selected = state.get('model') or {}
                report['available_model_count'] = len(models)
                report['configured_model_available'] = any(
                    isinstance(m, dict) and m.get('id') == selected.get('id')
                    and m.get('provider') == selected.get('provider') for m in models)
                report['process_reused'] = True
                # Neither model configuration nor availability proves a real reply.
                report['reason'] = ('NO_AVAILABLE_MODEL' if not models else
                    'CONFIGURED_MODEL_UNAVAILABLE' if not report['configured_model_available'] else
                    'PROVIDER_AUTH_NOT_PROBED')
            elif tool == 'agy':
                code, help_text = bounded_command([executable, '--help'], work, env)
                report['advertised_flags'] = [f for f in ['--input-format', '--output-format', '--remote-control'] if f in help_text]
                process = JsonLinesProcess([executable, 'acp'], work, env)
                result = process.request({'jsonrpc': '2.0', 'id': 1, 'method': 'initialize',
                    'params': {'protocolVersion': 1, 'clientCapabilities': {},
                               'clientInfo': {'name': 'bob-capability-probe', 'version': '1.0.1'}}})
                if not isinstance(result, dict) or 'protocolVersion' not in result:
                    raise ProbeError('PROTOCOL_INVALID')
                report['protocol_verified'] = True
                report['observed_transport'] = TOOLS[tool]
                report['checks'].append('initialize')
                # Declared methods are recorded separately, never as observed support.
                caps = result.get('agentCapabilities') or {}
                report['advertised_methods'] = {'loadSession': caps.get('loadSession') is True}
                report['reason'] = 'SESSION_AND_AUTH_NOT_PROBED'
            elif tool == 'claude':
                code, raw = bounded_command([executable, 'auth', 'status', '--json'], work, env)
                try:
                    status = json.loads(raw)
                except ValueError:
                    raise ProbeError('AUTH_STATUS_UNAVAILABLE') from None
                if status.get('loggedIn') is True:
                    report['auth_state'] = 'authenticated'
                elif status.get('loggedIn') is False:
                    report['auth_state'] = 'login_required'
                report['checks'].append('auth/status')
            else:
                report['reason'] = 'SERVICE_PROBE_REQUIRED'
        except ProbeError as exc:
            report['reason'] = str(exc)
        except (OSError, ValueError, subprocess.SubprocessError):
            report['reason'] = 'PROBE_UNAVAILABLE'
        finally:
            if process:
                process.close()
                report['owned_process_exited'] = process.proc.poll() is not None
    return report


if __name__ == '__main__':
    try:
        cfg = json.loads(sys.argv[1])
        print(json.dumps(probe(cfg), ensure_ascii=False))
    except (ValueError, KeyError, ProbeError):
        print(json.dumps({'error': 'HARNESS_PROBE_INVALID'}))
        sys.exit(1)
