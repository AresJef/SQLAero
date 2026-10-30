"""Prepare or stop SQLAero's isolated, loopback-only MySQL test instances.

This development utility never uses Homebrew services or its default data
directory. Secrets remain in a private temporary directory, outside Git.
"""

from __future__ import annotations

import argparse
import getpass
import json
import os
from pathlib import Path
import secrets
import shutil
import socket as network
import subprocess
import tempfile
import time
from typing import Any


_USER = str(os.getuid()) if os.name == 'posix' else getpass.getuser()
_DEFAULT_ROOT = Path('/private/tmp' if os.name == 'posix' and Path('/private/tmp').is_dir()
                     else tempfile.gettempdir()) / f'sqlaero-mysql-{_USER}'
ROOT = Path(os.environ.get('SQLAERO_TEST_MYSQL_ROOT', str(_DEFAULT_ROOT))).absolute()
VERSIONS: dict[str, tuple[str, int]] = {
    '84': ('8.4', int(os.environ.get('SQLAERO_TEST_MYSQL_PORT_84', '13384'))),
    '97': ('9.7', int(os.environ.get('SQLAERO_TEST_MYSQL_PORT_97', '13397'))),
}
if any(not 1 <= port <= 65535 for _, port in VERSIONS.values()):
    raise ValueError('Fixture ports must be integers in 1..=65535')
REPOSITORY = Path(__file__).resolve().parents[1]


def program(label: str, name: str) -> Path:
    """Resolve an explicitly provisioned native installation or a Homebrew keg."""
    prefix = os.environ.get(f'SQLAERO_MYSQL_{label}_ROOT')
    if prefix is None:
        brew = shutil.which('brew')
        if brew is None and Path('/opt/homebrew/bin/brew').is_file():
            brew = '/opt/homebrew/bin/brew'
        if brew is None:
            raise RuntimeError(f'Set SQLAERO_MYSQL_{label}_ROOT to the isolated MySQL installation')
        prefix = run([brew, '--prefix', f'mysql@{VERSIONS[label][0]}']).strip()
    path = Path(prefix) / 'bin' / (name + ('.exe' if os.name == 'nt' else ''))
    if not path.is_file():
        raise RuntimeError(f'Native fixture executable is unavailable: {path}')
    return path


def config_path(path: Path) -> str:
    """Quote one native path for MySQL option-file syntax, including Windows."""
    return '"' + path.as_posix().replace('"', '\\"') + '"'


def redact(text: str) -> str:
    """Remove this fixture's credentials before exporting native diagnostics."""
    for label in VERSIONS:
        state_file = ROOT / label / 'state.json'
        if state_file.is_file():
            state: dict[str, Any] = json.loads(state_file.read_text(encoding='utf-8'))
            for name in ('root_password', 'test_password'):
                password = state.get(name)
                if isinstance(password, str) and password:
                    text = text.replace(password, '<redacted>')
    return text


def run(command: list[str], *, data: str | None = None) -> str:
    """Report a native command failure while redacting fixture passwords."""
    result = subprocess.run(command, input=data, text=True, encoding='utf-8',
                            capture_output=True, check=False, timeout=90)
    if result.returncode:
        detail = redact(result.stderr.strip())
        raise RuntimeError(f'{Path(command[0]).name} failed (exit {result.returncode}); '
                           + detail[-2000:])
    return result.stdout


def diagnostics(destination: Path) -> None:
    """Export only known logs, never account state, option files or private keys."""
    destination.mkdir(parents=True, exist_ok=True)
    for label in VERSIONS:
        for name in ('initialize.log', 'server.log', 'launcher.log'):
            path = ROOT / label / name
            if path.is_file():
                text = path.read_text(encoding='utf-8', errors='replace')
                (destination / f'mysql-{label}-{name}').write_text(redact(text), encoding='utf-8')


def write_private(path: Path, content: str) -> None:
    """Write in the private fixture root; POSIX files additionally use mode 0600."""
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(descriptor, 'w', encoding='utf-8', newline='\n') as output:
        output.write(content)


def certificates() -> Path:
    """Create a private CA, server certificate and encrypted client key once."""
    directory = ROOT / 'tls'
    directory.mkdir(mode=0o700, exist_ok=True)
    if (directory / 'complete').exists():
        return directory
    openssl = os.environ.get('SQLAERO_OPENSSL') or shutil.which('openssl')
    if not openssl:
        raise RuntimeError('Set SQLAERO_OPENSSL or provide OpenSSL on PATH')
    configuration = directory / 'openssl.cnf'
    write_private(configuration, '[req]\ndistinguished_name=dn\nprompt=no\n'
                  '[dn]\nCN=SQLAero isolated test CA\n'
                  '[ca_extensions]\nbasicConstraints=critical,CA:TRUE\n'
                  'keyUsage=critical,keyCertSign,cRLSign\nsubjectKeyIdentifier=hash\n')
    run([openssl, 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '7',
         '-config', str(configuration), '-extensions', 'ca_extensions',
         '-subj', '/CN=SQLAero isolated test CA', '-keyout', str(directory / 'ca-key.pem'),
         '-out', str(directory / 'ca.pem')])
    for name, usage in [('server', 'serverAuth'), ('client', 'clientAuth')]:
        run([openssl, 'req', '-new', '-newkey', 'rsa:2048', '-nodes',
             '-config', str(configuration),
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
    write_private(directory / 'complete', 'Fixture certificates generated with native OpenSSL.\n')
    return directory


def mysql_command(label: str, account: str = 'root') -> list[str]:
    """Build a client command using the instance's private option file."""
    state: dict[str, Any] = json.loads((ROOT / label / 'state.json').read_text())
    return [state['mysql'],
            f'--defaults-file={ROOT / label / (account + ".cnf")}',
            '--no-login-paths',
            '--batch', '--raw', '--skip-column-names']


def register_clients(clients: dict[str, Path]) -> None:
    """Verify owned administration bridges before replacing prepared clients.

    Only CI preparation calls this operation. The same stored executable is
    subsequently used by existing Rust and Python tests; product code never
    knows whether administration runs on the host or in a client container.
    """
    prepared: list[tuple[Path, dict[str, Any]]] = []
    for label, client in clients.items():
        client = client.resolve(strict=True)
        if not client.is_file() or not client.is_relative_to(ROOT.resolve()):
            raise RuntimeError('Administration bridge must be a file in the owned fixture root')
        state_path = ROOT / label / 'state.json'
        state: dict[str, Any] = json.loads(state_path.read_text())
        identity = run([str(client), '--version']).strip()
        if f'Ver {VERSIONS[label][0]}.' not in identity:
            raise RuntimeError(f'Unexpected MySQL {label} administration client version')
        state['mysql'] = str(client)
        state['client_identity'] = identity
        prepared.append((state_path, state))
    for state_path, state in prepared:
        write_private(state_path, json.dumps(state))
    for label in clients:
        run(mysql_command(label), data='SELECT 1;')


def ready_endpoint(port: int, unix_socket: Path | None) -> bool:
    """Probe the local transport selected for fixture administration."""
    try:
        if unix_socket is None:
            with network.create_connection(('127.0.0.1', port), timeout=0.2):
                return True
        family: int | None = getattr(network, 'AF_UNIX', None)
        if family is None:
            return False
        with network.socket(family, network.SOCK_STREAM) as probe:
            probe.settimeout(0.2)
            probe.connect(str(unix_socket))
            return True
    except OSError:
        return False


def prepare(label: str, tls: Path) -> dict[str, Any]:
    """Initialize, launch and verify one independent native MySQL instance."""
    version, port = VERSIONS[label]
    directory = ROOT / label
    directory.mkdir(mode=0o700, exist_ok=True)
    data = directory / 'data'
    binary = program(label, 'mysqld')
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
        '[mysqld]', f'basedir={config_path(binary.parent.parent)}', f'datadir={config_path(data)}',
        'bind-address=127.0.0.1', f'port={port}', f'pid-file={config_path(pidfile)}',
        *([f'socket={config_path(socket)}'] if os.name != 'nt' else []),
        f'log-error={config_path(directory / "server.log")}', 'mysqlx=0', 'skip-name-resolve=ON',
        'max-allowed-packet=64M', 'local-infile=OFF', f'ssl-ca={config_path(tls / "ca.pem")}',
        f'ssl-cert={config_path(tls / "server.pem")}', f'ssl-key={config_path(tls / "server-key.pem")}', '',
    ]))
    bootstrap = directory / 'bootstrap.sql'
    if fresh:
        # Initialize accounts before accepting authenticated clients. This
        # avoids relying on root@localhost matching a TCP loopback address.
        setup = (f"ALTER USER 'root'@'localhost' IDENTIFIED BY '{state['root_password']}';\n"
                 f"CREATE USER 'root'@'127.0.0.1' IDENTIFIED BY '{state['root_password']}';\n"
                 "GRANT ALL ON *.* TO 'root'@'127.0.0.1' WITH GRANT OPTION;\n"
                 'CREATE DATABASE sqlaero_test;\n')
        for host in ('localhost', '127.0.0.1'):
            setup += (f"CREATE USER 'sqlaero'@'{host}' IDENTIFIED BY '{state['test_password']}';\n"
                      f"GRANT ALL ON sqlaero_test.* TO 'sqlaero'@'{host}';\n")
        write_private(bootstrap, setup)
    if not pidfile.exists():
        with (directory / 'launcher.log').open('ab') as output:
            process = subprocess.Popen([str(binary), f'--defaults-file={config}',
                                        *([f'--init-file={bootstrap.as_posix()}'] if fresh else [])],
                                       stdin=subprocess.DEVNULL, stdout=output, stderr=output,
                                       start_new_session=os.name != 'nt')
        end = time.monotonic() + 60
        while True:
            if process.poll() is not None or time.monotonic() >= end:
                raise RuntimeError(f'MySQL {version} did not become ready; inspect {directory}')
            if ready_endpoint(port, None if os.name == 'nt' else socket):
                break
            time.sleep(0.1)
    endpoint = (f'protocol=TCP\nhost=127.0.0.1\nport={port}\n'
                f'ssl-mode=VERIFY_IDENTITY\nssl-ca={config_path(tls / "ca.pem")}\n'
                if os.name == 'nt' else f'protocol=SOCKET\nsocket={config_path(socket)}\n')
    for account, user, password in [
        ('root', 'root', state['root_password']), ('test', 'sqlaero', state['test_password']),
    ]:
        write_private(directory / f'{account}.cnf', f'[client]\n{endpoint}'
                      f'user={user}\npassword={password}\n')
    state.update({'mysql': str(program(label, 'mysql')), 'port': str(port),
                  'transport': 'tcp' if os.name == 'nt' else 'unix'})
    write_private(state_path, json.dumps(state))
    actual = run(mysql_command(label), data='SELECT VERSION(), @@port, @@socket;').strip()
    if fresh:
        bootstrap.unlink()
    run(mysql_command(label, 'test'), data='SELECT 1;')
    tcp = mysql_command(label, 'test') + ['--protocol=TCP', '--host=127.0.0.1', f'--port={port}',
                                       '--ssl-mode=VERIFY_IDENTITY', f'--ssl-ca={tls / "ca.pem"}']
    run(tcp, data="SELECT 1; SHOW SESSION STATUS LIKE 'Ssl_version';")
    for name, statement in [('charsets', 'SHOW CHARACTER SET;'), ('collations', 'SHOW COLLATION;')]:
        (directory / f'{name}.tsv').write_text(run(mysql_command(label), data=statement))
    return {'line': version, 'identity': identity, 'actual': actual,
            'port': port, 'socket': str(socket), 'tcp_tls_verified': True,
            'unix_verified': os.name != 'nt', 'pid': int(pidfile.read_text())}


def stop(label: str) -> None:
    """Shut down the owned instance using its private administrative connection."""
    directory = ROOT / label
    pidfile = directory / 'mysql.pid'
    if not pidfile.exists():
        return
    run([str(program(label, 'mysqladmin')), f'--defaults-file={directory / "root.cnf"}',
         '--no-login-paths', 'shutdown'])
    end = time.monotonic() + 15
    while pidfile.exists() and time.monotonic() < end:
        time.sleep(0.1)
    if pidfile.exists():
        raise RuntimeError(f'MySQL fixture {label} did not stop cleanly')


def main() -> None:
    """Run the explicitly selected fixture operation."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['prepare', 'stop', 'diagnostics', 'clients'])
    parser.add_argument('--output', type=Path)
    parser.add_argument('--client84', type=Path)
    parser.add_argument('--client97', type=Path)
    arguments = parser.parse_args()
    if arguments.action == 'diagnostics':
        if arguments.output is None:
            parser.error('diagnostics requires --output')
        diagnostics(arguments.output)
        return
    ROOT.mkdir(mode=0o700, exist_ok=True)
    if ROOT.is_symlink() or (os.name == 'posix' and ROOT.stat().st_uid != os.getuid()):
        raise RuntimeError('Fixture root must be owned by the current user and not be a symlink')
    ROOT.chmod(0o700)
    if arguments.action == 'clients':
        if arguments.client84 is None or arguments.client97 is None:
            parser.error('clients requires --client84 and --client97')
        register_clients({'84': arguments.client84, '97': arguments.client97})
        return
    if arguments.action == 'stop':
        for label in VERSIONS:
            stop(label)
        return
    tls = certificates()
    result = [prepare(label, tls) for label in VERSIONS]
    destination = ROOT / 'instances.json'
    destination.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
