#!/usr/bin/env python3
"""One-time owner setup of encrypted SSH configuration for manual Actions jobs.

Run in the existing server terminal as the Bob service account. This does not
install a daemon, deploy Bob, or print passwords, OAuth tokens, or private keys.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import pwd
import re
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request
import uuid


class SetupError(Exception):
    pass


def run(arguments, **kwargs):
    return subprocess.run([str(value) for value in arguments], check=True, **kwargs)


def download(url, destination):
    request = urllib.request.Request(url, headers={'User-Agent': 'bob-operations-setup'})
    with urllib.request.urlopen(request, timeout=90) as response, destination.open('wb') as target:
        shutil.copyfileobj(response, target)


def github_cli(work):
    existing = shutil.which('gh')
    if existing:
        version = subprocess.check_output([existing, '--version'], text=True).splitlines()[0]
        match = re.search(r'(\d+)\.(\d+)\.', version)
        if match and tuple(map(int, match.groups())) >= (2, 40):
            return existing
    print('Downloading the official GitHub CLI to this private setup directory.', flush=True)
    request = urllib.request.Request('https://api.github.com/repos/cli/cli/releases/latest',
                                     headers={'User-Agent': 'bob-operations-setup'})
    with urllib.request.urlopen(request, timeout=30) as response:
        release = json.load(response)
    assets = {item['name']: item['browser_download_url'] for item in release['assets']}
    archive_name = next((name for name in assets if name.endswith('_linux_amd64.tar.gz')), None)
    checksum_name = next((name for name in assets if name.endswith('_checksums.txt')), None)
    if not archive_name or not checksum_name:
        raise SetupError('Official GitHub CLI assets were not found')
    archive, checksums = work / 'gh.tar.gz', work / 'checksums.txt'
    download(assets[archive_name], archive)
    download(assets[checksum_name], checksums)
    expected = next((line.split()[0] for line in checksums.read_text().splitlines()
                     if len(line.split()) == 2 and line.split()[1].lstrip('*') == archive_name), None)
    if not expected or hashlib.sha256(archive.read_bytes()).hexdigest() != expected:
        raise SetupError('GitHub CLI checksum verification failed')
    binary = work / 'gh'
    with tarfile.open(archive) as package:
        matches = [member for member in package if member.isfile() and member.name.endswith('/bin/gh')]
        if len(matches) != 1 or matches[0].size > 100 * 1024 * 1024:
            raise SetupError('Unexpected GitHub CLI archive')
        with package.extractfile(matches[0]) as source, binary.open('wb') as target:
            shutil.copyfileobj(source, target)
    binary.chmod(0o700)
    return str(binary)


def unit_value(service, key):
    return subprocess.check_output(['systemctl', 'show', service, '-p', key, '--value'], text=True).strip()


def configure_service_control(root, account, work):
    systemctl = Path(shutil.which('systemctl')).resolve()
    policy = work / 'sudoers-policy'
    rule = account + ' ALL=(root) NOPASSWD: ' + ', '.join(
        str(systemctl) + ' ' + action + ' bob-online.service'
        for action in ('start', 'stop', 'reset-failed')) + '\n'
    policy.write_text(rule)
    target = Path('/etc/sudoers.d/bob-agent-service-control')
    run(['sudo', '-v'])
    run(['sudo', '-n', 'visudo', '-cf', policy], stdout=subprocess.DEVNULL)
    if target.exists():
        matched = subprocess.run(['sudo', '-n', 'cmp', '-s', policy, target],
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if matched.returncode:
            raise SetupError('Existing Bob sudo policy differs; it was not overwritten')
    else:
        run(['sudo', '-n', 'install', '-m', '440', policy, target])
    # Only these two program/static replacement directories may need an ownership
    # adjustment. Do not recursively change data, vault, backup or source ownership.
    identity = pwd.getpwnam(account)
    for directory in (root, root / 'target/release'):
        if not os.access(directory, os.W_OK | os.X_OK):
            run(['sudo', '-n', 'chown', str(identity.pw_uid) + ':' + str(identity.pw_gid), directory])
            run(['sudo', '-n', 'chmod', 'u+rwx', directory])
    for action in ('start', 'stop', 'reset-failed'):
        run(['sudo', '-n', '-l', systemctl, action, 'bob-online.service'],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repository', required=True)
    parser.add_argument('--host', required=True)
    parser.add_argument('--port', type=int, default=22)
    parser.add_argument('--fingerprint', required=True)
    parser.add_argument('--root', required=True, type=Path)
    parser.add_argument('--configure-service-control', action='store_true')
    arguments = parser.parse_args()
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', arguments.repository):
        parser.error('Invalid repository')
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9.-]{0,252}', arguments.host) or not 1 <= arguments.port <= 65535:
        parser.error('Invalid SSH destination')
    if not re.fullmatch(r'SHA256:[A-Za-z0-9+/]{43}', arguments.fingerprint):
        parser.error('Owner-provided ED25519 fingerprint is required')
    if os.geteuid() == 0:
        parser.error('Run as the existing Bob service account, not root')
    account = pwd.getpwuid(os.getuid()).pw_name
    root = arguments.root.resolve(strict=True)
    if unit_value('bob-online.service', 'User') != account:
        raise SetupError('Current login is not the Bob service account')
    if Path(unit_value('bob-online.service', 'WorkingDirectory')).resolve() != root:
        raise SetupError('Unexpected Bob installation root')
    observed = subprocess.check_output(['ssh-keygen', '-lf', '/etc/ssh/ssh_host_ed25519_key.pub', '-E', 'sha256'], text=True).split()[1]
    if observed != arguments.fingerprint:
        raise SetupError('Server fingerprint differs from the owner-provided value')
    os.umask(0o077)
    with tempfile.TemporaryDirectory(prefix='bob-owner-setup-') as folder:
        work = Path(folder)
        gh = github_cli(work)
        env = os.environ.copy()
        # Keep newly authorized CLI credentials only in this private temporary
        # directory. Existing injected credentials, if present, are not replaced.
        env['GH_CONFIG_DIR'] = str(work / 'github-auth')
        if not env.get('GH_TOKEN') and not env.get('GITHUB_TOKEN'):
            print('Authorize GitHub CLI once using the device login page it displays.', flush=True)
            run([gh, 'auth', 'login', '--hostname', 'github.com', '--git-protocol', 'ssh', '--skip-ssh-key',
                 '--web', '--insecure-storage', '--scopes', 'repo'], env=env)
        admin = subprocess.check_output([gh, 'api', 'repos/' + arguments.repository,
                                        '--jq', '.permissions.admin'], env=env, text=True).strip()
        if admin != 'true':
            raise SetupError('GitHub authorization lacks repository administration permission')
        names = subprocess.check_output([gh, 'secret', 'list', '--repo', arguments.repository,
                                         '--json', 'name', '--jq', '.[].name'], env=env, text=True).splitlines()
        if 'BOB_DEPLOY_SSH_CONFIG' in names:
            print(json.dumps({'setupComplete': False, 'configurationAlreadyPresent': True,
                              'next': 'Agent should inspect the existing gateway before any replacement'}), flush=True)
            return
        if arguments.configure_service_control:
            print('Configuring passwordless control of only the Bob service.', flush=True)
            configure_service_control(root, account, work)
        ssh = Path.home() / '.ssh'
        ssh.mkdir(mode=0o700, exist_ok=True)
        authorized = ssh / 'authorized_keys'
        if ssh.is_symlink() or authorized.is_symlink() or (authorized.exists() and (not authorized.is_file() or authorized.stat().st_uid != os.getuid())):
            raise SetupError('SSH authorization uses links; individual review is required')
        ssh.chmod(0o700)
        key = work / 'operations-key'
        comment = 'bob-operations-' + uuid.uuid4().hex[:12]
        run(['ssh-keygen', '-q', '-t', 'ed25519', '-N', '', '-f', key, '-C', comment], stdout=subprocess.DEVNULL)
        public_key = key.with_suffix('.pub').read_text().strip()
        with authorized.open('a') as target:
            target.write('\nrestrict ' + public_key + '\n')
        authorized.chmod(0o600)
        config = {'host': arguments.host, 'port': arguments.port, 'user': account,
                  'fingerprint': arguments.fingerprint, 'private_key': key.read_text(), 'root': str(root)}
        try:
            run([gh, 'secret', 'set', 'BOB_DEPLOY_SSH_CONFIG', '--repo', arguments.repository],
                input=json.dumps(config), env=env, text=True,
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except BaseException:
            # Revoke only the new bootstrap key if encrypted secret storage fails.
            lines = authorized.read_text().splitlines()
            authorized.write_text('\n'.join(line for line in lines if line.strip() != 'restrict ' + public_key) + '\n')
            authorized.chmod(0o600)
            raise SetupError('Encrypted Secret setup failed; the newly added key was revoked') from None
        finally:
            config.clear()
        print(json.dumps({'setupComplete': True, 'repository': arguments.repository,
                          'secret': 'BOB_DEPLOY_SSH_CONFIG', 'user': account,
                          'serverFingerprint': observed, 'keyComment': comment,
                          'sshConnectionVerified': False,
                          'next': 'Agent should dispatch the inspect operation'}), flush=True)
    # Temporary private SSH and GitHub CLI credential files are now removed.


if __name__ == '__main__':
    try:
        main()
    except SetupError as error:
        print('SETUP STOPPED:', str(error), flush=True)
        raise SystemExit(1)
    except (Exception, KeyboardInterrupt) as error:
        print('SETUP STOPPED:', type(error).__name__, flush=True)
        raise SystemExit(1)
