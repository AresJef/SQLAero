"""Secret PEM/password text errors must not retain their malformed input."""

from os import PathLike
from typing import Any, Literal, cast

import pytest
from sqlaero import SSL, connect


SecretParameter = Literal['key_data', 'key_password']
_MARKER = 'SYNTHETIC_TLS_SECRET_MARKER'


class _BaseContents(str):
    def __str__(self) -> str:
        raise AssertionError('secret strings must use their base contents')

    def encode(self, encoding: str = 'utf-8', errors: str = 'strict') -> bytes:
        raise AssertionError('secret strings must not invoke encode overrides')


def _construct(name: SecretParameter, value: object) -> SSL:
    # These tests deliberately exercise malformed/illegal boundary inputs.
    if name == 'key_data':
        return SSL(cert_data=b'placeholder certificate', key_data=cast(Any, value))
    return SSL(key_password=cast(Any, value))


def _assert_no_secret(error: BaseException) -> None:
    for display in (str(error), repr(error), repr(error.args),
                    repr(getattr(error, '__notes__', ())),
                    repr(error.__cause__), repr(error.__context__)):
        assert _MARKER not in display
    assert error.__cause__ is None
    assert error.__context__ is None


@pytest.mark.parametrize('name', ['key_data', 'key_password'])
@pytest.mark.parametrize('subclass', [False, True])
def test_malformed_secret_text_reports_parameter_without_retaining_input(
    name: SecretParameter, subclass: bool,
) -> None:
    raw = _MARKER + '\ud800'
    value = _BaseContents(raw) if subclass else raw
    with pytest.raises(ValueError) as caught:
        _construct(name, value)
    assert type(caught.value) is ValueError
    assert name in str(caught.value)
    assert 'UTF-8' in str(caught.value)
    _assert_no_secret(caught.value)


@pytest.mark.parametrize('name', ['key_data', 'key_password'])
@pytest.mark.parametrize('subclass', [False, True])
def test_valid_secret_text_keeps_base_content_and_later_validation(
    name: SecretParameter, subclass: bool,
) -> None:
    value = _BaseContents(_MARKER) if subclass else _MARKER
    with pytest.raises(ValueError) as caught:
        _construct(name, value)
    assert type(caught.value) is ValueError
    # No real key is needed: certificate parsing / missing-key validation is
    # later than the text boundary being protected by this regression.
    expected = 'cert_data' if name == 'key_data' else 'key_password requires'
    assert expected in str(caught.value)
    assert 'UTF-8' not in str(caught.value)
    _assert_no_secret(caught.value)


@pytest.mark.parametrize('value', [1, False, bytearray(b'key'), memoryview(b'key'), object()])
def test_private_key_data_keeps_its_strict_type_boundary(value: object) -> None:
    with pytest.raises(TypeError, match='key_data') as caught:
        _construct('key_data', value)
    _assert_no_secret(caught.value)


@pytest.mark.parametrize('value', [1, False, b'password', bytearray(b'password'),
                                 memoryview(b'password'), object()])
def test_key_password_keeps_its_strict_string_boundary(value: object) -> None:
    with pytest.raises(TypeError, match='key_password') as caught:
        _construct('key_password', value)
    _assert_no_secret(caught.value)


@pytest.mark.parametrize('value', ['', b''])
def test_private_key_data_keeps_empty_material_rejection(value: str | bytes) -> None:
    with pytest.raises(ValueError, match='key_data.*nonempty'):
        _construct('key_data', value)


@pytest.mark.parametrize('value', ['', '\0'])
def test_key_password_keeps_empty_and_nul_rejection(value: str) -> None:
    with pytest.raises(ValueError, match='key_password.*nonempty.*NUL'):
        _construct('key_password', value)


def test_absent_secret_material_remains_optional() -> None:
    policy = SSL(key_data=None, key_password=None)
    assert policy.min_tls_version == 'TLSv1.2'
    assert policy.max_tls_version is None


@pytest.mark.parametrize('subclass', [False, True])
def test_connection_password_encoding_error_does_not_retain_input(subclass: bool) -> None:
    raw = _MARKER + '\ud800'
    value = _BaseContents(raw) if subclass else raw
    # The malformed secret must fail locally before DNS, sockets or auth.
    with pytest.raises(ValueError) as caught:
        connect(user='synthetic', password=value)
    assert type(caught.value) is ValueError
    assert 'password' in str(caught.value) and 'UTF-8' in str(caught.value)
    _assert_no_secret(caught.value)


@pytest.mark.parametrize('name', ['ca_data', 'cert_data'])
def test_public_material_encoding_errors_are_not_reclassified(name: str) -> None:
    value = 'PUBLIC_SYNTHETIC_MARKER\ud800'
    with pytest.raises(UnicodeEncodeError) as caught:
        if name == 'ca_data':
            SSL(ca_data=value)
        else:
            SSL(cert_data=value, key_data=b'placeholder key')
    assert caught.value.object == value


class _FailurePath(PathLike[str]):
    def __init__(self, error: BaseException) -> None:
        self.error = error

    def __fspath__(self) -> str:
        raise self.error


@pytest.mark.parametrize('error', [MemoryError('synthetic allocation failure'),
                                   KeyboardInterrupt('synthetic interruption'),
                                   UnicodeEncodeError('utf-8', 'PUBLIC\ud800', 6, 7, 'synthetic')])
def test_path_protocol_exceptions_keep_their_original_identity(error: BaseException) -> None:
    with pytest.raises(type(error)) as caught:
        SSL(ca_file=_FailurePath(error), key_password=_MARKER)
    assert caught.value is error
