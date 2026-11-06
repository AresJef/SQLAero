"""Public TLS material, peer-chain and real-signal boundaries on owned fixtures."""

from collections.abc import Callable, Generator
from contextlib import contextmanager
import base64
import errno
import gc
import hashlib
import os
from pathlib import Path
import socket
import subprocess
import sys
from tempfile import TemporaryDirectory
from types import FrameType
from typing import Literal

import pytest
from _native_signal import NativeAlarm
from sqlaero import SSL, TLSCertificate
from test_tls_expansion import ROOT, live, requires_mysql
from tools.mysql_fixture import mysql_command, run


Source = Literal['file', 'dir']
requires_mutable_tls = pytest.mark.skipif(
    os.environ.get('SQLAERO_TEST_MUTABLE_TLS_ROOT') != str(ROOT),
    reason='requires explicit opt-in to reload the selected owned fixture TLS',
)
_DNS_COUNT = 15000


def _policy(source: Source, member: Path) -> SSL:
    return SSL(ca_file=member) if source == 'file' else SSL(ca_dir=member.parent)


@requires_mysql
@pytest.mark.skipif(os.name != 'posix', reason='requires POSIX FIFO and Unix socket file objects')
@pytest.mark.parametrize('source', ['file', 'dir'])
@pytest.mark.parametrize('kind', ['fifo', 'socket'])
def test_special_ca_object_is_rejected_without_waiting_for_stream_contents(
    source: Source, kind: str,
) -> None:
    # A short owned directory also stays within native Unix socket path limits.
    with TemporaryDirectory(prefix='sqlaero-ca-', dir='/tmp') as directory:
        parent = Path(directory)
        (parent / 'valid.pem').write_bytes((ROOT / 'tls/ca.pem').read_bytes())
        member = parent / 'special.pem'
        with socket.socket(socket.AF_UNIX) as endpoint:
            if kind == 'fifo':
                os.mkfifo(member)
            else:
                endpoint.bind(str(member))
            # A regression that opens a FIFO must fail finitely rather than
            # block the parent runner. The product still owns all validation.
            code = r'''
import sys
import sysconfig
from sqlaero import SSL
if sysconfig.get_config_var('Py_GIL_DISABLED'):
    assert not sys._is_gil_enabled()
source, member = sys.argv[1:]
from pathlib import Path
try:
    SSL(**({'ca_file': member} if source == 'file' else {'ca_dir': str(Path(member).parent)}))
except OSError as error:
    diagnostic = '\n'.join([str(error), *getattr(error, '__notes__', [])])
    assert 'regular file' in diagnostic and 'special.pem' in diagnostic
else:
    raise AssertionError('a special object was accepted or silently skipped')
if sysconfig.get_config_var('Py_GIL_DISABLED'):
    assert not sys._is_gil_enabled()
print('special CA object rejected before stream reading')
'''
            result = subprocess.run([sys.executable, '-c', code, source, str(member)],
                                    capture_output=True, text=True, timeout=8, check=False)
            assert result.returncode == 0, result.stdout + result.stderr
            assert result.stdout.strip() == 'special CA object rejected before stream reading'


@requires_mysql
@pytest.mark.skipif(os.name != 'posix', reason='requires an actual POSIX EACCES boundary')
@pytest.mark.parametrize('source', ['file', 'dir'])
def test_actual_unreadable_ca_preserves_permission_error_and_member_path(
    tmp_path: Path, source: Source,
) -> None:
    if os.geteuid() == 0:
        pytest.skip('root cannot establish this test account permission boundary')
    member = tmp_path / 'unreadable.pem'
    member.write_bytes((ROOT / 'tls/ca.pem').read_bytes())
    assert _policy(source, member) is not None
    original_mode = member.stat().st_mode
    try:
        member.chmod(0)
        with pytest.raises(PermissionError) as caught:
            _policy(source, member)
        assert caught.value.errno == errno.EACCES
        diagnostic = '\n'.join([str(caught.value), *getattr(caught.value, '__notes__', [])])
        assert 'unreadable.pem' in diagnostic
        assert 'BEGIN CERTIFICATE' not in diagnostic
    finally:
        member.chmod(original_mode)
    assert _policy(source, member) is not None


def _openssl() -> str:
    value = os.environ.get('SQLAERO_OPENSSL')
    assert value is not None, 'select the existing native OpenSSL fixture tool'
    return value


def _der(pem: bytes) -> bytes:
    return base64.b64decode(b''.join(pem.splitlines()[1:-1]), validate=True)


@contextmanager
def _reloaded_peer(label: str, pem: bytes, *, server_ca_extra: bytes = b'') -> Generator[None, None, None]:
    """Replace only this lane's server chain, restoring bytes and active context."""
    server = ROOT / 'tls/server.pem'
    ca = ROOT / 'tls/ca.pem'
    original = server.read_bytes()
    original_ca = ca.read_bytes()
    try:
        if server_ca_extra:
            # MySQL verifies its own leaf against ssl_ca during reload; the
            # chain file alone is not its untrusted issuer input for this check.
            ca.write_bytes(original_ca + server_ca_extra)
        server.write_bytes(pem)
        run(mysql_command(label), data='ALTER INSTANCE RELOAD TLS;')
        yield
    finally:
        server.write_bytes(original)
        if server_ca_extra:
            ca.write_bytes(original_ca)
        run(mysql_command(label), data='ALTER INSTANCE RELOAD TLS;')
    assert server.read_bytes() == original
    assert ca.read_bytes() == original_ca
    with live(label):
        pass


def _signed_chain(tmp_path: Path) -> tuple[bytes, bytes]:
    openssl = _openssl()
    key = tmp_path / 'intermediate-key.pem'
    csr = tmp_path / 'intermediate.csr'
    intermediate = tmp_path / 'intermediate.pem'
    constraints = tmp_path / 'intermediate.ext'
    constraints.write_text('basicConstraints=critical,CA:TRUE,pathlen:0\n'
                           'keyUsage=critical,keyCertSign,cRLSign\n'
                           'subjectKeyIdentifier=hash\n')
    leaf = tmp_path / 'leaf.pem'
    try:
        run([openssl, 'req', '-new', '-newkey', 'rsa:2048', '-nodes',
             '-subj', '/CN=SQLAero D test intermediate',
             '-keyout', str(key), '-out', str(csr)])
        run([openssl, 'x509', '-req', '-in', str(csr),
             '-CA', str(ROOT / 'tls/ca.pem'), '-CAkey', str(ROOT / 'tls/ca-key.pem'),
             '-set_serial', '33', '-days', '2', '-extfile', str(constraints),
             '-out', str(intermediate)])
        run([openssl, 'x509', '-req', '-in', str(ROOT / 'tls/server.csr'),
             '-CA', str(intermediate), '-CAkey', str(key),
             '-set_serial', '34', '-days', '2', '-extfile', str(ROOT / 'tls/server.ext'),
             '-out', str(leaf)])
        return leaf.read_bytes(), intermediate.read_bytes()
    finally:
        key.unlink(missing_ok=True)


@requires_mysql
@requires_mutable_tls
@pytest.mark.parametrize('label', ['84', '97'])
def test_actual_peer_chain_order_is_original_material_not_a_completed_trust_path(
    tmp_path: Path, label: str,
) -> None:
    leaf, intermediate = _signed_chain(tmp_path)
    expected = (_der(leaf), _der(intermediate))
    root_ca = (ROOT / 'tls/ca.pem').read_bytes()
    with _reloaded_peer(label, leaf + intermediate, server_ca_extra=intermediate):
        connection = live(label, ssl=SSL(ca_data=root_ca))
        connection.close()
        info = connection.tls_info
        assert info is not None
        certificates = info.peer_certificates
        assert type(certificates) is tuple and len(certificates) == 2
        assert tuple(certificate.to_der() for certificate in certificates) == expected
        assert tuple(certificate.fingerprint_sha256 for certificate in certificates) == tuple(
            hashlib.sha256(value).hexdigest() for value in expected
        )
        assert (certificates[0].is_ca, certificates[1].is_ca) == (False, True)
        assert _der(root_ca) not in expected
        del connection, info
    # The two retained objects remain tied to the old actual peer after reload.
    assert tuple(certificate.to_der() for certificate in certificates) == expected


def _large_san_peer(tmp_path: Path) -> TLSCertificate:
    extensions = tmp_path / 'many-names.ext'
    extensions.write_text('basicConstraints=CA:FALSE\nextendedKeyUsage=serverAuth\n'
                          'keyUsage=digitalSignature,keyEncipherment\n'
                          'subjectAltName=@names\n[names]\n'
                          + ''.join(f'DNS.{index + 1}={"aa" if index % 2 == 0 else "ab"}\n'
                                    for index in range(_DNS_COUNT)))
    leaf = tmp_path / 'many-names.pem'
    run([_openssl(), 'x509', '-req', '-in', str(ROOT / 'tls/server.csr'),
         '-CA', str(ROOT / 'tls/ca.pem'), '-CAkey', str(ROOT / 'tls/ca-key.pem'),
         '-set_serial', '35', '-days', '2', '-extfile', str(extensions), '-out', str(leaf)])
    with _reloaded_peer('84', leaf.read_bytes()):
        connection = live('84', ssl=SSL(mode='REQUIRED'))
        connection.close()
        info = connection.tls_info
        assert info is not None
        certificate = info.peer_certificates[0]
    return certificate


@requires_mysql
@requires_mutable_tls
@pytest.mark.parametrize('kind', ['dns', 'certificate-str'])
def test_real_signal_during_cold_tls_names_preserves_original_exception_and_retry(
    tmp_path: Path, kind: str, record_property: Callable[[str, object], None],
) -> None:
    certificate = _large_san_peer(tmp_path)
    original_der = certificate.to_der()
    original = KeyboardInterrupt('D TLS diagnostic interruption')
    observed: list[int] = []
    samples: list[int] = []
    operation: Callable[[], object] = (
        (lambda: certificate.dns_names) if kind == 'dns' else (lambda: str(certificate))
    )
    baseline = 0

    def interrupt(_number: int, _frame: FrameType | None) -> None:
        allocated = sys.getallocatedblocks() - baseline
        if len(samples) < 32:
            samples.append(allocated)
        # Lower bound excludes arming/entry; upper bound excludes a completed
        # 15000-name tuple delivered before a late timer. Two-byte duplicate
        # names keep the actual certificate under the existing TLS size bound.
        # This is actual Python
        # output progress inside the native conversion, not a timing SLA.
        if 128 <= allocated < _DNS_COUNT // 2:
            observed.append(allocated)
            alarm.cancel()
            raise original

    alarm = NativeAlarm(interrupt)
    gc.collect()
    baseline = sys.getallocatedblocks()
    captured: KeyboardInterrupt | None = None
    try:
        alarm.start(0.00005, 0.00005, wait_ready=True)
        try:
            operation()
        except KeyboardInterrupt as error:
            captured = error
    finally:
        alarm.close()
    assert captured is original, f'no original interruption during conversion; progress samples={samples}'
    assert len(observed) == 1, 'no in-progress diagnostic signal was observed'
    assert not hasattr(original, 'error_info')
    record_property('actual_gil', getattr(sys, '_is_gil_enabled', lambda: True)())
    record_property('allocation_progress_at_signal', observed)
    names = certificate.dns_names
    assert len(names) == _DNS_COUNT
    assert all(name == ('aa' if index % 2 == 0 else 'ab') for index, name in enumerate(names))
    assert certificate.dns_names is names
    rendered = operation()
    assert operation() is rendered
    assert certificate.to_der() is original_der
    assert certificate.fingerprint_sha256 == hashlib.sha256(original_der).hexdigest()
