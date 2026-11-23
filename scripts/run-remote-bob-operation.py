#!/usr/bin/env python3
"""SSH transport for a manual Actions job. Private key material is never logged."""
import argparse
import json
import os
from pathlib import Path, PurePosixPath
import re
import shlex
import shutil
import subprocess
import tempfile


class GatewayError(Exception):
    pass


def validate_config(configuration):
    host = str(configuration.get('host', ''))
    user = str(configuration.get('user', ''))
    port = int(configuration.get('port', 22))
    fingerprint = str(configuration.get('fingerprint', ''))
    key = str(configuration.get('private_key', ''))
    root = str(configuration.get('root', ''))
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9.-]{0,252}', host):
        raise GatewayError('Invalid SSH host')
    if not re.fullmatch(r'[a-z_][a-z0-9_-]{0,31}', user):
        raise GatewayError('Invalid SSH user')
    if not 1 <= port <= 65535:
        raise GatewayError('Invalid SSH port')
    if not re.fullmatch(r'SHA256:[A-Za-z0-9+/]{43}', fingerprint):
        raise GatewayError('Expected ED25519 host fingerprint is required')
    if '-----BEGIN OPENSSH PRIVATE KEY-----' not in key:
        raise GatewayError('A dedicated OpenSSH private key is required')
    if not root.startswith('/') or root == '/' or '..' in PurePosixPath(root).parts or any(c in root for c in ('\n', '\r', '\x00')):
        raise GatewayError('An explicit installation root is required')
    return host, user, port, fingerprint, key, root


def run_operation(configuration, operation, source_sha, repository, scripts, result_file):
    host, user, port, expected, key, root = validate_config(configuration)
    if operation not in ('inspect', 'deploy'):
        raise GatewayError('Unsupported operation')
    if operation == 'deploy' and not re.fullmatch(r'[0-9a-f]{40}', source_sha):
        raise GatewayError('Deploy requires an exact source commit')
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository):
        raise GatewayError('Invalid source repository')
    os.umask(0o077)
    remote_directory = None
    with tempfile.TemporaryDirectory(prefix='bob-ssh-') as temporary:
        work = Path(temporary)
        key_file, known_hosts = work / 'key', work / 'known_hosts'
        key_file.write_text(key.rstrip() + '\n')
        key_file.chmod(0o600)
        # Remove secret material from local configuration before any command failure.
        configuration.clear()
        del key
        subprocess.run(['ssh-keygen', '-y', '-f', key_file], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL)
        with known_hosts.open('w') as target:
            subprocess.run(['ssh-keyscan', '-T', '10', '-p', str(port), '-t', 'ed25519', host],
                           check=True, stdout=target, stderr=subprocess.DEVNULL)
        lines = subprocess.check_output(['ssh-keygen', '-lf', known_hosts, '-E', 'sha256'], text=True).splitlines()
        fingerprints = [line.split()[1] for line in lines if line.strip()]
        if not fingerprints or any(value != expected for value in fingerprints):
            raise GatewayError('Server host key does not match the owner-provided fingerprint')
        common = ['-i', str(key_file), '-o', 'BatchMode=yes', '-o', 'IdentitiesOnly=yes',
                  '-o', 'StrictHostKeyChecking=yes', '-o', 'UserKnownHostsFile=' + str(known_hosts),
                  '-o', 'ConnectTimeout=20', '-o', 'ServerAliveInterval=20', '-o', 'ServerAliveCountMax=3']
        ssh = ['ssh', '-T', '-p', str(port), *common, user + '@' + host]
        scp = ['scp', '-P', str(port), *common]
        try:
            remote_directory = subprocess.check_output([*ssh, 'umask 077; mktemp -d /tmp/tmp.XXXXXXXXXX'], text=True).strip()
            if not re.fullmatch(r'/tmp/tmp\.[A-Za-z0-9]{6,32}', remote_directory):
                remote_directory = None
                raise GatewayError('Unexpected private staging directory response')
            print(json.dumps({'transportVerified': True, 'host': host, 'user': user, 'serverFingerprint': expected}), flush=True)
            subprocess.run([*scp, str(scripts / 'deploy-bob-online.py'), str(scripts / 'inspect-bob-online.py'),
                            user + '@' + host + ':' + remote_directory + '/'], check=True)
            if operation == 'inspect':
                command = ['python3', remote_directory + '/inspect-bob-online.py', '--root', root]
            else:
                command = ['python3', remote_directory + '/deploy-bob-online.py', '--root', root,
                           '--repository', 'https://github.com/' + repository + '.git',
                           '--commit', source_sha, '--non-interactive']
            result_file.parent.mkdir(parents=True, exist_ok=True)
            with result_file.open('w') as receipt:
                result = subprocess.run([*ssh, shlex.join(command)], stdout=receipt)
            # The remote entrypoints output only redacted operation status, not probe logs.
            print(result_file.read_text(), end='', flush=True)
            if result.returncode:
                raise GatewayError('Remote operation failed; see redacted job output')
        finally:
            if remote_directory:
                subprocess.run([*ssh, 'rm -rf -- ' + shlex.quote(remote_directory)],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--operation', choices=('inspect', 'deploy'), required=True)
    parser.add_argument('--source-sha', default='')
    parser.add_argument('--repository', required=True)
    parser.add_argument('--scripts', type=Path, required=True)
    parser.add_argument('--result-file', type=Path, required=True)
    arguments = parser.parse_args()
    raw = os.environ.pop('BOB_DEPLOY_SSH_CONFIG', '')
    if not raw:
        raise GatewayError('One-time setup required: repository Secret BOB_DEPLOY_SSH_CONFIG is missing')
    try:
        configuration = json.loads(raw)
    except json.JSONDecodeError:
        raise GatewayError('Deployment Secret must contain a JSON configuration') from None
    del raw
    if not isinstance(configuration, dict):
        raise GatewayError('Deployment Secret must contain a JSON object')
    run_operation(configuration, arguments.operation, arguments.source_sha,
                  arguments.repository, arguments.scripts, arguments.result_file)


if __name__ == '__main__':
    try:
        main()
    except GatewayError as error:
        print('GATEWAY NOT READY:', str(error))
        raise SystemExit(1)
    except Exception as error:
        # Do not print arbitrary subprocess inputs or the configuration on failure.
        print('GATEWAY FAILED:', type(error).__name__)
        raise SystemExit(1)
