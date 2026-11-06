"""RSA preparation failures retain their actual source before endpoint opening."""
from __future__ import annotations

import errno
import os
from pathlib import Path
import subprocess
import sys
from tempfile import TemporaryDirectory
from typing import Literal

import pytest
from sqlaero import SSL, connect
from sqlaero.errors import ErrorInfo, OperationalError, SQLAeroErrorCode

# Reuse the existing exact Windows DACL/POSIX mode restoration boundary.
from test_tls_material_boundaries import _deny_ca_read  # pyright: ignore[reportPrivateUsage]


_MALFORMED = b'OWNED_MALFORMED_RSA_PEM_SENTINEL'


def _diagnostic(error: OperationalError) -> str:
    info: ErrorInfo | None = error.error_info
    parts = [str(error), *info.notes]
    while info is not None:
        parts.append(info.message)
        info = info.cause
    return '\n'.join(parts)


def _native_io(error: OperationalError, expected_errno: int | None) -> None:
    assert type(error) is OperationalError
    assert error.code == SQLAeroErrorCode.TRANSPORT
    assert error.is_server_error is False
    info = error.error_info
    assert info.stage == 'connect' and info.native_type is not None
    cause = info.cause
    while cause is not None and cause.native_type != 'std::io::Error':
        cause = cause.cause
    assert cause is not None, 'the original native IO cause was lost'
    assert cause.os_errno == expected_errno
    assert cause.exception_type is None and cause.code is None


def _source(error: OperationalError, member: Path) -> None:
    diagnostic = _diagnostic(error)
    assert 'server_public_key' in diagnostic, diagnostic
    # Native path display may escape Windows separators; require the complete
    # actual path, not merely the basename or an unrelated endpoint path.
    path = str(member)
    assert path in diagnostic or repr(path)[1:-1] in diagnostic, diagnostic
    assert 'TLS material' not in diagnostic, diagnostic
    assert 'TLS handshake' not in diagnostic, diagnostic


def _connect(member: Path | bytes, unopened_socket: Path) -> None:
    # The selected source must fail before any socket/DNS/auth attempt. There
    # is no live server, credential, query or external path in these tests.
    connection = connect(user='unopened', unix_socket=unopened_socket,
                         ssl=SSL(mode='DISABLED'), server_public_key=member)
    connection.close()


def test_missing_rsa_file_reports_parameter_actual_path_and_native_errno(tmp_path: Path) -> None:
    member = tmp_path / 'missing-public-key.pem'
    with pytest.raises(OperationalError) as caught:
        _connect(member, tmp_path / 'unopened.sock')
    _native_io(caught.value, errno.ENOENT)
    _source(caught.value, member)


def test_rsa_directory_failure_is_a_non_tls_source_with_original_native_kind(tmp_path: Path) -> None:
    member = tmp_path / 'public-key-directory'
    member.mkdir()
    with pytest.raises(OperationalError) as caught:
        _connect(member, tmp_path / 'unopened.sock')
    # The shared regular-file gate creates InvalidInput, not a synthetic OS
    # errno; adding source context must not change that underlying fact.
    _native_io(caught.value, None)
    assert 'regular file' in _diagnostic(caught.value)
    _source(caught.value, member)


def _assert_permission_failure(member: Path) -> None:
    original_mode = member.stat().st_mode
    try:
        with _deny_ca_read(member):
            with pytest.raises(OperationalError) as caught:
                _connect(member, member.parent / 'unopened.sock')
            # ErrorInfo preserves std::io::Error's original platform number.
            # Windows access-denied is Win32 5; POSIX uses EACCES.
            _native_io(caught.value, 5 if os.name == 'nt' else errno.EACCES)
            _source(caught.value, member)
    finally:
        assert member.read_bytes() == _MALFORMED
        if os.name != 'nt':
            assert member.stat().st_mode == original_mode


def test_unreadable_rsa_file_preserves_actual_permission_source_and_restores_it(tmp_path: Path) -> None:
    if os.name == 'posix' and os.geteuid() == 0:
        import pwd

        identity = pwd.getpwnam('nobody')
        assert identity.pw_uid != 0
        # As in the existing CA permission test, only a new owned directory
        # and public synthetic input change owner; no fixture/user is modified.
        with TemporaryDirectory(prefix='sqlaero-rsa-permission-', dir='/tmp') as directory:
            parent = Path(directory)
            member = parent / 'unreadable-public-key.pem'
            member.write_bytes(_MALFORMED)
            os.chown(parent, identity.pw_uid, identity.pw_gid)
            os.chown(member, identity.pw_uid, identity.pw_gid)
            code = r'''
import os
from pathlib import Path
import sys
assert os.geteuid() != 0
sys.path.insert(0, sys.argv[2])
from test_rsa_source_diagnostics import _assert_permission_failure
_assert_permission_failure(Path(sys.argv[1]))
print('RSA permission source and restoration verified')
'''
            child = subprocess.run([sys.executable, '-c', code, str(member),
                                    str(Path(__file__).resolve().parent)],
                                   user=identity.pw_uid, group=identity.pw_gid, extra_groups=[],
                                   capture_output=True, text=True, timeout=8, check=False)
            assert child.returncode == 0, child.stdout + child.stderr
            assert child.stdout.strip() == 'RSA permission source and restoration verified'
        return
    member = tmp_path / 'unreadable-public-key.pem'
    member.write_bytes(_MALFORMED)
    _assert_permission_failure(member)


@pytest.mark.parametrize('source', ['file', 'bytes'])
def test_malformed_rsa_pem_keeps_existing_parameter_and_authentication_failure(
    tmp_path: Path, source: Literal['file', 'bytes'],
) -> None:
    member = tmp_path / 'malformed-public-key.pem'
    member.write_bytes(_MALFORMED)
    material = member if source == 'file' else _MALFORMED
    with pytest.raises(OperationalError) as caught:
        _connect(material, tmp_path / 'unopened.sock')
    error = caught.value
    assert type(error) is OperationalError
    assert error.code == SQLAeroErrorCode.AUTHENTICATION
    assert error.is_server_error is False
    info = error.error_info
    assert info.stage == 'connect' and info.native_type is not None
    diagnostic = _diagnostic(error)
    assert 'server_public_key' in diagnostic and 'SubjectPublicKeyInfo PEM' in diagnostic
    assert _MALFORMED.decode('ascii') not in diagnostic
    assert 'TLS handshake' not in diagnostic
