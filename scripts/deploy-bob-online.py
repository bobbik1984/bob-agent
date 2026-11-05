#!/usr/bin/env python3
"""Build a pinned Bob revision and deploy to an existing local systemd service.

Run as the service account. sudo is used only for service control, backups and
file replacement. This script never restores or overwrites the live data tree.
"""
import argparse
import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import pwd
import re
import shutil
import signal
import socket
import sqlite3
import stat
import subprocess
import sys
import time
import urllib.request
import uuid


def run(args, **kwargs):
    return subprocess.run([str(a) for a in args], check=True, **kwargs)


NON_INTERACTIVE = False


def privileged(*args):
    if NON_INTERACTIVE and args[0] != 'systemctl':
        command = list(args)
        if command[0] == 'chown':
            # Copies made in this mode belong to the service account already.
            target = Path(command[-1])
            if any(p.stat().st_uid != os.getuid() for p in [target, *target.rglob('*')]):
                raise RuntimeError('Probe copy ownership is not the service account')
            return
        if command[0] == 'cp':
            command.insert(2, '--no-preserve=ownership')
        if command[0] == 'install':
            for option in ('-o', '-g'):
                if option in command:
                    index = command.index(option)
                    del command[index:index + 2]
        if command[0] not in ('cp', 'mv', 'install'):
            raise RuntimeError('Unsupported unprivileged file operation')
        return run(command, stdout=subprocess.DEVNULL)
    return run(['sudo', '-n', *args], stdout=subprocess.DEVNULL)


def sha(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def http_index_sha(web):
    html = (web / 'index.html').read_text()
    marker = '<script>window.__BOB_NODE_RUNTIME__=true;</script>'
    injected = html.replace('<head>', '<head>' + marker, 1) if '<head>' in html else marker + html
    return hashlib.sha256(injected.encode()).hexdigest()


def local_http_sha():
    client = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with client.open('http://127.0.0.1:53020/', timeout=5) as response:
        if response.status != 200:
            raise RuntimeError('Existing local HTTP service is not healthy')
        return hashlib.sha256(response.read()).hexdigest()


def unit_value(service, key):
    return subprocess.check_output(
        ['systemctl', 'show', service, '-p', key, '--value'], text=True).strip()


def inspect_service(root, service):
    if unit_value(service, 'ActiveState') != 'active':
        raise RuntimeError('Existing service is not active; no changes made')
    account = unit_value(service, 'User') or 'root'
    if pwd.getpwuid(os.getuid()).pw_name != account:
        raise RuntimeError('Run this script as the existing service account')
    pid = unit_value(service, 'MainPID')
    proc = Path('/proc') / pid
    args = (proc / 'cmdline').read_bytes().decode().split('\0')
    env = dict(item.split('=', 1) for item in
               (proc / 'environ').read_bytes().decode().split('\0') if '=' in item)
    cwd = Path(os.readlink(proc / 'cwd')).resolve()

    def setting(flag, key, default):
        return args[args.index(flag) + 1] if flag in args else env.get(key, default)

    data = Path(setting('--data-dir', 'BOB_NODE_DATA_DIR', str(cwd / 'bob-node-data')))
    web = Path(setting('--static-dir', 'BOB_NODE_STATIC_DIR', ''))
    bind = setting('--bind', 'BOB_NODE_BIND', '0.0.0.0:53020')
    binary = root / 'target/release/bob-node'
    if cwd != root or data.resolve() != root / 'data' or web.resolve() != root / 'web_dist':
        raise RuntimeError('Service paths differ from the verified deployment layout')
    if bind != '127.0.0.1:53020':
        raise RuntimeError('Unexpected service bind address; no changes made')
    if os.readlink(proc / 'exe') != str(binary):
        raise RuntimeError('Running executable differs from the verified binary path')
    if any(path.is_symlink() for path in (binary, data, web)):
        raise RuntimeError('Symlink deployment layout needs individual review')
    if sha(proc / 'exe') != sha(binary):
        raise RuntimeError('On-disk executable differs from the running executable')
    return {'pid': pid, 'binary': str(binary), 'data': str(data), 'web': str(web),
            'binarySha256': sha(binary), 'webSha256': sha(web / 'index.html'), 'httpSha256': local_http_sha()}


def check_data(data):
    # A copied symlink could make a candidate probe write into the live tree.
    for directory, dirs, files in os.walk(data, followlinks=False):
        for name in dirs + files:
            item = Path(directory) / name
            if item.is_symlink() or not (stat.S_ISDIR(item.stat().st_mode) or stat.S_ISREG(item.stat().st_mode)):
                raise RuntimeError('Data contains links or special files; no automatic deployment')
    db = Path(data) / 'kernel.sqlite'
    with contextlib.closing(sqlite3.connect(db.as_uri() + '?mode=ro', uri=True)) as conn:
        if conn.execute('PRAGMA integrity_check').fetchone()[0] != 'ok':
            raise RuntimeError('Existing database integrity check failed')
        count = conn.execute(
            "SELECT COUNT(*) FROM executions WHERE status IN ('dispatching','running')"
        ).fetchone()[0]
        if count:
            raise RuntimeError('Bob has active executions; wait for them to finish')
    for state_file in (Path(data) / 'claude-harness/sessions').glob('*.json'):
        if json.loads(state_file.read_text()).get('active_request_id'):
            raise RuntimeError('Claude has an active request; wait for it to finish')


def build(work, cache, repository, commit):
    source = work / 'source'
    log = work / 'build.log'
    env = os.environ.copy()
    env['PATH'] = str(Path.home() / '.cargo/bin') + os.pathsep + env.get('PATH', '')
    for tool in ('git', 'node', 'pnpm', 'cargo'):
        if not shutil.which(tool, path=env['PATH']):
            raise RuntimeError('Required build tool is missing: ' + tool)
    major = int(subprocess.check_output(['node', '--version'], env=env, text=True).strip().lstrip('v').split('.')[0])
    if major < 20:
        raise RuntimeError('Node.js 20 or later is required; no service changes made')
    print('Building pinned source in an isolated directory; build log:', log, flush=True)
    with log.open('w') as output:
        run(['git', 'init', source], stdout=output, stderr=output, env=env)
        run(['git', '-C', source, 'fetch', '--depth=1', repository, commit], stdout=output, stderr=output, env=env)
        run(['git', '-C', source, 'checkout', '--detach', 'FETCH_HEAD'], stdout=output, stderr=output, env=env)
        actual = subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip()
        if actual != commit:
            raise RuntimeError('Fetched source does not match the pinned commit')
        frontend = work / 'frontend'
        shutil.copytree(source / 'apps/bob-app', frontend)
        env['CARGO_TARGET_DIR'] = str(cache / 'cargo-target')
        env['BOB_FRONTEND_DIST'] = str(work / 'web_dist')
        # All dependency/build caches are outside the authoritative source tree.
        print('Preparing locked frontend dependencies.', flush=True)
        run(['pnpm', '--dir', frontend, 'install', '--frozen-lockfile', '--store-dir', cache / 'pnpm-store'], env=env, stdout=output, stderr=output)
        print('Building the web application.', flush=True)
        run(['pnpm', '--dir', frontend, 'run', 'build'], env=env, stdout=output, stderr=output)
        print('Building the production Bob service.', flush=True)
        run(['cargo', 'build', '--locked', '--release', '-p', 'bob-node'], cwd=source, env=env, stdout=output, stderr=output)
    candidate = work / 'bob-node'
    shutil.copy2(cache / 'cargo-target/release/bob-node', candidate)
    candidate.chmod(0o755)
    for path in (work / 'web_dist').rglob('*'):
        path.chmod(0o755 if path.is_dir() else 0o644)
    (work / 'web_dist').chmod(0o755)
    return candidate, work / 'web_dist'


def probe(binary, data, web, log, strict=True):
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    env = os.environ.copy()
    for key in ('BOB_NODE_LEGACY_DB', 'BOB_NODE_PUBLIC_URL', 'BOB_NODE_DATA_DIR', 'BOB_NODE_STATIC_DIR', 'BOB_NODE_BIND'):
        env.pop(key, None)
    client = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with log.open('w') as output:
        process = subprocess.Popen([str(binary), '--bind', f'127.0.0.1:{port}',
                                    '--data-dir', str(data), '--static-dir', str(web), '--no-qr'],
                                   env=env, stdout=output, stderr=output)
        try:
            for _ in range(100):
                if process.poll() is not None:
                    raise RuntimeError('Compatibility probe exited; private log: ' + str(log))
                try:
                    with client.open(f'http://127.0.0.1:{port}/', timeout=1) as response:
                        if response.status == 200 and (not strict or hashlib.sha256(response.read()).hexdigest() == http_index_sha(web)):
                            return
                except OSError:
                    pass
                time.sleep(.2)
            raise RuntimeError('Compatibility probe timed out; private log: ' + str(log))
        finally:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()


def backup_files(root, binary, data, backup):
    privileged('install', '-d', '-m', '700', backup)
    privileged('cp', '-a', binary, backup / 'bob-node')
    privileged('cp', '-a', data, backup / 'data')
    privileged('cp', '-a', root / 'web_dist', backup / 'web_dist')


def replace_files(root, binary, candidate, web, tag):
    staged_binary = binary.with_name('bob-node.new-' + tag)
    staged_web = root / ('.web-new-' + tag)
    previous_web = root / ('.web-previous-' + tag)
    owner = binary.stat()
    privileged('install', '-m', '755', '-o', str(owner.st_uid), '-g', str(owner.st_gid), candidate, staged_binary)
    privileged('cp', '-a', web, staged_web)
    privileged('mv', '-fT', staged_binary, binary)
    privileged('mv', '-T', root / 'web_dist', previous_web)
    privileged('mv', '-T', staged_web, root / 'web_dist')


def restore_files(root, binary, backup, tag):
    # Restore only program/static files. Never replace a database with an older copy.
    staged_binary = binary.with_name('bob-node.rollback-' + tag)
    staged_web = root / ('.web-rollback-' + tag)
    privileged('cp', '-a', backup / 'bob-node', staged_binary)
    privileged('mv', '-fT', staged_binary, binary)
    privileged('cp', '-a', backup / 'web_dist', staged_web)
    if (root / 'web_dist').exists():
        privileged('mv', '-T', root / 'web_dist', root / ('.web-failed-' + tag))
    privileged('mv', '-T', staged_web, root / 'web_dist')


def verify_live(root, service, expected_binary, expected_web):
    client = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    for _ in range(60):
        try:
            if unit_value(service, 'ActiveState') != 'active':
                time.sleep(.3)
                continue
            pid = unit_value(service, 'MainPID')
            if sha(Path('/proc') / pid / 'exe') != expected_binary:
                raise RuntimeError('Running process does not match the new program')
            with client.open('http://127.0.0.1:53020/', timeout=2) as response:
                if response.status == 200 and hashlib.sha256(response.read()).hexdigest() == expected_web:
                    return pid
        except OSError:
            pass
        time.sleep(.3)
    raise RuntimeError('Live process or HTTP readback verification failed')


def deploy(root, service, work, candidate, web, initial, commit):
    binary, data = Path(initial['binary']), Path(initial['data'])
    tag = time.strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:8]
    backup = root / ('operator-backups' if NON_INTERACTIVE else 'backups') / ('quota-device-' + tag)
    # Renew the operator's sudo timestamp in their own terminal, without recording it.
    if not NON_INTERACTIVE:
        run(['sudo', '-v'])
    current = inspect_service(root, service)
    if current != initial:
        raise RuntimeError('The running deployment changed while building; nothing replaced')
    check_data(data)
    stopped = False
    replacing = False
    try:
        print('Stopping Bob for a consistent backup and compatibility probes.', flush=True)
        privileged('systemctl', 'stop', service)
        stopped = True
        check_data(data)
        backup_files(root, binary, data, backup)
        print('Consistent backup:', backup, flush=True)
        test_data = work / 'compatibility-data'
        privileged('cp', '-a', backup / 'data', test_data)
        privileged('chown', '-R', str(os.getuid()) + ':' + str(os.getgid()), test_data)
        old_binary = work / 'old-bob-node'
        shutil.copy2(binary, old_binary)
        probe(candidate, test_data, web, work / 'candidate-probe.log')
        probe(old_binary, test_data, web, work / 'old-reader-probe.log', strict=False)
        expected_binary, expected_web = sha(candidate), sha(web / 'index.html')
        replacing = True
        replace_files(root, binary, candidate, web, tag)
        privileged('systemctl', 'start', service)
        pid = verify_live(root, service, expected_binary, http_index_sha(web))
        result = {'deployed': True, 'codeCommit': commit, 'pid': pid,
                  'binarySha256': expected_binary, 'webSha256': expected_web, 'httpSha256': http_index_sha(web),
                  'backup': str(backup), 'localHttpReadback': 'passed',
                  'connectionsMigrated': False, 'publicBrowserVerified': False}
        # No database restoration is performed, even if later program checks fail.
        stopped = False
        receipt = work / 'deployment.json'
        receipt.write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(result, indent=2), flush=True)
        return result
    except BaseException:
        if stopped:
            if replacing:
                privileged('systemctl', 'stop', service)
                restore_files(root, binary, backup, tag)
                privileged('systemctl', 'reset-failed', service)
            privileged('systemctl', 'start', service)
            verify_live(root, service, initial['binarySha256'], initial['httpSha256'])
            print('Previous program restarted and HTTP verified. Live data was not restored from a backup.', flush=True)
        raise


def main():
    def interrupted(signum, frame):
        raise KeyboardInterrupt()
    signal.signal(signal.SIGHUP, interrupted)
    signal.signal(signal.SIGTERM, interrupted)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', required=True, type=Path)
    parser.add_argument('--service', default='bob-online.service')
    parser.add_argument('--repository', required=True)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--non-interactive', action='store_true', help='Use service-account file access and narrowly delegated service controls')
    options = parser.parse_args()
    global NON_INTERACTIVE
    NON_INTERACTIVE = options.non_interactive
    if not re.fullmatch(r'[0-9a-f]{40}', options.commit):
        parser.error('commit must be a full lowercase Git SHA')
    if not options.repository.startswith('https://github.com/'):
        parser.error('repository must be an HTTPS GitHub source URL')
    if platform.machine() != 'x86_64':
        parser.error('This release was validated for x86_64')
    os.umask(0o077)
    root = options.root.resolve(strict=True)
    if root == Path('/'):
        parser.error('Installation root must not be /')
    cache = Path.home() / '.cache/bob-online-deploy'
    cache.mkdir(parents=True, exist_ok=True, mode=0o700)
    with (cache / 'deploy.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        initial = inspect_service(root, options.service)
        check_data(Path(initial['data']))
        if NON_INTERACTIVE:
            for path in (root, root / 'target/release'):
                if not os.access(path, os.W_OK | os.X_OK):
                    raise RuntimeError('Service-account write permission is missing: ' + str(path))
            for action in ('start', 'stop', 'reset-failed'):
                run(['sudo', '-n', '-l', shutil.which('systemctl'), action, options.service], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        work = cache / ('release-' + options.commit[:12] + '-' + uuid.uuid4().hex[:8])
        work.mkdir(mode=0o700)
        # Require space for two private data copies plus the isolated build.
        size = sum(p.stat().st_size for p in Path(initial['data']).rglob('*') if p.is_file())
        shared_disk = cache.stat().st_dev == root.stat().st_dev
        needed = (3 if shared_disk else 2) * size + 4 * 1024**3
        if shutil.disk_usage(cache).free < needed:
            raise RuntimeError('Insufficient free build/compatibility space')
        if shutil.disk_usage(root).free < size + 512 * 1024**2:
            raise RuntimeError('Insufficient free backup space')
        candidate, web = build(work, cache, options.repository, options.commit)
        deploy(root, options.service, work, candidate, web, initial, options.commit)


if __name__ == '__main__':
    try:
        main()
    except (Exception, KeyboardInterrupt) as error:
        print('DEPLOYMENT STOPPED:', type(error).__name__, str(error), file=sys.stderr)
        sys.exit(1)
