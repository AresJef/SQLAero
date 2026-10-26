"""Prepare or stop SQLAero's isolated, loopback-only MySQL test instances.

This development utility never uses Homebrew services or its default data
directory. Secrets remain in a private temporary directory, outside Git.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import secrets
import signal
import subprocess
import time
from typing import Any


ROOT = Path('/private/tmp') / f'sqlaero-mysql-{os.getuid()}'
VERSIONS = {'84': ('8.4', 13384), '97': ('9.7', 13397)}
REPOSITORY = Path(__file__).resolve().parents[1]


def run(command: list[str], *, data: str | None = None) -> str:
    """Run a fixture command without exposing SQL or credentials on failure."""
    result = subprocess.run(command, input=data, text=True, capture_output=True, check=False)
    if result.returncode:
        raise RuntimeError(f'{Path(command[0]).name} failed (exit {result.returncode}); '
                           'inspect the private fixture logs')
    return result.stdout


def write_private(path: Path, content: str) -> None:
    """Write a fixture-owned file with owner-only permissions."""
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(descriptor, 'w') as output:
        output.write(content)


def certificates() -> Path:
    """Create a private CA, server certificate and encrypted client key once."""
    directory = ROOT / 'tls'
    directory.mkdir(mode=0o700, exist_ok=True)
    if (directory / 'complete').exists():
        return directory
    openssl = '/opt/homebrew/opt/openssl@4/bin/openssl'
    run([openssl, 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '7',
         '-subj', '/CN=SQLAero isolated test CA', '-keyout', str(directory / 'ca-key.pem'),
         '-out', str(directory / 'ca.pem')])
    for name, usage in [('server', 'serverAuth'), ('client', 'clientAuth')]:
        run([openssl, 'req', '-new', '-newkey', 'rsa:2048', '-nodes',
             '-subj', f'/CN=SQLAero {name}', '-keyout', str(directory / f'{name}-key.pem'),
             '-out', str(directory / f'{name}.csr')])
        extensions = directory / f'{name}.ext'
        write_private(extensions, f'basicConstraints=CA:FALSE\nextendedKeyUsage={usage}\n'
                      'keyUsage=digitalSignature,keyEncipherment\n'
                      + ('subjectAltName=DNS:localhost,IP:127.0.0.1\n' if name == 'server' else ''))
        run([openssl, 'x509', '-req', '-in', str(directory / f'{name}.csr'),
             '-CA', str(directory / 'ca.pem'), '-CAkey', str(directory / 'ca-key.pem'),
             '-CAcreateserial', '-days', '7', '-extfile', str(extensions),
             '-out', str(directory / f'{name}.pem')])
    password = directory / 'key-password'
    write_private(password, secrets.token_hex(24))
    run([openssl, 'pkcs8', '-topk8', '-in', str(directory / 'client-key.pem'),
         '-out', str(directory / 'client-key-encrypted.pem'), '-v2', 'aes-256-cbc',
         '-v2prf', 'hmacWithSHA256', '-passout', f'file:{password}'])
    for path in directory.iterdir():
        path.chmod(0o600)
    write_private(directory / 'complete', 'Fixture certificates generated with OpenSSL 4.\n')
    return directory


def mysql_command(label: str, account: str = 'root') -> list[str]:
    """Build a client command using the instance's private option file."""
    version, _ = VERSIONS[label]
    return [f'/opt/homebrew/opt/mysql@{version}/bin/mysql',
            f'--defaults-file={ROOT / label / (account + ".cnf")}',
            '--no-login-paths',
            '--batch', '--raw', '--skip-column-names']


def prepare(label: str, tls: Path) -> dict[str, Any]:
    """Initialize, launch and verify one independent native MySQL instance."""
    version, port = VERSIONS[label]
    directory = ROOT / label
    directory.mkdir(mode=0o700, exist_ok=True)
    data = directory / 'data'
    binary = Path(f'/opt/homebrew/opt/mysql@{version}/bin/mysqld')
    identity = run([str(binary), '--version']).strip()
    state_path = directory / 'state.json'
    fresh = not state_path.exists()
    if fresh:
        if data.exists():
            raise RuntimeError(f'Unregistered data directory exists: {data}')
        state: dict[str, Any] = {
            'identity': identity, 'root_password': secrets.token_hex(24),
            'test_password': secrets.token_hex(24),
        }
        run([str(binary), '--no-defaults', '--initialize-insecure', f'--datadir={data}',
             f'--basedir={binary.parent.parent}', f'--log-error={directory / "initialize.log"}'])
        write_private(state_path, json.dumps(state))
    else:
        state = json.loads(state_path.read_text())
        if state['identity'] != identity:
            raise RuntimeError('Fixture executable identity changed; explicit rebuild required')
    socket = ROOT / f'{label}.sock'
    pidfile = directory / 'mysql.pid'
    config = directory / 'my.cnf'
    write_private(config, '\n'.join([
        '[mysqld]', f'basedir={binary.parent.parent}', f'datadir={data}',
        'bind-address=127.0.0.1', f'port={port}', f'socket={socket}', f'pid-file={pidfile}',
        f'log-error={directory / "server.log"}', 'mysqlx=0', 'skip-name-resolve=ON',
        'max-allowed-packet=64M', 'local-infile=OFF', f'ssl-ca={tls / "ca.pem"}',
        f'ssl-cert={tls / "server.pem"}', f'ssl-key={tls / "server-key.pem"}', '',
    ]))
    if not socket.exists():
        with (directory / 'launcher.log').open('ab') as output:
            process = subprocess.Popen([str(binary), f'--defaults-file={config}'],
                                       stdin=subprocess.DEVNULL, stdout=output, stderr=output,
                                       start_new_session=True)
        end = time.monotonic() + 30
        while not socket.exists():
            if process.poll() is not None or time.monotonic() >= end:
                raise RuntimeError(f'MySQL {version} did not become ready; inspect {directory}')
            time.sleep(0.1)
    for account, user, password in [
        ('root', 'root', state['root_password']), ('test', 'sqlaero', state['test_password']),
    ]:
        write_private(directory / f'{account}.cnf', f'[client]\nprotocol=SOCKET\nsocket={socket}\n'
                      f'user={user}\npassword={password}\n')
    if fresh:
        setup = (f"ALTER USER 'root'@'localhost' IDENTIFIED BY '{state['root_password']}';\n"
                 'CREATE DATABASE sqlaero_test;\n')
        for host in ['localhost', '127.0.0.1']:
            setup += (f"CREATE USER 'sqlaero'@'{host}' IDENTIFIED BY '{state['test_password']}';\n"
                      f"GRANT ALL ON sqlaero_test.* TO 'sqlaero'@'{host}';\n")
        run([str(binary.parent / 'mysql'), '--no-defaults', '--no-login-paths', '--protocol=SOCKET',
             f'--socket={socket}', '--user=root'], data=setup)
    actual = run(mysql_command(label), data='SELECT VERSION(), @@port, @@socket;').strip()
    run(mysql_command(label, 'test'), data='SELECT 1;')
    tcp = mysql_command(label, 'test') + ['--protocol=TCP', '--host=127.0.0.1', f'--port={port}',
                                       '--ssl-mode=VERIFY_IDENTITY', f'--ssl-ca={tls / "ca.pem"}']
    run(tcp, data="SELECT 1; SHOW SESSION STATUS LIKE 'Ssl_version';")
    for name, statement in [('charsets', 'SHOW CHARACTER SET;'), ('collations', 'SHOW COLLATION;')]:
        (directory / f'{name}.tsv').write_text(run(mysql_command(label), data=statement))
    return {'line': version, 'identity': identity, 'actual': actual,
            'port': port, 'socket': str(socket), 'tcp_tls_verified': True,
            'unix_verified': True, 'pid': int(pidfile.read_text())}


def stop(label: str) -> None:
    """Stop only the registered PID whose command names this fixture config."""
    directory = ROOT / label
    pidfile = directory / 'mysql.pid'
    if not pidfile.exists():
        return
    pid = int(pidfile.read_text())
    command = run(['/bin/ps', '-p', str(pid), '-o', 'command='])
    if f'--defaults-file={directory / "my.cnf"}' not in command:
        raise RuntimeError(f'PID {pid} is not the registered fixture process')
    os.kill(pid, signal.SIGTERM)
    end = time.monotonic() + 15
    while pidfile.exists() and time.monotonic() < end:
        time.sleep(0.1)
    if pidfile.exists():
        raise RuntimeError(f'MySQL fixture {label} did not stop cleanly')


def main() -> None:
    """Run the explicitly selected fixture operation."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['prepare', 'stop'])
    arguments = parser.parse_args()
    ROOT.mkdir(mode=0o700, exist_ok=True)
    if ROOT.is_symlink() or ROOT.stat().st_uid != os.getuid():
        raise RuntimeError('Fixture root must be owned by the current user and not be a symlink')
    ROOT.chmod(0o700)
    if arguments.action == 'stop':
        for label in VERSIONS:
            stop(label)
        return
    tls = certificates()
    result = [prepare(label, tls) for label in VERSIONS]
    destination = REPOSITORY / 'evidence/rust-sync-kernel/startup/mysql-instances.json'
    destination.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
