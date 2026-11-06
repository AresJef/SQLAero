"""EX-073/075/076/079/083 public TLS behavior against selected owned fixtures."""

from concurrent.futures import ThreadPoolExecutor
from collections.abc import Callable
from datetime import datetime, timezone
import base64
import gc
import hashlib
import json
import os
from pathlib import Path
import shutil
from threading import Barrier
from typing import Any, cast
import weakref

import pytest
import sqlaero
from sqlaero.errors import DataError, OperationalError, SQLAeroErrorCode


ROOT = Path(os.environ.get('SQLAERO_TEST_MYSQL_ROOT', '/private/tmp/sqlaero-d-unselected'))
requires_mysql = pytest.mark.skipif('SQLAERO_TEST_MYSQL_ROOT' not in os.environ,
                                   reason='requires explicitly selected owned MySQL fixtures')


def certificate_serial(path: Path) -> int:
    """Read the original certificate's exact serial using the fixture's OpenSSL."""
    from tools.mysql_fixture import run

    openssl = os.environ.get('SQLAERO_OPENSSL') or shutil.which('openssl')
    assert openssl is not None, 'select the existing native OpenSSL fixture tool'
    serial = run([openssl, 'x509', '-in', str(path), '-noout', '-serial']).strip()
    prefix, separator, hexadecimal = serial.partition('=')
    assert prefix == 'serial' and separator == '=' and '\n' not in hexadecimal
    return int(hexadecimal, 16)


def live(label: str, **overrides: Any) -> sqlaero.Connection:
    """Use task-private credentials without exporting them to test diagnostics."""
    state: dict[str, Any] = json.loads((ROOT / label / 'state.json').read_text())
    settings: dict[str, Any] = {
        'user': 'sqlaero', 'password': state['test_password'],
        'database': 'sqlaero_test', 'host': '127.0.0.1',
        'port': int(state['port']), 'operation_timeout': 5,
        'ssl': sqlaero.SSL(ca_file=ROOT / 'tls/ca.pem'),
    }
    settings.update(overrides)
    if settings.get('unix_socket') is not None:
        if 'host' not in overrides:
            settings.pop('host')
        if 'port' not in overrides:
            settings.pop('port')
    return sqlaero.connect(**settings)


def concurrent_read(read: Callable[[], Any]) -> list[Any]:
    """Align eight first accesses so the NoGIL run exercises cold publication."""
    barrier = Barrier(8)

    def first(_: int) -> Any:
        barrier.wait(timeout=10)
        return read()

    with ThreadPoolExecutor(max_workers=8) as workers:
        return list(workers.map(first, range(8)))


@pytest.mark.parametrize(('minimum', 'maximum', 'expected'), [
    (None, None, ('TLSv1.2', None)),
    (' tlsv\t1.2 ', '1.2', ('TLSv1.2', 'TLSv1.2')),
    ('1.3', 'TLS 1.3', ('TLSv1.3', 'TLSv1.3')),
])
def test_disabled_tls_still_saves_normalized_readonly_range(
    minimum: str | None, maximum: str | None, expected: tuple[str, str | None],
) -> None:
    policy = sqlaero.SSL(mode='DISABLED', min_tls_version=minimum, max_tls_version=maximum)
    assert (policy.min_tls_version, policy.max_tls_version) == expected
    with pytest.raises(AttributeError):
        policy.min_tls_version = 'TLSv1.3'  # type: ignore[misc]
    with pytest.raises(AttributeError):
        policy.max_tls_version = None  # type: ignore[misc]
    assert policy.min_tls_version is policy.min_tls_version
    assert policy.max_tls_version is policy.max_tls_version


@pytest.mark.parametrize(('minimum', 'maximum'), [('1.3', '1.2'), ('1.1', None), (None, '1.4')])
def test_disabled_tls_rejects_invalid_ranges(minimum: str | None, maximum: str | None) -> None:
    with pytest.raises(ValueError, match='version'):
        sqlaero.SSL(mode='DISABLED', min_tls_version=minimum, max_tls_version=maximum)


@pytest.mark.parametrize('name', ['ca_data', 'cert_data', 'key_data'])
@pytest.mark.parametrize('value', [bytearray(b'PEM'), memoryview(b'PEM'), 1, False, object()])
def test_memory_material_rejects_unapproved_conversions(name: str, value: object) -> None:
    settings: dict[str, Any] = {name: value}
    if name == 'cert_data':
        settings['key_data'] = b'not reached before type validation'
    if name == 'key_data':
        settings['cert_data'] = b'not reached before type validation'
    with pytest.raises(TypeError, match=name):
        sqlaero.SSL(**settings)


@pytest.mark.parametrize('settings', [
    {'ca_file': 'unopened', 'ca_data': b'bad'},
    {'ca_dir': 'unopened', 'ca_data': b'bad'},
    {'cert_file': 'unopened', 'cert_data': b'bad', 'key_data': b'bad'},
    {'key_file': 'unopened', 'key_data': b'bad', 'cert_data': b'bad'},
])
def test_conflicting_material_sources_fail_before_file_io(settings: dict[str, Any]) -> None:
    with pytest.raises(ValueError, match='ca_|cert_|key_'):
        sqlaero.SSL(**settings)


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
@pytest.mark.parametrize('version', ['TLSv1.2', 'TLSv1.3'])
def test_real_tls_diagnostics_survive_close_and_first_concurrent_access(label: str, version: str) -> None:
    expected_serial = certificate_serial(ROOT / 'tls/server.pem')
    connection = live(label, ssl=sqlaero.SSL(ca_file=ROOT / 'tls/ca.pem',
                                           min_tls_version=version, max_tls_version=version))
    connection.close()
    infos = concurrent_read(lambda: connection.tls_info)
    info = infos[0]
    assert info is not None and all(value is info for value in infos)
    assert type(info) is sqlaero.TLSInfo
    assert info.mode == 'VERIFY_IDENTITY'
    assert info.min_version == version == info.max_version == info.version
    assert info.server_name == '127.0.0.1'
    assert type(info.cipher_id) is int and 0 <= info.cipher_id <= 65535
    assert info.cipher.startswith('TLS_') and 'TLS13_' not in info.cipher
    assert type(info.session_reused) is bool
    assert '\n' in str(info) and '\n' not in repr(info)
    assert 'peer_certificates=<' in str(info)
    assert str(info) is str(info) and repr(info) is repr(info)
    info_fields = ('mode', 'min_version', 'max_version', 'server_name', 'version',
                   'cipher', 'cipher_id', 'session_reused', 'peer_certificates')
    assert tuple(line.strip().split('=', 1)[0] for line in str(info).splitlines()[1:-1]) == info_fields
    for attribute in info_fields:
        with pytest.raises(AttributeError):
            setattr(info, attribute, None)
    assert not hasattr(connection, 'ssl_mode') and not hasattr(connection, 'tls_version')
    chains = concurrent_read(lambda: info.peer_certificates)
    chain = chains[0]
    assert type(chain) is tuple and chain and all(value is chain for value in chains)
    certificate = cast(sqlaero.TLSCertificate, chain[0])
    assert type(certificate) is sqlaero.TLSCertificate
    info_ref = weakref.ref(info)
    del connection, infos, chains, chain, info
    gc.collect()
    assert info_ref() is None
    names = concurrent_read(lambda: certificate.dns_names)
    assert all(value is names[0] for value in names)
    assert names[0] == ('localhost',)
    assert certificate.ip_addresses == ('127.0.0.1',)
    assert 'SQLAero server' in certificate.subject
    assert 'SQLAero isolated test CA' in certificate.issuer
    assert type(certificate.serial_number) is int and certificate.serial_number == expected_serial
    assert certificate.version == 3 and certificate.is_ca is False
    assert certificate.signature_algorithm_oid == '1.2.840.113549.1.1.11'
    assert certificate.public_key_algorithm_oid == '1.2.840.113549.1.1.1'
    assert isinstance(certificate.not_before, datetime)
    assert certificate.not_before.tzinfo is timezone.utc
    assert certificate.not_after.tzinfo is timezone.utc
    der = certificate.to_der()
    pem = certificate.to_pem()
    assert type(der) is bytes and type(pem) is str
    assert base64.b64decode(''.join(pem.splitlines()[1:-1]), validate=True) == der
    original = (ROOT / 'tls/server.pem').read_text().splitlines()
    assert der == base64.b64decode(''.join(original[1:-1]), validate=True)
    assert certificate.fingerprint_sha256 == hashlib.sha256(der).hexdigest()
    assert certificate.to_der() is der and certificate.to_pem() is pem
    lines = pem.splitlines()
    assert all(len(line) == 64 for line in lines[1:-2])
    assert pem.endswith('\n') and '\r' not in pem
    assert '\n' in str(certificate) and '\n' not in repr(certificate)
    certificate_fields = ('subject', 'issuer', 'serial_number', 'version', 'not_before', 'not_after',
                          'dns_names', 'ip_addresses', 'is_ca', 'signature_algorithm_oid',
                          'public_key_algorithm_oid', 'fingerprint_sha256')
    assert tuple(line.strip().split('=', 1)[0] for line in str(certificate).splitlines()[1:-1]) == certificate_fields
    for attribute in certificate_fields:
        with pytest.raises(AttributeError):
            setattr(certificate, attribute, None)


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
def test_actual_plain_connection_has_no_tls_info_after_close(label: str) -> None:
    connection = live(label, ssl=sqlaero.SSL(mode='DISABLED'),
                      server_public_key=(ROOT / label / 'data/public_key.pem').read_bytes())
    connection.close()
    assert connection.tls_info is None


@requires_mysql
@pytest.mark.skipif(os.name == 'nt', reason='Unix socket transport is unavailable')
@pytest.mark.parametrize('mode', ['REQUIRED', 'VERIFY_CA', 'VERIFY_IDENTITY'])
def test_unix_tls_diagnostic_identity_follows_explicit_name_only(mode: str) -> None:
    settings: dict[str, Any] = {'mode': mode}
    if mode != 'REQUIRED':
        settings['ca_file'] = ROOT / 'tls/ca.pem'
    if mode == 'VERIFY_IDENTITY':
        settings['server_name'] = 'localhost'
    connection = live('84', unix_socket=ROOT / '84.sock',
                      ssl=sqlaero.SSL(**settings))
    connection.close()
    info = connection.tls_info
    assert info is not None and info.mode == mode
    assert info.server_name == ('localhost' if mode == 'VERIFY_IDENTITY' else None)
    assert info.min_version == 'TLSv1.2' and info.max_version is None


@requires_mysql
@pytest.mark.skipif(os.name == 'nt', reason='Unix socket transport is unavailable')
def test_default_plain_unix_connection_retains_no_tls_snapshot() -> None:
    connection = live('84', unix_socket=ROOT / '84.sock', ssl=None)
    connection.close()
    assert connection.tls_info is None


def test_tls_diagnostic_classes_have_no_public_material_constructor() -> None:
    with pytest.raises(TypeError):
        sqlaero.TLSInfo()  # type: ignore[call-arg]
    with pytest.raises(TypeError):
        sqlaero.TLSCertificate()  # type: ignore[call-arg]


@pytest.mark.parametrize('value', [False, 1, b'append', object()])
def test_ca_mode_has_a_strict_string_boundary(value: object) -> None:
    with pytest.raises(TypeError, match='ca_mode'):
        sqlaero.SSL(ca_mode=value)  # type: ignore[arg-type]


@pytest.mark.parametrize('value', ['', 'merge', '\u00a0append', 'replace-all'])
def test_ca_mode_rejects_unknown_values(value: str) -> None:
    with pytest.raises(ValueError, match='ca_mode'):
        sqlaero.SSL(ca_mode=value)


@requires_mysql
def test_directory_failure_is_complete_and_old_policy_keeps_its_snapshot(tmp_path: Path) -> None:
    ca = (ROOT / 'tls/ca.pem').read_bytes()
    directory = tmp_path / 'ca.d'
    directory.mkdir()
    with pytest.raises(ValueError, match='ca_dir'):
        sqlaero.SSL(ca_dir=directory)
    (directory / 'active.PEM').write_bytes(ca)
    (directory / '.hidden.crt').write_bytes(ca)
    (directory / 'README').write_bytes(b'ignored')
    (directory / 'nested.cer').mkdir()
    (directory / 'nested.cer/broken.pem').write_bytes(b'not recursively scanned')
    old = sqlaero.SSL(ca_dir=directory)
    (directory / 'broken.cer').write_bytes(b'bad candidate')
    with pytest.raises(ValueError, match='broken.cer'):
        sqlaero.SSL(ca_dir=directory)
    connection = live('84', ssl=old)
    connection.close()
    (directory / 'broken.cer').unlink()
    if os.name != 'nt':
        (directory / 'outside.pem').symlink_to(ROOT / 'tls/ca.pem')
        sqlaero.SSL(ca_dir=directory)
        (directory / 'missing.CRT').symlink_to(directory / 'does-not-exist')
        with pytest.raises(OSError) as caught:
            sqlaero.SSL(ca_dir=directory)
        assert caught.value.errno is not None
        assert 'missing.CRT' in '\n'.join([str(caught.value), *getattr(caught.value, '__notes__', ())])


@requires_mysql
@pytest.mark.parametrize('cert_source', ['file', 'data'])
@pytest.mark.parametrize('key_source', ['file', 'data'])
def test_client_identity_sources_mix_files_and_memory(cert_source: str, key_source: str) -> None:
    settings: dict[str, Any] = {
        'ca_data': (ROOT / 'tls/ca.pem').read_text(),
        'key_password': (ROOT / 'tls/key-password').read_text(),
    }
    settings['cert_' + cert_source] = (ROOT / 'tls/client.pem' if cert_source == 'file'
                                      else (ROOT / 'tls/client.pem').read_bytes())
    settings['key_' + key_source] = (ROOT / 'tls/client-key-encrypted.pem' if key_source == 'file'
                                    else (ROOT / 'tls/client-key-encrypted.pem').read_text())
    connection = live('84', ssl=sqlaero.SSL(**settings))
    connection.close()


@requires_mysql
@pytest.mark.parametrize('password', [None, 'incorrect-test-password'])
def test_encrypted_memory_private_key_preserves_password_failures_without_leaks(password: str | None) -> None:
    actual_password = (ROOT / 'tls/key-password').read_text()
    private_material = (ROOT / 'tls/client-key-encrypted.pem').read_text()
    with pytest.raises(ValueError) as caught:
        sqlaero.SSL(cert_data=(ROOT / 'tls/client.pem').read_bytes(), key_data=private_material,
                    key_password=password)
    diagnostic = '\n'.join([str(caught.value), *getattr(caught.value, '__notes__', ())])
    if actual_password in diagnostic or private_material in diagnostic:
        pytest.fail('private-key failure exposed protected material')
    assert 'password' in diagnostic.lower()


@requires_mysql
def test_real_material_subclasses_use_base_contents_without_conversion_callbacks() -> None:
    class HostileString(str):
        def __str__(self) -> str:
            raise AssertionError('must read native string contents')

        def encode(self, *args: Any, **kwargs: Any) -> bytes:
            raise AssertionError('must not call overridden encode')

    class HostileBytes(bytes):
        def __bytes__(self) -> bytes:
            raise AssertionError('must read native bytes contents')

    tls = sqlaero.SSL(ca_data=HostileString((ROOT / 'tls/ca.pem').read_text()))
    connection = live('84', ssl=tls,
                      server_public_key=HostileBytes((ROOT / '84/data/public_key.pem').read_bytes()))
    connection.close()


@requires_mysql
def test_pathlike_returning_bytes_stays_a_path() -> None:
    class BytePath:
        def __init__(self, value: Path) -> None:
            self.value = value
            self.calls = 0

        def __fspath__(self) -> bytes:
            self.calls += 1
            return os.fsencode(self.value)

    ca = BytePath(ROOT / 'tls/ca.pem')
    rsa = BytePath(ROOT / '84/data/public_key.pem')
    connection = live('84', ssl=sqlaero.SSL(ca_file=ca), server_public_key=rsa)
    connection.close()
    assert ca.calls == rsa.calls == 1


@requires_mysql
@pytest.mark.parametrize('mode', [None, 'replace', ' APPEND\t'])
def test_custom_trust_modes_accept_owned_private_ca(mode: str | None) -> None:
    connection = live('84', ssl=sqlaero.SSL(ca_data=(ROOT / 'tls/ca.pem').read_bytes(), ca_mode=mode))
    connection.close()


@requires_mysql
def test_append_without_custom_material_keeps_real_system_validation() -> None:
    with pytest.raises(OperationalError):
        live('84', ssl=sqlaero.SSL(ca_mode='append'))


@requires_mysql
def test_explicit_bad_rsa_material_is_not_ignored_on_tls_or_fast_authentication() -> None:
    warm = live('84')
    warm.close()
    with pytest.raises(ValueError, match='server_public_key'):
        live('84', server_public_key=b'')
    for material in (b'not SPKI PEM', b'\x30\x00'):
        with pytest.raises(OperationalError, match='server_public_key'):
            live('84', server_public_key=material)
    with pytest.raises(TypeError, match='server_public_key_path'):
        live('84', server_public_key_path=ROOT / '84/data/public_key.pem')


@requires_mysql
@pytest.mark.parametrize('source', ['file', 'data', 'dir'])
@pytest.mark.parametrize('label', ['PUBLIC KEY', 'PRIVATE KEY', 'UNRECOGNIZED TEST MATERIAL'])
def test_all_ca_sources_reject_foreign_blocks_without_exposing_contents(
    tmp_path: Path, source: str, label: str,
) -> None:
    contents = ((ROOT / 'tls/ca.pem').read_bytes()
                + f'-----BEGIN {label}-----\nU0VDUkVUX01BUktFUg==\n-----END {label}-----\n'.encode())
    member = tmp_path / 'active.pem'
    member.write_bytes(contents)
    value: Any = {'file': member, 'data': contents, 'dir': tmp_path}[source]
    with pytest.raises(ValueError) as caught:
        sqlaero.SSL(**{'ca_' + source: value})
    diagnostic = '\n'.join([str(caught.value), *getattr(caught.value, '__notes__', ())])
    assert 'U0VDUkVUX01BUktFUg' not in diagnostic
    if source != 'data':
        assert 'active.pem' in diagnostic


@requires_mysql
@pytest.mark.parametrize('source', ['file', 'data', 'dir'])
def test_all_ca_sources_accept_explanation_text_and_duplicate_valid_cas(
    tmp_path: Path, source: str,
) -> None:
    ca = (ROOT / 'tls/ca.pem').read_bytes()
    contents = b'# CA deployment notes\n' + ca + b'ordinary explanation\n' + ca
    member = tmp_path / 'active.pem'
    member.write_bytes(contents)
    value: Any = {'file': member, 'data': contents, 'dir': tmp_path}[source]
    connection = live('84', ssl=sqlaero.SSL(**{'ca_' + source: value}))
    connection.close()


@pytest.mark.skipif(os.environ.get('SQLAERO_TEST_MUTABLE_TLS_ROOT') != str(ROOT),
                    reason='requires explicit opt-in to reload the selected owned fixture TLS')
def test_bad_san_from_actual_peer_is_local_and_content_exceptions_are_not_cached(tmp_path: Path) -> None:
    """Reload only an explicitly selected owned server, always restoring it."""
    from tools.mysql_fixture import mysql_command, run

    openssl = os.environ.get('SQLAERO_OPENSSL')
    assert openssl is not None, 'select the existing native OpenSSL fixture tool'
    extensions = tmp_path / 'bad-san.ext'
    extensions.write_text('basicConstraints=CA:FALSE\nextendedKeyUsage=serverAuth\n'
                          'keyUsage=digitalSignature,keyEncipherment\n'
                          'subjectAltName=DER:30:0b:82:03:ff:6f:6f:87:04:7f:00:00:01\n')
    replacement = tmp_path / 'bad-san.pem'
    run([openssl, 'x509', '-req', '-in', str(ROOT / 'tls/server.csr'),
         '-CA', str(ROOT / 'tls/ca.pem'), '-CAkey', str(ROOT / 'tls/ca-key.pem'),
         '-set_serial', '0x0102030405060708090a0b0c0d0e0f1011121314', '-days', '7',
         '-extfile', str(extensions), '-out', str(replacement)])
    server = ROOT / 'tls/server.pem'
    original = server.read_bytes()
    try:
        server.write_bytes(replacement.read_bytes())
        run(mysql_command('84'), data='ALTER INSTANCE RELOAD TLS;')
        connection = live('84', ssl=sqlaero.SSL(mode='REQUIRED'))
        connection.close()
    finally:
        server.write_bytes(original)
        run(mysql_command('84'), data='ALTER INSTANCE RELOAD TLS;')
    # First access happens after both close and restoration of the server. The
    # captured original peer bytes must still contain the malformed DNS name.
    info = connection.tls_info
    assert info is not None and info.mode == 'REQUIRED'
    assert 'unavailable' not in str(info)
    certificate = info.peer_certificates[0]
    def failure() -> DataError:
        try:
            _ = certificate.dns_names
        except DataError as error:
            return error
        raise AssertionError('malformed DNS SAN unexpectedly produced a value')

    failures = concurrent_read(failure)
    for error in failures:
        assert type(error) is DataError
        assert error.code == 65555 and error.is_server_error is False
        assert 'peer_certificates[0].dns_names' in str(error)
        assert error.error_info.stage == 'tls_certificate_details'
        assert error.error_info.native_type is not None
    assert len({id(error) for error in failures}) == 8
    assert certificate.ip_addresses == ('127.0.0.1',)
    assert 'SQLAero server' in certificate.subject
    assert certificate.public_key_algorithm_oid == '1.2.840.113549.1.1.1'
    expected_der = base64.b64decode(''.join(replacement.read_text().splitlines()[1:-1]), validate=True)
    assert certificate.to_der() == expected_der
    assert certificate.fingerprint_sha256 == hashlib.sha256(expected_der).hexdigest()
    assert '<unavailable:' in str(certificate) and '\n' not in repr(certificate)
    assert server.read_bytes() == original


@pytest.mark.skipif(os.environ.get('SQLAERO_TEST_MUTABLE_TLS_ROOT') != str(ROOT),
                    reason='requires explicit opt-in to reload the selected owned fixture TLS')
@pytest.mark.parametrize(('server_version', 'minimum', 'maximum'), [
    ('TLSv1.2', 'TLSv1.3', None),
    ('TLSv1.3', 'TLSv1.2', 'TLSv1.2'),
])
def test_actual_tls_range_rejection_preserves_native_cause_and_configured_bounds(
    server_version: str, minimum: str, maximum: str | None,
) -> None:
    """Use the documented runtime TLS context reload on the owned fixture only.

    https://dev.mysql.com/doc/refman/8.4/en/using-encrypted-connections.html
    """
    from tools.mysql_fixture import mysql_command, run

    original = run(mysql_command('84'), data='SELECT @@GLOBAL.tls_version;').strip()
    # Only the known protocol tokens are interpolated into administration SQL.
    assert original and all(value in ('TLSv1.2', 'TLSv1.3') for value in original.split(','))
    try:
        run(mysql_command('84'), data=f"SET GLOBAL tls_version='{server_version}'; ALTER INSTANCE RELOAD TLS;")
        with pytest.raises(OperationalError) as caught:
            live('84', ssl=sqlaero.SSL(ca_file=ROOT / 'tls/ca.pem',
                                      min_tls_version=minimum, max_tls_version=maximum))
    finally:
        run(mysql_command('84'), data=f"SET GLOBAL tls_version='{original}'; ALTER INSTANCE RELOAD TLS;")
    error = caught.value
    assert error.code == SQLAeroErrorCode.TLS and error.is_server_error is False
    info = error.error_info
    assert info.stage == 'connect' and info.native_type is not None and info.cause is not None
    assert info.cause.message
    notes = '\n'.join(getattr(error, '__notes__', ()))
    assert 'range' in notes and minimum in notes
    if maximum is None:
        assert 'None' in notes and 'no extra maximum' in notes
    else:
        assert maximum in notes
    assert run(mysql_command('84'), data='SELECT @@GLOBAL.tls_version;').strip() == original
    restored = live('84')
    restored.close()
