#!/usr/bin/env python3
"""Read-only, redacted inspection for the independent operations gateway."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
from urllib.parse import urlsplit


def endpoint_origin(value):
    parsed = urlsplit(str(value or ''))
    host = parsed.hostname or ''
    if parsed.port:
        host += ':' + str(parsed.port)
    return parsed.scheme + '://' + host if host else ''


def inspect(root, service):
    spec = importlib.util.spec_from_file_location('bob_deploy', Path(__file__).with_name('deploy-bob-online.py'))
    deploy = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(deploy)
    state = deploy.inspect_service(root, service)
    state['readOnly'] = True
    state['writableInstallDirectory'] = os.access(root, os.W_OK | os.X_OK)
    state['writableProgramDirectory'] = os.access(root / 'target/release', os.W_OK | os.X_OK)
    state['serviceControl'] = {
        action: subprocess.run(['sudo', '-n', '-l', shutil.which('systemctl'), action, service],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0
        for action in ('start', 'stop', 'reset-failed')
    }
    state['buildTools'] = {tool: shutil.which(tool) for tool in ('node', 'pnpm', 'cargo')}
    configuration = root / 'data/model_hub_state.json'
    hub = json.loads(configuration.read_text()) if configuration.is_file() else {}
    state['connections'] = [
        {'id': item.get('id'), 'deviceId': item.get('deviceId'),
         'name': item.get('name') or item.get('label'),
         'origin': endpoint_origin(item.get('baseUrl') or item.get('url')),
         'enabled': item.get('enabled', True)}
        for item in hub.get('remoteNodes', [])
    ]
    return state


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', required=True, type=Path)
    parser.add_argument('--service', default='bob-online.service')
    arguments = parser.parse_args()
    try:
        print(json.dumps(inspect(arguments.root.resolve(strict=True), arguments.service), indent=2))
    except Exception as error:
        # Avoid serializing arbitrary configuration or credential values on failure.
        print(json.dumps({'readOnly': True, 'errorType': type(error).__name__}))
        raise SystemExit(1)
