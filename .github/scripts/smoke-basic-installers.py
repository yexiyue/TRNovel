#!/usr/bin/env python3
"""Install local release fixtures through shell/PowerShell and npm."""
import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading

parser = argparse.ArgumentParser()
parser.add_argument('--directory', type=Path, required=True)
parser.add_argument('--target', required=True)
args = parser.parse_args()
assets = args.directory.resolve()
server = ThreadingHTTPServer(('127.0.0.1', 0), partial(SimpleHTTPRequestHandler, directory=str(assets)))
threading.Thread(target=server.serve_forever, daemon=True).start()
try:
    with tempfile.TemporaryDirectory(prefix='trnovel installers ') as temporary:
        directory = Path(temporary)
        environment = os.environ.copy()
        environment['TRNOVEL_RELEASE_BASE_URL'] = f'http://127.0.0.1:{server.server_port}'
        environment['TRNOVEL_INSTALL_DIR'] = str(directory / 'shell bin')
        if 'windows' in args.target:
            subprocess.run(['pwsh', '-NoProfile', '-File', str(assets / 'trnovel-basic-installer.ps1')], env=environment, check=True)
        else:
            subprocess.run(['sh', str(assets / 'trnovel-basic-installer.sh')], env=environment, check=True)
        subprocess.run([sys.executable, '.github/scripts/smoke-variant.py', 'basic', environment['TRNOVEL_INSTALL_DIR'], args.target], check=True)
        prefix = directory / 'npm prefix'
        subprocess.run(['npm', 'install', '--prefix', str(prefix), '--no-audit', '--no-fund', str(assets / 'trnovel-basic-npm-package.tar.gz')], env=environment, check=True)
        package = prefix / 'node_modules/@trnovel/trnovel-basic'
        subprocess.run([sys.executable, '.github/scripts/smoke-variant.py', 'basic', str(package / 'native'), args.target], check=True)
        for binary in ('trnovel', 'trn'):
            subprocess.run(['node', str(package / 'bin' / f'{binary}.js'), '--version'], check=True)
finally:
    server.shutdown()
    server.server_close()
print('Local basic script and npm installation passed')
