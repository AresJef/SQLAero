"""Validate the actual native JSON path before result-set integration."""

# pyright: reportPrivateUsage=false

import math
from typing import Any

import pytest

from sqlaero import _sqlaero


def decode(data: bytes) -> Any:
    """Call the private native output adapter, without a Python JSON parser."""
    return _sqlaero._decode_json(data)


def test_json_exact_integers_and_signed_float_underflow() -> None:
    values = decode(b'[9007199254740993,9007199254740993.0,-0,-0.0,-1e-400]')
    assert values[:3] == [9007199254740993, 9007199254740992.0, 0]
    assert math.copysign(1.0, values[3]) == -1.0
    assert math.copysign(1.0, values[4]) == -1.0
    text = b'9' * 1000
    assert decode(text) == int(text)


def test_json_final_objects_and_duplicate_key_order() -> None:
    result = decode(b'{"a":1,"b":[true,false,null,"\\u4e2d"],"a":2}')
    assert result == {'a': 2, 'b': [True, False, None, '中']}
    assert list(result) == ['a', 'b']
    assert decode(b'""') == ''


@pytest.mark.parametrize('data', [
    b'1e400', b'-1e400', b'NaN', b'Infinity', b'-Infinity', b'',
    b'{}[]', b'[1,]', b'{"a":}', b'/*x*/1', b'"\\ud800"', b'"\xff"',
])
def test_json_rejects_invalid_or_out_of_range_documents(data: bytes) -> None:
    with pytest.raises(ValueError):
        decode(data)


def test_json_preserves_upstream_resource_bounds() -> None:
    assert decode(b'[' * 200 + b'0' + b']' * 200) is not None
    with pytest.raises(ValueError, match='recursion'):
        decode(b'[' * 201 + b'0' + b']' * 201)
    with pytest.raises(ValueError):
        decode(b'9' * 4301)
