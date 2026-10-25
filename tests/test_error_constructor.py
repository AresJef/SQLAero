"""Check the native exception constructor without claiming full diagnostics."""

# pyright: reportPrivateUsage=false

from enum import IntEnum
from typing import Any

import pytest

from sqlaero._sqlaero import SQLAeroError


class SampleCode(IntEnum):
    """An independent enum accepted by manual exception construction."""
    DUPLICATE = 1062


def test_native_error_message_arguments_and_metadata() -> None:
    assert SQLAeroError().args == ()
    assert SQLAeroError('').args == ('',)
    assert SQLAeroError(message='message').args == ('message',)
    error = SQLAeroError('message', code=SampleCode.DUPLICATE, is_server_error=False)
    assert error.code == 1062 and type(error.code) is int
    assert error.is_server_error is False
    assert str(error) == 'Client Error [1062]: message'
    assert SQLAeroError(code=10 ** 100).code == 10 ** 100
    assert str(SQLAeroError()) == ''


def test_native_error_constructor_values_are_read_only() -> None:
    error = SQLAeroError('original', code=1062, is_server_error=True)
    for name in ['args', 'message', 'code', 'is_server_error']:
        with pytest.raises(AttributeError):
            setattr(error, name, None)
        with pytest.raises(AttributeError):
            delattr(error, name)
    assert error.args == ('original',)
    assert str(error) == 'Server Error [1062]: original'


@pytest.mark.parametrize('arguments', [
    {'message': None}, {'message': 1}, {'code': True}, {'code': '1062'},
    {'is_server_error': 1}, {'is_server_error': 'true'}, {'unknown': 1},
])
def test_native_error_rejects_wrong_types_and_unknown_keywords(arguments: dict[str, Any]) -> None:
    with pytest.raises(TypeError):
        SQLAeroError(**arguments)


def test_native_error_preserves_python_identity_and_chaining() -> None:
    error = SQLAeroError('failure')
    cause = ValueError('root')
    error.add_note('existing note')
    try:
        raise error from cause
    except SQLAeroError as received:
        assert received is error
        assert received.__cause__ is cause
        assert received.__notes__ == ['existing note']
